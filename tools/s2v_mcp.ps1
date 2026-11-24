<#
.SYNOPSIS
  Start/stop the Source 2 Viewer Debug GUI with its MCP automation server (--mcp), in the background.

.DESCRIPTION
  start   (default) Launch the Debug GUI with --mcp=<port> detached, wait until the endpoint answers, print the URL.
  stop    Ask the viewer to quit over MCP (the `quit` tool); kill the process if it does not exit.
  status  Say whether the endpoint answers.
  tools   Print the tool names the server exposes.
  call    Call one tool: s2v_mcp.ps1 call <tool> '{"json":"args"}'  (handy for scripts and smoke tests).

  The viewer is a Debug build of ValveResourceFormat 21.1 (the pinned tag; its GUI has the --mcp switch), built with
  the .NET 11 SDK. -Root is that checkout: -Root, else $env:MAGICIAN_VRF_SRC, else `vrf_src = ...` in Magician's config
  file (`python -I ingame\gamepaths.py config` prints where it is). -DotnetRoot is the SDK: -DotnetRoot, else
  $env:DOTNET_ROOT, else `dotnet_root = ...` in the config, else none (a machine-wide .NET install).
  It is unsandboxed (like the UI itself): it can open any file the user can. It binds loopback only.
  Never launches the game. Register it once with your MCP client, as an HTTP server:
    source2viewer  http://127.0.0.1:13338/mcp
#>
[CmdletBinding()]
param(
  [Parameter(Position = 0)][ValidateSet('start', 'stop', 'status', 'tools', 'call')][string]$Command = 'start',
  [Parameter(Position = 1)][string]$Tool,
  [Parameter(Position = 2)][string]$ArgsJson = '{}',
  [int]$Port = 13338,
  [int]$TimeoutSec = 300,
  [string]$Root = '',
  [string]$DotnetRoot = ''
)
$ErrorActionPreference = 'Stop'
$Ingame = Join-Path (Split-Path -Parent $PSScriptRoot) 'ingame'
function Get-ConfigValue([string]$Key) {
  $v = python -I -c "import sys; sys.path.insert(0, sys.argv[1]); import gamepaths; print(gamepaths.config(sys.argv[2]) or '')" $Ingame $Key
  if ($LASTEXITCODE -eq 0 -and $v) { return $v.Trim() } else { return '' }
}
if (-not $Root) { $Root = $env:MAGICIAN_VRF_SRC }
if (-not $Root) { $Root = Get-ConfigValue 'vrf_src' }
if (-not $Root -and $Command -eq 'start') {
  throw 'no Source 2 Viewer checkout: pass -Root <the ValveResourceFormat 21.1 checkout>, set MAGICIAN_VRF_SRC, or add vrf_src = <path> to the config file'
}
if (-not $DotnetRoot) { $DotnetRoot = $env:DOTNET_ROOT }
if (-not $DotnetRoot) { $DotnetRoot = Get-ConfigValue 'dotnet_root' }
if (-not $Root) { $Root = $env:TEMP }   # stop / status / tools / call need only the port; the pid file falls back here
$exe = Join-Path $Root 'GUI\bin\Debug\win-x64\Source2Viewer.exe'
$url = "http://127.0.0.1:$Port/mcp"
$pidFile = Join-Path $Root ".s2v_mcp_$Port.pid"

function Invoke-Mcp([string]$Method, $Params = @{}) {
  $body = @{ jsonrpc = '2.0'; id = 1; method = $Method; params = $Params } | ConvertTo-Json -Depth 20 -Compress
  Invoke-RestMethod -Uri $url -Method Post -ContentType 'application/json' -Body $body -TimeoutSec 120
}
function Test-Endpoint {
  try { $null = Invoke-Mcp 'tools/list'; $true } catch { $false }
}
function Get-ViewerProc {
  if (Test-Path $pidFile) { Get-Process -Id ([int](Get-Content $pidFile)) -ErrorAction SilentlyContinue | Where-Object ProcessName -eq 'Source2Viewer' }
}

switch ($Command) {
  'start' {
    if (Test-Endpoint) { Write-Host "already running: $url"; return }
    if (-not (Test-Path $exe)) { throw "Debug GUI not built: $exe. Build it with the .NET 11 SDK: dotnet build '$Root\GUI\GUI.csproj' -c Debug" }
    # a .NET runtime outside the machine-wide install is found by the apphost through DOTNET_ROOT
    if ($DotnetRoot) { $env:DOTNET_ROOT = $DotnetRoot }
    $p = Start-Process -FilePath $exe -ArgumentList "--mcp=$Port" -WorkingDirectory (Split-Path $exe) -PassThru
    Set-Content -Path $pidFile -Value $p.Id
    $deadline = (Get-Date).AddSeconds($TimeoutSec)
    while ((Get-Date) -lt $deadline) {
      if ($p.HasExited) { throw "viewer exited early (code $($p.ExitCode))" }
      if (Test-Endpoint) { Write-Host $url; return }
      Start-Sleep -Milliseconds 500
    }
    throw "endpoint did not answer within ${TimeoutSec}s: $url (pid $($p.Id) left running; use 'stop')"
  }
  'stop' {
    if (Test-Endpoint) { try { $null = Invoke-Mcp 'tools/call' @{ name = 'quit'; arguments = @{} } } catch {} }
    $p = Get-ViewerProc
    if ($p) { if (-not $p.WaitForExit(15000)) { Stop-Process -Id $p.Id -Force; Write-Host "killed pid $($p.Id)" } }
    Remove-Item $pidFile -ErrorAction SilentlyContinue
    Write-Host 'stopped'
  }
  'status' {
    if (Test-Endpoint) { Write-Host "up: $url" } else { Write-Host "down: $url"; exit 1 }
  }
  'tools' {
    (Invoke-Mcp 'tools/list').result.tools | ForEach-Object { $_.name }
  }
  'call' {
    if (-not $Tool) { throw 'usage: s2v_mcp.ps1 call <tool> [argsJson]' }
    # Raw JSON keeps this working on Windows PowerShell 5.1 (no ConvertFrom-Json -AsHashtable there).
    $body = '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":' + ($Tool | ConvertTo-Json -Compress) + ',"arguments":' + $ArgsJson + '}}'
    Invoke-RestMethod -Uri $url -Method Post -ContentType 'application/json' -Body $body -TimeoutSec 300 | ConvertTo-Json -Depth 20
  }
}

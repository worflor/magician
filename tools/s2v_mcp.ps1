# Start/stop the Source 2 Viewer Debug GUI with its MCP automation server, and talk to it: the PowerShell spelling of
# s2v_mcp.py, which does the work (and has --help). Kept for callers that already use it.
#
#   s2v_mcp.ps1 [start|stop|status|tools|call] [<tool>] [<argsJson>] [-Port 13338] [-TimeoutSec 300] [-Root DIR] [-DotnetRoot DIR]
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
$py = @("-I", (Join-Path $PSScriptRoot "s2v_mcp.py"), $Command, "--port", "$Port", "--timeout", "$TimeoutSec")
# the JSON goes through the environment: Windows PowerShell 5.1 strips the quotes out of an argument to a program
if ($Command -eq 'call') { $py += @($Tool); $env:S2V_MCP_ARGS = $ArgsJson }
if ($Root) { $py += @("--root", $Root) }
if ($DotnetRoot) { $py += @("--dotnet-root", $DotnetRoot) }
& python @py
exit $LASTEXITCODE

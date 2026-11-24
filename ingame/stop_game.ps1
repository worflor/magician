# Close Deadlock and wait until it has exited and released its files, so the next launch or install never meets a
# closing game (Windows holds the addon paks for several seconds after the process ends).
#
# One user of the game at a time (gamelock.py): only the holder stops the game, and only the game bound to its seat
# (the deadlock.exe launch.ps1 started for it). Anyone else is refused (exit 3, with the reason): never a kill of a
# game this caller did not launch. The seat is held through the whole stop and freed only once the process is gone
# (2026-11-24: a stop that freed the seat while deadlock.exe was still exiting let the next holder launch, and the
# first pipeline's fallback then killed that game). -KeepLock leaves the seat held after the stop, for a caller that
# still has work to do on the closed game's files (an uninstall); it releases the seat itself.
#
# -Force is for a person at the keyboard who has looked at the game: it stops every deadlock.exe whoever holds the
# seat. No script passes it.
param([string]$Holder = "", [switch]$Force, [string]$ConsoleKey = "", [switch]$NoQuit, [switch]$KeepLock)
$Here = Split-Path -Parent $MyInvocation.MyCommand.Path
if (-not $Holder) { $Holder = $env:MAGICIAN_GAME_HOLDER }
if (-not $Holder) { $Holder = "anonymous" }
$env:MAGICIAN_GAME_HOLDER = $Holder
function Lock-Field($name) {
    $j = python -I -c "import json,sys,os;sys.path.insert(0,r'$Here');import gamelock as g;l=g.read();print(json.dumps(l.get('$name') if l else None))"
    return ($j | ConvertFrom-Json)
}
$p = Get-Process deadlock -ErrorAction SilentlyContinue
if (-not $p) {
    # nothing to stop: free the seat only if it is ours (never another holder's, never forced)
    if (-not $KeepLock) { python -I "$Here\gamelock.py" release --holder $Holder | Out-Null }
    exit 0
}
if ($Force) {
    $targets = @($p)
} else {
    python -I "$Here\gamelock.py" may-drive --holder $Holder
    if ($LASTEXITCODE -ne 0) { Write-Host "not stopping the game (you are '$Holder')"; exit 3 }
    $gamePid = Lock-Field "game_pid"
    $targets = @($p | Where-Object Id -eq $gamePid)
    # hold the seat for the whole stop, however long the game takes to exit; renewed below while it lingers
    python -I "$Here\gamelock.py" acquire --holder $Holder --what "stopping" --hold 600 | Out-Null
    if ($LASTEXITCODE -ne 0) { Write-Host "not stopping the game: the seat is no longer '$Holder''s"; exit 3 }
}
# First ask the game to quit through its own console (a clean exit; Stop-Process can be refused to a sandboxed or
# non-elevated caller, which used to leave "deadlock.exe is still running"). Then the hard stop as a fallback, on the
# bound process only.
if (-not $NoQuit) {
    # --no-probe: a closing game may not answer the probe; console.py refuses by itself if the game is not ours
    $keyArg = @(); if ($ConsoleKey) { $keyArg = @("--key", $ConsoleKey) }   # else console.py reads the player's own bind
    python -I "$Here\console.py" @keyArg --holder $Holder --no-probe quit 2>&1 | Out-Null
    $targets | Wait-Process -Timeout 45 -ErrorAction SilentlyContinue
}
for ($i = 0; $i -lt 3 -and ($targets | Where-Object { -not $_.HasExited }); $i++) {   # it can outlive one kill
    $targets | Where-Object { -not $_.HasExited } | Stop-Process -Force -ErrorAction SilentlyContinue
    $targets | Wait-Process -Timeout 15 -ErrorAction SilentlyContinue
    if (-not $Force) { python -I "$Here\gamelock.py" acquire --holder $Holder --what "stopping" --hold 600 | Out-Null }
}
if ($targets | Where-Object { -not $_.HasExited }) { Write-Host "deadlock.exe is still running: the seat stays held by '$Holder'"; exit 1 }
$addons = Join-Path (python -I "$Here\gamepaths.py" citadel) "addons"
$deadline = (Get-Date).AddSeconds(30)
foreach ($f in Get-ChildItem $addons -Filter "pak*_dir.vpk" -ErrorAction SilentlyContinue) {
    while ((Get-Date) -lt $deadline) {
        try { $s = [IO.File]::Open($f.FullName, 'Open', 'ReadWrite', 'None'); $s.Close(); break } catch { Start-Sleep -Milliseconds 500 }
    }
}
if (-not $KeepLock) {
    if ($Force) { python -I "$Here\gamelock.py" release --force | Out-Null }
    else {
        python -I "$Here\gamelock.py" release --holder $Holder
        if ($LASTEXITCODE -ne 0) { Write-Host "closed the game, but the seat was not released"; exit 1 }
    }
}
Write-Host "closed the game"

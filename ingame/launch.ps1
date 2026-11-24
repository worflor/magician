# Launch Deadlock for in-game testing and wait until the main menu is live.
#
#   powershell -ExecutionPolicy Bypass -File toolkit\ingame\launch.ps1            # windowed borderless 1920x1080
#   ... -Width 3840 -Height 2160      # a window larger than the monitor (the cinema kit's supersampling; UNVERIFIED)
#   ... -Extra "+map hero_testing"
#
# -condebug writes game\citadel\console.log, which console.py reads back. No -dev: on build 6864 it crashes
# client.dll (null read) when hero_testing loads.
#
# One user of the game at a time (gamelock.py): the launch takes the game lock for -Holder (default $env:MAGICIAN_GAME_HOLDER,
# else "anonymous") and refuses, exit 3, while another holder has it; -Wait waits (up to -WaitSeconds) instead. The game it
# starts is bound to the seat (its pid and start time), so only this holder types into or stops it. The lock is cleared by
# stop_game.ps1 once the game has exited, and is stale when no deadlock.exe is running.
param([string]$Extra = "", [int]$Timeout = 420, [string]$Holder = "", [switch]$Wait, [int]$WaitSeconds = 1800, [switch]$Adopt,
      [int]$Width = 1920, [int]$Height = 1080)
$Here = Split-Path -Parent $MyInvocation.MyCommand.Path
if (-not $Holder) { $Holder = $env:MAGICIAN_GAME_HOLDER }
if (-not $Holder) { $Holder = "anonymous" }
$waitFor = 0; if ($Wait) { $waitFor = $WaitSeconds }
python -I "$Here\gamelock.py" acquire --holder $Holder --what "launch" --wait $waitFor --launching
if ($LASTEXITCODE -ne 0) { exit 3 }
$Steam = (Get-ItemProperty HKCU:\Software\Valve\Steam -ErrorAction SilentlyContinue).SteamExe
if (-not $Steam) { $Steam = "${env:ProgramFiles(x86)}\Steam\steam.exe" }
$Log = python -I "$Here\gamepaths.py" console_log   # the game as magician.exe finds it (DEADLOCK_GAME, the config file, Steam)
if ($LASTEXITCODE -ne 0) { python -I "$Here\gamelock.py" release --holder $Holder; exit 2 }
# A game already running was started by someone else's launch, with whatever paks were installed then (a dev pak, another
# mod's build). Testing in it is invalid, so refuse unless the caller knowingly adopts it (-Adopt), e.g. its own instance.
if (Get-Process deadlock -ErrorAction SilentlyContinue) {
    if ($Adopt) {
        # knowingly adopting your own instance binds it to your seat; refused if the seat or the binding fails
        python -I "$Here\gamelock.py" acquire --holder $Holder --what "launch (adopted)"
        if ($LASTEXITCODE -ne 0) { exit 3 }
        python -I "$Here\gamelock.py" bind --holder $Holder
        if ($LASTEXITCODE -ne 0) { python -I "$Here\gamelock.py" release --holder $Holder --leave-running | Out-Null; exit 4 }
        Write-Host "deadlock already running: adopted"; exit 0
    }
    python -I "$Here\gamelock.py" release --holder $Holder
    Write-Host "REFUSED: a Deadlock instance this launch didn't start is running (it may carry other paks). Run stop_game.ps1, check 'magician install --status', then launch again; -Adopt only for your own instance."
    exit 4
}
$args = @("-applaunch", "1422450", "-novid", "-console", "-condebug", "-windowed", "-noborder", "-w", "$Width", "-h", "$Height") + ($Extra -split " " | Where-Object { $_ })
$t0 = Get-Date
Start-Process $Steam -ArgumentList $args
$t = 0
while ($t -lt $Timeout) {
    Start-Sleep 3; $t += 3
    # only this launch counts: the previous session's console.log can still hold a matching line. The tail is long because a
    # heroless pawn in the sandbox can print 64 'OnPostPredictionError ... teleporting' lines a second (2026-11-21): 60 lines
    # were gone before the next 3 s poll and the launch timed out on a loaded game
    if ((Test-Path $Log) -and (Get-Item $Log).LastWriteTime -gt $t0 -and (Get-Content $Log -Tail 6000 | Select-String "ChangeGameState|PRE-LOADING|Hideout|Created physics")) {
        $p = Get-Process deadlock -ErrorAction SilentlyContinue | Where-Object { $_.MainWindowHandle -ne 0 -and $_.StartTime -ge $t0.AddSeconds(-2) } | Select-Object -First 1
        if ($p) {
            # bind the game this launch started to the seat: from here on only this holder drives it, and only it
            python -I "$Here\gamelock.py" bind --holder $Holder --pid $p.Id
            if ($LASTEXITCODE -ne 0) { Write-Host "LAUNCHED, but the seat is no longer held by $Holder (not driving it)"; exit 3 }
            python -I "$Here\gamelock.py" acquire --holder $Holder --what "launched" | Out-Null
            if ($LASTEXITCODE -ne 0) { exit 3 }
            Write-Host "LAUNCHED after ${t}s (pid $($p.Id))"; exit 0
        }
    }
}
# A game this launch started but that never reached the menu is still ours: bind it and keep the seat, so its stop
# (stop_game.ps1) is allowed and nobody launches beside it.
$p = Get-Process deadlock -ErrorAction SilentlyContinue | Where-Object { $_.StartTime -ge $t0.AddSeconds(-2) } | Select-Object -First 1
if ($p) {
    python -I "$Here\gamelock.py" bind --holder $Holder --pid $p.Id | Out-Null
    Write-Host "TIMEOUT waiting for the game (pid $($p.Id) started: stop it with stop_game.ps1 -Holder $Holder)"; exit 1
}
python -I "$Here\gamelock.py" release --holder $Holder | Out-Null
Write-Host "TIMEOUT waiting for the game"; exit 1

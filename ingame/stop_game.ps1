# Close Deadlock and wait until it has exited and let go of its files: the PowerShell spelling of stop_game.py, which
# does the work (and has the rules, the exit codes and --help). Kept for callers that already use it.
#
#   powershell -ExecutionPolicy Bypass -File ingame\stop_game.ps1 [-Holder NAME] [-NoQuit] [-ConsoleKey KEY] [-KeepLock] [-Force]
#
# -Force is for a person at the keyboard who has looked at the game. No script passes it.
param([string]$Holder = "", [switch]$Force, [string]$ConsoleKey = "", [switch]$NoQuit, [switch]$KeepLock)
$Here = Split-Path -Parent $MyInvocation.MyCommand.Path
$py = @("-I", "$Here\stop_game.py")
if ($Holder) { $py += @("--holder", $Holder) }
if ($ConsoleKey) { $py += @("--console-key", $ConsoleKey) }
if ($NoQuit) { $py += "--no-quit" }
if ($KeepLock) { $py += "--keep-lock" }
if ($Force) { $py += "--force" }
& python @py
exit $LASTEXITCODE

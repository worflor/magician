# Launch Deadlock for in-game testing: the PowerShell spelling of launch.py, which does the work (and has the rules,
# the exit codes and --help). Kept for callers that already use it.
#
#   powershell -ExecutionPolicy Bypass -File ingame\launch.ps1 [-Extra "+map new_player_basics"] [-Holder NAME] [-Wait]
#       [-WaitSeconds 1800] [-Timeout 420] [-Width 1920 -Height 1080] [-Adopt]
param([string]$Extra = "", [int]$Timeout = 420, [string]$Holder = "", [switch]$Wait, [int]$WaitSeconds = 1800, [switch]$Adopt,
      [int]$Width = 1920, [int]$Height = 1080)
$Here = Split-Path -Parent $MyInvocation.MyCommand.Path
$py = @("-I", "$Here\launch.py", "--timeout", "$Timeout", "--size", "${Width}x${Height}")
# Windows PowerShell drops an empty argument on its way to a program, so the optional ones go in only when given
if ($Extra) { $py += @("--extra", $Extra) }
if ($Holder) { $py += @("--holder", $Holder) }
if ($Wait) { $py += @("--wait", "$WaitSeconds") }
if ($Adopt) { $py += "--adopt" }
& python @py
exit $LASTEXITCODE

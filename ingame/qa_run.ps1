# One in-game QA pass for a mod: mod_session.py qa (take the seat, launch into the sandbox as the mod's hero, every scenario
# into a dated folder, contact sheets, crash dumps, the warnings the mod adds over vanilla, the seat handed back).
#
#   powershell -ExecutionPolicy Bypass -File ingame\qa_run.ps1 -Mod <mod folder or mod.toml> [-Scenarios a,b,c] [-Wait]
#
# The mod's hero comes from its mod.toml, the default pass from its [ingame] pass list (or give -Scenarios); scenario
# names are looked up in the mod's own folders first, then in ingame\scenarios (mod_session.py where <name> shows which).
# Output: <reference>\work\<slug>\ingame\<stamp>\ (index.md). Exit codes are mod_session.py's (3: the seat is held).
param([string]$Mod = ".", [string]$Scenarios = "", [switch]$Wait, [switch]$Launch)   # -Launch: kept for old callers; qa always launches
$Here = Split-Path -Parent $MyInvocation.MyCommand.Path
$Work = python -I "$Here\gamepaths.py" work
if ($LASTEXITCODE -ne 0) { exit 2 }
$toml = $Mod; if (Test-Path -PathType Container $Mod) { $toml = Join-Path $Mod "mod.toml" }
if (-not (Test-Path $toml)) { Write-Host "no mod.toml at $Mod"; exit 2 }
$slug = (Split-Path -Leaf (Split-Path -Parent (Resolve-Path $toml)))
$Out = Join-Path $Work "$slug\ingame\$(Get-Date -Format 'yyyyMMdd-HHmm')"
$waitArg = @("--wait", "0"); if ($Wait) { $waitArg = @("--wait", "1800") }
$items = @(); if ($Scenarios) { $items = $Scenarios -split "," | Where-Object { $_ } }
python -I "$Here\mod_session.py" qa --mod (Resolve-Path $toml).Path --out $Out @waitArg @items
exit $LASTEXITCODE

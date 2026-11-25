# Rip Deadlock hero models to Blender-ready .glb for reference work: the PowerShell spelling of rip_heroes.py, which
# does the work (and has --help). Kept for callers that already use it.
#
#   .\rip-heroes.ps1                      # every hero with a real model
#   .\rip-heroes.ps1 -Hero lash,sinclair  # by display name or internal key
#   .\rip-heroes.ps1 -Hero lash -Animations [-Force] [-Root DIR] [-Game DIR] [-Cli PATH]
#
# Valve assets: reference only. Never commit or redistribute these files.
param([string[]]$Hero, [switch]$Animations, [switch]$Force, [string]$Root = "", [string]$Game = "", [string]$Cli = "")
$py = @("-I", (Join-Path $PSScriptRoot "rip_heroes.py"))
if ($Hero) { $py += @("--hero", ($Hero -join ",")) }
if ($Animations) { $py += "--animations" }
if ($Force) { $py += "--force" }
if ($Root) { $py += @("--root", $Root) }
if ($Game) { $py += @("--game", $Game) }
if ($Cli) { $py += @("--cli", $Cli) }
& python @py
exit $LASTEXITCODE

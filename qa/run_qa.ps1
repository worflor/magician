# Magician visual QA: one entry point. Everything is in run_qa.py; this passes PowerShell-style arguments through.
#
#   powershell -ExecutionPolicy Bypass -File qa\run_qa.ps1 -Hero haze|<profile.json> -Vpk <mod pak> [-ClipVpk <clips pak>,...]
#       [-Steps all|stage,export,cache,index,geo,motion,holes,sequences,views,profile,cloth,report] [-Clips curated|all|a,b]
#       [-Frames 10] [-SeqFrames 8] [-Out <report dir>]
#
# Writes a dated report folder under <reference>\work\<mod>\qa\ (index.md, findings.json,
# sheets\), caches exports under ...\qa\cache\ by content hash. Never touches the game install.
# A release pak that carries its own clips needs no -ClipVpk. -Who ours -ValveFrom <earlier report> re-runs only our
# side and copies Valve's (it depends on the game build only).
param(
    [Parameter(Mandatory = $true)][string]$Hero,
    [Parameter(Mandatory = $true)][string]$Vpk,
    [string[]]$ClipVpk = @(),
    [string]$Steps = "all",
    [string]$Clips = "curated",
    [int]$Frames = 10,
    [int]$SeqFrames = 8,
    [string]$Out = "",
    [string]$Who = "both",
    [string]$ValveFrom = "",
    [switch]$Full       # full-quality pictures and ray grids; the default is draft (DEADLOCK_DRAFT unset or 1; 0 = full)
)
$ErrorActionPreference = "Stop"
$py = @("-I", (Join-Path $PSScriptRoot "run_qa.py"), "--hero", $Hero, "--vpk", (Resolve-Path $Vpk).Path,
        "--steps", $Steps, "--clips", $Clips, "--frames", $Frames, "--seq-frames", $SeqFrames)
foreach ($c in $ClipVpk) { $py += @("--clip-vpk", (Resolve-Path $c).Path) }
if ($Out) { $py += @("--out", $Out) }
$py += @("--who", $Who)
if ($Full) { $py += "--full" }
if ($ValveFrom) { $py += @("--valve-from", $ValveFrom) }
& python @py
exit $LASTEXITCODE

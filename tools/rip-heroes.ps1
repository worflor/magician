<#
.SYNOPSIS
  Rip Deadlock hero models to Blender-ready .glb (materials + textures embedded) for reference work.

.DESCRIPTION
  Reads the LIVE game data, never a hardcoded list: heroes.vdata gives each hero's model path,
  citadel_gc_hero_names_english.txt gives the display name. Output is keyed by game build
  (steam.inf ClientVersion), so references from different patches never mix.

    <Root>\<build>\heroes\<display-slug>\<model>.glb      model + materials + textures (+ anims with -Animations)
    <Root>\<build>\heroes\INDEX.md                        hero -> internal key -> VPK path -> glb

  Valve assets: reference only. Never commit or redistribute these files.

.EXAMPLE
  .\rip-heroes.ps1                      # every hero with a real model
  .\rip-heroes.ps1 -Hero lash,sinclair  # by display name or internal key
  .\rip-heroes.ps1 -Hero lash -Animations
#>
param(
    [string[]]$Hero,
    [switch]$Animations,
    [switch]$Force,
    [string]$Root = "",   # default: the reference folder (MAGICIAN_REFERENCE, the config file; see ingame\gamepaths.py)
    [string]$Game = "",   # default: the game's citadel folder, found as magician.exe finds it
    [string]$Cli  = ""    # default: the Source 2 Viewer CLI (MAGICIAN_VRF, the config's vrf, else Source2Viewer-CLI on PATH)
)
$paths = Join-Path (Split-Path -Parent $MyInvocation.MyCommand.Path) "..\ingame\gamepaths.py"
if (-not $Root) { $Root = python -I $paths reference }
if (-not $Game) { $Game = python -I $paths citadel; if ($LASTEXITCODE -ne 0) { exit 2 } }
if (-not $Cli)  { $Cli = python -I $paths vrf }
if (-not (Test-Path $Cli)) { $found = Get-Command $Cli -ErrorAction SilentlyContinue; if ($found) { $Cli = $found.Source } }
$ErrorActionPreference = "Stop"
$vpk = Join-Path $Game "pak01_dir.vpk"
foreach ($p in $vpk, $Cli) { if (-not (Test-Path $p)) { throw "missing: $p" } }

$build = (Select-String -Path (Join-Path $Game "steam.inf") -Pattern '^ClientVersion=(\d+)').Matches[0].Groups[1].Value
$out = Join-Path $Root "$build\heroes"
$cache = Join-Path $Root "$build\decompiled"
New-Item -ItemType Directory -Force $out, $cache | Out-Null

# 1. hero -> model path, from the game's own vdata.
$hv = Join-Path $cache "scripts\heroes.vdata"
if (-not (Test-Path $hv)) { & $Cli -i $vpk -f "scripts/heroes.vdata_c" -d -o $cache --quiet | Out-Null }
$vdata = Get-Content $hv -Raw
$models = @{}
foreach ($m in [regex]::Matches($vdata, '(?ms)^\t(hero_[a-z0-9_]+)\s*=\s*\r?\n\t\{.*?m_strModelName\s*=\s*resource_name:"([^"]+)"')) {
    $models[$m.Groups[1].Value] = $m.Groups[2].Value
}

# 2. hero -> display name, from localization.
$names = @{}
$loc = Join-Path $Game "resource\localization\citadel_gc_hero_names\citadel_gc_hero_names_english.txt"
foreach ($m in (Select-String -Path $loc -Pattern '^\s*"(hero_[a-z0-9_]+):n"\s+"([^"]+)"').Matches) {
    $names[$m.Groups[1].Value] = $m.Groups[2].Value
}

# 3. Group hero keys by model (several keys share placeholder models like gen_man).
$byModel = $models.GetEnumerator() | Group-Object Value
$placeholder = "models/heroes_wip/gen_man/gen_man.vmdl"

function Get-Slug([string]$s) { ($s.ToLower() -replace '[^a-z0-9]+', '-').Trim('-') }

$rows = @()
foreach ($g in $byModel | Sort-Object Name) {
    $keys = $g.Group.Name | Sort-Object
    $named = $keys | Where-Object { $names[$_] }
    # When several hero keys share one model (hero_lash + unreleased hero_skyrunner), the owner is the key
    # whose name matches the model file (hero_lash -> lash.vmdl, hero_chrono -> chrono.vmdl). The others show in INDEX.
    $base = [IO.Path]::GetFileNameWithoutExtension($g.Name)
    $owner = $keys | Where-Object { ($_ -replace '^hero_', '') -eq $base } | Select-Object -First 1
    $display = if ($g.Name -eq $placeholder) { "Placeholder Body (gen_man)" }
               elseif ($owner -and $names[$owner]) { $names[$owner] }
               elseif ($named) { ($named | ForEach-Object { $names[$_] } | Select-Object -Unique) -join " + " }
               else { ($keys -join " + ") }
    $slug = if ($g.Name -eq $placeholder) { "_placeholder-gen-man" } else { Get-Slug $display }

    if ($Hero) {
        $want = $Hero | ForEach-Object { $_.ToLower() }
        $hit = $keys | Where-Object { $want -contains $_ -or $want -contains ($_ -replace '^hero_', '') -or ($names[$_] -and $want -contains $names[$_].ToLower()) }
        if (-not $hit) { continue }
    }

    $dest = Join-Path $out $slug
    $glbName = [IO.Path]::GetFileNameWithoutExtension($g.Name) + ".glb"
    $glb = Join-Path $dest $glbName
    if ((Test-Path $glb) -and -not $Force) {
        Write-Output "skip  $display (exists)"
    } else {
        New-Item -ItemType Directory -Force $dest | Out-Null
        $tmp = Join-Path $dest "_export"
        $args = @("-i", $vpk, "-f", ($g.Name + "_c"), "-o", $tmp, "--gltf_export_format", "glb",
                  "--gltf_export_materials", "--gltf_textures_adapt", "--quiet")
        if ($Animations) { $args += "--gltf_export_animations" }
        & $Cli @args | Out-Null
        if ($LASTEXITCODE -ne 0) { Write-Output "FAIL  $display ($($g.Name)) exit $LASTEXITCODE"; continue }
        # Unreleased heroes can name a model in heroes.vdata that this build's VPK doesn't ship.
        # The CLI exits 0 with "no files matched", so check for output, not the exit code.
        if (-not (Test-Path $tmp) -or -not (Get-ChildItem $tmp -Recurse -File)) {
            Remove-Item $dest -Recurse -Force -ErrorAction SilentlyContinue
            Write-Output ("absent {0,-27} {1} (named in heroes.vdata, not in this build's VPK)" -f $display, $g.Name)
            $rows += [pscustomobject]@{ Display = $display; Keys = ($keys -join ", "); Vpk = $g.Name; Glb = "(not shipped)"; Size = "-" }
            continue
        }
        Get-ChildItem $tmp -Recurse -File | ForEach-Object { Move-Item $_.FullName (Join-Path $dest $_.Name) -Force }
        Remove-Item $tmp -Recurse -Force
        Write-Output ("rip   {0,-28} {1}" -f $display, $g.Name)
    }
    $size = if (Test-Path $glb) { "{0:N0} MB" -f ((Get-Item $glb).Length / 1MB) } else { "?" }
    $rows += [pscustomobject]@{ Display = $display; Keys = ($keys -join ", "); Vpk = $g.Name; Glb = "$slug\$glbName"; Size = $size }
}

# 4. Index (rebuilt from what is on disk + this run).
if (-not $Hero) {
    $md = @("# Deadlock hero reference models, build $build", "",
            "Ripped from ``pak01_dir.vpk`` with Source 2 Viewer. Reference only: never commit or redistribute.", "",
            "| Hero | Internal keys | VPK path | glb | Size |", "|---|---|---|---|---|")
    $md += $rows | Sort-Object Display | ForEach-Object { "| $($_.Display) | $($_.Keys) | ``$($_.Vpk)`` | ``$($_.Glb)`` | $($_.Size) |" }
    Set-Content (Join-Path $out "INDEX.md") $md -Encoding utf8
    Write-Output "index $(Join-Path $out 'INDEX.md')"
}

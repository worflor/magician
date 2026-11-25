<#
.SYNOPSIS
  Build the release asset for this platform and write the files `magician update` reads into a folder.

.DESCRIPTION
  A release is a few files at one location: one asset per platform (magician-windows-x64.zip,
  magician-linux-x64.tar.gz), SHA256SUMS and VERSION. This script builds the release executable, writes
  VERSION from what the built program says about itself (`magician update --describe`), packs the toolkit folder
  from the files git tracks (the executable, ingame\, tools\, mcp\, skills\, docs\, README.md, CHANGELOG.md,
  LICENSE; no target\, no .git, no logs or caches), and writes SHA256SUMS over every asset in the folder and VERSION.
  Nothing is uploaded and nothing is tagged: attaching the files to a release is the workflow's step (or yours).

  Run it from a clean checkout of the commit to release: it refuses uncommitted changes to tracked files, and checks
  that the program's own commit is HEAD. Cargo builds into $env:CARGO_TARGET_DIR when that is set, else target\.
  Windows PowerShell 5.1 and PowerShell 7 (Linux runners) both run it.

.EXAMPLE
  .\tools\make-release.ps1 -Out D:\scratch\release          # the Windows asset, SHA256SUMS and VERSION
  .\tools\make-release.ps1 -Out dist -Tag v0.2.0            # also refuse unless the program's version is 0.2.0
  .\tools\make-release.ps1 -Out dist -SumsOnly              # only rewrite SHA256SUMS over what dist already holds
#>
param(
    [Parameter(Mandatory = $true)][string]$Out,
    [string]$Tag = "",          # a release tag (v0.2.0): the program's version must be exactly that
    [switch]$AllowDirty,        # build from a tree with local changes (a local trial only; the VERSION says -dirty)
    [switch]$SumsOnly           # write SHA256SUMS over the files already in -Out and stop
)
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
$windows = ($env:OS -eq "Windows_NT")
$exeName = if ($windows) { "magician.exe" } else { "magician" }
$asset = if ($windows) { "magician-windows-x64.zip" } else { "magician-linux-x64.tar.gz" }
New-Item -ItemType Directory -Force $Out | Out-Null
$Out = (Resolve-Path $Out).Path
$utf8 = New-Object System.Text.UTF8Encoding($false)

function Write-Sums {
    # sha256sum's own form, LF line ends and no byte order mark, so `sha256sum -c SHA256SUMS` reads it
    $lines = @()
    $files = Get-ChildItem $Out -File | Where-Object { $_.Name -like "magician-*.zip" -or $_.Name -like "magician-*.tar.gz" -or $_.Name -eq "VERSION" } | Sort-Object Name
    foreach ($f in $files) { $lines += ((Get-FileHash $f.FullName -Algorithm SHA256).Hash.ToLower() + "  " + $f.Name) }
    [System.IO.File]::WriteAllText((Join-Path $Out "SHA256SUMS"), (($lines -join "`n") + "`n"), $utf8)
    return $files
}

if ($SumsOnly) {
    $files = Write-Sums
    Write-Host "SHA256SUMS over $($files.Count) file(s) in $Out"
    exit 0
}

Push-Location $root
try {
    if (-not $AllowDirty) {
        $dirty = git status --porcelain --untracked-files=no
        if ($dirty) { throw "the tree has local changes; commit them (or pass -AllowDirty for a local trial):`n$dirty" }
    }
    cargo build --release -p magician
    if ($LASTEXITCODE -ne 0) { throw "cargo build --release failed" }
    $target = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $root "target" }
    $exe = Join-Path (Join-Path $target "release") $exeName
    if (-not (Test-Path $exe)) { throw "no $exe after the build" }

    # VERSION is what the built program says about itself, never typed by hand
    $version = (& $exe update --describe) -join "`n"
    if ($LASTEXITCODE -ne 0) { throw "$exe update --describe failed" }
    $commit = ([regex]::Match($version, '(?m)^commit = (\S+)')).Groups[1].Value
    $head = (git rev-parse --short=12 HEAD).Trim()
    if (-not $AllowDirty -and $commit -ne $head) { throw "the program was built from commit '$commit', and HEAD is $head" }
    if ($Tag) {
        $v = ([regex]::Match($version, '(?m)^version = (\S+)')).Groups[1].Value
        if ($Tag -ne "v$v") { throw "tag $Tag is not v$v, the version the program reports" }
    }

    # the toolkit folder, from tracked files only
    $stage = Join-Path ([System.IO.Path]::GetTempPath()) ("magician-release-" + [guid]::NewGuid().ToString("N"))
    New-Item -ItemType Directory -Force $stage | Out-Null
    Copy-Item $exe (Join-Path $stage $exeName)
    $tracked = git ls-files -- ingame tools mcp skills docs README.md CHANGELOG.md LICENSE
    foreach ($rel in $tracked) {
        if ($rel -match '(^|/)(briefs|__pycache__|logs)/') { continue }
        $dest = Join-Path $stage $rel
        New-Item -ItemType Directory -Force (Split-Path -Parent $dest) | Out-Null
        Copy-Item (Join-Path $root $rel) $dest
    }
    foreach ($must in @($exeName, "ingame/mod_session.py", "README.md", "CHANGELOG.md", "LICENSE")) {
        if (-not (Test-Path (Join-Path $stage $must))) { throw "the toolkit folder lacks $must" }
    }

    $zip = Join-Path $Out $asset
    if (Test-Path $zip) { Remove-Item $zip }
    if ($windows) {
        # System32's tar (bsdtar) writes the zip; Git's GNU tar earlier on PATH cannot
        & (Join-Path $env:SystemRoot "System32\tar.exe") -a -c -f $zip -C $stage .
    } else {
        tar -czf $zip -C $stage .
    }
    if ($LASTEXITCODE -ne 0) { throw "could not pack $zip" }
    Remove-Item -Recurse -Force $stage
    [System.IO.File]::WriteAllText((Join-Path $Out "VERSION"), ($version + "`n"), $utf8)
    $files = Write-Sums
    Write-Host "wrote $($files.Count) file(s) and SHA256SUMS to ${Out}:"
    Get-Content (Join-Path $Out "SHA256SUMS")
}
finally {
    Pop-Location
}

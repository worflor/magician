# The cinema kit's dev pak: build, install for a shoot, uninstall afterwards. Dev only: never ships with a mod.
#
#   powershell -ExecutionPolicy Bypass -File cinema\cinema.ps1 build       # lights + bridge staged, magician build -> pak90
#   powershell ... cinema.ps1 install     # game closed first; installs pak90 for the mod "cinema"
#   powershell ... cinema.ps1 uninstall   # after the shoot, so no QA pass or real match ever runs with it
#   powershell ... cinema.ps1 status
#
# pak90: lower numbers win collisions, so every mod (pak01-pak20) beats it; the kit only adds files and replaces two
# Panorama files no mod ships (unit_status_v2.vcss, hud_hero_testing.vxml). The addon is CSDK
# citadel_addons\cinema_dev, a junction to this folder's addon\.
param([Parameter(Mandatory = $true)][ValidateSet("build", "install", "uninstall", "status")][string]$Action,
      [string]$Out = "")
$Here = Split-Path -Parent $MyInvocation.MyCommand.Path
$Paths = Join-Path (Split-Path -Parent $Here) "ingame\gamepaths.py"
if (-not $Out) { $Out = Join-Path (python -I $Paths work) "cinema\pak90_dir.vpk" }   # <reference>\work\cinema\
$Magician = Join-Path (Split-Path -Parent $Here) "target\release\magician.exe"
$Junction = Join-Path "$(python -I $Paths csdk)" "content\citadel_addons\cinema_dev"   # the CSDK as magician.exe finds it
switch ($Action) {
    "build" {
        if (-not (Test-Path $Junction)) { New-Item -ItemType Junction -Path $Junction -Target (Join-Path $Here "addon") | Out-Null }
        python -I (Join-Path $Here "studio_lights.py"); if (-not $?) { exit 1 }
        python -I (Join-Path $Here "bridge\build_bridge.py"); if (-not $?) { exit 1 }
        & $Magician build cinema_dev $Out
        exit $LASTEXITCODE
    }
    "install" {
        if (Get-Process deadlock -ErrorAction SilentlyContinue) { Write-Host "close the game first (ingame\stop_game.ps1): a running game holds the addon paks"; exit 1 }
        & $Magician install $Out --mod cinema
        exit $LASTEXITCODE
    }
    "uninstall" {
        if (Get-Process deadlock -ErrorAction SilentlyContinue) { Write-Host "close the game first (ingame\stop_game.ps1)"; exit 1 }
        & $Magician uninstall (Split-Path -Leaf $Out)
        exit $LASTEXITCODE
    }
    "status" { & $Magician install --status }
}

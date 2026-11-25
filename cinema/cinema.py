"""The cinema kit's dev pak: build, install for a shoot, uninstall afterwards. Dev only: never ships with a mod.

    python -I cinema/cinema.py build       lights and bridge staged, `magician build` -> pak90
    python -I cinema/cinema.py install     the game closed first; installs pak90 for the mod "cinema"
    python -I cinema/cinema.py uninstall   after the shoot, so no QA pass or real match ever runs with it
    python -I cinema/cinema.py status
    option: --out <pak90_dir.vpk>          default <reference>/work/cinema/pak90_dir.vpk

pak90: lower numbers win collisions, so every mod (pak01-pak20) beats it; the kit only adds files and replaces two
Panorama files no mod ships (unit_status_v2.vcss, hud_hero_testing.vxml). The addon is the CSDK's
citadel_addons/cinema_dev, a link to this folder's addon/ (made on first build: a junction on Windows, a symbolic link
elsewhere). The build compiles with the CSDK, under Wine off Windows (`magician paths` shows both). Standard library
only; cinema.ps1 is the same with PowerShell's options.
"""
import os
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(os.path.dirname(HERE), "ingame"))
import gameos  # noqa: E402
import gamepaths  # noqa: E402


def link_addon(csdk):
    """content/citadel_addons/cinema_dev in the CSDK, pointing at this folder's addon/ (made once)."""
    link = os.path.join(csdk, "content", "citadel_addons", "cinema_dev")
    if os.path.lexists(link):
        return link
    target = os.path.join(HERE, "addon")
    if gameos.WINDOWS:   # a junction needs no administrator, unlike a symbolic link
        subprocess.run(["cmd", "/c", "mklink", "/J", link, target], check=True, stdout=subprocess.DEVNULL)
    else:
        os.symlink(target, link)
    return link


def main(argv):
    if not argv or argv[0] in ("--help", "-h", "help") or argv[0] not in ("build", "install", "uninstall", "status"):
        print(__doc__)
        return 0 if argv[:1] in (["--help"], ["-h"], ["help"]) else 2
    action = argv[0]
    out = argv[argv.index("--out") + 1] if "--out" in argv else os.path.join(gamepaths.work("cinema"), "pak90_dir.vpk")
    magician = gamepaths.magician()
    if action == "build":
        try:
            link_addon(gamepaths.csdk())
        except LookupError as e:
            print(f"cinema: {e}", file=sys.stderr)
            return 2
        for step in ("studio_lights.py", os.path.join("bridge", "build_bridge.py")):
            if subprocess.run([sys.executable, "-I", os.path.join(HERE, step)]).returncode != 0:
                return 1
        os.makedirs(os.path.dirname(out), exist_ok=True)
        return subprocess.run([magician, "build", "cinema_dev", out]).returncode
    if action in ("install", "uninstall") and gameos.game_pids():
        print("close the game first (ingame/stop_game.py): a running game holds the addon paks")
        return 1
    if action == "install":
        return subprocess.run([magician, "install", out, "--mod", "cinema"]).returncode
    if action == "uninstall":
        return subprocess.run([magician, "uninstall", os.path.basename(out)]).returncode
    return subprocess.run([magician, "install", "--status"]).returncode


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))

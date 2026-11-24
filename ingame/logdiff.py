"""What did the mod add to the game's log? Diff a modded console.log against a vanilla baseline of the same scene.

    python -I logdiff.py --build 6866 --scene haze_new_player_basics [--mod <console.log>]
    python -I logdiff.py --mod <console.log> --base <vanilla.log>          # two named logs
    python -I logdiff.py                                   # current console.log vs the newest vanilla baseline
    python -I logdiff.py --save-vanilla --build 6866 --scene haze_new_player_basics [--mod <console.log>]
    python -I logdiff.py --save-baseline <name>            # copy the current console.log in under a name of your own

Lines are normalised before comparing: timestamps, pointers, entity indices, durations and long float tails go,
so a warning Valve's own build also prints (a camera-pose "height overshoot", missing ui_hero_reveal clips,
blocking particle loads) cancels out, and recompile float noise (6.420000 vs 6.419996) doesn't count as new.
Only warning-shaped lines are kept (warning, error, failed, unable, invalid, missing, cannot, overshoot).

Baselines live in <reference>\\work\\ingame-baselines\\ as vanilla_<build>_<scene>.log. A scene is what was played:
the hero and the map (`scene_name`), because another hero's vanilla log says nothing about this one's. With --build
and --scene the baseline of that build is used; when the build has none (the day of a game update), the newest
older build's for the same scene is, and the first line of the output says so: warnings Valve added or dropped in
the update then show up as the mod's, so take a new baseline (`mod_session.py qa --vanilla --restore`). A scene
with no baseline on any build is not compared.

Exit codes: 0 nothing new, 1 new warnings with the mod, 3 not compared (no baseline for the scene).
"""
import glob
import os
import re
import shutil
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))  # python -I leaves the script folder off the path
import gamepaths  # noqa: E402

LOG = os.path.join(gamepaths.try_game(), "citadel", "console.log")
BASES = gamepaths.work("ingame-baselines")
WARN = re.compile(r"warn|error|fail|unable|invalid|missing|cannot|can't|overshoot|not found|not precached", re.I)
# Not the mod's and not the scene's: timing chatter, and the state of the network (Steam, the game coordinator). A
# Steam hiccup during one of two passes would otherwise read as warnings the mod added.
NOISE = re.compile(r"PredictionError|Avg Frame|Performant|Frame Time|Idle Time|VProf"
                   r"|\[GCClient\]|\[SteamNetSockets\]|OnSteamServerConnect|SteamNetworkingSockets")
NAME = re.compile(r"vanilla_(\d+)_(.+)\.log$")


def norm(line):
    s = re.sub(r"^\d\d/\d\d \d\d:\d\d:\d\d\s*", "", line.strip())
    s = re.sub(r"0x[0-9a-fA-F]+|\b[0-9A-F]{12,16}\b", "<ptr>", s)
    s = re.sub(r"-?\d+\.\d{3,}", lambda m: f"{float(m.group()):.2f}", s)   # float noise: round, don't truncate
    s = re.sub(r"#\d{4,}", "#<n>", s)                                       # per-launch connection ids
    s = re.sub(r"\b\d+(\.\d+)?\s*(ms|s|MB|KB|bytes)\b", r"<n>\2", s)
    s = re.sub(r"(entity|entindex|player slot|hero) \d+", r"\1 <n>", s)
    s = re.sub(r"\d+/hero_", "<n>/hero_", s)
    return s


def warnings(path):
    out = {}
    with open(path, encoding="utf-8", errors="replace") as f:
        for line in f:
            if WARN.search(line) and not NOISE.search(line):
                k = norm(line)
                out[k] = out.get(k, 0) + 1
    return out


def scene_name(hero, game_map):
    """The scene a session played, as a baseline is named for it: `haze_new_player_basics`, `menu` with neither."""
    hero = (hero or "")[5:] if (hero or "").startswith("hero_") else (hero or "")
    parts = [p for p in (hero, game_map or "") if p]
    return re.sub(r"[^a-z0-9_]+", "_", "_".join(parts).lower()) or "menu"


def baseline_path(build, scene, bases=None):
    return os.path.join(bases or BASES, f"vanilla_{build}_{scene}.log")


def pick_base(build, scene, bases=None):
    """The vanilla baseline for a scene: (path, its build, True) for this build's, else (path, its build, False) for
    the newest older build's, else None. Never another scene's, and never a newer build's."""
    exact = baseline_path(build, scene, bases)
    if os.path.isfile(exact):
        return exact, str(build), True
    older = []
    for p in glob.glob(os.path.join(bases or BASES, "vanilla_*.log")):
        m = NAME.search(os.path.basename(p))
        if m and m.group(2) == scene and int(m.group(1)) < int(build):
            older.append((int(m.group(1)), p))
    if not older:
        return None
    b, p = max(older)
    return p, str(b), False


def save_vanilla(log, build, scene, bases=None):
    dst = baseline_path(build, scene, bases)
    os.makedirs(os.path.dirname(dst), exist_ok=True)
    shutil.copyfile(log, dst)
    return dst


def main(a):
    def opt(n, d=None):
        return a[a.index(n) + 1] if n in a else d

    mod = opt("--mod", LOG)
    build, scene = opt("--build"), opt("--scene")
    if "--save-baseline" in a:
        os.makedirs(BASES, exist_ok=True)
        dst = os.path.join(BASES, opt("--save-baseline") + ".log")
        shutil.copyfile(LOG, dst)
        print(f"baseline -> {dst}")
        return 0
    if "--save-vanilla" in a:
        if not (build and scene):
            print("--save-vanilla needs --build and --scene", file=sys.stderr)
            return 2
        print(f"baseline -> {save_vanilla(mod, build, scene)}")
        return 0
    note = ""
    if opt("--base"):
        base = opt("--base")
    elif build and scene:
        got = pick_base(build, scene)
        if got is None:
            print(f"NOT COMPARED: no vanilla baseline for scene {scene} on build {build} or any older one "
                  f"(take one: mod_session.py qa --vanilla --restore)")
            return 3
        base, base_build, exact = got
        if not exact:
            note = (f"NOTE: no vanilla baseline for build {build} yet; this one is from build {base_build}. What Valve's "
                    f"update added or dropped reads as the mod's below. Take a new one: mod_session.py qa --vanilla --restore\n")
    else:
        bases = glob.glob(os.path.join(BASES, "vanilla_*.log"))
        if not bases:
            print(f"NOT COMPARED: no vanilla baseline in {BASES}")
            return 3
        base = max(bases, key=os.path.getmtime)
    m, b = warnings(mod), warnings(base)
    new = sorted(k for k in m if k not in b)
    gone = sorted(k for k in b if k not in m)
    print(f"{note}mod log: {mod}\nbaseline: {base}\n")
    print(f"NEW warnings with the mod ({len(new)}):")
    for k in new:
        print(f"  x{m[k]:<3} {k[:220]}")
    print(f"\nwarnings the baseline has that this run doesn't ({len(gone)}) - usually just a scene not visited:")
    for k in gone[:15]:
        print(f"        {k[:160]}")
    return 1 if new else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))

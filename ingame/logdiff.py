"""What did the mod add to the game's log? Diff a modded console.log against a vanilla baseline from the same build.

    python -I logdiff.py                                   # current console.log vs the newest vanilla baseline
    python -I logdiff.py --mod <console.log> --base <vanilla.log>
    python -I logdiff.py --save-baseline <name>            # copy the current console.log in as a vanilla baseline

Lines are normalised before comparing: timestamps, pointers, entity indices, durations and long float tails go,
so a warning Valve's own build also prints (Lash's camera-pose "height overshoot", missing ui_hero_reveal clips,
blocking particle loads) cancels out, and recompile float noise (6.420000 vs 6.419996) doesn't count as new.
Only warning-shaped lines are kept (warning, error, failed, unable, invalid, missing, cannot, overshoot).
Baselines live in <reference>\\work\\ingame-baselines\\ (one per build and scene).
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
NOISE = re.compile(r"PredictionError|Avg Frame|Performant|Frame Time|Idle Time|VProf")


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


def main(a):
    def opt(n, d=None):
        return a[a.index(n) + 1] if n in a else d

    if "--save-baseline" in a:
        os.makedirs(BASES, exist_ok=True)
        dst = os.path.join(BASES, opt("--save-baseline") + ".log")
        shutil.copyfile(LOG, dst)
        print(f"baseline -> {dst}")
        return 0
    mod = opt("--mod", LOG)
    base = opt("--base") or max(glob.glob(os.path.join(BASES, "vanilla_*.log")), key=os.path.getmtime)
    m, b = warnings(mod), warnings(base)
    new = sorted(k for k in m if k not in b)
    gone = sorted(k for k in b if k not in m)
    print(f"mod log: {mod}\nbaseline: {base}\n")
    print(f"NEW warnings with the mod ({len(new)}):")
    for k in new:
        print(f"  x{m[k]:<3} {k[:220]}")
    print(f"\nwarnings the baseline has that this run doesn't ({len(gone)}) - usually just a scene not visited:")
    for k in gone[:15]:
        print(f"        {k[:160]}")
    return 1 if new else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))

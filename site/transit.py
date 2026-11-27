"""The proof section's transit map: re-read the mods' proofs and Magician's records, and write them into index.html.

Run it after new proofs land (python site/transit.py). It reads, and never writes, anything but index.html:
  - each mod's proofs.tsv and mod.toml (the mods folder beside this repository, or --mods);
  - `magician patchday --json` for what each mod ships today and its hero's data since its last proof;
  - `magician drift --from A --to B --json` for every pair of builds in a row: what changed under the paks each mod
    ships today;
  - the reference folder's baseline.tsv and engine.tsv per build (written by `magician baseline`): what Valve changed.
Only the two published skins go into the page, and only the fields below: no paths from this machine, no other mod.
"""

import argparse
import json
import re
import subprocess
import sys
import tomllib
from pathlib import Path

HERE = Path(__file__).resolve().parent
MODS = ["maid-lash", "loops-viscous"]

# What Valve changed between two builds, in plain words, by the files' place in the game.
AREAS = [
    ("scripts/heroes.vdata_c", "hero data"),
    ("scripts/abilities.vdata_c", "ability data"),
    ("scripts/", "game data"),
    ("particles/", "effects"),
    ("models/", "models"),
    ("panorama/", "the HUD"),
    ("sounds/", "sound"),
    ("soundevents/", "sound"),
    ("materials/", "materials"),
    ("maps/", "maps"),
]


def area(path):
    for prefix, word in AREAS:
        if path.startswith(prefix):
            return word
    return "other files"


def run(magician, args, cwd=None):
    """A Magician command's stdout. Exit 4 (re-prove) and 1 (broken) still print their report."""
    p = subprocess.run([magician, *args], cwd=cwd, capture_output=True, text=True, encoding="utf-8")
    if p.returncode not in (0, 1, 4):
        sys.exit(f"magician {' '.join(args)}: exit {p.returncode}\n{p.stderr}")
    return p.stdout


def paths(magician):
    out = {}
    for line in run(magician, ["paths"]).splitlines():
        parts = line.split("\t")
        if len(parts) >= 2:
            out[parts[0]] = parts[1]
    return out


def table(path):
    """A Magician tab-separated record (baseline.tsv, engine.tsv): path -> its second column."""
    if not path.exists():
        return None
    out = {}
    for line in path.read_text(encoding="utf-8").splitlines():
        if line and not line.startswith("#"):
            parts = line.split("\t")
            out[parts[0]] = parts[1] if len(parts) > 1 else ""
    return out


def engine(path):
    if not path.exists():
        return None
    out = {}
    for line in path.read_text(encoding="utf-8").splitlines():
        if line and not line.startswith("#"):
            parts = line.split("\t")
            out[parts[0]] = parts[-1]
    return out


def proofs(path):
    rows = []
    lines = [l for l in path.read_text(encoding="utf-8").splitlines() if l and not l.startswith("#")]
    head = lines[0].split("\t")
    for line in lines[1:]:
        rows.append(dict(zip(head, line.split("\t"))))
    return rows


def junction(reference, a, b):
    """What Valve changed from build a to build b, from Magician's records of both."""
    ta, tb = table(reference / a / "baseline.tsv"), table(reference / b / "baseline.tsv")
    j = {"from": a, "to": b, "areas": [], "changed": 0, "added": 0, "removed": 0, "code": None}
    if ta is None or tb is None:
        j["areas"] = None
        return j
    changed = [k for k in tb if k in ta and ta[k] != tb[k]]
    added = [k for k in tb if k not in ta]
    removed = [k for k in ta if k not in tb]
    j.update(changed=len(changed), added=len(added), removed=len(removed))
    words = []
    for k in changed + added + removed:
        w = area(k)
        if w not in words:
            words.append(w)
    ea, eb = engine(reference / a / "engine.tsv"), engine(reference / b / "engine.tsv")
    if ea is not None and eb is not None:
        j["code"] = sorted({k.rsplit("/", 1)[-1] for k in set(ea) | set(eb) if ea.get(k) != eb.get(k)})
    j["areas"] = words
    return j


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--magician", default="magician", help="the magician binary (default: on PATH)")
    ap.add_argument("--mods", default=str(HERE.parent.parent / "model-replacements"), help="the folder holding the mods")
    ap.add_argument("--page", default=str(HERE / "index.html"))
    ap.add_argument("--dry-run", action="store_true", help="print the data instead of writing the page")
    a = ap.parse_args()

    where = paths(a.magician)
    reference, game = Path(where["reference"]), Path(where["game"])
    mods_dir = Path(a.mods)
    tomls = [mods_dir / m / "mod.toml" for m in MODS]
    pd = json.loads(run(a.magician, ["patchday", "--json", *sum([["--mod", str(t)] for t in tomls], [])], cwd=mods_dir))
    installed = pd["installed"]
    ledger = {e["owner"]: e for e in pd["install"]["ledger"]}
    manifests = {m["slug"]: m for m in pd["manifests"]}

    mods = []
    first = None
    for slug, toml in zip(MODS, tomls):
        meta = tomllib.loads(toml.read_text(encoding="utf-8"))["mod"]
        owners = meta.get("owners") or [slug.replace("-", "_")]
        rows = proofs(toml.parent / "proofs.tsv")
        # one proof: its build, date and the bytes of every file it holds; a segment is one set of bytes
        by_proof = {}
        for r in rows:
            p = by_proof.setdefault(r["proof"], {"build": r["build"], "date": r["date"], "files": []})
            p["files"].append([r["file"], r["crc32"]])
        segments = []
        for order, p in enumerate(by_proof.values()):
            key = sorted(map(tuple, p["files"]))
            seg = next((s for s in segments if s["_key"] == key), None)
            if seg is None:
                main_file = next((f for f in key if f[0].endswith("_dir.vpk")), key[0])
                seg = {"_key": key, "_order": order, "pak": main_file[1], "files": [list(f) for f in key], "proofs": []}
                segments.append(seg)
            if not any(x["build"] == p["build"] for x in seg["proofs"]):
                seg["proofs"].append({"build": p["build"], "date": p["date"]})
        # the bytes it ships today: what the install ledger holds under its owners
        today = sorted((ledger[o]["name"], ledger[o]["crc"]) for o in owners if o in ledger and ledger[o].get("state") == "installed")
        for s in segments:
            s["current"] = bool(today) and [(f, c) for f, c in today] == [tuple(f) for f in s["_key"]]
        if not any(s["current"] for s in segments) and segments and not today:
            segments[-1]["current"] = True
        # older bytes first, by the build they were first proven on; within a build, the bytes shipped today last
        segments.sort(key=lambda s: (min(int(p["build"]) for p in s["proofs"]), s["current"], s["_order"]))
        for s in segments:
            s["proofs"].sort(key=lambda p: int(p["build"]))
            del s["_key"], s["_order"]
        lo = min(int(r["build"]) for r in rows)
        first = lo if first is None else min(first, lo)
        man = manifests.get(slug, {})
        hd = man.get("hero_data") or {}
        mods.append({
            "slug": slug,
            "name": meta["name"],
            "segments": segments,
            "paks": [game / "citadel" / "addons" / name for name, _ in today],
            "hero_data": {k: hd.get(k) for k in ("hero", "from", "state", "gone", "new")} if hd else None,
            "rows": [[r["build"], r["date"], r["owner"], r["file"], r["crc32"]] for r in rows],
        })

    builds = sorted(
        (d.name for d in reference.iterdir() if d.is_dir() and re.fullmatch(r"\d+", d.name) and (d / "baseline.tsv").exists() and first <= int(d.name) <= int(installed)),
        key=int,
    )
    junctions = [junction(reference, x, y) for x, y in zip(builds, builds[1:])]
    # what changed under each mod, pair by pair: Magician's drift of the paks it ships today
    for m in mods:
        m["drift"] = []
        start = min(int(p["build"]) for s in m["segments"] for p in s["proofs"])
        for x, y in zip(builds, builds[1:]):
            if int(x) < start or not m["paks"]:
                continue
            d = json.loads(run(a.magician, ["drift", *map(str, m["paks"]), "--from", x, "--to", y, "--json"]))
            under = []
            for p in d["paks"]:
                for k in ("changed", "removed", "broken", "deps_changed", "bones_lost", "params_lost"):
                    under += [f"{k.replace('_', ' ')}: {v}" for v in p.get(k, [])]
            m["drift"].append({"from": x, "to": y, "state": d["state"], "under": under})
        del m["paks"]

    data = {"installed": installed, "builds": builds, "junctions": junctions, "mods": mods}
    text = json.dumps(data, ensure_ascii=False, separators=(",", ":"))
    if a.dry_run:
        print(json.dumps(data, ensure_ascii=False, indent=1))
        return
    page = Path(a.page)
    html = page.read_bytes().decode("utf-8")
    new, n = re.subn(r'(<script id="transit-data" type="application/json">).*?(</script>)', lambda mm: mm.group(1) + text.replace("</", "<\\/") + mm.group(2), html, count=1, flags=re.S)
    if n != 1:
        sys.exit(f"{page}: no <script id=\"transit-data\"> to fill")
    page.write_bytes(new.encode("utf-8"))
    print(f"{page.name}: transit data for builds {builds[0]}-{builds[-1]}, installed {installed}: " + ", ".join(f"{m['name']} {sum(len(s['proofs']) for s in m['segments'])} stops" for m in mods))


if __name__ == "__main__":
    main()

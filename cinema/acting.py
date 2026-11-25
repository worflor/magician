"""Acting for a shoot: clips remapped onto moves the actors can trigger, and extra files, folded into the dev-only
cinema pak (pak90). Never shipped with a mod; pak90 is uninstalled after every session.

    python -I acting.py check <acting.toml>      # every source found, every target free (no lower pak masks it)
    python -I acting.py build <acting.toml>      # after cinema.ps1 build: pak90 repacked with the acting files

An acting file:

    [[remap]]
    from = "pak10:models/heroes_staging/lash_v2/clips/ui_hero_emote_02.vnmclip_c"   # an installed pak, pak01 (the
    to = "models/heroes_staging/lash_v2/clips/weapon_crouch_idle.vnmclip_c"         # game), or a file on disk
    why = "the smug curtsy on +duck (a shot first: the combat crouch)"

    [[file]]                                    # anything else the shoot needs (a particle, a material, a model)
    from = "D:/.../spill.vpcf_c"
    to = "particles/cinema/spill.vpcf_c"

How a remap works: the AnimGraph plays a clip by its path, so a compiled clip of the same skeleton at another
clip's path plays wherever the graph plays that one: the actor's own controls trigger it (crouch, an ability, the
emote key, an idle that always plays). Two limits, both checked here:
- A pak with a lower number wins a path. Maid Lash's pak10 ships 92 of Lash's clips, so pak90 can remap only the
  clips pak10 leaves to Valve (weapon_crouch_idle, the dashes, mantles, item casts, Flog, Ground Strike ...).
- The clip must be of the actor's own skeleton (a clip of Lash's on Lash's path): a clip from another hero
  plays on the wrong bones.

To remap a clip a skin pak owns (Lash's out-of-combat crouch, where his arms are free: pak10 ships it), the acting
file can install the cinema pak BELOW that pak for one shoot:

    [pak]
    install_as = "pak09_dir.vpk"      # wins over pak10..pak89; checked: no kit file may collide with those paks

The build writes `<pak>.install_as` beside the pak; session.py installs it under that name, uninstalls and verifies
it the same way. A later acting build without `[pak]` removes the note (pak90 again). The takes recorded with it
must be rendered with the same acting file (the clips are resolved at playback).
"""
import json
import os
import shutil
import subprocess
import sys

try:
    import tomllib
except ImportError:
    tomllib = None

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import vpkx  # noqa: E402
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "ingame"))
import gamepaths  # noqa: E402  (locations as magician.exe resolves them)

GAME = os.path.join(gamepaths.try_game(), "citadel")
ADDONS = os.path.join(GAME, "addons")
MAGICIAN = os.path.join(os.path.dirname(HERE), "target", "release", "magician.exe" if os.name == "nt" else "magician")
WORK = os.environ.get("CINEMA_WORK") or gamepaths.work("cinema")
PAK90 = os.path.join(WORK, "pak90_dir.vpk")


class ActingError(RuntimeError):
    pass


def install_as(path):
    """The pak name the acting file asks to be installed as (default pak90_dir.vpk), validated."""
    with open(path, "rb") as f:
        d = tomllib.load(f)
    name = str(d.get("pak", {}).get("install_as", "pak90_dir.vpk")).lower()
    if not (name.startswith("pak") and name.endswith("_dir.vpk") and name[3:5].isdigit() and len(name) == 13):
        raise ActingError(f"[pak] install_as must be pakNN_dir.vpk: {name!r}")
    if os.path.exists(os.path.join(ADDONS, name)) and name != "pak90_dir.vpk":
        raise ActingError(f"[pak] install_as {name}: that slot is taken by an installed pak")
    return name


def slot(name):
    return int(name[3:5])


def load(path):
    with open(path, "rb") as f:
        d = tomllib.load(f)
    items = []
    for kind in ("remap", "file"):
        for e in d.get(kind, []):
            if not isinstance(e, dict) or "from" not in e or "to" not in e:
                raise ActingError(f"[[{kind}]] needs from and to: {e!r}")
            items.append({"kind": kind, "from": str(e["from"]), "to": str(e["to"]).replace("\\", "/").lower(),
                          "why": str(e.get("why", ""))})
    return items


def source_bytes(src):
    """pakNN:<path> (game/citadel/addons/pakNN_dir.vpk), pak01:<path> (the game's own), or a file."""
    if ":" in src and src.split(":", 1)[0].lower().startswith("pak") and not os.path.exists(src):
        pak, inner = src.split(":", 1)
        p = os.path.join(GAME if pak.lower() == "pak01" else ADDONS, f"{pak.lower()}_dir.vpk")
        if not os.path.exists(p):
            raise ActingError(f"{src}: no {p}")
        return vpkx.Vpk(p).read(inner)
    p = os.path.expandvars(src)
    if not os.path.exists(p):
        raise ActingError(f"{src}: no such file")
    return open(p, "rb").read()


def installed_paks(lo, hi):
    """Installed addon paks numbered in [lo, hi)."""
    return [f for f in (sorted(os.listdir(ADDONS)) if os.path.isdir(ADDONS) else [])
            if f.startswith("pak") and f.endswith("_dir.vpk") and f[3:5].isdigit() and lo <= int(f[3:5]) < hi]


def maskers(target, below=90):
    """Installed addon paks numbered below the cinema pak's slot that ship the target path (they win it)."""
    out = []
    for f in installed_paks(0, below):
        try:
            if target in vpkx.Vpk(os.path.join(ADDONS, f)).entries:
                out.append(f)
        except vpkx.VpkError:
            pass
    return out


def check(items, below=90):
    problems = []
    for it in items:
        try:
            n = len(source_bytes(it["from"]))
        except (ActingError, vpkx.VpkError) as e:
            problems.append(str(e))
            continue
        m = maskers(it["to"], below)
        if m:
            problems.append(f"{it['to']}: shipped by {', '.join(m)}, which wins over pak{below:02d}: pick another clip")
        print(f"  {it['kind']:<5} {it['to']}  <- {it['from']} ({n} bytes){'  ' + it['why'] if it['why'] else ''}")
    return problems


def collisions(items, below, pak=PAK90):
    """Installed below the mods (install_as), the kit must not win any path but the acting targets: every other file
    of the pak shipped by a pak it now beats (pak<below>..pak89) is a problem."""
    if below >= 90 or not os.path.exists(pak):
        return []
    mine = set(it["to"] for it in items)
    kit = set(n.lower() for n in vpkx.Vpk(pak).names()) - mine
    out = []
    for f in installed_paks(below, 90):
        try:
            hit = kit & set(e.lower() for e in vpkx.Vpk(os.path.join(ADDONS, f)).entries)
        except vpkx.VpkError:
            continue
        out += [f"{h}: the kit would now win it over {f}" for h in sorted(hit)]
    return out


def build(items, pak=PAK90, as_name="pak90_dir.vpk"):
    """pak90 as built by cinema.ps1, plus the acting files, repacked deterministically by `magician pack`."""
    if not os.path.exists(pak):
        raise ActingError(f"no {pak}: run cinema.ps1 build first")
    stage = os.path.join(WORK, "acting_stage")
    shutil.rmtree(stage, ignore_errors=True)
    base = vpkx.Vpk(pak)
    # a pak90 that already carries acting files (an earlier acting build) drops them first: the build is idempotent
    ledger = pak + ".acting.json"
    added = set(json.load(open(ledger, encoding="utf-8"))) if os.path.exists(ledger) else set()
    names = [n for n in base.names() if n not in added]
    for n in names:
        p = os.path.join(stage, n)
        os.makedirs(os.path.dirname(p), exist_ok=True)
        with open(p, "wb") as f:
            f.write(base.read(n))
    for it in items:
        p = os.path.join(stage, it["to"])
        os.makedirs(os.path.dirname(p), exist_ok=True)
        with open(p, "wb") as f:
            f.write(source_bytes(it["from"]))
    tmp = pak + ".acting.vpk"
    r = subprocess.run([MAGICIAN, "pack", stage, tmp], capture_output=True, text=True)
    print((r.stdout + r.stderr).strip()[-1500:])
    if not os.path.exists(tmp):
        raise ActingError("magician pack wrote nothing")
    os.replace(tmp, pak)
    with open(ledger, "w", encoding="utf-8") as f:
        json.dump(sorted(it["to"] for it in items), f, indent=1)
    note = pak + ".install_as"
    if as_name != "pak90_dir.vpk":
        with open(note, "w", encoding="utf-8") as f:
            f.write(as_name + "\n")
        print(f"ACTING install as {as_name} (below the mods it must beat: {note})")
    elif os.path.exists(note):
        os.remove(note)
    shutil.rmtree(stage, ignore_errors=True)
    print(f"ACTING {pak}: {len(names)} kit files + {len(items)} acting files")
    return pak


def main(a):
    if len(a) < 2 or a[0] not in ("check", "build"):
        sys.exit(__doc__)
    try:
        items = load(a[1])
        as_name = install_as(a[1])
        problems = check(items, slot(as_name)) + collisions(items, slot(as_name))
        for p in problems:
            print("problem:", p)
        if problems:
            sys.exit(1)
        if a[0] == "build":
            build(items, as_name=as_name)
    except ActingError as e:
        sys.exit(f"acting: {e}")


if __name__ == "__main__":
    main(sys.argv[1:])

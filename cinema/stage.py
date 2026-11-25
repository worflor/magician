"""The stage: landmarks of a map read from its compiled entity lump, for scouting shots without the game.

    python -I stage.py list   <map> [--class point_camera] [--near x y [r]]   # landmarks, grouped
    python -I stage.py camera <map> <targetname>                              # a Valve camera as a spec_goto line
    python -I stage.py md     <map>                                           # the markdown table for the README

<map> is dl_midtown, new_player_basics, dl_hideout (citadel/maps/<map>.vpk). The entity lump
(`maps/<map>/entities/default_ents.vents_c`, KV3 zstd-compressed on 6864) is decoded by `magician dump` and cached
as text in work\\cinema\\maps\\<map>_ents.txt; nothing here is committed (it is Valve's data).

Valve's own `screenshot_camera_NN` point cameras (dl_midtown has 18) are composed views of the city: their origin
and angles drop straight into a scene as a `fixed` camera (`pos`, `ang`). Note the spectator's lens is fixed (about
90 degrees), so a camera Valve composed at another FOV frames differently (`zoom` crops in).
"""
import os
import re
import subprocess
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "ingame"))
import gamepaths  # noqa: E402  (locations as magician.exe resolves them)

HERE = os.path.dirname(os.path.abspath(__file__))
MAGICIAN = os.path.join(os.path.dirname(HERE), "target", "release", "magician.exe" if os.name == "nt" else "magician")
MAPS = os.path.join(gamepaths.try_game(), "citadel", "maps")
WORK = os.environ.get("CINEMA_WORK") or gamepaths.work("cinema")

# what a location scout looks for, by entity class
GROUPS = [
    ("Valve's cameras", ("point_camera", "point_camera_vertical_fov")),
    ("Team spawns", ("info_team_spawn",)),
    ("Patrons and guardians", ("npc_barrack_boss", "npc_boss_tier2", "npc_boss_tier3", "npc_base_defense_sentry")),
    ("Shops", ("trigger_item_shop", "citadel_shop_prop_dynamic")),
    ("Neutral camps", ("info_neutral_trooper_camp",)),
    ("Ziplines", ("citadel_zipline_path",)),
    ("Teleporters and launch pads", ("citadel_trigger_teleport", "trigger_catapult")),
    ("Buildings that fall", ("destroyable_building",)),
    ("Districts", ("citadel_trigger_in_map_district",)),
    ("Map markers", ("info_mini_map_marker", "trigger_ping_location")),
]


class StageError(RuntimeError):
    pass


def lump_text(name):
    out = os.path.join(WORK, "maps", f"{name}_ents.txt")
    vpk = os.path.join(MAPS, f"{name}.vpk")
    if not os.path.exists(vpk):
        raise StageError(f"no map {vpk}")
    if os.path.exists(out) and os.path.getmtime(out) >= os.path.getmtime(vpk) and os.path.getsize(out) > 1000:
        return out
    os.makedirs(os.path.dirname(out), exist_ok=True)
    r = subprocess.run([MAGICIAN, "dump", vpk, f"maps/{name}/entities/default_ents.vents_c"], capture_output=True)
    if r.returncode != 0 or len(r.stdout) < 1000:
        raise StageError(f"magician dump failed for {name}: {r.stderr.decode(errors='replace')[-400:]}")
    with open(out, "wb") as f:
        f.write(r.stdout)
    return out


VEC = re.compile(r"\[\s*(-?[\d.e+-]+),\s*(-?[\d.e+-]+),\s*(-?[\d.e+-]+)\s*\]")


def entities(name):
    """Every entity's top-level keyvalues (strings and vectors) as dicts."""
    ents, cur, depth = [], None, 0
    for line in open(lump_text(name), encoding="utf-8", errors="replace"):
        s = line.strip()
        if cur is None:
            if s == "values =":
                cur, depth = {}, 0
            continue
        if s == "{":
            depth += 1
            continue
        if s == "}":
            depth -= 1
            if depth == 0:
                ents.append(cur)
                cur = None
            continue
        if depth == 1 and " = " in s:
            k, v = s.split(" = ", 1)
            m = VEC.fullmatch(v)
            if m:
                cur[k] = [float(x) for x in m.groups()]
            elif v.startswith('"') and v.endswith('"'):
                cur[k] = v[1:-1]
            else:
                cur[k] = v
    return ents


def landmarks(name):
    ents = entities(name)
    out = []
    for title, classes in GROUPS:
        found = [e for e in ents if e.get("classname") in classes and isinstance(e.get("origin"), list)]
        found.sort(key=lambda e: (e.get("classname"), e.get("targetname", ""), e["origin"]))
        out.append((title, found))
    return out


def spec_line(e):
    o = e["origin"]
    a = e.get("angles", [0, 0, 0])
    return f"spec_goto {o[0]:.0f} {o[1]:.0f} {o[2]:.0f} {a[0]:.1f} {a[1]:.1f}"


def main(a):
    if len(a) < 2 or a[0] not in ("list", "camera", "md"):
        sys.exit(__doc__)
    try:
        name = a[1]
        if a[0] == "camera":
            for e in entities(name):
                if e.get("classname", "").startswith("point_camera") and e.get("targetname", "").endswith(a[2]):
                    print(spec_line(e), f"// fov {e.get('FOV', '?')}")
                    return
            raise StageError(f"no point_camera named {a[2]} in {name}")
        groups = landmarks(name)
        only = a[a.index("--class") + 1] if "--class" in a else None
        near = None
        if "--near" in a:
            i = a.index("--near")
            near = (float(a[i + 1]), float(a[i + 2]), float(a[i + 3]) if len(a) > i + 3 and not a[i + 3].startswith("--") else 2000)
        if a[0] == "md":
            print(f"| {name} | entity | name | origin (x, y, z) | angles |")
            print("|---|---|---|---|---|")
        for title, ents in groups:
            ents = [e for e in ents if (not only or e["classname"] == only)
                    and (not near or ((e["origin"][0] - near[0]) ** 2 + (e["origin"][1] - near[1]) ** 2) ** 0.5 <= near[2])]
            if not ents:
                continue
            if a[0] == "list":
                print(f"== {title} ({len(ents)})")
            for e in ents:
                o = e["origin"]
                ang = e.get("angles", [0, 0, 0])
                tn = e.get("targetname", "")
                if a[0] == "md":
                    print(f"| {title} | `{e['classname']}` | {tn} | {o[0]:.0f}, {o[1]:.0f}, {o[2]:.0f} | {ang[0]:.0f}, {ang[1]:.0f} |")
                else:
                    print(f"  {e['classname']:<28} {tn[:34]:<34} ({o[0]:7.0f} {o[1]:7.0f} {o[2]:6.0f})"
                          + (f"  {spec_line(e)}" if e["classname"].startswith("point_camera") else ""))
    except StageError as e:
        sys.exit(f"stage: {e}")


if __name__ == "__main__":
    main(sys.argv[1:])

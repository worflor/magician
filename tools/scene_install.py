#!/usr/bin/env python3
"""Install (or remove) a hero-page scene from a scene patch. Python 3.8+ standard library only; Linux, Steam Deck and
Windows.

    python3 install-scene.py [--game DIR] [--patch DIR] [--uninstall] [--force]

The twin of install-scene.ps1 (tools/scene_install.ps1) for players without Windows PowerShell: same algorithm, same
checks, same bytes as `magician scene install`. A scene patch is a folder with scene-patch.txt and files/<path>: only
the mod author's own files. The rest of the hero-page map comes from YOUR copy of Deadlock. This script:

  1. finds your game (or takes --game),
  2. reads your stock maps/ui/hero_prefabs/<hero>.vpk,
  3. checks every Valve file the patch keeps or replaces against the checksum the author saw (a game update that
     changed the map stops here, and nothing is written),
  4. builds the scene VPK (Valve's files from your game plus the author's files) and writes it to
     citadel/addons/maps/ui/hero_prefabs/<hero>.vpk,
  5. checks the result is byte-for-byte the scene the author tested (the patch's `result` checksum).

--patch   one scene-patch folder, or a folder of them. Default: `scene` beside this script.
--game    the Deadlock `game` folder (the one holding `citadel`). Default: $DEADLOCK_GAME, then Steam's library list.
          A folder given here or in $DEADLOCK_GAME that is not Deadlock's is an error, never a reason to look elsewhere.
--uninstall  remove the scene(s); only a file whose checksum is the patch's result (or, for a patch without one, the
             scene the patch builds from your game) is removed (--force overrides).
--force   replace or remove a scene file that is not this patch's result.
"""
import os, re, struct, sys, tempfile, zlib

SIGNATURE, INLINE, TERMINATOR = 0x55AA1234, 0x7FFF, 0xFFFF


class Refused(Exception):
    pass


def crc(b):
    return "%08x" % (zlib.crc32(b) & 0xFFFFFFFF)


def read_vpk(path):
    """Every file of a single-file VPK (v1 or v2), keyed by its path."""
    data = open(path, "rb").read()
    sig, version, tree_size = struct.unpack_from("<III", data, 0)
    if sig != SIGNATURE:
        raise Refused(f"{path} is not a VPK")
    header = {2: 28, 1: 12}.get(version)
    if header is None:
        raise Refused(f"{path}: VPK version {version} is not supported")
    pos, data_start, files = header, header + tree_size, {}

    def cstr():
        nonlocal pos
        end = data.index(b"\0", pos)
        s = data[pos:end].decode("utf-8")
        pos = end + 1
        return s

    while True:
        ext = cstr()
        if not ext:
            break
        while True:
            d = cstr()
            if not d:
                break
            while True:
                name = cstr()
                if not name:
                    break
                _crc, pre, archive, offset, length, term = struct.unpack_from("<IHHIIH", data, pos)
                pos += 18
                if term != TERMINATOR:
                    raise Refused(f"{path}: damaged directory")
                preload = data[pos:pos + pre]
                pos += pre
                if archive != INLINE and length > 0:
                    raise Refused(f"{path}: data in a separate archive is not supported")
                body = data[data_start + offset:data_start + offset + length] if length else b""
                p = ("" if d == " " else d + "/") + name + ("" if ext == " " else "." + ext)
                files[p] = preload + body
    return files


def split(path):
    d, _, f = path.rpartition("/")
    name, dot, ext = f.rpartition(".")
    if not dot:
        name, ext = f, ""
    return d or " ", name or " ", ext or " "


def write_vpk(out, files):
    """A v2 single-file VPK in Magician's deterministic layout (ext, dir, name in ordinal order; no preload)."""
    tree = {}
    for p, b in files.items():
        d, name, ext = split(p)
        tree.setdefault(ext, {}).setdefault(d, {})[name] = b
    t, blob = bytearray(), bytearray()
    for ext in sorted(tree):
        t += ext.encode() + b"\0"
        for d in sorted(tree[ext]):
            t += d.encode() + b"\0"
            for name in sorted(tree[ext][d]):
                b = tree[ext][d][name]
                t += name.encode() + b"\0"
                t += struct.pack("<IHHIIH", zlib.crc32(b) & 0xFFFFFFFF, 0, INLINE, len(blob), len(b), TERMINATOR)
                blob += b
            t += b"\0"
        t += b"\0"
    t += b"\0"
    with open(out, "wb") as f:
        f.write(struct.pack("<IIIIIII", SIGNATURE, 2, len(t), len(blob), 0, 0, 0))
        f.write(t)
        f.write(blob)


def steam_roots():
    roots = []
    if os.name == "nt":
        try:
            import winreg
            for hive, key in ((winreg.HKEY_CURRENT_USER, r"Software\Valve\Steam"),
                              (winreg.HKEY_LOCAL_MACHINE, r"SOFTWARE\WOW6432Node\Valve\Steam")):
                for value in ("SteamPath", "InstallPath"):
                    try:
                        with winreg.OpenKey(hive, key) as k:
                            roots.append(winreg.QueryValueEx(k, value)[0])
                    except OSError:
                        pass
        except ImportError:
            pass
    else:
        home = os.path.expanduser("~")
        for rel in (".steam/steam", ".steam/root", ".local/share/Steam",
                    ".var/app/com.valvesoftware.Steam/.local/share/Steam", "snap/steam/common/.local/share/Steam"):
            roots.append(os.path.join(home, rel))
    return roots


def find_game(given):
    if given:
        # the player named this folder: install there or nowhere, never into another game Steam knows of
        if os.path.isfile(os.path.join(given, "citadel", "gameinfo.gi")):
            return os.path.realpath(given)
        raise Refused(f"{given} is not Deadlock's `game` folder (no citadel/gameinfo.gi there). Point --game (or "
                      "DEADLOCK_GAME) at <...>/steamapps/common/Deadlock/game.")
    cands = []
    for root in steam_roots():
        libs = [root]
        vdf = os.path.join(root, "steamapps", "libraryfolders.vdf")
        if os.path.isfile(vdf):
            text = open(vdf, encoding="utf-8", errors="replace").read()
            libs += [m.replace("\\\\", "\\") for m in re.findall(r'"path"\s+"([^"]+)"', text)]
        cands += [os.path.join(lib, "steamapps", "common", "Deadlock", "game") for lib in libs]
    for c in cands:
        if c and os.path.isfile(os.path.join(c, "citadel", "gameinfo.gi")):
            return os.path.realpath(c)
    raise Refused('Deadlock not found. Run again with --game "<...>/steamapps/common/Deadlock/game" '
                  "(the folder that holds citadel).")


# Each line of scene-patch.txt and its field count (the name included); read as strictly as `magician scene` reads it.
FIELDS = {"map": 2, "mod": 2, "built_for": 2, "result": 2, "keep": 4, "replace": 5, "add": 4}


def read_patch(d):
    txt = os.path.join(d, "scene-patch.txt")
    p = {"dir": d, "keep": [], "replace": [], "add": [], "map": None, "mod": None, "build": None, "result": None}
    # a BOM (an editor's) is not part of the text
    lines = [(n, l.rstrip("\r\n")) for n, l in enumerate(open(txt, encoding="utf-8-sig"), 1)]
    lines = [(n, l) for n, l in lines if not re.match(r"^\s*(#|$)", l)]
    head = lines[0][1] if lines else ""
    if head != "magician-scene-patch\t1":
        if head.startswith("magician-scene-patch\t"):
            raise Refused(f"{txt}: patch format {head.split(chr(9))[1]} is newer than this script")
        raise Refused(f"{txt}: not a scene patch (it must start with magician-scene-patch<TAB>1)")

    def hex8(v, n):
        if not re.fullmatch(r"[0-9a-fA-F]{8}", v):
            raise Refused(f"{txt} line {n}: {v!r} is not a crc (8 hex digits)")
        return v.lower()

    def size(v, n):
        if not re.fullmatch(r"[0-9]{1,19}", v):
            raise Refused(f"{txt} line {n}: size {v!r} is not a number")
        return int(v)

    seen = set()
    for n, line in lines[1:]:
        f = line.split("\t")
        want = FIELDS.get(f[0])
        if want is None:
            raise Refused(f"{txt} line {n}: unknown field {f[0]!r} (fields are tab-separated)")
        if len(f) != want:
            raise Refused(f"{txt} line {n}: {f[0]!r} takes {want - 1} field(s), found {len(f) - 1}")
        if f[0] in ("map", "mod"):
            p[f[0]] = f[1]
        elif f[0] == "result":
            p["result"] = hex8(f[1], n)
        elif f[0] == "built_for":
            p["build"] = f[1]
        else:
            if f[1] in seen:
                raise Refused(f"{txt} line {n}: {f[1]} is listed twice")
            seen.add(f[1])
            if f[0] == "keep":
                p["keep"].append({"path": f[1], "valve": hex8(f[2], n), "size": size(f[3], n)})
            elif f[0] == "replace":
                p["replace"].append({"path": f[1], "valve": hex8(f[2], n), "ours": hex8(f[3], n), "size": size(f[4], n)})
            else:
                p["add"].append({"path": f[1], "ours": hex8(f[2], n), "size": size(f[3], n)})
    if not p["map"] or not re.match(r"^maps/ui/hero_prefabs/[a-z0-9_]+\.vpk$", p["map"]):
        raise Refused(f"{txt}: unexpected map '{p['map']}'")
    if not p["mod"]:
        raise Refused(f"{txt}: the patch names no mod")
    hero = p["map"].rsplit("/", 1)[1][:-4]
    for e in p["keep"] + p["replace"] + p["add"]:
        if not re.match(rf"^maps/ui/hero_prefabs/{hero}(\.vmap_c|/[a-z0-9_./-]+)$", e["path"]) or any(
                seg in ("", ".", "..") for seg in e["path"].split("/")):
            raise Refused(f"{txt}: refusing path '{e['path']}'")
    if f"maps/ui/hero_prefabs/{hero}.vmap_c" not in seen:
        raise Refused(f"{txt}: the patch has no maps/ui/hero_prefabs/{hero}.vmap_c; not a scene for {hero}")
    return p


def native(base, *rel):
    """`base` (a real folder) joined with patch paths, which always use `/`."""
    return os.path.join(base, *[q for part in rel for q in part.split("/")])


def scene_files(p, game):
    """Every file of the scene, from the player's stock map and the patch, after checking each against the patch.
    Raises Refused, listing what differs, when the map is not the one the patch was made against."""
    stock = native(os.path.join(game, "citadel"), p["map"])
    if not os.path.isfile(stock):
        raise Refused(f"Your game has no {p['map']}. Is this the right Deadlock folder?")
    valve = read_vpk(stock)
    problems, out = [], {}
    for e in p["keep"] + p["replace"]:
        if e["path"] not in valve:
            problems.append(f"missing in your game: {e['path']}")
            continue
        have = crc(valve[e["path"]])
        if have != e["valve"]:
            problems.append(f"changed in your game: {e['path']} ({have}, the patch expects {e['valve']})")
        elif e in p["keep"] and len(valve[e["path"]]) != e["size"]:
            problems.append(f"changed in your game: {e['path']} (size {len(valve[e['path']])}, the patch expects {e['size']})")
    for e in p["add"]:
        if e["path"] in valve:
            problems.append(f"new in your game: {e['path']} (the patch adds it; Valve's map now has a file there)")
    for e in p["keep"]:
        if e["path"] in valve:
            out[e["path"]] = valve[e["path"]]
    for e in p["replace"] + p["add"]:
        f = native(p["dir"], "files", e["path"])
        if not os.path.isfile(f):
            problems.append(f"missing from the patch: {e['path']}")
            continue
        b = open(f, "rb").read()
        if crc(b) != e["ours"] or len(b) != e["size"]:
            problems.append(f"damaged in the patch: {e['path']}")
            continue
        out[e["path"]] = b
    if problems:
        for pr in problems:
            print("  " + pr)
        raise Refused(f"This patch was made for build {p['build']} and your game's {p['map']} no longer matches it. "
                      "Nothing was written. A game update changed Valve's map, so this patch needs a new release "
                      "from its author.")
    return out


def install_one(p, game, force):
    citadel = os.path.join(game, "citadel")
    target = native(citadel, "addons", p["map"])
    out = scene_files(p, game)
    os.makedirs(os.path.dirname(target), exist_ok=True)
    fd, tmp = tempfile.mkstemp(suffix=".tmp", dir=os.path.dirname(target))
    os.close(fd)
    try:
        write_vpk(tmp, out)
        built = crc(open(tmp, "rb").read())
        if p["result"] and built != p["result"]:
            raise Refused(f"The built scene ({built}) is not the one the author tested ({p['result']}). "
                          "Nothing was written.")
        if os.path.exists(target):
            have = crc(open(target, "rb").read())
            if have == built:
                print(f"  already installed ({built})")
                return
            if not force:
                raise Refused(f"{target} already exists and is not this scene ({have}). "
                              "Run with --force to replace it.")
        os.replace(tmp, target)
    finally:
        if os.path.exists(tmp):
            os.remove(tmp)
    print(f"  installed {p['map']} ({built}) for {p['mod']}")
    gi = open(os.path.join(citadel, "gameinfo.gi"), encoding="utf-8", errors="replace").read()
    if "citadel/addons" not in gi:
        print("  note: gameinfo.gi has no citadel/addons search path, so the game will not load addons yet. "
              "A mod manager (or installing the skin) adds it.")


def uninstall_one(p, game, force):
    target = native(game, "citadel", "addons", p["map"])
    if not os.path.isfile(target):
        print("  not installed")
        return
    have = crc(open(target, "rb").read())
    expected = p["result"]
    if not expected and not force:
        # No result line: the scene this patch makes is its own answer. Build it (in a temporary file) and compare.
        try:
            out = scene_files(p, game)
        except Refused:
            raise Refused(f"This patch records no result checksum and your game's {p['map']} no longer matches it, so "
                          f"there is no telling whether {target} is this patch's scene. Run with --force to remove it "
                          "anyway.")
        fd, tmp = tempfile.mkstemp(suffix=".vpk")
        os.close(fd)
        try:
            write_vpk(tmp, out)
            expected = crc(open(tmp, "rb").read())
        finally:
            os.remove(tmp)
    if expected and have != expected and not force:
        raise Refused(f"{target} is not this patch's scene ({have}, expected {expected}). "
                      "Run with --force to remove it anyway.")
    os.remove(target)
    print(f"  removed {p['map']}")


def main(argv):
    args = {"--patch": os.path.join(os.path.dirname(os.path.abspath(__file__)), "scene"),
            "--game": os.environ.get("DEADLOCK_GAME")}
    flags, it = set(), iter(argv)
    for a in it:
        if a in ("-h", "--help"):
            print(__doc__)
            return 0
        if a in ("--patch", "--game"):
            args[a] = next(it, None)
            if args[a] is None or args[a].startswith("--"):
                print(f"{a} needs a folder; see --help")
                return 2
        elif a in ("--uninstall", "--force"):
            flags.add(a)
        else:
            print(f"unknown argument {a}; see --help")
            return 2
    patch = args["--patch"]
    if os.path.isfile(os.path.join(patch, "scene-patch.txt")):
        patches = [patch]
    elif os.path.isdir(patch):
        patches = sorted(os.path.join(patch, d) for d in os.listdir(patch)
                         if os.path.isfile(os.path.join(patch, d, "scene-patch.txt")))
    else:
        patches = []
    if not patches:
        print(f"No scene patch found in {patch}")
        return 2
    try:
        game = find_game(args["--game"])
    except Refused as e:
        print(e)
        return 2
    print(f"Deadlock: {game}")
    failed = False
    for d in patches:
        print(f"\n== {os.path.basename(d)}")
        try:
            p = read_patch(d)
            (uninstall_one if "--uninstall" in flags else install_one)(p, game, "--force" in flags)
        except (Refused, OSError, ValueError, struct.error) as e:
            # a damaged patch or map (a truncated VPK, bytes that are not text) is a message, not a traceback
            print(f"  {e}")
            failed = True
    print()
    if failed:
        print("Something above failed. Nothing was left half-done.")
        return 1
    print("Done.")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))

"""Assemble a release bundle for mod managers from a bundle.json spec. Python stdlib plus Pillow (previews only).

    python bundle.py <bundle.json> [--out DIR] [--zip] [--magician EXE] [--no-valve-check]

The bundle is a folder a player can import into Deadlock Mod Manager (it reads mod-interchange.json,
https://docs.deadlockmods.app/developer-docs/mod-interchange) or copy by hand:

    mod-interchange.json    one mod per pak, load order 0..n (DMM's interchange format)
    mod.config.json         the same metadata in DMM's draft package format (rfcs/000-mod-packaging), for tools that
                            read it; layers name the paks under files/
    files/NNNN-<slug>/pakNN_dir.vpk
    previews/*.png|jpg
    README.md  README.txt (the same text without Markdown marks)  CHANGELOG.md  SHA256SUMS.txt

Spec keys: name, slug, author, hero, category, description, version, game_build, out, readme (copied as README.md),
mods [{slug, name, description, order, files [{name, src}], gamebanana_id, optional, category, hero}],
previews [{src, out, crop [l,t,r,b]}], allowed_overlap [paths that may live in two paks; the lower-order mod wins],
warnings [strings], scenes [{src, hero}], bundle_magician (true copies the magician binary into tools/).
A mod's own `category` or `hero` overrides the spec's: give an add-on pak (a voice pak beside a skin) its own category,
e.g. "Voice Lines", or Deadlock Mod Manager sees two skins for one hero and offers to swap one out.
Community metadata (all optional): release (the public semver, e.g. "1.0.0"; `version` stays the internal build),
date (YYYY-MM-DD, also the zip's file times), nsfw (true/false: say it explicitly, DMM treats a missing value as
unknown and hides it with adult content), hero_id and content_type (DMM's taxonomy ids,
https://deadlockmods.app/v1/taxonomy.json, e.g. "lash" and "skins"), tags [strings], license (SPDX id), link (the
mod's page), thumbnail_url (an image URL), gamebanana_id (the GameBanana submission id: the mod's key becomes
gamebanana:mod:<id>, so DMM links it to its page and can offer updates; one id per mod, a spec-level id only for a
one-mod bundle), changelog (a file copied as CHANGELOG.md), zip (the zip's base name for --zip).
Relative paths resolve against the spec's folder; `~/` means the folder that holds this repository.

Scenes: `src` is a scene-patch folder (`magician scene make`: scene-patch.txt and files/, only our bytes). It goes to
`scene/<hero>/`, beside two installers that build the hero-page scene from the player's own Valve files, byte-identical
to `magician scene install`, so players never need Magician: `install-scene.ps1` (tools/scene_install.ps1, Windows
PowerShell, run by install-scene.cmd / uninstall-scene.cmd) and `install-scene.py` (tools/scene_install.py, Python 3
stdlib, for Linux and the Steam Deck, run by install-scene.sh / uninstall-scene.sh). The bundler checks that no file
in the patch is byte-identical to Valve's file at that path and that the patch fits the installed game.

--zip writes <zip>.zip beside the bundle folder: one top-level folder named like the zip, files in sorted order with
the spec's date, the .sh wrappers marked executable, so the same inputs give the same zip.

Checks before it writes anything: the spec has every key it needs, each name is one plain file or folder name,
and every source exists; no path lives in two paks unless listed in `allowed_overlap`; no compiled file is
byte-identical to Valve's file at the same path (crc against pak01_dir.vpk), so a Valve file can never ride along.
Only compiled mod paks are bundled. The bundle is built beside `out` and swapped in when it is complete, and an
existing `out` is replaced only when it is a bundle (it holds mod-interchange.json) or empty.
"""
import datetime, hashlib, json, os, re, shutil, subprocess, sys, zipfile, zlib

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "ingame"))
import gamepaths  # noqa: E402  (the game as magician finds it)

GAME_PAK = os.path.join(gamepaths.try_game(), "citadel", "pak01_dir.vpk")
TOOLS = os.path.dirname(os.path.abspath(__file__))


def magician_version():
    m = re.search(r'^version\s*=\s*"([^"]+)"', open(os.path.join(TOOLS, "..", "Cargo.toml"), encoding="utf-8").read(), re.M)
    return m.group(1) if m else "unknown"


def plain_text(md):
    """README.md as plain text for README.txt: no **, backticks or # marks; headings underlined."""
    out = []
    for line in md.splitlines():
        h = re.match(r"^(#{1,6})\s+(.*)$", line)
        line = re.sub(r"\*\*(.+?)\*\*", r"\1", h.group(2) if h else line).replace("`", "")
        line = re.sub(r"\[([^\]]+)\]\(([^)]+)\)", r"\1 (\2)", line)
        out.append(line)
        if h:
            out.append(("=" if len(h.group(1)) == 1 else "-") * len(line))
    return "\r\n".join(out) + "\r\n"


def resolve(base, p):
    if p.startswith("~/"):
        return os.path.join(ROOT, p[2:])
    return p if os.path.isabs(p) else os.path.join(base, p)


SCENE_PS1 = os.path.join(TOOLS, "scene_install.ps1")
SCENE_PY = os.path.join(TOOLS, "scene_install.py")
SH = """#!/bin/sh
# __VERB__ the hero-page scene(s) from your own copy of Deadlock (Linux, Steam Deck). Needs python3.
# Extra arguments go to install-scene.py, for example: --game "$HOME/.local/share/Steam/steamapps/common/Deadlock/game"
cd "$(dirname "$0")" || exit 1
exec python3 install-scene.py __ARGS__"$@"
"""
CMD = r"""@echo off
rem __VERB__ the hero-page scene(s) from your own copy of Deadlock. Needs only Windows PowerShell.
rem Extra arguments go to install-scene.ps1, for example: -Game "D:\SteamLibrary\steamapps\common\Deadlock\game"
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0install-scene.ps1" __ARGS__%*
set "RC=%ERRORLEVEL%"
pause
exit /b %RC%
"""


def read_manifest(patch):
    """A scene-patch manifest's `map` and `mod` and its file rows (tab-separated; see crates/magician/src/scene.rs)."""
    m = {"keep": [], "replace": [], "add": []}
    for line in open(os.path.join(patch, "scene-patch.txt"), encoding="utf-8"):
        f = line.rstrip("\r\n").split("\t")
        if f[0] in m:
            m[f[0]].append(f[1:])
        elif f[0] in ("map", "mod"):
            m[f[0]] = f[1]
    return m


def listing(exe, vpk):
    out = subprocess.run([exe, "ls", vpk], capture_output=True, text=True, check=True).stdout
    return {l.split("\t")[0]: l.split("\t")[1] for l in out.splitlines() if "\t" in l}


def sha256(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for b in iter(lambda: f.read(1 << 20), b""):
            h.update(b)
    return h.hexdigest()


def plain_name(v):
    """One file or folder name: what the spec's names become inside the bundle, so never a path out of it."""
    return isinstance(v, str) and v not in ("", ".", "..") and not re.search(r'[/\\:\x00-\x1f]', v)


def check_spec(spec, base, args):
    """Everything the bundle will read, before anything is written: a spec that fails later would leave a half-built
    bundle where the last good one was."""
    if not isinstance(spec, dict):
        sys.exit("the spec is not a JSON object")
    for k in ("name", "slug", "author", "hero", "description", "version"):
        if not isinstance(spec.get(k), (str, int)) or spec.get(k) == "":
            sys.exit(f"the spec needs {k}")
    if not plain_name(spec["slug"]):
        sys.exit(f"slug {spec['slug']!r} must be a plain name")
    if not isinstance(spec.get("mods"), list) or not spec["mods"]:
        sys.exit("the spec needs mods: [...]")
    for m in spec["mods"]:
        if not isinstance(m, dict) or not all(k in m for k in ("slug", "name", "description", "order", "files")):
            sys.exit(f"each mod needs slug, name, description, order and files: {m!r}")
        if not plain_name(m["slug"]) or not isinstance(m["order"], int) or not isinstance(m["files"], list) or not m["files"]:
            sys.exit(f"mod {m.get('slug')!r}: a plain slug, an integer order and a list of files")
        names = [f.get("name") for f in m["files"] if isinstance(f, dict)]
        if len(names) != len(m["files"]) or not all(plain_name(n) for n in names) or len(set(names)) != len(names) or not all(isinstance(f.get("src"), str) for f in m["files"]):
            sys.exit(f"mod {m['slug']}: every file needs a plain, unique name and a src")
    for sc in spec.get("scenes", []):
        if not isinstance(sc, dict) or not isinstance(sc.get("src"), str) or not re.fullmatch(r"[a-z0-9_]+", str(sc.get("hero", ""))):
            sys.exit(f"each scene needs a src and a hero (lowercase letters, digits, underscores): {sc!r}")
    for pv in spec.get("previews", []):
        if not isinstance(pv, dict) or not plain_name(pv.get("out")) or not os.path.isfile(resolve(base, str(pv.get("src", "")))):
            sys.exit(f"each preview needs a src that exists and a plain out name: {pv!r}")
    for k in ("readme", "changelog"):
        if spec.get(k) and not os.path.isfile(resolve(base, spec[k])):
            sys.exit(f"{k} {resolve(base, spec[k])} does not exist")
    if spec.get("zip") is not None and not plain_name(spec["zip"]):
        sys.exit(f"zip {spec['zip']!r} must be a plain name")
    if spec.get("date") is not None:
        try:
            datetime.date.fromisoformat(spec["date"])
        except (TypeError, ValueError):
            sys.exit(f"date {spec['date']!r} is not YYYY-MM-DD")
    if "out" not in spec and "--out" not in args:
        sys.exit("the spec has no out and no --out was given")


def main():
    args = sys.argv[1:]
    if args[:1] in (["--help"], ["-h"]):
        print(__doc__)
        return
    if not args:
        sys.exit(__doc__)
    spec_path = os.path.abspath(args[0])
    base = os.path.dirname(spec_path)
    try:
        spec = json.load(open(spec_path, encoding="utf-8-sig"))
    except (OSError, ValueError) as e:
        sys.exit(f"{spec_path}: {e}")

    def flag(k):
        if k not in args:
            return None
        i = args.index(k)
        if i + 1 >= len(args) or args[i + 1].startswith("--"):
            sys.exit(f"{k} needs a value")
        return args[i + 1]

    check_spec(spec, base, args)
    exe = flag("--magician") or gamepaths.magician()
    out = os.path.abspath(resolve(base, flag("--out") or spec["out"]))
    if spec.get("bundle_magician") and not os.path.isfile(exe):
        sys.exit(f"bundle_magician: no {exe}")
    # `out` is replaced at the end: never a folder that is not a bundle, and never one that holds the spec
    if os.path.isdir(out) and os.listdir(out) and not os.path.isfile(os.path.join(out, "mod-interchange.json")):
        sys.exit(f"refusing to replace {out}: it is not empty and not a bundle (no mod-interchange.json)")
    if os.path.normcase(base + os.sep).startswith(os.path.normcase(out + os.sep)):
        sys.exit(f"refusing to build into {out}: it holds the spec")

    lists = {}
    for m in spec["mods"]:
        for f in m["files"]:
            f["_src"] = resolve(base, f["src"])
            if not os.path.isfile(f["_src"]):
                sys.exit(f"missing {f['_src']}")
            lists[(m["slug"], f["name"])] = listing(exe, f["_src"])
    owner, allowed = {}, set(spec.get("allowed_overlap", []))
    for (slug, name), files in lists.items():
        for p in files:
            if p in owner and p not in allowed:
                sys.exit(f"{p} is in both {owner[p]} and {slug}/{name}")
            owner.setdefault(p, f"{slug}/{name}")
    if "--no-valve-check" not in args:
        valve = listing(exe, GAME_PAK)
        for (slug, name), files in lists.items():
            same = [p for p, c in files.items() if valve.get(p) == c]
            if same:
                sys.exit(f"{slug}/{name} carries {len(same)} file(s) byte-identical to Valve's, e.g. {same[0]}")
        print("valve check: no file is byte-identical to the game's")
    for pth in sorted(allowed):
        print("overlap allowed (lower order wins):", pth)

    scenes = []
    for sc in spec.get("scenes", []):
        patch = resolve(base, sc["src"])
        if not os.path.isfile(os.path.join(patch, "scene-patch.txt")):
            sys.exit(f"{patch} is not a scene patch (no scene-patch.txt)")
        man = read_manifest(patch)
        if man.get("map") != f"maps/ui/hero_prefabs/{sc['hero']}.vpk":
            sys.exit(f"{patch} is a patch for {man.get('map')}, not for hero {sc['hero']}")
        ours = {r[0]: int(r[2], 16) for r in man["replace"]}
        ours.update({r[0]: int(r[1], 16) for r in man["add"]})
        root = os.path.join(patch, "files")
        on_disk = {os.path.relpath(os.path.join(dp, f), root).replace("\\", "/"): os.path.join(dp, f) for dp, _, fs in os.walk(root) for f in fs}
        if set(on_disk) != set(ours):
            sys.exit(f"{patch}: files/ and the manifest disagree: {sorted(set(on_disk) ^ set(ours))}")
        for rel, path in on_disk.items():
            if zlib.crc32(open(path, "rb").read()) & 0xFFFFFFFF != ours[rel]:
                sys.exit(f"{patch}: {rel} does not match its crc in the manifest")
        if "--no-valve-check" not in args:
            valve = listing(exe, os.path.join(os.path.dirname(GAME_PAK), *man["map"].split("/")))
            same = [rel for rel in ours if valve.get(rel) == "%08x" % ours[rel]]
            if same:
                sys.exit(f"scene patch {sc['hero']} carries {len(same)} file(s) byte-identical to Valve's, e.g. {same[0]}")
            chk = subprocess.run([exe, "scene", "check", patch], capture_output=True, text=True)
            if chk.returncode:
                sys.exit(f"scene patch {sc['hero']} does not fit the installed game:\n{chk.stderr}{chk.stdout}")
            print(f"scene {sc['hero']}: {len(ours)} file(s) of ours, none Valve's; {chk.stdout.strip()}")
        scenes.append((sc, patch, man))

    gb_ids = [m.get("gamebanana_id") for m in spec["mods"] if m.get("gamebanana_id")]
    if spec.get("gamebanana_id"):
        if len(spec["mods"]) != 1:
            sys.exit("a spec-level gamebanana_id needs a one-mod bundle; give each mod its own gamebanana_id")
        spec["mods"][0].setdefault("gamebanana_id", spec["gamebanana_id"])
        gb_ids = [spec["gamebanana_id"]]
    if len(set(map(str, gb_ids))) != len(gb_ids):
        sys.exit("two mods share a gamebanana_id; DMM would import only the first")
    if "nsfw" in spec and not isinstance(spec["nsfw"], bool):
        sys.exit("nsfw must be true or false")

    # Built beside `out` and swapped in when complete, so a failure leaves the last good bundle where it was.
    final, out = out, out + ".partial"
    if os.path.isdir(out):
        shutil.rmtree(out)
    os.makedirs(os.path.join(out, "files"))
    mods, sums = [], []
    for i, m in enumerate(sorted(spec["mods"], key=lambda m: m["order"])):
        d = f"files/{i:04d}-{m['slug']}"
        os.makedirs(os.path.join(out, d))
        entries = []
        for f in m["files"]:
            dst = os.path.join(out, d, f["name"])
            shutil.copyfile(f["_src"], dst)
            digest = sha256(dst)
            sums.append(f"{digest}  {d}/{f['name']}")
            entries.append({"name": f["name"], "path": f"{d}/{f['name']}", "sha256": digest, "size": os.path.getsize(dst),
                            "selected": True})
            crc = zlib.crc32(open(dst, "rb").read()) & 0xFFFFFFFF
            print(f"  {d}/{f['name']}  {os.path.getsize(dst) / 1e6:.1f} MB  crc32 {crc:08x}  {len(lists[(m['slug'], f['name'])])} files")
        gb = m.get("gamebanana_id")
        entry = {
            "key": f"gamebanana:mod:{gb}" if gb else f"local:{spec['slug']}-{m['slug']}",
            "name": m["name"],
            "enabled": True,
            "order": m["order"],
            "origin": {"provider": "gamebanana", "submissionType": "mod", "submissionId": str(gb)} if gb
                      else {"provider": "local", "localId": f"{spec['slug']}-{m['slug']}"},
            "author": spec["author"],
            "description": m["description"],
            "category": m.get("category", spec.get("category", "Skins")),
            "hero": m.get("hero", spec["hero"]),
        }
        for key, src in (("thumbnailUrl", "thumbnail_url"), ("link", "link"), ("nsfw", "nsfw")):
            if src in spec:
                entry[key] = spec[src]
        if gb and "link" not in spec:
            entry["link"] = f"https://gamebanana.com/mods/{gb}"
        entry["files"] = entries
        entry["extensions"] = {"magician": {k: v for k, v in (
            ("version", spec["version"]), ("release", spec.get("release")), ("game_build", spec.get("game_build")),
            ("scenes", [sc["hero"] for sc, _, _ in scenes] or None)) if v is not None}}
        mods.append(entry)
    manifest = {
        "format": "deadlock-mod-interchange",
        "version": 1,
        "createdAt": (spec["date"] + "T12:00:00Z") if spec.get("date")
                     else datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "source": {"manager": "magician", "managerVersion": magician_version(), "profileName": spec["name"]},
        "contents": ["mods", "profiles"],
        "mods": mods,
        "profiles": [{
            "key": f"profile:{spec['slug']}", "name": spec["name"], "active": True, "description": spec["description"],
            "mods": [{"modKey": m["key"], "enabled": True, "order": m["order"]} for m in mods],
        }],
        "warnings": spec.get("warnings", []),
    }
    with open(os.path.join(out, "mod-interchange.json"), "w", encoding="utf-8", newline="\n") as f:
        json.dump(manifest, f, indent=2, ensure_ascii=False)
        f.write("\n")

    if scenes:
        for sc, patch, man in scenes:
            dst = os.path.join(out, "scene", sc["hero"])
            shutil.copytree(patch, dst)
            for dp, _, fs in sorted(os.walk(dst)):
                for f in sorted(fs):
                    full = os.path.join(dp, f)
                    sums.append(f"{sha256(full)}  {os.path.relpath(full, out).replace(os.sep, '/')}")
            print("  scene", sc["hero"], "->", os.path.relpath(dst, out), f"(owner {man['mod']})")
        # The installer is a self-contained PowerShell script (tools/scene_install.ps1): no Magician needed.
        shutil.copyfile(SCENE_PS1, os.path.join(out, "install-scene.ps1"))
        sums.append(f"{sha256(os.path.join(out, 'install-scene.ps1'))}  install-scene.ps1")
        for name, verb, extra in (("install-scene.cmd", "Builds and installs", ""), ("uninstall-scene.cmd", "Removes", "-Uninstall ")):
            with open(os.path.join(out, name), "w", newline="") as f:
                f.write(CMD.replace("__VERB__", verb).replace("__ARGS__", extra).replace(chr(13) + chr(10), chr(10)).replace(chr(10), chr(13) + chr(10)))
            sums.append(f"{sha256(os.path.join(out, name))}  {name}")
        # The same installer in Python 3 (stdlib only) for Linux and the Steam Deck, with LF-only shell wrappers.
        shutil.copyfile(SCENE_PY, os.path.join(out, "install-scene.py"))
        sums.append(f"{sha256(os.path.join(out, 'install-scene.py'))}  install-scene.py")
        for name, verb, extra in (("install-scene.sh", "Builds and installs", ""), ("uninstall-scene.sh", "Removes", "--uninstall ")):
            with open(os.path.join(out, name), "w", newline="\n") as f:
                f.write(SH.replace("__VERB__", verb).replace("__ARGS__", extra))
            sums.append(f"{sha256(os.path.join(out, name))}  {name}")
        if spec.get("bundle_magician"):
            os.makedirs(os.path.join(out, "tools"))
            name = os.path.basename(exe)   # magician.exe from Windows, magician from Linux
            shutil.copyfile(exe, os.path.join(out, "tools", name))
            sums.append(f"{sha256(os.path.join(out, 'tools', name))}  tools/{name}")

    if spec.get("previews"):
        from PIL import Image
        os.makedirs(os.path.join(out, "previews"))
        for p in spec["previews"]:
            im = Image.open(resolve(base, p["src"])).convert("RGB")
            if p.get("crop"):
                im = im.crop(tuple(p["crop"]))
            jpg = p["out"].lower().endswith((".jpg", ".jpeg"))
            im.save(os.path.join(out, "previews", p["out"]), optimize=True, **({"quality": 90} if jpg else {}))
            sums.append(f"{sha256(os.path.join(out, 'previews', p['out']))}  previews/{p['out']}")
            print("  preview", p["out"], im.size)
    if spec.get("readme"):
        md = open(resolve(base, spec["readme"]), encoding="utf-8").read()
        with open(os.path.join(out, "README.md"), "w", encoding="utf-8", newline="\n") as f:
            f.write(md)
        # Windows opens .txt on a double-click; .md asks which app to use.
        with open(os.path.join(out, "README.txt"), "w", encoding="utf-8", newline="") as f:
            f.write(plain_text(md))
        sums += [f"{sha256(os.path.join(out, n))}  {n}" for n in ("README.md", "README.txt")]
    if spec.get("changelog"):
        shutil.copyfile(resolve(base, spec["changelog"]), os.path.join(out, "CHANGELOG.md"))
        sums.append(f"{sha256(os.path.join(out, 'CHANGELOG.md'))}  CHANGELOG.md")

    # DMM's draft package manifest (rfcs/000-mod-packaging). Nothing reads it yet; it costs a few hundred bytes and
    # carries the same facts as mod-interchange.json in the shape a future packer expects. Valid for its validator:
    # kebab-case name, at least one author and one layer, description at most 500 characters.
    config = {
        "name": spec["slug"],
        "display_name": spec["name"],
        "version": spec.get("release") or spec["version"],
        "description": spec["description"][:500],
        "game_version": str(spec["game_build"]) if spec.get("game_build") else None,
        "authors": [spec["author"]],
        "license": spec.get("license"),
        "readme": "README.md" if spec.get("readme") else None,
        "homepage": spec.get("link") or (f"https://gamebanana.com/mods/{gb_ids[0]}" if len(gb_ids) == 1 else None),
        "screenshots": [f"previews/{p['out']}" for p in spec.get("previews", [])],
        "layers": [{"name": m["slug"], "priority": 0,
                    "description": f"{m['description']} ({', '.join(e['path'] for e in mm['files'])})",
                    "required": not m.get("optional", False)}
                   for m, mm in zip(sorted(spec["mods"], key=lambda m: m["order"]), mods)],
        "metadata": {k: v for k, v in (("tags", spec.get("tags")), ("category", spec.get("content_type")),
                                        ("nsfw", spec.get("nsfw"))) if v is not None},
    }
    config = {k: v for k, v in config.items() if v not in (None, [], {})}
    with open(os.path.join(out, "mod.config.json"), "w", encoding="utf-8", newline="\n") as f:
        json.dump(config, f, indent=2, ensure_ascii=False)
        f.write("\n")
    sums.append(f"{sha256(os.path.join(out, 'mod.config.json'))}  mod.config.json")
    sums.append(f"{sha256(os.path.join(out, 'mod-interchange.json'))}  mod-interchange.json")

    with open(os.path.join(out, "SHA256SUMS.txt"), "w", newline="\n") as f:
        f.write("\n".join(sorted(sums, key=lambda s: s.split("  ", 1)[1])) + "\n")
    if os.path.isdir(final):
        shutil.rmtree(final)
    os.replace(out, final)
    out = final
    print("bundle ->", out)

    if "--zip" in args:
        name = spec.get("zip") or f"{spec['slug']}-{spec.get('release') or spec['version']}"
        stamp = tuple(map(int, spec["date"].split("-"))) + (12, 0, 0) if spec.get("date") else (1980, 1, 1, 0, 0, 0)
        dst = os.path.join(os.path.dirname(out), name + ".zip")
        with zipfile.ZipFile(dst + ".tmp", "w", zipfile.ZIP_DEFLATED, compresslevel=9) as z:
            for rel in sorted(os.path.relpath(os.path.join(dp, f), out).replace(os.sep, "/")
                              for dp, _, fs in os.walk(out) for f in fs):
                info = zipfile.ZipInfo(f"{name}/{rel}", date_time=stamp)
                info.compress_type = zipfile.ZIP_DEFLATED
                setattr(info, "compress_level" if hasattr(info, "compress_level") else "_compresslevel", 9)  # renamed in 3.13
                info.external_attr = (0o100755 if rel.endswith(".sh") else 0o100644) << 16
                with open(os.path.join(out, rel), "rb") as src, z.open(info, "w") as w:
                    shutil.copyfileobj(src, w, 1 << 20)
        os.replace(dst + ".tmp", dst)
        print(f"zip -> {dst} ({os.path.getsize(dst) / 1e6:.1f} MB)")


if __name__ == "__main__":
    main()

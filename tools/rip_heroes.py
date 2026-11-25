"""Rip Deadlock hero models to Blender-ready .glb (materials and textures embedded) for reference work.

    python -I tools/rip_heroes.py                         every hero with a real model
    python -I tools/rip_heroes.py --hero lash,sinclair    by display name or internal key
    python -I tools/rip_heroes.py --hero lash --animations [--force] [--root DIR] [--game DIR] [--cli PATH]

Reads the live game data, never a hardcoded list: heroes.vdata gives each hero's model path,
citadel_gc_hero_names_english.txt its display name. Output is keyed by game build (steam.inf ClientVersion), so
references from different patches never mix:

    <root>/<build>/heroes/<display-slug>/<model>.glb      model, materials and textures (and clips with --animations)
    <root>/<build>/heroes/INDEX.md                        hero -> internal key -> VPK path -> glb

--root is the reference folder by default, --game the game's citadel folder and --cli the Source 2 Viewer CLI, each
found as the magician CLI finds them (ingame/gamepaths.py). Valve's assets: reference only. Never commit or
redistribute these files. Standard library only; runs wherever Source 2 Viewer's CLI does (Windows, Linux, macOS).
tools/rip-heroes.ps1 is the same with PowerShell's options.
"""
import os
import re
import shutil
import subprocess
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "ingame"))
import gamepaths  # noqa: E402

PLACEHOLDER = "models/heroes_wip/gen_man/gen_man.vmdl"


def slug(s):
    return re.sub(r"[^a-z0-9]+", "-", s.lower()).strip("-")


def hero_models(vdata):
    """hero key -> model path, from a decompiled heroes.vdata."""
    return dict(re.findall(r'(?ms)^\t(hero_[a-z0-9_]+)\s*=\s*\r?\n\t\{.*?m_strModelName\s*=\s*resource_name:"([^"]+)"', vdata))


def hero_names(loc_text):
    """hero key -> display name, from the English hero-names localization file."""
    return dict(re.findall(r'(?m)^\s*"(hero_[a-z0-9_]+):n"\s+"([^"]+)"', loc_text))


def groups(models, names):
    """[(model, keys, display, slug)]: hero keys grouped by model (several share placeholder models like gen_man).
    When several keys share one model, the owner is the key whose name matches the model file (hero_lash ->
    lash.vmdl); the others show in INDEX.md."""
    by_model = {}
    for k, m in models.items():
        by_model.setdefault(m, []).append(k)
    out = []
    for model in sorted(by_model):
        keys = sorted(by_model[model])
        base = os.path.splitext(os.path.basename(model))[0]
        owner = next((k for k in keys if k[len("hero_"):] == base), None)
        named = [k for k in keys if names.get(k)]
        if model == PLACEHOLDER:
            display = "Placeholder Body (gen_man)"
        elif owner and names.get(owner):
            display = names[owner]
        elif named:
            display = " + ".join(dict.fromkeys(names[k] for k in named))
        else:
            display = " + ".join(keys)
        out.append((model, keys, display, "_placeholder-gen-man" if model == PLACEHOLDER else slug(display)))
    return out


def wanted(keys, names, want):
    return any(k in want or k[len("hero_"):] in want or names.get(k, "").lower() in want for k in keys)


def main(argv):
    o = {"hero": None, "animations": False, "force": False, "root": None, "game": None, "cli": None}
    i = 0
    while i < len(argv):
        a = argv[i]
        if a in ("--help", "-h"):
            print(__doc__)
            return 0
        if a in ("--animations", "--force"):
            o[a[2:]] = True
        elif a in ("--hero", "--root", "--game", "--cli") and i + 1 < len(argv):
            o[a[2:]] = argv[i + 1]
            i += 1
        else:
            print(f"rip_heroes: unknown or incomplete argument {a!r}\n\n{__doc__}", file=sys.stderr)
            return 2
        i += 1
    try:
        game = o["game"] or gamepaths.citadel()
    except LookupError as e:
        print(f"rip_heroes: {e}", file=sys.stderr)
        return 2
    root = o["root"] or gamepaths.reference()
    cli = o["cli"] or gamepaths.vrf()
    cli = cli if os.path.isfile(cli) else (shutil.which(cli) or cli)
    vpk = os.path.join(game, "pak01_dir.vpk")
    for p in (vpk, cli):
        if not os.path.isfile(p):
            print(f"rip_heroes: missing: {p}", file=sys.stderr)
            return 2
    with open(os.path.join(game, "steam.inf"), encoding="utf-8", errors="replace") as f:
        build = re.search(r"(?m)^ClientVersion=(\d+)", f.read()).group(1)
    out, cache = os.path.join(root, build, "heroes"), os.path.join(root, build, "decompiled")
    os.makedirs(out, exist_ok=True)
    os.makedirs(cache, exist_ok=True)

    hv = os.path.join(cache, "scripts", "heroes.vdata")
    if not os.path.isfile(hv):
        subprocess.run([cli, "-i", vpk, "-f", "scripts/heroes.vdata_c", "-d", "-o", cache, "--quiet"],
                       stdout=subprocess.DEVNULL, cwd=cache)
    with open(hv, encoding="utf-8", errors="replace") as f:
        models = hero_models(f.read())
    loc = os.path.join(game, "resource", "localization", "citadel_gc_hero_names", "citadel_gc_hero_names_english.txt")
    with open(loc, encoding="utf-8", errors="replace") as f:
        names = hero_names(f.read())
    want = [h.strip().lower() for h in o["hero"].split(",")] if o["hero"] else None

    rows = []
    for model, keys, display, sl in groups(models, names):
        if want and not wanted(keys, names, want):
            continue
        dest = os.path.join(out, sl)
        glb_name = os.path.splitext(os.path.basename(model))[0] + ".glb"
        glb = os.path.join(dest, glb_name)
        if os.path.isfile(glb) and not o["force"]:
            print(f"skip  {display} (exists)")
        else:
            tmp = os.path.join(dest, "_export")
            os.makedirs(dest, exist_ok=True)
            args = [cli, "-i", vpk, "-f", model + "_c", "-o", tmp, "--gltf_export_format", "glb",
                    "--gltf_export_materials", "--gltf_textures_adapt", "--quiet"]
            if o["animations"]:
                args.append("--gltf_export_animations")
            rc = subprocess.run(args, stdout=subprocess.DEVNULL, cwd=dest).returncode
            if rc != 0:
                print(f"FAIL  {display} ({model}) exit {rc}")
                continue
            files = [os.path.join(dp, n) for dp, _, fs in os.walk(tmp) for n in fs] if os.path.isdir(tmp) else []
            # an unreleased hero can name a model this build's VPK doesn't ship: the CLI exits 0 with "no files
            # matched", so the output is what is checked, not the exit code
            if not files:
                shutil.rmtree(dest, ignore_errors=True)
                print(f"absent {display:<27} {model} (named in heroes.vdata, not in this build's VPK)")
                rows.append((display, ", ".join(keys), model, "(not shipped)", "-"))
                continue
            for p in files:
                os.replace(p, os.path.join(dest, os.path.basename(p)))
            shutil.rmtree(tmp, ignore_errors=True)
            print(f"rip   {display:<28} {model}")
        size = f"{os.path.getsize(glb) / (1 << 20):,.0f} MB" if os.path.isfile(glb) else "?"
        rows.append((display, ", ".join(keys), model, f"{sl}/{glb_name}", size))

    if not want:
        md = [f"# Deadlock hero reference models, build {build}", "",
              "Ripped from `pak01_dir.vpk` with Source 2 Viewer. Reference only: never commit or redistribute.", "",
              "| Hero | Internal keys | VPK path | glb | Size |", "|---|---|---|---|---|"]
        md += [f"| {d} | {k} | `{v}` | `{g}` | {s} |" for d, k, v, g, s in sorted(rows)]
        with open(os.path.join(out, "INDEX.md"), "w", encoding="utf-8", newline="\n") as f:
            f.write("\n".join(md) + "\n")
        print(f"index {os.path.join(out, 'INDEX.md')}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))

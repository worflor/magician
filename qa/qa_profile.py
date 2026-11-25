"""QA profile: what the mod costs the GPU against Valve's original, from the compiled files themselves.

  python -I qa_profile.py --hero-cfg <hero.json> --vpk <mod pak> --out <dir>

Model (ours from --vpk, Valve's from the game's pak01), per LOD (the model's m_refLODGroupMasks):
  triangles, vertices, draw calls (one per mesh and material: each is a state change and a submit), materials,
  bones (m_modelSkeleton), LOD switch distances.
Textures: every texture of every material the default material group draws, read from its VTEX header (size,
  format, mip count), so GPU memory is exact for block-compressed formats, not a file-size guess. A texture shared
  by several materials counts once.
Particles: every particle file the mod ships under the hero's prefixes, decompiled, against Valve's file at the
  same path. Per file: max particles, emitters (burst count, rate), renderers, sprite radius, lifetime, children,
  and a fill estimate: sprites alive x sprite area, as a share of a 1080p screen 10 m away (90 degree FOV), i.e.
  "full-screen layers of overdraw at 10 m". Lights and meshes are listed, not filled.
Writes <out>/profile.json and <out>/profile.md.
"""
import argparse, json, os, re, subprocess, sys, tempfile, shutil
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "ingame"))
import gamepaths  # noqa: E402  (locations as magician.exe resolves them)

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.abspath(os.path.join(HERE, "..", ".."))
VRF = gamepaths.vrf()
MAGICIAN = os.path.join(HERE, "..", "target", "release", "magician.exe" if os.name == "nt" else "magician")
GAMEDIR = gamepaths.try_game()
GAME_PAK = os.path.join(GAMEDIR, "citadel", "pak01_dir.vpk")
CORE_PAK = os.path.join(GAMEDIR, "core", "pak01_dir.vpk")
K = 0.0254

ap = argparse.ArgumentParser()
ap.add_argument("--hero-cfg", required=True)
ap.add_argument("--vpk", required=True)
ap.add_argument("--out", required=True)
A = ap.parse_args()
cfg = json.load(open(A.hero_cfg))
os.makedirs(A.out, exist_ok=True)

def vrf(args):
    return subprocess.run([VRF] + args, capture_output=True, text=True, errors="replace").stdout

LISTS = {}
def listing(vpk):
    if vpk not in LISTS:
        out = subprocess.run([MAGICIAN, "ls", vpk], capture_output=True, text=True).stdout
        LISTS[vpk] = {l.split("\t")[0]: int(l.split("\t")[2]) for l in out.splitlines() if l.count("\t") == 2}
    return LISTS[vpk]

def where(path, prefer):
    for v in prefer:
        if path in listing(v):
            return v
    return None

# ------------------------------------------------------------------------------------------------ KV3 text
TOK = re.compile(r'(?:[A-Za-z_0-9]+:)?"(?:[^"\\]|\\.)*"|[{}\[\]=,]|[^\s{}\[\]=,"]+')

def kv3(text):
    text = re.sub(r"<!--.*?-->", "", text, flags=re.S)
    text = re.sub(r"//[^\n]*", "", text)
    t = [m.group(0) for m in TOK.finditer(text)]
    pos = [0]
    def val():
        x = t[pos[0]]; pos[0] += 1
        if x == "{":
            d = {}
            while t[pos[0]] != "}":
                k = t[pos[0]]; pos[0] += 2          # key =
                d[k.strip('"')] = val()
                if t[pos[0]] == ",":
                    pos[0] += 1
            pos[0] += 1
            return d
        if x == "[":
            l = []
            while t[pos[0]] != "]":
                l.append(val())
                if t[pos[0]] == ",":
                    pos[0] += 1
            pos[0] += 1
            return l
        if x.endswith('"'):
            return x[x.index('"') + 1:-1]
        try:
            return float(x)
        except ValueError:
            return x
    return val()

def num(x, default=None):
    if isinstance(x, (int, float)):
        return float(x)
    if isinstance(x, dict):
        for k in ("m_flLiteralValue", "m_nLiteralValue", "m_flValue"):
            if k in x:
                return num(x[k], default)
        if "m_flInput0" in x and "m_flOutput1" in x:
            return num(x.get("m_flOutput1"), default)
    return default

# ------------------------------------------------------------------------------------------------ model
def model_profile(vpk, path):
    data = vrf(["-i", vpk, "-f", path, "-b", "DATA"])
    mdat = vrf(["-i", vpk, "-f", path, "-b", "MDAT"])
    m = re.search(r"m_refLODGroupMasks = \s*\[(.*?)\]", data, re.S)
    masks = [int(x) for x in re.findall(r"\d+", m.group(1))] if m else []
    sw = re.search(r"m_lodGroupSwitchDistances = \[(.*?)\]", data)
    bones_blk = re.search(r"m_boneName = \s*\[(.*?)\]", data, re.S)
    bones = re.findall(r'"([^"]+)"', bones_blk.group(1)) if bones_blk else []
    meshes = []
    for blk in mdat.split('--- Data for block "MDAT" ---')[1:]:
        calls = []
        for c in re.finditer(r"m_flUvDensity = ([\d.eE+-]+).*?m_nVertexCount = (\d+).*?m_nIndexCount = (\d+).*?m_material = resource:\"([^\"]+)\"", blk, re.S):
            calls.append({"uv_density": float(c.group(1)), "verts": int(c.group(2)), "tris": int(c.group(3)) // 3, "material": c.group(4)})
        meshes.append(calls)
    lods = {}
    for i, calls in enumerate(meshes):
        mask = masks[i] if i < len(masks) else 1
        for lod in range(8):
            if mask & (1 << lod):
                L = lods.setdefault(lod, {"tris": 0, "verts": 0, "draw_calls": 0, "materials": set(), "meshes": 0})
                L["meshes"] += 1
                L["draw_calls"] += len(calls)
                for c in calls:
                    L["tris"] += c["tris"]; L["verts"] += c["verts"]; L["materials"].add(c["material"])
    for L in lods.values():
        L["materials"] = sorted(L["materials"])
    return {"bones": len(bones), "lod_switch": sw.group(1).strip() if sw else "", "lods": lods, "meshes": meshes,
            "materials": sorted({c["material"] for calls in meshes for c in calls}), "file_bytes": listing(vpk).get(path, 0)}

BPP = {"DXT1": 0.5, "BC1": 0.5, "ATI1N": 0.5, "BC4": 0.5, "DXT5": 1, "BC3": 1, "ATI2N": 1, "BC5": 1, "BC7": 1, "BC6H": 1,
       "RGBA8888": 4, "BGRA8888": 4, "RGBA16161616F": 8, "RGBA16161616": 8, "R16F": 2, "R16": 2, "RG1616F": 4,
       "I8": 1, "IA88": 2, "R32F": 4, "RGBA32323232F": 16}

def tex_profile(vpk, path):
    d = vrf(["-i", vpk, "-f", path, "-b", "DATA"])
    g = lambda k: (re.search(k + r"\s*=\s*(\S+)", d) or [None, "0"])[1]
    w, h, mips = int(g("Width")), int(g("Height")), int(g("NumMipLevels"))
    fm = re.search(r"Format\s*=\s*\d+ \(VTEX_FORMAT_(\w+)\)", d)
    fmt = fm.group(1) if fm else "?"
    bpp = BPP.get(fmt, 1)
    total, ww, hh = 0.0, w, h
    for _ in range(max(mips, 1)):
        bw, bh = (max(4, ww), max(4, hh)) if bpp < 2 and fmt not in ("I8",) else (max(1, ww), max(1, hh))
        total += bw * bh * bpp
        ww, hh = max(1, ww // 2), max(1, hh // 2)
    return {"path": path, "w": w, "h": h, "mips": mips, "format": fmt, "bytes": int(total)}

def mat_textures(vmat, prefer):
    v = where(vmat + "_c", prefer)
    if v is None:
        return []
    out = subprocess.run([MAGICIAN, "refs", v, vmat + "_c"], capture_output=True, text=True).stdout
    return [l.split()[-1] for l in out.splitlines() if l.strip().endswith(".vtex_c")]

def texture_set(materials, prefer):
    seen, rows = {}, []
    for m in materials:
        for t in mat_textures(m, prefer):
            if t in seen:
                seen[t]["materials"].append(m)
                continue
            v = where(t, prefer + [CORE_PAK])
            if v is None:
                continue
            r = tex_profile(v, t)
            r["materials"] = [m]
            r["shared_with_game"] = v in (GAME_PAK, CORE_PAK)
            seen[t] = r
            rows.append(r)
    return rows

ours_vpk = os.path.abspath(A.vpk)
model = cfg["model"]
prof = {"ours": model_profile(ours_vpk, model), "valve": model_profile(GAME_PAK, model)}
prof["ours"]["textures"] = texture_set(prof["ours"]["materials"], [ours_vpk, GAME_PAK])
prof["valve"]["textures"] = texture_set(prof["valve"]["materials"], [GAME_PAK])
for k in ("ours", "valve"):
    tx = prof[k]["textures"]
    prof[k]["texture_count"] = len(tx)
    prof[k]["texture_mb"] = round(sum(t["bytes"] for t in tx) / 2 ** 20, 2)
    # memory the mod adds: its own textures (Valve's shared ones are resident for the original anyway)
    prof[k]["texture_mb_own"] = round(sum(t["bytes"] for t in tx if not t["shared_with_game"]) / 2 ** 20, 2)
    for lod, L in prof[k]["lods"].items():
        L["texture_mb"] = round(sum(t["bytes"] for t in tx if any(m in L["materials"] for m in t["materials"])) / 2 ** 20, 2)
print("model done", flush=True)

# ------------------------------------------------------------------------------------------------ particles
tmp = tempfile.mkdtemp(prefix="qa_vpcf_")
try:
    ours_p = [p for p in listing(ours_vpk) if p.endswith(".vpcf_c") and p.startswith(tuple(cfg["particle_prefixes"]))]
    for side, vpk in (("ours", ours_vpk), ("valve", GAME_PAK)):
        for pre in cfg["particle_prefixes"]:
            vrf(["-i", vpk, "-f", pre, "-e", "vpcf_c", "-d", "-o", os.path.join(tmp, side)])
    SPRITE = ("C_OP_RenderSprites", "C_OP_RenderTrails", "C_OP_RenderRopes", "C_OP_RenderStandardLight")
    def pcf(side, path):
        f = os.path.join(tmp, side, path[:-2])
        if not os.path.exists(f):
            return None
        d = kv3(open(f, encoding="utf-8", errors="replace").read())
        maxp = num(d.get("m_nMaxParticles"), 1000.0)
        rends = [r.get("_class", "?") for r in d.get("m_Renderers", []) if isinstance(r, dict)]
        burst, rate = 0.0, 0.0
        for e in d.get("m_Emitters", []):
            if not isinstance(e, dict):
                continue
            if "Instantaneous" in e.get("_class", ""):
                burst += num(e.get("m_nParticlesToEmit"), 1.0) or 0
            elif "Continuous" in e.get("_class", ""):
                rate += num(e.get("m_flEmitRate"), 100.0) or 0
        rad = num(d.get("m_flConstantRadius"), 5.0)
        life = num(d.get("m_flConstantLifespan"), 1.0)
        for i in d.get("m_Initializers", []):
            if not isinstance(i, dict):
                continue
            c = i.get("_class", "")
            if c == "C_INIT_RandomRadius":
                rad = max(num(i.get("m_flRadiusMin"), rad) or rad, num(i.get("m_flRadiusMax"), rad) or rad)
            if c == "C_INIT_RandomLifeTime":
                life = num(i.get("m_fLifetimeMax"), life) or life
            if c == "C_INIT_InitFloat" and "RADIUS" in str(i.get("m_nOutputField", "")).upper():
                rad = num(i.get("m_InputValue"), rad) or rad
        alive = min(maxp, burst + rate * life)
        sprite = any(r in SPRITE[:3] for r in rends)
        fill = alive * (2 * rad * K) ** 2 / 225.0 if sprite else 0.0
        kids = [c.get("m_ChildRef", "") for c in d.get("m_Children", []) if isinstance(c, dict)]
        lights = sum(1 for r in rends if "Light" in r)
        return {"max_particles": maxp, "burst": burst, "rate": rate, "lifetime": round(life, 3), "radius_units": round(rad, 2),
                "alive_est": round(alive, 1), "renderers": rends, "lights": lights, "children": len(kids),
                "fill_10m": round(fill, 4)}
    import difflib
    parts = []
    for p in sorted(ours_p):
        o, v = pcf("ours", p), pcf("valve", p)
        fo, fv = os.path.join(tmp, "ours", p[:-2]), os.path.join(tmp, "valve", p[:-2])
        changed = None
        if os.path.exists(fo) and os.path.exists(fv):
            a = open(fo, encoding="utf-8", errors="replace").read().splitlines()
            b = open(fv, encoding="utf-8", errors="replace").read().splitlines()
            changed = sum(1 for l in difflib.unified_diff(b, a, n=0) if l[:1] in "+-" and l[:3] not in ("+++", "---"))
        parts.append({"path": p, "ours": o, "valve": v, "new": v is None, "lines_changed": changed})
    prof["particles"] = parts
finally:
    shutil.rmtree(tmp, ignore_errors=True)

def tot(side, key):
    return round(sum((x[side] or {}).get(key, 0) for x in prof["particles"]), 3)
prof["particle_totals"] = {s: {"files": sum(1 for x in prof["particles"] if x[s]), "max_particles": tot(s, "max_particles"),
                               "alive_est": tot(s, "alive_est"), "fill_10m": tot(s, "fill_10m"), "lights": tot(s, "lights"),
                               "children": tot(s, "children")} for s in ("ours", "valve")}
json.dump(prof, open(os.path.join(A.out, "profile.json"), "w"), indent=1)

# ------------------------------------------------------------------------------------------------ markdown
o, v = prof["ours"], prof["valve"]
L = ["# Profile: %s, ours against Valve's" % cfg["hero"], "",
     "| LOD | tris ours | tris Valve | x | verts ours | verts Valve | draw calls ours | Valve | materials ours | Valve | textures MB ours | Valve |",
     "|---|---|---|---|---|---|---|---|---|---|---|---|"]
for lod in sorted(set(o["lods"]) | set(v["lods"]), key=int):
    a, b = o["lods"].get(lod, {}), v["lods"].get(lod, {})
    L.append("| %s | %s | %s | %.2f | %s | %s | %s | %s | %s | %s | %s | %s |" % (
        lod, f"{a.get('tris', 0):,}", f"{b.get('tris', 0):,}", a.get("tris", 0) / max(b.get("tris", 1), 1),
        f"{a.get('verts', 0):,}", f"{b.get('verts', 0):,}", a.get("draw_calls", 0), b.get("draw_calls", 0),
        len(a.get("materials", [])), len(b.get("materials", [])), a.get("texture_mb", 0), b.get("texture_mb", 0)))
L += ["", "Bones: ours %d, Valve's %d. LOD switch distances: ours [%s], Valve's [%s]. Model file: ours %.2f MB, Valve's %.2f MB." % (
    o["bones"], v["bones"], o["lod_switch"], v["lod_switch"], o["file_bytes"] / 2 ** 20, v["file_bytes"] / 2 ** 20),
      "Textures: ours %d (%.1f MB on the GPU, %.1f MB of it the mod's own), Valve's %d (%.1f MB)." % (
    o["texture_count"], o["texture_mb"], o["texture_mb_own"], v["texture_count"], v["texture_mb"]), "",
      "| texture | size | format | mips | MB | materials |", "|---|---|---|---|---|---|"]
for t in sorted(o["textures"], key=lambda t: -t["bytes"]):
    L.append("| %s | %dx%d | %s | %d | %.2f | %s |" % (t["path"].split("/")[-1], t["w"], t["h"], t["format"], t["mips"],
                                                        t["bytes"] / 2 ** 20, ", ".join(m.split("/")[-1] for m in t["materials"])))
L += ["", "Draw calls, LOD0, with the UV density the compiler measured (texture px per unit; higher is sharper):", "",
      "| model | mesh | material | tris | uv density |", "|---|---|---|---|---|"]
for side in ("ours", "valve"):
    masks_lod0 = [i for i, calls in enumerate(prof[side]["meshes"])]
    for i, calls in enumerate(prof[side]["meshes"]):
        for c in calls:
            if 0 in prof[side]["lods"] and c["material"] in prof[side]["lods"][0]["materials"] and i < 5:
                L.append("| %s | %d | %s | %s | %.1f |" % (side, i, c["material"].split("/")[-1], f"{c['tris']:,}", c["uv_density"]))
pt = prof["particle_totals"]
L += ["", "## Particles", "",
      "Totals over the %d files the mod ships: max particles ours %d vs Valve %d; particles alive (estimate) %.0f vs %.0f; "
      "fill at 10 m %.2f vs %.2f screens; lights %d vs %d; children %d vs %d." % (
          pt["ours"]["files"], pt["ours"]["max_particles"], pt["valve"]["max_particles"], pt["ours"]["alive_est"],
          pt["valve"]["alive_est"], pt["ours"]["fill_10m"], pt["valve"]["fill_10m"], pt["ours"]["lights"], pt["valve"]["lights"],
          pt["ours"]["children"], pt["valve"]["children"]), "",
      "| file | lines changed | max ours | max Valve | alive ours | alive Valve | radius ours | Valve | fill 10 m ours | Valve | renderers ours |",
      "|---|---|---|---|---|---|---|---|---|---|---|"]
for x in sorted(prof["particles"], key=lambda x: -((x["ours"] or {}).get("fill_10m", 0))):
    a, b = x["ours"] or {}, x["valve"] or {}
    L.append("| %s%s | %s | %s | %s | %s | %s | %s | %s | %s | %s | %s |" % (
        x["path"].split("/")[-1], " (new)" if x["new"] else "", "-" if x.get("lines_changed") is None else x["lines_changed"], a.get("max_particles", "-"), b.get("max_particles", "-"),
        a.get("alive_est", "-"), b.get("alive_est", "-"), a.get("radius_units", "-"), b.get("radius_units", "-"),
        a.get("fill_10m", "-"), b.get("fill_10m", "-"), ", ".join(r.replace("C_OP_Render", "") for r in a.get("renderers", []))))
open(os.path.join(A.out, "profile.md"), "w", encoding="utf-8").write("\n".join(L) + "\n")
print("PROFILE_DONE", json.dumps({"lod0_tris": [o["lods"].get(0, {}).get("tris"), v["lods"].get(0, {}).get("tris")],
                                  "tex_mb": [o["texture_mb"], v["texture_mb"]], "particles": pt}))

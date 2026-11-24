"""Magician visual QA: one run, one dated report folder. Called by run_qa.ps1 (same arguments).

  python -I run_qa.py --hero haze|<profile.json> --vpk <mod pak> [--clip-vpk <clips pak>]... [--steps all|a,b] [--clips curated|all|a,b]
                      [--frames 10] [--seq-frames 8] [--out <report dir>] [--work <work root>]

Steps (each reads only what earlier steps cached; any can be re-run alone):
  stage     the hero model from --vpk plus every clip in --vpk and in each --clip-vpk, packed into one VPK, so Source 2 Viewer
            resolves our clips first and Valve's for the rest: our compiled mesh, Valve's skeleton, our clips
  export    that VPK and Valve's shipped model to glb (materials, every clip, additive clips composed over bind)
  cache     each glb into a .blend (Blender 5.2), clip names shortened (glb_clips.py)
  index     the designer's pieces (piece_index.py), so compiled loose parts get their piece names
  geo       topology and surface checks on both models at rest (qa_geo.py)
  motion    every clip of the set posed frame by frame on both models: layer inversions, new intersections,
            stretch, spikes; worst offenders rendered (qa_motion.py)
  holes     back faces a camera sees on every sampled frame (gameplay, front, below): open sheets, flipped faces,
            and a body's removed faces exposed when the garment over them lifts (qa_holes.py)
  sequences fixed-camera frame strips of each sequence clip, ours over Valve's (qa_render.py --mode sequences)
  views     view styles in matched cameras and their pixel diffs (qa_render.py --mode views, qa_diff.py)
  profile   triangles, vertices, draw calls, bones, textures per LOD; particles and overdraw (qa_profile.py)
  cloth     every compiled cloth chain's freedom (static, position-driven, goal, damping) against Valve's (qa_cloth.py)
  report    index.md, contact sheets, findings.json (qa_report.py)

Cached under <work>/qa/cache/ by content hash: a second run on the same VPKs skips stage, export and cache.
"""
import argparse, glob, hashlib, json, os, shutil, subprocess, sys, time, datetime
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "ingame"))
import gamepaths  # noqa: E402  (locations as magician.exe resolves them)

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.abspath(os.path.join(HERE, "..", ".."))
VRF = gamepaths.vrf()
BLENDER = os.environ.get("BLENDER") or gamepaths.config("blender") or "blender"
# piece_index.py runs under the 4.5 series (BLENDER_45 or the config's blender_45; else the same Blender)
BLENDER_45 = os.environ.get("BLENDER_45") or gamepaths.config("blender_45") or BLENDER
GAME = "steam:1422450/game/citadel"
GAME_PAK = GAME + "/pak01_dir.vpk"
GAMEINFO = os.path.join(gamepaths.try_game(), "citadel", "gameinfo.gi")
MAGICIAN = os.path.join(HERE, "..", "target", "release", "magician.exe")
REFROOT = gamepaths.work()
ALL_STEPS = ["stage", "export", "cache", "index", "geo", "motion", "holes", "sequences", "views", "profile", "cloth", "report"]

ap = argparse.ArgumentParser()
ap.add_argument("--hero", required=True)
ap.add_argument("--vpk", required=True)
ap.add_argument("--clip-vpk", action="append", default=[])
ap.add_argument("--steps", default="all")
ap.add_argument("--clips", default="curated")
ap.add_argument("--frames", type=int, default=10, help="frames sampled per clip in the motion sweep (0 = every frame)")
ap.add_argument("--seq-frames", type=int, default=8)
ap.add_argument("--work", default=None)
ap.add_argument("--out", default=None)
ap.add_argument("--res", type=int, default=None, help="render width; default 480 (draft: 240)")
ap.add_argument("--full", action="store_true", help="full-quality pictures and ray grids; the default is draft (DEADLOCK_DRAFT unset or 1; 0 = full)")
ap.add_argument("--who", default="both", help="ours, valve or both: Valve's side depends only on the game build, so a "
                "re-run for a new mod build can take it from --valve-from")
ap.add_argument("--valve-from", default=None, help="an earlier report folder to copy Valve's results from")
A = ap.parse_args()
# Draft mode: half-size renders, minimal samples (the holes step's ray grid is unchanged: halving it saved no time). Detection on geometry
# (geo, motion detectors, profile, cloth) is identical in both modes. The Blender steps read DEADLOCK_DRAFT.
DRAFT = (not A.full) and os.environ.get("DEADLOCK_DRAFT", "1") != "0"
os.environ["DEADLOCK_DRAFT"] = "1" if DRAFT else "0"
if A.res is None:
    A.res = 240 if DRAFT else 480

# --hero names a profile in qa/heroes/ (haze is the worked example) or is the path of one, e.g. a mod's own
# <mod>/source/qa/heroes/<hero>.json
HERO_CFG = A.hero if A.hero.lower().endswith(".json") or os.path.isfile(A.hero) else os.path.join(HERE, "heroes", A.hero + ".json")
if not os.path.isfile(HERO_CFG):
    sys.exit(f"run_qa: no hero profile {HERO_CFG} (copy qa/heroes/haze.json into your mod and pass its path)")
cfg = json.load(open(HERO_CFG, encoding="utf-8"))
WORK = A.work or os.path.join(REFROOT, cfg.get("mod", A.hero), "qa")
CACHE = os.path.join(WORK, "cache")
OUT = A.out or os.path.join(WORK, datetime.datetime.now().strftime("%Y-%m-%d_%H%M"))
steps = ALL_STEPS if A.steps == "all" else A.steps.split(",")
os.makedirs(OUT, exist_ok=True)
os.makedirs(CACHE, exist_ok=True)
LOG = open(os.path.join(OUT, "run.log"), "a", encoding="utf-8")
T0 = time.time()

def say(*a):
    line = "[qa %6.0fs] " % (time.time() - T0) + " ".join(str(x) for x in a)
    print(line, flush=True)
    LOG.write(line + "\n"); LOG.flush()

def run(cmd, log=None, check=True):
    say("$", " ".join(os.path.basename(c) if i == 0 else c for i, c in enumerate(cmd))[:400])
    with open(log or os.devnull, "a", encoding="utf-8", errors="replace") as lf:
        low = os.path.basename(cmd[0]).lower().startswith("blender")      # every Blender launch at below-normal priority
        p = subprocess.run(cmd, stdout=lf if log else subprocess.DEVNULL, stderr=subprocess.STDOUT,
                           creationflags=subprocess.BELOW_NORMAL_PRIORITY_CLASS if low else 0)
    if check and p.returncode != 0:
        raise SystemExit("step failed (%d): %s; see %s" % (p.returncode, cmd[0], log))
    return p.returncode

def blender(script, args, log):
    return run([BLENDER, "--background", "--factory-startup", "--python", os.path.join(HERE, script), "--"] + args, log)

def vpk_listing(vpk):
    out = subprocess.run([MAGICIAN, "ls", vpk], capture_output=True, text=True).stdout
    return [l.split("\t") for l in out.splitlines() if l.count("\t") == 2]

def game_args():
    return ["--game", GAME] if os.path.exists(GAMEINFO) else []

# ---------------------------------------------------------------- identities
model = cfg["model"]
ours_rows = [r for r in vpk_listing(A.vpk) if r[0] == model]
if not ours_rows:
    raise SystemExit("%s has no %s" % (A.vpk, model))
clip_rows = [r for r in vpk_listing(A.vpk) if r[0].endswith(".vnmclip_c")]      # a release pak carries its clips
for cv in A.clip_vpk:
    clip_rows += [r for r in vpk_listing(cv) if r[0].endswith(".vnmclip_c")]
mat_rows = [r for r in vpk_listing(A.vpk) if r[0].endswith((".vmat_c", ".vtex_c"))]          # the staged materials and textures
key = hashlib.sha1(("|".join(ours_rows[0]) + "".join("|".join(r) for r in sorted(clip_rows + mat_rows))).encode()).hexdigest()[:10]
OURS = os.path.join(CACHE, "ours-" + key)
VALVE = os.path.join(CACHE, "valve")
os.makedirs(OURS, exist_ok=True)
os.makedirs(VALVE, exist_ok=True)
meta = {"hero": A.hero, "vpk": A.vpk, "clip_vpks": A.clip_vpk, "model": model, "model_crc": ours_rows[0][1],
        "model_size": int(ours_rows[0][2]), "clips_overlaid": len(clip_rows), "cache_key": key,
        "started": datetime.datetime.now().isoformat(timespec="seconds"), "steps": steps, "draft": DRAFT, "res": A.res}
json.dump(meta, open(os.path.join(OUT, "run.json"), "w"), indent=1)
say("hero", A.hero, "model crc", ours_rows[0][1], "clips overlaid", len(clip_rows), "->", OUT)

# ---------------------------------------------------------------- stage + export + cache
def stage():
    st = os.path.join(OURS, "stage")
    if os.path.exists(os.path.join(OURS, "qa_input.vpk")):
        return
    shutil.rmtree(st, ignore_errors=True)
    run([VRF, "-i", A.vpk, "-f", model, "-o", st], os.path.join(OURS, "stage.log"))
    # the mod's own materials and textures: the model refers to them by path, and without them they export blank and the
    # classes below file the mod's own surfaces under gear (b25's body pass: lash_body_ext). A mod pak holds only what
    # it made, so every material and texture in it is one the model may reference; Valve's own come from the game.
    pak_rows = vpk_listing(A.vpk)
    for ext in ("vmat_c", "vtex_c"):
        if any(r[0].endswith("." + ext) for r in pak_rows):
            run([VRF, "-i", A.vpk, "-e", ext, "-o", st], os.path.join(OURS, "stage.log"))
    if any(r[0].endswith(".vnmclip_c") for r in vpk_listing(A.vpk)):
        run([VRF, "-i", A.vpk, "-e", "vnmclip_c", "-o", st], os.path.join(OURS, "stage.log"))
    for cv in A.clip_vpk:
        run([VRF, "-i", cv, "-e", "vnmclip_c", "-o", st], os.path.join(OURS, "stage.log"))
    run([VRF, "-i", st, "--vpk_create", os.path.join(OURS, "qa_input.vpk")], os.path.join(OURS, "stage.log"))
    shutil.rmtree(st, ignore_errors=True)

def export_one(src_vpk, dst):
    blend = os.path.join(dst, "model.blend")
    if os.path.exists(blend):
        return
    gl = os.path.join(dst, "glb")
    if not os.path.exists(os.path.join(dst, "named.glb")):
        run([VRF, "-i", src_vpk, "-f", model] + game_args() + ["-o", gl, "--gltf_export_format", "glb",
             "--gltf_export_materials", "--gltf_export_animations", "--gltf_compose_additive", "--quiet"],
            os.path.join(dst, "export.log"))
        raw = os.path.join(gl, os.path.dirname(model), os.path.basename(model).replace(".vmdl_c", ".glb"))
        # keep the textures beside the renamed glb: the .blend refers to them by path
        run([sys.executable, "-I", os.path.join(HERE, "glb_clips.py"), raw, os.path.join(os.path.dirname(raw), "named.glb"),
             "--tsv", os.path.join(dst, "clips.tsv")], os.path.join(dst, "export.log"))
        os.remove(raw)
        os.replace(os.path.join(os.path.dirname(raw), "named.glb"), os.path.join(dst, "named.glb"))
    return True

def cache_one(dst):
    blend = os.path.join(dst, "model.blend")
    if os.path.exists(blend):
        return
    tex = None
    for root, _, files in os.walk(os.path.join(dst, "glb")):
        if any(f.endswith(".png") for f in files):
            tex = root
    named = os.path.join(dst, "named.glb")
    # the glb names its textures relatively: import it from beside them
    local = os.path.join(tex, "named.glb") if tex else named
    if tex:
        os.replace(named, local)
    blender("qa_cache.py", [local, blend, "--clips-tsv", os.path.join(dst, "actions.tsv")], os.path.join(dst, "cache.log"))
    os.remove(local)       # the .blend holds the meshes and clips; the textures stay for its image paths

if "stage" in steps:
    stage()
if "export" in steps:
    export_one(os.path.join(OURS, "qa_input.vpk"), OURS)
    export_one(GAME_PAK, VALVE)
if "cache" in steps:
    cache_one(OURS)
    cache_one(VALVE)

# ---------------------------------------------------------------- analysis
INDEX = os.path.join(CACHE, "pieces.json")
if "index" in steps and cfg.get("design_blend"):
    db = cfg["design_blend"]
    db = db if os.path.isabs(db) else os.path.join(REFROOT, db)   # relative: under <reference>/work
    stamp = "%s|%d" % (db, os.path.getmtime(db))
    if not (os.path.exists(INDEX) and json.load(open(INDEX)).get("stamp") == stamp):
        run([BLENDER_45, "--background", "--factory-startup", "--python",
             os.path.join(HERE, "piece_index.py"), "--", db, INDEX, "--stamp", stamp], os.path.join(CACHE, "index.log"))

clipset = cfg["clip_sets"].get(A.clips, None)
clips = ",".join(clipset) if clipset else A.clips
common = ["--hero-cfg", HERO_CFG, "--res", str(A.res)]

if A.valve_from:
    for sub in ("geo", "motion", "holes", "sequences", "views"):
        src = os.path.join(A.valve_from, sub, "valve")
        if os.path.isdir(src) and not os.path.exists(os.path.join(OUT, sub, "valve")):
            shutil.copytree(src, os.path.join(OUT, sub, "valve"))
            say("valve", sub, "from", A.valve_from)
for who, d in (("ours", OURS), ("valve", VALVE)):
    if A.who != "both" and who != A.who:
        continue
    blend = os.path.join(d, "model.blend")
    # the designer's piece names belong to our compiled model only
    common = [c for c in common if c not in ("--pieces", INDEX)] + (["--pieces", INDEX] if who == "ours" and os.path.exists(INDEX) else [])
    if "geo" in steps:
        blender("qa_geo.py", [blend, os.path.join(OUT, "geo", who), "--who", who] + common, os.path.join(OUT, "geo_%s.log" % who))
    if "motion" in steps:
        mirror = os.path.join(OUT, "motion", "ours", "motion.json")
        extra = ["--mirror", mirror] if who == "valve" and os.path.exists(mirror) else []
        blender("qa_motion.py", [blend, os.path.join(OUT, "motion", who), "--who", who, "--clips", clips,
                                 "--frames", str(A.frames)] + extra + common, os.path.join(OUT, "motion_%s.log" % who))
    if "holes" in steps:
        blender("qa_holes.py", [blend, os.path.join(OUT, "holes", who), "--who", who, "--clips", clips,
                                "--frames", str(A.frames)] + common, os.path.join(OUT, "holes_%s.log" % who))
    if "sequences" in steps:
        blender("qa_render.py", [blend, os.path.join(OUT, "sequences", who), "--mode", "sequences", "--who", who,
                                 "--clips", ",".join(cfg["clip_sets"]["sequences"]), "--frames", str(A.seq_frames),
                                 "--ref", os.path.join(VALVE, "model.blend")] + common, os.path.join(OUT, "seq_%s.log" % who))
    if "views" in steps:
        blender("qa_render.py", [blend, os.path.join(OUT, "views", who), "--mode", "views", "--who", who,
                                 "--ref", os.path.join(VALVE, "model.blend")] + common, os.path.join(OUT, "views_%s.log" % who))
if "motion" in steps and A.valve_from and A.who == "ours":
    # Valve's sweep came from the earlier folder; its renders of OUR worst frames are made fresh
    for f in glob.glob(os.path.join(OUT, "motion", "valve", "top", "mirror_*")):
        os.remove(f)
    blender("qa_motion.py", [os.path.join(VALVE, "model.blend"), os.path.join(OUT, "motion", "valve"), "--who", "valve",
                             "--mirror", os.path.join(OUT, "motion", "ours", "motion.json"), "--mirror-only",
                             "--hero-cfg", HERO_CFG, "--res", str(A.res)],
            os.path.join(OUT, "motion_valve_mirror.log"))
# our mesh on Valve's own clips, for every clip of the set that a --clip-vpk replaces: a defect that is there with
# Valve's clip too belongs to the mesh; one that only shows on our clip belongs to the clip
replaced = sorted({os.path.basename(r[0]).split(".")[0] for r in clip_rows})
if "motion" in steps and replaced:
    sel = [c for c in clips.split(",") if c in replaced] if clips != "all" else replaced
    if sel:
        blender("qa_motion.py", [os.path.join(OURS, "model.blend"), os.path.join(OUT, "motion", "ours_valveclips"),
                                 "--who", "ours_valveclips", "--clips", ",".join(sel), "--frames", str(A.frames), "--top", "0",
                                 "--clip-source", os.path.join(VALVE, "model.blend"), "--hero-cfg",
                                 HERO_CFG, "--res", str(A.res)]
                + (["--pieces", INDEX] if os.path.exists(INDEX) else []), os.path.join(OUT, "motion_ours_valveclips.log"))
json.dump(dict(meta, replaced_clips=replaced), open(os.path.join(OUT, "run.json"), "w"), indent=1)
if "views" in steps:
    run([sys.executable, "-I", os.path.join(HERE, "qa_diff.py"), os.path.join(OUT, "views")], os.path.join(OUT, "diff.log"))
if "profile" in steps:
    run([sys.executable, "-I", os.path.join(HERE, "qa_profile.py"), "--hero-cfg", HERO_CFG,
         "--vpk", A.vpk, "--out", os.path.join(OUT, "profile")], os.path.join(OUT, "profile.log"))
if "cloth" in steps:
    run([sys.executable, "-I", os.path.join(HERE, "qa_cloth.py"), "--vpk", os.path.join(OURS, "qa_input.vpk"), "--model", model,
         "--out", os.path.join(OUT, "cloth")], os.path.join(OUT, "cloth.log"))
if "report" in steps:
    run([sys.executable, "-I", os.path.join(HERE, "qa_report.py"), OUT, "--hero-cfg", HERO_CFG],
        os.path.join(OUT, "report.log"))
say("done", OUT)

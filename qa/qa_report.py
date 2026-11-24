"""QA report: contact sheets, findings.json and index.md for one run folder.

  python -I qa_report.py <run dir> --hero-cfg <hero.json>

Reads what the other steps wrote (geo/, motion/, sequences/, views/diff/, profile/) and:
- aggregates motion events across frames into findings, one per (detector, piece, piece): frames hit, clips, the
  worst depth / length / ratio, the most visible frame (px from the gameplay camera);
- triages each finding automatically:
    valve-own   both sides are Valve's body or kit (skin, gear) and Valve's model shows the same detector on the
                same classes and bone region in the same clip
    clip        a replaced clip shows it, and our mesh on Valve's version of that clip does not (motion/ours_valveclips)
    mesh        the rest: ours (garments, or Valve's parts in a way Valve's model does not show)
  and grades it: defect (over the visibility and size bars below), minor (seen but small), hidden (not seen from
  the gameplay camera). Cloth-driven findings (over half the vertices on cloth bones) are marked: offline they sit
  at the animated goal pose and the solver moves them in game.
- builds sheets: sheets/seq_<clip>.png (per camera, Valve's row over ours), sheets/motion_<kind>.png (worst
  offenders: gameplay, close, close highlighted; Valve's same frame beside), sheets/geo_<who>.png.
The bars: poke >= 3 mm deep and >= 20 px; contact >= 2 cm and >= 40 px; float >= 15 mm and >= 20 px; stretch >= 2.0x or crush <= 0.35x and
>= 150 px; any spike seen.
"""
import sys, os, json, glob, datetime
from PIL import Image, ImageDraw, ImageFont

run = sys.argv[1]
cfg = json.load(open(sys.argv[sys.argv.index("--hero-cfg") + 1]))
SH = os.path.join(run, "sheets")
os.makedirs(SH, exist_ok=True)
try:
    FONT = ImageFont.load_default(size=13)
except TypeError:
    FONT = ImageFont.load_default()

def jload(p):
    return json.load(open(p)) if os.path.exists(p) else None

def tile(path, w, h=None):
    if not path or not os.path.exists(path):
        im = Image.new("RGB", (w, h or w), (30, 30, 34))
        return im
    im = Image.open(path).convert("RGBA")
    bg = Image.new("RGBA", im.size, (40, 40, 46, 255)); bg.alpha_composite(im)
    im = bg.convert("RGB")
    h = h or int(im.height * w / im.width)
    return im.resize((w, h))

def caption(im, text, h=34):
    out = Image.new("RGB", (im.width, im.height + h), (18, 18, 22))
    out.paste(im, (0, 0))
    d = ImageDraw.Draw(out)
    y = im.height + 3
    for line in text.split("\n")[:2]:
        d.text((5, y), line, fill=(235, 225, 200), font=FONT); y += 15
    return out

def grid(rows, path, title=""):
    rows = [r for r in rows if r]
    if not rows:
        return None
    W = max(sum(t.width for t in r) for r in rows)
    H = sum(max(t.height for t in r) for r in rows) + (26 if title else 0)
    sheet = Image.new("RGB", (W, H), (14, 14, 17))
    d = ImageDraw.Draw(sheet)
    y = 0
    if title:
        d.text((6, 6), title, fill=(255, 210, 120), font=FONT); y = 26
    for r in rows:
        x = 0
        for t in r:
            sheet.paste(t, (x, y)); x += t.width
        y += max(t.height for t in r)
    sheet.save(path)
    return os.path.basename(path)

# ------------------------------------------------------------------------------------------- motion findings
mo = {w: jload(os.path.join(run, "motion", w, "motion.json")) for w in ("ours", "valve", "ours_valveclips")}
meta = jload(os.path.join(run, "run.json")) or {}
replaced = set(meta.get("replaced_clips", []))

def metric(e):
    return {"poke": ("depth_mm", "mm"), "contact": ("length_cm", "cm"), "stretch": ("ratio", "x"), "crush": ("ratio", "x"),
            "spike": ("dev_cm", "cm"), "float": ("gap_mm", "mm")}[e["kind"]]   # contact "cm": extent of the crossing (a sheet passing through a sheet counts its area's span)

COMP = {w: jload(os.path.join(run, "geo", w, "components.json")) for w in ("ours", "valve")}

def relabel(events, who):
    """Name pieces by the geometry step's labels (same cache, newest piece matching), by component id."""
    m = COMP.get(who)
    if not m:
        return
    for e in events:
        if str(e.get("a_comp")) in m:
            e["a"] = m[str(e["a_comp"])]
        if e.get("b_comp", -1) >= 0 and str(e["b_comp"]) in m:
            e["b"] = m[str(e["b_comp"])]

for w in ("ours", "valve", "ours_valveclips"):
    if mo[w]:
        relabel(mo[w]["events"] + mo[w].get("top", []), "valve" if w == "valve" else "ours")

def aggregate(events):
    agg = {}
    for e in events:
        k = (e["kind"], e["a"], e["b"])
        f = agg.setdefault(k, {"kind": e["kind"], "a": e["a"], "b": e["b"], "a_cls": e["a_cls"], "b_cls": e["b_cls"],
                               "frames": 0, "clips": {}, "worst": None, "size": None, "cloth": 0.0, "regions": {}})
        f["frames"] += 1
        f["clips"][e["clip"]] = f["clips"].get(e["clip"], 0) + 1
        f["regions"][e["region"]] = f["regions"].get(e["region"], 0) + 1
        f["cloth"] = max(f["cloth"], e["cloth"])
        mk, _ = metric(e)
        v = e.get(mk, 0)
        if f["size"] is None or (v < f["size"] if e["kind"] == "crush" else v > f["size"]):
            f["size"] = v
        if f["worst"] is None or e["vis_px"] > f["worst"]["vis_px"]:
            f["worst"] = {k2: e[k2] for k2 in ("clip", "frame", "vis_px", "vis_frac", "region", "centroid") if k2 in e}
            f["worst"][mk] = v
    return agg

findings = []
if mo["ours"]:
    agg = aggregate(mo["ours"]["events"])
    v_idx = {}
    if mo["valve"]:
        for e in mo["valve"]["events"]:
            v_idx.setdefault((e["kind"], e["a_cls"], e["b_cls"], e["clip"]), set()).add(e["region"])
    vc_idx = set()
    if mo["ours_valveclips"]:
        for e in mo["ours_valveclips"]["events"] + []:
            vc_idx.add((e["kind"], e["a"], e["b"], e["clip"].replace("@src", "")))
    for f in agg.values():
        w = f["worst"]
        valve_parts = {f["a_cls"], f["b_cls"]} <= {"skin", "gear", ""}
        same_in_valve = [c for c in f["clips"] if (f["kind"], f["a_cls"], f["b_cls"], c) in v_idx and
                         set(f["regions"]) & v_idx[(f["kind"], f["a_cls"], f["b_cls"], c)]]
        rep = [c for c in f["clips"] if c in replaced]
        if valve_parts and same_in_valve:
            owner = "valve-own"
        elif rep and mo["ours_valveclips"] and all((f["kind"], f["a"], f["b"], c) not in vc_idx for c in rep) and set(f["clips"]) <= replaced:
            owner = "clip"
        else:
            owner = "mesh"
        size = f["size"] or 0
        k = f["kind"]
        big = {"poke": size >= 3, "contact": size >= 2, "stretch": size >= 2.0, "crush": size <= 0.35, "spike": True, "float": size >= 15}[k]
        bar = {"poke": 20, "contact": 40, "stretch": 150, "crush": 150, "spike": 1, "float": 20}[k]
        grade = "hidden" if w["vis_px"] <= 0 else ("defect" if big and w["vis_px"] >= bar else "minor")
        # an edge-length change is a measurement, not a picture: on a plain dark satin a 4x stretch is invisible,
        # on lace or a stripe it is a smear. Never graded past "review"; the sheet decides
        if k in ("stretch", "crush") and grade == "defect":
            grade = "review"
        f.update({"owner": owner, "grade": grade, "replaced_clips_hit": rep, "valve_same_clips": same_in_valve[:6],
                  "cloth_driven": f["cloth"] > 0.5, "unit": metric({"kind": k})[1]})
        findings.append(f)
findings.sort(key=lambda f: (-{"defect": 3, "review": 2, "minor": 1, "hidden": 0}[f["grade"]], -f["worst"]["vis_px"]))

# ------------------------------------------------------------------------------------------- families
import re
FAM = [(k, re.compile(v)) for k, v in cfg.get("families", {}).items() if not k.startswith("_")]
def family(label):
    for k, rx in FAM:
        if rx.search(label):
            return k
    return "other"
for f in findings:
    fa, fb = family(f["a"]), family(f["b"]) if f["b"] else None
    f["family"] = fa if (fb is None or fb == fa or fa != "Valve's body") else fb
fam_rows = {}
for f in findings:
    if f["grade"] not in ("defect", "review"):
        continue
    r = fam_rows.setdefault(f["family"], {"defect": 0, "review": 0, "worst": [], "clips": {}, "owners": {}})
    r[f["grade"]] += 1
    r["owners"][f["owner"]] = r["owners"].get(f["owner"], 0) + 1
    for c, n in f["clips"].items():
        r["clips"][c] = r["clips"].get(c, 0) + n
    if len(r["worst"]) < 5 and f["grade"] == "defect":
        r["worst"].append("%s %s%s %s %s, %s f%d (%.0f px)" % (f["kind"], f["a"], " | " + f["b"] if f["b"] else "", f["size"], f["unit"],
                                                            f["worst"]["clip"], f["worst"]["frame"], f["worst"]["vis_px"]))

# ------------------------------------------------------------------------------------------- holes
def generic(label):
    return re.sub(r"#\d+ \(\d+ cm\)", "", label)
holes = {w: jload(os.path.join(run, "holes", w, "holes.json")) for w in ("ours", "valve")}
hole_findings = []
if holes["ours"]:
    vkey = {}
    for e in (holes["valve"] or {}).get("events", []):
        k = (e["clip"], e["frame"], e["cam"], e.get("kind", "hole"), generic(e["label"]))
        vkey[k] = max(vkey.get(k, 0), e["px"])
    agg = {}
    for e in holes["ours"]["events"]:
        excess = e["px"] - e["rest_px"]
        k = (e["cam"], e.get("kind", "hole"), e["label"])
        f = agg.setdefault(k, {"cam": e["cam"], "kind": e.get("kind", "hole"), "label": e["label"], "cls": e["cls"], "frames": 0, "clips": {}, "worst": None,
                               "rest_px": e["rest_px"], "valve_px_same_frame": 0})
        f["frames"] += 1
        f["clips"][e["clip"]] = f["clips"].get(e["clip"], 0) + 1
        if f["worst"] is None or e["px"] > f["worst"]["px"]:
            f["worst"] = {"clip": e["clip"], "frame": e["frame"], "px": e["px"]}
            f["valve_px_same_frame"] = vkey.get((e["clip"], e["frame"], e["cam"], f["kind"], generic(e["label"])), 0)
    for f in agg.values():
        valve_parts = f["cls"] in ("skin", "gear")
        f["owner"] = "valve-own" if valve_parts and f["valve_px_same_frame"] >= 0.5 * f["worst"]["px"] else "mesh"
        f["grade"] = "defect" if f["worst"]["px"] >= 150 else ("minor" if f["worst"]["px"] >= 30 else "hidden")
        hole_findings.append(f)
    hole_findings.sort(key=lambda f: -f["worst"]["px"] * (1.5 if f["cam"] == "gameplay" else 1))

# ------------------------------------------------------------------------------------------- sheets
sheets = {}
# motion top offenders, per detector
if mo["ours"]:
    mirror = {}
    for p in glob.glob(os.path.join(run, "motion", "valve", "top", "mirror_*_gameplay.png")):
        rank = int(os.path.basename(p).split("_")[1])
        mirror[rank] = p
    by_kind = {}
    for rank, e in enumerate(mo["ours"]["top"], 1):
        by_kind.setdefault(e["kind"], []).append((rank, e))
    for kind, evs in by_kind.items():
        rows = []
        for rank, e in evs:
            base = os.path.join(run, "motion", "ours", "top", e.get("image", "%02d_%s_f%d" % (rank, e["clip"], e["frame"])))
            mk, unit = metric(e)
            cap = "#%d %s %s f%d  %s %s%s  %.0f px\n%s | %s" % (rank, kind, e["clip"], e["frame"], e.get(mk), unit, "", e["vis_px"], e["a"], e["b"])
            r = [tile(base + "_gameplay.png", 240, 360), tile(base + "_close.png", 360, 360), tile(base + "_close_hl.png", 360, 360)]
            mp = mirror.get(rank)
            if mp:
                r += [tile(mp, 240, 360), tile(mp.replace("_gameplay", "_close"), 360, 360)]
            row = Image.new("RGB", (sum(t.width for t in r), 360))
            x = 0
            for t in r:
                row.paste(t, (x, 0)); x += t.width
            rows.append([caption(row, cap)])
        sheets["motion_" + kind] = grid(rows, os.path.join(SH, "motion_%s.png" % kind),
                                        "%s: worst offenders from the gameplay camera. ours: gameplay (red drawn over everything), close-up, close-up with the triangles in red; then Valve's model on the same clip and frame" % kind)
# holes: textures and the facing style, side by side
for who in ("ours", "valve"):
    h = holes.get(who) if "holes" in dir() else None
    if not h:
        continue
    rows = []
    for rank, e in enumerate(h["top"], 1):
        b = os.path.join(run, "holes", who, "top", e.get("image", ""))
        tex = tile(b + "_tex.png", 300, 450)
        over = tex.copy()
        if os.path.exists(b + "_mask.png"):
            mk = Image.open(b + "_mask.png").convert("RGBA").resize((300, 450), Image.NEAREST)
            red = Image.new("RGB", (300, 450), (255, 20, 20))
            over.paste(red, (0, 0), mk.split()[3].point(lambda a: 170 if a > 0 else 0))
        r = [tex, over]
        row = Image.new("RGB", (600, 450)); row.paste(r[0], (0, 0)); row.paste(r[1], (300, 0))
        rows.append(caption(row, "#%d %s %s f%d %s: %.0f px (%.0f at rest)\n%s" % (rank, e.get("kind", "hole"), e["clip"], e["frame"], e["cam"], e["px"], e["rest_px"], e["label"][:70])))
    sheets["holes_" + who] = grid([rows[i:i + 3] for i in range(0, len(rows), 3)], os.path.join(SH, "holes_%s.png" % who),
                                  "holes and linings on %s: textures | the event's own pixels in red (hole: a back face the game culls; lining: a garment's inside seen from outside)" % who)
# sequences: per clip, Valve's row over ours, per camera
seqv = jload(os.path.join(run, "sequences", "valve", "sequences.json"))
seqo = jload(os.path.join(run, "sequences", "ours", "sequences.json"))
if seqv and seqo:
    for clip, frames in seqo["clips"].items():
        rows = []
        for cam in seqo["cams"]:
            for who, lab in (("valve", "Valve"), ("ours", "ours")):
                fr = (seqv if who == "valve" else seqo)["clips"].get(clip, frames)
                r = [caption(tile(os.path.join(run, "sequences", who, clip, "%s_f%03d.png" % (cam, f)), 160, 240), "%s %s f%d" % (lab, cam, f), 18) for f in fr]
                rows.append(r)
        sheets["seq_" + clip] = grid(rows, os.path.join(SH, "seq_%s.png" % clip),
                                     "%s: Valve's model and clip (upper row) and ours with our clips (lower row), per camera" % clip)
# geometry
for who in ("ours", "valve"):
    g = jload(os.path.join(run, "geo", who, "geo.json"))
    if not g:
        continue
    rows = []
    for im in g.get("images", []):
        if not im.get("image"):
            continue
        b = os.path.join(run, "geo", who, "img", im["image"])
        r = [tile(b + ".png", 300, 300), tile(b + "_hl.png", 300, 300)]
        row = Image.new("RGB", (600, 300)); row.paste(r[0], (0, 0)); row.paste(r[1], (300, 0))
        rows.append(caption(row, "%s #%d: %s\n%s" % (im["check"], im["rank"], im["count"], im["label"][:80])))
    pairs = [rows[i:i + 3] for i in range(0, len(rows), 3)]
    sheets["geo_" + who] = grid(pairs, os.path.join(SH, "geo_%s.png" % who), "geometry checks on %s at rest: plain, and the flagged cluster in red" % who)

# ------------------------------------------------------------------------------------------- index.md
geo = {w: jload(os.path.join(run, "geo", w, "geo.json")) for w in ("ours", "valve")}
prof = jload(os.path.join(run, "profile", "profile.json"))
diff = jload(os.path.join(run, "views", "diff", "diff.json"))
L = ["# Visual QA: %s, %s" % (cfg["hero"], os.path.basename(run.rstrip("/\\"))), "",
     "Model `%s` crc %s from `%s`, clips overlaid from %s (%d files). Generated %s." % (
         meta.get("model"), meta.get("model_crc"), meta.get("vpk"), ", ".join("`%s`" % c for c in meta.get("clip_vpks", [])) or "none",
         meta.get("clips_overlaid", 0), datetime.datetime.now().isoformat(timespec="minutes")), ""]
if meta.get("draft"):
    L += ["> **DRAFT run.** Pictures are half size (width %s px) with minimal samples (the holes step's ray grid is kept at 160 px: halving it saved no time and moved the hole areas). Detection on geometry (geo, motion detectors, profile, cloth) is exact and the same as a full run. Re-run with `-Full` (or `DEADLOCK_DRAFT=0`) before a verdict is signed off." % meta.get("res", "?"), ""]
if findings:
    n = {g: sum(1 for f in findings if f["grade"] == g) for g in ("defect", "minor", "hidden")}
    L += ["## Motion: findings by visibility from the gameplay camera", "",
          "%d findings over %d clips: %d defect, %d minor, %d hidden. Owners: %s." % (
              len(findings), len(mo["ours"]["clips"]), n["defect"], n["minor"], n["hidden"],
              ", ".join("%s %d" % (o, sum(1 for f in findings if f["owner"] == o)) for o in ("mesh", "clip", "valve-own"))),
          "", "| # | grade | owner | detector | piece | against | worst | px | clip f | frames | clips | cloth |",
          "|---|---|---|---|---|---|---|---|---|---|---|---|"]
    for i, f in enumerate([f for f in findings if f["grade"] != "hidden"][:80], 1):
        w = f["worst"]
        L.append("| %d | %s | %s | %s | %s | %s | %s %s | %.0f | %s f%s | %d | %d | %s |" % (
            i, f["grade"], f["owner"], f["kind"], f["a"], f["b"] or "-", f["size"], f["unit"], w["vis_px"], w["clip"], w["frame"],
            f["frames"], len(f["clips"]), "yes" if f["cloth_driven"] else ""))
    L += ["", "### By family (defects, then the worst of each)", ""]
    for k, r in sorted(fam_rows.items(), key=lambda kv: -kv[1]["defect"]):
        top_clips = ", ".join(c for c, _ in sorted(r["clips"].items(), key=lambda kv: -kv[1])[:6])
        L += ["- **%s**: %d defects, %d to review; owners %s; most in %s" % (k, r["defect"], r["review"],
              ", ".join("%s %d" % kv for kv in r["owners"].items()), top_clips)] + ["  - " + w for w in r["worst"]]
    L += ["", "Per clip (events on ours / on Valve's):", "", "| clip | frames | poke | contact | stretch | crush | spike | worst px |", "|---|---|---|---|---|---|---|---|"]
    for c, s in mo["ours"]["clips"].items():
        v = (mo["valve"] or {}).get("clips", {}).get(c, {})
        L.append("| %s%s | %d | %d / %s | %d / %s | %d / %s | %d / %s | %d / %s | %.0f / %s |" % (
            c, " (replaced)" if c in replaced else "", len(s["frames"]), s["poke"], v.get("poke", "-"), s["contact"], v.get("contact", "-"),
            s["stretch"], v.get("stretch", "-"), s["crush"], v.get("crush", "-"), s["spike"], v.get("spike", "-"), s["worst_px"],
            "%.0f" % v["worst_px"] if v else "-"))
if hole_findings:
    L += ["", "## Holes and linings (px at 720x1080 from the camera named)", "",
          "hole: a back face is the first thing the camera meets (the game culls it and draws what is behind). lining: the drawn side of a back shell, i.e. the inside of a garment, seen from outside.", "",
          "| # | grade | owner | kind | camera | piece | worst px | at rest | Valve same frame | clip f | frames | clips |", "|---|---|---|---|---|---|---|---|---|---|---|---|"]
    for i, f in enumerate([f for f in hole_findings if f["grade"] != "hidden"][:40], 1):
        L.append("| %d | %s | %s | %s | %s | %s | %.0f | %.0f | %.0f | %s f%d | %d | %d |" % (
            i, f["grade"], f["owner"], f["kind"], f["cam"], f["label"], f["worst"]["px"], f["rest_px"], f["valve_px_same_frame"],
            f["worst"]["clip"], f["worst"]["frame"], f["frames"], len(f["clips"])))
for who in ("ours", "valve"):
    g = geo[who]
    if g:
        L += ["", "## Geometry at rest: %s" % who, "", "Totals: " + ", ".join("%s %s" % kv for kv in g["totals"].items()), "",
              "Rest contacts by class pair (cm of intersection curve): " + ", ".join("%s %s" % kv for kv in sorted(g["by_class"].items(), key=lambda kv: -kv[1])), "",
              "Texel density (px/m, median, p5-p95, outliers): " + "; ".join("%s %s (%s-%s, %s)" % (m, s["median_px_per_m"], s["p5"], s["p95"], s["outliers"]) for m, s in g["texel_density"].items())]
cloth = jload(os.path.join(run, "cloth", "cloth.json"))
if cloth:
    rig = [r for r in cloth["ours"] if r["reads_rigid"]]
    L += ["", "## Cloth chains as compiled", "", "%d of our chains read rigid: %s. Full table: `cloth/cloth.md`." % (
        len(rig), ", ".join("%s (%s)" % (r["chain"], "over-damped, vertex attraction %.3f" % r["vertex_attraction_mean"]
                                         if r.get("over_damped") else "held %.0f%%" % (100 * r["held_share"])) for r in rig) or "none")]
geo_o = geo.get("ours") or {}
if geo_o.get("flat_swatches"):
    L += ["", "## Flat swatches (one palette colour, no texture), light colours first", "", "| piece | colour | luminance | px seen |", "|---|---|---|---|"]
    for r in geo_o["flat_swatches"][:15]:
        L.append("| %s | %s | %.2f | %.0f |" % (r["label"], r["colour_rgb"], r["luminance"], r["seen_px"]))
if geo_o.get("chain_pieces_skinned_rigid"):
    L += ["", "Pieces the designer classed chain but skinned rigid: " + ", ".join(r["label"] for r in geo_o["chain_pieces_skinned_rigid"])]
if prof:
    L += ["", "## Profile", "", open(os.path.join(run, "profile", "profile.md"), encoding="utf-8").read().split("\n", 2)[2].split("| texture |")[0].strip()]
if diff:
    L += ["", "## Silhouette and pixel diffs (ours against Valve's, matched cameras)", "", "| view | ours adds | ours loses | mean abs diff |", "|---|---|---|---|"]
    for k, s in sorted(diff.items()):
        if k.split("/")[1].startswith(("lookdev", "xray")):
            L.append("| %s | %.1f%% | %.1f%% | %.1f |" % (k, s["ours_only_pct"], s["valve_only_pct"], s["mean_abs_diff"]))
L += ["", "## Sheets", ""] + ["- `sheets/%s`" % v for v in sheets.values() if v]
L += ["- `views/diff/%s`" % os.path.basename(p) for p in sorted(glob.glob(os.path.join(run, "views", "diff", "sheet_*.png")))]
open(os.path.join(run, "index.md"), "w", encoding="utf-8").write("\n".join(L) + "\n")
json.dump({"findings": findings, "families": fam_rows, "holes": hole_findings, "sheets": sheets}, open(os.path.join(run, "findings.json"), "w"), indent=1)
print("REPORT_DONE", len(findings), "findings,", sum(1 for f in findings if f["grade"] == "defect"), "defects;", len(sheets), "sheets")

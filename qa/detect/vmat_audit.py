"""Material audit: every compiled material a mod ships, held against the materials Valve ships for the same job.

Reads compiled `.vmat_c` files through `magician dump` (no decompiler, no Blender). Two families:

Hero-model materials (the ones the mod's hero model references), against Valve's own materials of that hero
(the reference set is what the game's copy of the same model references):
  V01 status-proxy     F_USE_STATUS_EFFECTS_PROXY off where every Valve hero material has it. Status effects
                       (Shadow Weave, Cosmic Veil, damage flashes) skip the surface: it stays visible while the
                       hero is invisible. Error.
  V02 npr-lighting     an opaque hero material without F_USE_NPR_LIGHTING while Valve's have it: the piece is lit
                       by a different model from the rest of the hero. Warning.
  V03 outline-holes    F_ALPHA_TEST with a non-zero NPR outline mask: the outline traces every cut-out (lace,
                       eyelets) and turns into a speckle at distance. Warning.
  V04 self-illum       F_SELF_ILLUM on an opaque hero material where Valve's opaque hero materials have none:
                       the surface ignores shadow and reads in the dark. Note, with the mask texture named.
  V05 translucent      F_TRANSLUCENT or F_ADDITIVE_BLEND on a hero material that is not a glint/sparkle card
                       (Valve's only translucent hero material is lash_sparkles): sorting and status-effect risk.
  V06 no-ao            an opaque hero material with the default AO texture where Valve's own cloth/gear carry an
                       AO map: no occlusion in folds and linings (flat panels). Note.
  V07 transmissive     an opaque fabric material carrying the hero's skin transmissive colour (light through
                       thin skin): linings and folds glow lilac at grazing light. Note.
  V08 outline-tint     g_vSolidOutlineTint differs from every Valve hero material. Note.

Effect materials (on particles or props the mod ships):
  V20 particle-model-unlit  an opaque, lit pbr.vfx material drawn by a particle's C_OP_RenderModels without
                       F_USE_NPR_LIGHTING (and without self-illum). Valve's own opaque particle props use the
                       NPR path (bookworm_book_dragon.vmat) or self-illum; Maid Lash's porcelain shards
                       rendered black in game without it. Warning.

usage: python -I vmat_audit.py --vpk <mod.vpk> --game <pak01_dir.vpk> --model <path.vmdl_c> [--out f.json]
Exit 1 on any error-level finding.
"""
import argparse, json, os, re, subprocess, sys

MAGICIAN = os.path.normpath(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", "target", "release", "magician.exe" if os.name == "nt" else "magician"))
DEFAULT_AO = "materials/default/default_ao"
DEFAULT_MASK = "materials/default/default_mask"


def run(*a):
    r = subprocess.run([MAGICIAN] + list(a), capture_output=True, text=True, encoding="utf-8", errors="replace")
    return r.stdout


def parse_vmat(text):
    m = {"shader": "", "int": {}, "float": {}, "vec": {}, "tex": {}, "dyn": []}
    s = re.search(r'm_shaderName = "([^"]*)"', text)
    m["shader"] = s.group(1) if s else ""
    def block(name):
        i = text.find(name)
        if i < 0:
            return ""
        j = text.find("\n\tm_", i + len(name))
        return text[i:j if j > 0 else len(text)]
    for k, v in re.findall(r'm_name = "(\w+)"\s*m_nValue = (-?\d+)', block("m_intParams")):
        m["int"][k] = int(v)
    for k, v in re.findall(r'm_name = "(\w+)"\s*m_flValue = ([-\d.e]+)', block("m_floatParams")):
        m["float"][k] = float(v)
    for k, v in re.findall(r'm_name = "(\w+)"\s*m_value = \[([^\]]*)\]', block("m_vectorParams")):
        m["vec"][k] = [float(x) for x in v.split(",")]
    for k, v in re.findall(r'm_name = "(\w+)"\s*m_pValue = "([^"]*)"', block("m_textureParams")):
        m["tex"][k] = v
    m["dyn"] = re.findall(r'm_name = "(\w+)"', block("m_dynamicParams"))
    return m


def material_refs(vpk, model):
    out = run("refs", vpk, model)
    return [l.split()[-1] for l in out.splitlines() if l.strip().endswith(".vmat_c")]


def load(vpk, path):
    return parse_vmat(run("dump", vpk, path))


def flag(m, f):
    return m["int"].get(f, 0) == 1


def is_card(path, m):
    return (flag(m, "F_ADDITIVE_BLEND") or flag(m, "F_TRANSLUCENT")) and any(k in path for k in ("glint", "sparkle"))


def audit_hero(vpk, game, model):
    ours_paths = material_refs(vpk, model)
    valve_paths = material_refs(game, model)
    ours = {p: load(vpk, p) for p in ours_paths if p.split("/")[0] in ("materials", "models") and run("ls", vpk, p).count(p)}
    valve = {p: load(game, p) for p in valve_paths}
    vopaque = {p: m for p, m in valve.items() if not (flag(m, "F_TRANSLUCENT") or flag(m, "F_ADDITIVE_BLEND"))}
    every_proxy = valve and all(flag(m, "F_USE_STATUS_EFFECTS_PROXY") for m in valve.values())
    npr_valve = vopaque and all(flag(m, "F_USE_NPR_LIGHTING") for m in vopaque.values())
    valve_illum = any(flag(m, "F_SELF_ILLUM") for m in vopaque.values())
    valve_tints = {tuple(round(x, 3) for x in m["vec"].get("g_vSolidOutlineTint", [])) for m in valve.values()}
    valve_ao = any(not m["tex"].get("g_tAmbientOcclusion", DEFAULT_AO).startswith(DEFAULT_AO) for m in vopaque.values())
    skin_trans = {tuple(round(x, 3) for x in m["vec"].get("TextureNprTramsissiveColor1", [0, 0, 0, 0])[:3]) for m in vopaque.values()}
    F = []
    def add(code, sev, path, msg):
        F.append({"code": code, "severity": sev, "path": path, "message": msg})
    for p, m in ours.items():
        translucent = flag(m, "F_TRANSLUCENT") or flag(m, "F_ADDITIVE_BLEND")
        if every_proxy and not flag(m, "F_USE_STATUS_EFFECTS_PROXY"):
            add("V01", "error", p, "no F_USE_STATUS_EFFECTS_PROXY; every one of Valve's %d materials on this model has it" % len(valve))
        if not translucent and npr_valve and not flag(m, "F_USE_NPR_LIGHTING"):
            add("V02", "warn", p, "opaque without F_USE_NPR_LIGHTING; Valve's opaque materials on this model all use it")
        if flag(m, "F_ALPHA_TEST"):
            mask = m["vec"].get("TextureNprOutlineMask1")
            tex = m["tex"].get("g_tNprOutlineMask", "")
            if (mask and max(mask[:3]) > 0) or (tex and not mask):
                add("V03", "warn", p, "alpha-tested with NPR outline mask %s: the outline traces every cut-out (ref %.2f)"
                    % (mask[:3] if mask else tex, m["float"].get("g_flAlphaTestReference1", 0.5)))
        if not translucent and flag(m, "F_SELF_ILLUM") and not valve_illum:
            add("V04", "note", p, "opaque self-illum (mask %s, scale %s); Valve's opaque materials on this model have none"
                % (m["tex"].get("g_tSelfIllumMask", "?").split("/")[-1], m["float"].get("g_flSelfIllumScale1", "?")))
        if translucent and not is_card(p, m):
            add("V05", "warn", p, "translucent/additive hero material that is not a glint card")
        if not translucent and valve_ao and m["tex"].get("g_tAmbientOcclusion", DEFAULT_AO).startswith(DEFAULT_AO):
            add("V06", "note", p, "default AO texture; Valve's gear and body on this model carry AO maps")
        tr = tuple(round(x, 3) for x in m["vec"].get("TextureNprTramsissiveColor1", [0, 0, 0, 0])[:3])
        fabric = re.search(r"cloth|lace|ribbon|satin|fabric|silk|velvet", p.split("/")[-1]) and p not in valve
        if not translucent and any(tr) and tr in skin_trans and fabric:
            add("V07", "note", p, "carries Valve's transmissive colour %s (light through thin skin/gear) on a fabric material" % (list(tr),))
        tint = tuple(round(x, 3) for x in m["vec"].get("g_vSolidOutlineTint", []))
        if tint and valve_tints and tint not in valve_tints:
            add("V08", "note", p, "outline tint %s matches no Valve material on this model" % (list(tint),))
    return F, {"ours": sorted(ours), "valve": sorted(valve)}


def audit_fx(vpk, game):
    """Opaque lit materials drawn by particle model renderers."""
    F = []
    listing = run("ls", vpk, "particles")
    for line in listing.splitlines():
        p = line.split("\t")[0]
        if not p.endswith(".vpcf_c"):
            continue
        t = run("dump", vpk, p)
        if "C_OP_RenderModels" not in t:
            continue
        for mat in re.findall(r'm_hOverrideMaterial = "([^"]+)"', t):
            mp = mat + "_c"
            if mp not in run("ls", vpk, mp):
                continue                      # Valve's own material, not ours
            m = load(vpk, mp)
            lit_opaque = m["shader"] == "pbr.vfx" and not any(flag(m, f) for f in ("F_TRANSLUCENT", "F_ADDITIVE_BLEND", "F_SELF_ILLUM"))
            if lit_opaque and not flag(m, "F_USE_NPR_LIGHTING"):
                F.append({"code": "V20", "severity": "warn", "path": mp, "message":
                          "lit opaque pbr.vfx drawn by C_OP_RenderModels in %s without F_USE_NPR_LIGHTING or self-illum "
                          "(Valve's opaque particle props use the NPR path); renders black in game" % p})
    return F


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--vpk", required=True)
    ap.add_argument("--game", required=True)
    ap.add_argument("--model", required=True)
    ap.add_argument("--out")
    a = ap.parse_args()
    F, sets = audit_hero(a.vpk, a.game, a.model)
    F += audit_fx(a.vpk, a.game)
    res = {"materials": sets, "findings": F}
    if a.out:
        open(a.out, "w").write(json.dumps(res, indent=1))
    for f in F:
        print("%-5s %-4s %s\n      %s" % (f["severity"], f["code"], f["path"], f["message"]))
    print("%d findings (%d error, %d warn, %d note)" % (len(F), sum(f["severity"] == "error" for f in F),
          sum(f["severity"] == "warn" for f in F), sum(f["severity"] == "note" for f in F)))
    return 1 if any(f["severity"] == "error" for f in F) else 0


if __name__ == "__main__":
    sys.exit(main())

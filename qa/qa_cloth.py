"""QA cloth: how free each compiled cloth chain really is, ours against Valve's, from the compiled FeModel.

  python -I qa_cloth.py --vpk <mod pak or qa_input.vpk> --model <path in the pak> --out <dir>

Offline posing cannot simulate cloth, so a chain that the game will hold rigid is invisible to the motion sweep.
This reads the compiled model's FeModel (Source 2 Viewer -b PHYS) for ours and Valve's shipped model and, per
chain (nodes grouped by their bone name without the trailing joint index), reports:
  nodes, static (pinned) nodes, position-driven nodes (moved by the animation only: rigid by construction),
  free nodes, and over the free nodes the mean and max goal attraction (flAnimationForceAttraction = goal_strength
  cubed: 1 follows the animated pose exactly), vertex attraction (damping toward the pose), gravity.
A chain whose free nodes average a goal attraction of 0.5 or more (goal strength 0.8+), whose nodes are mostly
static or position-driven, or whose vertex attraction (goal damping) is over 3x the highest of Valve's chains
("over-damped": its velocity is pulled onto the body's, so it swings with the body instead of trailing it) moves
with its bones like a skinned piece: "reads rigid". Valve's own chains (neckties,
hair, wraps) are the calibration: Lash's free nodes sit at goal 0.1-0.15 (attraction 0.001-0.003).
Writes <out>/cloth.json and <out>/cloth.md.
"""
import argparse, json, os, re, subprocess, sys
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "ingame"))
import gamepaths  # noqa: E402  (locations as magician.exe resolves them)

VRF = gamepaths.vrf()
GAME_PAK = os.path.join(gamepaths.try_game(), "citadel", "pak01_dir.vpk")
ap = argparse.ArgumentParser()
ap.add_argument("--vpk", required=True)
ap.add_argument("--model", required=True)
ap.add_argument("--out", required=True)
A = ap.parse_args()
os.makedirs(A.out, exist_ok=True)

def fe(vpk):
    t = subprocess.run([VRF, "-i", vpk, "-f", A.model, "-b", "PHYS"], capture_output=True, text=True, errors="replace").stdout
    names = re.findall(r'"([^"]+)"', re.search(r"m_CtrlName = \s*\[(.*?)\]", t, re.S).group(1))
    integ = re.search(r"m_NodeIntegrator = \s*\[(.*?)\n\t\t\]", t, re.S).group(1)
    nodes = []
    for blk in re.findall(r"\{(.*?)\}", integ, re.S):
        g = lambda k: float((re.search(k + r" = ([-\d.eE+]+)", blk) or [0, "0"])[1])
        nodes.append({"damp": g("flPointDamping"), "goal": g("flAnimationForceAttraction"),
                      "vert": g("flAnimationVertexAttraction"), "grav": g("flGravity")})
    gi = lambda k: int((re.search(k + r" = (\d+)", t) or [0, "0"])[1])
    return names, nodes, gi("m_nStaticNodes"), gi("m_nFirstPositionDrivenNode") or len(names), gi("m_nNodeCount")

def chains(vpk):
    names, nodes, nstatic, first_pd, ncount = fe(vpk)
    out = {}
    for i, n in enumerate(names[:ncount]):
        key = re.sub(r"^\$cc", "", n)
        key = re.sub(r"(_\d+)+(_[LRlr])?$", r"\2", key)        # joint index off: maid_train_l_3 -> maid_train_l
        key = re.sub(r"_end(_[LR])?$", r"\1", key)
        c = out.setdefault(key, {"nodes": 0, "static": 0, "driven": 0, "free": [], "names": []})
        c["nodes"] += 1
        c["names"].append(n)
        if i < nstatic:
            c["static"] += 1
        elif i >= first_pd:
            c["driven"] += 1
        elif i < len(nodes):
            c["free"].append(nodes[i])
    rows = []
    for k, c in out.items():
        f = c["free"]
        mean = lambda key: round(sum(x[key] for x in f) / len(f), 4) if f else None
        r = {"chain": k, "nodes": c["nodes"], "static": c["static"], "position_driven": c["driven"], "free": len(f),
             "goal_mean": mean("goal"), "goal_max": round(max((x["goal"] for x in f), default=0), 4),
             "vertex_attraction_mean": mean("vert"), "gravity_mean": mean("grav"), "damping_mean": mean("damp")}
        if c["nodes"] == 1 and c["static"] == 1:
            continue                     # a collision shape's anchor node on a body bone, not a chain
        moving = len(f)
        r["held_share"] = round((c["static"] + c["driven"]) / c["nodes"], 2)
        r["reads_rigid"] = bool(moving == 0 or (r["goal_mean"] or 0) >= 0.5 or moving < 0.5 * (c["nodes"] - c["static"]))
        rows.append(r)
    return sorted(rows, key=lambda r: r["chain"])

ours, valve = chains(A.vpk), chains(GAME_PAK)
# over-damped: vertex attraction (goal damping) pulls a node's velocity onto the animated pose's, so the chain swings
# with the body instead of lagging behind it. Calibrated on Valve's own free nodes
vmax = max((r["vertex_attraction_mean"] or 0) for r in valve)
for r in ours:
    r["over_damped"] = bool((r["vertex_attraction_mean"] or 0) > 3 * vmax)
    r["reads_rigid"] = r["reads_rigid"] or r["over_damped"]
json.dump({"ours": ours, "valve": valve}, open(os.path.join(A.out, "cloth.json"), "w"), indent=1)
L = ["# Cloth chains as compiled: ours and Valve's", "",
     "| model | chain | nodes | static | position-driven | free | goal mean | goal max | vertex attr. | gravity | over-damped | reads rigid |",
     "|---|---|---|---|---|---|---|---|---|---|---|---|"]
for side, rows in (("ours", ours), ("valve", valve)):
    for r in rows:
        L.append("| %s | %s | %d | %d | %d | %d | %s | %s | %s | %s | %s | %s |" % (side, r["chain"], r["nodes"], r["static"], r["position_driven"],
                 r["free"], r["goal_mean"], r["goal_max"], r["vertex_attraction_mean"], r["gravity_mean"],
                 "yes" if r.get("over_damped") else "", "**yes**" if r["reads_rigid"] else ""))
open(os.path.join(A.out, "cloth.md"), "w", encoding="utf-8").write("\n".join(L) + "\n")
print("CLOTH_DONE ours %d chains (%d read rigid), Valve %d" % (len(ours), sum(r["reads_rigid"] for r in ours), len(valve)))

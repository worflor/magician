"""Texture parity: every texture a mod ships at one of Valve's paths, against Valve's file at that path.

A replaced texture is drawn by Valve's own panels, materials and code at Valve's sizes. When ours drops the mip
chain (Valve's has one), it shimmers and aliases wherever the game draws it smaller than its size (hero cards on the
scoreboard, the hero grid, the kill feed); when ours changes the format (BGRA8888 to DXT5), block artefacts appear on
flat colour and fine line work that Valve kept uncompressed; when ours changes the size, Valve's layout scales it.

Reads VTEX headers through `magician dump` (size, format, mip count). No pixels are decoded.

usage: python -I texture_parity.py --vpk <mod.vpk> --game <pak01_dir.vpk> [--prefix panorama/] [--out f.json]
Findings: T01 fewer mips than Valve (warn), T02 a different format (warn; error when Valve is uncompressed and ours
is block-compressed below 256 px on a side), T03 a different size (warn). Exit 1 on an error.
"""
import argparse, json, os, re, subprocess, sys

MAGICIAN = os.path.normpath(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", "target", "release", "magician.exe"))
HDR = re.compile(r": (\d+)x(\d+)x(\d+) (\S+) \(format (\d+), (\d+) mips")
UNCOMPRESSED = {"BGRA8888", "RGBA8888"}


def run(*a):
    return subprocess.run([MAGICIAN] + list(a), capture_output=True, text=True, encoding="utf-8", errors="replace").stdout


def header(vpk, path):
    m = HDR.search(run("dump", vpk, path))
    if not m:
        return None
    w, h, d, fmt, code, mips = m.groups()
    return {"w": int(w), "h": int(h), "format": fmt, "mips": int(mips)}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--vpk", required=True)
    ap.add_argument("--game", required=True)
    ap.add_argument("--prefix", default="")
    ap.add_argument("--out")
    a = ap.parse_args()
    F = []
    for line in run("ls", a.vpk, a.prefix).splitlines():
        p = line.split("\t")[0]
        if not p.endswith(".vtex_c"):
            continue
        ours = header(a.vpk, p)
        valve = header(a.game, p)
        if not ours or not valve:
            continue                         # a new file, not a replacement
        if ours["mips"] < valve["mips"]:
            F.append({"code": "T01", "severity": "warn", "path": p,
                      "message": "%d mips, Valve's %d: aliases wherever the game draws it smaller" % (ours["mips"], valve["mips"])})
        if ours["format"] != valve["format"]:
            sev = "error" if valve["format"] in UNCOMPRESSED and ours["format"] not in UNCOMPRESSED and min(ours["w"], ours["h"]) < 256 else "warn"
            F.append({"code": "T02", "severity": sev, "path": p, "message": "format %s, Valve's %s" % (ours["format"], valve["format"])})
        if (ours["w"], ours["h"]) != (valve["w"], valve["h"]):
            F.append({"code": "T03", "severity": "warn", "path": p,
                      "message": "%dx%d, Valve's %dx%d" % (ours["w"], ours["h"], valve["w"], valve["h"])})
    if a.out:
        open(a.out, "w").write(json.dumps(F, indent=1))
    for f in F:
        print("%-5s %s %s\n      %s" % (f["severity"], f["code"], f["path"], f["message"]))
    print("%d findings" % len(F))
    return 1 if any(f["severity"] == "error" for f in F) else 0


if __name__ == "__main__":
    sys.exit(main())

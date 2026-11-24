"""Colour grades: named looks as ffmpeg filter fragments, applied per shot by the encode (film.py assemble).

    [scene] grade = "parlour"          # every shot
    [[shot]] grade = "brawler"         # this shot ("none": as captured)

    python -I grade.py stills <out.jpg> <frame.png> [frame2.png ...] [--looks parlour,brawler,fever]
        # a sheet: each frame as captured, then in every look (the look book)

A look is a chain of plain ffmpeg filters on rgb24 frames. Where a look needs a second branch (bloom: a blurred,
thresholded copy screened back over the frame) the fragment carries its own labels, made unique per shot.

The trailer's looks (Maid Lash, 2026-11-21) mark the two Lashes of the trailer's brief:
- parlour: Lash *playing* Maid Lash. The service persona switched on: candy warmth, rose-lifted shadows, pearly
  highlights with a soft bloom, a gentle vignette. Confection, not softness: contrast stays up, so the muscle and
  the moustache keep their edges.
- brawler: main Lash. The persona off: steel shadows, warm skin, a firm curve, film grain, no bloom. Fight-poster,
  Bear Pit. (Cut 2 lifted it: cut 1's crushed blacks lost him on the night street.)
- fever: the leap, the flip and the camera grab: the fever dream. High key, saturated, a magenta and cyan split
  tone, a strong bloom (no chromatic fringe since cut 3: it softened the lace).
"""
import os
import subprocess
import sys

LOOKS = {
    "none": {"chain": [], "bloom": None},
    "parlour": {
        # cut 5: the shadows lifted less (cut 4's 0.08-0.09 floor read as a purple haze at phone size, the café the
        # murkiest part of the film) and the mids opened: still rose in the shadows, but with blacks and a face
        "chain": ["curves=r='0/0.05 0.25/0.32 0.5/0.61 0.75/0.84 1/1'"
                  ":g='0/0.02 0.25/0.25 0.5/0.53 0.75/0.78 1/0.97'"
                  ":b='0/0.055 0.25/0.29 0.5/0.55 0.75/0.77 1/0.96'",
                  "eq=contrast=1.1:saturation=1.2:gamma=1.1",
                  "colorbalance=rm=0.04:bm=0.02:gm=-0.02:rh=0.03:bh=-0.02",
                  "vignette=angle=PI/5:mode=forward"],
        "bloom": {"sigma": 20, "knee": 0.55, "opacity": 0.45},
    },
    "brawler": {
        # cut 2: lifted. Cut 1's curve (0.12 -> 0.07, contrast 1.12, saturation 0.88) took the night street to black
        # and lost him in it; the steel, the warm skin and the grain stay, the shadows open and the mids lift
        "chain": ["curves=all='0/0.015 0.10/0.12 0.30/0.36 0.5/0.57 0.85/0.92 1/1'",
                  "colorbalance=rs=-0.05:gs=-0.01:bs=0.06:rh=0.05:gh=0.01:bh=-0.04",
                  "eq=contrast=1.05:saturation=0.98:gamma=1.06",
                  "noise=alls=6:allf=t",
                  "vignette=angle=PI/5"],
        "bloom": None,
    },
    "fever": {
        "chain": ["eq=brightness=0.07:contrast=1.02:saturation=1.55:gamma=1.22",
                  "colorbalance=rs=0.10:bs=0.16:gs=-0.08:rm=0.06:gm=-0.04:bm=0.04:rh=0.06:bh=0.08",
                  # no chromatic fringe since cut 3: even 1 px softened the lace and the embroidery (the author's
                  # reviewer); the fever is the colour, the split tone and the bloom
                  "vignette=angle=PI/4.5"],
        "bloom": {"sigma": 30, "knee": 0.42, "opacity": 0.62},
    },
}


class GradeError(ValueError):
    pass


def fragment(look, src, dst, tag):
    """The filter-graph text that grades [src] into [dst] (labels unique by `tag`)."""
    if look not in LOOKS:
        raise GradeError(f"grade {look!r}: one of {', '.join(sorted(LOOKS))}")
    g = LOOKS[look]
    chain = list(g["chain"]) or ["null"]
    b = g["bloom"]
    if not b:
        return f"[{src}]{','.join(chain)}[{dst}]"
    k = b["knee"]
    # bloom: everything above the knee, blurred, screened back over the graded frame
    return (f"[{src}]{','.join(chain)},split[{tag}a][{tag}b];"
            f"[{tag}b]curves=all='0/0 {k:.3f}/0 1/1',gblur=sigma={b['sigma']}[{tag}c];"
            f"[{tag}a][{tag}c]blend=all_mode=screen:all_opacity={b['opacity']},format=rgb24[{dst}]")


def stills(out, frames, looks=("parlour", "brawler", "fever"), width=640):
    """A look book: one row a frame, the capture then each look, labelled."""
    font = "C\\:/Windows/Fonts/segoeuib.ttf"
    args, chains, rows = [], [], []
    for i, f in enumerate(frames):
        args += ["-i", f]
        cols = ["none"] + list(looks)
        chains.append(f"[{i}:v]scale={width}:-2:flags=lanczos,format=rgb24,split={len(cols)}"
                      + "".join(f"[r{i}c{j}]" for j in range(len(cols))))
        for j, look in enumerate(cols):
            chains.append(fragment(look, f"r{i}c{j}", f"g{i}c{j}", f"t{i}x{j}"))
            label = "as captured" if look == "none" else look
            chains.append(f"[g{i}c{j}]drawtext=fontfile='{font}':text='{label}':x=10:y=10:fontsize=20:"
                          f"fontcolor=white:box=1:boxcolor=black@0.45:boxborderw=4[l{i}c{j}]")
        chains.append("".join(f"[l{i}c{j}]" for j in range(len(cols))) + f"hstack=inputs={len(cols)}[row{i}]")
        rows.append(f"[row{i}]")
    if len(rows) > 1:
        chains.append("".join(rows) + f"vstack=inputs={len(rows)}[out]")
    else:
        chains.append(f"{rows[0]}null[out]")
    subprocess.run(["ffmpeg", "-y", "-hide_banner", "-loglevel", "error"] + args +
                   ["-filter_complex", ";".join(chains), "-map", "[out]", "-frames:v", "1", "-q:v", "3", out], check=True)
    return out


def main(a):
    if len(a) < 3 or a[0] != "stills":
        sys.exit(__doc__)
    looks = a[a.index("--looks") + 1].split(",") if "--looks" in a else ["parlour", "brawler", "fever"]
    frames = [x for x in a[2:] if not x.startswith("--") and x not in looks and os.path.exists(x)]
    print(stills(a[1], frames, looks))


if __name__ == "__main__":
    main(sys.argv[1:])

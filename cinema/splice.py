"""Splice re-rendered shots into a delivered master, and mix the scene's sound again (written for the Maid Lash
trailer's late fixes; several ranges in one pass).

    python -I splice.py <scene.toml> <patch master.mkv> <patch first frame> <film first frame> <frames>
    python -I splice.py <scene.toml> <patch master.mkv> <p0>:<f0>:<n> [<p0>:<f0>:<n> ...]

`film.py encode` removes the shot frames once the lossless master is written, so a late fix to one shot would mean
rendering the whole film again. This takes `n` frames of a patch master (a scene of just those shots, encoded with
`--keep-frames`: graded and cropped the way the film grades it) from patch frame `p0`, puts them over the film's
master from film frame `f0` (each range in turn; ranges may not overlap), mixes the scene's [music] and [[sfx]] again
(so sound edits made since the encode land too), and writes master.mkv in place (the old one kept as
master_before_splice.mkv until the new one is written) with the scene's edit files beside it. The frame count of
the master never changes. To splice into a new cut's folder, copy the old cut's master there first. Then
`film.py lossy <scene.toml>` makes the preview and the phone copy from it.
"""
import os
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import film  # noqa: E402
import scene as scn  # noqa: E402


def parse_ranges(args):
    """[(p0, f0, n), ...] from either form, sorted by film frame; overlapping or empty ranges are refused."""
    if len(args) == 3 and all(":" not in a for a in args):
        ranges = [tuple(int(v) for v in args)]
    else:
        ranges = []
        for a in args:
            parts = a.split(":")
            if len(parts) != 3:
                raise ValueError(f"a range is p0:f0:n, not {a!r}")
            ranges.append(tuple(int(v) for v in parts))
    ranges.sort(key=lambda r: r[1])
    for p0, f0, n in ranges:
        if n <= 0 or p0 < 0 or f0 < 0:
            raise ValueError(f"range {p0}:{f0}:{n}: frames must be positive")
    for a, b in zip(ranges, ranges[1:]):
        if a[1] + a[2] > b[1]:
            raise ValueError(f"ranges {a} and {b} overlap in the film")
    return ranges


def splice_graph(ranges, total):
    """The ffmpeg video graph: the film's master is input 0, the patch input 1; output [v], `total` frames."""
    parts, labels, at = [], [], 0
    for i, (p0, f0, n) in enumerate(ranges):
        if f0 > at:
            parts.append(f"[0:v]trim=start_frame={at}:end_frame={f0},setpts=PTS-STARTPTS[k{i}]")
            labels.append(f"[k{i}]")
        parts.append(f"[1:v]trim=start_frame={p0}:end_frame={p0 + n},setpts=PTS-STARTPTS[p{i}]")
        labels.append(f"[p{i}]")
        at = f0 + n
    if at < total:
        parts.append(f"[0:v]trim=start_frame={at},setpts=PTS-STARTPTS[tail]")
        labels.append("[tail]")
    # an input stream may feed several trims (as the one-range form always did): each trim drops what it does not keep
    return ";".join(parts + ["".join(labels) + f"concat=n={len(labels)}:v=1:a=0[v]"])


def count_frames(path):
    return int(subprocess.run(["ffprobe", "-v", "error", "-count_packets", "-select_streams", "v:0", "-show_entries",
                               "stream=nb_read_packets", "-of", "csv=p=0", path], capture_output=True, text=True,
                              check=True).stdout.strip())


def main(a):
    if len(a) < 3:
        sys.exit(__doc__)
    sc = scn.load(a[0])
    patch = a[1]
    try:
        ranges = parse_ranges(a[2:])
    except ValueError as e:
        sys.exit(f"splice: {e}")
    dest = film.out_dir(sc)
    master = os.path.join(dest, "master.mkv")
    old = os.path.join(dest, "master_before_splice.mkv")
    total = count_frames(master)
    have = count_frames(patch)
    for p0, f0, n in ranges:
        if p0 + n > have or f0 + n > total:
            sys.exit(f"splice: range {p0}:{f0}:{n} runs past the patch ({have} frames) or the film ({total})")
    if os.path.exists(old):
        os.remove(old)
    os.replace(master, old)
    length = float(subprocess.run(["ffprobe", "-v", "error", "-show_entries", "format=duration", "-of", "csv=p=0", old],
                                  capture_output=True, text=True, check=True).stdout.strip())
    graph = splice_graph(ranges, total)
    args = ["-i", old, "-i", patch]
    amap = []
    sound = film.audio_mix(sc, length, 2)
    if sound:
        args += sound[0]
        graph += ";" + sound[1]
        amap = ["-map", f"[{sound[2]}]", "-c:a", "flac"]
    subprocess.run(["ffmpeg", "-y", "-hide_banner", "-loglevel", "error"] + args + ["-filter_complex", graph,
                    "-map", "[v]", "-r", str(sc.fps), "-c:v", "ffv1", "-level", "3", "-pix_fmt", "bgr0"] + amap + [master],
                   check=True)
    frames = count_frames(master)
    for p0, f0, n in ranges:
        print(f"SPLICE {master}: frames {f0}-{f0 + n - 1} from {patch} {p0}-{p0 + n - 1}")
    print(f"SPLICE {frames} frames (was {total}); sound mixed again")
    if frames != total:
        sys.exit(f"splice: the master has {frames} frames, not {total}: check it before anything else")
    # the edit files follow the scene (a spliced shot may come from another take, at other ticks)
    print("SPLICE edit files: " + ", ".join(os.path.basename(p) for p in film.edit_files(sc, dest)))


if __name__ == "__main__":
    main(sys.argv[1:])

"""Stills and contact sheets from a recorded shot (replay.py film or record.py, a lossless mkv), through ffmpeg.

    python -I frames.py sheet <shot.mkv> <out.png> [--from S] [--to S] [--n 12] [--cols 4] [--w 640] [--crop x:y:w:h]
    python -I frames.py still <shot.mkv> <out.png> <seconds> [--crop x:y:w:h]

Times are seconds of the recording (a replay.py shot opens with the console and the warm-up: its first frame of the
take is about 1.0 s + 1.5 s / demo_timescale in). --crop is in source pixels (x, y of the corner, then width and
height), applied before scaling. ingame/mod_session.py sheets runs `sheet` on every shot a session filmed.
"""
import subprocess
import sys


def opt(a, name, default):
    return a[a.index(name) + 1] if name in a else default


def duration(path):
    o = subprocess.run(["ffprobe", "-v", "error", "-show_entries", "format=duration", "-of", "csv=p=0", path],
                       capture_output=True, text=True)
    return float(o.stdout.strip() or 0)


def crop_filter(crop):
    """`x:y:w:h` -> ffmpeg's `crop=w:h:x:y,` (empty for none)."""
    if not crop:
        return ""
    x, y, w, h = crop.split(":")
    return f"crop={w}:{h}:{x}:{y},"


def sheet_filter(t0, t1, n, cols, width, crop=None):
    rows = (n + cols - 1) // cols
    fps = n / max(t1 - t0, 0.01)
    return "%sfps=%.5f,scale=%d:-1,tile=%dx%d" % (crop_filter(crop), fps, width, cols, rows)


def sheet(src, out, t0=0.0, t1=None, n=12, cols=4, width=640, crop=None):
    t1 = duration(src) if t1 is None else t1
    subprocess.run(["ffmpeg", "-y", "-loglevel", "error", "-ss", str(t0), "-t", str(t1 - t0), "-i", src, "-frames:v", "1",
                    "-vf", sheet_filter(t0, t1, n, cols, width, crop), out], check=True)


def still(src, out, at, crop=None):
    subprocess.run(["ffmpeg", "-y", "-loglevel", "error", "-ss", str(at), "-i", src, "-frames:v", "1",
                    "-vf", crop_filter(crop) + "null", out], check=True)


def main(a):
    if len(a) < 3 or a[0] not in ("sheet", "still") or (a[0] == "still" and len(a) < 4):
        sys.exit(__doc__)
    crop = opt(a, "--crop", None)
    if a[0] == "still":
        still(a[1], a[2], a[3], crop)
        return 0
    t1 = opt(a, "--to", None)
    sheet(a[1], a[2], float(opt(a, "--from", 0)), float(t1) if t1 is not None else None, int(opt(a, "--n", 12)),
          int(opt(a, "--cols", 4)), int(opt(a, "--w", 640)), crop)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))

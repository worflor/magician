"""Record the game window to video in real time (retail has no startmovie): ffmpeg off the desktop.

    python -I record.py <out.mkv> --seconds 12 [--fps 60] [--method ddagrab|gdigrab] [--codec lossless|nvenc|x264]
    python -I record.py <out.mkv> --seconds 12 --then "cin_slow"     # type a console line as the recording starts

ddagrab (Desktop Duplication, on the GPU) keeps up at 60 fps at 1920x1080; gdigrab (GDI) is the fallback and
drops frames above about 30. Both read the screen, so the game window must be on top and unobstructed (the
desktop app's floating windows end up in the frame; PrintWindow bursts in capture.py do not have that problem
but cannot hold a frame rate). Codecs: lossless = FFV1 in mkv (edit master, ~1 GB a minute at 1080p60 with
grain, much less on clean frames); nvenc = h264_nvenc at constant quality 16 (review copies); x264 = libx264
crf 14 on the CPU (competes with the game for cores: use only for slow-motion takes).
For a slow-motion hero moment, record at host_timescale 0.25 and keep the take as it is: the engine
interpolates every rendered frame, so 60 fps of capture is 240 fps of game time.
"""
import os
import subprocess
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(os.path.dirname(HERE), "ingame"))


def monitor_of(x, y):
    """Desktop Duplication output index and the window's offset inside that output."""
    import ctypes
    import ctypes.wintypes as wt
    user32 = ctypes.WinDLL("user32")
    mons = []
    PROC = ctypes.WINFUNCTYPE(wt.BOOL, wt.HMONITOR, wt.HDC, ctypes.POINTER(wt.RECT), wt.LPARAM)

    def cb(h, dc, r, _):
        mons.append((r.contents.left, r.contents.top, r.contents.right, r.contents.bottom))
        return True

    user32.EnumDisplayMonitors(None, None, PROC(cb), 0)
    for i, (l, t, r, b) in enumerate(mons):
        if l <= x < r and t <= y < b:
            return i, x - l, y - t
    return 0, x, y


def main(a):
    def opt(name, default, cast=str):
        return cast(a[a.index(name) + 1]) if name in a else default

    if not a or a[0].startswith("--"):
        sys.exit(__doc__)
    out = a[0]
    seconds, fps = opt("--seconds", 10, float), opt("--fps", 60, int)
    method, codec = opt("--method", "ddagrab"), opt("--codec", "lossless")
    from capture import find_window, client_rect
    hwnd = find_window()
    if not hwnd:
        sys.exit("record: no Deadlock window")
    x, y, w, h = client_rect(hwnd)
    if method == "ddagrab":
        idx, ox, oy = monitor_of(x, y)
        src = ["-f", "lavfi", "-i",
               f"ddagrab=output_idx={idx}:framerate={fps}:offset_x={ox}:offset_y={oy}:video_size={w}x{h}:draw_mouse=0"]
        pre = "hwdownload,format=bgra"
    else:
        src = ["-f", "gdigrab", "-framerate", str(fps), "-offset_x", str(x), "-offset_y", str(y),
               "-video_size", f"{w}x{h}", "-draw_mouse", "0", "-i", "desktop"]
        pre = "format=bgra"
    if codec == "lossless":
        enc = ["-vf", pre + ",format=bgr0", "-c:v", "ffv1", "-level", "3", "-g", "1", "-slices", "16"]
    elif codec == "nvenc":
        enc = ["-vf", pre + ",format=yuv420p", "-c:v", "h264_nvenc", "-preset", "p5", "-rc", "vbr", "-cq", "16", "-b:v", "0"]
    else:
        enc = ["-vf", pre + ",format=yuv420p", "-c:v", "libx264", "-preset", "veryfast", "-crf", "14"]
    os.makedirs(os.path.dirname(os.path.abspath(out)) or ".", exist_ok=True)
    # Desktop Duplication delivers a frame only when the screen changes: hold the rate constant (a paused frame
    # repeats) so the take's clock is the wall clock.
    cmd = (["ffmpeg", "-y", "-hide_banner", "-loglevel", "warning", "-stats"] + src + ["-t", str(seconds)] + enc
           + ["-fps_mode", "cfr", "-r", str(fps), out])
    print(" ".join(cmd))
    p = subprocess.Popen(cmd)
    if "--then" in a:
        from console import Console, focus_game
        time.sleep(0.8)
        focus_game()
        Console(opt("--key", None)).run(opt("--then", ""))
    p.wait()
    if p.returncode == 0:
        probe = subprocess.run(["ffprobe", "-v", "error", "-count_frames", "-select_streams", "v:0", "-show_entries",
                                "stream=nb_read_frames", "-of", "csv=p=0", out], capture_output=True, text=True)
        n = probe.stdout.strip()
        print(f"RECORD {out} {w}x{h} {n} frames in {seconds:g} s (asked {fps} fps: {float(n) / seconds:.1f})"
              if n.isdigit() else f"RECORD {out}")
    sys.exit(p.returncode)


if __name__ == "__main__":
    main(sys.argv[1:])

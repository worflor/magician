"""The film kit's capture on the harness's seam (gameos.py), against a window of this test's own, never the game.

    python test_capture.py          (needs an X display with XTest, e.g. xvfb-run -a python test_capture.py, and ffmpeg)

The window is named and classed as Proton names the game's. record.py finds it through capture.py, records it with
ffmpeg's x11grab for two seconds, and the frames hold the pixels drawn; film.py's frame grabs (capture.grab) return the
same pixels. On Windows the kit records with ddagrab or gdigrab, proven by real shoots; this test is for X11.
"""
import ctypes
import ctypes.util
import os
import shutil
import subprocess
import sys
import tempfile
import time
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(os.path.dirname(HERE), "ingame"))
import gameos  # noqa: E402

READY = gameos.LINUX and gameos.display() is not None and shutil.which("ffmpeg") and shutil.which("ffprobe")


@unittest.skipUnless(READY, "needs an X display with XTest and ffmpeg: " + str(gameos.display_problem()))
class Record(unittest.TestCase):
    BG, FG = 0x2050A0, 0xE0C020

    @classmethod
    def setUpClass(cls):
        x = cls.x = ctypes.CDLL(ctypes.util.find_library("X11") or "libX11.so.6")
        P, UL = ctypes.c_void_p, ctypes.c_ulong
        x.XOpenDisplay.restype = P
        x.XOpenDisplay.argtypes = [ctypes.c_char_p]
        x.XDefaultRootWindow.restype = UL
        x.XDefaultRootWindow.argtypes = [P]
        x.XCreateSimpleWindow.restype = UL
        x.XCreateSimpleWindow.argtypes = [P, UL, ctypes.c_int, ctypes.c_int, ctypes.c_uint, ctypes.c_uint, ctypes.c_uint, UL, UL]
        x.XInternAtom.restype = UL
        x.XInternAtom.argtypes = [P, ctypes.c_char_p, ctypes.c_int]
        x.XChangeProperty.argtypes = [P, UL, UL, UL, ctypes.c_int, ctypes.c_int, ctypes.c_char_p, ctypes.c_int]
        x.XMapRaised.argtypes = [P, UL]
        x.XDestroyWindow.argtypes = [P, UL]
        x.XCreateGC.restype = P
        x.XCreateGC.argtypes = [P, UL, UL, P]
        x.XSetForeground.argtypes = [P, P, UL]
        x.XFillRectangle.argtypes = [P, UL, P, ctypes.c_int, ctypes.c_int, ctypes.c_uint, ctypes.c_uint]
        x.XSync.argtypes = [P, ctypes.c_int]
        cls.d = x.XOpenDisplay(None)
        cls.win = x.XCreateSimpleWindow(cls.d, x.XDefaultRootWindow(cls.d), 30, 30, 320, 180, 0, 0, cls.BG)

        def prop(name, kind, fmt, data, n):
            x.XChangeProperty(cls.d, cls.win, x.XInternAtom(cls.d, name, 0), x.XInternAtom(cls.d, kind, 0), fmt, 0, data, n)

        title = b"Deadlock (film kit test window)"
        prop(b"WM_NAME", b"STRING", 8, title, len(title))
        cls_ = b"steam_app_1422450\0steam_app_1422450\0"
        prop(b"WM_CLASS", b"STRING", 8, cls_, len(cls_))
        x.XMapRaised(cls.d, cls.win)
        x.XSync(cls.d, 0)
        time.sleep(0.8)
        gc = x.XCreateGC(cls.d, cls.win, 0, None)
        x.XSetForeground(cls.d, gc, cls.FG)
        x.XFillRectangle(cls.d, cls.win, gc, 0, 0, 160, 180)   # left half yellow, right half blue
        x.XSync(cls.d, 0)
        time.sleep(0.3)
        cls.tmp = tempfile.mkdtemp(prefix="magician-film-")

    @classmethod
    def tearDownClass(cls):
        cls.x.XDestroyWindow(cls.d, cls.win)
        cls.x.XSync(cls.d, 0)
        shutil.rmtree(cls.tmp, ignore_errors=True)

    def color_at(self, png, px, py):
        sys.path.insert(0, os.path.join(os.path.dirname(HERE), "ingame"))
        from capture import read_png_rgb
        w, h, rows = read_png_rgb(png)
        r, g, b = rows[py][px * 3:px * 3 + 3]
        return (r << 16) | (g << 8) | b

    def test_record_finds_the_window_and_records_what_it_shows(self):
        mkv = os.path.join(self.tmp, "take.mkv")
        r = subprocess.run([sys.executable, "-I", os.path.join(HERE, "record.py"), mkv, "--seconds", "2", "--fps", "10"],
                           capture_output=True, text=True, timeout=120)
        self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
        self.assertIn("x11grab", r.stdout)
        frames = subprocess.run(["ffprobe", "-v", "error", "-count_frames", "-select_streams", "v:0", "-show_entries",
                                 "stream=nb_read_frames,width,height", "-of", "csv=p=0", mkv], capture_output=True, text=True).stdout
        w, h, n = (int(v) for v in frames.strip().split(","))
        self.assertEqual((w, h), (320, 180))
        self.assertGreaterEqual(n, 18, "two seconds at 10 fps")
        # the frame as raw pixels: capture.py's PNG reader reads only the PNGs capture.py writes (unfiltered rows)
        px = subprocess.run(["ffmpeg", "-v", "error", "-i", mkv, "-frames:v", "1", "-f", "rawvideo", "-pix_fmt", "rgb24", "-"],
                            check=True, capture_output=True).stdout
        self.assertEqual(len(px), w * h * 3)

        def at(x, y):
            return int.from_bytes(px[(y * w + x) * 3:(y * w + x) * 3 + 3], "big")

        self.assertEqual(at(40, 90), self.FG)
        self.assertEqual(at(280, 90), self.BG)

    def test_a_frame_grab_is_the_pixels_drawn(self):
        from capture import find_window, client_rect, grab, write_png
        w = find_window()
        x, y, width, height = client_rect(w)
        png = os.path.join(self.tmp, "grab.png")
        write_png(png, width, height, grab(x, y, width, height, w))
        self.assertEqual(self.color_at(png, 40, 90), self.FG)
        self.assertEqual(self.color_at(png, 280, 90), self.BG)


if __name__ == "__main__":
    unittest.main()

"""The operating-system seam the harness stands on (gameos.py), against things this test makes itself, never the game.

    python -I test_gameos.py

Everywhere: the rule that tells the game's process from others, and this process's own start time. On Linux: a
process named deadlock.exe that this test starts is found in /proc, stopped and waited for, and a file is held open
and let go. With an X display (DISPLAY, libX11 and libXtst; Xvfb or XWayland will do): a window this test opens,
named and classed as Proton names the game's, is found, measured, brought forward and captured pixel for pixel, and
keys, a mouse move, a button and the wheel sent through XTest arrive in it. The Windows display backend is today's
SendInput and PrintWindow, driven by a real session (ingame/README.md); this test does not open windows there.
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
sys.path.insert(0, HERE)
import gameos  # noqa: E402


class Processes(unittest.TestCase):
    def test_the_game_is_found_by_its_program_not_its_arguments(self):
        self.assertTrue(gameos.is_game_process("deadlock.exe\n", ""))
        self.assertTrue(gameos.is_game_process("wine64-preload", "/home/u/.steam/proton/files/bin/wine64-preload\0"
                                               "Z:\\home\\u\\Deadlock\\game\\bin\\win64\\deadlock.exe\0-novid\0"))
        self.assertTrue(gameos.is_game_process("", "Z:\\steamapps\\common\\Deadlock\\game\\bin\\win64\\project8.exe\0"))
        self.assertFalse(gameos.is_game_process("vim", "vim\0notes/deadlock.exe\0"))
        self.assertFalse(gameos.is_game_process("bash", "bash\0-c\0echo\0deadlock.exe"))
        self.assertFalse(gameos.is_game_process("steam", "/usr/bin/steam\0"))

    def test_this_process_has_a_start_time_and_a_reused_number_is_not_it(self):
        me = gameos.process_started(os.getpid())
        self.assertIsNotNone(me)
        self.assertLess(abs(me - time.time()), 3600, "started within the hour, by the clock this test reads")
        self.assertTrue(gameos.process_alive(os.getpid(), me))
        self.assertFalse(gameos.process_alive(os.getpid(), me - 100), "another process that got the same number")
        self.assertIsNone(gameos.process_started(-5))

    @unittest.skipUnless(gameos.LINUX, "the /proc side")
    def test_a_process_named_like_the_game_is_found_stopped_and_waited_for(self):
        d = tempfile.mkdtemp(prefix="magician-gameos-")
        fake = os.path.join(d, "deadlock.exe")
        with open(fake, "w", newline="\n") as f:   # a script, not a copy of `sleep`: a multi-call coreutils refuses a new name
            f.write("#!/bin/sh\nread _\n")
        os.chmod(fake, 0o755)
        p = subprocess.Popen([fake], stdin=subprocess.PIPE)
        try:
            for _ in range(50):
                if p.pid in gameos.game_pids():
                    break
                time.sleep(0.05)
            self.assertIn(p.pid, gameos.game_pids())
            started = gameos.process_started(p.pid)
            self.assertTrue(gameos.process_alive(p.pid, started))
            self.assertTrue(gameos.terminate(p.pid))
            p.wait(5)   # this test is its parent: reaped here, so it is gone, not a zombie
            self.assertTrue(gameos.wait_exit([(p.pid, started)], 5))
            self.assertNotIn(p.pid, gameos.game_pids())
        finally:
            if p.poll() is None:
                p.kill()
            p.stdin.close()
            shutil.rmtree(d, ignore_errors=True)

    @unittest.skipUnless(gameos.LINUX, "the /proc side")
    def test_a_file_held_open_is_not_free_until_it_is_closed(self):
        with tempfile.NamedTemporaryFile(prefix="magician-gameos-", suffix=".vpk", delete=False) as f:
            path = f.name
            self.assertFalse(gameos.file_free(path))
        self.assertTrue(gameos.file_free(path))
        os.remove(path)


def _x11_ready():
    return gameos.LINUX and gameos.display() is not None


@unittest.skipUnless(_x11_ready(), "needs an X display with XTest: " + str(gameos.display_problem()))
class X11(unittest.TestCase):
    """A window of this test's own, on its own X connection, as the game's would be."""

    W, H, BG, FG = 320, 200, 0x3366CC, 0x22DD44

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
        for n in ("XMapRaised", "XSelectInput", "XDestroyWindow"):
            getattr(x, n).argtypes = [P, UL] + ([ctypes.c_long] if n == "XSelectInput" else [])
        x.XCreateGC.restype = P
        x.XCreateGC.argtypes = [P, UL, UL, P]
        x.XSetForeground.argtypes = [P, P, UL]
        x.XFillRectangle.argtypes = [P, UL, P, ctypes.c_int, ctypes.c_int, ctypes.c_uint, ctypes.c_uint]
        x.XFreeGC.argtypes = [P, P]
        x.XPending.argtypes = [P]
        x.XNextEvent.argtypes = [P, P]
        x.XSync.argtypes = [P, ctypes.c_int]
        x.XFlush.argtypes = [P]
        x.XCloseDisplay.argtypes = [P]
        x.XWarpPointer.argtypes = [P, UL, UL, ctypes.c_int, ctypes.c_int, ctypes.c_uint, ctypes.c_uint, ctypes.c_int, ctypes.c_int]
        x.XQueryPointer.argtypes = [P, UL] + [ctypes.POINTER(UL)] * 2 + [ctypes.POINTER(ctypes.c_int)] * 4 + [ctypes.POINTER(ctypes.c_uint)]
        cls.d = x.XOpenDisplay(None)
        cls.root = x.XDefaultRootWindow(cls.d)
        cls.win = x.XCreateSimpleWindow(cls.d, cls.root, 40, 40, cls.W, cls.H, 0, 0, cls.BG)

        def prop(name, kind, fmt, data, n):
            x.XChangeProperty(cls.d, cls.win, x.XInternAtom(cls.d, name, 0), x.XInternAtom(cls.d, kind, 0), fmt, 0, data, n)

        title = b"Deadlock (harness test window)"
        prop(b"WM_NAME", b"STRING", 8, title, len(title))
        prop(b"_NET_WM_NAME", b"UTF8_STRING", 8, title, len(title))
        cls_ = b"steam_app_1422450\0steam_app_1422450\0"
        prop(b"WM_CLASS", b"STRING", 8, cls_, len(cls_))
        pid = ctypes.c_long(os.getpid())
        prop(b"_NET_WM_PID", b"CARDINAL", 32, ctypes.cast(ctypes.byref(pid), ctypes.c_char_p), 1)
        # KeyPress, KeyRelease, ButtonPress, ButtonRelease, PointerMotion, Exposure
        x.XSelectInput(cls.d, cls.win, (1 << 0) | (1 << 1) | (1 << 2) | (1 << 3) | (1 << 6) | (1 << 15))
        x.XMapRaised(cls.d, cls.win)
        x.XSync(cls.d, 0)
        cls.events = []
        cls.pump(1.0)
        gc = x.XCreateGC(cls.d, cls.win, 0, None)
        x.XSetForeground(cls.d, gc, cls.FG)
        x.XFillRectangle(cls.d, cls.win, gc, 100, 50, 60, 40)
        x.XFreeGC(cls.d, gc)
        x.XSync(cls.d, 0)
        time.sleep(0.3)
        cls.g = gameos.display()

    @classmethod
    def tearDownClass(cls):
        cls.x.XDestroyWindow(cls.d, cls.win)
        cls.x.XSync(cls.d, 0)
        cls.x.XCloseDisplay(cls.d)

    @classmethod
    def pump(cls, secs):
        """Read this window's events for `secs`: (type, detail) pairs; 2 KeyPress, 3 KeyRelease, 4 ButtonPress, 6 Motion."""
        buf = ctypes.create_string_buffer(192)
        end = time.time() + secs
        while time.time() < end:
            while cls.x.XPending(cls.d):
                cls.x.XNextEvent(cls.d, buf)
                kind = ctypes.c_int.from_buffer(buf, 0).value
                # XKeyEvent and XButtonEvent: type, serial, send_event, display, window, root, subwindow, time, x, y,
                # x_root, y_root, state, then keycode / button (unsigned int) at the same offset: 84 with 8-byte longs
                off = 84 if ctypes.sizeof(ctypes.c_long) == 8 else 52
                detail = ctypes.c_uint.from_buffer(buf, off).value if kind in (2, 3, 4, 5) else 0
                cls.events.append((kind, detail))
            time.sleep(0.02)

    def setUp(self):
        del self.events[:]
        self.pump(0.1)
        del self.events[:]

    def test_the_window_is_found_by_its_class_and_title_and_names_its_process(self):
        w = self.g.find_window()
        self.assertEqual(w, self.win)
        self.assertEqual(self.g.window_pid(w), os.getpid())
        self.assertIn("harness test window", self.g.describe(w))
        x, y, width, height = self.g.client_rect(w)
        self.assertEqual((width, height), (self.W, self.H))

    def test_the_capture_returns_the_pixels_drawn(self):
        w = self.g.find_window()
        self.g.bring_forward(w, 1)
        time.sleep(0.3)
        x, y, width, height = self.g.client_rect(w)
        px = self.g.grab(x, y, width, height, w)
        self.assertEqual(len(px), width * height * 4)

        def at(cx, cy):
            i = (cy * width + cx) * 4
            b, g_, r = px[i], px[i + 1], px[i + 2]
            return (r << 16) | (g_ << 8) | b

        self.assertEqual(at(10, 10), self.BG, "the window's background")
        self.assertEqual(at(130, 70), self.FG, "inside the rectangle drawn at 100,50")
        self.assertEqual(at(170, 70), self.BG, "just right of it")
        sx, sy, sw, sh = self.g.desktop_rect()
        self.assertGreaterEqual(sw, width)

    def test_keys_sent_arrive_in_the_window_in_front_and_read_as_held(self):
        w = self.g.find_window()
        for attempt in range(1, 20):
            if self.g.in_front(w):
                break
            self.g.bring_forward(w, attempt)
            time.sleep(0.2)
        self.assertTrue(self.g.in_front(w), "in front: " + self.g.describe(self.g.foreground()))
        code_a, code_rshift = self.g._keycode("a"), self.g._keycode("rshift")
        self.g.key("rshift", False)
        self.g.key("a", False)
        self.assertEqual(self.g.keys_down(["a", "rshift", "w"]), ["a", "rshift"])
        self.g.keys(["a", "rshift"], True)
        self.assertEqual(self.g.keys_down(["a", "rshift"]), [])
        self.pump(0.5)
        presses = [d for k, d in self.events if k == 2]
        releases = [d for k, d in self.events if k == 3]
        self.assertEqual(presses, [code_rshift, code_a], self.events)
        self.assertEqual(sorted(releases), sorted([code_a, code_rshift]), self.events)
        # every key name the harness types has a keycode in this keyboard map
        for name in gameos.SC:
            self.assertTrue(self.g._keycode(name), name)

    def test_the_mouse_moves_by_what_was_sent_and_its_buttons_and_wheel_arrive(self):
        w = self.g.find_window()
        x, y, width, height = self.g.client_rect(w)
        self.x.XWarpPointer(self.d, 0, self.win, 0, 0, 0, 0, 20, 20)
        self.x.XSync(self.d, 0)
        time.sleep(0.2)

        def pointer():
            r, c = ctypes.c_ulong(), ctypes.c_ulong()
            rx, ry, wx, wy, m = ctypes.c_int(), ctypes.c_int(), ctypes.c_int(), ctypes.c_int(), ctypes.c_uint()
            self.x.XQueryPointer(self.d, self.root, ctypes.byref(r), ctypes.byref(c), ctypes.byref(rx), ctypes.byref(ry),
                                 ctypes.byref(wx), ctypes.byref(wy), ctypes.byref(m))
            return rx.value, ry.value

        before = pointer()
        self.g.mouse_move(30, 17)
        time.sleep(0.2)
        after = pointer()
        self.assertEqual((after[0] - before[0], after[1] - before[1]), (30, 17))
        self.g.button(1, False)
        self.g.button(1, True)
        self.g.button(2, False)
        self.g.button(2, True)
        self.g.wheel(1)
        self.g.wheel(-2)
        self.pump(0.5)
        self.assertEqual([d for k, d in self.events if k == 4], [1, 3, 4, 5, 5], self.events)


if __name__ == "__main__":
    unittest.main()

"""Tests for session.py's hand-back (no game needed): python -I test_session.py

The rule under test: the seat is released only after pak90 is uninstalled AND verified gone (`magician install
--status` and the addons folder), however long the game takes to stop. 2026-11-22: a hung `quit` outlasted the
fixed 300 s hold, the lock went stale as the game exited, a waiting session took the seat and installed pak90, and
the closing session's uninstall then removed it (the next render ran without the kit and showed the HUD).
"""
import os
import subprocess
import sys
import tempfile
import time
import types
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import session as S  # noqa: E402
import gamelock as G  # noqa: E402


class FakeWorld:
    """The game, the addons folder, magician's install state and the clock, all fake. Time advances on sleep."""

    def __init__(self, quit_hangs_s=0.0, uninstall_fails=0, other_pipeline=True):
        self.now = 1_000_000.0
        self.running = True
        self.installed = True                 # pak90 in addons and in magician's status
        self.quit_hangs_s = quit_hangs_s      # how long stop_game.ps1 spends before the game exits
        self.uninstall_fails = uninstall_fails
        self.events = []                      # the order things happened in
        self.other_pipeline = other_pipeline          # a session waiting for the seat (it installs pak90 when it gets it)
        self.other_has_seat = False
        self.lock = os.path.join(tempfile.mkdtemp(), "game.lock")

    # --- the waiting pipeline: polls the seat; when it gets it, it launches and installs pak90 ---
    def poll_other(self):
        if not self.other_pipeline or self.other_has_seat:
            return
        if G.acquire("cinema-next", "next render", 0, path=self.lock, running=lambda: self.running,
                     say=lambda *_: None):
            self.other_has_seat = True
            self.installed = True
            self.events.append("OTHER took the seat and installed pak90")

    def sleep(self, s):
        self.now += s
        self.poll_other()

    def time(self):
        return self.now

    def run(self, cmd, capture_output=True, text=True, timeout=None):
        exe = os.path.basename(cmd[0]).lower()
        if exe in ("magician.exe", "magician") and cmd[1] == "install" and cmd[2] == "--status":
            out = "build 6864: addons search path present\n  pak10_dir.vpk  crc 1  for maid_lash  installed\n"
            if self.installed:
                out += "  pak90_dir.vpk  crc 2  for cinema  installed\n"
            return types.SimpleNamespace(returncode=0, stdout=out, stderr="")
        if exe in ("magician.exe", "magician") and cmd[1] == "uninstall":
            if self.uninstall_fails > 0:
                self.uninstall_fails -= 1
                self.events.append("uninstall failed (files busy)")
                return types.SimpleNamespace(returncode=1, stdout="", stderr="in use")
            self.installed = False
            self.events.append("SELF uninstalled pak90")
            return types.SimpleNamespace(returncode=0, stdout="uninstalled pak90_dir.vpk", stderr="")
        if exe == "tasklist":
            return types.SimpleNamespace(returncode=0, stdout="deadlock.exe 1234" if self.running else "INFO: none",
                                         stderr="")
        if exe == "taskkill":
            self.running = False
            self.events.append("game killed")
            return types.SimpleNamespace(returncode=0, stdout="", stderr="")
        if exe == "powershell":      # stop_game.ps1 -KeepLock: the quit hangs, then the game exits
            start = self.now
            while self.now < start + self.quit_hangs_s:
                if timeout is not None and self.now - start >= timeout:
                    raise subprocess.TimeoutExpired(cmd, timeout)
                self.sleep(5)
            self.running = False
            self.events.append("game exited")
            self.sleep(10)                 # stop_game waits for Windows to let go of the addon paks
            return types.SimpleNamespace(returncode=0, stdout="closed the game", stderr="")
        raise AssertionError(f"unexpected command {cmd}")


class HandBack(unittest.TestCase):
    def setUp(self):
        self.saved = {}

    def patch(self, obj, name, value):
        self.saved.setdefault((obj, name), getattr(obj, name))
        setattr(obj, name, value)

    def tearDown(self):
        for (obj, name), v in self.saved.items():
            setattr(obj, name, v)

    def world(self, **kw):
        w = FakeWorld(**kw)
        self.patch(S.subprocess, "run", w.run)
        self.patch(S.time, "sleep", w.sleep)
        self.patch(S.time, "time", w.time)
        self.patch(G.time, "time", w.time)
        self.patch(G, "LOCK", w.lock)
        self.patch(G, "game_running", lambda: w.running)
        self.patch(S, "game_running", lambda: w.running)
        real_exists = os.path.exists
        self.patch(S.os.path, "exists", lambda p: w.installed if str(p).endswith("pak90_dir.vpk") else real_exists(p))
        # the heartbeat's thread would race the fake clock: beat on every fake sleep instead (same effect, in order)
        real_sleep = w.sleep
        beats = []

        def sleep_and_beat(s):
            for h in beats:
                if not h._stop.is_set() and h._thread is not None:
                    h.beat()
            real_sleep(s)
        w.sleep = sleep_and_beat
        self.patch(S.time, "sleep", sleep_and_beat)

        class Heart(S._Heartbeat):
            def start(self):
                if self._thread is not None:
                    return
                self.beat()
                self._thread = True
                beats.append(self)

            def stop(self):
                self._stop.set()
        self.patch(S, "_Heartbeat", Heart)
        # the session's own game, bound to its seat at launch: alive while the fake game runs
        self.patch(G, "process_alive", lambda pid, started=None: pid == 1234 and w.running)
        assert G.acquire(S.HOLDER, "cinema session", 0, path=w.lock, running=lambda: True)
        assert G.bind(S.HOLDER, 1234, path=w.lock, started=lambda pid: 1.0)
        return w

    def test_the_hard_stop_kills_only_this_sessions_own_game(self):
        w = self.world()
        killed = []
        real = w.run
        w.run = lambda cmd, **kw: (killed.append(cmd), real(cmd, **kw))[1]
        self.patch(S.subprocess, "run", w.run)
        self.assertTrue(S.kill_game())
        self.assertEqual(killed[-1][:3], ["taskkill", "/PID", "1234"], "the bound pid, never /IM deadlock.exe")
        # another holder's seat: nothing is killed
        w.running = True
        G.release(S.HOLDER, path=w.lock, leave_running=True)
        G.acquire("someone-else", "theirs", 0, path=w.lock, running=lambda: True)
        G.bind("someone-else", 1234, path=w.lock, started=lambda pid: 1.0)
        n = len(killed)
        self.assertFalse(S.kill_game())
        self.assertEqual(len(killed), n)
        self.assertTrue(w.running)

    def session(self):
        s = S.Session.__new__(S.Session)
        s.con, s.installed, s.held, s.fps_max, s.say = None, True, True, "180", (lambda *_: None)
        return s

    def test_a_hung_quit_never_frees_the_seat_before_the_uninstall(self):
        w = self.world(quit_hangs_s=330)          # the quit hangs past the old fixed 300 s hold
        s = self.session()
        s._close()
        self.assertFalse(s.held)
        self.assertFalse(w.installed and not w.other_has_seat, w.events)
        i_uninstall = w.events.index("SELF uninstalled pak90")
        others = [i for i, e in enumerate(w.events) if e.startswith("OTHER")]
        self.assertTrue(all(i > i_uninstall for i in others), w.events)
        self.assertFalse(w.installed and w.events[-1] == "SELF uninstalled pak90" and not w.other_has_seat)

    def test_the_old_fixed_hold_lost_the_race(self):
        # the bug, reproduced on the same fake world: a 300 s hold and no heartbeat
        w = self.world(quit_hangs_s=400)
        G.acquire(S.HOLDER, "cinema hand-back", 0, hold=300, path=w.lock, running=lambda: w.running)
        w.run(["powershell", "stop_game.ps1"])    # the game exits after 400 s; the waiting pipeline polls meanwhile
        w.sleep(1)
        self.assertTrue(w.other_has_seat, "the old hold should have gone stale (the bug)")

    def test_a_stop_that_never_returns_is_cut_off_and_the_game_killed(self):
        w = self.world(quit_hangs_s=10_000)
        s = self.session()
        s._close()
        self.assertIn("game killed", w.events)
        self.assertFalse(w.installed or w.other_has_seat, w.events)
        self.assertFalse(s.held)
        self.assertLess(w.events.index("game killed"), w.events.index("SELF uninstalled pak90"))

    def test_a_failed_uninstall_is_retried_and_verified(self):
        w = self.world(uninstall_fails=1)
        s = self.session()
        s._close()
        self.assertFalse(w.installed, w.events)
        self.assertFalse(s.held)
        self.assertFalse(w.other_has_seat and w.events.index("OTHER took the seat and installed pak90")
                         < w.events.index("SELF uninstalled pak90"))

    def test_an_uninstall_that_never_works_keeps_the_seat(self):
        w = self.world(uninstall_fails=99)
        s = self.session()
        s._close()
        self.assertTrue(w.installed)
        self.assertTrue(s.held, "the seat must not be released while pak90 is installed")
        self.assertFalse(w.other_has_seat, w.events)
        w.sleep(3600)                              # an hour later the lock still holds (long hold)
        self.assertFalse(w.other_has_seat, w.events)

    def test_status_reads_pak90_only_from_its_own_line(self):
        w = self.world()
        w.installed = False
        self.assertEqual(S.pak_state(), (False, False))
        w.installed = True
        self.assertEqual(S.pak_state(), (True, True))


if __name__ == "__main__":
    unittest.main()

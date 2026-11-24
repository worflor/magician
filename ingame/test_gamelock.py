"""Tests for gamelock.py (no game needed): python test_gamelock.py"""
import os
import tempfile
import threading
import unittest

import gamelock as g


class Lock(unittest.TestCase):
    def setUp(self):
        self.path = os.path.join(tempfile.mkdtemp(), "sub", "game.lock")
        self.said = []

    def take(self, holder, running=True, **kw):
        return g.acquire(holder, "scenario", path=self.path, running=lambda: running, say=self.said.append, poll=0.02, **kw)

    def test_a_second_holder_is_refused_and_told_who_has_it(self):
        self.assertTrue(self.take("pipeline-a"))
        self.assertFalse(self.take("pipeline-b"))
        self.assertIn("pipeline-a", self.said[-1])
        self.assertIn("MAGICIAN_GAME_HOLDER", self.said[-1])

    def test_the_holder_can_take_it_again_and_rename_the_scenario(self):
        self.assertTrue(self.take("pipeline-a"))
        self.assertTrue(g.acquire("pipeline-a", "lash_motion", path=self.path, running=lambda: True, say=print))
        self.assertEqual(g.read(self.path)["what"], "lash_motion")

    def test_no_game_running_means_the_lock_is_stale(self):
        self.assertTrue(self.take("pipeline-a"))
        self.age_lock(g.FRESH_GRACE + 1)
        self.assertTrue(self.take("pipeline-b", running=False))
        self.assertEqual(g.read(self.path)["holder"], "pipeline-b")
        self.assertTrue(any("stale" in s for s in self.said))

    def test_a_hold_keeps_the_seat_after_the_game_has_closed(self):
        # a hand-back closes the game, then uninstalls a pak: a waiting pipeline must not take the seat in between
        self.assertTrue(self.take("pipeline-a"))
        self.age_lock(g.LAUNCH_GRACE + 1)
        self.assertTrue(self.take("pipeline-a", hold=60))
        self.assertFalse(self.take("pipeline-b", running=False))
        self.assertTrue(g.release("pipeline-a", path=self.path))
        self.assertTrue(self.take("pipeline-b", running=False))

    def age_lock(self, seconds):
        lock = g.read(self.path)
        lock["since"] -= seconds
        g._write(self.path, lock, exclusive=False)

    def test_a_lock_just_taken_survives_until_its_holder_launches(self):
        self.assertTrue(self.take("pipeline-a", running=False))
        self.assertFalse(self.take("pipeline-b", running=False), "a fresh lock is not stale before the game starts")
        self.age_lock(g.FRESH_GRACE + 1)
        self.assertTrue(self.take("pipeline-b", running=False), "an old lock with no game is stale")

    def test_a_launch_in_progress_holds_the_lock_before_the_game_appears(self):
        self.assertTrue(self.take("pipeline-a", running=False, launching=True))
        self.assertFalse(self.take("pipeline-b", running=False))
        lock = g.read(self.path)
        self.assertTrue(g.is_stale(lock, False, now=lock["since"] + g.LAUNCH_GRACE + 1), "a launch that never produced a game expires")

    def test_release_is_for_the_holder_unless_forced(self):
        self.assertTrue(self.take("pipeline-a"))
        self.assertFalse(g.release("pipeline-b", path=self.path))
        self.assertTrue(os.path.exists(self.path))
        self.assertTrue(g.release("pipeline-b", force=True, path=self.path))
        self.assertFalse(os.path.exists(self.path))
        self.assertTrue(g.release("nobody", path=self.path))

    def test_waiting_gets_the_game_when_the_holder_lets_go(self):
        self.assertTrue(self.take("pipeline-a"))
        threading.Timer(0.3, lambda: g.release("pipeline-a", path=self.path)).start()
        self.assertTrue(self.take("pipeline-b", wait=5))

    def test_an_unreadable_lock_is_stale(self):
        os.makedirs(os.path.dirname(self.path))
        with open(self.path, "w") as f:
            f.write("{broken")
        self.assertTrue(self.take("pipeline-a"))

    def test_a_lock_whose_times_are_not_numbers_or_far_ahead_is_stale_not_a_crash_or_a_forever_hold(self):
        os.makedirs(os.path.dirname(self.path))
        for body in ['{"holder": "x", "since": "a"}', '{"holder": "x", "since": 1e18}', '{"holder": "x", "since": 0, "busy_until": Infinity}',
                     '{"holder": "x", "busy_until": 1e18}', '{"holder": "x", "since": NaN}', '{"holder": 7}', '{"holder": "x", "since": true}']:
            with open(self.path, "w") as f:
                f.write(body)
            self.assertIsNone(g.read(self.path), body)
            self.assertTrue(self.take("pipeline-a", running=False), body)
            self.assertEqual(g.read(self.path)["holder"], "pipeline-a", body)
            os.remove(self.path)

    def test_the_command_line_refuses_a_value_that_is_not_one(self):
        import subprocess, sys
        env = dict(os.environ, MAGICIAN_GAME_LOCK=self.path)
        here = os.path.dirname(os.path.abspath(__file__))
        for args in (["acquire", "--wait", "abc"], ["acquire", "--hold", "inf"], ["acquire", "--hold"], ["acquire", "--wait", "-1"]):
            r = subprocess.run([sys.executable, "-I", os.path.join(here, "gamelock.py")] + args, capture_output=True, text=True, env=env)
            self.assertNotEqual(r.returncode, 0, args)
            self.assertNotIn("Traceback", r.stderr, args)
        self.assertFalse(os.path.exists(self.path), "nothing was taken")

    # The game is bound to the seat (2026-11-24: a stop freed the seat while deadlock.exe was still exiting, the next
    # holder launched, and the first pipeline's fallback killed the new holder's game).
    def bound(self, holder="pipeline-a", pid=4242, started=1000.0):
        self.assertTrue(self.take(holder))
        self.assertTrue(g.bind(holder, pid, path=self.path, started=lambda p: started if p == pid else None))
        return pid

    def test_a_bound_game_keeps_the_seat_until_its_process_has_exited(self):
        pid = self.bound()
        self.age_lock(g.LAUNCH_GRACE + 1)
        exiting = {"alive": True}
        alive = lambda p, s=None: exiting["alive"] and p == pid and s == 1000.0
        # tasklist no longer lists it, but the process has not ended: nobody else gets the seat, and the holder's own
        # release is refused until it has
        self.assertFalse(g.acquire("pipeline-b", path=self.path, running=lambda: False, say=self.said.append, alive=alive))
        self.assertFalse(g.release("pipeline-a", path=self.path, alive=alive, say=self.said.append))
        self.assertIn("still running or exiting", self.said[-1])
        exiting["alive"] = False
        self.assertTrue(g.release("pipeline-a", path=self.path, alive=alive))
        self.assertTrue(g.acquire("pipeline-b", path=self.path, running=lambda: False, say=self.said.append, alive=alive))

    def test_handing_the_seat_back_over_a_live_game_is_explicit_and_the_next_holder_cannot_drive_it(self):
        pid = self.bound()
        alive = lambda p, s=None: True
        self.assertFalse(g.release("pipeline-a", path=self.path, alive=alive))
        self.assertTrue(g.release("pipeline-a", path=self.path, alive=alive, leave_running=True))
        self.assertTrue(self.take("pipeline-b"))
        self.assertIn("not launched by this holder", g.may_drive(g.read(self.path), "pipeline-b", pid, alive=alive))

    def test_only_the_holder_and_only_in_the_game_it_launched_may_drive(self):
        alive = lambda p, s=None: p in (4242, 5151)
        self.assertIn("nobody holds", g.may_drive(None, "pipeline-a", 4242, alive=alive))
        self.assertTrue(self.take("pipeline-a"))
        self.assertIn("no game is bound", g.may_drive(g.read(self.path), "pipeline-a", 4242, alive=alive))
        self.bound()
        lock = g.read(self.path)
        self.assertIsNone(g.may_drive(lock, "pipeline-a", 4242, alive=alive))
        self.assertIn("in use by pipeline-a", g.may_drive(lock, "pipeline-b", 4242, alive=alive))
        self.assertIn("not the one 'pipeline-a' launched", g.may_drive(lock, "pipeline-a", 5151, alive=alive))
        self.assertIn("no Deadlock window", g.may_drive(lock, "pipeline-a", 0, alive=alive))
        dead = lambda p, s=None: False
        self.assertIn("has exited", g.may_drive(lock, "pipeline-a", 4242, alive=dead))

    def test_bind_is_for_the_holder_and_a_running_process_only(self):
        self.assertTrue(self.take("pipeline-a"))
        self.assertFalse(g.bind("pipeline-b", 4242, path=self.path, started=lambda p: 1.0))
        self.assertFalse(g.bind("pipeline-a", 4242, path=self.path, started=lambda p: None))
        self.assertNotIn("game_pid", g.read(self.path))
        self.assertTrue(g.bind("pipeline-a", 4242, path=self.path, started=lambda p: 1.0))
        lock = g.read(self.path)
        self.assertEqual((lock["game_pid"], lock["game_started"], lock["launching"]), (4242, 1.0, False))
        # the holder taking the seat again keeps the binding
        self.assertTrue(g.acquire("pipeline-a", "stop", path=self.path, running=lambda: True, say=print, hold=60))
        self.assertEqual(g.read(self.path)["game_pid"], 4242)

    def test_with_two_games_running_the_bound_one_is_judged(self):
        lock = {"holder": "a", "game_pid": 5151}
        self.assertEqual(g.running_game(lock, [4242, 5151]), 5151)
        self.assertEqual(g.running_game(lock, [4242]), 4242, "the bound game is gone: the other one is judged, and refused")
        self.assertEqual(g.running_game(None, []), 0)

    def test_a_reused_pid_is_not_the_bound_game(self):
        me = g.process_started(os.getpid())
        self.assertIsNotNone(me, "this process is running")
        self.assertTrue(g.process_alive(os.getpid(), me))
        if os.name == "nt":
            self.assertFalse(g.process_alive(os.getpid(), me - 100), "another process that got the same number")
        self.assertFalse(g.process_alive(0))
        self.assertIsNone(g.process_started(-5))

    def test_a_lock_with_a_bad_game_pid_is_stale(self):
        os.makedirs(os.path.dirname(self.path))
        for body in ['{"holder": "x", "game_pid": "12"}', '{"holder": "x", "game_pid": -1}', '{"holder": "x", "game_pid": true}',
                     '{"holder": "x", "game_pid": 12, "game_started": "a"}']:
            with open(self.path, "w") as f:
                f.write(body)
            self.assertIsNone(g.read(self.path), body)

    def test_the_holder_defaults_to_the_environment(self):
        os.environ["MAGICIAN_GAME_HOLDER"] = "pipeline-env"
        try:
            self.assertEqual(g.holder_name(), "pipeline-env")
            self.assertEqual(g.holder_name("x"), "x")
        finally:
            del os.environ["MAGICIAN_GAME_HOLDER"]
        self.assertEqual(g.holder_name(), g.DEFAULT_HOLDER)


if __name__ == "__main__":
    unittest.main()

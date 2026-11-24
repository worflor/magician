"""Tests for takes.py's plan reader and frozen-hero check (no game needed): python test_takes.py"""
import unittest

import takes as T

PLAN = """
# a live plan
home 1300 -1450 20 4 54
> sv_cheats 1
cam SIDE
take run 20 10 full : 0:+w 1.5:-w 1.6:space
wait 0.5
shot after
home
"""

STAGE = """
home 1300 -1450 20 4 54
beat dash 3 : 0:+w 0.5:shift 2.5:-w 2.6:m300,0,0.5
"""


class Plans(unittest.TestCase):
    def test_a_live_plan_reads_every_line(self):
        steps = T.parse_plan(PLAN, "live")
        self.assertEqual([s[0] for s in steps], ["home", "cmd", "cam", "take", "wait", "shot", "home"])
        take = steps[3][1]
        self.assertEqual((take["name"], take["frames"], take["fps"], take["full"]), ("run", 20, 10, True))
        self.assertEqual(take["events"], [(0.0, "+w"), (1.5, "-w"), (1.6, "space")])

    def test_a_stage_plan_has_beats_not_takes(self):
        steps = T.parse_plan(STAGE, "stage")
        self.assertEqual(steps[1][1]["secs"], 3.0)
        with self.assertRaises(ValueError):
            T.parse_plan(STAGE, "live")
        with self.assertRaises(ValueError):
            T.parse_plan(PLAN, "stage")

    def test_a_bad_line_is_refused_before_anything_is_typed(self):
        for bad in ("jump 3", "take x 10 : 0:+w", "beat x : 0:+w", "cam NOWHERE", "home 1 2", "take r 5 5 : 0+w"):
            with self.assertRaises(ValueError, msg=bad):
                T.parse_plan("home 1 2 3 4 5\n" + bad, "live" if not bad.startswith("beat") else "stage")

    def test_home_and_cam_need_a_spot_first(self):
        with self.assertRaises(ValueError):
            T.parse_plan("home\n", "live")
        with self.assertRaises(ValueError):
            T.parse_plan("cam SIDE\n", "live")

    def test_events_sort_by_time_and_keep_their_order_at_one_time(self):
        self.assertEqual(T.parse_events("2:-w 0:+w 0:space"), [(0.0, "+w"), (0.0, "space"), (2.0, "-w")])


class Frozen(unittest.TestCase):
    def test_held_movement_spans_merge_across_keys_and_run_to_the_end(self):
        ev = T.parse_events("0:+w 1:+a 2:-w 3:-a 4:space 5:+s")
        self.assertEqual(T.held_spans(ev, 8.0), [(0.0, 3.0), (5.0, 8.0)])
        self.assertEqual(T.held_spans(T.parse_events("0:+e 1:-e 2:shift"), 5.0), [], "not movement")

    def test_two_moments_a_second_apart_inside_each_long_enough_span(self):
        self.assertEqual(T.probe_times([(0.0, 3.0), (5.0, 6.5), (7.0, 10.0)]), [(1.0, 2.0), (8.0, 9.0)])

    def test_the_same_picture_is_noticed_and_a_moving_one_is_not(self):
        still = bytes(range(256)) * 400
        moved = bytes((b + 40) % 256 for b in still)
        self.assertTrue(T.same_picture(still, bytes(still)))
        noisy = bytearray(still)
        noisy[::997] = bytes(len(noisy[::997]))        # a few changed pixels: a blinking cursor, a HUD digit
        self.assertTrue(T.same_picture(still, bytes(noisy)))
        self.assertFalse(T.same_picture(still, moved))
        self.assertFalse(T.same_picture(still, still[:-4]), "frames of different sizes are not compared")
        self.assertFalse(T.same_picture(None, still))


if __name__ == "__main__":
    unittest.main()

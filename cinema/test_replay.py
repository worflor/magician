"""Tests for replay.py's shot list and demo-end check, offline: python test_replay.py"""
import unittest

import replay as R

MARKS = "1000.000\thome\t0.00\n1010.500\tdash\t3.00\n1020.000\tdash\t4.00\n"
SHOTS = """
# a comment
side   beat=dash at=0.5 dist=120 bear=h+90 half
fixed  tick=640 dur=2 pos=1,2,3 ang=10,90
"""


class Shots(unittest.TestCase):
    def test_marks_keep_the_last_beat_of_a_name(self):
        self.assertEqual(R.read_marks(MARKS)["dash"], (1020.0, 4.0))

    def test_a_beat_shot_starts_at_its_wall_time_from_tick_zero(self):
        beats = R.read_marks(MARKS)
        (name, k, flags), (name2, k2, _) = R.read_shots(SHOTS)
        self.assertEqual((name, flags), ("side", {"half"}))
        t0, tick, dur = R.shot_ticks(k, beats, 1000.0)
        self.assertEqual((t0, tick, dur), (1280, 1312, 4.0))
        self.assertEqual(R.shot_ticks(k2, beats, 0), (640, 640, 2.0))
        with self.assertRaises(KeyError):
            R.shot_ticks({"beat": "nope"}, beats, 0)

    def test_a_camera_given_outright_needs_no_track(self):
        _, k, _ = R.read_shots(SHOTS)[1]
        self.assertEqual(R.camera(k, None, 0, 0, "Haze#0"), (1.0, 2.0, 3.0, 10.0, 90.0))

    def test_the_end_of_a_demo_is_read_from_the_log(self):
        # lines as build 6865 prints them
        self.assertTrue(R.demo_ended("11/24 07:33:55 [Demo] Demo playback finished ( 495.9 seconds, 83343 render frames, 168.06 fps).\n"))
        self.assertTrue(R.demo_ended("11/24 05:46:19 [Demo] Error - Not currently playing back a demo.\n"))
        self.assertFalse(R.demo_ended("11/24 07:25:33 [HostStateManager] Host activate: Playing Demo (replays/x.dem)\n"
                                      "[Demo] CDemoPlayer::SetIgnorePacketsForResourceLoading(false)\n"))


class Frames(unittest.TestCase):
    def test_a_crop_is_given_corner_first_and_ffmpeg_wants_size_first(self):
        import frames as F
        self.assertEqual(F.crop_filter("100:50:640:360"), "crop=640:360:100:50,")
        self.assertEqual(F.crop_filter(None), "")
        self.assertEqual(F.sheet_filter(0, 6, 12, 4, 640), "fps=2.00000,scale=640:-1,tile=4x3")


if __name__ == "__main__":
    unittest.main()

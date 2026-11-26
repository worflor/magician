"""capture.py's pure parts, without a window: the burst's frame times.

    python -I test_capture.py
"""
import os
import sys
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import capture  # noqa: E402


class FrameTimes(unittest.TestCase):
    def test_each_frame_has_its_offset_and_wall_clock(self):
        tsv = capture.frame_times([(0.0, b""), (0.125, b""), (0.2611, b"")], 1795700000.5)
        rows = [line.split("\t") for line in tsv.splitlines()]
        self.assertEqual(rows[0], ["file", "secs", "wall"])
        self.assertEqual(rows[1], ["f000.png", "0.0000", "1795700000.5000"])
        self.assertEqual(rows[3], ["f002.png", "0.2611", "1795700000.7611"])
        self.assertTrue(tsv.endswith("\n"))


if __name__ == "__main__":
    unittest.main()

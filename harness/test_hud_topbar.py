"""The top-bar portrait test, offline: synthetic cards and mask, never a game file.

  python test_hud_topbar.py
"""
import json
import os
import shutil
import sys
import tempfile
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import hud_topbar as hb  # noqa: E402
from PIL import Image, ImageDraw  # noqa: E402


def mask():
    """The badge mask's shape: square shoulders above, a half circle below (the card's size, alpha carries it)."""
    m = Image.new("RGBA", (hb.CARD_W, hb.CARD_H), (0, 0, 0, 0))
    d = ImageDraw.Draw(m)
    d.rectangle([0, 0, hb.CARD_W - 1, hb.CARD_H - hb.CARD_W // 2], fill=(255, 255, 255, 255))
    d.ellipse([0, hb.CARD_H - hb.CARD_W, hb.CARD_W - 1, hb.CARD_H - 1], fill=(255, 255, 255, 255))
    return m


def card(colour, face=None):
    """A card filled with one colour, with an optional darker face disc in the middle."""
    c = Image.new("RGBA", (hb.CARD_W, hb.CARD_H), colour + (255,))
    if face:
        ImageDraw.Draw(c).ellipse([60, 80, 220, 240], fill=face + (255,))
    return c


class TopBar(unittest.TestCase):
    def setUp(self):
        self.d = tempfile.mkdtemp(prefix="hud-topbar-")
        self.valve = os.path.join(self.d, "valve")
        os.makedirs(os.path.join(self.valve, hb.CARDS))
        os.makedirs(os.path.dirname(os.path.join(self.valve, hb.MASK)))
        mask().save(os.path.join(self.valve, hb.MASK))
        for i, name in enumerate(["alpha", "bravo", "charlie", "delta", "echo", "foxtrot", "golf"]):
            card((40 + 20 * i, 90, 120), face=(230, 200, 170)).save(os.path.join(self.valve, hb.CARDS, f"{name}_card_psd.png"))
        card((200, 40, 40), face=(20, 20, 20)).save(os.path.join(self.valve, hb.CARDS, "alpha_card_critical_psd.png"))

    def tearDown(self):
        shutil.rmtree(self.d, ignore_errors=True)

    def test_the_slot_is_the_games_size(self):
        # 88 x 120 badge; the 280 x 380 card "contain"ed in 80 % of its width: 70 x 95 at 1080p, x 4/3 at 1440p
        self.assertEqual(hb.slot_size(1.0), (88, 120, 70, 95))
        self.assertEqual(hb.slot_size(4 / 3), (117, 160, 94, 128))

    def test_the_mask_cuts_the_card_and_the_team_disc_shows_where_it_is_clear(self):
        b = hb.badge(card((255, 255, 255)), mask(), 1.0, hb.TEAM1)
        self.assertEqual(b.size, (88, 120))
        x0 = (88 - 70) // 2
        self.assertEqual(b.getpixel((x0 + 35, 10))[:3], (255, 255, 255), "the card above")
        self.assertEqual(b.getpixel((x0 + 1, 94))[3], 0, "the half circle cuts the bottom corners")
        self.assertEqual(b.getpixel((x0 - 3, 40))[3], 0, "nothing outside the image container")
        clear = Image.new("RGBA", (hb.CARD_W, hb.CARD_H), (0, 0, 0, 0))
        self.assertEqual(hb.badge(clear, mask(), 1.0, hb.TEAM2).getpixel((x0 + 35, 70))[:3], hb.TEAM2, "the team disc behind a clear card")

    def test_a_flat_card_measures_less_contrast_than_a_face(self):
        flat = hb.measure(card((240, 240, 240)), mask())
        face = hb.measure(card((240, 240, 240), face=(40, 30, 30)), mask())
        self.assertAlmostEqual(flat["fill"], 1.0, places=2)
        self.assertLess(flat["contrast"], 0.01)
        self.assertGreater(face["contrast"], 0.2)
        half = Image.new("RGBA", (hb.CARD_W, hb.CARD_H), (0, 0, 0, 0))
        half.paste(card((200, 200, 200)).crop((0, 0, hb.CARD_W, hb.CARD_H // 2)), (0, 0))
        self.assertLess(hb.measure(half, mask())["fill"], 0.7, "a card that leaves the disc showing fills less")

    def test_the_sheet_puts_ours_among_valves_and_writes_its_measures(self):
        ours = os.path.join(self.d, "ours.png")
        card((250, 250, 250), face=(30, 30, 60)).save(ours)
        out, js = os.path.join(self.d, "sheet.png"), os.path.join(self.d, "m.json")
        hb.main(["--out", out, "--hero", "alpha", "--valve", self.valve, "--row", f"now={ours}", "--row", f"low={ours},critical", "--json", js])
        sheet = Image.open(out)
        self.assertGreater(sheet.size[0], 88 * 8 * 4)
        m = json.load(open(js, encoding="utf-8"))
        self.assertEqual(m["slot_1080p"], [88, 120, 70, 95])
        now, low = m["rows"]
        self.assertEqual((now["form"], low["form"]), ("card", "card_critical"))
        self.assertEqual(now["valve_cards"], 6, "the hero itself is not in the stock")
        self.assertIn("valve_own", now)
        self.assertEqual(set(now["valve_median"]), {"fill", "contrast"})

    def test_a_capture_gives_the_ground_and_a_bad_row_is_refused(self):
        frame = Image.new("RGB", (2560, 1440), (10, 200, 10))
        self.assertEqual(hb.strip_of(frame, hb.STRIP_1080).size, (854, 176), "the 1080p box scaled to a 1440p capture")
        self.assertEqual(hb.strip_of(None, hb.STRIP_1080).getpixel((0, 0)), hb.GROUND)
        with self.assertRaises(SystemExit):
            hb.parse_row("ours.png")
        with self.assertRaises(SystemExit):
            hb.parse_row("x=ours.png,purple")


if __name__ == "__main__":
    unittest.main()

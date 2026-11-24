"""Tests for logdiff.py and the session index's warning list, no game needed: python -I test_logdiff.py

A vanilla baseline belongs to a build and a scene (the hero and the map). The picker never hands back another
scene's or a newer build's, says when the one it found is from an older build, and a vanilla pass saves a new one.
"""
import os
import shutil
import subprocess
import sys
import tempfile
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import logdiff as L  # noqa: E402
import mod_session as S  # noqa: E402

VANILLA = """11/24 17:16:56 [RenderSystem] returning error texture in CTextureManagerVulkan::GetTextureResource
11/24 17:16:56 [RenderSystem] returning error texture in CTextureManagerVulkan::GetTextureResource
11/24 17:17:04 [ResourceSystem] Failed loading resource "models/heroes/haze/clips/ui_hero_reveal.vnmclip_c" (ERROR_FILEOPEN: File not found)
11/24 17:17:05 Loaded hero 3/hero_haze
"""
MODDED = VANILLA.replace("17:16:56", "18:01:02") + """11/24 18:01:30 [GCClient] OnSteamServerConnectFailure(result=3, StillRetrying=1)
11/24 18:01:31 CClientSteamContext OnSteamServerConnectFailure logged on = 0
11/24 18:01:32 [SteamNetSockets] SteamNetworkingSockets lock held for 5.1ms.  (Performance warning.)  GetConnectionRealTimeStatus
11/24 18:02:00 [ResourceSystem] Failed loading resource "materials/example_skin/coat.vmat_c" (ERROR_FILEOPEN: File not found)
"""


class Baselines(unittest.TestCase):
    def setUp(self):
        self.d = tempfile.mkdtemp()

    def tearDown(self):
        shutil.rmtree(self.d, ignore_errors=True)

    def put(self, name, text=VANILLA):
        p = os.path.join(self.d, name)
        with open(p, "w", encoding="utf-8") as f:
            f.write(text)
        return p

    def test_a_scene_is_the_hero_and_the_map(self):
        self.assertEqual(L.scene_name("hero_haze", "new_player_basics"), "haze_new_player_basics")
        self.assertEqual(L.scene_name("hero_lady-geist", None), "lady_geist")
        self.assertEqual(L.scene_name(None, None), "menu")

    def test_the_baseline_is_this_builds_or_the_newest_older_one_of_the_same_scene(self):
        self.assertIsNone(L.pick_base("6866", "haze_new_player_basics", self.d))
        old = self.put("vanilla_6864_haze_new_player_basics.log")
        self.put("vanilla_6861_haze_new_player_basics.log")
        self.put("vanilla_6865_abrams_new_player_basics.log")   # another hero's says nothing about this one
        self.put("vanilla_6870_haze_new_player_basics.log")     # a newer build's is not a "before"
        self.assertEqual(L.pick_base("6866", "haze_new_player_basics", self.d), (old, "6864", False))
        exact = L.save_vanilla(old, "6866", "haze_new_player_basics", self.d)
        self.assertEqual(os.path.basename(exact), "vanilla_6866_haze_new_player_basics.log")
        self.assertEqual(L.pick_base("6866", "haze_new_player_basics", self.d), (exact, "6866", True))
        self.assertIsNone(L.pick_base("6866", "haze_street_test", self.d))

    def test_the_command_says_when_the_baseline_is_an_older_builds_and_when_there_is_none(self):
        mod = self.put("modded.log", MODDED)
        env = dict(os.environ, MAGICIAN_REFERENCE=self.d)
        bases = os.path.join(self.d, "work", "ingame-baselines")
        os.makedirs(bases)

        def run(*a):
            return subprocess.run([sys.executable, "-I", os.path.join(HERE, "logdiff.py"), "--mod", mod] + list(a),
                                  capture_output=True, text=True, env=env)
        r = run("--build", "6866", "--scene", "haze_new_player_basics")
        self.assertEqual(r.returncode, 3, r.stdout + r.stderr)
        self.assertIn("NOT COMPARED", r.stdout)
        shutil.copyfile(self.put("v.log"), os.path.join(bases, "vanilla_6864_haze_new_player_basics.log"))
        r = run("--build", "6866", "--scene", "haze_new_player_basics")
        self.assertEqual(r.returncode, 1, r.stdout + r.stderr)
        self.assertIn("this one is from build 6864", r.stdout)
        self.assertIn("NEW warnings with the mod (1)", r.stdout)
        self.assertIn("materials/example_skin/coat.vmat_c", r.stdout)
        self.assertNotIn("ui_hero_reveal", r.stdout.split("NEW warnings")[1].split("warnings the baseline")[0])
        r = run("--save-vanilla", "--build", "6866", "--scene", "haze_new_player_basics")
        self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
        r = run("--build", "6866", "--scene", "haze_new_player_basics")
        self.assertNotIn("NOTE:", r.stdout)
        self.assertEqual(r.returncode, 0, "a log compared with itself adds nothing")

    def test_the_index_lists_what_vanilla_does_not_print_and_counts_the_rest(self):
        vanilla = L.warnings(self.put("v.log"))
        modded = [l for l in MODDED.strip().splitlines() if "Steam" not in l]
        hits = modded[:3] + modded[4:] + modded[4:]
        lines = S.warning_lines(hits, vanilla, True)
        self.assertEqual(len(lines), 2, lines)
        self.assertTrue(lines[0].strip().startswith("x2 ") and "coat.vmat_c" in lines[0], lines[0])
        self.assertIn("2 distinct warning(s) vanilla prints too", lines[1])
        self.assertIn("none that vanilla does not print too", S.warning_lines(modded[:3], vanilla, True)[0])
        every = S.warning_lines(hits, {}, False)
        self.assertEqual(len(every), 3, "with no baseline every line is listed and nothing is said about whose it is")
        self.assertTrue(every[0].strip().startswith("x2 ") and "error texture" in every[0], every[0])
        self.assertEqual(S.warning_lines([], vanilla, True), ["    none"])

if __name__ == "__main__":
    unittest.main()

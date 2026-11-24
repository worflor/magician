"""Tests for mod_session.py, no game needed: python test_session.py

The mod and item lookup are pure. The install and its exact restore run the real magician binary against a scratch
game folder (a two-file fake Deadlock made here), never the live install; they skip when the binary is not built.
"""
import os
import shutil
import subprocess
import sys
import tempfile
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
sys.path.insert(0, os.path.join(os.path.dirname(HERE), "tools"))
import mod_session as S  # noqa: E402

STATUS = """build 6865: addons search path present
backup: none for this build
  maps/ui/hero_prefabs/haze.vpk  crc d287ca64  for example_showcase  installed
  pak06_dir.vpk  crc 0a0455ab  for example_voice  installed
  pak12_dir.vpk  crc c7387a19  for example_skin  installed
  pak20_dir.vpk  crc bac13441  for other_mod  installed
"""


def write(path, text):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w", encoding="utf-8") as f:
        f.write(text)


class Mod(unittest.TestCase):
    def setUp(self):
        self.d = tempfile.mkdtemp()
        self.mod = os.path.join(self.d, "example-skin")
        write(os.path.join(self.mod, "mod.toml"), '[mod]\nslug = "example-skin"\nhero = "hero_haze"\nowners = ["example_voice"]\n'
              '[ingame]\nholder = "example-qa"\npass = ["angles", "own_pass"]\n')
        write(os.path.join(self.mod, "source", "ingame", "scenarios", "own_pass.txt"), "sv_cheats 1\n")
        write(os.path.join(self.mod, "source", "ingame", "scenarios", "angles.txt"), "# the mod's own angles\n")
        write(os.path.join(self.mod, "source", "ingame", "takes", "a.stage"), "home 1 2 3 4 5\n")

    def tearDown(self):
        shutil.rmtree(self.d, ignore_errors=True)

    def test_the_mod_is_found_from_a_folder_below_it_and_read(self):
        deep = os.path.join(self.mod, "source", "ingame")
        toml = S.find_mod_toml(deep)
        self.assertEqual(toml, os.path.join(self.mod, "mod.toml"))
        m = S.read_mod(toml)
        self.assertEqual((m["slug"], m["owner"], m["hero"], m["holder"]), ("example-skin", "example_skin", "hero_haze", "example-qa"))
        self.assertEqual(m["owners"], ["example_skin", "example_voice"])
        self.assertEqual(m["pass"], ["angles", "own_pass"])
        self.assertIsNone(S.find_mod_toml(os.path.join(self.d, "elsewhere")), "no mod.toml in it or above it")

    def test_an_item_is_a_path_or_the_mods_own_file_before_the_toolkits(self):
        own = S.resolve_item("angles", self.mod)
        self.assertEqual(own, os.path.join(self.mod, "source", "ingame", "scenarios", "angles.txt"), "the mod's own wins")
        self.assertEqual(S.resolve_item("angles"), os.path.join(HERE, "scenarios", "angles.txt"), "then the toolkit's example")
        self.assertEqual(S.resolve_item("a", self.mod), os.path.join(self.mod, "source", "ingame", "takes", "a.stage"))
        self.assertEqual(S.resolve_item("own_pass.txt", self.mod), os.path.join(self.mod, "source", "ingame", "scenarios", "own_pass.txt"))
        self.assertEqual(S.resolve_item(own), own)
        self.assertIsNone(S.resolve_item("nowhere", self.mod))
        self.assertEqual(S.item_name(own), "angles")

    def test_the_toolkit_examples_are_on_a_stock_hero_and_parse(self):
        import takes as T
        for n in ("enter_haze", "angles", "motion"):
            self.assertIsNotNone(S.resolve_item(n), n)
        with open(S.resolve_item("enter_haze"), encoding="utf-8") as f:
            self.assertIn("selecthero hero_haze", f.read())
        self.assertTrue(T.parse_events("0:+w 1:-w"))


class Status(unittest.TestCase):
    def test_rows_and_the_mods_slot(self):
        rows = S.status_rows(STATUS)
        self.assertEqual(rows[0], ("maps/ui/hero_prefabs/haze.vpk", "d287ca64", "example_showcase", "installed"))
        self.assertEqual(S.slot_of(rows, "example_skin"), "pak12_dir.vpk")
        self.assertIsNone(S.slot_of(rows, "example_showcase"), "a scene map is not a pak slot")
        self.assertIsNone(S.slot_of(rows, "nobody"))

    def test_the_command_line_refuses_what_it_does_not_know(self):
        for argv in (["up"], ["bogus"], ["up", "--out"], ["up", "--out", "x", "--frobnicate"], ["up", "--out", "x", "--pak", "a", "--vanilla"]):
            with self.assertRaises(S.Stop, msg=argv) as e:
                S.parse(argv)
            self.assertEqual(e.exception.code, 2)
        verb, o, rest = S.parse(["all", "--out", "x", "--hero", "haze", "--no-tv", "angles", "motion"])
        self.assertEqual((verb, o, rest), ("all", {"out": "x", "hero": "haze", "no-tv": True}, ["angles", "motion"]))

    def test_a_verb_after_up_does_nothing_in_a_game_that_is_not_ours(self):
        import gamelock as G
        d = tempfile.mkdtemp()
        saved = (G.LOCK, G.game_pids, G.read)
        try:
            G.game_pids = lambda: [4242]
            G.read = lambda path=None: {"holder": "someone-else", "since": 0, "game_pid": 4242}
            with self.assertRaises(S.Stop) as e:
                S.ours({"holder": "example-qa", "out": d})
            self.assertEqual(e.exception.code, 9)
            self.assertIn("NOT OURS", str(e.exception))
        finally:
            G.LOCK, G.game_pids, G.read = saved


EXE = S.magician_exe()


@unittest.skipUnless(os.path.isfile(EXE), "the magician binary is not built (cargo build --release)")
class InstallAndRestore(unittest.TestCase):
    """--pak and --vanilla with --restore put back exactly what was there, in a scratch game."""

    def setUp(self):
        import scene_install
        self.d = tempfile.mkdtemp()
        game = os.path.join(self.d, "game")
        for sub in ("citadel", "core"):
            os.makedirs(os.path.join(game, sub, "addons" if sub == "citadel" else ""), exist_ok=True)
            scene_install.write_vpk(os.path.join(game, sub, "pak01_dir.vpk"), {f"scripts/{sub}_stub.txt": b"stub"})
        write(os.path.join(game, "citadel", "steam.inf"), "ClientVersion=6865\nPatchVersion=6865\n")
        write(os.path.join(game, "citadel", "gameinfo.gi"), '"GameInfo"\n{\n\tFileSystem\n\t{\n\t\tSearchPaths\n\t\t{\n'
              '\t\t\tGame\tcitadel\n\t\t\tGame\tcore\n\t\t}\n\t}\n}\n')
        self.env = dict(os.environ, DEADLOCK_GAME=game, MAGICIAN_REFERENCE=os.path.join(self.d, "ref"),
                        MAGICIAN_GAME_LOCK=os.path.join(self.d, "lock"), MAGICIAN_CONFIG=os.path.join(self.d, "none.txt"))
        self.saved_env = dict(os.environ)
        os.environ.update(self.env)
        self.paks = {}
        for name, body in (("old", b"the installed build"), ("new", b"a candidate build"), ("voice", b"a voice pak")):
            p = os.path.join(self.d, name, "pak50_dir.vpk")
            os.makedirs(os.path.dirname(p))
            scene_install.write_vpk(p, {f"sounds/example/{name}.txt": body})
            self.paks[name] = p
        for pak, slot, owner in ((self.paks["old"], "pak12_dir.vpk", "example_skin"), (self.paks["voice"], "pak06_dir.vpk", "example_voice")):
            r = subprocess.run([EXE, "install", pak, "--as", slot, "--mod", owner, "--holder", "drill"], capture_output=True, text=True)
            if r.returncode != 0:
                self.fail("magician install on the scratch game: " + (r.stdout + r.stderr)[-400:])
        self.st = {"out": os.path.join(self.d, "out"), "owner": "example_skin", "owners": ["example_skin", "example_voice"],
                   "installed_before": S.install_status()}
        os.makedirs(self.st["out"])

    def tearDown(self):
        os.environ.clear()
        os.environ.update(self.saved_env)
        shutil.rmtree(self.d, ignore_errors=True)

    def test_a_candidate_pak_goes_over_the_mods_slot_and_the_old_one_comes_back(self):
        rows = S.status_rows(self.st["installed_before"])
        self.assertEqual(S.slot_of(rows, "example_skin"), "pak12_dir.vpk")
        S.install_for(self.st, self.paks["new"], "pak12_dir.vpk", "drill", restore=True)
        now = S.status_rows(S.install_status())
        self.assertIn(("pak12_dir.vpk", S.crc_of(self.paks["new"]), "example_skin", "installed"), now)
        S.restore_installs(self.st, "drill")
        self.assertEqual(sorted(S.status_rows(S.install_status())), sorted(rows), "the same bytes in the same slots (ledger order aside)")

    def test_a_vanilla_pass_uninstalls_every_pak_of_the_mod_and_restores_them(self):
        rows = S.status_rows(self.st["installed_before"])
        S.uninstall_mod(self.st, "drill", restore=True)
        self.assertEqual(S.status_rows(S.install_status()), [])
        S.restore_installs(self.st, "drill")
        self.assertEqual(sorted(S.status_rows(S.install_status())), sorted(rows))

    def test_a_restore_that_does_not_match_says_so(self):
        S.install_for(self.st, self.paks["new"], "pak12_dir.vpk", "drill", restore=False)
        with self.assertRaises(S.Stop) as e:
            S.restore_installs(self.st, "drill")
        self.assertIn("not as it was", str(e.exception))


if __name__ == "__main__":
    unittest.main()

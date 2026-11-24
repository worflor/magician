"""The player-side scene installers on good and bad patches, offline: scene_install.py and scene_install.ps1 (when
Windows PowerShell is here) must install a good patch to the same bytes (and the same bytes as `magician scene install`
when the release binary is built), and refuse every malformed patch the same way, writing nothing. Every game here is
a scratch folder with a synthetic stock map.

    python -I tools/test_scene_install.py
"""
import hashlib, os, shutil, subprocess, sys, tempfile, unittest, zlib

TOOLS = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, TOOLS)
import scene_install  # noqa: E402

PS = shutil.which("powershell") if os.name == "nt" else None
EXE = os.path.join(TOOLS, "..", "target", "release", "magician.exe" if os.name == "nt" else "magician")
SCENE = "maps/ui/hero_prefabs/lash"
VALVE = {f"{SCENE}.vmap_c": b"valve map", f"{SCENE}/entities/default_ents.vents_c": b"valve lump", f"{SCENE}/cubemaps/c.vtex_c": b"valve cube"}
OURS = {f"{SCENE}/entities/default_ents.vents_c": b"OUR lump", f"{SCENE}/models/ribbon.vmdl_c": b"our ribbon"}


def put(path, b):
    with open(path, "wb") as f:
        f.write(b)


def read(path):
    with open(path, "rb") as f:
        return f.read()


def crc(b):
    return "%08x" % (zlib.crc32(b) & 0xFFFFFFFF)


def manifest(**over):
    rows = {
        "head": "magician-scene-patch\t1",
        "map": "map\tmaps/ui/hero_prefabs/lash.vpk",
        "mod": "mod\tlash_showcase",
        "built_for": "built_for\t6864",
        "keep1": f"keep\t{SCENE}.vmap_c\t{crc(VALVE[SCENE + '.vmap_c'])}\t{len(VALVE[SCENE + '.vmap_c'])}",
        "keep2": f"keep\t{SCENE}/cubemaps/c.vtex_c\t{crc(b'valve cube')}\t10",
        "replace": f"replace\t{SCENE}/entities/default_ents.vents_c\t{crc(b'valve lump')}\t{crc(b'OUR lump')}\t8",
        "add": f"add\t{SCENE}/models/ribbon.vmdl_c\t{crc(b'our ribbon')}\t10",
    }
    rows.update(over)
    return "".join(r + "\n" for r in rows.values() if r is not None)


class Installers(unittest.TestCase):
    def setUp(self):
        self.d = tempfile.mkdtemp(prefix="magician-scene-")

    def tearDown(self):
        shutil.rmtree(self.d, ignore_errors=True)

    def game(self, name, valve=VALVE):
        g = os.path.join(self.d, name, "game")
        os.makedirs(os.path.join(g, "citadel", "maps", "ui", "hero_prefabs"))
        put(os.path.join(g, "citadel", "gameinfo.gi"), b'"GameInfo"\n{\n\tFileSystem\n\t{\n\t\tSearchPaths\n\t\t{\n\t\t\tGame\t\t\t\tcitadel/addons\n\t\t\tGame\t\t\t\tcitadel\n\t\t}\n\t}\n}\n')
        put(os.path.join(g, "citadel", "steam.inf"), b"ClientVersion=6864\n")
        scene_install.write_vpk(os.path.join(g, "citadel", "maps", "ui", "hero_prefabs", "lash.vpk"), valve)
        return g

    def patch(self, text, files=OURS, raw=None):
        p = os.path.join(self.d, "patch-%d" % len(os.listdir(self.d)))
        for rel, b in files.items():
            f = os.path.join(p, "files", *rel.split("/"))
            os.makedirs(os.path.dirname(f), exist_ok=True)
            put(f, b)
        os.makedirs(p, exist_ok=True)
        put(os.path.join(p, "scene-patch.txt"), raw if raw is not None else text.encode())
        return p

    def run_all(self, patch, valve=VALVE):
        """(tool, exit code, the installed scene's sha256 or None) for each installer, each into its own game."""
        out = []
        tools = [("py", lambda g: [sys.executable, "-I", os.path.join(TOOLS, "scene_install.py"), "--patch", patch, "--game", g])]
        if PS:
            tools.append(("ps1", lambda g: [PS, "-NoProfile", "-ExecutionPolicy", "Bypass", "-File", os.path.join(TOOLS, "scene_install.ps1"), "-Patch", patch, "-Game", g]))
        if os.path.isfile(EXE):
            tools.append(("magician", lambda g: [EXE, "scene", "install", patch, "--game", g, "--ignore-game"]))
        env = dict(os.environ, DEADLOCK_GAME="", MAGICIAN_CONFIG=os.path.join(self.d, "none.txt"), MAGICIAN_REFERENCE=os.path.join(self.d, "ref"))
        for name, cmd in tools:
            g = self.game(f"{name}-{len(os.listdir(self.d))}", valve)
            r = subprocess.run(cmd(g), capture_output=True, text=True, env=env)
            target = os.path.join(g, "citadel", "addons", *f"{SCENE}.vpk".split("/"))
            left = [f for f in os.listdir(os.path.dirname(target))] if os.path.isdir(os.path.dirname(target)) else []
            out.append((name, r.returncode, hashlib.sha256(read(target)).hexdigest() if os.path.isfile(target) else None, left, r.stdout + r.stderr))
        return out

    def test_a_good_patch_installs_to_the_same_bytes_everywhere(self):
        res = self.run_all(self.patch(manifest()))
        for name, code, digest, _, text in res:
            self.assertEqual(code, 0, f"{name}: {text}")
        self.assertEqual(len({r[2] for r in res}), 1, res)
        # and a BOM an editor put in front is not part of the text
        bom = self.run_all(self.patch(None, raw=b"\xef\xbb\xbf" + manifest().encode()))
        self.assertEqual({r[2] for r in bom}, {res[0][2]}, bom)

    def test_every_malformed_patch_is_refused_and_writes_nothing(self):
        cases = {
            "no header": manifest(head=None),
            "a newer format behind a BOM": None,
            "fields separated by spaces": manifest(keep2=f"keep {SCENE}/cubemaps/c.vtex_c {crc(b'valve cube')} 10"),
            "an unknown field": manifest(extra="sizes\t1"),
            "a field missing": manifest(add=f"add\t{SCENE}/models/ribbon.vmdl_c\t{crc(b'our ribbon')}"),
            "a crc that is not 8 hex digits": manifest(keep2=f"keep\t{SCENE}/cubemaps/c.vtex_c\t{int(crc(b'valve cube'), 16):#x}\t10"),
            "a size that is not a number": manifest(keep2=f"keep\t{SCENE}/cubemaps/c.vtex_c\t{crc(b'valve cube')}\t0x0a"),
            "a path listed twice": manifest(extra=f"add\t{SCENE}/models/ribbon.vmdl_c\t{crc(b'our ribbon')}\t10"),
            "no mod": manifest(mod=None),
            "an uppercase path": manifest(add=f"add\t{SCENE}/models/Ribbon.vmdl_c\t{crc(b'our ribbon')}\t10"),
            "an empty segment": manifest(add=f"add\t{SCENE}//ribbon.vmdl_c\t{crc(b'our ribbon')}\t10"),
            "a walk out of the scene": manifest(add=f"add\t{SCENE}/../../../../gameinfo.gi\t{crc(b'our ribbon')}\t10"),
            "a keep whose size changed": manifest(keep2=f"keep\t{SCENE}/cubemaps/c.vtex_c\t{crc(b'valve cube')}\t11"),
            "no map file": manifest(keep1=None),
        }
        for why, text in cases.items():
            raw = ("﻿magician-scene-patch\t2\n" + manifest(head=None)).encode() if text is None else None
            files = dict(OURS)
            if why == "an uppercase path":
                files = {f"{SCENE}/entities/default_ents.vents_c": b"OUR lump", f"{SCENE}/models/Ribbon.vmdl_c": b"our ribbon"}
            res = self.run_all(self.patch(text, files=files, raw=raw))
            for name, code, digest, left, out in res:
                with self.subTest(case=why, tool=name):
                    self.assertNotEqual(code, 0, f"{why}: {name} accepted it: {out}")
                    self.assertIsNone(digest, f"{why}: {name} wrote a scene")
                    self.assertEqual(left, [], f"{why}: {name} left {left}")
                    self.assertNotIn("Traceback", out, f"{why}: {name}")
        # an add where Valve's map now has a file: the game changed under the patch
        valve = dict(VALVE, **{f"{SCENE}/models/ribbon.vmdl_c": b"valve's new ribbon"})
        for name, code, digest, _, out in self.run_all(self.patch(manifest()), valve=valve):
            with self.subTest(case="an add over a file Valve's map now has", tool=name):
                self.assertNotEqual(code, 0, f"{name}: {out}")
                self.assertIsNone(digest)
                self.assertIn("new in your game", out, name)

    def test_uninstall_without_a_result_line_removes_only_the_scene_the_patch_builds(self):
        patch = self.patch(manifest())  # no `result` row
        tools = [("py", lambda g, *a: [sys.executable, "-I", os.path.join(TOOLS, "scene_install.py"), "--patch", patch, "--game", g, *a])]
        if PS:
            tools.append(("ps1", lambda g, *a: [PS, "-NoProfile", "-ExecutionPolicy", "Bypass", "-File", os.path.join(TOOLS, "scene_install.ps1"), "-Patch", patch, "-Game", g, *[{"--uninstall": "-Uninstall", "--force": "-Force"}[x] for x in a]]))
        for name, cmd in tools:
            with self.subTest(tool=name):
                g = self.game(f"uninstall-{name}")
                target = os.path.join(g, "citadel", "addons", *f"{SCENE}.vpk".split("/"))
                run = lambda *a: subprocess.run(cmd(g, *a), capture_output=True, text=True)
                self.assertEqual(run().returncode, 0)
                r = run("--uninstall")
                self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
                self.assertFalse(os.path.exists(target), "its own scene is removed")
                # another mod's scene at the same path is left alone, unless forced
                put(target, b"someone else's scene")
                r = run("--uninstall")
                self.assertNotEqual(r.returncode, 0, r.stdout + r.stderr)
                self.assertIn("is not this patch's scene", r.stdout)
                self.assertEqual(read(target), b"someone else's scene")
                # and when the game changed under the patch, there is no telling: refused too
                scene_install.write_vpk(os.path.join(g, "citadel", "maps", "ui", "hero_prefabs", "lash.vpk"), dict(VALVE, **{f"{SCENE}/cubemaps/c.vtex_c": b"new cube"}))
                r = run("--uninstall")
                self.assertNotEqual(r.returncode, 0)
                self.assertIn("no telling", r.stdout)
                self.assertTrue(os.path.isfile(target))
                r = run("--uninstall", "--force")
                self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
                self.assertFalse(os.path.exists(target))

    def test_a_damaged_stock_map_is_a_message_not_a_traceback(self):
        p = self.patch(manifest())
        g = self.game("damaged")
        stock = os.path.join(g, "citadel", "maps", "ui", "hero_prefabs", "lash.vpk")
        with open(stock, "r+b") as f:
            f.truncate(40)
        r = subprocess.run([sys.executable, "-I", os.path.join(TOOLS, "scene_install.py"), "--patch", p, "--game", g], capture_output=True, text=True)
        self.assertEqual(r.returncode, 1, r.stdout + r.stderr)
        self.assertNotIn("Traceback", r.stdout + r.stderr)

    def test_a_game_folder_that_is_not_one_is_an_error_not_a_search(self):
        p = self.patch(manifest())
        nowhere = os.path.join(self.d, "not-a-game")
        os.makedirs(nowhere)
        r = subprocess.run([sys.executable, "-I", os.path.join(TOOLS, "scene_install.py"), "--patch", p, "--game", nowhere], capture_output=True, text=True)
        self.assertEqual(r.returncode, 2, r.stdout)
        self.assertIn("is not Deadlock's", r.stdout)
        self.assertNotIn("Deadlock:", r.stdout, "it named no other game")
        if PS:
            r = subprocess.run([PS, "-NoProfile", "-ExecutionPolicy", "Bypass", "-File", os.path.join(TOOLS, "scene_install.ps1"), "-Patch", p, "-Game", nowhere], capture_output=True, text=True)
            self.assertEqual(r.returncode, 2, r.stdout + r.stderr)
            self.assertIn("is not Deadlock's", r.stdout)
            self.assertNotIn("Deadlock:", r.stdout)


if __name__ == "__main__":
    unittest.main()

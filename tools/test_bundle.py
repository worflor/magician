"""The bundler's guards, offline: a spec is checked before anything is written, `out` is replaced only when it is a
bundle, and a failed build leaves the last good bundle where it was. Needs the release magician (it lists the paks);
needs no game (--no-valve-check).

    python -I tools/test_bundle.py
"""
import json, os, shutil, subprocess, sys, tempfile, unittest

TOOLS = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, TOOLS)
import scene_install  # noqa: E402  (its VPK writer makes the test paks)

sys.path.insert(0, os.path.join(TOOLS, "..", "ingame"))
import gamepaths  # noqa: E402  (MAGICIAN_BIN, else the release build where cargo put it)

EXE = gamepaths.magician()


class Bundle(unittest.TestCase):
    def setUp(self):
        self.d = tempfile.mkdtemp(prefix="magician-bundle-")
        self.spec_dir = os.path.join(self.d, "package")
        os.makedirs(self.spec_dir)
        self.pak = os.path.join(self.d, "pak10_dir.vpk")
        scene_install.write_vpk(self.pak, {"particles/x.vpcf_c": b"ours"})
        self.out = os.path.join(self.d, "bundle")

    def text(self, *rel):
        with open(os.path.join(*rel), encoding="utf-8") as f:
            return f.read()

    def tearDown(self):
        shutil.rmtree(self.d, ignore_errors=True)

    def spec(self, **over):
        s = {"name": "Test", "slug": "test", "author": "me", "hero": "Lash", "description": "d", "version": "b1",
             "date": "2026-11-22", "out": self.out,
             "mods": [{"slug": "test", "name": "Test", "description": "d", "order": 0, "files": [{"name": "pak10_dir.vpk", "src": self.pak}]}]}
        s.update(over)
        return s

    def run_bundle(self, spec, *extra):
        p = os.path.join(self.spec_dir, "bundle.json")
        with open(p, "w", encoding="utf-8") as f:
            json.dump(spec, f)
        return subprocess.run([sys.executable, "-I", os.path.join(TOOLS, "bundle.py"), p, "--no-valve-check", "--magician", EXE] + list(extra),
                              capture_output=True, text=True)

    def test_a_good_spec_builds_and_rebuilds_in_place(self):
        r = self.run_bundle(self.spec())
        self.assertEqual(r.returncode, 0, r.stderr + r.stdout)
        self.assertTrue(os.path.isfile(os.path.join(self.out, "mod-interchange.json")))
        first = self.text(self.out, "SHA256SUMS.txt")
        r = self.run_bundle(self.spec())
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual(self.text(self.out, "SHA256SUMS.txt"), first, "the same inputs, the same bundle")
        self.assertFalse(os.path.exists(self.out + ".partial"))

    def test_a_mod_can_carry_its_own_category(self):
        # a voice pak beside a skin: two "Skins" for one hero make a mod manager offer to swap one out
        s = self.spec()
        s["mods"][0]["category"] = "Voice Lines"
        r = self.run_bundle(s)
        self.assertEqual(r.returncode, 0, r.stderr + r.stdout)
        mod = json.loads(self.text(self.out, "mod-interchange.json"))["mods"][0]
        self.assertEqual(mod["category"], "Voice Lines")
        self.assertEqual(mod["hero"], "Lash")

    def test_out_is_never_a_folder_that_is_not_a_bundle(self):
        # the spec's own folder (out: "."), and a folder of someone's files
        r = self.run_bundle(self.spec(out="."))
        self.assertNotEqual(r.returncode, 0)
        self.assertTrue(os.path.isfile(os.path.join(self.spec_dir, "bundle.json")), "the spec's folder is untouched")
        os.makedirs(self.out)
        with open(os.path.join(self.out, "notes.txt"), "w") as f:
            f.write("mine")
        r = self.run_bundle(self.spec())
        self.assertNotEqual(r.returncode, 0)
        self.assertIn("not a bundle", r.stderr)
        self.assertTrue(os.path.isfile(os.path.join(self.out, "notes.txt")))

    def test_a_spec_that_would_fail_halfway_is_refused_before_the_last_bundle_goes(self):
        self.assertEqual(self.run_bundle(self.spec()).returncode, 0)
        good = self.text(self.out, "mod-interchange.json")
        bad = self.spec()
        del bad["author"]
        for spec, why in ((bad, "needs author"),
                          (self.spec(mods=[dict(self.spec()["mods"][0], files=[{"name": "../escape.vpk", "src": self.pak}])]), "plain, unique name"),
                          (self.spec(readme="missing.md"), "does not exist"),
                          (self.spec(date="22/11/2026"), "YYYY-MM-DD")):
            r = self.run_bundle(spec)
            self.assertNotEqual(r.returncode, 0, why)
            self.assertIn(why, r.stderr)
            self.assertEqual(self.text(self.out, "mod-interchange.json"), good, why)
        self.assertFalse(os.path.exists(os.path.join(self.d, "escape.vpk")))

    def test_the_scene_installers_keep_their_line_ends_whatever_the_checkout_has(self):
        # a Windows checkout can hand the bundler CRLF files and a Linux one LF: the bundle must be the same bytes
        import bundle
        for given in (b"a\r\nb\r\n", b"a\nb\n", b"a\r\nb\n"):
            src, dst = os.path.join(self.d, "in.txt"), os.path.join(self.d, "out.txt")
            with open(src, "wb") as f:
                f.write(given)
            bundle.copy_text(src, dst, "\n")
            self.assertEqual(open(dst, "rb").read(), b"a\nb\n", given)
            bundle.copy_text(src, dst, "\r\n")
            self.assertEqual(open(dst, "rb").read(), b"a\r\nb\r\n", given)

    def test_a_flag_without_its_value_is_an_error(self):
        r = self.run_bundle(self.spec(), "--out")
        self.assertNotEqual(r.returncode, 0)
        self.assertIn("--out needs a value", r.stderr)


if __name__ == "__main__":
    if not os.path.isfile(EXE):
        print(f"skipped: no magician at {EXE} (cargo build --release)")
        sys.exit(0)
    unittest.main()

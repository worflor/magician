"""The mod-source writer's guards, offline: a spec is checked before anything is written, the tree is the same bytes
for the same inputs, the content revision moves exactly when what gets installed moves, files it did not write are
never touched, and downloads on another host are declared with the hosts they redirect to. Builds its own bundle zips
the way bundle.py lays them out; needs no game and no magician binary. With DMM_SOURCE_CHECK pointing at DMM's
examples/mod-source/check.mjs (and node on the PATH) it also runs DMM's own checker on the tree.

    python -I tools/test_source.py
"""
import hashlib, io, json, os, re, shutil, subprocess, sys, tempfile, unittest, zipfile

TOOLS = os.path.dirname(os.path.abspath(__file__))
CHECK = os.environ.get("DMM_SOURCE_CHECK")
try:
    from PIL import Image
except ImportError:
    Image = None


def png(color):
    buf = io.BytesIO()
    Image.new("RGB", (64, 40), color).save(buf, "PNG")
    return buf.getvalue()


class Source(unittest.TestCase):
    def setUp(self):
        self.d = tempfile.mkdtemp(prefix="magician-source-")
        self.out = os.path.join(self.d, "site")
        self.skin, self.voice = b"skin pak bytes", b"voice pak bytes"

    def tearDown(self):
        shutil.rmtree(self.d, ignore_errors=True)

    def bundle(self, release="1.0.0", readme_extra="", **over):
        """A bundle.json and the zip bundle.py --zip would write for it: a skin and an optional voice pak."""
        spec = {"name": "Test Skin", "slug": "test-skin", "author": "me", "hero": "Lash", "hero_id": "lash",
                "content_type": "skins", "tags": ["lash", "skin"], "nsfw": False, "release": release,
                "date": "2026-11-22", "game_build": 6864, "description": "A test skin.", "version": "b1",
                "zip": f"TestSkin_{release}", "scenes": [{"src": "scene", "hero": "lash"}],
                "mods": [{"slug": "skin", "name": "Test Skin", "description": "The skin.", "order": 0,
                          "files": [{"name": "pak10_dir.vpk", "src": "x"}]},
                         {"slug": "voice", "name": "Test Skin, voice", "description": "Optional. A voice.", "order": 1,
                          "optional": True, "files": [{"name": "pak11_dir.vpk", "src": "x"}]}]}
        if Image:
            spec["previews"] = [{"src": "x", "out": "cover.jpg"}, {"src": "x", "out": "back.png"}]
        spec.update(over)
        top = spec["zip"]
        paks = {"files/0000-skin/pak10_dir.vpk": self.skin, "files/0001-voice/pak11_dir.vpk": self.voice}
        inter = {"mods": [{"name": m["name"], "order": m["order"],
                           "files": [{"path": p, "sha256": hashlib.sha256(b).hexdigest(), "size": len(b)}
                                     for p, b in paks.items() if p.startswith(f"files/{i:04d}-")],
                           "extensions": {"magician": {"release": release}}}
                          for i, m in enumerate(spec["mods"])]}
        members = dict(paks)
        members["mod-interchange.json"] = json.dumps(inter).encode()
        members["README.md"] = (f"# {spec['name']}\n\nIntro line one\nwrapped here.\n\nPreviews are in `previews/`.\n\n"
                                f"## Quick start\n\nCopy it.\n\n## Not seen yet\n\nMelee.{readme_extra}\n").encode()
        members["CHANGELOG.md"] = (f"# Changelog\n\n## {release} (2026-11-22)\n\n- **New** `thing`\n  wrapped.\n\n"
                                   "## 0.9.0\n\n- Old.\n").encode()
        if Image:
            members["previews/cover.jpg"] = png((200, 40, 40))
            members["previews/back.png"] = png((40, 40, 200))
        os.makedirs(os.path.join(self.d, "package"), exist_ok=True)
        zpath = os.path.join(self.d, top + ".zip")
        with zipfile.ZipFile(zpath, "w") as z:
            for rel in sorted(members):
                z.writestr(zipfile.ZipInfo(f"{top}/{rel}", date_time=(2026, 11, 22, 12, 0, 0)), members[rel])
        with open(os.path.join(self.d, "package", "bundle.json"), "w", encoding="utf-8") as f:
            json.dump(spec, f)
        return zpath

    def source(self, zpath, **over):
        s = {"source_id": "test-source", "name": "Test source", "base_url": "https://example.github.io/skins/",
             "out": self.out, "mods": [{"bundle": "package/bundle.json", "zip": zpath}]}
        s.update(over)
        p = os.path.join(self.d, "source.json")
        with open(p, "w", encoding="utf-8") as f:
            json.dump(s, f)
        return p

    def run_source(self, spec, *extra):
        return subprocess.run([sys.executable, "-I", os.path.join(TOOLS, "source.py"), spec] + list(extra),
                              capture_output=True, text=True)

    def ok(self, spec, *extra):
        r = self.run_source(spec, *extra)
        self.assertEqual(r.returncode, 0, r.stderr + r.stdout)
        return r

    def doc(self, *rel):
        with open(os.path.join(self.out, *rel), encoding="utf-8") as f:
            return json.load(f)

    def text(self, *rel):
        with open(os.path.join(self.out, *rel), encoding="utf-8") as f:
            return f.read()

    def snapshot(self):
        got = {}
        for dp, _, fs in os.walk(self.out):
            for f in fs:
                with open(os.path.join(dp, f), "rb") as h:
                    got[os.path.relpath(os.path.join(dp, f), self.out)] = h.read()
        return got

    def test_a_bundle_becomes_a_listing_with_its_zip_and_a_component_plan(self):
        z = self.bundle()
        self.ok(self.source(z))
        d = self.doc("dmm-source.json")
        self.assertEqual((d["format"], d["protocolVersion"], d["gameId"]), ("dmm-source", "1.0", "deadlock"))
        self.assertEqual(d["networkOrigins"]["downloads"], ["https://example.github.io"])
        self.assertFalse(os.path.exists(os.path.join(self.out, ".well-known")), "not at the origin's root")
        self.assertTrue(os.path.isfile(os.path.join(self.out, ".nojekyll")))
        listing = self.doc("mods", "test-skin.json")
        self.assertIs(listing["isNsfw"], False)
        self.assertEqual((listing["hero"], listing["contentType"]), ("lash", "skins"))
        rel = listing["release"]
        art = rel["artifacts"][0]
        with open(z, "rb") as f:
            data = f.read()
        self.assertEqual((art["size"], art["hashes"]["sha256"]), (len(data), hashlib.sha256(data).hexdigest()))
        self.assertEqual(art["download"]["url"], "../files/TestSkin_1.0.0.zip")
        with open(os.path.join(self.out, "files", "TestSkin_1.0.0.zip"), "rb") as f:
            self.assertEqual(f.read(), data)
        inst = rel["installation"]
        self.assertEqual(inst["mode"], "components")
        self.assertEqual(inst["requiredComponents"], ["skin"])
        self.assertEqual(inst["components"][1]["files"][0]["path"], "TestSkin_1.0.0/files/0001-voice/pak11_dir.vpk")
        self.assertEqual(inst["choiceGroups"], [{"id": "optional-voice", "label": "Test Skin, voice", "minSelections": 0,
                                                 "maxSelections": 1, "componentIds": ["voice"], "defaultComponentIds": []}])
        self.assertEqual(rel["changelog"], "1.0.0 (2026-11-22)\n\n- New thing wrapped.")
        self.assertIn("install-scene.cmd", listing["description"], "the scene can't come through DMM; say how")
        self.assertIn("Intro line one wrapped here.", listing["description"])
        self.assertNotIn("previews/", listing["description"])
        self.assertIn("## Not seen yet\n\nMelee.", listing["description"])
        self.assertEqual(self.doc("mods", "test-skin", "releases.json")["items"][0]["id"], rel["id"])
        page = self.text("index.html")
        self.assertIn("deadlock-mod-manager://sources/install?url=https%3A%2F%2Fexample.github.io%2Fskins%2Fdmm-source.json"
                      "&amp;source=test-source&amp;listing=test-skin", page)
        if Image:
            self.assertEqual(listing["images"], ["../media/test-skin/cover.jpg", "../media/test-skin/back.jpg"])
            with open(os.path.join(self.out, "media", "test-skin", "back.jpg"), "rb") as f:
                self.assertEqual(f.read(3), b"\xff\xd8\xff", "a PNG preview is served as JPEG")
        if CHECK and shutil.which("node"):
            r = subprocess.run(["node", CHECK, self.out], capture_output=True, text=True)
            self.assertEqual(r.returncode, 0, r.stderr)

    def test_the_same_inputs_give_the_same_tree(self):
        spec = self.source(self.bundle())
        self.ok(spec)
        first = self.snapshot()
        self.ok(spec)
        self.assertEqual(self.snapshot(), first)

    def test_the_revision_moves_with_the_bytes_and_only_with_them(self):
        spec = self.source(self.bundle())
        self.ok(spec)
        r1 = self.doc("mods", "test-skin.json")["release"]
        self.bundle(readme_extra=" And a new sentence.")    # words change, paks don't, but the zip's bytes do
        self.ok(spec)
        r2 = self.doc("mods", "test-skin.json")["release"]
        self.assertNotEqual(r1["contentRevision"], r2["contentRevision"], "the zip is the artifact: new zip, new revision")
        self.ok(spec)
        self.assertEqual(self.doc("mods", "test-skin.json")["release"], r2)
        # a description in the source spec alone is not a new release
        z = os.path.join(self.d, "TestSkin_1.0.0.zip")
        spec = self.source(z, mods=[{"bundle": "package/bundle.json", "zip": z, "sections": []}])
        self.ok(spec)
        r3 = self.doc("mods", "test-skin.json")
        self.assertEqual(r3["release"]["contentRevision"], r2["contentRevision"])
        self.assertNotIn("Not seen yet", r3["description"])

    def test_a_new_release_keeps_the_old_one_in_the_history_and_its_zip_until_prune(self):
        spec = self.source(self.bundle())
        self.ok(spec)
        old_id = self.doc("mods", "test-skin.json")["release"]["id"]
        self.skin = b"new skin pak bytes"
        z2 = self.bundle(release="1.1.0")
        spec = self.source(z2)
        r = self.ok(spec)
        self.assertIn("kept files/TestSkin_1.0.0.zip", r.stdout)
        items = self.doc("mods", "test-skin", "releases.json")["items"]
        self.assertEqual([i["version"] for i in items], ["1.1.0", "1.0.0"])
        self.assertEqual(items[1]["id"], old_id)
        self.assertEqual(items[1]["changelog"], "1.0.0 (2026-11-22)\n\n- New thing wrapped.")
        self.ok(spec, "--prune")
        self.assertFalse(os.path.exists(os.path.join(self.out, "files", "TestSkin_1.0.0.zip")))
        self.assertTrue(os.path.exists(os.path.join(self.out, "files", "TestSkin_1.1.0.zip")))
        self.assertEqual(len(self.doc("mods", "test-skin", "releases.json")["items"]), 2, "history outlives the zip")

    def test_github_release_downloads_declare_the_host_they_redirect_to(self):
        spec = self.source(self.bundle(), downloads_url="https://github.com/me/skins/releases/download/{id}-{version}/")
        r = self.ok(spec)
        d = self.doc("dmm-source.json")
        self.assertEqual(d["networkOrigins"]["downloads"], ["https://github.com", "https://release-assets.githubusercontent.com"])
        self.assertEqual(d["networkOrigins"]["api"], ["https://example.github.io"])
        art = self.doc("mods", "test-skin.json")["release"]["artifacts"][0]
        self.assertEqual(art["download"]["url"], "https://github.com/me/skins/releases/download/test-skin-1.0.0/TestSkin_1.0.0.zip")
        self.assertFalse(os.path.exists(os.path.join(self.out, "files")), "the zip is uploaded there, not copied here")
        self.assertIn("upload ", r.stdout)
        self.assertIn("release-assets.githubusercontent.com", r.stdout)

    def test_a_source_at_the_root_of_its_origin_also_has_a_well_known_descriptor(self):
        self.ok(self.source(self.bundle(), base_url="https://mods.example.org"))
        self.assertEqual(self.doc(".well-known", "dmm-source.json")["catalog"]["indexUrl"], "../catalog.json")
        self.assertEqual(self.doc("dmm-source.json")["catalog"]["indexUrl"], "./catalog.json")

    def test_a_bad_spec_is_refused_before_anything_is_written(self):
        z = self.bundle()
        cases = [
            (dict(nsfw=None), "nsfw must be true or false"),
            (dict(content_type="Skins"), "not a DMM content type"),
            (dict(hero_id="Lash"), "not a taxonomy id"),
        ]
        for over, why in cases:
            z = self.bundle(**over)
            r = self.run_source(self.source(z))
            self.assertNotEqual(r.returncode, 0, why)
            self.assertIn(why, r.stderr)
            self.assertFalse(os.path.exists(self.out), why)
        z = self.bundle()
        for over, why in ((dict(base_url="http://example.org/"), "must be https"),
                          (dict(source_id="My Source"), "source_id"),
                          (dict(mods=[{"bundle": "package/bundle.json", "zip": z, "id": "../up"}]), "listing id")):
            r = self.run_source(self.source(z, **over))
            self.assertNotEqual(r.returncode, 0, why)
            self.assertIn(why, r.stderr)
            self.assertFalse(os.path.exists(self.out), why)

    def test_a_zip_that_does_not_match_its_spec_or_its_manifest_is_refused(self):
        z = self.bundle()
        spec = self.source(z)
        with open(os.path.join(self.d, "package", "bundle.json"), encoding="utf-8") as f:
            b = json.load(f)
        b["release"] = "2.0.0"
        with open(os.path.join(self.d, "package", "bundle.json"), "w", encoding="utf-8") as f:
            json.dump(b, f)
        r = self.run_source(spec)
        self.assertIn("rebuild the zip", r.stderr)
        self.bundle()
        with zipfile.ZipFile(z) as src:
            members = {i.filename: src.read(i) for i in src.infolist()}
        members["TestSkin_1.0.0/files/0000-skin/pak10_dir.vpk"] = b"tampered"
        with zipfile.ZipFile(z, "w") as dst:
            for k, v in members.items():
                dst.writestr(k, v)
        r = self.run_source(spec)
        self.assertNotEqual(r.returncode, 0)
        self.assertIn("does not match the sha256", r.stderr)
        self.assertFalse(os.path.exists(self.out))

    def test_files_it_did_not_write_are_never_touched(self):
        spec = self.source(self.bundle())
        os.makedirs(self.out)
        for name, text in (("README.md", "the repo's own readme"), ("CNAME", "skins.example.org")):
            with open(os.path.join(self.out, name), "w") as f:
                f.write(text)
        self.ok(spec)
        with open(os.path.join(self.out, "index.html"), "w") as f:
            f.write("hand-made")
        os.remove(os.path.join(self.out, ".dmm-source-files"))  # as if it never wrote it
        r = self.run_source(spec)
        self.assertNotEqual(r.returncode, 0)
        self.assertIn("did not write it", r.stderr)
        self.assertEqual(self.text("index.html"), "hand-made")
        self.assertEqual(self.text("README.md"), "the repo's own readme")
        self.assertEqual(self.text("CNAME"), "skins.example.org")

    def test_a_manifest_naming_a_path_outside_out_is_refused(self):
        spec = self.source(self.bundle())
        self.ok(spec)
        victim = os.path.join(self.d, "victim.txt")
        with open(victim, "w") as f:
            f.write("keep me")
        with open(os.path.join(self.out, ".dmm-source-files"), "a") as f:
            f.write("../victim.txt\n")
        r = self.run_source(spec)
        self.assertNotEqual(r.returncode, 0)
        self.assertIn("not a plain path", r.stderr)
        self.assertTrue(os.path.exists(victim))

    def test_each_mod_has_its_own_page_and_the_listing_points_at_it(self):
        self.ok(self.source(self.bundle(), mods=[{"bundle": "package/bundle.json", "zip": os.path.join(self.d, "TestSkin_1.0.0.zip"),
                                                  "tagline": "A *line*.", "blurb": "Short blurb.",
                                                  "links": [{"label": "Trailer", "url": "https://example.com/t"}]}]))
        listing = self.doc("mods", "test-skin.json")
        self.assertEqual(listing["homepageUrl"], "https://example.github.io/skins/test-skin/")
        page = self.text("test-skin", "index.html")
        self.assertIn("<em>line</em>", page)
        self.assertIn('href="https://example.com/t"', page)
        self.assertIn('href="../files/TestSkin_1.0.0.zip"', page, "downloads are relative to the mod's own folder")
        self.assertIn("<h2>Not seen yet</h2>", page, "the page shows the description DMM shows")
        self.assertIn("<li>New thing wrapped.</li>", page, "and every release's notes")
        self.assertIn("&amp;source=test-source&amp;listing=test-skin", page)
        self.assertIn('<link rel="canonical" href="https://example.github.io/skins/test-skin/">', page)
        front = self.text("index.html")
        self.assertIn('href="test-skin/"', front)
        self.assertIn("Short blurb.", front)
        feed = self.text("feed.xml")
        self.assertIn("<title>Test Skin 1.0.0</title>", feed)
        self.assertIn("https://example.github.io/skins/test-skin/#" + listing["release"]["id"], feed)

    def test_a_badge_is_a_pill_beside_the_version_on_the_card_and_the_page(self):
        self.ok(self.source(self.bundle(), mods=[{"bundle": "package/bundle.json", "zip": os.path.join(self.d, "TestSkin_1.0.0.zip"),
                                                  "badge": "early"}]))
        pill = '<span class="pill badge">early</span>'
        self.assertIn(pill, self.text("index.html"))
        self.assertIn(pill, self.text("test-skin", "index.html"))
        self.assertNotIn("early", json.dumps(self.doc("mods", "test-skin.json")), "DMM has no badge; the listing is unchanged")

    def test_a_badge_is_a_short_plain_label(self):
        for bad in ("<b>early</b>", "x" * 25, ""):
            r = self.run_source(self.source(self.bundle(), mods=[{"bundle": "package/bundle.json", "badge": bad,
                                                                  "zip": os.path.join(self.d, "TestSkin_1.0.0.zip")}]))
            self.assertNotEqual(r.returncode, 0, bad)
            self.assertIn("badge must be a short plain label", r.stderr + r.stdout)

    def proofs(self, *proofs):
        """A proof ledger beside the mod.toml, as `magician prove` writes it: (build, [(owner, bytes)]) per proof."""
        rows = ["# magician proofs/1", "build\tdate\tproof\towner\tfile\tcrc32\tsha256\tevidence\tevidence_sha256"]
        for build, files in proofs:
            pairs = sorted({(o, hashlib.sha256(b).hexdigest()) for o, b in files})
            h = hashlib.sha256(build.encode())
            for o, s in pairs:
                h.update(f"\n{o}\t{s}".encode())
            for o, s in pairs:
                rows.append(f"{build}\t2026-11-24\t{h.hexdigest()[:16]}\t{o}\tpak.vpk\t00000000\t{s}\t-\t-")
        with open(os.path.join(self.d, "proofs.tsv"), "w", encoding="utf-8") as f:
            f.write("\n".join(rows) + "\n")

    def test_the_proof_id_is_magicians(self):
        sys.path.insert(0, TOOLS)
        import source
        self.assertEqual(source.proof_id("6866", [("voice", "b" * 64), ("skin", "a" * 64)]), "7b1c92d29e5922d8",
                         "the value crates/magician/src/proofs.rs pins")

    def test_the_builds_the_zips_own_paks_were_proven_on_are_said_without_a_new_release(self):
        z = self.bundle()
        self.ok(self.source(z))
        before = self.doc("mods", "test-skin.json")
        self.assertNotIn("Proven in game", before["description"])
        # the summary alone names no bytes: it says nothing
        with open(os.path.join(self.d, "mod.toml"), "w", encoding="utf-8") as f:
            f.write('[mod]\nslug = "test-skin"\n\n[proven]\n# filled by prove\nbuilds = [6864, 6866, 6865]\n\n[other]\nbuilds = [1]\n')
        self.ok(self.source(z))
        self.assertNotIn("Proven in game", self.doc("mods", "test-skin.json")["description"])
        # proofs of these bytes (both paks, one proof each build), of other bytes (6864), and one of the skin alone
        old = b"the pak before this release"
        self.proofs(("6864", [("test_skin", old), ("test_voice", self.voice)]),
                    ("6865", [("test_skin", self.skin), ("test_voice", self.voice)]),
                    ("6866", [("test_skin", self.skin), ("test_voice", self.voice)]),
                    ("6867", [("test_skin", self.skin)]))
        self.ok(self.source(z))
        after = self.doc("mods", "test-skin.json")
        self.assertIn("Proven in game on builds 6865 and 6866.", after["description"])
        self.assertNotIn("6864", after["description"].split("Proven in game")[1], "6864 was proven for other bytes")
        self.assertEqual(after["release"]["id"], before["release"]["id"], "a re-prove is not an update")
        self.assertIn("proven on Deadlock build 6866", self.text("test-skin", "index.html"))
        # a proof that lost a row in a merge counts for nothing
        with open(os.path.join(self.d, "proofs.tsv"), encoding="utf-8") as f:
            lines = f.read().splitlines()
        drop = next(i for i, l in enumerate(lines) if l.startswith("6866\t") and "\ttest_voice\t" in l)
        with open(os.path.join(self.d, "proofs.tsv"), "w", encoding="utf-8") as f:
            f.write("\n".join(lines[:drop] + lines[drop + 1:] + ["<<<<<<< HEAD"]) + "\n")
        self.ok(self.source(z))
        self.assertIn("Proven in game on build 6865.", self.doc("mods", "test-skin.json")["description"])
        # a rebuilt release: its paks have no proof, so the listing says nothing about proofs
        self.skin = b"skin pak bytes, rebuilt"
        z = self.bundle(release="1.0.1")
        self.ok(self.source(z))
        self.assertNotIn("Proven in game", self.doc("mods", "test-skin.json")["description"])
        self.assertNotIn("proven on Deadlock build", self.text("test-skin", "index.html"))
        r = self.run_source(self.source(z, mods=[{"bundle": "package/bundle.json", "zip": z, "mod_toml": "nope.toml"}]))
        self.assertNotEqual(r.returncode, 0)

    def test_the_page_block_dresses_every_page_in_the_sites_own_chrome(self):
        for name, body in (("t.css", ".x { color: red; }"), ("h.html", '<a href="{root}">home</a>'), ("f.html", "<p>foot</p>")):
            with open(os.path.join(self.d, name), "w", encoding="utf-8") as f:
                f.write(body)
        self.ok(self.source(self.bundle(), page={"title": "My skins", "intro": "Hello **there**.", "css": "t.css",
                                                 "header": "h.html", "footer": "f.html"}))
        front, mod = self.text("index.html"), self.text("test-skin", "index.html")
        for page in (front, mod):
            self.assertIn(".x { color: red; }", page)
            self.assertIn("<p>foot</p>", page)
        self.assertIn("<title>My skins</title>", front)
        self.assertIn("Hello <strong>there</strong>.", front)
        self.assertIn('<a href="">home</a>', front)
        self.assertIn('<a href="../">home</a>', mod, "{root} is the way back to the top from each page")
        r = self.run_source(self.source(self.bundle(), page={"css": "missing.css"}))
        self.assertNotEqual(r.returncode, 0)
        self.assertIn("page.css", r.stderr)

    def test_the_add_source_button_leads_the_front_page_and_the_bottom_keeps_the_address(self):
        z = self.bundle()
        self.ok(self.source(z))
        front = self.text("index.html")
        add = 'href="deadlock-mod-manager://sources/add?url=https%3A%2F%2Fexample.github.io%2Fskins%2Fdmm-source.json"'
        header = front[front.index('<header class="top">'):front.index("</header>")]
        self.assertIn(add, header, "the button sits in the header, under the title and the lede")
        self.assertLess(header.index('class="lede"'), header.index(add))
        self.assertIn("<span>Add to Deadlock Mod Manager</span>", header)
        self.assertIn('<svg class="add-glyph"', header)
        self.assertIn('aria-hidden="true"', header)
        self.assertIn("One click adds every mod here", header)
        self.assertIn("Get the mod manager", header)
        self.assertEqual(front.count(add), 1, "one button; the bottom is the manual fallback")
        bottom = front[front.index('id="source"'):]
        self.assertIn("https://example.github.io/skins/dmm-source.json</code>", bottom)
        self.assertIn("If the button does nothing", bottom)
        self.assertNotIn("deadlock-mod-manager://sources/add", bottom)
        self.assertNotIn('class="add-source"', self.text("test-skin", "index.html"), "a mod's page keeps its own install button")

        self.ok(self.source(z, page={"add_source": {"label": "Add my skins", "note": "Every **skin** here, one click."}}))
        front = self.text("index.html")
        self.assertIn("<span>Add my skins</span>", front)
        self.assertIn("Every <strong>skin</strong> here, one click.", front)

        self.ok(self.source(z, page={"add_source": False}))
        front = self.text("index.html")
        self.assertNotIn("deadlock-mod-manager://sources/add", front)
        self.assertNotIn("If the button does nothing", front)
        self.assertIn("https://example.github.io/skins/dmm-source.json</code>", front)

        for bad in ("yes", {"label": "<b>x</b>"}, {"label": "x" * 49}, {"colour": "red"}, {"note": ""}):
            r = self.run_source(self.source(z, page={"add_source": bad}))
            self.assertNotEqual(r.returncode, 0, bad)
            self.assertIn("page.add_source", r.stderr + r.stdout)

    def test_what_is_coming_is_a_card_and_if_asked_a_listing_with_nothing_to_install(self):
        z = self.bundle()
        self.ok(self.source(z, upcoming=[{"title": "Next Skin", "hero": "Abrams", "note": "Soon."}]))
        self.assertIn("Next Skin", self.text("index.html"))
        self.assertEqual([i["id"] for i in self.doc("catalog.json")["items"]], ["test-skin"], "a card alone is not a listing")
        self.ok(self.source(z, upcoming=[{"title": "Next Skin", "id": "next-skin", "hero": "Abrams", "hero_id": "abrams",
                                          "author": "me", "nsfw": False, "note": "Soon.", "in_manager": True}]))
        self.assertEqual([(i["id"], i["availability"]) for i in self.doc("catalog.json")["items"]],
                         [("test-skin", "available"), ("next-skin", "unavailable")])
        coming = self.doc("mods", "next-skin.json")
        self.assertNotIn("release", coming, "DMM refuses an unavailable listing that pretends to install")
        self.assertIn("Not released yet", coming["description"])
        if CHECK and shutil.which("node"):
            r = subprocess.run(["node", CHECK, self.out], capture_output=True, text=True)
            self.assertEqual(r.returncode, 0, r.stderr)
        for bad in ({"title": "x", "in_manager": True}, {"title": "x", "id": "test-skin"}, {"title": "x", "id": "mods"}, {"note": "n"}):
            r = self.run_source(self.source(z, upcoming=[bad]))
            self.assertNotEqual(r.returncode, 0, bad)

    def ld(self, *rel):
        """Every schema.org script element of a page, parsed, and the raw text of each."""
        page = self.text(*rel)
        raw = re.findall(r'<script type="application/ld\+json">\n(.*?)\n</script>', page, flags=re.S)
        return [json.loads(r) for r in raw], raw

    def test_each_page_carries_structured_data_from_what_the_listing_says(self):
        z = self.bundle()
        self.ok(self.source(z, authors={"me": "https://me.example.org/"}, mods=[{
            "bundle": "package/bundle.json", "zip": z, "blurb": "Short blurb.",
            "links": [{"label": "Trailer", "url": "https://www.youtube.com/watch?v=x", "kind": "video"},
                      {"label": "Nexus Mods", "url": "https://www.nexusmods.com/deadlock/mods/1", "kind": "same"},
                      {"label": "Notes", "url": "https://example.com/notes"}]}]))
        (doc,), _ = self.ld("test-skin", "index.html")
        app, crumbs = doc["@graph"]
        self.assertEqual(doc["@context"], "https://schema.org")
        self.assertEqual((app["@type"], app["name"], app["url"]),
                         ("SoftwareApplication", "Test Skin", "https://example.github.io/skins/test-skin/"))
        self.assertEqual(app["author"], {"@type": "Person", "name": "me", "url": "https://me.example.org/"})
        self.assertEqual((app["softwareVersion"], app["datePublished"], app["dateModified"]), ("1.0.0", "2026-11-22", "2026-11-22"))
        self.assertEqual(app["softwareRequirements"]["name"], "Deadlock")
        with open(z, "rb") as f:
            sha = hashlib.sha256(f.read()).hexdigest()
        self.assertEqual(app["downloadUrl"], "https://example.github.io/skins/files/TestSkin_1.0.0.zip")
        self.assertEqual((app["encoding"]["sha256"], app["encoding"]["contentUrl"]), (sha, app["downloadUrl"]))
        self.assertEqual(app["sameAs"], ["https://www.nexusmods.com/deadlock/mods/1"], "a plain link is not the same work")
        self.assertEqual(app["subjectOf"], [{"@type": "VideoObject", "name": "Trailer", "url": "https://www.youtube.com/watch?v=x"}])
        self.assertEqual(app["releaseNotes"], "1.0.0 (2026-11-22)\n\n- New thing wrapped.")
        self.assertIn("listing=test-skin", app["installUrl"])
        self.assertNotIn("6864", json.dumps(doc), "the build it was made for is no proof, and is not said here")
        self.assertEqual([i["item"] for i in crumbs["itemListElement"]],
                         ["https://example.github.io/skins/", "https://example.github.io/skins/test-skin/"])
        page = self.text("test-skin", "index.html")
        for href in ("https://www.youtube.com/watch?v=x", "https://www.nexusmods.com/deadlock/mods/1", "https://example.com/notes"):
            self.assertIn(f'href="{href}">', page, "every link is a plain anchor")
        self.assertNotIn("nofollow", page)
        self.assertIn('by <a href="https://me.example.org/" rel="author">me</a>', page)
        self.assertIn('<link rel="alternate" type="application/json" title="Deadlock Mod Manager listing" href="../mods/test-skin.json">', page)
        (front,), _ = self.ld("index.html")
        self.assertEqual(front["@type"], "CollectionPage")
        self.assertEqual(front["mainEntity"]["itemListElement"],
                         [{"@type": "ListItem", "position": 1, "url": "https://example.github.io/skins/test-skin/", "name": "Test Skin"}])
        self.assertIn('<link rel="alternate" type="application/json" title="Deadlock Mod Manager catalog" href="catalog.json">',
                      self.text("index.html"))
        if Image:
            self.assertEqual(app["image"][0], {"@type": "ImageObject", "contentUrl": "https://example.github.io/skins/media/test-skin/cover.jpg",
                                               "width": 64, "height": 40})
            self.assertIn('<meta property="og:image:width" content="64">\n<meta property="og:image:height" content="40">', page)
        # a second release keeps the first one's date as the mod's
        self.skin = b"new skin pak bytes"
        z2 = self.bundle(release="1.1.0", date="2026-12-01")
        self.ok(self.source(z2))
        (doc,), _ = self.ld("test-skin", "index.html")
        self.assertEqual((doc["@graph"][0]["datePublished"], doc["@graph"][0]["dateModified"]), ("2026-11-22", "2026-12-01"))

    def test_a_hostile_name_cannot_leave_the_structured_data(self):
        name = 'Evil </script><script>alert(1)</script> <!-- & "quoted"  '
        self.ok(self.source(self.bundle(name=name, description="</SCRIPT> & <b>x</b>")))
        for rel in (("test-skin", "index.html"), ("index.html",)):
            docs, raw = self.ld(*rel)
            self.assertEqual(len(docs), 1, rel)
            self.assertFalse(re.search(r"[<>& ]", raw[0]), "markup characters are escaped inside the element")
            self.assertEqual(self.text(*rel).count("<script"), 2, "the JSON-LD and the gallery's, nothing injected")
        app = self.ld("test-skin", "index.html")[0][0]["@graph"][0]
        self.assertEqual(app["name"], name)
        self.assertEqual(app["description"], "</SCRIPT> & <b>x</b>")

    def test_the_sitemap_lists_every_page_with_its_newest_release(self):
        import xml.etree.ElementTree as ET
        z = self.bundle()
        self.ok(self.source(z, upcoming=[{"title": "Next Skin", "note": "Soon."}]))
        ns = {"s": "http://www.sitemaps.org/schemas/sitemap/0.9"}
        root = ET.fromstring(self.text("sitemap.xml").encode())
        rows = [(u.find("s:loc", ns).text, u.find("s:lastmod", ns).text) for u in root.findall("s:url", ns)]
        self.assertEqual(rows, [("https://example.github.io/skins/", "2026-11-22"),
                                ("https://example.github.io/skins/test-skin/", "2026-11-22")])
        self.assertIn("sitemap.xml", self.text(".dmm-source-files"))

    def test_a_spec_without_the_new_keys_renders_its_links_and_author_as_before(self):
        z = self.bundle(readme_extra="\n\n# A loud heading")
        self.ok(self.source(z, mods=[{"bundle": "package/bundle.json", "zip": z,
                                      "links": [{"label": "Trailer", "url": "https://example.com/t"}]}]))
        page = self.text("test-skin", "index.html")
        self.assertIn('<a class="more" href="https://example.com/t">Trailer</a></p>', page)
        self.assertIn("Lash &middot; by me &middot; version 1.0.0", page)
        app = self.ld("test-skin", "index.html")[0][0]["@graph"][0]
        self.assertNotIn("sameAs", app)
        self.assertNotIn("subjectOf", app)
        self.assertEqual(app["author"], {"@type": "Person", "name": "me"})
        self.assertEqual(page.count("<h1"), 1, "a heading in the description is not a second h1")
        self.assertIn("<h2>A loud heading</h2>", page)
        for bad, why in (([{"label": "x", "url": "https://e.org/", "kind": "mirror"}], "a kind is one of"),
                         ([{"label": "x", "url": "http://e.org/", "kind": "same"}], "needs an https address"),
                         ([{"label": "x"}], "needs a label and a url")):
            r = self.run_source(self.source(z, mods=[{"bundle": "package/bundle.json", "zip": z, "links": bad}]))
            self.assertNotEqual(r.returncode, 0, why)
            self.assertIn(why, r.stderr)
        r = self.run_source(self.source(z, authors={"me": "ftp://me.example.org/"}))
        self.assertNotEqual(r.returncode, 0)

    def test_a_mod_dropped_from_the_spec_takes_its_files_with_it(self):
        z = self.bundle()
        self.ok(self.source(z))
        self.assertTrue(os.path.exists(os.path.join(self.out, "mods", "test-skin.json")))
        spec = self.source(z, mods=[{"bundle": "package/bundle.json", "zip": z, "id": "renamed"}])
        self.ok(spec, "--prune")
        self.assertFalse(os.path.exists(os.path.join(self.out, "mods", "test-skin.json")))
        self.assertFalse(os.path.exists(os.path.join(self.out, "mods", "test-skin")))
        self.assertEqual([i["id"] for i in self.doc("catalog.json")["items"]], ["renamed"])
        self.assertFalse(os.path.exists(os.path.join(self.out, "test-skin")), "its page goes too")
        self.assertTrue(os.path.exists(os.path.join(self.out, "renamed", "index.html")))


if __name__ == "__main__":
    unittest.main()

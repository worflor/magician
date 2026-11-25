"""The Nexus Mods publisher against a fake Nexus on this machine: the seven steps as Nexus's upload API has them (the
key header, part sizes and ETags, the completion XML, the JSON bodies), each step failing in turn, a part that fails
and is tried again, a version already there, the dry run touching nothing, a missing key, the release gate's refusal
and its override, the 255-character description, plain http refused off this machine, and the key in no output.
Offline; needs no game and no magician binary (the gate's grade is stood in for).

    python -I tools/test_publish.py
"""
import contextlib, hashlib, io, json, os, re, shutil, subprocess, sys, tempfile, threading, unittest, zipfile
import xml.etree.ElementTree as ET
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

TOOLS = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, TOOLS)
import publish  # noqa: E402

KEY = "test-key-0123456789abcdef"
PART = 1000


class FakeNexus(ThreadingHTTPServer):
    """Nexus's v3 upload flow and a presigned storage host on one port. `fail` maps a step to (status, body)."""

    def __init__(self):
        super().__init__(("127.0.0.1", 0), Handler)
        self.base = f"http://127.0.0.1:{self.server_address[1]}"
        self.reset()

    def reset(self):
        self.calls, self.fail, self.part_failures = [], {}, {}
        self.parts, self.size, self.versions, self.changelogs, self.polls = {}, None, [], [], 0
        self.created = None
        self.errors = []


class Handler(BaseHTTPRequestHandler):
    def log_message(self, *a):
        pass

    def answer(self, status, doc=None, headers=None, raw=None):
        body = raw if raw is not None else (json.dumps(doc).encode() if doc is not None else b"")
        self.send_response(status)
        for k, v in (headers or {}).items():
            self.send_header(k, v)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def body(self):
        return self.rfile.read(int(self.headers.get("Content-Length") or 0))

    def handle_one(self, method):
        s, path, data = self.server, self.path, self.body()
        s.calls.append((method, path))
        storage = path.startswith("/storage/")
        if storage and self.headers.get("apikey") is not None:
            s.errors.append(f"the key was sent to storage: {method} {path}")
        if not storage and self.headers.get("apikey") != KEY:
            return self.answer(401, {"title": "Unauthorized", "detail": f"bad key {self.headers.get('apikey')}"})
        step = {("POST", "/v3/uploads/multipart"): "create", ("POST", "/storage/complete"): "complete"}.get((method, path))
        if path.startswith("/storage/part/"):
            step = "part"
        elif re.fullmatch(r"/v3/uploads/[^/]+/finalise", path):
            step = "finalise"
        elif method == "GET" and re.fullmatch(r"/v3/uploads/[^/]+", path):
            step = "poll"
        elif re.fullmatch(r"/v3/mod-files/[^/]+/versions", path):
            step = "versions" if method == "GET" else "version"
        elif path.startswith("/v3/mods/") and "/changelogs" in path:
            step = "changelogs" if method == "GET" else "changelog"
        if step in s.fail:
            status, text = s.fail[step]
            # a server that echoes the key back must not get it printed
            return self.answer(status, raw=text.replace("{key}", self.headers.get("apikey") or "").encode())

        if step == "create":
            doc = json.loads(data)
            if self.headers.get("Content-Type") != "application/json" or not isinstance(doc.get("size_bytes"), int) or not doc.get("filename"):
                s.errors.append(f"bad create body {doc}")
            s.size = doc["size_bytes"]
            s.created = doc
            n = -(-s.size // PART)
            return self.answer(201, {"data": {"id": "11111111-2222-3333-4444-555555555555", "user": {"id": "1"}, "state": "created",
                                              "part_size_bytes": PART, "complete_presigned_url": f"{s.base}/storage/complete",
                                              "part_presigned_urls": [f"{s.base}/storage/part/{i}" for i in range(1, n + 1)]}})
        if step == "part":
            i = int(path.rsplit("/", 1)[1])
            if s.part_failures.get(i, 0) > 0:
                s.part_failures[i] -= 1
                return self.answer(503, raw=b"slow down")
            n = -(-s.size // PART)
            want = PART if i < n else s.size - PART * (n - 1)
            if len(data) != want or self.headers.get("Content-Type") != "application/octet-stream":
                s.errors.append(f"part {i}: {len(data)} bytes, expected {want}")
            s.parts[i] = data
            return self.answer(200, headers={"ETag": '"' + hashlib.md5(data).hexdigest() + '"'})
        if step == "complete":
            if self.headers.get("Content-Type") != "application/xml":
                s.errors.append("complete without the XML content type")
            root = ET.fromstring(data)
            got = [(int(p.find("PartNumber").text), p.find("ETag").text) for p in root.findall("Part")]
            want = [(i, hashlib.md5(s.parts[i]).hexdigest()) for i in sorted(s.parts)]
            if root.tag != "CompleteMultipartUpload" or got != want:
                s.errors.append(f"complete XML {got} != {want}")
            return self.answer(200, raw=b"<CompleteMultipartUploadResult/>")
        if step == "finalise":
            return self.answer(200, {"data": {"id": "1", "user": {"id": "1"}, "state": "created"}})
        if step == "poll":
            s.polls += 1
            return self.answer(200, {"data": {"id": "1", "user": {"id": "1"}, "state": "available" if s.polls >= 2 else "created"}})
        if step == "versions":
            return self.answer(200, {"data": {"versions": s.versions}})
        if step == "version":
            doc = json.loads(data)
            for k, t in (("upload_id", str), ("name", str), ("version", str), ("file_category", str),
                         ("archive_existing_file", bool), ("update_mod_version", bool)):
                if not isinstance(doc.get(k), t):
                    s.errors.append(f"version body: {k} is {doc.get(k)!r}")
            if len(doc.get("description", "")) > 255:
                s.errors.append("description over 255")
            s.version_body = doc
            s.versions.append({"id": "v-new", "version": doc["version"], "category": doc["file_category"], "size_in_bytes": s.size})
            return self.answer(201, {"data": {"file": {"id": "f"}, "version": {"id": "v-new", "position": "2"}}})
        if step == "changelogs":
            return self.answer(200, {"data": [{"version": v, "entries": t.splitlines()} for v, t in s.changelogs],
                                     "meta": {"page": 1, "page_size": 50, "total_count": len(s.changelogs)}})
        if step == "changelog":
            doc = json.loads(data)
            s.changelogs.append((doc["version"], doc["changelog"]))
            return self.answer(201, {"data": doc})
        return self.answer(404, {"title": "Not Found", "detail": path})

    def do_GET(self):
        self.handle_one("GET")

    def do_POST(self):
        self.handle_one("POST")

    def do_PUT(self):
        self.handle_one("PUT")


def grade_doc(passes=True, blockers=()):
    return json.dumps({"schema": "magician.grade/3", "label": "Costume (look)", "freshness": {"target": 6868},
                       "release_gate": {"pass": passes, "blockers": list(blockers), "warnings": []}})


class Publish(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.server = FakeNexus()
        threading.Thread(target=cls.server.serve_forever, daemon=True).start()

    @classmethod
    def tearDownClass(cls):
        cls.server.shutdown()
        cls.server.server_close()

    def setUp(self):
        self.server.reset()
        self.d = tempfile.mkdtemp(prefix="magician-publish-test-")
        self.env = {k: os.environ.get(k) for k in ("NEXUSMODS_API_KEY", "NEXUSMODS_API_BASE")}
        os.environ["NEXUSMODS_API_KEY"] = KEY
        os.environ["NEXUSMODS_API_BASE"] = self.server.base + "/v3"
        self.saved = (publish.RETRY_PAUSE, publish.POLL_FIRST, publish.run_magician)
        publish.RETRY_PAUSE, publish.POLL_FIRST = 0, 0.01
        self.grade = grade_doc()
        self.graded = []

        def fake(cmd):
            self.graded.append(cmd)
            return subprocess.CompletedProcess(cmd, 0, self.grade, "")
        publish.run_magician = fake
        with open(os.path.join(self.d, "mod.toml"), "w") as f:
            f.write('[mod]\nhero = "hero_lash"\n')

    def tearDown(self):
        publish.RETRY_PAUSE, publish.POLL_FIRST, publish.run_magician = self.saved
        for k, v in self.env.items():
            if v is None:
                os.environ.pop(k, None)
            else:
                os.environ[k] = v
        shutil.rmtree(self.d, ignore_errors=True)
        self.assertEqual(self.server.errors, [])

    def bundle(self, release="1.0.2", nexus=None, **over):
        spec = {"name": "Test Skin", "slug": "test-skin", "author": "me", "hero": "Lash", "description": "A test skin.",
                "version": "b2", "release": release, "zip": f"TestSkin_{release}", "out": os.path.join(self.d, "packages", "test-skin"),
                "mods": [{"slug": "skin", "name": "Test Skin", "description": "d", "order": 0, "files": [{"name": "pak10_dir.vpk", "src": "x"}]}],
                "nexus": {"file_id": "4242", "mod_id": "77"} if nexus is None else nexus}
        spec.update(over)
        pak = os.urandom(3000)   # incompressible: the zip runs to four parts of PART bytes
        top = spec["zip"]
        inter = {"mods": [{"name": "Test Skin", "order": 0, "extensions": {"magician": {"release": release}},
                           "files": [{"path": "files/0000-skin/pak10_dir.vpk", "sha256": hashlib.sha256(pak).hexdigest(), "size": len(pak)}]}]}
        os.makedirs(os.path.join(self.d, "packages"), exist_ok=True)
        os.makedirs(os.path.join(self.d, "package"), exist_ok=True)
        self.zip = os.path.join(self.d, "packages", top + ".zip")
        with zipfile.ZipFile(self.zip, "w") as z:
            z.writestr(f"{top}/files/0000-skin/pak10_dir.vpk", pak)
            z.writestr(f"{top}/mod-interchange.json", json.dumps(inter))
            z.writestr(f"{top}/CHANGELOG.md", f"# Changelog\n\n## {release} (2026-11-25)\n\n- Ribbons stay down\n  in the air.\n- Tested.\n\n## 1.0.0\n\n- First.\n")
        p = os.path.join(self.d, "package", "bundle.json")
        with open(p, "w", encoding="utf-8") as f:
            json.dump(spec, f)
        return p

    def run_publish(self, *args):
        out, err = io.StringIO(), io.StringIO()
        with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
            code = publish.main(["nexus"] + list(args))
        o, e = out.getvalue(), err.getvalue()
        self.assertNotIn(KEY, o + e, "the key is never printed")
        return code, o, e

    def test_a_release_goes_up_in_seven_steps(self):
        code, o, e = self.run_publish(self.bundle())
        self.assertEqual(code, 0, o + e)
        s = self.server
        self.assertEqual(s.created, {"size_bytes": os.path.getsize(self.zip), "filename": "TestSkin_1.0.2.zip"})
        with open(self.zip, "rb") as f:
            self.assertEqual(b"".join(s.parts[i] for i in sorted(s.parts)), f.read(), "the parts are the zip")
        self.assertEqual(len(s.parts), 4)
        b = s.version_body
        self.assertEqual((b["version"], b["name"], b["file_category"], b["description"]), ("1.0.2", "Test Skin", "main", "A test skin."))
        self.assertEqual((b["archive_existing_file"], b["update_mod_version"]), (False, False))
        self.assertFalse({"primary_mod_manager_download", "allow_mod_manager_download", "show_requirements_pop_up"} & set(b), "left to Nexus")
        self.assertEqual(s.changelogs, [("1.0.2", "Ribbons stay down in the air.\nTested.")])
        order = [c for c in s.calls if not c[1].startswith("/storage/part/")]
        self.assertEqual([m for m, _ in order], ["GET", "POST", "POST", "POST", "GET", "GET", "POST", "GET", "POST"])
        self.assertIn("published: TestSkin_1.0.2.zip is version 1.0.2 of file 4242 (version id v-new)", o)
        for n in range(1, 8):
            self.assertIn(f"step {n} of 7", o)
        # the gate graded the zip's own pak with the mod's mod.toml
        cmd = self.graded[0]
        self.assertEqual((cmd[1], cmd[-3], cmd[-2], cmd[-1]), ("grade", "--mod", os.path.join(self.d, "mod.toml"), "--json"))
        self.assertTrue(cmd[2].endswith("pak10_dir.vpk"))

    def test_the_switches_go_as_nexus_names_them(self):
        nexus = {"file_id": "4242", "category": "optional", "archive_existing_file": True, "update_mod_version": True,
                 "allow_mod_manager_download": False, "show_requirements_pop_up": True, "name": "Test Skin (optional)", "changelog": False}
        code, o, e = self.run_publish(self.bundle(nexus=nexus))
        self.assertEqual(code, 0, o + e)
        b = self.server.version_body
        self.assertEqual((b["file_category"], b["archive_existing_file"], b["update_mod_version"], b["allow_mod_manager_download"],
                          b["show_requirements_pop_up"], b["name"]), ("optional", True, True, False, True, "Test Skin (optional)"))
        self.assertEqual(self.server.changelogs, [])
        self.assertIn("step 7 of 7: no changelog sent", o)

    def test_a_long_description_is_cut_at_a_word_to_255(self):
        long = "Lash in a maid's outfit, " * 20
        code, o, e = self.run_publish(self.bundle(description=long))
        self.assertEqual(code, 0, o + e)
        d = self.server.version_body["description"]
        self.assertLessEqual(len(d), 255)
        self.assertTrue(d.endswith("...") and long.startswith(d[:-3]), d)
        self.assertEqual(publish.cut("x" * 300), "x" * 252 + "...")
        self.assertEqual(publish.cut("short"), "short")

    def test_each_step_that_fails_is_named_with_the_servers_words(self):
        for step, said in (("versions", "read file 4242's versions: HTTP 404"), ("create", "step 1 of 7, create the multipart upload: HTTP 422"),
                           ("complete", "step 3 of 7, complete the multipart upload: HTTP 400"), ("finalise", "step 4 of 7, finalise the upload: HTTP 500"),
                           ("poll", "step 5 of 7, wait for the upload to be available: HTTP 500"),
                           ("version", "step 6 of 7, create version 1.0.2 of file 4242: HTTP 422"),
                           ("changelog", "step 7 of 7, add the changelog to mod 77: HTTP 403")):
            self.server.reset()
            status = int(re.search(r"HTTP (\d+)", said).group(1))
            self.server.fail = {step: (status, json.dumps({"title": "No", "detail": f"the server says no to {step}; key {{key}}"}))}
            code, o, e = self.run_publish(self.bundle())
            self.assertEqual(code, 1, step)
            self.assertIn(said, e, step)
            self.assertIn(f"No - the server says no to {step}", e, step)
            if step != "complete":   # storage is never sent the key, so has none to echo
                self.assertIn("[NEXUSMODS_API_KEY]", e, "an echoed key is hidden")
            self.assertNotIn("published:", o)

    def test_a_part_that_fails_is_tried_again_and_gives_up_in_the_end(self):
        self.server.part_failures = {2: 2}
        code, o, e = self.run_publish(self.bundle())
        self.assertEqual(code, 0, o + e)
        self.assertIn("part 2: step 2 of 7, upload part 2 of 4: HTTP 503: slow down; trying again", o)
        self.server.reset()
        self.server.part_failures = {3: 99}
        code, o, e = self.run_publish(self.bundle())
        self.assertEqual(code, 1)
        self.assertIn("step 2 of 7, upload part 3 of 4: HTTP 503: slow down (tried 4 times)", e)
        self.assertFalse(any(p == "/storage/complete" for _, p in self.server.calls), "never completed")

    def test_the_same_version_is_not_uploaded_twice(self):
        p = self.bundle()
        self.server.versions = [{"id": "v-old", "version": "1.0.2", "category": "main", "size_in_bytes": os.path.getsize(self.zip)}]
        code, o, e = self.run_publish(p)
        self.assertEqual(code, 0, o + e)
        self.assertIn("already published: version 1.0.2 is on file 4242 (version id v-old", o)
        self.assertFalse(any(c[1] == "/v3/uploads/multipart" for c in self.server.calls))
        self.assertEqual(self.server.changelogs, [("1.0.2", "Ribbons stay down in the air.\nTested.")], "the missing changelog is added")
        code, o, e = self.run_publish(p)
        self.assertEqual(code, 0)
        self.assertIn("already has one for 1.0.2; not added again", o)
        self.assertEqual(len(self.server.changelogs), 1)
        # the same version with other bytes is refused
        self.server.versions[0]["size_in_bytes"] = 5
        code, o, e = self.run_publish(p)
        self.assertEqual(code, 3)
        self.assertIn("new bytes need a new release", e)

    def test_a_dry_run_says_everything_and_contacts_nothing(self):
        code, o, e = self.run_publish(self.bundle(description="d " * 200), "--dry-run")
        self.assertEqual(code, 0, o + e)
        self.assertEqual(self.server.calls, [])
        with open(self.zip, "rb") as f:
            self.assertIn(hashlib.sha256(f.read()).hexdigest(), o)
        for said in ("dry run, nothing is sent", "version      1.0.2", "file id      4242", "mod id       77", "category     main",
                     "archive_existing_file          false", "allow_mod_manager_download     not sent (Nexus decides)",
                     "of 255 characters, cut from 399)", "changelog    1.0.2, to mod 77:", "      Ribbons stay down in the air.",
                     "NEXUSMODS_API_KEY is set", "release gate: pass", "dry run: nothing sent"):
            self.assertIn(said, o)
        # without ids or a key it still says everything, and says what is missing
        os.environ.pop("NEXUSMODS_API_KEY")
        code, o, e = self.run_publish(self.bundle(nexus={}), "--dry-run")
        self.assertEqual(code, 2)
        self.assertIn("file id      not set", o)
        self.assertIn("NEXUSMODS_API_KEY is not set", o)
        self.assertIn("no nexus.file_id", e)
        self.assertEqual(self.server.calls, [])

    def test_a_missing_key_is_a_usage_error_that_says_where_to_get_one(self):
        os.environ.pop("NEXUSMODS_API_KEY")
        code, o, e = self.run_publish(self.bundle())
        self.assertEqual(code, 2)
        self.assertIn("NEXUSMODS_API_KEY is not set", e)
        self.assertIn(publish.KEY_PAGE, e)
        self.assertEqual(self.server.calls, [])

    def test_the_release_gate_refuses_and_can_be_overridden(self):
        p = self.bundle()
        self.grade = grade_doc(False, ["freshness: not proven on 6868 (no proven build on record for these bytes)"])
        code, o, e = self.run_publish(p)
        self.assertEqual(code, 3)
        self.assertIn("refusing to publish: the release gate has a blocker (freshness: not proven on 6868", e)
        self.assertEqual(self.server.calls, [], "nothing sent")
        code, o, e = self.run_publish(p, "--dry-run")
        self.assertEqual(code, 3, "a dry run says the gate would refuse")
        code, o, e = self.run_publish(p, "--ignore-gate")
        self.assertEqual(code, 0, o + e)
        self.assertIn("WARNING: --ignore-gate: publishing past the release gate", o)
        # a gate that could not judge refuses too; so does a mod without a mod.toml
        self.server.reset()
        self.grade = ""
        code, o, e = self.run_publish(p)
        self.assertEqual(code, 3)
        self.assertIn("could not judge", e)
        code, o, e = self.run_publish(p, "--mod", os.path.join(self.d, "nowhere.toml"))
        self.assertEqual(code, 3)
        self.assertIn("no mod.toml at", e)
        self.assertEqual(self.server.calls, [])

    def test_a_bundle_that_does_not_hold_together_is_refused_before_anything(self):
        p = self.bundle()
        with open(p, encoding="utf-8") as f:
            spec = json.load(f)
        spec["release"] = "1.0.3"
        with open(p, "w", encoding="utf-8") as f:
            json.dump(spec, f)
        code, o, e = self.run_publish(p, "--zip", self.zip)
        self.assertEqual(code, 2)
        self.assertIn("is release '1.0.2', ", e)
        for nexus, said in (({"file_id": "../x"}, "is not an id"), ({"file_id": "1", "categroy": "main"}, "categroy"),
                            ({"file_id": "1", "category": "update"}, "is one of main"), ({"file_id": "1", "archive_existing_file": "yes"}, "true or false"),
                            ({"file_id": "1", "category": "optional", "primary_mod_manager_download": True}, "only a main file")):
            code, o, e = self.run_publish(self.bundle(nexus=nexus))
            self.assertEqual(code, 2, nexus)
            self.assertIn(said, e, nexus)
        self.assertEqual(self.server.calls, [])

    def test_plain_http_is_refused_off_this_machine_and_redirects_are_not_followed(self):
        os.environ["NEXUSMODS_API_BASE"] = "http://api.example.com/v3"
        code, o, e = self.run_publish(self.bundle())
        self.assertEqual(code, 2)
        self.assertIn("is not https", e)
        self.assertEqual(publish.check_url("https://api.nexusmods.com/v3", "x"), "https://api.nexusmods.com/v3")
        with self.assertRaises(publish.Stop):
            publish.check_url("http://10.0.0.1/storage", "a presigned URL from Nexus")
        # a redirect would carry the key elsewhere: it is a failure, not followed
        os.environ["NEXUSMODS_API_BASE"] = self.server.base + "/v3"
        self.server.fail = {"versions": (302, "")}
        code, o, e = self.run_publish(self.bundle())
        self.assertEqual(code, 1)
        self.assertIn("HTTP 302", e)

    def test_only_nexus_is_a_destination(self):
        out, err = io.StringIO(), io.StringIO()
        with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
            self.assertEqual(publish.main(["steam", "x.json"]), 2)
            self.assertEqual(publish.main(["nexus", "x.json", "--bogus"]), 2)
            self.assertEqual(publish.main(["nexus", "--help"]), 0)
        self.assertIn("the one destination is nexus", err.getvalue())
        self.assertIn("--bogus is not an option", err.getvalue())

    def test_the_key_is_in_no_output_even_run_as_a_program(self):
        # the whole program, its own process: a dry run and a refusal, the key in the environment
        p = self.bundle()
        env = dict(os.environ, MAGICIAN_EXE=os.path.join(self.d, "no-such-magician"))
        for extra in (["--dry-run"], []):
            r = subprocess.run([sys.executable, "-I", os.path.join(TOOLS, "publish.py"), "nexus", p] + extra, capture_output=True, text=True, env=env)
            self.assertEqual(r.returncode, 3, r.stdout + r.stderr)   # no magician: the gate can't judge
            self.assertNotIn(KEY, r.stdout + r.stderr)
            self.assertIn("magician did not start", r.stdout)
        self.assertEqual(self.server.calls, [])


if __name__ == "__main__":
    unittest.main()

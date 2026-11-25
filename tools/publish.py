"""Publish a release bundle somewhere players get it. Python stdlib only.

    python publish.py nexus <bundle.json> [--zip FILE] [--mod mod.toml] [--dry-run] [--ignore-gate] [--magician EXE]

One destination today, Nexus Mods (`nexus`): the release zip `bundle.py --zip` wrote is uploaded as a new version of a
file on an existing Nexus Mods page, through Nexus's upload API (the flow of their own GitHub Action,
https://github.com/Nexus-Mods/upload-action). The API adds versions to files that exist: the mod page and its first
file are made by hand on the site, and their ids go into bundle.json:

    "nexus": {"mod_id": "...", "file_id": "...", "category": "main"}

file_id (required) is the file the version is added to and mod_id (for the changelog) the page, both the ids the
Files tab shows under Advanced (or the file's edit menu), not the number in the page's address. Optional, with Nexus's
own names: category (main, optional or miscellaneous; default main), name (default the bundle's name), description
(default the bundle's; cut at a word to Nexus's 255 characters), archive_existing_file and update_mod_version (default
false), primary_mod_manager_download, allow_mod_manager_download and show_requirements_pop_up (left to Nexus unless
given), changelog (false sends none; else the zip's CHANGELOG.md section for the release goes to mod_id).

The zip is the one `bundle.py --zip` wrote beside the bundle's `out` folder (--zip names another); it must be the
release bundle.json says, with paks that match its own mod-interchange.json. The key is NEXUSMODS_API_KEY, read from
the environment only, never written anywhere and never shown. NEXUSMODS_API_BASE points at another server (a test
one); plain http is refused anywhere but this machine.

The release gate: before anything is sent, the zip's own paks are graded with the mod's mod.toml (the one beside the
bundle's folder, or --mod), exactly as `magician grade --gate` judges them: lint errors, files byte-identical to
Valve's, fair play when enforced, a proof of these bytes on the installed build, rights. A blocker refuses the upload
(exit 3) and says why; --ignore-gate publishes anyway and says so.

--dry-run says what would be uploaded where (file, size, sha256, version, name, description as it will be sent,
changelog, ids, switches) and runs the gate, and contacts nothing. Otherwise the file's versions are read first: the
same version already there with the same size is said and nothing is uploaded (a missing changelog is added); with
another size it is refused. Every request has a time limit, a part that fails is tried again a few times, and a step
that fails is named with the server's own words.

Exit codes: 0 published (or already there), 1 a step failed, 2 usage (bad arguments, a missing key or id, a bundle
that does not hold together), 3 refused (the release gate, a version already there with other bytes).
"""
import hashlib, json, math, os, re, shutil, subprocess, sys, tempfile, time, urllib.error, urllib.parse, urllib.request, zipfile

TOOLS = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, TOOLS)
import source  # noqa: E402  (the same `~/` paths, sha256 and changelog section as the mod source)

sys.path.insert(0, os.path.join(TOOLS, "..", "ingame"))
import gamepaths  # noqa: E402  (the magician binary, as the CLI finds it)

API_BASE = "https://api.nexusmods.com/v3"
KEY_PAGE = "https://www.nexusmods.com/settings/api-keys"
DESCRIPTION_MAX = 255                      # Nexus's limit on a file version's description
CATEGORIES = ("main", "optional", "miscellaneous")   # what a new version may be filed under
SWITCHES = ("archive_existing_file", "update_mod_version")   # always sent, default false
OPTIONAL_SWITCHES = ("primary_mod_manager_download", "allow_mod_manager_download", "show_requirements_pop_up")
NEXUS_KEYS = {"mod_id", "file_id", "category", "name", "description", "changelog"} | set(SWITCHES) | set(OPTIONAL_SWITCHES)
API_TIMEOUT = 30          # seconds, each API request
PART_TIMEOUT = 120        # seconds without progress, each part upload
PART_TRIES = 4            # a part upload is tried this many times
RETRY_PAUSE = 2.0         # seconds before the second try, doubling
POLL_FIRST = 2.0          # seconds before the first look at the upload's state, then 1.5 times longer, at most 30
POLL_LIMIT = 600          # seconds to wait for the upload to be available
SERVER_TEXT = 600         # how much of a server's answer an error quotes
USER_AGENT = "magician-publish"
SECRET = []               # the key, so nothing printed can carry it


class Stop(Exception):
    def __init__(self, code, msg):
        super().__init__(msg)
        self.code = code


def usage(msg):
    return Stop(2, msg)


def hide(text):
    text = str(text)
    for s in SECRET:
        if s:
            text = text.replace(s, "[NEXUSMODS_API_KEY]")
    return text


def say(*parts):
    print(hide(" ".join(str(p) for p in parts)), flush=True)


def utf16_len(s):
    """Length as Nexus's own client counts it (JavaScript string length)."""
    return len(s.encode("utf-16-le")) // 2


def cut(text, limit=DESCRIPTION_MAX):
    """`text` on one line, cut at the last word that fits `limit` with "..." when it is longer."""
    text = " ".join(text.split())
    if utf16_len(text) <= limit:
        return text
    out = ""
    for w in text.split(" "):
        nxt = (out + " " + w) if out else w
        if utf16_len(nxt) + 3 > limit:
            break
        out = nxt
    if not out:   # one word longer than the limit
        out = text
        while utf16_len(out) + 3 > limit:
            out = out[:-1]
    return out.rstrip(",;:") + "..."


def mb(n):
    return f"{n / 1e6:.1f} MB"


# --- the bundle ------------------------------------------------------------------------------------------------------

def read_bundle(spec_path, zip_arg):
    """bundle.json, its nexus block and its zip, checked against each other before anything else happens."""
    spec_path = os.path.abspath(spec_path)
    try:
        spec = json.load(open(spec_path, encoding="utf-8-sig"))
    except (OSError, ValueError) as e:
        raise usage(f"{spec_path}: {e}")
    base = os.path.dirname(spec_path)
    if not isinstance(spec, dict):
        raise usage(f"{spec_path} is not a JSON object")
    for k in ("name", "slug", "description", "release"):
        if not isinstance(spec.get(k), str) or not spec[k].strip():
            raise usage(f"{spec_path}: publishing needs {k}")
    if zip_arg:
        zpath = os.path.abspath(zip_arg)
    else:
        if not isinstance(spec.get("out"), str):
            raise usage(f"{spec_path} has no out, so its zip can't be found; --zip names it")
        out = os.path.abspath(source.resolve(base, spec["out"]))
        zpath = os.path.join(os.path.dirname(out), (spec.get("zip") or f"{spec['slug']}-{spec['release']}") + ".zip")
    if not os.path.isfile(zpath):
        raise usage(f"no zip at {zpath} (magician bundle {spec_path} --zip writes it)")
    try:
        z = zipfile.ZipFile(zpath)
    except (OSError, zipfile.BadZipFile) as e:
        raise usage(f"{zpath}: {e}")
    names = set(z.namelist())
    tops = {n.split("/", 1)[0] for n in names}
    if len(tops) != 1:
        raise usage(f"{zpath} is not a bundle zip (one top-level folder expected, found {len(tops)})")
    top = tops.pop()
    try:
        inter = json.loads(z.read(f"{top}/mod-interchange.json"))
    except KeyError:
        raise usage(f"{zpath} has no {top}/mod-interchange.json: not a zip bundle.py wrote")
    except ValueError as e:
        raise usage(f"{zpath}: mod-interchange.json: {e}")
    built = (inter.get("mods") or [{}])[0].get("extensions", {}).get("magician", {}).get("release")
    if built != spec["release"]:
        raise usage(f"{zpath} is release {built!r}, {spec_path} says {spec['release']!r}: publish the zip of that release, or rebuild it (magician bundle --zip)")
    paks = []
    for m in inter["mods"]:
        for f in m.get("files", []):
            member = f"{top}/{f.get('path', '')}"
            if member not in names:
                raise usage(f"{zpath}: no {member}, which its mod-interchange.json lists")
            h = hashlib.sha256()
            with z.open(member) as src:
                for b in iter(lambda: src.read(1 << 20), b""):
                    h.update(b)
            if h.hexdigest() != f.get("sha256") or z.getinfo(member).file_size != f.get("size"):
                raise usage(f"{zpath}: {member} does not match the sha256 and size in its mod-interchange.json")
            if member.lower().endswith(".vpk"):
                paks.append(member)
    if not paks:
        raise usage(f"{zpath} carries no pak")
    changelog = None
    if f"{top}/CHANGELOG.md" in names:
        section = source.changelog_section(z.read(f"{top}/CHANGELOG.md").decode("utf-8"), spec["release"])
        if section:
            lines = [re.sub(r"^[-*+]\s+", "", l).strip() for l in section.splitlines()[1:]]
            changelog = "\n".join(l for l in lines if l) or None
    return {"spec": spec, "spec_path": spec_path, "base": base, "zip": zpath, "zip_obj": z, "top": top, "paks": paks,
            "size": os.path.getsize(zpath), "sha256": source.sha256_file(zpath), "changelog": changelog}


def nexus_block(spec, spec_path):
    """The nexus block, checked; ids may be missing (a dry run says so)."""
    n = spec.get("nexus", {})
    if not isinstance(n, dict):
        raise usage(f"{spec_path}: nexus must be an object")
    unknown = sorted(set(n) - NEXUS_KEYS)
    if unknown:
        raise usage(f"{spec_path}: nexus has {', '.join(unknown)}, which is not one of {', '.join(sorted(NEXUS_KEYS))}")
    for k in ("mod_id", "file_id"):
        v = n.get(k)
        if v is not None and not re.fullmatch(r"[A-Za-z0-9_-]{1,64}", str(v) if isinstance(v, (str, int)) and not isinstance(v, bool) else "!"):
            raise usage(f"{spec_path}: nexus.{k} {v!r} is not an id (letters, digits, - and _)")
    cat = n.get("category", "main")
    if cat not in CATEGORIES:
        raise usage(f"{spec_path}: nexus.category {cat!r} is one of {', '.join(CATEGORIES)}")
    for k in SWITCHES + OPTIONAL_SWITCHES + ("changelog",):
        if k in n and not isinstance(n[k], bool):
            raise usage(f"{spec_path}: nexus.{k} must be true or false")
    for k in ("name", "description"):
        if k in n and (not isinstance(n[k], str) or not n[k].strip()):
            raise usage(f"{spec_path}: nexus.{k} must be text")
    if n.get("primary_mod_manager_download") and cat != "main":
        raise usage(f"{spec_path}: only a main file can be the primary mod-manager download (Nexus refuses it)")
    return {
        "mod_id": str(n["mod_id"]) if n.get("mod_id") is not None else None,
        "file_id": str(n["file_id"]) if n.get("file_id") is not None else None,
        "category": cat,
        "name": " ".join((n.get("name") or spec["name"]).split()),
        "description_full": n.get("description") or spec["description"],
        "switches": {k: n.get(k, False) for k in SWITCHES},
        "optional": {k: n[k] for k in OPTIONAL_SWITCHES if k in n},
        "send_changelog": n.get("changelog", True),
    }


# --- the release gate ------------------------------------------------------------------------------------------------

def run_magician(cmd):
    """The one place the publisher runs magician (tests stand in for it here)."""
    return subprocess.run(cmd, capture_output=True, text=True, encoding="utf-8", errors="replace")


def gate(b, toml, exe):
    """The release gate on the zip's own paks: `magician grade <paks> --mod <toml> --json`, its release_gate read.
    Returns (passes, lines): passes is None when the gate could not judge."""
    if not os.path.isfile(toml):
        return None, [f"no mod.toml at {toml}: the gate reads the mod's proofs and rights from it (--mod names it)"]
    tmp = tempfile.mkdtemp(prefix="magician-publish-")
    try:
        paks = []
        for i, member in enumerate(b["paks"]):
            name = member.rsplit("/", 1)[-1]
            if not source.plain_name(name):
                return None, [f"{member} is not a plain pak name"]
            d = os.path.join(tmp, str(i))
            os.makedirs(d)
            with b["zip_obj"].open(member) as src, open(os.path.join(d, name), "wb") as dst:
                shutil.copyfileobj(src, dst, 1 << 20)
            paks.append(os.path.join(d, name))
        try:
            r = run_magician([exe, "grade"] + paks + ["--mod", toml, "--json"])
        except OSError as e:
            return None, [f"magician did not start ({exe}): {e}"]
        try:
            doc = json.loads(r.stdout)
            rg = doc["release_gate"]
        except (ValueError, KeyError, TypeError):
            why = (r.stderr.strip() or r.stdout.strip() or f"exit {r.returncode}").splitlines()
            return None, [f"magician grade could not judge the zip's paks (exit {r.returncode}): {why[-1] if why else ''}"]
        lines = [f"{doc.get('label', '')} on build {doc.get('freshness', {}).get('target', '?')}"]
        lines += [f"blocker: {x}" for x in rg.get("blockers", [])]
        lines += [f"warning: {x}" for x in rg.get("warnings", [])]
        return bool(rg.get("pass")) and not rg.get("blockers"), lines
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


# --- HTTP ------------------------------------------------------------------------------------------------------------

def check_url(url, what):
    """https anywhere; plain http only to this machine (a test server)."""
    p = urllib.parse.urlsplit(url)
    loopback = p.hostname in ("localhost", "127.0.0.1", "::1")
    if p.scheme == "https" and p.hostname:
        return url
    if p.scheme == "http" and loopback:
        return url
    raise Stop(2 if what == "NEXUSMODS_API_BASE" else 1, f"{what} {hide(url)!r} is not https (plain http is only for a server on this machine)")


class NoRedirect(urllib.request.HTTPRedirectHandler):
    """A redirect would carry the key to wherever it points: none is followed."""

    def redirect_request(self, req, fp, code, msg, headers, newurl):
        return None


OPENER = urllib.request.build_opener(NoRedirect)


class Failed(Exception):
    pass


def request(method, url, step, body=None, headers=None, timeout=API_TIMEOUT, ok=(200, 201, 204)):
    """One request; the answer's status, headers and body. A failure names the step and quotes the server."""
    req = urllib.request.Request(url, data=body, method=method, headers={"User-Agent": USER_AGENT, **(headers or {})})
    try:
        with OPENER.open(req, timeout=timeout) as r:
            status, hdrs, data = r.status, r.headers, r.read()
    except urllib.error.HTTPError as e:
        status, hdrs, data = e.code, e.headers, e.read() if e.fp else b""
    except (urllib.error.URLError, OSError) as e:
        reason = getattr(e, "reason", e)
        raise Failed(f"{step}: no answer from {urllib.parse.urlsplit(url).netloc} ({reason})")
    if status not in ok:
        text = data.decode("utf-8", "replace").strip()
        try:
            doc = json.loads(text)
            if isinstance(doc, dict) and (doc.get("title") or doc.get("detail")):
                text = " - ".join(str(doc[k]) for k in ("title", "detail") if doc.get(k))
                if isinstance(doc.get("errors"), list):
                    text += "; " + "; ".join(f"{e.get('pointer', '')} {e.get('detail', '')}".strip() for e in doc["errors"] if isinstance(e, dict))
        except ValueError:
            pass
        raise Failed(f"{step}: HTTP {status}" + (f": {text[:SERVER_TEXT]}" if text else ""))
    return status, hdrs, data


class Nexus:
    def __init__(self, base, key):
        self.base = check_url(base.rstrip("/"), "NEXUSMODS_API_BASE")
        self.key = key

    def api(self, method, path, step, doc=None, ok=(200, 201, 204)):
        body = json.dumps(doc).encode() if doc is not None else None
        headers = {"apikey": self.key, "Accept": "application/json"}
        if body is not None:
            headers["Content-Type"] = "application/json"
        _, _, data = request(method, self.base + path, step, body, headers, ok=ok)
        if not data.strip():
            return {}
        try:
            return json.loads(data)
        except ValueError:
            raise Failed(f"{step}: the answer is not JSON: {data[:200].decode('utf-8', 'replace')}")

    def versions(self, file_id):
        doc = self.api("GET", f"/mod-files/{file_id}/versions", f"read file {file_id}'s versions")
        return (doc.get("data") or {}).get("versions") or []

    def has_changelog(self, mod_id, version):
        for page in range(1, 51):
            doc = self.api("GET", f"/mods/{mod_id}/changelogs?page={page}&page_size=50", f"read mod {mod_id}'s changelogs")
            items = doc.get("data") or []
            if any(isinstance(c, dict) and c.get("version") == version and c.get("entries") for c in items):
                return True
            meta = doc.get("meta") or {}
            if not items or page * int(meta.get("page_size") or 50) >= int(meta.get("total_count") or 0):
                return False
        return False


def put_part(url, data, number, total):
    step = f"step 2 of 7, upload part {number} of {total}"
    pause = RETRY_PAUSE
    for attempt in range(1, PART_TRIES + 1):
        try:
            _, hdrs, _ = request("PUT", url, step, data, {"Content-Type": "application/octet-stream", "Content-Length": str(len(data))},
                                 timeout=PART_TIMEOUT, ok=(200, 201, 204))
            etag = (hdrs.get("ETag") or "").replace('"', "").strip()
            if not etag:
                raise Failed(f"{step}: the storage server sent no ETag")
            return etag
        except Failed as e:
            if attempt == PART_TRIES:
                raise Failed(f"{e} (tried {PART_TRIES} times)")
            say(f"  part {number}: {e}; trying again in {pause:g} s")
            time.sleep(pause)
            pause *= 2


def complete_xml(etags):
    parts = "\n".join(f"  <Part>\n    <PartNumber>{i}</PartNumber>\n    <ETag>{e}</ETag>\n  </Part>" for i, e in enumerate(etags, 1))
    return f"<CompleteMultipartUpload>\n{parts}\n</CompleteMultipartUpload>"


# --- publishing ------------------------------------------------------------------------------------------------------

def plan_lines(b, n, base, key_set):
    spec = b["spec"]
    desc = cut(n["description_full"])
    lines = [
        f"  zip          {b['zip']}",
        f"  size         {b['size']} bytes ({mb(b['size'])})",
        f"  sha256       {b['sha256']}",
        f"  version      {spec['release']}",
        f"  name         {n['name']}",
        f"  description  {desc} ({utf16_len(desc)} of {DESCRIPTION_MAX} characters"
        + (", cut from " + str(utf16_len(' '.join(n['description_full'].split()))) + ")" if desc != " ".join(n["description_full"].split()) else ")"),
        f"  file id      {n['file_id'] or 'not set: nexus.file_id in bundle.json (the Files tab, Advanced, on the mod page)'}",
        f"  mod id       {n['mod_id'] or 'not set: nexus.mod_id in bundle.json (needed for the changelog)'}",
        f"  category     {n['category']}",
    ]
    for k in SWITCHES:
        lines.append(f"  {k:<30} {str(n['switches'][k]).lower()}")
    for k in OPTIONAL_SWITCHES:
        lines.append(f"  {k:<30} {str(n['optional'][k]).lower() if k in n['optional'] else 'not sent (Nexus decides)'}")
    if not n["send_changelog"]:
        lines.append("  changelog    none (nexus.changelog is false)")
    elif not b["changelog"]:
        lines.append(f"  changelog    none: the zip's CHANGELOG.md has no section for {spec['release']}")
    else:
        where = f"to mod {n['mod_id']}" if n["mod_id"] else "not sent: it goes to the mod page, and nexus.mod_id is not set"
        lines.append(f"  changelog    {spec['release']}, {where}:")
        lines += [f"      {l}" for l in b["changelog"].splitlines()]
    lines.append(f"  api          {base}")
    lines.append(f"  key          NEXUSMODS_API_KEY {'is set' if key_set else 'is not set (a real run needs it: ' + KEY_PAGE + ')'}")
    return lines


def publish(b, n, nexus):
    spec, version, file_id = b["spec"], b["spec"]["release"], n["file_id"]
    wants_changelog = n["send_changelog"] and b["changelog"] and n["mod_id"]

    def changelog():
        if not wants_changelog:
            return
        if nexus.has_changelog(n["mod_id"], version):
            say(f"changelog: mod {n['mod_id']} already has one for {version}; not added again (Nexus appends, never replaces)")
            return
        nexus.api("POST", f"/mods/{n['mod_id']}/changelogs", f"step 7 of 7, add the changelog to mod {n['mod_id']}",
                  {"version": version, "changelog": b["changelog"]})
        say(f"step 7 of 7: changelog added to mod {n['mod_id']} for {version}")

    # before anything is uploaded: is this version already on the file?
    same = [v for v in nexus.versions(file_id) if isinstance(v, dict) and str(v.get("version")) == version and v.get("category") not in ("removed",)]
    if same:
        v = same[0]
        if v.get("size_in_bytes") == b["size"]:
            say(f"already published: version {version} is on file {file_id} (version id {v.get('id')}, {v.get('size_in_bytes')} bytes, the same size); nothing uploaded")
            changelog()
            return v.get("id")
        raise Stop(3, f"version {version} is already on file {file_id} (version id {v.get('id')}) with {v.get('size_in_bytes')} bytes, and this zip has {b['size']}: "
                      f"new bytes need a new release (a new `release` in bundle.json, then magician bundle --zip)")

    filename = os.path.basename(b["zip"])
    created = nexus.api("POST", "/uploads/multipart", "step 1 of 7, create the multipart upload", {"size_bytes": b["size"], "filename": filename})
    d = created.get("data") or {}
    upload_id, urls, part, complete = d.get("id"), d.get("part_presigned_urls"), d.get("part_size_bytes"), d.get("complete_presigned_url")
    if not (isinstance(upload_id, str) and re.fullmatch(r"[A-Za-z0-9-]{1,64}", upload_id) and isinstance(urls, list) and urls
            and isinstance(part, int) and part > 0 and isinstance(complete, str)):
        raise Failed(f"step 1 of 7, create the multipart upload: the answer lacks an upload id, part URLs, a part size or a complete URL: {json.dumps(d)[:300]}")
    if len(urls) != math.ceil(b["size"] / part):
        raise Failed(f"step 1 of 7: {len(urls)} part URLs for {b['size']} bytes in parts of {part} (expected {math.ceil(b['size'] / part)})")
    for u in urls + [complete]:
        check_url(u, "a presigned URL from Nexus")
    say(f"step 1 of 7: upload {upload_id}, {len(urls)} part(s) of {part} bytes")

    etags = []
    with open(b["zip"], "rb") as f:
        for i, url in enumerate(urls, 1):
            f.seek((i - 1) * part)
            data = f.read(part)
            etags.append(put_part(url, data, i, len(urls)))
            say(f"step 2 of 7: part {i} of {len(urls)} ({len(data)} bytes)")

    _, _, data = request("POST", complete, "step 3 of 7, complete the multipart upload", complete_xml(etags).encode(), {"Content-Type": "application/xml"})
    if b"<Error>" in data:   # storage can answer 200 with an error inside
        raise Failed(f"step 3 of 7, complete the multipart upload: {data[:SERVER_TEXT].decode('utf-8', 'replace')}")
    say("step 3 of 7: multipart upload complete")

    nexus.api("POST", f"/uploads/{upload_id}/finalise", "step 4 of 7, finalise the upload")
    say("step 4 of 7: upload finalised")

    waited, pause = 0.0, POLL_FIRST
    while True:
        doc = nexus.api("GET", f"/uploads/{upload_id}", "step 5 of 7, wait for the upload to be available")
        state = (doc.get("data") or {}).get("state")
        if state == "available":
            break
        if waited >= POLL_LIMIT:
            raise Failed(f"step 5 of 7: upload {upload_id} is still {state!r} after {POLL_LIMIT} s; it can be made a version on the site, or run again")
        time.sleep(pause)
        waited += pause
        pause = min(pause * 1.5, 30.0)
    say("step 5 of 7: upload available")

    body = {"upload_id": upload_id, "name": n["name"], "version": version, "file_category": n["category"]}
    desc = cut(n["description_full"])
    if desc:
        body["description"] = desc
    body.update(n["switches"])
    body.update(n["optional"])
    doc = nexus.api("POST", f"/mod-files/{file_id}/versions", f"step 6 of 7, create version {version} of file {file_id}", body)
    vid = ((doc.get("data") or {}).get("version") or {}).get("id")
    say(f"step 6 of 7: version {version} created on file {file_id} (version id {vid})")
    changelog()
    if not wants_changelog:
        say("step 7 of 7: no changelog sent")
    return vid


def main(argv=None):
    args = list(sys.argv[1:] if argv is None else argv)
    for stream in (sys.stdout, sys.stderr):   # a name the console can't show is not a reason to stop halfway
        if hasattr(stream, "reconfigure"):
            stream.reconfigure(errors="replace")
    try:
        return run(args)
    except Stop as e:
        print(hide(e), file=sys.stderr, flush=True)
        return e.code
    except Failed as e:
        print(hide(e), file=sys.stderr, flush=True)
        return 1


def run(args):
    if not args or args[0] in ("--help", "-h") or "--help" in args or "-h" in args:
        print(__doc__)
        return 0
    dest, rest = args[0], args[1:]
    if dest != "nexus":
        raise usage(f"publish {dest}: the one destination is nexus (magician publish nexus <bundle.json>)")
    values = {"--zip": None, "--mod": None, "--magician": None}
    flags = {"--dry-run": False, "--ignore-gate": False}
    positional = []
    i = 0
    while i < len(rest):
        a = rest[i]
        if a in values:
            if i + 1 >= len(rest) or rest[i + 1].startswith("--"):
                raise usage(f"{a} needs a value")
            values[a] = rest[i + 1]
            i += 2
            continue
        if a in flags:
            flags[a] = True
        elif a.startswith("-"):
            raise usage(f"{a} is not an option of publish nexus (--zip FILE, --mod FILE, --magician EXE, --dry-run, --ignore-gate)")
        else:
            positional.append(a)
        i += 1
    if len(positional) != 1:
        raise usage("publish nexus <bundle.json> [--zip FILE] [--mod mod.toml] [--dry-run] [--ignore-gate] [--magician EXE]")

    key = os.environ.get("NEXUSMODS_API_KEY", "").strip()
    SECRET[:] = [key] if key else []
    base = (os.environ.get("NEXUSMODS_API_BASE", "").strip() or API_BASE).rstrip("/")
    check_url(base, "NEXUSMODS_API_BASE")
    dry = flags["--dry-run"]

    b = read_bundle(positional[0], values["--zip"])
    n = nexus_block(b["spec"], b["spec_path"])
    toml = os.path.abspath(values["--mod"]) if values["--mod"] else os.path.join(os.path.dirname(b["base"]), "mod.toml")
    exe = values["--magician"] or os.environ.get("MAGICIAN_EXE") or gamepaths.magician()

    say(("dry run, nothing is sent: " if dry else "") + f"{os.path.basename(b['zip'])} as version {b['spec']['release']} of Nexus Mods file {n['file_id'] or '(not set)'}")
    if dry:
        for line in plan_lines(b, n, base, bool(key)):
            say(line)

    passes, lines = gate(b, toml, exe)
    verdict = {True: "pass", False: "blocked", None: "could not judge"}[passes]
    say(f"release gate: {verdict} ({toml})")
    for line in lines:
        say(f"  {line}")
    if passes is not True:
        if flags["--ignore-gate"]:
            say("WARNING: --ignore-gate: publishing past the release gate")
        else:
            raise Stop(3, f"refusing to publish: the release gate {'has a blocker' if passes is False else 'could not judge the zip'} "
                          f"({lines[-1] if passes is None else '; '.join(l[len('blocker: '):] for l in lines if l.startswith('blocker: '))}); "
                          "fix it, or --ignore-gate when you know better")

    missing = [k for k in ("file_id",) if not n[k]]
    if missing:
        raise usage(f"bundle.json has no nexus.file_id: make the mod page and its first file on the site, then copy the file's id "
                    f"from the Files tab (Advanced) into \"nexus\": {{\"file_id\": ...}} in {b['spec_path']}")
    if dry:
        say("dry run: nothing sent")
        return 0
    if not key:
        raise usage(f"NEXUSMODS_API_KEY is not set: make a personal API key at {KEY_PAGE} and put it in the environment "
                    "(it is read from nowhere else, and never written down)")
    if any(c in key for c in "\r\n\x00"):
        raise usage("NEXUSMODS_API_KEY holds a line break: it is not a key")
    vid = publish(b, n, Nexus(base, key))
    say(f"published: {os.path.basename(b['zip'])} is version {b['spec']['release']} of file {n['file_id']} (version id {vid})")
    return 0


if __name__ == "__main__":
    sys.exit(main())

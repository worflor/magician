"""Write a static Deadlock Mod Manager mod source from release bundles. Python stdlib plus Pillow (images only).

    python source.py <source.json> [--out DIR] [--base-url URL] [--taxonomy FILE] [--prune]

A mod source is a set of JSON documents on public HTTPS that Deadlock Mod Manager (DMM) reads into its Mods Store:
players add it once and get one-click installs and updates (the native source protocol,
https://docs.deadlockmods.app/developer-docs/build-a-mod-registry). This writes a complete static one, ready to put
on any static host (GitHub Pages is the worked case):

    dmm-source.json         the descriptor: the URL players add (also .well-known/dmm-source.json when the source
                            sits at the root of its origin)
    catalog.json            one page, every listing, a snapshotId that changes whenever anything listed changes
    mods/<id>.json          the full listing: metadata, images and the current release with its artifact and plan
    mods/<id>/releases.json release history, newest first (kept across runs: earlier releases are read back)
    media/<id>/*.jpg        the bundle's previews, as JPEG under the protocol's 2 MiB per image
    files/<zip>             the release zip, when downloads live on the same host (see downloads_url)
    index.html              the front page: a card per mod, what is coming, and the Add source button
    <id>/index.html         a page per mod: its pictures, Install with DMM, the description DMM shows, every release
    feed.xml                an Atom feed of every release, for a reader that is not a mod manager
    sitemap.xml             the pages, each with the date of its newest release, for search engines
    .nojekyll               so GitHub Pages publishes .well-known and does not run Jekyll
    .dmm-source-files       what this tool wrote, so the next run replaces only its own files

Source spec (JSON): source_id (permanent; never reuse it), name, base_url (where the tree is served, https, e.g.
https://example.github.io/skins/; --base-url overrides it, so it is the one value to change), out (or --out),
mods [{bundle (a bundle.json), zip (the zip bundle.py --zip wrote) or bundle_dir (the bundle folder; the zip is found
beside it as bundle.py names it), id (the listing id, default the bundle's slug), sections (README sections copied
into the description, default ["Not seen yet"]), preselect (optional mods ticked by default, default none),
homepage (the mod's page, default the landing page's entry), mod_toml (default the mod.toml beside the bundle's
folder: the builds its proof ledger, proofs.tsv beside it, proves the zip's own paks on are said in the listing, so a
re-prove after a game update shows without a new release; [proven] builds, which names no bytes, is not read)}].
Optional: homepage (default base_url), downloads_url (default base_url + "files/"; may hold {id} and {version}, e.g.
https://github.com/<user>/<repo>/releases/download/{id}-{version}/), media_url (default base_url + "media/"),
download_origins (more origins artifacts may come from or redirect to), icon (an image; a 256 px square JPEG),
taxonomy (DMM's taxonomy.json, to check hero and content-type ids), dmm_url (the Get DMM link).
Per mod, for the pages: tagline, blurb (the card's text, default the bundle's description; both take inline Markdown),
badge (a short label shown as a pill beside the version on the card and the page, e.g. "early"; DMM has no place for
one, so a listing that needs to say it says it in its description too) and links [{label, url, kind}] (a trailer, say): kind "same" says the link is this mod on another site (its Nexus
Mods page), "video" that it is a video of it, and the pages' structured data says so (sameAs, subjectOf).
authors {name: url}: an author's own site, for an author as bundle.json names them; the mod's page links it.
page {title, intro, css, head, header, footer}: the pages' own look. css is a stylesheet appended to the built-in one
(which is written on variables: --bg, --ink, --muted, --line, --accent, --font, --width ...), head, header and footer
are HTML files put in <head>, above the content and in the footer of every page; {root} in them is the way back to
the top of the tree, so the same chrome works on the front page and a mod's page.
upcoming [{title, hero, when, note, image}]: cards under "Coming" on the front page. With in_manager (and id, author,
hero_id, nsfw) one is also a listing DMM shows and cannot install (availability "unavailable": DMM has no "coming
soon" state, so the description says it). Release the mod later under the same id and that listing turns installable.
Relative paths resolve against the spec's folder; `~/` means the folder that holds this repository.

From each bundle: one listing. Title, author, hero (hero_id), contentType (content_type), tags, isNsfw (nsfw, which
must be given), the release (release, date) and its changelog (the CHANGELOG.md section for that release, as plain
text) come from bundle.json and the zip. The zip is the one artifact, with its size and sha256; the plan is
"components": each mod of the bundle is a component naming its VPK inside the zip, in load order, the required ones
always installed, each optional one its own choice (minSelections 0). The contentRevision is a hash of the artifact's
bytes and the plan's ids and paths, so it changes exactly when what gets installed changes; the release id is the
version plus the start of that hash, so new bytes are always a new release. A source cannot run scripts, so a
bundle's hero-page scene is not installed by DMM: the description says so and links the zip, which carries the
installers.

URLs inside the tree are relative, so the tree works wherever base_url points; downloads or media elsewhere get
absolute URLs, and every origin used is declared in networkOrigins. GitHub release downloads redirect to
release-assets.githubusercontent.com (DMM follows artifact redirects only to declared origins), so that origin is
added and said. Only files this tool wrote are replaced or removed; a file already in `out` that it did not write is
never overwritten. Old release zips under files/ stay (in-progress downloads) until --prune.
Same inputs and same earlier tree, same bytes.
"""
import hashlib, html, io, json, os, posixpath, re, shutil, sys, urllib.parse, zipfile

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
MANIFEST = ".dmm-source-files"
MAX_MEDIA = 2 * 1024 * 1024          # DMM's limit per image response
MEDIA_TARGET = 1024 * 1024           # what we aim for: four images per listing are fetched on every refresh
MAX_ICON = 128 * 1024
PAGES_FILE_LIMIT = 100 * 1024 * 1024  # GitHub refuses a pushed file over 100 MiB; it warns over 50 MiB
CONTENT_TYPES = ("skins", "gameplay", "hud", "models", "music", "maps", "ability-sounds", "weapon-sounds",
                 "voice-lines", "kill-sounds", "killstreak-music", "other")
GITHUB_RELEASE_REDIRECT = "https://release-assets.githubusercontent.com"
DMM_URL = "https://deadlockmods.app/download"


def die(msg):
    sys.exit(msg)


def resolve(base, p):
    if p.startswith("~/"):
        return os.path.join(ROOT, p[2:])
    return p if os.path.isabs(p) else os.path.join(base, p)


def sha256_file(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for b in iter(lambda: f.read(1 << 20), b""):
            h.update(b)
    return h.hexdigest()


def plain_text(md):
    """A Markdown changelog section as the plain text DMM shows: no #, **, backticks or link marks."""
    out = []
    for line in md.strip().splitlines():
        line = re.sub(r"^#{1,6}\s+", "", line)
        line = re.sub(r"\*\*(.+?)\*\*", r"\1", line).replace("`", "")
        out.append(re.sub(r"\[([^\]]+)\]\(([^)]+)\)", r"\1 (\2)", line))
    return "\n".join(out)


def sections(md):
    """README.md as (intro, {heading: body}): the intro is everything after the title before the first ## heading."""
    parts = re.split(r"^## +(.+?)\s*$", md, flags=re.M)
    intro = re.sub(r"^# .*\n", "", parts[0], count=1).strip()
    return intro, {parts[i]: parts[i + 1].strip() for i in range(1, len(parts) - 1, 2)}


def origin(url):
    p = urllib.parse.urlsplit(url)
    port = f":{p.port}" if p.port and p.port != {"https": 443, "http": 80}.get(p.scheme) else ""
    return f"{p.scheme}://{p.hostname}{port}"


def check_url(url, what):
    p = urllib.parse.urlsplit(url)
    local = p.hostname in ("localhost", "127.0.0.1")
    if p.scheme != "https" and not (p.scheme == "http" and local):
        die(f"{what} {url!r} must be https (DMM's release builds take nothing else)")
    if not p.hostname or p.username or p.password or p.query or p.fragment:
        die(f"{what} {url!r} needs a host and no credentials, query or fragment")
    return local


def folder_url(url, what):
    check_url(url, what)
    return url if url.endswith("/") else url + "/"


def safe_id(v):
    return isinstance(v, str) and re.fullmatch(r"[a-z0-9][a-z0-9-]{0,63}", v) is not None


def plain_name(v):
    return (isinstance(v, str) and v not in ("", ".", "..") and not re.search(r'[/\\:<>"|?*\x00-\x1f]', v)
            and not v.endswith((".", " ")))


def rel_url(from_doc, target):
    """`target` (a path in the tree) as a URL relative to the document at `from_doc` (also a path in the tree)."""
    rel = posixpath.relpath(target, posixpath.dirname(from_doc) or ".")
    return urllib.parse.quote(rel if rel.startswith("../") else "./" + rel)


def unwrap(md):
    """Hard-wrapped Markdown as one line per paragraph or list item, so no renderer turns a wrap into a break."""
    out, fence = [], False
    for line in md.splitlines():
        s = line.strip()
        if s.startswith("```"):
            fence = not fence
            out.append(line)
        elif fence or not s or not out or not out[-1].strip() or re.match(r"([-*+] |\d+[.)] |#|\||>)", s) \
                or out[-1].lstrip().startswith(("#", "|", "```")):
            out.append(line.rstrip())
        else:
            out[-1] = out[-1].rstrip() + " " + s
    return "\n".join(out)


class Tree:
    """Every file the run writes, held until all of them are known, then checked and written in publishing order."""

    def __init__(self, out):
        self.out = os.path.abspath(out)
        self.files = {}   # rel -> bytes, or ("copy", src path, sha256)

    def path(self, rel):
        if not rel or rel.startswith("/") or "\\" in rel or any(s in ("", ".", "..") for s in rel.split("/")):
            die(f"refusing to write {rel!r}: not a plain path inside the out folder")
        p = os.path.normpath(os.path.join(self.out, *rel.split("/")))
        real_out = os.path.realpath(self.out)
        real = os.path.realpath(os.path.dirname(p))
        if os.path.commonpath([os.path.normcase(real_out), os.path.normcase(real)]) != os.path.normcase(real_out):
            die(f"refusing to write {rel!r}: it would land outside {self.out}")
        return p

    def put(self, rel, data):
        self.path(rel)
        if rel in self.files:
            die(f"two outputs want {rel}")
        self.files[rel] = data.encode("utf-8") if isinstance(data, str) else data

    def json(self, rel, doc):
        self.put(rel, json.dumps(doc, indent=2, ensure_ascii=False) + "\n")

    def copy(self, rel, src, digest):
        self.path(rel)
        self.files[rel] = ("copy", src, digest)


def read_manifest(out):
    p = os.path.join(out, MANIFEST)
    if not os.path.isfile(p):
        return set()
    return {l.strip() for l in open(p, encoding="utf-8") if l.strip()}


def load_json(path, what):
    try:
        return json.load(open(path, encoding="utf-8-sig"))
    except (OSError, ValueError) as e:
        die(f"{what} {path}: {e}")


def read_bundle(entry, base, taxonomy):
    """One bundle.json and its zip: everything a listing needs, checked against each other."""
    if not isinstance(entry, dict) or not isinstance(entry.get("bundle"), str):
        die(f"each mod needs bundle (a bundle.json): {entry!r}")
    spec_path = os.path.abspath(resolve(base, entry["bundle"]))
    spec = load_json(spec_path, "bundle")
    sbase = os.path.dirname(spec_path)
    for k in ("name", "slug", "author", "hero", "description", "release", "date", "hero_id", "content_type", "mods"):
        if spec.get(k) in (None, "", []):
            die(f"{spec_path}: a listing needs {k}")
    if not isinstance(spec.get("nsfw"), bool):
        die(f"{spec_path}: nsfw must be true or false (DMM hides an unknown classification with adult content)")
    if not re.fullmatch(r"\d{4}-\d{2}-\d{2}", str(spec["date"])):
        die(f"{spec_path}: date {spec['date']!r} is not YYYY-MM-DD")
    if spec["content_type"] not in (taxonomy["contentTypes"] if taxonomy else CONTENT_TYPES):
        die(f"{spec_path}: content_type {spec['content_type']!r} is not a DMM content type")
    if taxonomy and spec["hero_id"] not in taxonomy["heroes"]:
        die(f"{spec_path}: hero_id {spec['hero_id']!r} is not in the taxonomy")
    if not safe_id(spec["hero_id"]):
        die(f"{spec_path}: hero_id {spec['hero_id']!r} is not a taxonomy id (lowercase, digits, hyphens)")
    tags = spec.get("tags", [])
    if not isinstance(tags, list) or len(tags) > 100 or not all(isinstance(t, str) and t for t in tags):
        die(f"{spec_path}: tags must be at most 100 strings")
    lid = entry.get("id", spec["slug"])
    if not safe_id(lid):
        die(f"listing id {lid!r} must be lowercase letters, digits and hyphens (it names files in the tree)")
    if lid in RESERVED_DIRS:
        die(f"listing id {lid!r} is a folder the tree already uses; give the mod another id")
    badge = entry.get("badge")
    if badge is not None and (not isinstance(badge, str) or not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9 .'-]{0,23}", badge)):
        die(f"{lid}: badge must be a short plain label (up to 24 letters, digits, spaces, . ' -), e.g. \"early\"")
    links = entry.get("links", [])
    if not isinstance(links, list):
        die(f"{lid}: links must be a list of {{label, url}}")
    for x in links:
        if not isinstance(x, dict) or not all(isinstance(x.get(k), str) and x[k] for k in ("label", "url")):
            die(f"{lid}: each link needs a label and a url: {x!r}")
        if "kind" in x:
            if x["kind"] not in LINK_KINDS:
                die(f"{lid}: link {x['label']!r} has kind {x['kind']!r}; a kind is one of {', '.join(LINK_KINDS)}")
            if urllib.parse.urlsplit(x["url"]).scheme != "https":
                die(f"{lid}: link {x['label']!r} is the mod elsewhere or a video, so it needs an https address")

    if entry.get("zip"):
        zpath = resolve(base, entry["zip"])
    elif entry.get("bundle_dir"):
        bdir = resolve(base, entry["bundle_dir"])
        zpath = os.path.join(os.path.dirname(os.path.abspath(bdir)), (spec.get("zip") or f"{spec['slug']}-{spec['release']}") + ".zip")
    else:
        die(f"{lid}: give zip (the release zip) or bundle_dir (the bundle folder the zip was written beside)")
    zpath = os.path.abspath(zpath)
    if not os.path.isfile(zpath):
        die(f"{lid}: no zip at {zpath} (python tools/bundle.py {spec_path} --zip)")
    zname = os.path.basename(zpath)
    if not plain_name(zname) or not zname.endswith(".zip"):
        die(f"{lid}: {zname!r} must be a plain file name ending in lowercase .zip")

    z = zipfile.ZipFile(zpath)
    names = set(z.namelist())
    tops = {n.split("/", 1)[0] for n in names}
    if len(tops) != 1:
        die(f"{zpath}: not a bundle zip (one top-level folder expected, found {len(tops)})")
    top = tops.pop()

    def member(rel, what):
        if f"{top}/{rel}" not in names:
            die(f"{zpath}: no {top}/{rel} ({what})")
        return z.read(f"{top}/{rel}")

    inter = json.loads(member("mod-interchange.json", "a bundle.py zip carries its mod-interchange.json"))
    built = (inter.get("mods") or [{}])[0].get("extensions", {}).get("magician", {}).get("release")
    if built != spec["release"]:
        die(f"{zpath} is release {built!r}, {spec_path} says {spec['release']!r}: rebuild the zip with bundle.py --zip")
    imods = {(m["order"], m["name"]): m for m in inter["mods"]}
    mods = sorted(spec["mods"], key=lambda m: m["order"])
    if len(imods) != len(mods):
        die(f"{zpath} holds {len(imods)} mod(s), {spec_path} has {len(mods)}: rebuild the zip")
    components, required, groups = [], [], []
    pak_shas = []
    preselect = entry.get("preselect", [])
    for m in mods:
        im = imods.get((m["order"], m["name"]))
        if im is None:
            die(f"{zpath} has no mod {m['name']!r} at order {m['order']}: it was built from another spec")
        if not safe_id(m["slug"]):
            die(f"{spec_path}: mod slug {m['slug']!r} must be lowercase letters, digits and hyphens (a component id)")
        files = []
        for f in im["files"]:
            data = f"{top}/{f['path']}"
            if data not in names:
                die(f"{zpath}: no {data}, which its mod-interchange.json lists")
            h = hashlib.sha256()
            with z.open(data) as src:
                for b in iter(lambda: src.read(1 << 20), b""):
                    h.update(b)
            if h.hexdigest() != f["sha256"] or z.getinfo(data).file_size != f["size"]:
                die(f"{zpath}: {data} does not match the sha256 and size in its mod-interchange.json")
            if data.lower().endswith(".vpk"):
                pak_shas.append(f["sha256"])
            files.append({"artifactId": "release-zip", "path": data})
        components.append({"id": m["slug"], "label": m["name"], "files": files})
        if m.get("optional"):
            groups.append({"id": f"optional-{m['slug']}", "label": m["name"], "minSelections": 0, "maxSelections": 1,
                           "componentIds": [m["slug"]], "defaultComponentIds": [m["slug"]] if m["slug"] in preselect else []})
        else:
            required.append(m["slug"])
    if not required:
        die(f"{spec_path}: every mod is optional; a listing needs one that is always installed")
    unknown = set(preselect) - {g["componentIds"][0] for g in groups}
    if unknown:
        die(f"{lid}: preselect names {sorted(unknown)}, which are not optional mods of the bundle")

    readme = member("README.md", "the player readme").decode("utf-8") if f"{top}/README.md" in names else ""
    changelog = member("CHANGELOG.md", "the changelog").decode("utf-8") if f"{top}/CHANGELOG.md" in names else ""
    previews = []
    for p in spec.get("previews", []):
        previews.append((p["out"], member(f"previews/{p['out']}", "a preview bundle.json lists")))
    return {"id": lid, "spec": spec, "spec_path": spec_path, "zip": zpath, "zip_name": zname, "top": top,
            "size": os.path.getsize(zpath), "sha256": sha256_file(zpath), "readme": readme, "changelog": changelog,
            "previews": previews, "components": components, "required": required, "groups": groups,
            "scenes": [sc["hero"] for sc in spec.get("scenes", [])], "entry": entry,
            "proven": proven_builds(entry, base, sbase, pak_shas),
            "page": open(resolve(base, entry["page"]), encoding="utf-8").read() if entry.get("page") else None}


PROOF_COLUMNS = 9


def proof_id(build, pairs):
    """Magician's proof id (crates/magician/src/proofs.rs): the build and the proof's sorted (owner, sha256) pairs."""
    h = hashlib.sha256(build.encode())
    for owner, sha in sorted(set(pairs)):
        h.update(f"\n{owner}\t{sha}".encode())
    return h.hexdigest()[:16]


def proven_builds(entry, base, bundle_dir, shas):
    """The game builds the bytes being published were proven on in game. A proof belongs to bytes: the mod's proof
    ledger (`proofs.tsv` beside its mod.toml, the entry's mod_toml, else the mod.toml beside the bundle's folder,
    written by `magician prove`) holds, per proof, the build and the sha256 of every file seen in game. A build counts
    when one complete proof of it holds every pak in the zip (`shas`). `[proven] builds` in the mod.toml is a summary
    for any bytes and is not read: a rebuilt pak's release says nothing about proofs until it is proven itself. A
    listing says them, so after a game update and a re-prove the page and DMM show it without a new release."""
    path = resolve(base, entry["mod_toml"]) if entry.get("mod_toml") else os.path.join(os.path.dirname(bundle_dir), "mod.toml")
    if not os.path.isfile(path):
        if entry.get("mod_toml"):
            die(f"mod_toml {path} does not exist")
        return []
    ledger = os.path.join(os.path.dirname(path), "proofs.tsv")
    if not shas or not os.path.isfile(ledger):
        return []
    proofs = {}
    for line in open(ledger, encoding="utf-8"):
        c = line.rstrip("\r\n").split("\t")
        if line.startswith("#") or len(c) != PROOF_COLUMNS or not c[0].isdigit():
            continue
        p = proofs.setdefault((c[0], c[2]), set())
        p.add((c[3], c[6]))
    builds = set()
    for (build, pid), pairs in proofs.items():
        # a proof that lost a row in a merge no longer adds up to its id, and counts for nothing
        if proof_id(build, pairs) == pid and set(shas) <= {s for _, s in pairs}:
            builds.add(int(build))
    return sorted(builds)


def join_and(items):
    items = [str(i) for i in items]
    return items[0] if len(items) == 1 else ", ".join(items[:-1]) + " and " + items[-1]


def jpeg(data, max_side, limit, square=False):
    """An image as JPEG within `limit` bytes, the long side at most `max_side`. Same input, same bytes."""
    from PIL import Image
    im = Image.open(io.BytesIO(data)).convert("RGB")
    if square:
        s = min(im.size)
        l, t = (im.size[0] - s) // 2, (im.size[1] - s) // 2
        im = im.crop((l, t, l + s, t + s))
    side = max_side
    while True:
        cur = im.copy()
        cur.thumbnail((side, side), Image.LANCZOS)
        for q in (88, 82, 76, 70):
            buf = io.BytesIO()
            cur.save(buf, "JPEG", quality=q, optimize=True)
            if buf.tell() <= limit:
                return buf.getvalue(), cur.size
        side = int(side * 0.8)
        if side < 64:
            die("an image does not fit its size limit even at 64 px")


def site_name(url):
    host = urllib.parse.urlsplit(url).hostname or url
    return {"gamebanana.com": "GameBanana", "www.nexusmods.com": "Nexus Mods", "nexusmods.com": "Nexus Mods"}.get(host, host)


def changelog_section(md, release):
    m = re.search(rf"^##\s+v?{re.escape(release)}\b.*?(?=^##\s|\Z)", md, flags=re.M | re.S)
    return plain_text(unwrap(m.group(0))) if m else None


def page_markdown(html, drop=()):
    """A mod page written as simple HTML (headings, paragraphs, em/strong/s, blockquotes, lists, rules) as Markdown.
    Comments and images go (DMM shows a listing's images itself); a block headed by one of `drop` (a bold-only
    paragraph or a heading, e.g. "Install") is left out up to the next such head or rule."""
    html = re.sub(r"<!--.*?-->", "", html, flags=re.S)
    html = re.sub(r"<p>\s*<img\b[^>]*>\s*</p>|<img\b[^>]*>", "", html, flags=re.I)

    def inline(t):
        t = re.sub(r"<br\s*/?>\s*", "  \n", t, flags=re.I)
        t = re.sub(r"</?(strong|b)>", "**", t, flags=re.I)
        t = re.sub(r"</?(em|i)>", "*", t, flags=re.I)
        t = re.sub(r"</?(s|del|strike)>", "~~", t, flags=re.I)
        t = re.sub(r"</?code>", "`", t, flags=re.I)
        t = re.sub(r'<a\b[^>]*href="([^"]*)"[^>]*>(.*?)</a>', r"[\2](\1)", t, flags=re.I | re.S)
        t = re.sub(r"<[^>]+>", "", t)
        for k, v in (("&amp;", "&"), ("&lt;", "<"), ("&gt;", ">"), ("&quot;", '"'), ("&nbsp;", " ")):
            t = t.replace(k, v)
        lines = [" ".join(l.split()) + ("  " if l.endswith("  ") else "") for l in t.strip().split("\n")]
        return "\n".join(lines).strip()

    blocks = []
    for m in re.finditer(r"<(h[1-6]|p|blockquote|ul|ol)\b[^>]*>(.*?)</\1>|<hr\b[^>]*>", html, flags=re.I | re.S):
        tag, body = (m.group(1) or "hr").lower(), m.group(2) or ""
        if tag == "hr":
            blocks.append(("rule", "---"))
        elif tag[0] == "h":
            blocks.append(("head", "#" * min(int(tag[1]) + 1, 6) + " " + inline(body)))
        elif tag == "p":
            t = inline(body)
            if t:
                blocks.append(("head" if re.fullmatch(r"\*\*[^*]+\*\*", t) else "para", t))
        elif tag == "blockquote":
            t = "\n".join(inline(x) for x in re.findall(r"<p\b[^>]*>(.*?)</p>", body, flags=re.I | re.S)) or inline(body)
            blocks.append(("para", "\n".join("> " + l for l in t.split("\n"))))
        else:
            items = [inline(x).replace("\n", " ") for x in re.findall(r"<li\b[^>]*>(.*?)</li>", body, flags=re.I | re.S)]
            blocks.append(("para", "\n".join((f"{i + 1}. " if tag == "ol" else "- ") + x for i, x in enumerate(items))))
    dropped = {d.lower() for d in drop}
    out, skipping = [], False
    for kind, text in blocks:
        if kind == "head":
            skipping = text.strip("#* ").rstrip(".").lower() in dropped
        elif skipping and (kind == "rule" or re.match(r"\*\*[^*]+\*\*", text)):
            # a rule ends the dropped block, and so does a paragraph that opens with its own bold label
            skipping = False
            if kind == "rule":
                continue
        if not skipping:
            out.append(text)
    while out and out[-1] == "---":
        out.pop()
    return "\n\n".join(out)


def description(b, download_url, zip_note):
    spec = b["spec"]
    intro, secs = sections(b["readme"]) if b["readme"] else (spec["description"], {})
    # The folder's own pointers (previews/, files/) mean nothing in DMM: it shows the images itself.
    paras = [p for p in re.split(r"\n\s*\n", intro) if "`previews/`" not in p]
    lead = "\n\n".join(paras) or spec["description"]
    if b.get("page"):
        # The mod's own page copy is the description; the readme's intro is written for the folder.
        lead = re.sub(r"\A#+ [^\n]*\n+", "", page_markdown(b["page"], b["entry"].get("page_drop", ["Install"])))
    out = [lead, "", "## What you install", ""]
    optional = {g["componentIds"][0]: g for g in b["groups"]}
    by_slug = {m["slug"]: m for m in spec["mods"]}
    for c in b["components"]:
        d = by_slug[c["id"]]["description"]
        if c["id"] in optional:
            d = re.sub(r"^optional\.\s*", "", d, flags=re.I)
            when = "optional, ticked by default" if optional[c["id"]]["defaultComponentIds"] else "optional, choose it when you install"
        else:
            when = "always installed"
        out.append(f"- **{c['label']}** ({when}). {d}")
    if b["scenes"]:
        out += ["", "## The hero-page scene", "",
                f"The pose and lighting {spec['hero']} gets in the hero roster is a scene patch, and a mod source can't "
                "run the script that installs it, so DMM installs the skin without it. The skin works on its own. "
                f"For the scene, download the full zip ([{b['zip_name']}]({download_url}), {zip_note}), close Deadlock "
                "and run `install-scene.cmd` from it (on Linux or Steam Deck, `install-scene.sh`). It builds the scene "
                "from your own game files. Turning the mod off in DMM leaves the scene in place; `uninstall-scene.cmd` "
                "removes it."]
    for name in b["entry"].get("sections", ["Not seen yet"]):
        if name in secs:
            out += ["", f"## {name}", "", secs[name]]
    tail = f"Version {spec['release']}" + (f", built for Deadlock build {spec['game_build']}" if spec.get("game_build") else "") + "."
    if b.get("proven"):
        tail += f" Proven in game on build{'s' if len(b['proven']) > 1 else ''} {join_and(b['proven'])}."
    if spec.get("link") and urllib.parse.urlsplit(spec["link"]).hostname != urllib.parse.urlsplit(download_url).hostname:
        tail += f" Also on [{site_name(spec['link'])}]({spec['link']})."
    out += ["", tail]
    return unwrap("\n".join(out)) + "\n"


def revision(artifacts, installation):
    """What gets installed, and nothing else: the artifacts' bytes and the plan's ids and paths. Labels, defaults
    and URLs are not part of it (DMM: a description edit or a moved URL is not a new version)."""
    plan = {"components": [{"id": c["id"], "files": c["files"]} for c in installation["components"]],
            "required": installation["requiredComponents"],
            "groups": [{k: g[k] for k in ("id", "minSelections", "maxSelections", "componentIds")} for g in installation["choiceGroups"]]}
    evidence = [dict({k: a[k] for k in ("id", "filename", "format", "size")}, sha256=a["hashes"]["sha256"]) for a in artifacts]
    blob = json.dumps({"artifacts": evidence, "installation": plan}, sort_keys=True, separators=(",", ":"))
    return hashlib.sha256(blob.encode()).hexdigest()[:32]


def check_tree(tree, base_url, descriptor):
    """The rules DMM applies to what we wrote, read back: every URL resolves to a declared origin for its role,
    artifact names, plans, sizes of media and text. The offline checker in DMM's repository takes only relative
    URLs, so this is the check for a tree whose downloads live elsewhere."""
    o = descriptor["networkOrigins"]

    def url(doc_rel, u, role):
        full = urllib.parse.urljoin(base_url + doc_rel, u)
        if origin(full) not in o[role]:
            die(f"{doc_rel} points at {full}, outside the declared {role} origins (an earlier release's download "
                "on a host the source no longer declares? remove it from that file's history)")
        return full

    def local(full):
        if full.startswith(base_url):
            return urllib.parse.unquote(full[len(base_url):])
        return None

    def doc(rel):
        data = tree.files[rel]
        if len(data) > 10 * 1024 * 1024:
            die(f"{rel} is over DMM's 10 MiB JSON limit")
        return json.loads(data)

    for rel in [r for r in tree.files if r.endswith("dmm-source.json")]:
        d = doc(rel)
        if d.get("iconUrl"):
            ic = local(url(rel, d["iconUrl"], "media"))
            if ic in tree.files and len(tree.files[ic]) > MAX_ICON:
                die(f"{ic} is over 128 KiB")
        cat = local(url(rel, d["catalog"]["indexUrl"], "api"))
        if cat not in tree.files:
            die(f"internal: {rel}'s catalog {cat} is not in the tree")
    page = doc("catalog.json")
    for item in page["items"]:
        rel = local(url("catalog.json", item["detailUrl"], "api"))
        listing = doc(rel)
        if listing["id"] != item["id"]:
            die(f"internal: {rel} has id {listing['id']}, the catalog says {item['id']}")
        for im in listing["images"]:
            r = local(url(rel, im, "media"))
            if r in tree.files and len(tree.files[r]) > MAX_MEDIA:
                die(f"{r} is over DMM's 2 MiB image limit")
        if len(listing["description"].encode()) > 512 * 1024 or len(listing["tags"]) > 100 or len(listing["images"]) > 32:
            die(f"{rel}: description, tags or images over DMM's limits")
        if listing["availability"] != "available":
            if listing.get("release") or listing.get("releaseHistoryUrl"):
                die(f"internal: {rel} is {listing['availability']} and carries a release")
            continue
        hist_rel = local(url(rel, listing["releaseHistoryUrl"], "api"))
        releases = [(rel, listing["release"])] + [(hist_rel, r) for r in doc(hist_rel)["items"]]
        for at, release in releases:
            if len((release.get("changelog") or "").encode()) > 64 * 1024:
                die(f"{at}: changelog over 64 KiB")
            ids = set()
            for a in release["artifacts"]:
                url(at, a["download"]["url"], "downloads")
                if not plain_name(a["filename"]) or not a["filename"].endswith("." + a["format"]):
                    die(f"{at}: artifact filename {a['filename']!r}")
                ids.add(a["id"])
            inst = release["installation"]
            seen = set()
            for c in inst["components"]:
                for f in c["files"]:
                    if f["artifactId"] not in ids or not f["path"].lower().endswith(".vpk") or "\\" in f["path"] or ":" in f["path"]:
                        die(f"{at}: component {c['id']} file {f!r}")
            for cid in inst["requiredComponents"] + [c for g in inst["choiceGroups"] for c in g["componentIds"]]:
                if cid in seen:
                    die(f"{at}: component {cid} is in two places")
                seen.add(cid)
            if seen != {c["id"] for c in inst["components"]}:
                die(f"{at}: every component must be required or in one choice group")


def main():
    args = sys.argv[1:]
    if args[:1] in (["--help"], ["-h"]):
        print(__doc__)
        return
    if not args or args[0].startswith("--"):
        sys.exit(__doc__)

    def flag(k):
        if k not in args:
            return None
        i = args.index(k)
        if i + 1 >= len(args) or args[i + 1].startswith("--"):
            die(f"{k} needs a value")
        return args[i + 1]

    known = {"--out", "--base-url", "--taxonomy", "--prune"}
    for a in args[1:]:
        if a.startswith("--") and a not in known:
            die(f"unknown option {a} (options: {', '.join(sorted(known))})")
    spec_path = os.path.abspath(args[0])
    base = os.path.dirname(spec_path)
    src = load_json(spec_path, "source spec")
    if not isinstance(src, dict):
        die("the source spec is not a JSON object")
    for k in ("source_id", "name", "base_url"):
        if not isinstance(src.get(k), str) or not src[k].strip():
            die(f"the source spec needs {k}")
    if not re.fullmatch(r"[a-z0-9][a-z0-9._-]{0,127}", src["source_id"]):
        die(f"source_id {src['source_id']!r}: lowercase letters, digits, '.', '_' and '-' (and never change it)")
    if len(src["name"].encode()) > 256:
        die("name is over 256 bytes")
    if not isinstance(src.get("mods"), list) or not src["mods"]:
        die("the source spec needs mods: [...]")
    out = flag("--out") or src.get("out")
    if not out:
        die("the source spec has no out and no --out was given")
    out = os.path.abspath(resolve(base, out))
    if os.path.isfile(out):
        die(f"out {out} is a file")
    if os.path.normcase(base + os.sep).startswith(os.path.normcase(out + os.sep)):
        die(f"refusing to write into {out}: it holds the source spec")

    base_url = folder_url(flag("--base-url") or src["base_url"], "base_url")
    homepage = src.get("homepage") or base_url
    check_url(homepage, "homepage")
    authors = src.get("authors", {})
    if not isinstance(authors, dict) or not all(isinstance(v, str) for v in authors.values()):
        die("authors must map an author's name (as bundle.json says it) to the address of their own site")
    for v in authors.values():
        check_url(v, "authors entry")
    media_url = folder_url(src.get("media_url") or base_url + "media/", "media_url")
    dl_template = src.get("downloads_url") or base_url + "files/"
    taxonomy = None
    tax_path = flag("--taxonomy") or src.get("taxonomy")
    if tax_path:
        t = load_json(resolve(base, tax_path), "taxonomy")
        if t.get("format") != "dmm-taxonomy" or t.get("version") != 1:
            die(f"{tax_path} is not DMM's taxonomy (format dmm-taxonomy, version 1)")
        taxonomy = {"heroes": {h["id"]: h["name"] for h in t["heroes"]}, "contentTypes": {c["id"]: c["name"] for c in t["contentTypes"]}}

    bundles = [read_bundle(e, base, taxonomy) for e in src["mods"]]
    if len({b["id"] for b in bundles}) != len(bundles):
        die("two mods share a listing id")
    if len({b["zip_name"].lower() for b in bundles}) != len(bundles):
        die("two mods share a zip name; their downloads would collide")

    tree = Tree(out)
    old = read_manifest(out)
    for rel in old:
        tree.path(rel)  # a manifest naming a path outside out is refused, not obeyed
    api_origin = origin(base_url)
    downloads, media = set(), set()
    notes, uploads = [], []
    for o in src.get("download_origins", []):
        check_url(o, "download_origins entry")
        if origin(o) != o.rstrip("/"):
            die(f"download_origins entry {o!r} must be an origin: scheme and host, no path")
        downloads.add(origin(o))

    entries, listings = [], []
    for b in bundles:
        spec = b["spec"]
        dl_base = folder_url(dl_template.replace("{id}", b["id"]).replace("{version}", spec["release"]), "downloads_url")
        dl_url = dl_base + urllib.parse.quote(b["zip_name"])
        downloads.add(origin(dl_url))
        if urllib.parse.urlsplit(dl_url).hostname == "github.com" and "/releases/download/" in dl_url:
            if GITHUB_RELEASE_REDIRECT not in downloads:
                notes.append(f"GitHub release downloads redirect to {GITHUB_RELEASE_REDIRECT}: declared as a download origin")
            downloads.add(GITHUB_RELEASE_REDIRECT)
        hosted = dl_url.startswith(base_url)
        mrel = f"mods/{b['id']}.json"
        if hosted:
            zrel = urllib.parse.unquote(dl_url[len(base_url):])
            if b["size"] > PAGES_FILE_LIMIT:
                die(f"{b['zip_name']} is {b['size'] / 2**20:.0f} MiB: over GitHub's 100 MiB file limit for a Pages "
                    "repository; host downloads elsewhere (downloads_url) or make the zip smaller")
            if b["size"] > 50 * 2**20:
                notes.append(f"{b['zip_name']} is {b['size'] / 2**20:.1f} MiB: git warns over 50 MiB and takes it")
            tree.copy(zrel, b["zip"], b["sha256"])
            art_url_for = lambda doc_rel, zrel=zrel: rel_url(doc_rel, zrel)
            page_dl = zrel
        else:
            uploads.append((b["zip"], dl_url))
            art_url_for = lambda doc_rel, dl_url=dl_url: dl_url
            page_dl = dl_url

        images, image_rels = [], []
        b["download_url"], b["image_sizes"] = dl_url, {}
        for name, data in b["previews"]:
            stem = os.path.splitext(name)[0]
            target = media_url + f"{b['id']}/{stem}.jpg"
            if not re.fullmatch(r"[A-Za-z0-9._-]+", stem):
                die(f"{b['id']}: preview {name!r} needs a plain name (letters, digits, . _ -)")
            jpg, size = jpeg(data, 1920, MEDIA_TARGET)
            if target.startswith(base_url):
                rel = target[len(base_url):]
                tree.put(rel, jpg)
                image_rels.append(rel)
                b["image_sizes"][rel] = size
                images.append(rel_url(mrel, rel))
            else:
                die(f"media_url {media_url} is not under base_url {base_url}: media is written into the tree, so "
                    "it must be served from it")
            media.add(origin(target))

        artifact = {"id": "release-zip", "filename": b["zip_name"], "format": "zip",
                    "download": {"url": None}, "size": b["size"], "hashes": {"sha256": b["sha256"]}}
        installation = {"mode": "components", "components": b["components"],
                        "requiredComponents": b["required"], "choiceGroups": b["groups"]}
        rev = revision([artifact], installation)
        published = published_of(spec)
        notes_text = changelog_section(b["changelog"], spec["release"]) if b["changelog"] else None
        if b["changelog"] and notes_text is None:
            die(f"{b['id']}: CHANGELOG.md has no '## {spec['release']}' section")

        def release_doc(doc_rel):
            a = dict(artifact, download={"url": art_url_for(doc_rel)})
            r = {"id": f"{spec['release']}-{rev[:8]}", "contentRevision": rev, "version": spec["release"],
                 "publishedAt": published}
            if notes_text:
                r["changelog"] = notes_text
            r["artifacts"] = [a]
            r["installation"] = installation
            return r

        hrel = f"mods/{b['id']}/releases.json"
        history = [release_doc(hrel)]
        if hrel in old and os.path.isfile(tree.path(hrel)):
            try:
                earlier = json.load(open(tree.path(hrel), encoding="utf-8"))["items"]
            except (OSError, ValueError, KeyError, TypeError) as e:
                die(f"{hrel} from the last run does not read ({e}); move it away to start the history again")
            for r in earlier:
                if r.get("id") == history[0]["id"]:
                    continue
                if r.get("version") == spec["release"]:
                    notes.append(f"{b['id']}: release {spec['release']} was published before with other bytes "
                                 f"({r.get('contentRevision')}); give a new build a new release number")
                history.append(r)
            history = history[:1] + sorted(history[1:], key=lambda r: (r.get("publishedAt") or "", r["id"]), reverse=True)
        tree.json(hrel, {"items": history, "nextCursor": None})

        dl_note = f"{b['size'] / 1e6:.0f} MB"
        hero_name = taxonomy["heroes"][spec["hero_id"]] if taxonomy else spec["hero"]
        listing = {
            "id": b["id"], "title": spec["name"], "availability": "available", "isNsfw": spec["nsfw"],
            "description": description(b, dl_url, dl_note), "descriptionFormat": "markdown",
            "author": spec["author"], "hero": spec["hero_id"], "contentType": spec["content_type"],
            "tags": spec.get("tags", []), "images": images,
            "homepageUrl": b["entry"].get("homepage") or f"{base_url}{b['id']}/",
            "updatedAt": published, "release": release_doc(mrel), "releaseHistoryUrl": rel_url(mrel, hrel),
        }
        tree.json(mrel, listing)
        listings.append(listing)
        entries.append((b, listing, page_dl, image_rels, hero_name, history))

    # What is coming: a card on the landing page, and (in_manager) a listing DMM shows but cannot install. DMM has
    # no "coming soon" state; "unavailable" is its word for a listing with nothing to install yet, so the
    # description says what it is. Released later under the same id, the listing turns installable in place.
    upcoming = []
    for n, u in enumerate(src.get("upcoming", [])):
        if not isinstance(u, dict) or not isinstance(u.get("title"), str) or not u["title"].strip():
            die("each upcoming entry needs a title")
        u = dict(u)
        uid = u.get("id")
        if uid is not None and (not safe_id(uid) or uid in RESERVED_DIRS or uid in {b["id"] for b in bundles}):
            die(f"upcoming id {uid!r} must be a new listing id (lowercase letters, digits, hyphens)")
        if u.get("image"):
            ip = resolve(base, u["image"])
            if not os.path.isfile(ip):
                die(f"upcoming image {ip} does not exist")
            u["image_rel"] = f"media/{uid}/teaser.jpg" if uid else f"media/upcoming-{n + 1}.jpg"
            if not (media_url + u["image_rel"][len("media/"):]).startswith(base_url):
                die("an upcoming image is written into the tree, so media_url must be under base_url")
            tree.put(u["image_rel"], jpeg(open(ip, "rb").read(), 1920, MEDIA_TARGET)[0])
            media.add(origin(base_url))
        if u.get("in_manager"):
            for k in ("id", "author", "hero_id"):
                if not isinstance(u.get(k), str) or not u[k]:
                    die(f"upcoming {u['title']!r}: in_manager needs {k}")
            if not isinstance(u.get("nsfw"), bool):
                die(f"upcoming {u['title']!r}: in_manager needs nsfw true or false")
            ctype = u.get("content_type", "skins")
            if ctype not in (taxonomy["contentTypes"] if taxonomy else CONTENT_TYPES):
                die(f"upcoming {u['title']!r}: content_type {ctype!r} is not a DMM content type")
            if taxonomy and u["hero_id"] not in taxonomy["heroes"]:
                die(f"upcoming {u['title']!r}: hero_id {u['hero_id']!r} is not in the taxonomy")
            mrel = f"mods/{uid}.json"
            text = (u.get("note", "").strip() + "\n\n" if u.get("note") else "") + \
                "**Not released yet.** There is nothing to install here until it is; this listing becomes the download when it is out."
            listing = {"id": uid, "title": u["title"], "availability": "unavailable", "isNsfw": u["nsfw"],
                       "description": text + "\n", "descriptionFormat": "markdown", "author": u["author"],
                       "hero": u["hero_id"], "contentType": ctype, "tags": u.get("tags", []),
                       "images": [rel_url(mrel, u["image_rel"])] if u.get("image_rel") else [],
                       "homepageUrl": base_url}
            if u.get("date"):
                if not re.fullmatch(r"\d{4}-\d{2}-\d{2}", str(u["date"])):
                    die(f"upcoming {u['title']!r}: date {u['date']!r} is not YYYY-MM-DD")
                listing["updatedAt"] = f"{u['date']}T12:00:00Z"
            tree.json(mrel, listing)
            listings.append(listing)
        upcoming.append(u)

    icon_rel = None
    if src.get("icon"):
        ip = resolve(base, src["icon"])
        if not os.path.isfile(ip):
            die(f"icon {ip} does not exist")
        icon_target = media_url + "icon.jpg"
        if not icon_target.startswith(base_url):
            die("the icon is written into the tree, so media_url must be under base_url")
        icon_rel = icon_target[len(base_url):]
        tree.put(icon_rel, jpeg(open(ip, "rb").read(), 256, MAX_ICON, square=True)[0])
        media.add(origin(icon_target))

    items = [{"id": l["id"], "title": l["title"], "availability": l["availability"], "isNsfw": l["isNsfw"],
              "hero": l["hero"], "contentType": l["contentType"], "detailUrl": rel_url("catalog.json", f"mods/{l['id']}.json")}
             for l in listings]
    snapshot = hashlib.sha256(json.dumps([items, listings], sort_keys=True).encode()).hexdigest()[:16]
    tree.json("catalog.json", {"snapshotId": snapshot, "items": items, "nextUrl": None})

    def descriptor(at):
        d = {"format": "dmm-source", "protocolVersion": "1.0", "sourceId": src["source_id"], "name": src["name"],
             "gameId": "deadlock", "catalog": {"kind": "static", "indexUrl": rel_url(at, "catalog.json")},
             "networkOrigins": {"api": [api_origin], "downloads": sorted(downloads), "media": sorted(media)},
             "homepageUrl": homepage}
        if icon_rel:
            d["iconUrl"] = rel_url(at, icon_rel)
        d["capabilities"] = {"search": "local", "releaseHistory": True, "installationChoices": True}
        return d

    desc_url = base_url + "dmm-source.json"
    tree.json("dmm-source.json", descriptor("dmm-source.json"))
    if urllib.parse.urlsplit(base_url).path == "/":
        tree.json(".well-known/dmm-source.json", descriptor(".well-known/dmm-source.json"))
    tree.put(".nojekyll", b"")
    page = src.get("page") or {}
    if not isinstance(page, dict):
        die("page must be an object: title, intro, css, head, header, footer")
    theme = {k: fragment(base, page, k) for k in ("css", "head", "header", "footer")}
    tree.put("index.html", landing(src, base_url, desc_url, entries, icon_rel, theme, upcoming))
    for en in entries:
        tree.put(f"{en[1]['id']}/index.html", mod_page(src, base_url, desc_url, en, icon_rel, theme))
    tree.put("feed.xml", atom(src, base_url, entries))
    tree.put("sitemap.xml", sitemap(base_url, entries))
    check_tree(tree, base_url, descriptor("dmm-source.json"))

    # Nothing is written over a file this tool did not write (unless it already holds the same bytes).
    for rel, data in tree.files.items():
        p = tree.path(rel)
        if rel in old or not os.path.exists(p):
            continue
        if os.path.isdir(p):
            die(f"refusing to write {rel}: a folder of that name is in {out}")
        same = sha256_file(p) == (data[2] if isinstance(data, tuple) else hashlib.sha256(data).hexdigest())
        if not same:
            die(f"refusing to replace {p}: this tool did not write it (it is not in {MANIFEST})")

    # Artifacts and media first, then the release documents, the catalog, and the descriptor last, so a host that
    # serves the folder while it is written never lists a release whose bytes are not there yet.
    order = lambda rel: (0 if isinstance(tree.files[rel], tuple) or rel.startswith("media/") else
                         1 if rel.startswith("mods/") else 2 if rel == "catalog.json" else 3, rel)
    for rel in sorted(tree.files, key=order):
        data, p = tree.files[rel], tree.path(rel)
        os.makedirs(os.path.dirname(p), exist_ok=True)
        if isinstance(data, tuple):
            if os.path.isfile(p) and os.path.getsize(p) == os.path.getsize(data[1]) and sha256_file(p) == data[2]:
                continue
            shutil.copyfile(data[1], p + ".tmp")
        else:
            if os.path.isfile(p) and open(p, "rb").read() == data:
                continue
            with open(p + ".tmp", "wb") as f:
                f.write(data)
        os.replace(p + ".tmp", p)

    kept = set()
    dl_prefix = dl_template[len(base_url):] if dl_template.startswith(base_url) and "{" not in dl_template else None
    for rel in sorted(old - set(tree.files) - {MANIFEST}):
        p = tree.path(rel)
        if dl_prefix and rel.startswith(dl_prefix) and rel.endswith(".zip") and "--prune" not in args and os.path.isfile(p):
            kept.add(rel)
            continue
        if os.path.isfile(p):
            os.remove(p)
            print("removed", rel)
        d = os.path.dirname(p)
        while d != tree.out and os.path.isdir(d) and not os.listdir(d):
            os.rmdir(d)
            d = os.path.dirname(d)
    with open(os.path.join(out, MANIFEST + ".tmp"), "w", encoding="utf-8", newline="\n") as f:
        f.write("".join(r + "\n" for r in sorted(set(tree.files) | kept | {MANIFEST})))
    os.replace(os.path.join(out, MANIFEST + ".tmp"), os.path.join(out, MANIFEST))

    for b, l, dl_url, _, _, _ in entries:
        r = l["release"]
        print(f"  {b['id']}: {r['version']} ({r['id']}), {b['size'] / 1e6:.1f} MB, "
              f"{len(r['installation']['components'])} component(s), {len(l['images'])} image(s)")
    for rel in sorted(kept):
        print("kept", rel, "(an earlier release's download; --prune removes it)")
    for n in notes:
        print("note:", n)
    for path, url in uploads:
        print(f"upload {path} -> {url}")
    print("source ->", out)
    print("descriptor:", desc_url)
    print("add link:", dmm_link("add", url=desc_url))


def dmm_link(action, **params):
    return f"deadlock-mod-manager://sources/{action}?" + "&".join(f"{k}={urllib.parse.quote(v, safe='')}" for k, v in params.items())


PAGE_CSS = """
:root { --bg: #f6f4ef; --surface: #ffffff; --ink: #1d1b18; --muted: #6b665d; --line: #e2ddd3; --accent: #9a3b1f;
  --accent-ink: #ffffff; --soft: #efe9de; --font: system-ui, -apple-system, "Segoe UI", Roboto, sans-serif;
  --mono: ui-monospace, "Cascadia Mono", Consolas, monospace; --width: 58rem; --prose: 42rem; --radius: 8px;
  color-scheme: light dark; }
@media (prefers-color-scheme: dark) { :root { --bg: #15140f; --surface: #1f1d17; --ink: #ece7dc; --muted: #a29b8d;
  --line: #34312a; --accent: #e07a4f; --accent-ink: #15140f; --soft: #2a2720; } }
* { box-sizing: border-box; }
html { -webkit-text-size-adjust: 100%; }
body { margin: 0; background: var(--bg); color: var(--ink); font: 16px/1.55 var(--font); }
img { max-width: 100%; height: auto; display: block; }
a { color: inherit; text-underline-offset: 0.2em; }
a:focus-visible, summary:focus-visible { outline: 2px solid var(--accent); outline-offset: 3px; }
.wrap { width: 100%; max-width: var(--width); margin-inline: auto; padding-inline: clamp(1rem, 4vw, 1.5rem); }
main { padding-block: 2.5rem 4rem; }
.crumb { font-size: 0.9rem; color: var(--muted); margin: 0 0 1.25rem; }
.crumb a { text-decoration: none; }
.crumb a:hover { text-decoration: underline; }
.page-title { font-size: clamp(2rem, 1.5rem + 2vw, 3rem); line-height: 1.15; letter-spacing: -0.02em; margin: 0 0 0.35rem;
  text-wrap: balance; }
.tagline { font-size: 1.15rem; font-style: italic; margin: 0 0 0.5rem; }
.lede { color: var(--muted); max-width: var(--prose); margin: 0 0 0.5rem; }
.meta { color: var(--muted); font-size: 0.9rem; font-variant-numeric: tabular-nums; margin: 0; }
.sec { margin-top: 3rem; }
.sec-head { margin-bottom: 1rem; padding-bottom: 0.75rem; border-bottom: 1px solid var(--line); }
.sec-title { font-size: 0.78rem; font-weight: 600; letter-spacing: 0.12em; text-transform: uppercase; color: var(--muted); margin: 0; }
.card { display: flex; flex-wrap: wrap; gap: 1.5rem; padding-block: 1.5rem; }
.card + .card { border-top: 1px solid var(--line); }
.card-shot { flex: 0 1 24rem; min-width: min(100%, 16rem); border-radius: var(--radius); overflow: hidden; background: var(--soft);
  aspect-ratio: 16 / 9; }
.card-shot img { width: 100%; height: 100%; object-fit: cover; object-position: 50% 20%; }
.card-body { flex: 1 1 18rem; min-width: 0; }
.card-line { margin: 0 0 0.5rem; display: flex; flex-wrap: wrap; align-items: baseline; gap: 0.15rem 0.45rem; }
.card-title { font-weight: 600; font-size: 1.15rem; text-decoration: none; }
a.card-title:hover { text-decoration: underline; }
.card-text { color: var(--muted); margin: 0 0 1rem; max-width: 60ch; }
.pill { display: inline-flex; align-items: center; gap: 0.35em; padding: 0.1em 0.55em; border: 1px solid var(--line);
  border-radius: calc(var(--radius) * 0.5); font-size: 0.72rem; font-weight: 600; letter-spacing: 0.04em; color: var(--muted);
  white-space: nowrap; }
.pill.badge { border-color: var(--accent); color: var(--accent); }
.actions { display: flex; flex-wrap: wrap; gap: 0.6rem; align-items: center; margin: 1rem 0 0.5rem; }
.btn { display: inline-flex; align-items: center; gap: 0.5em; min-height: 2.75rem; padding: 0.55rem 1rem; border-radius: var(--radius);
  border: 1px solid var(--line); background: var(--surface); color: var(--ink); text-decoration: none; font-weight: 600;
  font-size: 0.95rem; }
.btn:hover { border-color: var(--muted); }
.btn.primary { background: var(--accent); border-color: var(--accent); color: var(--accent-ink); }
.btn .dim { font-weight: 400; opacity: 0.7; }
.more { font-size: 0.95rem; padding: 0.55rem 0.25rem; }
.small { font-size: 0.88rem; color: var(--muted); margin: 0.5rem 0; max-width: var(--prose); }
.url { display: block; padding: 0.6rem 0.75rem; border-radius: var(--radius); background: var(--soft); border: 1px solid var(--line);
  font: 0.85rem/1.4 var(--mono); overflow-wrap: anywhere; user-select: all; max-width: var(--prose); }
code { font: 0.85em var(--mono); background: var(--soft); padding: 1px 5px; border-radius: 4px; overflow-wrap: anywhere; }
.gallery { margin: 1.5rem 0 0; }
.shot { display: block; border-radius: var(--radius); overflow: hidden; background: var(--soft); }
.shot img { width: 100%; max-height: 78vh; object-fit: contain; }
.thumbs { display: flex; gap: 0.5rem; margin-top: 0.5rem; overflow-x: auto; padding-bottom: 0.25rem; }
.thumbs a { flex: 0 0 auto; width: 6.5rem; aspect-ratio: 16 / 10; border-radius: calc(var(--radius) * 0.75); overflow: hidden;
  border: 1px solid var(--line); opacity: 0.6; }
.thumbs a:hover, .thumbs a[aria-current="true"] { opacity: 1; border-color: var(--muted); }
.thumbs img { width: 100%; height: 100%; object-fit: cover; object-position: 50% 20%; }
.prose { max-width: var(--prose); margin-top: 2.5rem; }
.prose h2 { font-size: 1.3rem; margin: 2.2rem 0 0.6rem; }
.prose h3 { font-size: 1.05rem; margin: 2rem 0 0.6rem; }
.prose h4 { font-size: 1.1rem; font-weight: 500; margin: 0 0 1rem; }
.prose p { margin: 0 0 1rem; }
.prose ul, .prose ol { margin: 0 0 1rem; padding-left: 1.3rem; }
.prose li { margin: 0 0 0.45rem; }
.prose blockquote { margin: 0 0 1rem; padding: 0.1rem 0 0.1rem 1rem; border-left: 2px solid var(--line); color: var(--muted); }
.prose blockquote p { margin: 0; }
.prose hr { border: 0; border-top: 1px solid var(--line); margin: 2rem 0; }
.prose a { text-decoration: underline; }
.releases { list-style: none; margin: 0; padding: 0; max-width: var(--prose); }
.releases > li { padding: 1rem 0; }
.releases > li + li { border-top: 1px solid var(--line); }
.rel-head { margin: 0 0 0.4rem; display: flex; flex-wrap: wrap; gap: 0.15rem 0.6rem; align-items: baseline; }
.rel-head strong { font-size: 1.05rem; }
.releases ul { margin: 0; padding-left: 1.2rem; color: var(--muted); }
.releases li li { margin: 0 0 0.25rem; }
.facts { display: grid; grid-template-columns: max-content minmax(0, 1fr); gap: 0.4rem 1.25rem; margin: 0; max-width: var(--prose);
  font-size: 0.92rem; }
.facts dt { color: var(--muted); }
.facts dd { margin: 0; overflow-wrap: anywhere; }
.foot { margin-top: 1rem; padding-block: 1.5rem 3rem; font-size: 0.88rem; color: var(--muted); border-top: 1px solid var(--line); }
.foot p { margin: 0 0 0.5rem; max-width: var(--prose); }
@media (prefers-reduced-motion: reduce) { * { transition: none !important; animation: none !important; } }
"""

# The gallery without this script is a row of links to the pictures; with it, a thumbnail shows its picture in place.
GALLERY_JS = """
document.querySelectorAll('.gallery').forEach(function (g) {
  var shot = g.querySelector('.shot'), img = shot && shot.querySelector('img');
  if (!img) return;
  g.querySelectorAll('.thumbs a').forEach(function (a) {
    a.addEventListener('click', function (ev) {
      ev.preventDefault();
      img.src = a.href; img.alt = a.querySelector('img').alt; shot.href = a.href;
      g.querySelectorAll('.thumbs a').forEach(function (b) { b.setAttribute('aria-current', b === a ? 'true' : 'false'); });
    });
  });
});
"""

MONTHS = ("January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November",
          "December")
RESERVED_DIRS = ("mods", "media", "files")
# A link's kind: "same" is this mod on another site (its Nexus Mods page), "video" is a video of it (a trailer).
# Without a kind a link is related material and appears on the page only.
LINK_KINDS = ("same", "video")


def long_date(iso):
    """2026-11-24 (or a timestamp that starts with it) as 24 November 2026."""
    m = re.match(r"(\d{4})-(\d{2})-(\d{2})", iso or "")
    return f"{int(m.group(3))} {MONTHS[int(m.group(2)) - 1]} {m.group(1)}" if m else (iso or "")


def md_inline(t):
    """The inline Markdown this tool writes (code, links, bold, italic, strike, a two-space break) as HTML."""
    t = html.escape(t, quote=False)
    t = re.sub(r"`([^`]+)`", r"<code>\1</code>", t)

    def link(m):
        u = m.group(2).replace("&amp;", "&")
        if not re.match(r"(https://|http://localhost|\.{0,2}/|#)", u):
            return m.group(0)
        return f'<a href="{html.escape(u)}">{m.group(1)}</a>'
    t = re.sub(r"\[([^\]]+)\]\(([^)\s]+)\)", link, t)
    t = re.sub(r"\*\*(.+?)\*\*", r"<strong>\1</strong>", t)
    t = re.sub(r"(?<![\w*])\*(?!\s)(.+?)(?<!\s)\*(?![\w*])", r"<em>\1</em>", t)
    t = re.sub(r"~~(.+?)~~", r"<s>\1</s>", t)
    return re.sub(r" {2,}\n", "<br>\n", t)


def md_html(md):
    """A listing description (the Markdown `description` writes: headings, paragraphs, quotes, lists, rules) as HTML,
    so the web page and DMM show the same words. A paragraph that is one bold line reads as a heading."""
    out = []
    for block in re.split(r"\n\s*\n", md.strip()):
        lines = block.split("\n")
        h = re.match(r"(#{1,6})\s+(.*)", lines[0])
        if re.fullmatch(r"-{3,}", block.strip()):
            out.append("<hr>")
        elif h and len(lines) == 1:
            n = max(2, min(len(h.group(1)), 6))   # the page's one h1 is the mod's name
            out.append(f"<h{n}>{md_inline(h.group(2))}</h{n}>")
        elif all(l.startswith(">") for l in lines):
            body = "\n".join(re.sub(r"^> ?", "", l) for l in lines)
            out.append("<blockquote><p>" + re.sub(r"(?<!<br>)\n", "<br>\n", md_inline(body)) + "</p></blockquote>")
        elif all(re.match(r"[-*] ", l) for l in lines):
            out.append("<ul>" + "".join(f"<li>{md_inline(l[2:])}</li>" for l in lines) + "</ul>")
        elif all(re.match(r"\d+[.)] ", l) for l in lines):
            out.append("<ol>" + "".join("<li>" + md_inline(re.sub(r"^\d+[.)] ", "", l)) + "</li>" for l in lines) + "</ol>")
        elif re.fullmatch(r"\*\*[^*]+\*\*", block.strip()):
            out.append(f"<h3>{md_inline(block.strip()[2:-2])}</h3>")
        else:
            out.append("<p>" + md_inline(block) + "</p>")
    return "\n".join(out)


def changelog_html(text):
    """A release's plain-text changelog (a heading line, then '- ' items) as a list; other lines as paragraphs."""
    items, paras = [], []
    for line in (text or "").splitlines()[1:]:
        line = line.strip()
        if line.startswith("- "):
            items.append(line[2:])
        elif line and items:
            items[-1] += " " + line
        elif line:
            paras.append(line)
    e = html.escape
    return "".join(f"<p>{e(p)}</p>" for p in paras) + ("<ul>" + "".join(f"<li>{e(i)}</li>" for i in items) + "</ul>" if items else "")


def fragment(base, page, key):
    """A file the spec's page block names (the site's own stylesheet or chrome), or nothing."""
    if not page.get(key):
        return ""
    p = resolve(base, page[key])
    if not os.path.isfile(p):
        die(f"page.{key} {p} does not exist")
    return open(p, encoding="utf-8").read().strip()


def ld_script(doc):
    """Structured data as a script element. JSON allows `<`, `>` and `&` only inside strings, where \\u escapes mean
    the same characters, so a name holding `</script>` or `<!--` cannot end or bend the element."""
    s = json.dumps(doc, indent=1, ensure_ascii=False)
    for c, u in (("&", "\\u0026"), ("<", "\\u003c"), (">", "\\u003e"), (" ", "\\u2028"), (" ", "\\u2029")):
        s = s.replace(c, u)
    return f'<script type="application/ld+json">\n{s}\n</script>'


def shell(title, desc, canonical, og_image, root, body, theme, icon_rel, og_size=None, alternate=None, ld=None):
    """One HTML document: the shared head, the theme's header and footer, and `body` between them. `root` is the
    way from this page to the top of the tree ("" or "../"); {root} in a theme fragment becomes it. `alternate` is
    (title, path in the tree) of the JSON the page is a view of; `ld` its schema.org structured data."""
    e = html.escape
    fill = lambda s: s.replace("{root}", root)
    head = [f'<meta property="og:image" content="{e(og_image)}">', '<meta name="twitter:card" content="summary_large_image">'] if og_image else []
    if og_image and og_size:
        head[1:1] = [f'<meta property="og:image:width" content="{og_size[0]}">',
                     f'<meta property="og:image:height" content="{og_size[1]}">']
    if icon_rel:
        head.append(f'<link rel="icon" href="{e(root + icon_rel)}">')
    head.append(f'<link rel="alternate" type="application/atom+xml" title="Releases" href="{root}feed.xml">')
    if alternate:
        head.append(f'<link rel="alternate" type="application/json" title="{e(alternate[0])}" href="{e(root + alternate[1])}">')
    if ld:
        head.append(ld_script(ld))
    if theme["head"]:
        head.append(fill(theme["head"]))
    nl = "\n"
    return f"""<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{e(title)}</title>
<meta name="description" content="{e(desc)}">
<link rel="canonical" href="{e(canonical)}">
<meta property="og:title" content="{e(title)}">
<meta property="og:description" content="{e(desc)}">
<meta property="og:url" content="{e(canonical)}">
<meta property="og:type" content="website">
{nl.join(head)}
<style>{PAGE_CSS}{theme['css']}</style>
</head>
<body>
{fill(theme['header'])}
<main class="wrap">
{body}
</main>
<footer class="foot wrap">
<p>Every download is a zip you can also install by hand; its README says how.</p>
{fill(theme['footer'])}
</footer>
<script>{GALLERY_JS}</script>
</body>
</html>
"""


def badge_html(b):
    """The mod's badge (an early release, say), a pill beside its version on the card and the page."""
    badge = b["entry"].get("badge")
    return f' <span class="pill badge">{html.escape(badge)}</span>' if badge else ""


def blurb_of(b):
    return b["entry"].get("blurb") or b["spec"]["description"]


def buttons(src, desc_url, b, l, dl_href):
    e = html.escape
    install = dmm_link("install", url=desc_url, source=src["source_id"], listing=l["id"])
    return (f'<a class="btn primary" href="{e(install)}">Install with Deadlock Mod Manager</a>'
            f'<a class="btn" href="{e(dl_href)}" download>Download zip <span class="dim">{b["size"] / 1e6:.0f} MB</span></a>')


def landing(src, base_url, desc_url, entries, icon_rel, theme, upcoming):
    """The front page: every mod as a card that leads to its own page, what is coming, and how to add the source."""
    e = html.escape
    page = src.get("page") or {}
    dmm = src.get("dmm_url") or DMM_URL
    title = page.get("title") or src["name"]
    intro = page.get("intro") or ("Install these with Deadlock Mod Manager: add this source once and its mods show up in "
                                  "the Mods Store, with updates when a new version is out.")
    cards = []
    for b, l, dl_url, image_rels, hero_name, history in entries:
        spec = b["spec"]
        img = f'<img src="{e(image_rels[0])}" alt="" loading="lazy">' if image_rels else ""
        cards.append(f"""
<article class="card" id="{e(l['id'])}">
  <a class="card-shot" href="{e(l['id'])}/" tabindex="-1" aria-hidden="true">{img}</a>
  <div class="card-body">
    <p class="card-line"><a class="card-title" href="{e(l['id'])}/">{e(spec['name'])}</a>
      <span class="meta">{e(hero_name)} &middot; {e(long_date(spec['date']))}</span> <span class="pill">{e(spec['release'])}</span>{badge_html(b)}</p>
    <p class="card-text">{md_inline(blurb_of(b))}</p>
    <p class="actions">{buttons(src, desc_url, b, l, dl_url)}<a class="more" href="{e(l['id'])}/">Details</a></p>
  </div>
</article>""")
    soon = []
    for u in upcoming:
        img = f'<img src="{e(u["image_rel"])}" alt="" loading="lazy">' if u.get("image_rel") else ""
        bits = " &middot; ".join(e(x) for x in (u.get("hero"), u.get("when")) if x)
        soon.append(f"""
<article class="card soon">
  <div class="card-shot">{img}</div>
  <div class="card-body">
    <p class="card-line"><span class="card-title">{e(u['title'])}</span> <span class="meta">{bits}</span> <span class="pill">coming</span></p>
    <p class="card-text">{md_inline(u.get('note', ''))}</p>
  </div>
</article>""")
    soon_sec = ('<section class="sec" aria-labelledby="coming"><div class="sec-head"><h2 class="sec-title" id="coming">Coming</h2></div>'
                + "".join(soon) + "</section>") if soon else ""
    body = f"""<header class="top">
  <h1 class="page-title">{e(title)}</h1>
  <p class="lede">{md_inline(intro)}</p>
</header>
<section class="sec" aria-labelledby="released">
  <div class="sec-head"><h2 class="sec-title" id="released">Released</h2></div>{''.join(cards)}
</section>
{soon_sec}
<section class="sec" aria-labelledby="install" id="source">
  <div class="sec-head"><h2 class="sec-title" id="install">In the mod manager</h2></div>
  <p class="lede">Add this page to Deadlock Mod Manager as a mod source once. Everything here then shows up in its Mods
  Store, and it tells you when a new version is out.</p>
  <p class="actions"><a class="btn primary" href="{e(dmm_link('add', url=desc_url))}">Add source to Deadlock Mod Manager</a>
    <a class="btn" href="{e(dmm)}">Get the mod manager</a></p>
  <p class="small">Needs a version with <strong>Settings &gt; Mods &gt; Mod sources</strong>. If a button does nothing,
  the mod manager is not installed or is older than that: add the source by hand with this address, or download a zip.</p>
  <code class="url">{e(desc_url)}</code>
</section>"""
    og = base_url + entries[0][3][0] if entries and entries[0][3] else None
    og_size = entries[0][0]["image_sizes"].get(entries[0][3][0]) if og else None
    ld = {"@context": "https://schema.org", "@type": "CollectionPage", "@id": base_url, "url": base_url, "name": title,
          "description": plain_text(intro), "inLanguage": "en",
          "mainEntity": {"@type": "ItemList", "numberOfItems": len(entries), "itemListElement": [
              {"@type": "ListItem", "position": i + 1, "url": f"{base_url}{en[1]['id']}/", "name": en[0]["spec"]["name"]}
              for i, en in enumerate(entries)]}}
    return shell(title, plain_text(intro), base_url, og, "", body, theme, icon_rel, og_size,
                 ("Deadlock Mod Manager catalog", "catalog.json"), ld)


def mod_page(src, base_url, desc_url, entry, icon_rel, theme):
    """A mod's own page: its pictures, the install buttons, the description DMM shows, every release, the checksum."""
    e = html.escape
    b, l, dl_url, image_rels, hero_name, history = entry
    spec, root = b["spec"], "../"
    up = lambda rel: rel if re.match(r"https?://", rel) else root + rel
    title = (src.get("page") or {}).get("title") or src["name"]
    gallery = ""
    if image_rels:
        thumbs = "".join(
            f'<a href="{e(up(r))}" aria-current="{"true" if i == 0 else "false"}"><img src="{e(up(r))}" '
            f'alt="{e(spec["name"])}, picture {i + 1}" loading="lazy"></a>' for i, r in enumerate(image_rels))
        gallery = (f'<figure class="gallery"><a class="shot" href="{e(up(image_rels[0]))}"><img src="{e(up(image_rels[0]))}" '
                   f'alt="{e(spec["name"])}, picture 1"></a>'
                   + (f'<div class="thumbs">{thumbs}</div>' if len(image_rels) > 1 else "") + "</figure>")
    links = "".join(f'<a class="more" href="{e(x["url"])}">{e(x["label"])}</a>' for x in b["entry"].get("links", []))
    tagline = f'<p class="tagline">{md_inline(b["entry"]["tagline"])}</p>' if b["entry"].get("tagline") else ""
    author_url = (src.get("authors") or {}).get(spec["author"])
    author = f'<a href="{e(author_url)}" rel="author">{e(spec["author"])}</a>' if author_url else e(spec["author"])
    build = f" &middot; Deadlock build {e(str(spec['game_build']))}" if spec.get("game_build") else ""
    if b.get("proven"):
        build = f" &middot; proven on Deadlock build {e(str(b['proven'][-1]))}"
    rels = []
    for r in history:
        a = r["artifacts"][0]
        href = urllib.parse.urljoin(base_url + f"mods/{l['id']}/releases.json", a["download"]["url"])
        if href.startswith(base_url):
            href = root + href[len(base_url):]
        rels.append(f'<li><p class="rel-head"><strong>{e(r["version"])}</strong> <span class="meta">{e(long_date(r.get("publishedAt")))}</span> '
                    f'<a class="meta" href="{e(href)}" download>{e(a["filename"])}</a></p>{changelog_html(r.get("changelog"))}</li>')
    body = f"""<nav class="crumb" aria-label="Breadcrumb"><a href="{root}">&larr; {e(title)}</a></nav>
<header class="top">
  <h1 class="page-title">{e(spec['name'])}</h1>
  {tagline}
  <p class="meta">{e(hero_name)} &middot; by {author} &middot; version {e(spec['release'])}{badge_html(b)} &middot; {e(long_date(spec['date']))}{build}</p>
</header>
<p class="actions">{buttons(src, desc_url, b, l, up(dl_url))}{links}</p>
<p class="small">One click needs a Deadlock Mod Manager with mod sources (<strong>Settings &gt; Mods &gt; Mod sources</strong>).
Without it, download the zip: its README has the steps, by hand or by import.</p>
{gallery}
<div class="prose">
{md_html(l['description'])}
</div>
<section class="sec" aria-labelledby="releases">
  <div class="sec-head"><h2 class="sec-title" id="releases">Releases</h2></div>
  <ol class="releases" reversed>{''.join(rels)}</ol>
</section>
<section class="sec" aria-labelledby="file">
  <div class="sec-head"><h2 class="sec-title" id="file">The file</h2></div>
  <dl class="facts"><dt>Download</dt><dd>{e(b['zip_name'])}, {b['size'] / 1e6:.1f} MB</dd>
  <dt>SHA-256</dt><dd><code>{e(b['sha256'])}</code></dd></dl>
</section>"""
    og = base_url + image_rels[0] if image_rels else None
    ld = mod_ld(src, base_url, desc_url, entry, title, author_url)
    return shell(f"{spec['name']} | {title}", plain_text(blurb_of(b)), f"{base_url}{l['id']}/", og, root, body, theme,
                 icon_rel, b["image_sizes"].get(image_rels[0]) if image_rels else None,
                 ("Deadlock Mod Manager listing", f"mods/{l['id']}.json"), ld)


# Deadlock's own page, the game every listing is for (gameId "deadlock" in the descriptor).
DEADLOCK = {"@type": "VideoGame", "name": "Deadlock", "url": "https://store.steampowered.com/app/1422450/Deadlock/"}


def mod_ld(src, base_url, desc_url, entry, site_title, author_url):
    """A mod page's schema.org structured data, from what the listing already says.

    The mod is a SoftwareApplication: the type search engines read for something downloaded and installed, with
    the properties a release has (downloadUrl, fileSize, softwareVersion, releaseNotes, installUrl). The game is
    its softwareRequirements, a VideoGame (itself a SoftwareApplication). The zip is a MediaObject under encoding,
    the one type with a sha256 property. The same mod on another site is sameAs; a video of it is a VideoObject it
    is the subjectOf. Builds it was proven on are said on the page and not here: no schema.org property means
    "tested on", and softwareRequirements would claim the mod needs that build. The BreadcrumbList is the page's
    own breadcrumb."""
    b, l, _, image_rels, _, history = entry
    spec = b["spec"]
    url = f"{base_url}{l['id']}/"
    person = {"@type": "Person", "name": spec["author"]}
    if author_url:
        person["url"] = author_url
    app = {"@type": "SoftwareApplication", "@id": url + "#mod", "url": url, "name": spec["name"],
           "description": plain_text(blurb_of(b)), "author": person, "softwareVersion": spec["release"],
           "datePublished": min(r.get("publishedAt") or published_of(spec) for r in history)[:10],
           "dateModified": spec["date"], "applicationCategory": "GameApplication", "softwareRequirements": DEADLOCK,
           "isAccessibleForFree": True, "inLanguage": "en"}
    if spec.get("tags"):
        app["keywords"] = ", ".join(spec["tags"])
    if image_rels:
        app["image"] = [dict({"@type": "ImageObject", "contentUrl": base_url + r},
                             **({"width": b["image_sizes"][r][0], "height": b["image_sizes"][r][1]} if r in b["image_sizes"] else {}))
                        for r in image_rels]
    app["downloadUrl"] = b["download_url"]
    app["fileSize"] = f"{b['size'] / 1e6:.1f} MB"
    app["encoding"] = {"@type": "MediaObject", "name": b["zip_name"], "contentUrl": b["download_url"],
                       "encodingFormat": "application/zip", "contentSize": app["fileSize"], "sha256": b["sha256"]}
    app["installUrl"] = dmm_link("install", url=desc_url, source=src["source_id"], listing=l["id"])
    if l["release"].get("changelog"):
        app["releaseNotes"] = l["release"]["changelog"]
    links = b["entry"].get("links", [])
    same = [x["url"] for x in links if x.get("kind") == "same"]
    if spec.get("link") and not spec["link"].startswith(base_url) and spec["link"] not in same:
        same.append(spec["link"])   # the bundle's own link, which the description says the mod is "also on"
    if same:
        app["sameAs"] = same
    videos = [{"@type": "VideoObject", "name": x["label"], "url": x["url"]} for x in links if x.get("kind") == "video"]
    if videos:
        app["subjectOf"] = videos
    crumbs = {"@type": "BreadcrumbList", "itemListElement": [
        {"@type": "ListItem", "position": 1, "name": site_title, "item": base_url},
        {"@type": "ListItem", "position": 2, "name": spec["name"], "item": url}]}
    return {"@context": "https://schema.org", "@graph": [app, crumbs]}


def published_of(spec):
    return f"{spec['date']}T12:00:00Z"


def sitemap(base_url, entries):
    """The landing page and every mod's page, each last modified on its newest release (the landing page on the
    newest of all), as the sitemaps.org protocol has it. It lists only pages under base_url, which a sitemap there
    may hold."""
    x = lambda t: html.escape(t, quote=False)
    pages = [(f"{base_url}{l['id']}/", b["spec"]["date"]) for b, l, *_ in entries]
    rows = [(base_url, max((d for _, d in pages), default=None))] + pages
    out = ['<?xml version="1.0" encoding="UTF-8"?>', '<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">']
    for loc, when in rows:
        out.append(f"<url><loc>{x(loc)}</loc>" + (f"<lastmod>{when}</lastmod>" if when else "") + "</url>")
    return "\n".join(out + ["</urlset>"]) + "\n"


def atom(src, base_url, entries):
    """Every release of every mod, newest first, as an Atom feed: updates for a reader that is not a mod manager."""
    x = lambda t: html.escape(t or "", quote=True)
    items = sorted(((r.get("publishedAt") or "", l, r) for _, l, _, _, _, history in entries for r in history),
                   key=lambda t: (t[0], t[1]["id"], t[2]["id"]), reverse=True)
    title = (src.get("page") or {}).get("title") or src["name"]
    out = ['<?xml version="1.0" encoding="utf-8"?>', '<feed xmlns="http://www.w3.org/2005/Atom">',
           f"<title>{x(title)}</title>", f'<link href="{x(base_url)}"/>', f'<link rel="self" href="{x(base_url)}feed.xml"/>',
           f"<id>{x(base_url)}</id>", f"<updated>{x(items[0][0] if items else '1970-01-01T00:00:00Z')}</updated>"]
    for when, l, r in items:
        out += ["<entry>", f"<title>{x(l['title'])} {x(r['version'])}</title>", f'<link href="{x(base_url + l["id"])}/"/>',
                f"<id>{x(base_url + l['id'])}/#{x(r['id'])}</id>", f"<updated>{x(when)}</updated>",
                f"<author><name>{x(l['author'])}</name></author>", f'<content type="text">{x(r.get("changelog"))}</content>', "</entry>"]
    return "\n".join(out + ["</feed>"]) + "\n"


if __name__ == "__main__":
    main()

"""Re-dress a UI scene map (maps/ui/...vpk): rewrite its entity lump, keep every other file of the map byte for byte.

  python -I scene_lump.py <stock map.vpk> <spec.json> <out map.vpk> [--work DIR] [--extra DIR]

UI scenes are separate map VPKs (docs/reference/engine.md, Hero page). Their lights are dynamic (`directlight "2"`,
client-side), so retinting, moving and adding lights, light cookies (gobos), particles and props needs no
lighting bake: only `<map>/entities/default_ents.vents_c` changes. That file is a resource with an edit-info
block (RED2, copied as is) and a DATA block holding KeyValues3: `m_entityKeyValues[]`, each entity's keys in
`keyValues3Data.values`. This script reads DATA through `magician dump` (the native KV3 decoder), applies the
spec, and writes DATA back as binary KV3 v5, uncompressed (the layout of Magician's own `kv3::encode`), then
proves the write by decoding it again with `magician dump` and comparing value for value.

spec.json:
  {"set":    [{"match": {"targetname": "[PR#]Key Light", "hammerUniqueId": "4"}, "values": {"color": [255, 200, 140]}}],
   "add":    [{"like": {"hammerUniqueId": "4"}, "values": {"origin": "0 0 0", "hammerUniqueId": "9001", ...}}],
   "remove": [{"hammerUniqueId": "18"}]}
An entry may also carry "connections": [{"output", "target", "input", "param", "delay", "times"}] (the entity's
I/O list, replaced). --extra DIR copies more files into the map at their relative paths (models, particles).
A match is every given key equal (as text). "like" copies an existing entity's keys, then applies "values".
Numbers keep their KV3 kind: a JSON int is an int, a JSON float a double; lists of numbers are arrays.
"""
import json, os, re, shutil, struct, subprocess, sys

HERE = os.path.dirname(os.path.abspath(__file__))
MAGICIAN = os.path.join(HERE, "..", "target", "release", "magician.exe" if os.name == "nt" else "magician")
sys.path.insert(0, os.path.join(HERE, "..", "ingame"))
import gamepaths  # noqa: E402
VRF = gamepaths.vrf()


# ------------------------------------------------------------------------------------------- KV3 text (magician dump)
class Blob(bytes):
    pass


class Obj(list):
    """A KV3 object: a list of (key, value) pairs (kept apart from arrays even when empty)."""


def parse_text(s):
    i = 0
    n = len(s)

    def ws():
        nonlocal i
        while i < n:
            if s[i].isspace():
                i += 1
            elif s.startswith("//", i):
                while i < n and s[i] != "\n":
                    i += 1
            else:
                break

    def value():
        nonlocal i
        ws()
        c = s[i]
        if c == "{":
            i += 1; out = Obj()
            while True:
                ws()
                if s[i] == "}":
                    i += 1; return out
                k = bare() if s[i] != '"' else quoted()
                ws(); assert s[i] == "=", s[i - 40:i + 20]; i += 1
                out.append((k, value()))
        if c == "[":
            i += 1; out = []
            while True:
                ws()
                if s[i] == "]":
                    i += 1; return out
                out.append(value()); ws()
                if s[i] == ",":
                    i += 1
        if c == "#" and s[i + 1] == "[":
            j = s.index("]", i); body = s[i + 2:j]; i = j + 1
            return Blob(bytes(int(x, 16) for x in body.split()))
        if c == '"':
            return quoted()
        t = bare()
        if t == "true":
            return True
        if t == "false":
            return False
        if t == "null":
            return None
        if re.fullmatch(r"-?\d+", t):
            return int(t)
        return float(t)

    def quoted():
        nonlocal i
        i += 1; out = []
        while True:
            c = s[i]; i += 1
            if c == '"':
                return "".join(out)
            if c == "\\":
                d = s[i]; i += 1
                out.append({"n": "\n", "t": "\t"}.get(d, d))
            else:
                out.append(c)

    def bare():
        nonlocal i
        j = i
        while i < n and (s[i].isalnum() or s[i] in "_.-+$/@"):
            i += 1
        return s[j:i]

    ws()
    if s.startswith("<!--", i):
        i = s.index("-->", i) + 3
    return value()


# ------------------------------------------------------------------------------------------- KV3 v5 binary
FORMAT_GENERIC = bytes([0x7c, 0x16, 0x12, 0x74, 0xe9, 0x06, 0x98, 0x46, 0xaf, 0xf2, 0xe6, 0x3e, 0xb5, 0x90, 0x37, 0xe7])
SENTINEL = 0xFFEEDD00


def encode(v):
    st = {"strings": [], "counts": bytearray(), "bytes": bytearray(), "ints": bytearray(), "doubles": bytearray(),
          "types": bytearray(), "objects": 0, "arrays": 0, "members": 0, "blobs": []}

    def sid(x):
        if x == "":
            return -1
        if x not in st["strings"]:
            st["strings"].append(x)
        return st["strings"].index(x)

    def val(x):
        T = st["types"]
        if x is None:
            T.append(1)
        elif isinstance(x, bool):
            T.append(13 if x else 14)
        elif isinstance(x, Blob):
            T.append(7); st["blobs"].append(bytes(x))      # blobs live in blocks after the buffers, as Valve's do
        elif isinstance(x, int):
            if x == 0:
                T.append(15)
            elif x == 1:
                T.append(16)
            else:
                T.append(3); st["doubles"] += struct.pack("<q", x)
        elif isinstance(x, float):
            if x == 0.0:
                T.append(17)
            elif x == 1.0:
                T.append(18)
            else:
                T.append(5); st["doubles"] += struct.pack("<d", x)
        elif isinstance(x, str):
            T.append(6); st["ints"] += struct.pack("<i", sid(x))
        elif isinstance(x, list) and not isinstance(x, Obj):
            st["arrays"] += 1; T.append(8); st["ints"] += struct.pack("<I", len(x))
            for y in x:
                val(y)
        else:                                         # object: list of (key, value)
            st["objects"] += 1; st["members"] += len(x); T.append(9); st["counts"] += struct.pack("<I", len(x))
            for k, y in x:
                st["ints"] += struct.pack("<i", sid(k)); val(y)

    val(v)
    pad = lambda b, k: b.extend(b"\0" * ((-len(b)) % k))
    b0 = bytearray()
    for x in st["strings"]:
        b0 += x.encode("utf-8") + b"\0"
    nb0 = len(b0)
    pad(b0, 4)
    b0 += struct.pack("<I", len(st["strings"]))
    b1 = bytearray(st["counts"]) + st["bytes"]
    ni, nd = len(st["ints"]) // 4, len(st["doubles"]) // 8
    if ni:
        pad(b1, 4)
    b1 += st["ints"]
    if nd:
        pad(b1, 8)
    b1 += st["doubles"] + st["types"]
    for x in st["blobs"]:
        b1 += struct.pack("<I", len(x))
    b1 += struct.pack("<I", SENTINEL)
    blob_data = b"".join(st["blobs"])
    if st["blobs"]:                                # after the blob data, the stream ends on its own end marker
        blob_data += struct.pack("<I", SENTINEL)
    u = lambda x: struct.pack("<I", x)
    o = bytearray(b"\x053VK") + FORMAT_GENERIC
    o += u(0) + struct.pack("<HH", 0, 0)
    o += u(nb0) + u(1) + u(0) + u(len(st["types"])) + struct.pack("<HH", st["objects"], st["arrays"])
    o += u(len(b0) + len(b1)) * 2 + u(len(st["blobs"])) + u(sum(len(x) for x in st["blobs"])) + u(0) + u(0)
    o += u(len(b0)) * 2 + u(len(b1)) * 2
    o += u(len(st["bytes"])) + u(0) + u(ni) + u(nd)
    o += u(st["members"]) + u(st["objects"]) + u(st["arrays"]) + u(0)
    return bytes(o + b0 + b1 + blob_data)


def encode_v4(v):
    """KV3 v4, uncompressed: one buffer (bytes | ints, the string count first and object member counts inline |
    doubles | strings and types | blob sizes | end marker), the layout Valve's v4 files decompress to."""
    st = {"strings": [], "bytes": bytearray(), "ints": bytearray(), "doubles": bytearray(), "types": bytearray(),
          "objects": 0, "arrays": 0, "blobs": []}

    def sid(x):
        if x == "":
            return -1
        if x not in st["strings"]:
            st["strings"].append(x)
        return st["strings"].index(x)

    def val(x):
        T = st["types"]
        if x is None:
            T.append(1)
        elif isinstance(x, bool):
            T.append(13 if x else 14)
        elif isinstance(x, Blob):
            T.append(7); st["blobs"].append(bytes(x))
        elif isinstance(x, int):
            if x == 0:
                T.append(15)
            elif x == 1:
                T.append(16)
            else:
                T.append(3); st["doubles"] += struct.pack("<q", x)
        elif isinstance(x, float):
            if x == 0.0:
                T.append(17)
            elif x == 1.0:
                T.append(18)
            else:
                T.append(5); st["doubles"] += struct.pack("<d", x)
        elif isinstance(x, str):
            T.append(6); st["ints"] += struct.pack("<i", sid(x))
        elif isinstance(x, list) and not isinstance(x, Obj):
            st["arrays"] += 1; T.append(8); st["ints"] += struct.pack("<I", len(x))
            for y in x:
                val(y)
        else:
            st["objects"] += 1; T.append(9); st["ints"] += struct.pack("<I", len(x))
            for k, y in x:
                st["ints"] += struct.pack("<i", sid(k)); val(y)

    val(v)
    buf = bytearray(st["bytes"])
    while len(buf) % 4:
        buf.append(0)
    ints = struct.pack("<I", len(st["strings"])) + st["ints"]
    buf += ints
    while len(buf) % 8:
        buf.append(0)
    buf += st["doubles"]
    sat = bytearray()
    for x in st["strings"]:
        sat += x.encode("utf-8") + bytes(1)
    sat += st["types"]
    buf += sat
    for x in st["blobs"]:
        buf += struct.pack("<I", len(x))
    buf += struct.pack("<I", SENTINEL)
    blob_data = b"".join(st["blobs"])
    if st["blobs"]:                                # after the blob data, the stream ends on its own end marker
        blob_data += struct.pack("<I", SENTINEL)
    u = lambda x: struct.pack("<I", x)
    o = bytearray(bytes([4]) + b"3VK") + FORMAT_GENERIC
    o += u(0) + struct.pack("<HH", 0, 0)
    o += u(len(st["bytes"])) + u(len(ints) // 4) + u(len(st["doubles"]) // 8) + u(len(sat))
    o += struct.pack("<HH", st["objects"], st["arrays"])
    o += u(len(buf)) + u(len(buf)) + u(len(st["blobs"])) + u(sum(len(x) for x in st["blobs"])) + u(0) + u(0)
    return bytes(o + buf + blob_data)


# ------------------------------------------------------------------------------------------- resource
def blocks(b):
    off, cnt = struct.unpack_from("<II", b, 8)
    tab = 8 + off
    out = []
    for k in range(cnt):
        e = tab + 12 * k
        tag = b[e:e + 4]; rel, size = struct.unpack_from("<II", b, e + 4)
        out.append((tag, b[e + 4 + rel:e + 4 + rel + size]))
    return out


def build(orig, new_data):
    hv, ver = struct.unpack_from("<HH", orig, 4)
    bl = [(t, (new_data if t == b"DATA" else d)) for t, d in blocks(orig)]
    n = len(bl)
    head = 16 + 12 * n
    body = bytearray(); pos = []
    for t, d in bl:
        while (head + len(body)) % 16:
            body.append(0)
        pos.append(head + len(body)); body += d
    out = bytearray(struct.pack("<IHHII", head + len(body), hv, ver, 8, n))
    for k, (t, d) in enumerate(bl):
        e = 16 + 12 * k
        out += t + struct.pack("<II", pos[k] - (e + 4), len(d))
    return bytes(out + body)


# ------------------------------------------------------------------------------------------- the edit
def get(obj, k):
    return next((v for kk, v in obj if kk == k), None)


def setk(obj, k, v):
    for j, (kk, _) in enumerate(obj):
        if kk == k:
            obj[j] = (k, v); return
    obj.append((k, v))


def conv(v):
    if isinstance(v, list):
        return [conv(x) for x in v]
    return v


def text_of(v):
    if isinstance(v, float):
        return ("%.6f" % v).rstrip("0").rstrip(".")
    return str(v)


def connections(e, conns):
    """Replace an entity's I/O list: [{"output", "target", "input", "param", "delay", "times"}] (UNVERIFIED that a UI
    scene runs entity I/O at all; the hero-page probe settles it)."""
    out = []
    for c in conns:
        o = Obj()
        o += [("m_outputName", c["output"]), ("m_targetType", 7), ("m_targetName", c["target"]),
              ("m_inputName", c["input"]), ("m_overrideParam", c.get("param", "")),
              ("m_flDelay", float(c.get("delay", 0.0))), ("m_nTimesToFire", int(c.get("times", -1))), ("m_paramMap", None)]
        out.append(o)
    setk(e, "m_connections", out)


def matches(vals, m):
    return all(text_of(get(vals, k)) == text_of(x) for k, x in m.items())


def main():
    src, spec_p, out = sys.argv[1:4]
    work = sys.argv[sys.argv.index("--work") + 1] if "--work" in sys.argv else out + ".work"
    spec = json.load(open(spec_p, encoding="utf-8"))
    if os.path.isdir(work):
        shutil.rmtree(work)
    tree = os.path.join(work, "tree")
    os.makedirs(tree)
    r = subprocess.run([VRF, "-i", src, "-o", tree, "-q"], capture_output=True, text=True)
    lumps = [os.path.join(dp, f) for dp, _, fs in os.walk(tree) for f in fs if f == "default_ents.vents_c"]
    assert len(lumps) == 1, ("expected one entity lump", lumps, r.stdout[-500:])
    lump = lumps[0]
    rel = os.path.relpath(lump, tree).replace("\\", "/")
    orig = open(lump, "rb").read()
    txt = subprocess.run([MAGICIAN, "dump", src, rel], capture_output=True, text=True, encoding="utf-8").stdout
    data = parse_text(txt)
    ents = get(data, "m_entityKeyValues")
    vals = lambda e: get(get(e, "keyValues3Data"), "values")
    log = []
    for s in spec.get("set", []):
        hit = [e for e in ents if matches(vals(e), s["match"])]
        assert len(hit) == 1, ("set must match one entity", s["match"], len(hit))
        for k, v in s["values"].items():
            setk(vals(hit[0]), k, conv(v))
        if "connections" in s:
            connections(hit[0], s["connections"])
        log.append("set %s %s" % (s["match"], sorted(s["values"])))
    for s in spec.get("remove", []):
        hit = [e for e in ents if matches(vals(e), s)]
        assert len(hit) == 1, ("remove must match one entity", s, len(hit))
        ents.remove(hit[0]); log.append("remove %s" % s)
    import copy
    for s in spec.get("add", []):
        like = [e for e in ents if matches(vals(e), s["like"])]
        assert len(like) == 1, ("add: like must match one entity", s["like"], len(like))
        e = copy.deepcopy(like[0])
        for k, v in s["values"].items():
            setk(vals(e), k, conv(v))
        for k in s.get("drop", []):
            vv = vals(e); vv[:] = [(kk, x) for kk, x in vv if kk != k]
        if "connections" in s:
            connections(e, s["connections"])
        ents.append(e); log.append("add %s like %s" % (get(vals(e), "classname"), s["like"]))
    new_data = encode_v4(data) if os.environ.get("KV3_V5") is None else encode(data)
    new = build(orig, new_data)
    open(lump, "wb").write(new)
    # proof: Magician decodes what we wrote to exactly the edited value
    probe = os.path.join(work, "probe")
    os.makedirs(os.path.join(probe, os.path.dirname(rel)), exist_ok=True)
    shutil.copyfile(lump, os.path.join(probe, rel))
    pv = os.path.join(work, "probe.vpk")
    subprocess.run([MAGICIAN, "pack", probe, pv], capture_output=True)
    if not os.path.exists(pv):
        raise SystemExit("pack of the probe failed")
    back = parse_text(subprocess.run([MAGICIAN, "dump", pv, rel], capture_output=True, text=True, encoding="utf-8").stdout)
    assert back == data, "re-decoded entity lump differs from what was written"
    if "--extra" in sys.argv:                         # more files for the map (models, particles), at their own paths
        extra = sys.argv[sys.argv.index("--extra") + 1]
        for dp, _, fs in os.walk(extra):
            for f in fs:
                d = os.path.join(tree, os.path.relpath(dp, extra))
                os.makedirs(d, exist_ok=True); shutil.copyfile(os.path.join(dp, f), os.path.join(d, f))
                log.append("extra %s" % os.path.relpath(os.path.join(dp, f), extra).replace("\\", "/"))
    if os.path.exists(out):
        os.remove(out)
    r = subprocess.run([MAGICIAN, "pack", tree, out], capture_output=True, text=True)
    print(r.stdout[-1500:])
    print("SCENE_LUMP", out, "entities", len(ents), "bytes", len(orig), "->", len(new))
    for l in log:
        print("  " + l)


if __name__ == "__main__":
    main()


# ------------------------------------------------------------------------------------------- helpers for specs
def qangle_matrix(p, y, r):
    import math
    p, y, r = (math.radians(v) for v in (p, y, r))
    cz, sz, cy, sy, cx, sx = math.cos(y), math.sin(y), math.cos(p), math.sin(p), math.cos(r), math.sin(r)
    Rz = [[cz, -sz, 0], [sz, cz, 0], [0, 0, 1]]
    Ry = [[cy, 0, sy], [0, 1, 0], [-sy, 0, cy]]
    Rx = [[1, 0, 0], [0, cx, -sx], [0, sx, cx]]
    mm = lambda A, B: [[sum(A[i][k] * B[k][j] for k in range(3)) for j in range(3)] for i in range(3)]
    return mm(mm(Rz, Ry), Rx)


def to_world(prop_pos, prop_ang, p):
    """A point in the prop's (the hero model's) frame, in map coordinates."""
    R = qangle_matrix(*prop_ang)
    return [prop_pos[i] + sum(R[i][k] * p[k] for k in range(3)) for i in range(3)]


def aim(src, dst):
    """QAngle (pitch, yaw, 0) of an entity at src looking along +X at dst (positive pitch looks down)."""
    import math
    d = [dst[i] - src[i] for i in range(3)]
    yaw = math.degrees(math.atan2(d[1], d[0]))
    pitch = -math.degrees(math.atan2(d[2], math.hypot(d[0], d[1])))
    return [round(pitch, 4), round(yaw, 4), 0.0]


def vec_text(v):
    return " ".join("%.6f" % x for x in v)

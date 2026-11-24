"""Minimal glb reader for the defect detectors (numpy only, no Blender).

Reads Source 2 Viewer's glb export of a model: the node tree (bone bind transforms), every mesh primitive's
positions, normals, joints, weights, UVs and indices, and each primitive's material name. Positions come back in
the glb's own space (metres, Y up, as Source 2 Viewer writes them); `to_source()` converts to Source units, Z up.
"""
import json, struct
import numpy as np

_CT = {5120: np.int8, 5121: np.uint8, 5122: np.int16, 5123: np.uint16, 5125: np.uint32, 5126: np.float32}
_NC = {"SCALAR": 1, "VEC2": 2, "VEC3": 3, "VEC4": 4, "MAT4": 16}
INCH = 0.0254


class Glb:
    def __init__(self, path):
        data = open(path, "rb").read()
        magic, ver, length = struct.unpack_from("<III", data, 0)
        assert magic == 0x46546C67, "not a glb: %s" % path
        off, self.bin = 12, b""
        while off < length:
            clen, ctype = struct.unpack_from("<II", data, off)
            chunk = data[off + 8: off + 8 + clen]
            if ctype == 0x4E4F534A:
                self.j = json.loads(chunk)
            elif ctype == 0x004E4942:
                self.bin = chunk
            off += 8 + clen

    def acc(self, i):
        a = self.j["accessors"][i]
        bv = self.j["bufferViews"][a["bufferView"]]
        n, dt = _NC[a["type"]], np.dtype(_CT[a["componentType"]])
        start = bv.get("byteOffset", 0) + a.get("byteOffset", 0)
        stride = bv.get("byteStride", 0) or n * dt.itemsize
        raw = np.frombuffer(self.bin, np.uint8, count=stride * (a["count"] - 1) + n * dt.itemsize, offset=start)
        if stride == n * dt.itemsize:
            out = np.frombuffer(raw.tobytes(), dt).reshape(a["count"], n)
        else:
            out = np.lib.stride_tricks.as_strided(raw, (a["count"], n * dt.itemsize), (stride, 1))
            out = np.frombuffer(out.copy().tobytes(), dt).reshape(a["count"], n)
        if a.get("normalized"):
            out = out.astype(np.float32) / np.iinfo(dt).max
        return out

    # --------------------------------------------------------------------------------------------- nodes
    def node_world(self):
        """World matrix (4x4, glb space) of every node index."""
        nodes = self.j["nodes"]
        parent = {}
        for i, n in enumerate(nodes):
            for c in n.get("children", []):
                parent[c] = i
        local = []
        for n in nodes:
            if "matrix" in n:
                local.append(np.array(n["matrix"], np.float64).reshape(4, 4).T)
                continue
            t = np.array(n.get("translation", [0, 0, 0]), np.float64)
            x, y, z, w = n.get("rotation", [0, 0, 0, 1])
            s = np.array(n.get("scale", [1, 1, 1]), np.float64)
            r = np.array([[1 - 2 * (y * y + z * z), 2 * (x * y - z * w), 2 * (x * z + y * w)],
                          [2 * (x * y + z * w), 1 - 2 * (x * x + z * z), 2 * (y * z - x * w)],
                          [2 * (x * z - y * w), 2 * (y * z + x * w), 1 - 2 * (x * x + y * y)]])
            m = np.eye(4)
            m[:3, :3] = r * s
            m[:3, 3] = t
            local.append(m)
        world = [None] * len(nodes)

        def w(i):
            if world[i] is None:
                world[i] = local[i] if i not in parent else w(parent[i]) @ local[i]
            return world[i]
        for i in range(len(nodes)):
            w(i)
        return world

    def bones(self):
        """name -> 4x4 world bind matrix (glb space) for every joint of every skin."""
        W = self.node_world()
        out = {}
        for sk in self.j.get("skins", []):
            for ji in sk["joints"]:
                out[self.j["nodes"][ji].get("name", str(ji))] = W[ji]
        return out

    # --------------------------------------------------------------------------------------------- meshes
    def primitives(self):
        """Yield dicts: mesh, material, pos (glb space, world at bind), idx (n,3), joints, weights, uv, normal."""
        W = self.node_world()
        mats = [m.get("name", "") for m in self.j.get("materials", [])]
        for ni, n in enumerate(self.j["nodes"]):
            if "mesh" not in n:
                continue
            m = self.j["meshes"][n["mesh"]]
            # skinned meshes are authored in bind space; unskinned ones take their node's world matrix
            M = np.eye(4) if "skin" in n else W[ni]
            for p in m["primitives"]:
                at = p["attributes"]
                pos = self.acc(at["POSITION"]).astype(np.float64)
                pos = pos @ M[:3, :3].T + M[:3, 3]
                idx = self.acc(p["indices"]).reshape(-1, 3) if "indices" in p else np.arange(len(pos)).reshape(-1, 3)
                yield {
                    "mesh": m.get("name", str(n["mesh"])), "node": n.get("name", ""),
                    "material": mats[p["material"]] if "material" in p else "",
                    "pos": pos, "idx": idx.astype(np.int64),
                    "normal": self.acc(at["NORMAL"]).astype(np.float64) if "NORMAL" in at else None,
                    "uv": self.acc(at["TEXCOORD_0"]).astype(np.float64) if "TEXCOORD_0" in at else None,
                    "joints": self.acc(at["JOINTS_0"]) if "JOINTS_0" in at else None,
                    "weights": self.acc(at["WEIGHTS_0"]).astype(np.float64) if "WEIGHTS_0" in at else None,
                    "skin": n.get("skin"),
                }


def to_source(p):
    """glb space (metres, Y up, -Z forward as Source 2 Viewer writes it) -> Source units, Z up."""
    p = np.asarray(p, np.float64) / INCH
    return np.stack([p[..., 0], -p[..., 2], p[..., 1]], -1)

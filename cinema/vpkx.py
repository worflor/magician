"""Read files out of a VPK (v1/v2, single-file or a _dir with numbered archives), stdlib only.

    python -I vpkx.py ls  <pak_dir.vpk> [prefix]
    python -I vpkx.py get <pak_dir.vpk> <inner/path> <out file>

For the cinema kit's acting pak (acting.py): a compiled clip copied from the game or an installed mod to the path of
another clip. What it reads is Valve's (or a mod's) data: it goes into the dev-only pak90, never into a repo.
"""
import os
import struct
import sys


class VpkError(RuntimeError):
    pass


class Vpk:
    def __init__(self, path):
        self.path = path
        with open(path, "rb") as f:
            head = f.read(28)
            magic, version, tree = struct.unpack("<III", head[:12])
            if magic != 0x55AA1234 or version not in (1, 2):
                raise VpkError(f"{path}: not a VPK (magic {magic:#x}, version {version})")
            hsize = 12 if version == 1 else 28
            f.seek(hsize)
            data = f.read(tree)
        self.data_start = hsize + tree
        self.entries = {}
        i = 0

        def cstr():
            nonlocal i
            j = data.index(b"\0", i)
            s = data[i:j].decode("utf-8", "replace")
            i = j + 1
            return s
        while True:
            ext = cstr()
            if not ext:
                break
            while True:
                folder = cstr()
                if not folder:
                    break
                while True:
                    name = cstr()
                    if not name:
                        break
                    crc, pre, arch, off, length, term = struct.unpack("<IHHIIH", data[i:i + 18])
                    i += 18
                    preload = data[i:i + pre]
                    i += pre
                    p = (f"{folder}/" if folder.strip() else "") + name + (f".{ext}" if ext.strip() else "")
                    self.entries[p.lower()] = (crc, arch, off, length, preload)

    def names(self, prefix=""):
        return sorted(p for p in self.entries if p.startswith(prefix.lower()))

    def read(self, inner):
        e = self.entries.get(inner.lower().replace("\\", "/"))
        if e is None:
            raise VpkError(f"{inner} is not in {self.path}")
        crc, arch, off, length, preload = e
        body = b""
        if length:
            if arch == 0x7FFF:
                src, at = self.path, self.data_start + off
            else:
                src, at = self.path.replace("_dir.vpk", f"_{arch:03d}.vpk"), off
            with open(src, "rb") as f:
                f.seek(at)
                body = f.read(length)
        return preload + body


def main(a):
    if len(a) < 2 or a[0] not in ("ls", "get"):
        sys.exit(__doc__)
    v = Vpk(a[1])
    if a[0] == "ls":
        for n in v.names(a[2] if len(a) > 2 else ""):
            print(n)
    else:
        os.makedirs(os.path.dirname(os.path.abspath(a[3])), exist_ok=True)
        with open(a[3], "wb") as f:
            f.write(v.read(a[2]))
        print(a[3])


if __name__ == "__main__":
    main(sys.argv[1:])

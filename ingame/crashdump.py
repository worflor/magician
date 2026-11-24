"""Read a Deadlock minidump with no debugger: exception, faulting module+offset, and a heuristic stack scan.

    python -I crashdump.py <file.mdmp>            # newest dump if given a folder
    python -I crashdump.py --latest               # newest dump under the game's bin and citadel folders

Prints the exception code and address resolved to module+offset, the parameters (for an access violation:
read/write and the target address), and every stack slot of the crashing thread that points into a loaded
module's image (the return addresses of a real walk are among them; most of the rest are stale). Then the
strings the stack points at (resource paths are what tie a crash to a mod file). Zero dependencies.
"""
import glob
import os
import struct
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))  # python -I leaves the script folder off the path
import gamepaths  # noqa: E402

GAME = gamepaths.try_game()
CODES = {0xC0000005: "ACCESS_VIOLATION", 0xC00000FD: "STACK_OVERFLOW", 0x80000003: "BREAKPOINT",
         0xC0000409: "STACK_BUFFER_OVERRUN", 0xE06D7363: "C++ exception", 0xC000001D: "ILLEGAL_INSTRUCTION"}


class Dump:
    def __init__(self, path):
        self.b = open(path, "rb").read()
        if self.b[:4] != b"MDMP":
            raise ValueError("not a minidump")
        n, rva = struct.unpack_from("<II", self.b, 8)
        self.streams = {}
        for i in range(n):
            t, size, r = struct.unpack_from("<III", self.b, rva + 12 * i)
            self.streams.setdefault(t, (r, size))
        self.modules = self._modules()
        self.memory = self._memory()

    def _str(self, rva):
        n = struct.unpack_from("<I", self.b, rva)[0]
        return self.b[rva + 4:rva + 4 + n].decode("utf-16-le", "replace")

    def _modules(self):
        if 4 not in self.streams:
            return []
        rva, _ = self.streams[4]
        n = struct.unpack_from("<I", self.b, rva)[0]
        out = []
        for i in range(n):
            o = rva + 4 + 108 * i
            base, size, _, ts, name = struct.unpack_from("<QIIII", self.b, o)
            out.append((base, size, os.path.basename(self._str(name)), ts))
        return sorted(out)

    def _memory(self):
        """(start, bytes) ranges from MemoryListStream (5) and Memory64ListStream (9)."""
        mem = []
        if 5 in self.streams:
            rva, _ = self.streams[5]
            n = struct.unpack_from("<I", self.b, rva)[0]
            for i in range(n):
                start, size, r = struct.unpack_from("<QII", self.b, rva + 4 + 16 * i)
                mem.append((start, self.b[r:r + size]))
        if 9 in self.streams:
            rva, _ = self.streams[9]
            n, base = struct.unpack_from("<QQ", self.b, rva)
            off = base
            for i in range(n):
                start, size = struct.unpack_from("<QQ", self.b, rva + 16 + 16 * i)
                mem.append((start, self.b[off:off + size]))
                off += size
        return mem

    def read(self, addr, n):
        for start, data in self.memory:
            if start <= addr < start + len(data):
                return data[addr - start:addr - start + n]
        return None

    def where(self, addr):
        for base, size, name, _ in self.modules:
            if base <= addr < base + size:
                return f"{name}+0x{addr - base:x}"
        return None

    def exception(self):
        if 6 not in self.streams:
            return None
        rva, _ = self.streams[6]
        tid = struct.unpack_from("<I", self.b, rva)[0]
        code, flags, _, addr, nparams = struct.unpack_from("<IIQQI", self.b, rva + 8)
        params = struct.unpack_from("<15Q", self.b, rva + 8 + 32)[:nparams]
        ctx_size, ctx_rva = struct.unpack_from("<II", self.b, rva + 8 + 32 + 120)
        rsp = struct.unpack_from("<Q", self.b, ctx_rva + 0x98)[0] if ctx_size >= 0x100 else None
        return tid, code, addr, params, rsp

    def string_at(self, addr, limit=200):
        raw = self.read(addr, limit)
        if not raw:
            return None
        s = raw.split(b"\0", 1)[0]
        if len(s) >= 6 and all(32 <= c < 127 for c in s):
            return s.decode()
        return None


def report(path):
    d = Dump(path)
    print(f"dump: {path}")
    ex = d.exception()
    if not ex:
        print("no exception stream"); return
    tid, code, addr, params, rsp = ex
    print(f"thread {tid}: {CODES.get(code, hex(code))} at {d.where(addr) or hex(addr)}")
    if code == 0xC0000005 and len(params) >= 2:
        print(f"  {['read', 'write', '?', '?', '?', '?', '?', '?', 'execute'][params[0]]} of 0x{params[1]:x}")
    if rsp is None:
        return
    stack = d.read(rsp, 0x4000) or b""
    print(f"stack scan from rsp=0x{rsp:x} ({len(stack)} bytes):")
    seen, strings = 0, []
    for i in range(0, len(stack) - 7, 8):
        v = struct.unpack_from("<Q", stack, i)[0]
        w = d.where(v)
        if w and seen < 40:
            print(f"  [rsp+0x{i:04x}] {w}")
            seen += 1
        s = d.string_at(v)
        if s and s not in strings:
            strings.append(s)
    if strings:
        print("strings the stack points at:")
        for s in strings[:40]:
            print(f"  {s!r}")


if __name__ == "__main__":
    a = sys.argv[1:]
    if not a or a[0] == "--latest":
        dumps = glob.glob(os.path.join(GAME, "bin", "win64", "*.mdmp")) + glob.glob(os.path.join(GAME, "citadel", "*.mdmp"))
        if not dumps:
            sys.exit("no dumps found")
        report(max(dumps, key=os.path.getmtime))
    else:
        report(a[0])

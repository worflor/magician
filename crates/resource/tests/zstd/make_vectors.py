"""Write the zstd test vectors: synthetic inputs, compressed by libzstd through Windows' tar.exe (bsdtar 3.8, libzstd
1.5.7). bsdtar wraps its input in a tar archive, so each vector is the zstd frame of a tar stream and the expected
output is the same archive written uncompressed. No game data.    python -I make_vectors.py"""
import os, random, subprocess, tempfile
HERE = os.path.dirname(os.path.abspath(__file__))
TAR = r"C:\Windows\System32\tar.exe"
rnd = random.Random(6864)
words = "camera orbit dolly track lash marigold sinclair street demo tick frame spec goto cut dissolve fade".split()
inputs = {
    "text": " ".join(rnd.choice(words) for _ in range(12000)).encode(),
    "binary": bytes(rnd.getrandbits(8) if i % 7 else (i // 7) & 255 for i in range(30000)),
    "small": b"abcabcabcabcabc hello hello hello\n" * 3,
    "runs": b"".join(bytes([rnd.randrange(4)]) * rnd.randrange(1, 300) for _ in range(600)),
}
with tempfile.TemporaryDirectory() as tmp:
    for name, data in inputs.items():
        src = os.path.join(tmp, name + ".bin")
        open(src, "wb").write(data)
        os.utime(src, (1795000000, 1795000000))
        plain = os.path.join(tmp, name + ".tar")
        subprocess.run([TAR, "--format", "ustar", "-cf", plain, "-C", tmp, name + ".bin"], check=True)
        for level in (1, 19):
            z = os.path.join(HERE, f"{name}_l{level}.zst")
            subprocess.run([TAR, "--format", "ustar", "--zstd", "--options", f"zstd:compression-level={level}",
                            "-cf", z, "-C", tmp, name + ".bin"], check=True)
            open(os.path.join(HERE, f"{name}_l{level}.out"), "wb").write(open(plain, "rb").read())
            print(name, level, os.path.getsize(z), "->", os.path.getsize(plain))

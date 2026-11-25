#!/usr/bin/env python3
"""Starts the MCP server, lists the tools, calls `rules`, `doctor`, `policy`, `lint`, `grade`, `check`, `patchday` and `ls`,
checks the JSON.

    python mcp/test_server.py [path\to\small\pakNN_dir.vpk]

Without a pak (argument or MAGICIAN_TEST_PAK) it packs a one-file pak of its own with `magician pack`. Linting needs
the game installed; without it the lint, grade, check and patchday checks are skipped and the rest still runs. Standard library only. Exit 0 when every check passes.
"""
import json
import os
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
EXPECTED = {"rules", "lint", "mesh", "cloth", "grade", "release", "build", "install_status", "refs", "ls", "dump", "verify",
            "drift", "patchday", "policy", "check", "doctor"}
FORBIDDEN = {"install", "uninstall", "launch"}

failures = []


def check(cond, what):
    print(("ok    " if cond else "FAIL  ") + what)
    if not cond:
        failures.append(what)


class Client:
    def __init__(self):
        self.p = subprocess.Popen([sys.executable, "-I", os.path.join(HERE, "server.py")], stdin=subprocess.PIPE,
                                  stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, encoding="utf-8")
        self.n = 0

    def send(self, method, params=None, notify=False):
        msg = {"jsonrpc": "2.0", "method": method}
        if params is not None:
            msg["params"] = params
        if not notify:
            self.n += 1
            msg["id"] = self.n
        self.p.stdin.write(json.dumps(msg) + "\n")
        self.p.stdin.flush()
        if notify:
            return None
        resp = json.loads(self.p.stdout.readline())
        assert resp.get("id") == self.n, resp
        return resp

    def call(self, name, args):
        return self.send("tools/call", {"name": name, "arguments": args})

    def close(self):
        self.p.stdin.close()
        self.p.wait(timeout=10)


def binary():
    return os.environ.get("MAGICIAN_BIN") or os.path.join(HERE, "..", "target", "release", "magician.exe")


def game_found():
    """Does the CLI find the game? (`magician paths` prints `game<TAB>-` when it does not)"""
    try:
        out = subprocess.run([binary(), "paths"], capture_output=True, text=True, timeout=60).stdout
    except OSError:
        return False
    return not any(l.startswith("game\t-") for l in out.splitlines())


def synthetic_pak():
    """A one-file pak packed by the release binary, or None when it cannot be built."""
    exe = binary()
    d = tempfile.mkdtemp(prefix="magician-mcp-test-")
    src = os.path.join(d, "src", "materials", "magician_test")
    os.makedirs(src)
    with open(os.path.join(src, "readme.txt"), "w") as f:
        f.write("synthetic file for mcp/test_server.py\n")
    pak = os.path.join(d, "pak99_dir.vpk")
    try:
        subprocess.run([exe, "pack", os.path.join(d, "src"), pak], capture_output=True, timeout=300)
    except OSError:
        return None
    return pak if os.path.isfile(pak) else None


def main():
    pak = sys.argv[1] if len(sys.argv) > 1 else os.environ.get("MAGICIAN_TEST_PAK") or synthetic_pak() or ""
    c = Client()
    r = c.send("initialize", {"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "test", "version": "0"}})
    check(r["result"]["serverInfo"]["name"] == "magician" and "tools" in r["result"]["capabilities"], "initialize")
    c.send("notifications/initialized", notify=True)
    check(c.send("ping")["result"] == {}, "ping")

    tools = c.send("tools/list")["result"]["tools"]
    names = {t["name"] for t in tools}
    check(names == EXPECTED, "tools/list has exactly %s" % sorted(EXPECTED))
    check(not (names & FORBIDDEN), "no install/uninstall/launch tool is exposed")
    check(all(t["inputSchema"]["type"] == "object" and t["description"] for t in tools), "every tool has a schema and description")

    r = c.call("rules", {})["result"]
    d = r["structuredContent"]
    check(r["isError"] is False and d["count"] > 50 and len(d["rules"]) == d["count"], "rules: %d rules" % d["count"])
    check(d["schema"] == "magician.rules/1" and set(d["rules"][0]) == {"code", "name", "severity", "in_game"}, "rules: schema and object shape")
    check(any(x["code"] == "M129" and x["severity"] == "error" for x in d["rules"]), "rules: M129 is an error")
    check(json.loads(r["content"][0]["text"]) == d, "rules: text content equals structuredContent")

    if os.path.isfile(pak) and game_found():
        r = c.call("lint", {"vpk": pak})["result"]
        d = r["structuredContent"]
        check(r["isError"] is False, "lint: ran")
        check({"vpk", "build", "depth", "counts", "findings", "stale_allowances", "ok", "run"} <= set(d), "lint: top-level keys")
        check(set(d["counts"]) == {"errors", "warnings", "notes", "allowed", "stale"}, "lint: counts keys")
        check(d["depth"] == "light" and isinstance(d["build"], str) and os.path.isfile(d["run"]["log"]), "lint: depth, build, log path")
        check(d["schema"] == "magician.lint/1" and d["run"]["meaning"] in ("clean", "broken"), "lint: schema and exit meaning")
        check(isinstance(d["findings"], list) and all({"code", "name", "severity", "path", "message", "allowed"} <= set(f) for f in d["findings"]),
              "lint: findings are objects (%d)" % len(d["findings"]))
        check(d["ok"] == (d["counts"]["errors"] == 0), "lint: ok matches error count")
        r = c.call("lint", {"vpk": pak, "deep": True})["result"]
        check(r["structuredContent"]["depth"] == "deep", "lint deep: depth deep")
        # the grade record of a pak that touches nothing of the hero: not graded, said so, never a made-up description
        r = c.call("grade", {"vpk": pak, "hero": "lash"})["result"]
        d = r["structuredContent"]
        if r["isError"] and ("Source2Viewer" in d.get("error", "") or "decompile" in d.get("error", "")):
            print("skip  grade: the Source 2 Viewer CLI is not set up (magician paths)")
        else:
            check(r["isError"] is False and d.get("schema") == "magician.grade/3", "grade: the record (%s)" % d.get("error", "ok"))
            check(d.get("label", "").startswith("not graded"), "grade: a pak with nothing of the hero is not graded (%s)" % d.get("label"))
            check({"broken", "description", "identity", "fair_play", "freshness", "rights", "release_gate", "policy", "label"} <= set(d) and "depth" not in d, "grade: record keys")
            check(d.get("release_gate", {}).get("pass") is False and d.get("ok") is False, "grade: the gate blocks an unproven, unreviewed mod")
        # check on a mod folder whose mod.toml names the pak: one verdict, the steps behind it
        m = tempfile.mkdtemp(prefix="magician-mcp-mod-")
        with open(os.path.join(m, "mod.toml"), "w") as f:
            f.write('[mod]\nslug = "mcp-probe"\npaks = ["%s"]\n' % os.path.abspath(pak).replace("\\", "/"))
        r = c.call("check", {"mod": m, "light": True})["result"]
        d = r["structuredContent"]
        check(r["isError"] is False and d.get("schema") == "magician.check/1" and d.get("verdict") in ("ok", "re-prove", "broken"),
              "check: one verdict (%s)" % d.get("verdict", d.get("error")))
        check([x["step"] for x in d.get("steps", [])][:2] == ["lint", "grade"] and d.get("ok") == (d.get("verdict") == "ok"), "check: the steps behind it")
        r = c.call("patchday", {})["result"]
        d = r["structuredContent"]
        check(r["isError"] is False and d.get("schema") == "magician.patchday/1" and d["verdict"]["state"] in ("clear", "re-prove", "broken"),
              "patchday: the report (%s)" % d.get("verdict", d.get("error")))
    elif os.path.isfile(pak):
        print("skip  lint: Deadlock not found (magician paths)")
    if os.path.isfile(pak):
        r = c.call("ls", {"vpk": pak})["result"]["structuredContent"]
        check(r["total"] >= 1 and {"path", "crc", "size"} == set(r["files"][0]), "ls: files")
    else:
        print("skip  lint/ls: no pak (%s)" % (pak or "magician pack failed"))

    r = c.call("doctor", {})["result"]
    d = r["structuredContent"]
    check(d.get("schema") == "magician.doctor/1" and isinstance(d.get("checks"), list) and d["checks"]
          and all({"name", "state", "detail"} <= set(x) for x in d["checks"]) and d.get("ok") == (d.get("failing") == 0),
          "doctor: one check per part (%d)" % len(d.get("checks") or []))

    r = c.call("policy", {})["result"]
    d = r["structuredContent"]
    check(r["isError"] is False and len(d["fingerprint"] or "") == 64 and len(d["entries"]) > 50, "policy: %d values and a fingerprint" % len(d["entries"]))
    check(any(e["key"] == "fair_play.hitbox_egregious_units" and e["value"] == "18" for e in d["entries"]), "policy: the M129 line")
    bad = os.path.join(tempfile.mkdtemp(prefix="magician-mcp-policy-"), "bad.toml")
    with open(bad, "w") as f:
        f.write("[fair_play]\nhitbox_units = 3\n")
    r = c.call("policy", {"policy": bad})["result"]
    check(r["isError"] is True and "unknown key fair_play.hitbox_units" in r["structuredContent"]["error"], "policy: a bad file is an error naming the key")

    r = c.call("lint", {"vpk": r"C:\no\such.vpk"})["result"]
    check(r["isError"] is True and "no such file" in r["structuredContent"]["error"], "lint: missing file is an error result")
    r = c.call("lint", {"vpk": "--deep"})["result"]
    check(r["isError"] is True, "flag-looking argument refused")
    r = c.send("tools/call", {"name": "install", "arguments": {}})
    check("error" in r and r["error"]["code"] == -32602, "unknown tool 'install' is a protocol error")
    check(c.send("nope")["error"]["code"] == -32601, "unknown method")

    # requests that make no sense are error replies, and the server keeps serving after each
    def raw(line):
        c.p.stdin.buffer.write(line if isinstance(line, bytes) else line.encode("utf-8"))
        c.p.stdin.buffer.write(b"\n")
        c.p.stdin.flush()
        return json.loads(c.p.stdout.readline())
    for line, code, what in [
        ('{"jsonrpc":"2.0","id":91,"method":"tools/call","params":"x"}', -32602, "params that are not an object"),
        ('{"jsonrpc":"2.0","id":92,"method":"tools/call","params":[1]}', -32602, "params that are a list"),
        ('{"jsonrpc":"2.0","id":93,"method":"tools/call","params":{"name":[]}}', -32602, "a tool name that is a list"),
        ('{"jsonrpc":"2.0","id":94,"method":"tools/call","params":{"name":{"a":1}}}', -32602, "a tool name that is an object"),
        (b"\xff\xfe not utf-8 at all", -32700, "a line that is not UTF-8"),
        ("[]", -32600, "an empty batch"),
    ]:
        r = raw(line)
        check(r.get("error", {}).get("code") == code, "%s: error %s (%s)" % (what, code, r.get("error")))
    check(c.send("ping")["result"] == {}, "the server is still serving")
    for t in ["inf", 1e999, -5, True, 10 ** 12]:
        r = c.call("rules", {"timeout": t})["result"]
        check(r["isError"] is True and "timeout must be" in r["structuredContent"]["error"], "timeout %r is refused before anything runs" % (t,))
    r = c.call("mesh", {"vpk": os.path.abspath(__file__), "model": "m.vmdl_c", "with": "abc"})["result"]
    check(r["isError"] is True and "a b c" not in json.dumps(r), "mesh: `with` is a list of strings, not characters")
    c.close()

    # a relative path means the server's working folder, not the repository the binary runs in
    if os.path.isfile(pak):
        here = os.getcwd()
        os.chdir(os.path.dirname(os.path.abspath(pak)))
        try:
            rel = Client()
            rel.send("initialize", {"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "t", "version": "0"}})
            r = rel.call("ls", {"vpk": os.path.basename(pak)})["result"]
            check(r["isError"] is False and r["structuredContent"]["total"] >= 1, "ls: a path relative to the server's folder is that file (%s)" % r["structuredContent"].get("error", "ok"))
            rel.close()
        finally:
            os.chdir(here)
    print("\n%s" % ("ALL PASSED" if not failures else "%d FAILED" % len(failures)))
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())

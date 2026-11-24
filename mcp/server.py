#!/usr/bin/env python3
"""Magician MCP server: the Deadlock mod toolkit as structured tools over stdio.

Python standard library only. Speaks MCP (JSON-RPC 2.0, one JSON message per line on stdin/stdout):
initialize, ping, tools/list, tools/call. Every tool runs the magician release binary as a subprocess with a
timeout; output goes to a log file whose path is returned, so nothing blocks forever and nothing is lost.

Safety: install, uninstall and anything that launches the game are not exposed. Only the sub-commands in
ALLOWED_SUBCOMMANDS can be run at all.

Commands with a JSON form (lint, grade, release, drift, patchday, rules, check, doctor) run with --json, and their one JSON document
is read from stdout; nothing here parses their human text.
"""
import json
import os
import re
import subprocess
import sys
import tempfile
import time

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(HERE)
LOG_DIR = os.environ.get("MAGICIAN_MCP_LOGS") or os.path.join(HERE, "logs")
SERVER_INFO = {"name": "magician", "version": "0.1.0"}
SUPPORTED_PROTOCOLS = ["2025-06-18", "2025-03-26", "2024-11-05"]

# Never install, uninstall, launch or write into the game from here.
ALLOWED_SUBCOMMANDS = {
    "lint", "mesh", "cloth", "grade", "release", "build", "ls", "refs", "dump", "verify", "rules",
    "drift", "patchday", "policy", "check", "doctor",
}
INSTALL_STATUS_ARGS = ["install", "--status"]  # the one read-only form of `install`

DEFAULT_TIMEOUT = {"lint": 300, "mesh": 300, "cloth": 120, "grade": 600, "release": 1800, "build": 1800,
                   "ls": 60, "refs": 60, "dump": 60, "verify": 600, "rules": 30, "install_status": 30,
                   "drift": 300, "patchday": 600, "policy": 30, "check": 900,
                   "doctor": 60}
# What each exit code means (`magician --help`): 0 clean, 1 broken, 2 usage error, 3 refused on purpose, 4 re-prove.
EXIT_MEANING = {0: "clean", 1: "broken", 2: "usage error", 3: "refused", 4: "re-prove"}
TAIL_LINES = 30


def find_binary():
    env = os.environ.get("MAGICIAN_BIN")
    if env:
        return env
    cands = [os.path.join(REPO, "target", "release", "magician.exe"),
             os.path.join(REPO, "target", "mcp-build", "release", "magician.exe"),
             os.path.join(REPO, "target", "release", "magician")]
    cands = [c for c in cands if os.path.isfile(c)]
    if not cands:
        return os.path.join(REPO, "target", "release", "magician.exe")
    return max(cands, key=os.path.getmtime)  # the newest build wins


class ToolError(Exception):
    pass


def _kill_tree(proc):
    try:
        if os.name == "nt":
            subprocess.run(["taskkill", "/T", "/F", "/PID", str(proc.pid)], capture_output=True, timeout=15)
        else:
            proc.kill()
    except Exception:
        pass
    try:
        proc.kill()
    except Exception:
        pass


def run_magician(tool, args, timeout=None, json_out=False):
    """Run the binary; return dict(exit_code, timed_out, seconds, log, tail, text, json).

    With json_out the command's stdout (its one JSON document) goes to a file of its own beside the log and is parsed
    into `json`; the log keeps stderr. Otherwise stdout and stderr share the log and `text`."""
    if not args or (args[0] not in ALLOWED_SUBCOMMANDS and args[:2] != INSTALL_STATUS_ARGS):
        raise ToolError("sub-command not allowed through MCP: %r" % (args[:1],))
    exe = find_binary()
    if not os.path.isfile(exe):
        raise ToolError("magician binary not found at %s (build it: cargo build --release, or set MAGICIAN_BIN)" % exe)
    timeout = check_timeout(timeout) or DEFAULT_TIMEOUT.get(tool, 300)
    os.makedirs(LOG_DIR, exist_ok=True)
    log = os.path.join(LOG_DIR, "%s-%s-%d.log" % (time.strftime("%Y%m%d-%H%M%S"), tool, os.getpid()))
    out_path = log[:-len(".log")] + ".json"
    t0 = time.time()
    timed_out = False
    with open(log, "wb") as fh, open(out_path if json_out else os.devnull, "wb") as out:
        fh.write(("$ %s %s\n" % (exe, " ".join(args))).encode("utf-8", "replace"))
        fh.flush()
        proc = subprocess.Popen([exe] + args, stdin=subprocess.DEVNULL, stdout=out if json_out else fh,
                                stderr=fh if json_out else subprocess.STDOUT, cwd=REPO)
        try:
            code = proc.wait(timeout=timeout)
        except subprocess.TimeoutExpired:
            timed_out = True
            _kill_tree(proc)
            try:
                proc.wait(timeout=15)
            except Exception:
                pass
            code = None
            fh.write(("\n[mcp] timed out after %.0f s and was killed\n" % timeout).encode())
    with open(log, "rb") as fh:
        text = fh.read().decode("utf-8", "replace")
    lines = text.splitlines()[1:]  # drop the command echo
    data = read_json_file(out_path) if json_out else None
    return {"exit_code": code, "timed_out": timed_out, "seconds": round(time.time() - t0, 2),
            "log": log, "text": "\n".join(lines), "tail": lines[-TAIL_LINES:], "json": data}


def check_timeout(t):
    """A tool's `timeout` argument: seconds, a number from 1 to a day. Anything else is the caller's mistake, said
    before the binary starts (an infinite or negative wait would leave a process behind or kill it at once)."""
    if t is None:
        return None
    if isinstance(t, bool) or not isinstance(t, (int, float)) or not (1 <= t <= 86400):
        raise ToolError("timeout must be a number of seconds from 1 to 86400, got %r" % (t,))
    return float(t)


def tmp_json():
    fd, path = tempfile.mkstemp(prefix="magician-mcp-", suffix=".json")
    os.close(fd)
    return path


def read_json_file(path):
    try:
        with open(path, "r", encoding="utf-8") as fh:
            data = fh.read()
        return json.loads(data) if data.strip() else None
    except Exception:
        return None
    finally:
        try:
            os.remove(path)
        except OSError:
            pass


def need(args, key):
    v = args.get(key)
    if v is None or v == "":
        raise ToolError("missing required argument: %s" % key)
    if not isinstance(v, str):
        raise ToolError("%s must be a string" % key)
    return v


def value(args, key, required=True):
    v = need(args, key) if required else args.get(key)
    if v is None:
        return None
    if not isinstance(v, str):
        raise ToolError("%s must be a string" % key)
    if v.startswith("-"):
        raise ToolError("%s must not start with '-' (looks like a flag)" % key)
    return v


def existing_file(args, key):
    """A file argument, checked and passed on as an absolute path: the binary runs in the repository, not in the
    server's working folder, so a relative path would name another file there."""
    v = value(args, key)
    if not os.path.isfile(v):
        raise ToolError("%s: no such file: %s" % (key, v))
    return os.path.abspath(v)


def run_meta(r):
    return {"exit_code": r["exit_code"], "meaning": EXIT_MEANING.get(r["exit_code"]), "timed_out": r["timed_out"],
            "seconds": r["seconds"], "log": r["log"]}


def json_result(r, what):
    """The command's JSON document with `run` attached, or an error naming the log."""
    fail_unless_ran(r, what)
    data = r["json"]
    if isinstance(data, dict) and data.get("schema") == "magician.error/1":
        raise ToolError("%s stopped (exit %s, %s): %s; log: %s" % (what, r["exit_code"], data.get("kind"), data.get("error"), r["log"]))
    if not isinstance(data, dict) or "schema" not in data:
        raise ToolError("%s gave no JSON document (exit %s): %s; log: %s"
                        % (what, r["exit_code"], "\n".join(r["tail"]) or "no output", r["log"]))
    data["run"] = run_meta(r)
    return data


def strings(a, key):
    """An optional argument that is one string or a list of strings, as a list, each checked like `value`."""
    v = a.get(key)
    if v is None:
        return []
    if isinstance(v, str):
        v = [v]
    if not isinstance(v, list):
        raise ToolError("%s must be a string or a list of strings" % key)
    return [value({key: x}, key) for x in v]


def existing_files(a, key):
    out = strings(a, key)
    for p in out:
        if not os.path.isfile(p):
            raise ToolError("%s: no such file: %s" % (key, p))
    return [os.path.abspath(p) for p in out]


def fail_unless_ran(r, what):
    if r["timed_out"]:
        raise ToolError("%s timed out after %s s; log: %s" % (what, r["seconds"], r["log"]))


# ---------------------------------------------------------------------------------------------- tools

def t_rules(a):
    r = run_meta_call("rules", ["rules", "--json"], a)
    data = parse_stdout_json(r)
    if data is None:
        raise ToolError("rules --json gave no JSON (is the binary older than the --json flag?); log: %s" % r["log"])
    data.update({"count": len(data.get("rules", []))})
    return data


def run_meta_call(tool, argv, a):
    r = run_magician(tool, argv, a.get("timeout"))
    fail_unless_ran(r, tool)
    return r


def parse_stdout_json(r):
    try:
        return json.loads(r["text"])
    except Exception:
        return None


def t_lint(a):
    vpk = existing_file(a, "vpk")
    argv = ["lint", vpk, "--json"]
    if a.get("deep"):
        argv.append("--deep")
    if a.get("allow"):
        argv += ["--allow", existing_file(a, "allow")]
    if a.get("policy"):
        argv += ["--policy", existing_file(a, "policy")]
    data = json_result(run_magician("lint", argv, a.get("timeout"), json_out=True), "lint")
    data["ok"] = data["counts"]["errors"] == 0
    return data


def t_mesh(a):
    vpk = existing_file(a, "vpk")
    model = value(a, "model")
    argv = ["mesh", vpk, model, "--no-png", "--against", value(a, "against", False) or "valve"]
    for w in strings(a, "with"):
        argv += ["--with", w]
    out = tmp_json()
    argv += ["--json-out", out]
    try:
        r = run_magician("mesh", argv, a.get("timeout"))
    except ToolError:
        read_json_file(out)  # removes it
        raise
    data = read_json_file(out)
    fail_unless_ran(r, "mesh")
    if data is None:
        raise ToolError("mesh produced no verdict (exit %s); log: %s\n%s" % (r["exit_code"], r["log"], "\n".join(r["tail"])))
    data["run"] = run_meta(r)
    # the LOD and silhouette tables are text-only today: pass the head of the report through
    head = []
    for line in r["text"].splitlines():
        if line.startswith("verdict:"):
            break
        head.append(line)
    data["report_head"] = head
    return data


ROW = re.compile(r"^(\S.*?)\s{2,}(\S.*?)(?:\s{2,}(\S.*))?$")


def t_cloth(a):
    vpk = existing_file(a, "vpk")
    argv = ["cloth", vpk, value(a, "model"), "--against", value(a, "against", False) or "valve"]
    if a.get("shapes"):
        argv.append("--shapes")
    r = run_magician("cloth", argv, a.get("timeout"))
    fail_unless_ran(r, "cloth")
    lines = r["text"].splitlines()
    summary = []
    for l in lines[1:]:
        if not l.strip():
            if summary:
                break
            continue
        m = ROW.match(l)
        if m and not l.startswith(("ours", " ")):
            summary.append({"field": m.group(1), "ours": m.group(2), "valve": m.group(3)})
    return {"ok": r["exit_code"] == 0, "no_cloth": any("no cloth" in l for l in lines[:4]),
            "summary": summary, "report": r["text"][:60000], "truncated": len(r["text"]) > 60000, "run": run_meta(r)}


def t_grade(a):
    paks = existing_files(a, "vpks") or [existing_file(a, "vpk")]
    argv = ["grade"] + paks + ["--json"]
    if not a.get("hero") and not a.get("mod"):
        raise ToolError("grade needs hero, or mod (a mod.toml whose [mod] hero names it)")
    if a.get("hero"):
        argv += ["--hero", value(a, "hero")]
    if a.get("mod"):
        argv += ["--mod", existing_file(a, "mod")]
    if a.get("light"):
        argv.append("--light")
    if a.get("target"):
        argv += ["--target", value(a, "target")]
    for f in existing_files(a, "allow"):
        argv += ["--allow", f]
    if a.get("policy"):
        argv += ["--policy", existing_file(a, "policy")]
    data = json_result(run_magician("grade", argv, a.get("timeout"), json_out=True), "grade")
    data["ok"] = bool(data.get("release_gate", {}).get("pass"))
    return data


def long_run(tool, argv, a):
    """`build`, which has no JSON form: the summary tail, the lint counts line, and the log."""
    r = run_magician(tool, argv, a.get("timeout"))
    out = {"ok": r["exit_code"] == 0 and not r["timed_out"], "run": run_meta(r), "summary": r["tail"]}
    if r["timed_out"]:
        out["error"] = "timed out after %s s; the process tree was killed. See the log." % r["seconds"]
    m = re.search(r"(\d+) error\(s\), (\d+) warning\(s\), (\d+) note\(s\), (\d+) allowed, (\d+) stale", r["text"])
    if m:
        out["lint"] = dict(zip(("errors", "warnings", "notes", "allowed", "stale"), map(int, m.groups())))
    return out


def t_release(a):
    argv = ["release", value(a, "addon"), "--hero", value(a, "hero"), "--no-wait", "--json"]
    for w in a.get("with") or []:
        argv += ["--with", value({"w": w}, "w")]
    if a.get("out"):
        argv += ["--out", value(a, "out")]
    if a.get("mod"):
        argv += ["--mod", existing_file(a, "mod")]
    if a.get("no_compile"):
        argv.append("--no-compile")
    r = run_magician("release", argv, a.get("timeout"), json_out=True)
    data = json_result(r, "release")
    data["ok"] = r["exit_code"] == 0
    return data


def t_drift(a):
    paks = existing_files(a, "vpks")
    if not paks:
        raise ToolError("missing required argument: vpks")
    argv = ["drift"] + paks + ["--from", value(a, "from"), "--json"]
    if a.get("to"):
        argv += ["--to", value(a, "to")]
    data = json_result(run_magician("drift", argv, a.get("timeout"), json_out=True), "drift")
    data["ok"] = data.get("state") == "clean"
    return data


def t_patchday(a):
    argv = ["patchday", "--json"]
    if a.get("capture"):
        argv.append("--capture")
    if a.get("from"):
        argv += ["--from", value(a, "from")]
    for m in existing_files(a, "mods"):
        argv += ["--mod", m]
    data = json_result(run_magician("patchday", argv, a.get("timeout"), json_out=True), "patchday")
    data["ok"] = data.get("verdict", {}).get("state") == "clear"
    return data


def t_check(a):
    """`magician check` on one mod: its folder or mod.toml (the binary runs in the repository, so it is always given)."""
    m = value(a, "mod")
    if not (os.path.isfile(m) or os.path.isfile(os.path.join(m, "mod.toml"))):
        raise ToolError("mod: no mod.toml at %s" % m)
    argv = ["check", os.path.abspath(m), "--json"]
    if a.get("light"):
        argv.append("--light")
    if a.get("policy"):
        argv += ["--policy", existing_file(a, "policy")]
    data = json_result(run_magician("check", argv, a.get("timeout"), json_out=True), "check")
    data["ok"] = data.get("verdict") == "ok"
    return data


def t_doctor(a):
    argv = ["doctor", "--json"]
    if a.get("mod"):
        m = value(a, "mod")
        if not (os.path.isfile(m) or os.path.isfile(os.path.join(m, "mod.toml"))):
            raise ToolError("mod: no mod.toml at %s" % m)
        argv += ["--mod", os.path.abspath(m if os.path.isfile(m) else os.path.join(m, "mod.toml"))]
    data = json_result(run_magician("doctor", argv, a.get("timeout"), json_out=True), "doctor")
    data["ok"] = data.get("failing") == 0
    return data


def t_policy(a):
    argv = ["policy"]
    if a.get("policy"):
        argv += ["--policy", existing_file(a, "policy")]
    r = run_magician("policy", argv, a.get("timeout"))
    fail_unless_ran(r, "policy")
    if r["exit_code"] != 0:
        raise ToolError(r["text"].strip() or "policy failed")
    out = {"file": None, "source": None, "fingerprint": None, "moved": None, "entries": [], "run": run_meta(r)}
    for l in r["text"].splitlines():
        f = l.split("\t")
        if len(f) != 3:
            continue
        if f[0] == "policy":
            out["file"], out["source"] = (None if f[1] == "-" else f[1]), f[2]
        elif f[0] == "fingerprint":
            out["fingerprint"], out["moved"] = f[1], f[2]
        else:
            out["entries"].append({"key": f[0], "value": f[1], "source": f[2]})
    return out


def t_build(a):
    argv = ["build", value(a, "addon"), value(a, "out"), "--no-wait"]
    if a.get("no_compile"):
        argv.append("--no-compile")
    return long_run("build", argv, a)


def t_install_status(a):
    r = run_magician("install_status", INSTALL_STATUS_ARGS, a.get("timeout"))
    fail_unless_ran(r, "install_status")
    out = {"ok": r["exit_code"] == 0, "build": None, "search_path": None, "backup": None, "installed": [], "run": run_meta(r)}
    for l in r["text"].splitlines():
        m = re.match(r"^build (\S+?): (.*)$", l)
        if m:
            out["build"], out["search_path"] = m.group(1), m.group(2)
        m = re.match(r"^backup: (.*)$", l)
        if m:
            out["backup"] = m.group(1)
        m = re.match(r"^\s+(\S+)\s+crc (\w+)\s+for (.*?)\s+(installed|\S.*)$", l)
        if m:
            out["installed"].append({"file": m.group(1), "crc": m.group(2), "mod": None if m.group(3) == "?" else m.group(3), "state": m.group(4)})
    if not out["installed"] and out["build"] is None:
        out["raw"] = r["text"]
    return out


def t_refs(a):
    vpk, path = existing_file(a, "vpk"), value(a, "path")
    r = run_magician("refs", ["refs", vpk, path], a.get("timeout"))
    fail_unless_ran(r, "refs")
    if r["exit_code"] != 0:
        raise ToolError(r["text"].strip() or "refs failed")
    out = {"path": path, "blocks": [], "references": [], "run": run_meta(r)}
    for l in r["text"].splitlines():
        m = re.match(r"^\s+blocks: (.*)$", l)
        if m:
            out["blocks"] = m.group(1).split()
        m = re.match(r"^\s+(ok|--) (\S.*)$", l)
        if m:
            out["references"].append({"path": m.group(2).strip(), "resolves": m.group(1) == "ok"})
    out["unresolved"] = sum(1 for x in out["references"] if not x["resolves"])
    return out


def t_ls(a):
    vpk = existing_file(a, "vpk")
    argv = ["ls", vpk] + ([value(a, "prefix", False)] if a.get("prefix") else [])
    r = run_magician("ls", argv, a.get("timeout"))
    fail_unless_ran(r, "ls")
    if r["exit_code"] != 0:
        raise ToolError(r["text"].strip() or "ls failed")
    files = []
    for l in r["text"].splitlines():
        p = l.split("\t")
        if len(p) == 3:
            files.append({"path": p[0], "crc": p[1], "size": int(p[2])})
    limit = int(a.get("limit") or 500)
    return {"total": len(files), "returned": min(limit, len(files)), "truncated": len(files) > limit,
            "files": files[:limit], "run": run_meta(r)}


def t_dump(a):
    vpk, path = existing_file(a, "vpk"), value(a, "path")
    argv = ["dump", vpk, path]
    if a.get("block"):
        argv += ["--block", value(a, "block")]
    r = run_magician("dump", argv, a.get("timeout"))
    fail_unless_ran(r, "dump")
    if r["exit_code"] != 0:
        raise ToolError(r["text"].strip() or "dump failed")
    limit = int(a.get("max_chars") or 40000)
    text = r["text"]
    return {"path": path, "block": a.get("block") or "DATA", "chars": len(text), "truncated": len(text) > limit,
            "text": text[:limit], "run": run_meta(r)}


def t_verify(a):
    vpk = existing_file(a, "vpk")
    argv = ["verify", vpk]
    if a.get("key"):
        argv += ["--key", str(a["key"])]
    if a.get("summary", True):
        argv.append("--summary")
    r = run_magician("verify", argv, a.get("timeout"))
    fail_unless_ran(r, "verify")
    text = r["text"]
    out = {"ok": r["exit_code"] == 0, "run": run_meta(r)}
    m = re.search(r"(\d+) of (\d+) texture\(s\)", text)
    if m:
        out["textures_checked"] = int(m.group(2))
    m = re.search(r"(\d+) texture\(s\) decoded in ([\d.]+)s, (\d+) carry the mark.*?; (\d+) not decodable", text)
    if m:
        out.update({"decoded": int(m.group(1)), "carry_mark": int(m.group(3)), "not_decodable": int(m.group(4))})
    m = re.search(r"pooled over (\d+).*?z ([\d.]+), p < (\S+)", text) or re.search(r"combined over (\d+) texture\(s\).*?z ([\d.-]+), p < (\S+)", text)
    if m:
        out["pooled"] = {"textures": int(m.group(1)), "z": float(m.group(2)), "p_below": m.group(3)}
    out["report"] = text[:20000]
    return out


# --------------------------------------------------------------------------------------- tool table

TIMEOUT_PROP = {"type": "integer", "description": "Seconds before the command is killed (the log is kept)."}
VPK_PROP = {"type": "string", "description": "Absolute path to a mod .vpk (pakNN_dir.vpk)."}


def schema(props, required):
    return {"type": "object", "properties": props, "required": required, "additionalProperties": False}


TOOLS = [
    ("rules", "The lint rule catalogue: code, name, severity and what each rule costs in game.",
     schema({"timeout": TIMEOUT_PROP}, []), t_rules),
    ("lint", "Lint a mod VPK against the installed Deadlock build (schema magician.lint/1). Returns every finding as an "
             "object (code, name, severity, path, message, allowed, reason) plus counts. deep=true adds the mesh "
             "geometry rules (slow: seconds cold per hero model; default timeout 300 s).",
     schema({"vpk": VPK_PROP, "deep": {"type": "boolean"}, "allow": {"type": "string", "description": "Path to a magician.allow file."},
             "policy": {"type": "string", "description": "A policy file for the fair-play limits."},
             "timeout": TIMEOUT_PROP}, ["vpk"]), t_lint),
    ("mesh", "Full mesh verdict (PASS / ATTENTION / FAIL) for one compiled model in a VPK against Valve's model "
             "at the same path, with every M1xx finding. No images are written.",
     schema({"vpk": VPK_PROP, "model": {"type": "string", "description": "Path in the VPK, e.g. models/heroes_staging/x/x.vmdl_c"},
             "against": {"type": "string", "enum": ["valve", "none"]},
             "with": {"type": "array", "items": {"type": "string"}, "description": "Extra VPKs that hold the model's materials."},
             "timeout": TIMEOUT_PROP}, ["vpk", "model"]), t_mesh),
    ("cloth", "What the compiler made of a model's cloth (compiled FeModel), ours beside Valve's: a parsed summary "
              "table plus the full per-chain report.",
     schema({"vpk": VPK_PROP, "model": {"type": "string"}, "against": {"type": "string", "enum": ["valve", "none"]},
             "shapes": {"type": "boolean"}, "timeout": TIMEOUT_PROP}, ["vpk", "model"]), t_cloth),
    ("grade", "Grade a hero mod (every pak it ships, together) and return the grade record (schema magician.grade/2): "
              "the label (e.g. 'S Echo · stale(6864)'), depth (letter, score, archetype rubric, every criterion, caps), "
              "identity (kind, signals, evidence), fair_play (pass/fail/unknown, checks run, violations, allowances "
              "with value and limit, advisories), freshness, rights, release_gate (blockers, warnings), next_steps and "
              "the policy fingerprint. Lints deep unless light=true (fair play then stays unknown for a model). ok is "
              "true when the release gate passes.",
     schema({"vpk": VPK_PROP, "vpks": {"type": "array", "items": {"type": "string"}, "description": "Every pak of the mod (instead of vpk)."},
             "hero": {"type": "string", "description": "Any way a person names one: lash, Lash, hero_lash, or the model's path."}, "mod": {"type": "string", "description": "The mod's mod.toml (hero, proven builds, rights)."},
             "light": {"type": "boolean"}, "target": {"type": "string", "description": "The build to judge freshness against (default: installed)."},
             "allow": {"type": "array", "items": {"type": "string"}, "description": "magician.allow files."},
             "policy": {"type": "string", "description": "A policy file (default: the built-in lines, or MAGICIAN_POLICY)."},
             "timeout": TIMEOUT_PROP}, []), t_grade),
    ("release", "Long-running. Build every department's addon into one VPK, lint (deep), grade, and write "
                "release-report.txt, lint.json and grade.json beside it. Does NOT install into the game. Returns the "
                "release document (schema magician.release/1) with the lint run and the grade record inside. Default "
                "timeout 1800 s; never waits on another build's lock.",
     schema({"addon": {"type": "string"}, "hero": {"type": "string"},
             "with": {"type": "array", "items": {"type": "string"}, "description": "Extra addons (e.g. an animation addon)."},
             "out": {"type": "string"}, "mod": {"type": "string", "description": "The mod's mod.toml, for the grade."},
             "no_compile": {"type": "boolean"}, "timeout": TIMEOUT_PROP}, ["addon", "hero"]), t_release),
    ("drift", "What a game build changed under mod paks since the build they were proven on (schema magician.drift/1): "
              "per pak its state (clean, re-prove, broken), changed and removed originals, broken references, rig "
              "changes, lost bones and graph parameters, model contract breaches.",
     schema({"vpks": {"type": "array", "items": {"type": "string"}}, "from": {"type": "string", "description": "The build the paks were proven on."},
             "to": {"type": "string"}, "timeout": TIMEOUT_PROP}, ["vpks", "from"]), t_drift),
    ("patchday", "The patch-day check on the installed game (schema magician.patchday/1): baseline, install state, every "
                 "installed mod's drift from its last proven build, the mods' manifests and a verdict (clear, re-prove, "
                 "broken). capture=true writes a missing baseline and rig sidecar into the reference folder, never the "
                 "game.",
     schema({"capture": {"type": "boolean"}, "from": {"type": "string"},
             "mods": {"type": "array", "items": {"type": "string"}, "description": "mod.toml files (default: found above the repository)."},
             "timeout": TIMEOUT_PROP}, []), t_patchday),
    ("check", "Everything Magician can say about one mod on the installed build, as one verdict (schema "
              "magician.check/1): its paks (from its mod.toml: [mod] paks, the paks beside it, or the ones installed "
              "for it) linted and graded together, drift since its last proven build, its scene patches; verdict ok, "
              "re-prove (loads, not proven on this build) or broken, the grade record inside, and next steps. The "
              "first call on a new build may capture its baseline into the reference folder (never the game).",
     schema({"mod": {"type": "string", "description": "The mod's folder or its mod.toml."}, "light": {"type": "boolean"},
             "policy": {"type": "string"}, "timeout": TIMEOUT_PROP}, ["mod"]), t_check),
    ("doctor", "Read-only: is this machine ready (schema magician.doctor/1)? One check per part (config, game, build, "
               "addons mount, baseline, viewer, CSDK, policy, and the mod given), each ok, todo, skip or fail with "
               "the command or config line that fixes it.",
     schema({"mod": {"type": "string", "description": "A mod's folder or mod.toml to check too."}, "timeout": TIMEOUT_PROP}, []), t_doctor),
    ("policy", "Every judgement value in force (fair-play limits, allowance limits, gate, weights, letters, identity, "
               "coverage), where each came from, and the fingerprint a grade records.",
     schema({"policy": {"type": "string"}, "timeout": TIMEOUT_PROP}, []), t_policy),
    ("build", "Long-running. Compile an addon and pack it into one VPK, then lint with the addon's magician.allow. "
              "Does NOT install into the game. Default timeout 1800 s.",
     schema({"addon": {"type": "string"}, "out": {"type": "string", "description": "Output pakNN_dir.vpk path."},
             "no_compile": {"type": "boolean"}, "timeout": TIMEOUT_PROP}, ["addon", "out"]), t_build),
    ("install_status", "Read-only: the game's addons search path, gameinfo backup, and the VPKs Magician installed "
                       "(file, crc, owning mod). Cannot install or uninstall.",
     schema({"timeout": TIMEOUT_PROP}, []), t_install_status),
    ("refs", "A compiled resource's block list and dependencies, each marked as resolving inside the VPK or not.",
     schema({"vpk": VPK_PROP, "path": {"type": "string"}, "timeout": TIMEOUT_PROP}, ["vpk", "path"]), t_refs),
    ("ls", "List the files in a VPK (path, crc, size), optionally under a prefix.",
     schema({"vpk": VPK_PROP, "prefix": {"type": "string"}, "limit": {"type": "integer", "description": "Max files returned (default 500)."},
             "timeout": TIMEOUT_PROP}, ["vpk"]), t_ls),
    ("dump", "A compiled file's KV3 block as text (DATA by default; CTRL, MDAT, PHYS...), or a texture's header.",
     schema({"vpk": VPK_PROP, "path": {"type": "string"}, "block": {"type": "string", "description": "4-letter block tag."},
             "max_chars": {"type": "integer"}, "timeout": TIMEOUT_PROP}, ["vpk", "path"]), t_dump),
    ("verify", "Detect the maker's mark in a VPK's compiled textures (summary mode by default). The key is the author's "
               "name: `key`, else the mod.toml beside the pak ([mark] key, else [mod] author); with neither it refuses.",
     schema({"vpk": VPK_PROP, "key": {"type": "string", "description": "The author's mark key."}, "summary": {"type": "boolean"},
             "timeout": TIMEOUT_PROP}, ["vpk"]), t_verify),
]
TOOL_FUNCS = {name: fn for name, _d, _s, fn in TOOLS}


def tool_list():
    return [{"name": n, "description": d, "inputSchema": s} for n, d, s, _f in TOOLS]


def call_tool(name, args):
    fn = TOOL_FUNCS.get(name)
    if fn is None:
        return None
    if not isinstance(args, dict):
        args = {}
    try:
        data = fn(args)
        return {"content": [{"type": "text", "text": json.dumps(data, indent=2)}], "structuredContent": data, "isError": False}
    except ToolError as e:
        data = {"ok": False, "error": str(e)}
    except Exception as e:  # a bug here must not kill the server
        data = {"ok": False, "error": "%s: %s" % (type(e).__name__, e)}
    return {"content": [{"type": "text", "text": json.dumps(data, indent=2)}], "structuredContent": data, "isError": True}


# ----------------------------------------------------------------------------------------- protocol

def reply(id_, result=None, error=None):
    msg = {"jsonrpc": "2.0", "id": id_}
    if error is not None:
        msg["error"] = error
    else:
        msg["result"] = result
    sys.stdout.write(json.dumps(msg) + "\n")
    sys.stdout.flush()


def handle(msg):
    """One message. A request the server can't make sense of is an error reply, never the end of the server."""
    try:
        dispatch(msg)
    except Exception as e:  # a malformed request, or a bug here: say so and keep serving
        reply(msg.get("id") if isinstance(msg, dict) else None, error={"code": -32603, "message": "internal error: %s: %s" % (type(e).__name__, e)})


def dispatch(msg):
    if not isinstance(msg, dict):
        reply(None, error={"code": -32600, "message": "invalid request"})
        return
    method, id_, params = msg.get("method"), msg.get("id"), msg.get("params")
    is_request = "id" in msg
    if params is None:
        params = {}
    if not isinstance(params, dict):
        if is_request:
            reply(id_, error={"code": -32602, "message": "params must be an object"})
        return
    if method == "initialize":
        want = params.get("protocolVersion")
        reply(id_, {"protocolVersion": want if want in SUPPORTED_PROTOCOLS else SUPPORTED_PROTOCOLS[0],
                    "capabilities": {"tools": {"listChanged": False}}, "serverInfo": SERVER_INFO,
                    "instructions": "Magician toolkit for Deadlock mods. Read-only and build tools; install/uninstall and launching the game are CLI-only."})
    elif method == "ping":
        reply(id_, {})
    elif method == "tools/list":
        reply(id_, {"tools": tool_list()})
    elif method == "tools/call":
        if not isinstance(params.get("name"), str):
            reply(id_, error={"code": -32602, "message": "tools/call needs a tool name (a string)"})
            return
        res = call_tool(params.get("name"), params.get("arguments"))
        if res is None:
            reply(id_, error={"code": -32602, "message": "unknown tool: %s" % params.get("name")})
        else:
            reply(id_, res)
    elif is_request:
        reply(id_, error={"code": -32601, "message": "method not found: %s" % method})
    # notifications (notifications/initialized, cancelled, ...) need no reply


def main():
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        try:
            msg = json.loads(line)
        except ValueError:
            reply(None, error={"code": -32700, "message": "parse error"})
            continue
        if isinstance(msg, list):  # batch
            if not msg:
                reply(None, error={"code": -32600, "message": "invalid request: an empty batch"})
            for m in msg:
                handle(m)
        else:
            handle(msg)


if __name__ == "__main__":
    if hasattr(sys.stdin, "reconfigure"):
        # a byte that is not UTF-8 becomes a parse error for its line, not the end of the server
        sys.stdin.reconfigure(encoding="utf-8", errors="replace")
        sys.stdout.reconfigure(encoding="utf-8")
    main()

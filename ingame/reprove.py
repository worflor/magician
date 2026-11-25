"""Patch day in game, in one command: every mod that needs a re-prove gets its pass, and one report says where to look.

    python -I reprove.py [--mod DIR|mod.toml]... [--out DIR] [--skip-vanilla] [--plan] [--json]
    python -I reprove.py accept <pass dir>

With no --mod it asks `magician patchday --capture --json` which mods must be re-proven on the installed build (run
it from the folder you run patchday from). That is the patch-day check itself: it writes the new build's baseline
when there is none, and when it finds something BROKEN nothing is launched. For each mod, one after another, holding the game seat only while it plays:

  1. a vanilla pass, when this build has no vanilla log for the mod's hero and map yet (mod_session.py qa --vanilla
     --restore: the mod's paks out, the same scenarios, the log kept as the baseline, the paks put back exactly);
  2. the mod's pass (mod_session.py qa: its [ingame] pass list, contact sheets, crash dumps, warnings);
  3. the pass's console warnings against that baseline (logdiff.py).

Passes are kept under <reference>\\work\\passes\\<slug>\\<build>-<date>-<time>\\ (or under --out). The report
(reprove-<build>-<date>-<time>.md and .json beside them) gives each mod one of three states (--json also prints the .json document, alone, on stdout; everything else then
goes to stderr):

    ready    every scenario ran and the mod adds no warning over vanilla on this build
    look     it ran, and it adds warnings or they could not be compared: the report says which
    failed   a scenario failed, the game crashed, or the session could not run

None of the three proves a mod: the pictures are not judged here (docs/PATCH-DAY.md, "Why stills are not compared").
A person or an agent looks at the contact sheets; `look` says what to read first. Then `reprove.py accept <pass dir>`
records it: it checks the pass is a whole one, filmed on the installed build with the paks that are installed now,
runs `magician prove <mod.toml> --evidence <pass dir>` (the proof is recorded against the installed paks' bytes, with
the pass's index.md hashed as its evidence), and leaves an ACCEPTED note in the pass folder.

--plan says what would run (which mods, whether a vanilla pass is needed, which scenarios, where to) and launches
nothing. A mod with no `[ingame] pass` list in its mod.toml, or one naming a scenario that is not found, is reported
as failed before anything is launched for it.

A vanilla pass that does not put the install back stops the whole run: nothing else is launched on an install that
is not as it was.

Exit codes: 0 every mod ready, 4 at least one to look at, 1 at least one failed, 2 usage, 3 the seat is someone else's.
"""
import json
import os
import subprocess
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import gamepaths  # noqa: E402
import logdiff  # noqa: E402
import mod_session as S  # noqa: E402

MARK = "ACCEPTED"


def say(*a):
    print("reprove:", *a, flush=True)


def passes_root(out=None):
    return os.path.abspath(out) if out else gamepaths.work("passes")


def pass_dir(root, slug, build, stamp, vanilla=False):
    return os.path.join(root, slug, f"{build}-{stamp}" + ("-vanilla" if vanilla else ""))


def mods_to_reprove(cwd=None):
    """mod.toml paths of the mods `magician patchday` says must be re-proven on the installed build. The check runs
    with --capture: a build seen for the first time gets its baseline here, as the runbook's first step does."""
    r = subprocess.run([S.magician_exe(), "patchday", "--capture", "--json"], capture_output=True, text=True, cwd=cwd)
    try:
        doc = json.loads(r.stdout)
    except ValueError:
        raise S.Stop(1, "magician patchday --json did not answer: " + (r.stderr or r.stdout).strip()[-300:])
    if doc.get("verdict", {}).get("state") == "broken":
        n = doc.get("verdict", {}).get("problems", "?")
        raise S.Stop(1, f"patchday says BROKEN ({n} problem(s)): run `magician patchday` and fix that first; a pass would film a broken mod")
    want = set(doc.get("verdict", {}).get("reprove", []))
    return [m["path"] for m in doc.get("manifests", []) if m.get("slug") in want and m.get("installed")]


def holder_for(mod, pipeline):
    """Whose name a mod's sessions take the seat under: the pipeline's when the caller named one for the whole run
    (MAGICIAN_GAME_HOLDER), else the mod's own. Each session is told it outright: mod_session.py leaves its holder
    in the environment, and the next mod's sessions would otherwise sit in the seat under the last mod's name."""
    return pipeline or mod["holder"] or mod["slug"]


def classify(f):
    """One mod's findings as (state, reasons). `f`: session_rc, runs [(name, rc)], dumps, base (None | exact bool),
    new_warnings (int | None)."""
    bad = []
    if f.get("session_rc"):
        bad.append(f"the session ended with code {f['session_rc']}")
    bad += [f"scenario {n} failed (code {rc})" for n, rc in f.get("runs", []) if rc]
    if f.get("dumps"):
        bad.append(f"{len(f['dumps'])} crash dump(s)")
    if not f.get("runs") and not bad:
        bad.append("no scenario ran")
    if bad:
        return "failed", bad
    look = []
    if f.get("base") is None:
        look.append("no vanilla baseline for this hero and map: warnings not compared")
    elif not f["base"]:
        look.append("the vanilla baseline is from an older build: what the update changed reads as the mod's")
    if f.get("new_warnings"):
        look.append(f"{f['new_warnings']} warning(s) vanilla does not print")
    return ("look", look) if look else ("ready", [])


def findings(pdir, build, slug):
    """Read a finished pass folder back: what ran, what crashed, what it adds over vanilla."""
    st = S.load(pdir)
    f = {"pass": pdir, "slug": slug, "mod": st.get("mod"), "runs": [(r["name"], r["rc"]) for r in st.get("runs", [])],
         "dumps": [], "base": None, "new_warnings": None, "sheets": [os.path.relpath(s, pdir) for s in S.sheets_list(pdir)]}
    index = os.path.join(pdir, "index.md")
    if os.path.isfile(index):
        text = open(index, encoding="utf-8").read()
        if "## CRASH DUMPS" in text:
            f["dumps"] = [line[2:] for line in text.split("## CRASH DUMPS", 1)[1].split("\n## ")[0].splitlines() if line.startswith("- ")]
    log = os.path.join(pdir, "console_full.log")
    scene = logdiff.scene_name(st.get("hero"), st.get("map"))
    base = logdiff.pick_base(build, scene)
    if base and os.path.isfile(log):
        f["base"] = base[2]
        theirs, ours = logdiff.warnings(base[0]), logdiff.warnings(log)
        f["new"] = sorted(k for k in ours if k not in theirs)
        f["new_warnings"] = len(f["new"])
    return f


def report(build, results, stamp):
    """The run as Markdown: one section a mod, worst first, each ending in the command that records it."""
    order = {"failed": 0, "look": 1, "ready": 2}
    out = [f"# Re-prove on build {build} ({stamp})", "",
           "Nothing here proves a mod: the pictures are not judged. Look at the contact sheets; `look` says what to read first.", ""]
    for r in sorted(results, key=lambda r: (order[r["state"]], r["slug"])):
        out += [f"## {r['slug']}: {r['state']}", ""]
        out += [f"- {x}" for x in r["reasons"]] or ["- every scenario ran and it adds no warning over vanilla on this build"]
        if r.get("pass"):
            out.append(f"- pass: {r['pass']} (index.md, {len(r.get('sheets', []))} contact sheet(s))")
        for k in r.get("new", [])[:12]:
            out.append(f"    new warning: {k[:200]}")
        if r["state"] != "failed" and r.get("pass"):
            out.append(f"- when it has been looked at: `python -I ingame\\reprove.py accept \"{r['pass']}\"`")
        out.append("")
    return "\n".join(out)


def accept(pdir):
    """Record a pass someone has looked at: `magician prove` for its mod, and a note in the pass folder."""
    pdir = os.path.abspath(pdir)
    st = S.load(pdir)
    if not st.get("mod") or not os.path.isfile(st["mod"]):
        raise S.Stop(2, f"{pdir} is not a pass folder (no session.json naming a mod.toml)")
    if st.get("vanilla"):
        raise S.Stop(2, "that is a vanilla pass: it proves nothing about the mod")
    bad = [r["name"] for r in st.get("runs", []) if r["rc"]]
    if bad or not st.get("runs"):
        raise S.Stop(1, "not accepting a pass with failed or no scenarios: " + (", ".join(bad) or "none ran"))
    build = st.get("build") or gamepaths.build()
    if build != gamepaths.build():
        raise S.Stop(1, f"the pass was filmed on build {build}; the installed game is {gamepaths.build()}: film it again")
    now = {(n, c, o) for n, c, o, _ in S.status_rows(S.install_status()) if o in st.get("owners", [])}
    then = {(n, c, o) for n, c, o, _ in S.status_rows(st.get("installed_before", "")) if o in st.get("owners", [])}
    if now != then:
        raise S.Stop(1, "the mod's installed paks are not the ones this pass filmed: " + ", ".join(sorted(f"{n} {c}" for n, c, _ in now ^ then)))
    rc, text = S.magician("prove", st["mod"], "--evidence", pdir)
    say(text.strip().splitlines()[-1] if text.strip() else f"prove rc {rc}")
    if rc != 0:
        raise S.Stop(1, "magician prove failed")
    with open(os.path.join(pdir, MARK), "w", encoding="utf-8", newline="\n") as f:
        f.write(f"build {build}\n" + "".join(f"{n} {c} {o}\n" for n, c, o in sorted(now)))
    say(f"accepted: {st['slug']} on build {build}, from {pdir}")
    return 0


def run(o):
    """The run. With --json, stdout carries the one document and nothing else: everything said on the way (ours, the
    sessions', the tools they start) goes to stderr, at the file-descriptor level so a child process's lines go too."""
    if not o.get("json"):
        return _run(o)
    sys.stdout.flush()
    real = os.dup(1)
    os.dup2(2, 1)
    doc = {}
    try:
        rc = _run(o, doc)
    finally:
        sys.stdout.flush()
        os.dup2(real, 1)
        os.close(real)
    print(json.dumps(doc.get("doc") or {"schema": "magician.reprove/1", "build": gamepaths.build(), "mods": []}, indent=1))
    return rc


def _run(o, keep=None):
    build = gamepaths.build()
    if not build:
        raise S.Stop(1, "the installed game's build could not be read")
    root = passes_root(o.get("out"))
    tomls = [S.find_mod_toml(m) for m in o["mods"]] if o["mods"] else mods_to_reprove()
    if None in tomls:
        raise S.Stop(2, "a --mod has no mod.toml")
    if not tomls:
        say(f"nothing to re-prove on build {build}")
        return 0
    stamp = time.strftime("%Y%m%d-%H%M")
    pipeline = os.environ.get("MAGICIAN_GAME_HOLDER")
    results = []
    for toml in tomls:
        mod = S.read_mod(toml)
        slug = mod["slug"]
        holder = holder_for(mod, pipeline)
        scene = logdiff.scene_name(mod["hero"], mod["map"] or "new_player_basics")
        base = logdiff.pick_base(build, scene)
        need_vanilla = not (base and base[2]) and not o.get("skip-vanilla")
        missing = [i for i in mod["pass"] if not S.resolve_item(i, mod["dir"])]
        why = ("its mod.toml has no [ingame] pass list: nothing to run" if not mod["pass"]
               else "not found: " + ", ".join(missing) if missing else None)
        if o.get("plan"):
            say(f"{slug}: " + (f"WOULD FAIL, {why}" if why else
                               f"{'a vanilla pass, then ' if need_vanilla else ''}{', '.join(mod['pass'])} as {mod['hero']} on "
                               f"{mod['map'] or 'new_player_basics'}, into {pass_dir(root, slug, build, stamp)}"))
            continue
        if why:
            results.append({"slug": slug, "state": "failed", "reasons": [why], "sheets": []})
            say(f"{slug}: failed: {why}")
            continue
        if need_vanilla:
            vdir = pass_dir(root, slug, build, stamp, vanilla=True)
            say(f"{slug}: no vanilla log for {scene} on build {build}: taking one into {vdir}")
            rc = S.main(["qa", "--vanilla", "--restore", "--holder", holder, "--mod", mod["dir"], "--out", vdir])
            if rc == 3:
                raise S.Stop(3, "the game seat is someone else's")
            if rc != 0:
                results.append({"slug": slug, "state": "failed", "pass": vdir, "sheets": [],
                                "reasons": [f"the vanilla pass ended with code {rc}: check `magician install --status` against {vdir}\\installed_before.txt before anything else"]})
                say(f"{slug}: the vanilla pass failed (code {rc}); stopping here")
                break
        pdir = pass_dir(root, slug, build, stamp)
        say(f"{slug}: the mod's pass into {pdir}")
        rc = S.main(["qa", "--holder", holder, "--mod", mod["dir"], "--out", pdir])
        if rc == 3:
            raise S.Stop(3, "the game seat is someone else's")
        f = findings(pdir, build, slug) if os.path.isfile(S.state_path(pdir)) else {"pass": pdir, "slug": slug, "runs": [], "sheets": []}
        f["session_rc"] = rc
        f["state"], f["reasons"] = classify(f)
        results.append(f)
        say(f"{slug}: {f['state']}" + (": " + "; ".join(f["reasons"]) if f["reasons"] else ""))
    if o.get("plan"):
        return 0
    os.makedirs(root, exist_ok=True)
    base_name = os.path.join(root, f"reprove-{build}-{stamp}")
    with open(base_name + ".md", "w", encoding="utf-8", newline="\n") as fh:
        fh.write(report(build, results, stamp) + "\n")
    doc = {"schema": "magician.reprove/1", "build": build, "stamp": stamp, "mods": results}
    with open(base_name + ".json", "w", encoding="utf-8", newline="\n") as fh:
        json.dump(doc, fh, indent=1)
    if keep is not None:
        keep["doc"] = doc
    say(f"report: {base_name}.md")
    states = {r["state"] for r in results}
    return 1 if "failed" in states else 4 if "look" in states else 0


def main(argv):
    try:
        if argv[:1] and argv[0] in ("--help", "-h", "help"):
            raise S.Stop(0, __doc__)
        if argv[:1] == ["accept"]:
            if len(argv) != 2:
                raise S.Stop(2, "accept <pass dir>")
            return accept(argv[1])
        o, i = {"mods": []}, 0
        while i < len(argv):
            a = argv[i]
            if a in ("--mod", "--out"):
                if i + 1 >= len(argv):
                    raise S.Stop(2, f"{a} needs a value")
                if a == "--mod":
                    o["mods"].append(argv[i + 1])
                else:
                    o["out"] = argv[i + 1]
                i += 2
            elif a in ("--skip-vanilla", "--json", "--plan"):
                o[a[2:]] = True
                i += 1
            else:
                raise S.Stop(2, __doc__)
        return run(o)
    except S.Stop as e:
        print(str(e) if str(e) == __doc__ else f"reprove: {e}", flush=True)
        return e.code


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))

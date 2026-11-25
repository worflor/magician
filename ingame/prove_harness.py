"""Prove the in-game harness on this machine in one command: what the system offers, then a real game through every
step a session takes (the seat, launch, the console, a capture, stop, the seat given back), and a report.

    python -I ingame/prove_harness.py [--out DIR] [--holder NAME] [--map new_player_basics | --menu] [--timeout SECS]

Nothing is installed: whatever is in the game's addons folder is what runs. The seat is taken under --holder (default
prove-harness) and handed back however the run ends; a game this run did not start is never touched (launch.py
refuses it). The report is <out>/harness-report.md (default <reference>/work/prove-harness/<date>-<time>/), with the
frames it took. Exit 0 every step passed, 1 a step failed (the report says which and why), 2 this machine lacks
something the harness needs (said before anything starts).

Made for the first run on a Linux desktop or a Steam Deck in desktop mode: everything it does is what
`magician session` does, one step at a time, so a failure names the step.
"""
import os
import platform
import subprocess
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import gameos  # noqa: E402
import gamelock  # noqa: E402
import gamepaths  # noqa: E402


def main(argv):
    o = {"out": None, "holder": "prove-harness", "map": "new_player_basics", "timeout": "900"}
    i = 0
    while i < len(argv):
        a = argv[i]
        if a in ("--help", "-h"):
            print(__doc__)
            return 0
        if a == "--menu":
            o["map"] = None
        elif a[2:] in o and i + 1 < len(argv):
            o[a[2:]] = argv[i + 1]
            i += 1
        else:
            print(f"prove_harness: unknown argument {a!r}", file=sys.stderr)
            return 2
        i += 1
    out = o["out"] or gamepaths.work("prove-harness", time.strftime("%Y%m%d-%H%M%S"))
    os.makedirs(out, exist_ok=True)
    holder = o["holder"]
    os.environ["MAGICIAN_GAME_HOLDER"] = holder
    rows = []

    def step(name, ok, detail):
        rows.append((name, "ok" if ok else "FAILED", detail))
        print(f"{'ok    ' if ok else 'FAILED'} {name}: {detail}", flush=True)
        return ok

    def report(code):
        lines = [f"# Harness proof ({time.strftime('%Y-%m-%d %H:%M')})", "",
                 f"- system: {platform.platform()}, Python {platform.python_version()}",
                 f"- display: {gameos.display().name if gameos.display() else gameos.display_problem()}",
                 f"- DISPLAY={os.environ.get('DISPLAY', '')} WAYLAND_DISPLAY={os.environ.get('WAYLAND_DISPLAY', '')}",
                 f"- holder `{holder}`, map `{o['map'] or 'menu'}`", "", "| Step | Result | Detail |", "|---|---|---|"]
        lines += [f"| {n} | {r} | {d.replace('|', '/')} |" for n, r, d in rows]
        lines += ["", "Exit " + str(code) + ". Look at the frames: a frame of the game, not of the desktop, is the proof."]
        with open(os.path.join(out, "harness-report.md"), "w", encoding="utf-8", newline="\n") as f:
            f.write("\n".join(lines) + "\n")
        print(f"report: {os.path.join(out, 'harness-report.md')}")
        return code

    # 1. what this machine offers, before anything starts
    why = gameos.process_problem() or gameos.display_problem()
    step("system", not why, why or f"processes and {gameos.display().name}")
    try:
        steam = gameos.steam_command()
        step("steam", True, " ".join(steam))
    except gameos.Unsupported as e:
        step("steam", False, str(e))
        why = why or str(e)
    try:
        step("game", True, gamepaths.game())
    except LookupError as e:
        step("game", False, str(e))
        why = why or str(e)
    if why:
        return report(2)

    tool = lambda script, *a, timeout=None: subprocess.run(   # noqa: E731
        [sys.executable, "-I", os.path.join(HERE, script)] + [str(x) for x in a], capture_output=True, text=True, timeout=timeout)
    last = lambda r: ((r.stdout + r.stderr).strip().splitlines() or ["(no output)"])[-1]   # noqa: E731
    code = 1
    try:
        # 2. the seat and the launch (launch.py takes the seat and binds the game it starts)
        extra = ["+map", o["map"]] if o["map"] else []
        t0 = time.time()
        r = tool("launch.py", "--holder", holder, "--timeout", o["timeout"], *(["--extra", " ".join(extra)] if extra else []))
        if not step("launch", r.returncode == 0, f"{last(r)} ({time.time() - t0:.0f} s)"):
            return report(1)
        lock = gamelock.read()
        step("seat", bool(lock and lock.get("holder") == holder and lock.get("game_pid")),
             f"held by {lock and lock.get('holder')}, bound to pid {lock and lock.get('game_pid')}")
        w = gameos.display().find_window()
        step("window", bool(w), gameos.display().describe(w) if w else "no game window found")
        # 3. the console: the probe proves typed keys reach it (an unknown command echoed into console.log)
        if o["map"]:
            import console
            console.wait_log("ChangeGameState: GameInProgress", 300, 0)
            time.sleep(3)
        r = tool("console.py", "--holder", holder, "--out", out, "--script", os.path.join(HERE, "scenarios", "probe.txt"))
        step("console", r.returncode == 0, last(r))
        # 4. a capture of the client area
        shot = os.path.join(out, "frame.png")
        r = tool("capture.py", "shot", shot)
        step("capture", r.returncode == 0 and os.path.isfile(shot), last(r))
        code = 0 if all(x[1] == "ok" for x in rows) else 1
    finally:
        # 5. the stop, and the seat given back, however the run went
        if gamelock.game_running():
            r = tool("stop_game.py", "--holder", holder, timeout=360)
            stopped = r.returncode == 0
            step("stop", stopped, last(r))
            code = code if stopped else 1
        lock = gamelock.read()
        free = lock is None or lock.get("holder") != holder
        if not free:
            free = gamelock.release(holder)
        step("seat back", free, "free" if free else "still held: see gamelock.py status")
    return report(code if free else 1)


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))

"""Launch Deadlock for in-game testing and wait until the main menu is live.

    python -I launch.py                                  # windowed borderless 1920x1080
    python -I launch.py --size 3840x2160                 # a window larger than the monitor (the cinema kit's supersampling; UNVERIFIED)
    python -I launch.py --extra "+map new_player_basics" [--holder NAME] [--wait [SECS]] [--timeout SECS] [--adopt]

The game starts through Steam (`-applaunch 1422450`; gameos.steam_command says which Steam) with -condebug, which
writes game/citadel/console.log for console.py to read back. No -dev: on build 6864 it crashes client.dll (null read)
when hero_testing loads.

One user of the game at a time (gamelock.py): the launch takes the game lock for --holder (default
MAGICIAN_GAME_HOLDER, else "anonymous") and refuses, exit 3, while another holder has it; --wait waits (up to SECS,
default 1800) instead. The game it starts is bound to the seat (its pid and start time), so only this holder types into
or stops it. The lock is cleared by stop_game.py once the game has exited, and is stale when no game is running.

A game already running was started by someone else's launch, with whatever paks were installed then (a dev pak, another
mod's build). Testing in it is invalid, so it is refused (exit 4) unless the caller knowingly adopts it (--adopt), e.g.
its own instance, which binds it.

Exit codes: 0 launched, 1 timed out, 2 this machine can't drive the game (said why), 3 someone else holds the game,
4 a game it did not start is running.
"""
import os
import re
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import gamelock  # noqa: E402
import gameos  # noqa: E402
import gamepaths  # noqa: E402

LOADED = re.compile(r"ChangeGameState|PRE-LOADING|Hideout|Created physics")


def say(msg):
    print(msg, flush=True)


def log_tail(path, size=2 << 20):
    """The end of console.log. Long on purpose: a heroless pawn in the sandbox can print 64 'OnPostPredictionError ...
    teleporting' lines a second (2026-11-21); a short tail lost the loaded line between two polls and the launch timed
    out on a loaded game."""
    try:
        with open(path, "rb") as f:
            f.seek(max(0, os.path.getsize(path) - size))
            return f.read().decode("utf-8", "replace")
    except OSError:
        return ""


def started_game(t0, need_window):
    """The pid of a game that started with this launch (at most 2 s before it), with its window up when asked."""
    win_pid = 0
    if need_window:
        d = gameos.display()
        w = d.find_window() if d else None
        win_pid = d.window_pid(w) if w else 0
    for pid in gameos.game_pids():
        t = gameos.process_started(pid)
        if t is not None and (not t or t >= t0 - 2) and (not need_window or pid == win_pid):
            return pid
    return 0


def parse(argv):
    o = {"extra": "", "timeout": 420.0, "holder": None, "wait": 0.0, "adopt": False, "size": "1920x1080"}
    i = 0
    while i < len(argv):
        a = argv[i]
        if a in ("--help", "-h"):
            print(__doc__)
            sys.exit(0)
        if a == "--adopt":
            o["adopt"] = True
        elif a == "--wait":
            o["wait"] = 1800.0
            if i + 1 < len(argv) and not argv[i + 1].startswith("-"):
                o["wait"] = float(argv[i + 1])
                i += 1
        elif a in ("--extra", "--timeout", "--holder", "--size"):
            if i + 1 >= len(argv):
                sys.exit(f"launch: {a} needs a value")
            o[a[2:]] = argv[i + 1]
            i += 1
        else:
            sys.exit(f"launch: unknown argument {a!r}\n\n{__doc__}")
        i += 1
    o["timeout"] = float(o["timeout"])
    if not re.fullmatch(r"\d+x\d+", o["size"]):
        sys.exit(f"launch: --size wants WIDTHxHEIGHT, got {o['size']!r}")
    return o


def main(argv):
    o = parse(argv)
    holder = gamelock.holder_name(o["holder"])
    os.environ["MAGICIAN_GAME_HOLDER"] = holder
    # refuse before the seat is taken or Steam is asked: a launch that can't be driven or bound is half a session
    why = gameos.process_problem() or gameos.display_problem()
    if why:
        say(f"REFUSED: can't drive the game on this machine: {why}")
        return 2
    try:
        steam = gameos.steam_command()
    except gameos.Unsupported as e:
        say(f"REFUSED: {e}")
        return 2
    if not gamelock.acquire(holder, "launch", o["wait"], launching=True):
        return 3
    try:
        log = gamepaths.console_log()
    except LookupError as e:
        say(f"launch: {e}")
        gamelock.release(holder)
        return 2
    pids = gameos.game_pids()
    if pids:
        if o["adopt"]:
            # knowingly adopting your own instance binds it to your seat; refused if the seat or the binding fails
            if not gamelock.acquire(holder, "launch (adopted)"):
                return 3
            if len(pids) != 1 or not gamelock.bind(holder, pids[0]):
                say(f"game lock: not bound: {len(pids)} game(s) running")
                gamelock.release(holder, leave_running=True)
                return 4
            say("deadlock already running: adopted")
            return 0
        gamelock.release(holder)
        say("REFUSED: a Deadlock instance this launch didn't start is running (it may carry other paks). Run "
            "stop_game.py, check 'magician install --status', then launch again; --adopt only for your own instance.")
        return 4
    w, h = o["size"].lower().split("x")
    args = ["-applaunch", gameos.APP_ID, "-novid", "-console", "-condebug", "-windowed", "-noborder", "-w", w, "-h", h]
    args += [x for x in o["extra"].split(" ") if x]
    t0 = time.time()
    gameos.spawn(steam + args)
    t = 0
    while t < o["timeout"]:
        time.sleep(3)
        t += 3
        # only this launch counts: the previous session's console.log can still hold a matching line
        if os.path.isfile(log) and os.path.getmtime(log) > t0 and LOADED.search(log_tail(log)):
            pid = started_game(t0, need_window=True)
            if pid:
                # bind the game this launch started to the seat: from here on only this holder drives it, and only it
                if not gamelock.bind(holder, pid):
                    say(f"LAUNCHED, but the seat is no longer held by {holder} (not driving it)")
                    return 3
                if not gamelock.acquire(holder, "launched", say=lambda *_: None):
                    return 3
                say(f"LAUNCHED after {t}s (pid {pid})")
                return 0
    # A game this launch started but that never reached the menu is still ours: bind it and keep the seat, so its
    # stop (stop_game.py) is allowed and nobody launches beside it.
    pid = started_game(t0, need_window=False)
    if pid:
        gamelock.bind(holder, pid)
        say(f"TIMEOUT waiting for the game (pid {pid} started: stop it with stop_game.py --holder {holder})")
        return 1
    gamelock.release(holder)
    say("TIMEOUT waiting for the game")
    return 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))

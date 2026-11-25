"""Close Deadlock and wait until it has exited and let go of its files, so the next launch or install never meets a
closing game (Windows holds the addon paks for several seconds after the process ends).

    python -I stop_game.py [--holder NAME] [--console-key KEY] [--no-quit] [--keep-lock] [--force]

One user of the game at a time (gamelock.py): only the holder stops the game, and only the game bound to its seat (the
deadlock.exe launch.py started for it). Anyone else is refused (exit 3, with the reason): never a kill of a game this
caller did not launch. The seat is held through the whole stop and freed only once the process is gone (2026-11-24: a
stop that freed the seat while deadlock.exe was still exiting let the next holder launch, and the first pipeline's
fallback then killed that game). --keep-lock leaves the seat held after the stop, for a caller that still has work to
do on the closed game's files (an uninstall); it releases the seat itself.

First the game is asked to `quit` through its own console (a clean exit; a hard stop can be refused to a sandboxed or
non-elevated caller), and waited for up to 45 s; then the bound process is stopped hard, up to three times.

--force is for a person at the keyboard who has looked at the game: it stops every running game whoever holds the
seat. No script passes it.

Exit codes: 0 stopped (or nothing to stop), 1 still running (the seat stays held), 3 not this holder's game.
"""
import glob
import os
import subprocess
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import gamelock  # noqa: E402
import gameos  # noqa: E402
import gamepaths  # noqa: E402


def say(msg):
    print(msg, flush=True)


def parse(argv):
    o = {"holder": None, "console-key": None, "no-quit": False, "keep-lock": False, "force": False}
    i = 0
    while i < len(argv):
        a = argv[i]
        if a in ("--help", "-h"):
            print(__doc__)
            sys.exit(0)
        if a in ("--no-quit", "--keep-lock", "--force"):
            o[a[2:]] = True
        elif a in ("--holder", "--console-key"):
            if i + 1 >= len(argv):
                sys.exit(f"stop_game: {a} needs a value")
            o[a[2:]] = argv[i + 1]
            i += 1
        else:
            sys.exit(f"stop_game: unknown argument {a!r}\n\n{__doc__}")
        i += 1
    return o


def hold(holder):
    """Renew the seat for the stop, however long the game takes to exit."""
    return gamelock.acquire(holder, "stopping", 0, hold=600, say=lambda *_: None)


def main(argv):
    o = parse(argv)
    holder = gamelock.holder_name(o["holder"])
    os.environ["MAGICIAN_GAME_HOLDER"] = holder
    pids = gameos.game_pids()
    if not pids:
        # nothing to stop: free the seat only if it is ours (never another holder's, never forced)
        if not o["keep-lock"]:
            gamelock.release(holder)
        return 0
    if o["force"]:
        targets = [(p, gameos.process_started(p)) for p in pids]
    else:
        lock = gamelock.read()
        why = gamelock.may_drive(lock, holder, gamelock.running_game(lock, pids))
        if why:
            say(f"refused: {why}")
            say(f"not stopping the game (you are '{holder}')")
            return 3
        targets = [(p, lock.get("game_started")) for p in pids if p == lock.get("game_pid")]
        if not hold(holder):
            say(f"not stopping the game: the seat is no longer '{holder}''s")
            return 3
    if not o["no-quit"]:
        # --no-probe: a closing game may not answer the probe; console.py refuses by itself if the game is not ours
        key = ["--key", o["console-key"]] if o["console-key"] else []   # else console.py reads the player's own bind
        subprocess.run([sys.executable, "-I", os.path.join(HERE, "console.py"), *key, "--holder", holder, "--no-probe",
                        "quit"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        gameos.wait_exit(targets, 45)
    for _ in range(3):   # it can outlive one kill
        left = [(p, s) for p, s in targets if gameos.process_alive(p, s)]
        if not left:
            break
        for p, _s in left:
            gameos.terminate(p)
        gameos.wait_exit(left, 15)
        if not o["force"]:
            hold(holder)
    if any(gameos.process_alive(p, s) for p, s in targets):
        say(f"deadlock.exe is still running: the seat stays held by '{holder}'")
        return 1
    addons = os.path.join(gamepaths.citadel(), "addons")
    deadline = time.time() + 30
    for f in sorted(glob.glob(os.path.join(addons, "pak*_dir.vpk"))):
        while time.time() < deadline and not gameos.file_free(f):
            time.sleep(0.5)
    if not o["keep-lock"]:
        if o["force"]:
            gamelock.release(holder, force=True)
        elif not gamelock.release(holder, say=say):
            say("closed the game, but the seat was not released")
            return 1
    say("closed the game")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))

"""One user of the game at a time.

Several pipelines drive the one retail game (launch, console scenarios, captures). They must not overlap: a second
`map` or `selecthero` mid-scenario wrecks the first one's shots. The lock is a small JSON file naming the holder
(a pipeline, a scenario, a caller):

    <reference>/work/ingame-game.lock   (MAGICIAN_GAME_LOCK overrides; gamepaths.py resolves it as magician does)

Rules:
- The holder is `--holder NAME`, else the MAGICIAN_GAME_HOLDER environment variable, else "anonymous" (all unnamed
  callers share one name, so they do not exclude each other; name your pipeline to be protected from the others).
- The game is bound to the lock: launch.py records the pid (and start time) of the deadlock.exe it started
  (`bind`). Only the holder, and only in that game, may type (console.py) or stop it (stop_game.py): a game the
  holder did not launch, or one left running by another pipeline, is never driven (`may_drive`). Adopting a game you
  started yourself is explicit (launch.py --adopt).
- The lock is stale when its bound game has exited and no deadlock.exe is running, so a closed or crashed game frees
  it. Exceptions: a launch in progress (`launching`, at most LAUNCH_GRACE seconds), a lock taken moments ago
  (FRESH_GRACE), and a hold (`--hold`: a hand-back still working on the closed game's files).
- A holder's release is refused while the game it launched is still running or exiting: the seat is free only once
  the process is gone, so the next holder can never launch beside (or be killed by) a game that is closing.
  `--leave-running` hands the seat back over a live game on purpose; the next holder still cannot drive it.
- `stop_game.py` clears it. A caller that is not the holder is refused, and the lock names who holds it.

    python -I gamelock.py acquire [--holder NAME] [--what TEXT] [--wait SECS] [--launching] [--hold SECS]
    python -I gamelock.py bind [--holder NAME] [--pid N]      (the running deadlock.exe, or that one)
    python -I gamelock.py may-drive [--holder NAME]          (exit 0 when NAME holds the seat and the running game is
                                                              the one bound to it; 3 with the reason otherwise)
    python -I gamelock.py release [--holder NAME] [--leave-running] [--force]
    python -I gamelock.py status            (exit 0 free, 3 held; prints the holder)

Exit codes: 0 ok, 3 held by someone else, the wait ran out, or refused (the reason is printed).
`--force` is for a person at the keyboard who has looked at the game; no script passes it.
"""
import json
import os
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))  # python -I leaves the script folder off the path
import gameos  # noqa: E402
import gamepaths  # noqa: E402

LOCK = gamepaths.game_lock()
LAUNCH_GRACE = 300.0
FRESH_GRACE = 120.0   # a lock taken moments ago is the holder about to launch, not a stale one
DEFAULT_HOLDER = "anonymous"


def game_pids():
    """The pids of every running game (gameos.py: tasklist on Windows, /proc on Linux)."""
    return gameos.game_pids()


def game_running():
    """Is the game running?"""
    return bool(game_pids())


def process_started(pid):
    """When process `pid` was created (Unix time), or None when it is not running (or cannot be asked)."""
    return gameos.process_started(pid)


def process_alive(pid, started=None):
    """Is `pid` the same live process that was created at `started`? (A pid is reused after its process exits: the
    start time tells a new process with an old number from the one the lock was bound to.)"""
    t = process_started(pid)
    if t is None:
        return False
    return started is None or not t or abs(t - started) < 2.0


def bound_alive(lock, alive=None):
    """The game bound to this lock is still running (or still exiting: a process is alive until its last thread ends)."""
    alive = alive or process_alive
    return lock is not None and isinstance(lock.get("game_pid"), int) and alive(lock["game_pid"], lock.get("game_started"))


def holder_name(explicit=None):
    return explicit or os.environ.get("MAGICIAN_GAME_HOLDER") or DEFAULT_HOLDER


# The longest a lock's times may reach ahead of the clock: a start in the future, or a hold past this, is a corrupt
# or hostile file, and would otherwise keep the game held with no game running until someone forces it free.
MAX_AHEAD = 24 * 3600


def read(path=None):
    """The lock, or None for no lock or one that does not read: not JSON, no holder, or times that are not numbers
    (a lock that can't be judged is stale; a pipeline would otherwise crash reading it, or hold the game forever)."""
    try:
        with open(path or LOCK, encoding="utf-8") as f:
            d = json.load(f)
    except (OSError, ValueError):
        return None
    if not isinstance(d, dict) or not isinstance(d.get("holder"), str) or not d["holder"]:
        return None
    now = time.time()
    for k in ("since", "busy_until"):
        v = d.get(k, 0)
        if isinstance(v, bool) or not isinstance(v, (int, float)) or v != v or abs(v) == float("inf") or v > now + MAX_AHEAD:
            return None
    pid = d.get("game_pid")
    if pid is not None and (isinstance(pid, bool) or not isinstance(pid, int) or pid <= 0):
        return None
    st = d.get("game_started")
    if st is not None and (isinstance(st, bool) or not isinstance(st, (int, float)) or st != st or abs(st) == float("inf")):
        return None
    return d


def is_stale(lock, running, now=None, alive=None):
    """No lock content, or no game running (unless a launch is still booting it, or the lock was just taken:
    a pipeline that takes the lock and then launches must not lose it to another pipeline in between).
    A lock whose bound game is still alive is never stale, whatever the process list said a moment ago."""
    if lock is None:
        return True
    if running or bound_alive(lock, alive):
        return False
    now = time.time() if now is None else now
    if now < lock.get("busy_until", 0):   # the holder is still working on the closed game's files (an uninstall)
        return False
    age = now - lock.get("since", 0)
    if age < FRESH_GRACE:
        return False
    return not (lock.get("launching") and age < LAUNCH_GRACE)


def describe(lock):
    ago = int(time.time() - lock.get("since", time.time()))
    return f"{lock['holder']} ({lock.get('what') or 'no scenario'}), for {ago // 60}m{ago % 60:02d}s"


def _write(path, lock, exclusive):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    flags = os.O_WRONLY | os.O_CREAT | (os.O_EXCL if exclusive else os.O_TRUNC)
    fd = os.open(path, flags)
    with os.fdopen(fd, "w", encoding="utf-8") as f:
        json.dump(lock, f)


def acquire(holder, what="", wait=0.0, launching=False, path=None, running=game_running,
            say=print, poll=2.0, hold=0.0, alive=None):
    """True when `holder` now holds the game. False when someone else does (after waiting `wait` seconds).
    `hold`: the lock stays held for that many seconds even with no game running (a hand-back that closes the game
    and then uninstalls a pak keeps the seat until it releases it)."""
    path = path or LOCK
    deadline = time.time() + wait
    said = 0.0
    while True:
        lock = read(path)
        if is_stale(lock, running(), alive=alive):
            if lock is not None:
                say(f"game lock: clearing a stale lock of {describe(lock)} (no deadlock.exe running)")
            try:
                os.remove(path)
            except OSError:
                pass
            mine = {"holder": holder, "what": what, "since": time.time(), "launching": launching, "pid": os.getpid()}   # pid: the taker, not the game
            if hold:
                mine["busy_until"] = time.time() + hold
            try:
                _write(path, mine, exclusive=True)
                return True
            except FileExistsError:
                continue   # someone took it between our remove and write: look again
        elif lock["holder"] == holder:
            lock.update({"what": what or lock.get("what", ""), "launching": launching})
            if hold:
                lock["busy_until"] = time.time() + hold
            _write(path, lock, exclusive=False)
            return True
        if time.time() >= deadline:
            say(f"game lock: the game is in use by {describe(lock)}. Wait for it, or ask them to run stop_game.py "
                f"(you are '{holder}'; set MAGICIAN_GAME_HOLDER to name your pipeline)")
            return False
        if time.time() - said >= 30:
            say(f"game lock: waiting for {describe(lock)}")
            said = time.time()
        time.sleep(poll)


def release_refusal(lock, holder, leave_running=False, force=False, alive=None):
    """Why `holder` may not clear this lock, or None when it may."""
    if lock is None or force:
        return None
    if lock["holder"] != holder:
        return f"held by {describe(lock)}, not by '{holder}'"
    if not leave_running and bound_alive(lock, alive):
        return (f"the game '{holder}' launched (pid {lock['game_pid']}) is still running or exiting; the seat is free "
                f"only once it is gone (stop_game.py waits for that; --leave-running hands it back over a live game)")
    return None


def release(holder, force=False, path=None, leave_running=False, alive=None, say=None):
    """Clear the lock if `holder` holds it and the game it launched has exited (or `leave_running`, or `force`).
    Returns False, and says why, when the lock stays."""
    path = path or LOCK
    lock = read(path)
    why = release_refusal(lock, holder, leave_running, force, alive)
    if why:
        if say:
            say(f"game lock: not released: {why}")
        return False
    if lock is None:
        return True
    try:
        os.remove(path)
    except OSError:
        pass
    return True


def bind(holder, pid, path=None, alive=None, started=None):
    """Record the game `holder` launched (or knowingly adopted) in its lock: from now on only that game is driven.
    False when `holder` does not hold the seat or `pid` is no running process."""
    path = path or LOCK
    started = started or process_started
    lock = read(path)
    if lock is None or lock["holder"] != holder:
        return False
    t = started(pid)
    if t is None:
        return False
    lock.update({"game_pid": pid, "game_started": t, "launching": False})
    _write(path, lock, exclusive=False)
    return True


def running_game(lock, pids=None):
    """The deadlock.exe to judge: the lock's bound game when it is among the running ones, else the first running
    one (0 for none). Two games at once happen (another player's own instance); only the bound one is ever ours."""
    pids = game_pids() if pids is None else pids
    if lock is not None and lock.get("game_pid") in pids:
        return lock["game_pid"]
    return pids[0] if pids else 0


def may_drive(lock, holder, pid, alive=None):
    """Why `holder` may not type into, or stop, the running game `pid` (0: none found), or None when it may: the
    seat must be the holder's, and the game the one bound to the seat at launch."""
    alive = alive or process_alive
    if lock is None:
        return f"nobody holds the game seat; take it with launch.py (or mod_session.py up), which binds the game it starts"
    if lock["holder"] != holder:
        return f"the game is in use by {describe(lock)} (you are '{holder}')"
    if not pid:
        return "no Deadlock window is running"
    if not isinstance(lock.get("game_pid"), int):
        return (f"'{holder}' holds the seat but no game is bound to it: this game (pid {pid}) was not launched by this "
                f"holder. Launch through launch.py, or adopt your own instance with launch.py --adopt")
    if lock["game_pid"] != pid or not alive(pid, lock.get("game_started")):
        return (f"the running game (pid {pid}) is not the one '{holder}' launched (pid {lock['game_pid']}); "
                f"it belongs to someone else, or the seat's game has exited")
    return None


def main(a):
    if a[:1] and a[0] in ("--help", "-h", "help"):
        print(__doc__)
        return 0
    if not a or a[0] not in ("acquire", "release", "status", "bind", "may-drive"):
        sys.exit(__doc__)

    def opt(name, default=None):
        if name not in a:
            return default
        i = a.index(name)
        if i + 1 >= len(a) or a[i + 1].startswith("--"):
            sys.exit(f"{name} needs a value")
        return a[i + 1]

    def seconds(name):
        v = opt(name, "0")
        try:
            x = float(v)
        except ValueError:
            x = -1.0
        if not 0 <= x <= MAX_AHEAD:
            sys.exit(f"{name} wants seconds from 0 to {MAX_AHEAD}, got {v}")
        return x

    holder = holder_name(opt("--holder"))
    if a[0] == "status":
        lock = read()
        if is_stale(lock, game_running()):
            print("free")
            return 0
        print(f"held by {describe(lock)}")
        return 3
    if a[0] == "release":
        return 0 if release(holder, force="--force" in a, leave_running="--leave-running" in a, say=print) else 3
    if a[0] == "bind":
        if opt("--pid") is not None:
            if not opt("--pid").isdigit():
                sys.exit(f"--pid wants a process id, got {opt('--pid')}")
            pid = int(opt("--pid"))
        else:
            pids = game_pids()
            if len(pids) != 1:
                print(f"game lock: not bound: {len(pids)} deadlock.exe running; name one with --pid")
                return 3
            pid = pids[0]
        if bind(holder, pid):
            print(f"game lock: {holder} bound to deadlock.exe pid {pid}")
            return 0
        print(f"game lock: not bound: '{holder}' does not hold the seat, or pid {pid} is not running")
        return 3
    if a[0] == "may-drive":
        lock = read()
        pid = running_game(lock)
        why = may_drive(lock, holder, pid)
        if why:
            print(f"refused: {why}")
            return 3
        print(f"ok: {holder} drives deadlock.exe pid {pid}")
        return 0
    return 0 if acquire(holder, opt("--what", ""), seconds("--wait"), "--launching" in a, hold=seconds("--hold")) else 3


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))

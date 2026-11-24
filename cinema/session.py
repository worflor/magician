"""One cinema session in the retail game: take the seat, install the cinema pak, launch, drive, and always hand back.

    from session import Session
    with Session(out_dir, map="new_player_basics", sourcetv=True) as s:
        s.run("sv_cheats 1", "selecthero hero_lash")       # console lines and console.py directives (@wait, @shot ...)
        s.wait_log("Loaded hero", 60)

    python -I session.py --out <dir> --script <steps.txt> [--map new_player_basics] [--no-tv] [--keep-pak]
    python -I session.py --out <dir> --serve <inbox> [--idle 600]     # run step files as they are dropped in

What the context manager guarantees, however the body ends (an exception, Ctrl+C, a lost console):
  - the game lock is taken as `cinema` before anything (waits for another pipeline, up to `wait_seat` seconds);
  - pak90 is installed only for this session and uninstalled before the seat is handed back (cinema README rule);
  - `exec cinema_reset` runs before the game quits (the camera, HUD, telemetry, fov and frame-rate cvars are
    archived in the user's config), and every key the kit binds is unbound;
  - the game is closed through stop_game.ps1, the lock released, the blind-console state file removed.
The SourceTV take of the session (when `sourcetv`) is `citadel/replays/auto-*-<map>-*.dem`; `take()` names it.
"""
import glob
import os
import subprocess
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
TOOLKIT = os.path.dirname(HERE)
INGAME = os.path.join(TOOLKIT, "ingame")
sys.path.insert(0, INGAME)
sys.path.insert(0, HERE)
import console  # noqa: E402
import gamelock  # noqa: E402
import gamepaths  # noqa: E402

GAME = gamepaths.try_game()
CITADEL = os.path.join(GAME, "citadel")
REPLAYS = os.path.join(CITADEL, "replays")
MAGICIAN = os.path.join(TOOLKIT, "target", "release", "magician.exe")
PAK = os.path.join(os.environ.get("CINEMA_WORK") or gamepaths.work("cinema"), "pak90_dir.vpk")
HOLDER = "cinema"
KEY = console.console_key()   # the player's own console bind (console.py)
# every key the kit may bind during a session (cinema_unbind covers the keypad)
KIT_KEYS = ["SCROLLLOCK"]
MACHINE_CVARS = os.path.join(CITADEL, "cfg", "machine_convars.vcfg")


def user_fps_max():
    """The player's fps_max as saved in machine_convars.vcfg (a paced render lowers it; the session puts it back)."""
    import re
    try:
        m = re.search(r'"fps_max"\s+"([\d.]+)"', open(MACHINE_CVARS, encoding="utf-8", errors="replace").read())
        return f"{float(m.group(1)):g}" if m else "180"
    except OSError:
        return "180"


def ps(script, *args, timeout=None):
    return subprocess.run(["powershell", "-NoProfile", "-ExecutionPolicy", "Bypass", "-File", os.path.join(INGAME, script)]
                          + list(args), capture_output=True, text=True, timeout=timeout)


def game_running():
    return gamelock.game_running()


class SessionError(RuntimeError):
    pass


class Session:
    def __init__(self, out_dir, map="new_player_basics", sourcetv=True, snapshotrate=64, wait_seat=7200,
                 launch_timeout=600, install=True, extra="", say=print, size=(1920, 1080)):
        self.out = out_dir
        self.size = tuple(size)   # the game window (larger than the monitor: supersampling, UNVERIFIED in game)
        self.map = map
        self.sourcetv = sourcetv
        self.snapshotrate = snapshotrate
        self.wait_seat = wait_seat
        self.launch_timeout = launch_timeout
        self.install = install
        self.extra = extra
        self.say = say
        self.con = None
        self.launched_at = 0.0
        self.log_start = 0
        self.installed = False
        self.held = False
        self.fps_max = user_fps_max()
        os.makedirs(out_dir, exist_ok=True)
        os.environ["MAGICIAN_GAME_HOLDER"] = HOLDER

    # ---- set-up ---------------------------------------------------------------------------------------------
    def __enter__(self):
        try:
            self._open()
        except BaseException:
            self._close()
            raise
        return self

    def _open(self):
        self.say(f"session: waiting for the seat (holder {HOLDER}, up to {self.wait_seat}s)")
        if not gamelock.acquire(HOLDER, "cinema session", self.wait_seat, launching=True):
            raise SessionError("the game seat is busy (gamelock): not starting")
        self.held = True
        # A game left running by another pipeline carries whatever paks it was started with: never film in it.
        t = time.time()
        while game_running() and time.time() - t < 300:
            time.sleep(3)
        if game_running():
            raise SessionError("a deadlock.exe this session didn't start is still running")
        if self.install:
            # the acting build may ask for a slot below the mods (acting.py [pak] install_as): pak90 by default
            global PAK_AS
            PAK_AS = install_name()
            r = subprocess.run([MAGICIAN, "install", PAK, "--as", PAK_AS, "--mod", "cinema", "--holder", HOLDER],
                               capture_output=True, text=True)
            self.say("session: " + (r.stdout.strip().splitlines() or ["(no output)"])[-1])
            if r.returncode != 0:
                raise SessionError(f"cinema pak install failed: {r.stdout}{r.stderr}")
            self.installed = True
        extra = []
        if self.sourcetv:
            extra += ["+tv_enable", "1", "+tv_delay", "0", "+tv_transmitall", "1", "+tv_snapshotrate", str(self.snapshotrate)]
        if self.extra:
            extra += self.extra.split()
        if self.map:
            extra += ["+map", self.map]
        self.log_start = console.log_size() if os.path.exists(console.LOG) else 0
        self.launched_at = time.time()
        self.say(f"session: launching ({' '.join(extra)})")
        r = ps("launch.ps1", "-Holder", HOLDER, "-Timeout", str(self.launch_timeout), "-Width", str(self.size[0]),
               "-Height", str(self.size[1]), "-Extra", " ".join(extra))
        self.say("session: " + (r.stdout.strip().splitlines() or ["(no output)"])[-1])
        if r.returncode != 0:
            raise SessionError(f"launch failed (rc {r.returncode}): {r.stdout[-400:]}")
        # the log is rewritten at launch (-condebug): offsets from here on
        self.log_start = 0
        if self.map:
            if not self.wait_log("ChangeGameState: GameInProgress", 300, offset=0):
                raise SessionError("the map never reached GameInProgress")
            time.sleep(6)
        self.con = console.Console(KEY)
        # On the main menu the console answers only once the menu has finished loading (the window comes up first):
        # probe until it does, for up to two minutes.
        deadline = time.time() + (120 if not self.map else 30)
        while True:
            console.release_stuck()
            console.focus_game()
            try:
                self.con.probe()
                break
            except RuntimeError:
                if time.time() > deadline:
                    raise
                self.say("session: the console does not answer yet; waiting")
                time.sleep(10)
        self.say("session: console proven")

    # ---- driving --------------------------------------------------------------------------------------------
    def run(self, *lines):
        """Console lines and console.py directives (@wait, @shot, @log, @key, @burst ...), as a scenario."""
        console.run_script(list(lines), KEY, self.out, probe=False)

    def cmd(self, line, pause=0.3):
        console.ensure_focus()
        self.con.run(line)
        time.sleep(pause)

    def log_offset(self):
        return console.log_size()

    def log_since(self, offset):
        return console.log_since(offset)

    def wait_log(self, text, timeout, offset=None):
        return console.wait_log(text, timeout, self.log_offset() if offset is None else offset)

    def take(self, newest=True):
        """The SourceTV take this session is recording (or recorded): the newest replay since the launch."""
        files = [f for f in glob.glob(os.path.join(REPLAYS, "*.dem")) if os.path.getmtime(f) >= self.launched_at - 5]
        files.sort(key=os.path.getmtime)
        return files[-1] if files else None

    # ---- hand-back ------------------------------------------------------------------------------------------
    def __exit__(self, et, ev, tb):
        self._close()
        return False

    def _close(self):
        """Reset, stop the game, uninstall pak90, verify it is gone, and only then release the seat. The seat is
        held by a heartbeat for the whole hand-back (stop, uninstall, check), however long the stop takes: on
        2026-11-22 a hung `quit` outlasted a fixed 300 s hold, the lock went stale as the game finally exited, a
        waiting session took the seat and installed pak90, and this hand-back's uninstall then removed it (the next
        render filmed the HUD). Nothing here releases the seat while pak90 is still installed."""
        heart = _Heartbeat(self.say) if self.held else None
        try:
            self._hand_back(heart)
        finally:
            if heart is not None:
                heart.stop()
        if self.held and not game_running() and not self.installed:
            gamelock.release(HOLDER)
            self.held = False
            self.say("session: seat handed back")
        elif self.held:
            # keep the seat: a long hold so no pipeline launches onto a game that still carries pak90 (or is running)
            gamelock.acquire(HOLDER, "cinema hand-back FAILED: pak90 may be installed", 0, hold=6 * 3600)
            self.say("session: ERROR the seat is kept (pak90 still installed or the game still running): uninstall "
                     "pak90 (magician uninstall pak90_dir.vpk --holder cinema), check `magician install --status`, "
                     "then `gamelock.py release --holder cinema`")

    def _hand_back(self, heart):
        if self.con is not None and game_running():
            try:
                console.release_held()
                console.release_stuck()
                console.focus_game()
                # Panorama may be hidden (cin_clean_full): cinema_reset shows it again, so run it first
                self.con.run(f"exec cinema_reset;fps_max {self.fps_max}")
                time.sleep(0.5)
                self.con.run(";".join(f"unbind {k}" for k in KIT_KEYS) + ";exec cinema_unbind")
                time.sleep(0.5)
                self.say("session: cinema_reset done")
            except BaseException as e:   # the stop below must still happen
                self.say(f"session: reset failed ({e}); killing the game loses this session's archived changes, which is safe")
        if game_running() and self.held:
            # keep the seat through the uninstall (-KeepLock): stop_game would release it as the game closes, and a
            # pipeline waiting for it could launch and install between our stop and our uninstall. The heartbeat keeps
            # the hold alive however long the stop takes (a hung quit took 5 minutes).
            heart.start()
            try:
                r = ps("stop_game.ps1", "-Holder", HOLDER, "-KeepLock", timeout=STOP_TIMEOUT)
                self.say("session: " + (r.stdout.strip().splitlines() or ["(no output)"])[-1])
            except subprocess.TimeoutExpired:
                self.say(f"session: stop_game.ps1 gave no answer in {STOP_TIMEOUT}s (a hung quit): killing deadlock.exe")
            t = time.time()
            while game_running() and time.time() - t < 120:
                if time.time() - t > 30:
                    kill_game()
                time.sleep(3)
        elif self.held and heart is not None:
            heart.start()
        if self.installed and not game_running():
            if heart is not None and not heart.ours():
                # another holder has the seat (it should be impossible with the heartbeat): their pak90 is theirs to
                # uninstall; ours was replaced by their install. Do not touch it, do not release their seat.
                self.say("session: ERROR the seat was taken by another holder before the uninstall: not uninstalling")
                self.held = False
                self.installed = False
            else:
                self.installed = not uninstall_verified(self.say)
        elif self.installed:
            self.say("session: WARNING the game is still running: pak90 is still installed (uninstall it before anyone plays)")
        try:
            os.remove(os.path.join(INGAME, ".console-blind"))
        except OSError:
            pass


PAK_AS = os.path.basename(PAK)   # the name pak90 is installed under for this session (install_name())


def install_name():
    """pak90_dir.vpk, or the slot the last acting build asked for (`<PAK>.install_as`, written by acting.py)."""
    try:
        name = open(PAK + ".install_as", encoding="utf-8").read().strip().lower()
    except OSError:
        return os.path.basename(PAK)
    ok = name.startswith("pak") and name.endswith("_dir.vpk") and len(name) == 13 and name[3:5].isdigit()
    if not ok:
        raise SessionError(f"{PAK}.install_as names no pak slot: {name!r}")
    return name


STOP_TIMEOUT = 240     # stop_game.ps1's own path is at most ~2.5 min (45 s quit, 3 kills of 15 s, 30 s for the files)


def pak_state():
    """(listed, on_disk) for pak90: `magician install --status` lists it for the cinema mod, or the file sits in addons.
    Either one means installed. A status that fails to run counts as installed (unverified is not uninstalled)."""
    name = PAK_AS
    on_disk = os.path.exists(os.path.join(CITADEL, "addons", name))
    try:
        r = subprocess.run([MAGICIAN, "install", "--status"], capture_output=True, text=True, timeout=60)
    except (OSError, subprocess.TimeoutExpired):
        return True, on_disk
    if r.returncode != 0:
        return True, on_disk
    listed = any(line.split()[:1] == [name] for line in r.stdout.splitlines())
    return listed, on_disk


def uninstall_verified(say=print, tries=3, wait=10.0):
    """Uninstall pak90 and check `magician install --status` (and the addons folder) until it is gone. True only when
    verified uninstalled. Windows can hold a closed game's paks for seconds: retried."""
    for i in range(tries):
        listed, on_disk = pak_state()
        if not listed and not on_disk:
            say("session: the cinema pak verified uninstalled (magician install --status, addons)")
            return True
        r = subprocess.run([MAGICIAN, "uninstall", PAK_AS, "--holder", HOLDER], capture_output=True, text=True)
        say("session: " + (r.stdout.strip() or r.stderr.strip() or f"uninstall rc {r.returncode}"))
        listed, on_disk = pak_state()
        if not listed and not on_disk:
            say("session: the cinema pak verified uninstalled (magician install --status, addons)")
            return True
        say(f"session: pak90 still {'listed' if listed else ''}{' and ' if listed and on_disk else ''}"
            f"{'on disk' if on_disk else ''} after uninstall try {i + 1}")
        time.sleep(wait)
    return False


def kill_game():
    """The hard stop, on this session's own game only: the deadlock.exe bound to the seat while the seat is still
    ours. Never every deadlock.exe (a lingering game killed that way was once another pipeline's)."""
    lock = gamelock.read()
    if lock is None or lock.get("holder") != HOLDER or not gamelock.bound_alive(lock):
        return False
    subprocess.run(["taskkill", "/PID", str(lock["game_pid"]), "/F", "/T"], capture_output=True, text=True)
    return True


class _Heartbeat:
    """Keeps this session's seat held (busy_until renewed every `every` s) from the stop to the release, so the lock
    never goes stale while pak90 is still installed. `ours()` says whether the lock is still this holder's."""

    def __init__(self, say, every=15.0, hold=90.0):
        import threading
        self.say, self.every, self.hold = say, every, hold
        self._stop = threading.Event()
        self._thread = None
        self.lost = False

    def beat(self):
        if not gamelock.acquire(HOLDER, "cinema hand-back", 0, hold=self.hold, say=lambda *_: None):
            self.lost = True

    def start(self):
        import threading
        if self._thread is not None:
            return
        self.beat()

        def run():
            while not self._stop.wait(self.every):
                self.beat()
        self._thread = threading.Thread(target=run, daemon=True)
        self._thread.start()

    def ours(self):
        lock = gamelock.read()
        return not self.lost and lock is not None and lock.get("holder") == HOLDER

    def stop(self):
        self._stop.set()
        if self._thread is not None:
            self._thread.join(timeout=5)


def serve(s, inbox, idle):
    """Hold the session and run the scenario files dropped into `inbox` one by one (`<name>.txt` -> `<name>.done`,
    the run's output in `<name>.out`), until a file `quit` appears or nothing arrives for `idle` seconds (the seat is
    never held idle). Python steps: `<name>.py` is exec'd with `s` (the Session) in scope."""
    import contextlib
    import io
    import traceback
    os.makedirs(inbox, exist_ok=True)
    last = time.time()
    while True:
        if os.path.exists(os.path.join(inbox, "quit")):
            s.say("serve: quit asked")
            os.remove(os.path.join(inbox, "quit"))
            return
        todo = sorted(f for f in os.listdir(inbox) if f.endswith((".txt", ".py"))
                      and not os.path.exists(os.path.join(inbox, f.rsplit(".", 1)[0] + ".done")))
        if not todo:
            if time.time() - last > idle:
                s.say(f"serve: idle for {idle}s, handing the seat back")
                return
            time.sleep(1)
            continue
        f = todo[0]
        stem = os.path.join(inbox, f.rsplit(".", 1)[0])
        buf = io.StringIO()
        t0 = time.time()
        ok = True
        with contextlib.redirect_stdout(buf):
            try:
                if f.endswith(".py"):
                    exec(compile(open(os.path.join(inbox, f), encoding="utf-8").read(), f, "exec"), {"s": s, "__name__": "step"})
                else:
                    s.run(*open(os.path.join(inbox, f), encoding="utf-8-sig").read().splitlines())
            except Exception:
                ok = False
                traceback.print_exc(file=buf)
        with open(stem + ".out", "w", encoding="utf-8") as o:
            o.write(buf.getvalue())
        with open(stem + ".done", "w") as o:
            o.write(f"{'ok' if ok else 'FAILED'} {time.time() - t0:.1f}s\n")
        s.say(f"serve: {f} {'ok' if ok else 'FAILED'} in {time.time() - t0:.1f}s")
        last = time.time()


def main(a):
    def opt(name, default=None):
        return a[a.index(name) + 1] if name in a else default

    if "--out" not in a or ("--script" not in a and "--serve" not in a):
        sys.exit(__doc__)
    m = opt("--map", "new_player_basics")
    with Session(opt("--out"), map=None if m == "none" else m, sourcetv="--no-tv" not in a,
                 install="--keep-pak" not in a, extra=opt("--extra", ""),
                 size=tuple(int(v) for v in opt("--size", "1920x1080").split("x"))) as s:
        if "--serve" in a:
            serve(s, opt("--serve"), float(opt("--idle", 600)))
        else:
            s.run(*open(opt("--script"), encoding="utf-8-sig").read().splitlines())
        print(f"take: {s.take()}")


if __name__ == "__main__":
    main(sys.argv[1:])

"""Drive Deadlock's developer console from outside (retail has no netcon or -netconport).

    python -I console.py "map new_player_basics" "selecthero hero_haze"   # each argument is one command
    python -I console.py --wait-log "OnGameStateChanged: GameInProgress" 120 "map new_player_basics"
    python -I console.py --script scenario.txt                           # one command per line; see below
    python -I console.py --tail 30                                       # last lines of console.log

The game reads keyboard input through SDL raw scancodes, so pasted text and virtual-key events are ignored:
this sends hardware scancodes with SendInput. The game window is brought to the front first, and the
console is toggled with the key bound to it (--key, else MAGICIAN_CONSOLE_KEY, else the player's own bind read
from citadel/cfg/user_keys_0_slot0.vcfg, else grave). Output comes back
through console.log (launch with -condebug).

Scenario lines: a console command, or a directive:
    @wait 2.5                  sleep seconds
    @log <text> [timeout]      wait until console.log gains a line containing <text>
    @shot <name>               capture.py shot into the run folder
    @burst <name> <frames> <fps> [full]   (full: full resolution, for judging detail)
    @key <name> [hold_s]       press a game key outside the console (w, space, q, e, r, f, mouse1, ...; `console`
                               is the console key, so a scenario can open the console without knowing its bind)
    @down <key[+key]>          game keys down and left down (release with @up): motion for a screen recording
    @up <key[+key]>
    @hold, @mouse, @wheel, @entity, @turntable, @orbit: see ingame/README.md
    # comment

One user of the game at a time: before typing, the game lock (gamelock.py) is taken for --holder NAME (else
$MAGICIAN_GAME_HOLDER, else "anonymous"); exit 3 while another holder has the game, or --wait-lock SECS to wait.
Nothing is typed into a game the holder did not launch: the window's process must be the one launch.ps1 bound to
the seat (exit 3 otherwise, with the reason). --release-lock hands the seat back after the run with the game still up
(the next holder cannot drive it; otherwise the holder keeps the seat until stop_game.ps1).
Shots and failure evidence go to --out DIR, else <reference>/work/console.
"""
import ctypes
import ctypes.wintypes as wt
import os
import subprocess
import sys
import time

user32 = ctypes.WinDLL("user32", use_last_error=True)
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import gamepaths  # noqa: E402

LOG = os.path.join(gamepaths.try_game(), "citadel", "console.log")

KEYEVENTF_KEYUP, KEYEVENTF_SCANCODE, KEYEVENTF_EXTENDEDKEY = 0x2, 0x8, 0x1
MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP = 0x2, 0x4, 0x8, 0x10


class KEYBDINPUT(ctypes.Structure):
    _fields_ = [("wVk", wt.WORD), ("wScan", wt.WORD), ("dwFlags", wt.DWORD), ("time", wt.DWORD),
                ("dwExtraInfo", ctypes.c_size_t)]


class MOUSEINPUT(ctypes.Structure):
    _fields_ = [("dx", wt.LONG), ("dy", wt.LONG), ("mouseData", wt.DWORD), ("dwFlags", wt.DWORD),
                ("time", wt.DWORD), ("dwExtraInfo", ctypes.c_size_t)]


class _U(ctypes.Union):
    _fields_ = [("ki", KEYBDINPUT), ("mi", MOUSEINPUT), ("pad", ctypes.c_byte * 32)]


class INPUT(ctypes.Structure):
    _fields_ = [("type", wt.DWORD), ("u", _U)]


user32.SendInput.argtypes = [wt.UINT, ctypes.POINTER(INPUT), ctypes.c_int]

# US layout set-1 scancodes
SC = {"esc": 0x01, "1": 0x02, "2": 0x03, "3": 0x04, "4": 0x05, "5": 0x06, "6": 0x07, "7": 0x08, "8": 0x09,
      "9": 0x0A, "0": 0x0B, "-": 0x0C, "=": 0x0D, "backspace": 0x0E, "tab": 0x0F, "q": 0x10, "w": 0x11,
      "e": 0x12, "r": 0x13, "t": 0x14, "y": 0x15, "u": 0x16, "i": 0x17, "o": 0x18, "p": 0x19, "[": 0x1A,
      "]": 0x1B, "enter": 0x1C, "ctrl": 0x1D, "a": 0x1E, "s": 0x1F, "d": 0x20, "f": 0x21, "g": 0x22,
      "h": 0x23, "j": 0x24, "k": 0x25, "l": 0x26, ";": 0x27, "'": 0x28, "grave": 0x29, "shift": 0x2A,
      "\\": 0x2B, "z": 0x2C, "x": 0x2D, "c": 0x2E, "v": 0x2F, "b": 0x30, "n": 0x31, "m": 0x32, ",": 0x33,
      ".": 0x34, "/": 0x35, "alt": 0x38, "space": 0x39, "f1": 0x3B, "f2": 0x3C, "f3": 0x3D, "f4": 0x3E,
      "f5": 0x3F, "f6": 0x40, "f7": 0x41, "f8": 0x42, "f9": 0x43, "f10": 0x44, "f11": 0x57, "f12": 0x58, "scrolllock": 0x46, "rshift": 0x36}
SHIFTED = {'"': "'", "_": "-", "+": "=", ":": ";", "?": "/", ">": ".", "<": ",", "{": "[", "}": "]",
           "|": "\\", "!": "1", "@": "2", "#": "3", "$": "4", "%": "5", "^": "6", "&": "7", "*": "8",
           "(": "9", ")": "0", "~": "grave"}


def bound_console_key(vcfg_text):
    """The key bound to `toggleconsole` in a user_keys vcfg (`"F7"  "toggleconsole"`), as a SC name, or None."""
    import re
    m = re.search(r'"([^"]+)"\s+"toggleconsole"', vcfg_text or "")
    if not m:
        return None
    k = m.group(1).lower()
    k = {"`": "grave", "~": "grave"}.get(k, k)
    return k if k in SC else None


def console_key():
    """The key that toggles the console on this machine: MAGICIAN_CONSOLE_KEY, else the player's own bind in
    citadel\\cfg\\user_keys_0_slot0.vcfg, else grave (the game's default)."""
    if os.environ.get("MAGICIAN_CONSOLE_KEY", "").strip():
        return os.environ["MAGICIAN_CONSOLE_KEY"].strip().lower()
    try:
        with open(os.path.join(gamepaths.try_game(), "citadel", "cfg", "user_keys_0_slot0.vcfg"), encoding="utf-8",
                  errors="replace") as f:
            return bound_console_key(f.read()) or "grave"
    except OSError:
        return "grave"


def _send(*inputs):
    arr = (INPUT * len(inputs))(*inputs)
    if user32.SendInput(len(inputs), arr, ctypes.sizeof(INPUT)) != len(inputs):
        raise OSError(f"SendInput blocked (error {ctypes.get_last_error()}); is an elevated window in front?")


def _key(sc, up):
    i = INPUT(type=1)
    i.u.ki = KEYBDINPUT(0, sc, KEYEVENTF_SCANCODE | (KEYEVENTF_KEYUP if up else 0), 0, 0)
    return i


def tap(name, hold=0.03):
    sc = SC[name]
    _send(_key(sc, False))
    time.sleep(hold)
    _send(_key(sc, True))
    time.sleep(0.02)


def mouse(button, hold=0.05):
    down, up = (MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP) if button == "mouse1" else (MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP)
    i = INPUT(type=0)
    i.u.mi = MOUSEINPUT(0, 0, 0, down, 0, 0)
    _send(i)
    time.sleep(hold)
    i.u.mi = MOUSEINPUT(0, 0, 0, up, 0, 0)
    _send(i)


def type_text(text):
    for ch in text:
        if ch == " ":
            tap("space", 0.01)
        elif ch.lower() in SC and len(ch) == 1 and not ch.isupper():
            tap(ch, 0.01)
        elif ch.isupper() or ch in SHIFTED:
            # Right Shift, never the left one: the game reads keys raw even while the console is open, and in a demo
            # the roaming spectator flies up on Left Shift ("Fly Up"); every underscore typed with it kicked the free
            # camera upward and spec_goto keeps the velocity (build 6864, cinema session 2026-11-21: the "drift" of
            # every typed camera path). Gameplay binds Left Shift too (Lash's dash). The console types the same.
            base = ch.lower() if ch.isupper() else SHIFTED[ch]
            _send(_key(SC["rshift"], False))
            try:
                tap(base, 0.01)
            finally:
                _send(_key(SC["rshift"], True))
        else:
            raise ValueError(f"no scancode for {ch!r}")


def _describe(hwnd):
    if not hwnd:
        return "nothing"
    t = ctypes.create_unicode_buffer(200)
    user32.GetWindowTextW(hwnd, t, 200)
    c = ctypes.create_unicode_buffer(200)
    user32.GetClassNameW(hwnd, c, 200)
    return f"{t.value[:60]!r} (class {c.value})"


def game_window():
    sys.path.insert(0, HERE)
    from capture import find_window
    return find_window()


def game_pid():
    hwnd = game_window()
    if not hwnd:
        return 0
    pid = wt.DWORD()
    user32.GetWindowThreadProcessId(hwnd, ctypes.byref(pid))
    return pid.value


def game_started():
    """When the running game's process was created (Unix time); 0 when there is no game."""
    pid = game_pid()
    if not pid:
        return 0.0
    kernel32 = ctypes.WinDLL("kernel32")
    kernel32.OpenProcess.restype = wt.HANDLE
    h = kernel32.OpenProcess(0x1000, False, pid)   # PROCESS_QUERY_LIMITED_INFORMATION
    if not h:
        return 0.0
    made, a, b, c = wt.FILETIME(), wt.FILETIME(), wt.FILETIME(), wt.FILETIME()
    ok = kernel32.GetProcessTimes(wt.HANDLE(h), ctypes.byref(made), ctypes.byref(a), ctypes.byref(b), ctypes.byref(c))
    kernel32.CloseHandle(wt.HANDLE(h))
    if not ok:
        return 0.0
    return ((made.dwHighDateTime << 32) | made.dwLowDateTime) / 1e7 - 11644473600.0


class NotOurGame(SystemExit):
    """The running game is not the one this holder launched: nothing may be typed into it (exit 3)."""


_DRIVEN = {"pid": 0}   # the game pid this process has been allowed to drive (re-checked when the window's pid changes)


def guard():
    """Refuse, before any key reaches it, a game that this holder did not launch: the seat must be this holder's
    ($MAGICIAN_GAME_HOLDER, or --holder) and the window's process the one launch.ps1 bound to the seat. On 2026-11-24 a
    pipeline whose seat had been taken typed a stage plan into the next holder's game. The holder is re-read every time
    (cheap); the process start time only when the window's pid changes."""
    import gamelock
    pid = game_pid()
    lock = gamelock.read()
    holder = gamelock.holder_name()
    if lock is not None and lock["holder"] == holder and pid and pid == _DRIVEN["pid"] and lock.get("game_pid") == pid:
        return pid
    why = gamelock.may_drive(lock, holder, pid)
    if why:
        print(f"console: refused: {why}")
        raise NotOurGame(3)
    _DRIVEN["pid"] = pid
    return pid


FOCUS_WAIT = 12.0   # seconds focus_game keeps trying before it gives up


def focus_game():
    """Bring the game to the front and prove it is there. Called before a script and again before every command and
    key (ensure_focus): SendInput goes to whatever window is in front, so a window that takes the foreground mid-script
    (a notification, the desktop app on the other monitor) would otherwise receive the rest of the scenario as typing."""
    hwnd = game_window()
    if not hwnd:
        sys.exit("console: no Deadlock window")
    guard()
    user32.GetForegroundWindow.restype = wt.HWND
    # A shell flyout (the Start menu, Quick settings, a notification's panel) holds the foreground against every
    # route below and lets go by itself within seconds, so three quick tries gave up on a window that was about to
    # leave. Keep asking for FOCUS_WAIT seconds; nothing is typed until the game is in front.
    deadline, tries = time.time() + FOCUS_WAIT, 0
    while time.time() < deadline:
        if user32.GetForegroundWindow() == hwnd:
            break
        tries += 1
        # Windows only lets the foreground thread hand focus over: join its input queue for the switch,
        # and tap Alt (which lifts the foreground lock) as a second route.
        kernel32 = ctypes.WinDLL("kernel32")
        fg = user32.GetForegroundWindow()
        fg_thread = user32.GetWindowThreadProcessId(fg, None) if fg else 0
        me = kernel32.GetCurrentThreadId()
        if fg_thread and fg_thread != me:
            user32.AttachThreadInput(me, fg_thread, True)
        if tries <= 3:   # the Alt tap goes to whatever is in front: a few, not a stream into someone's window
            _send(_key(SC["alt"], False), _key(SC["alt"], True))
        user32.ShowWindow(hwnd, 9)
        user32.BringWindowToTop(hwnd)
        user32.SetForegroundWindow(hwnd)
        if fg_thread and fg_thread != me:
            user32.AttachThreadInput(me, fg_thread, False)
        time.sleep(0.4)
    if user32.GetForegroundWindow() != hwnd:
        sys.exit("console: could not bring the Deadlock window to the front; in front: "
                 + _describe(user32.GetForegroundWindow()))
    return hwnd


HELD = set()   # keys this run holds down through @down (released when the script ends, however it ends)
_VK = {"ctrl": 0xA2, "shift": 0xA0, "rshift": 0xA1, "alt": 0xA4, "w": 0x57, "a": 0x41, "s": 0x53, "d": 0x44, "space": 0x20}


def release_stuck():
    """Send key-up for every modifier and movement key the system still reads as down. A key-down sent with SendInput
    stays down for the whole desktop until its key-up: a scenario that died between `@down ctrl` and `@up ctrl` left
    Ctrl held (2026-11-20), so every later session typed Ctrl+letter chords into the console (nothing arrives, no hero
    is selected), Enter opened chat, and Esc became Ctrl+Esc, the Start menu, which then took the foreground."""
    user32.GetAsyncKeyState.restype = ctypes.c_short
    stuck = [k for k, vk in _VK.items() if user32.GetAsyncKeyState(vk) & 0x8000]
    for k in stuck:
        _send(_key(SC[k], True))
    if stuck:
        print(f"console: released keys left held down by an earlier run: {', '.join(stuck)}")
        time.sleep(0.1)
    return stuck


def release_held():
    for k in sorted(HELD):
        _send(_key(SC[k], True))
    HELD.clear()


def ensure_focus():
    """Cheap check before each command or key: one call when the game is already in front."""
    hwnd = game_window()
    user32.GetForegroundWindow.restype = wt.HWND
    if hwnd and user32.GetForegroundWindow() == hwnd:
        guard()
        return
    print(f"console: the game lost the foreground to {_describe(user32.GetForegroundWindow())}; taking it back")
    focus_game()


def log_size():
    return os.path.getsize(LOG) if os.path.exists(LOG) else 0


def log_since(offset):
    with open(LOG, "rb") as f:
        f.seek(offset)
        return f.read().decode("utf-8", "replace")


def wait_log(text, timeout, offset):
    end = time.time() + timeout
    while time.time() < end:
        if text in log_since(offset):
            return True
        time.sleep(0.5)
    return False


class Console:
    # The console is a panorama panel: with r_drawpanorama 0 (the cinema kit's clean frame) it is invisible,
    # so its state can't be read off the screen. After a command that hides panorama the console is driven
    # blind (one tap per toggle, state tracked here) until a command shows panorama again.
    BLIND_ON = ("r_drawpanorama 0", "r_drawpanorama false", "cin_clean_full")
    BLIND_OFF = ("r_drawpanorama 1", "r_drawpanorama true", "cin_hud")

    STATE = os.path.join(HERE, ".console-blind")   # survives between console.py runs while panorama is hidden

    def __init__(self, key=None):
        self.key = key or console_key()
        # None: read the screen; True/False: the tracked state while panorama is hidden.
        # The state file belongs to one game process (its pid is the first word). A file left by an earlier game (a
        # session that ended, crashed or was killed with panorama hidden) says nothing about this one: a new game
        # starts with panorama shown and the console closed. Trusting it drove every later session blind from the
        # wrong state (2026-11-20: commands typed into the game, Enter opening party chat, no hero selected).
        self.blind = None
        if os.path.exists(self.STATE):
            words = open(self.STATE).read().split()
            # valid: written after this game started (and for this pid, when the file names one; a file from before
            # the pid was recorded holds the state alone)
            fresh = os.path.getmtime(self.STATE) > game_started()
            if fresh and words and words[-1] in ("open", "closed") and (len(words) == 1 or words[0] == str(game_pid())):
                self.blind = words[-1] == "open"
            else:
                print(f"console: dropped a stale blind-console state ({' '.join(words)!r}, game pid {game_pid()})")
                os.remove(self.STATE)

    def _track(self, state):
        self.blind = state
        if state is None:
            if os.path.exists(self.STATE):
                os.remove(self.STATE)
        else:
            with open(self.STATE, "w") as f:
                f.write(f"{game_pid()} {'open' if state else 'closed'}")

    def probe(self):
        """Prove that typed commands reach the game's console before a scenario spends its time: type a token that is
        not a command and find the game's "Unknown command: <token>" in console.log (`echo`, a convar query, `find`
        and `say` leave nothing in the log on build 6864). One resync by the screen and a second try, then a loud
        failure."""
        token = f"magician_probe_{os.getpid()}_{int(time.time()) % 100000}"
        for attempt in (1, 2):
            start = log_size()
            try:
                self.run(token)
            except RuntimeError as e:
                print(f"console: probe {attempt}: {e}")
            if wait_log(token, 3.0, start):
                return
            print(f"console: probe {attempt} did not reach the console (in front: "
                  f"{_describe(user32.GetForegroundWindow())}, blind state {self.blind})")
            if attempt == 1:
                self._track(None)      # forget any tracked state and read the screen again
                release_stuck()
                focus_game()
        raise RuntimeError("commands do not reach the game's console (the probe never appeared in console.log)")

    def set_open(self, want):
        """Toggle until the console's on-screen state matches; a lost keypress otherwise desyncs every later
        command (typed text lands in the game, shots get the console panel in them)."""
        if self.blind is not None:
            if self.blind != want:
                tap(self.key)
                time.sleep(0.6)
                self._track(want)
            return
        for _ in range(4):
            if console_open() == want:
                return
            tap(self.key)
            time.sleep(0.6)   # the panel animates; keys sent sooner are lost
        if console_open() != want:
            raise RuntimeError(f"console would not {'open' if want else 'close'}")

    def run(self, cmd):
        ensure_focus()
        self.set_open(True)
        type_text(cmd)
        tap("enter")
        time.sleep(0.15)
        last = max(((cmd.rfind(s), True) for s in self.BLIND_ON), default=(-1, None))
        last = max([last] + [(cmd.rfind(s), False) for s in self.BLIND_OFF])
        if last[0] >= 0:
            # the panel is (in)visible from the next frame: hand over to the tracked state while it is open
            self._track(True if last[1] else None)
            time.sleep(0.2)
        self.set_open(False)


def console_open():
    """The developer console is a (35,35,35) panel with a (54,54,54) frame at the window's top-left."""
    sys.path.insert(0, HERE)
    from capture import find_window, client_rect, grab
    hwnd = find_window()
    x, y, w, h = client_rect(hwnd)
    px = grab(x, y, w, h, hwnd)
    # positions measured at 1920x1080; Panorama scales the console with the window height (a larger window: the
    # cinema kit's supersampling), so the probe points scale with it
    sc = h / 1080.0
    rows = [int(yy * sc) for yy in range(120, 600, 10)]
    fx = int(round(20 * sc))
    frame = sum(1 for yy in rows if px[(yy * w + fx) * 4:(yy * w + fx) * 4 + 3] == b"\x36\x36\x36")
    # the fill is counted per column and the three clearest columns are kept: a console full of long spam lines (the
    # sandbox's prediction-error flood, the renderer's error-texture lines) puts text under any fixed column, and an
    # open console then read as closed (2026-11-20: "console would not open" with the console on screen)
    cols = sorted((sum(1 for yy in rows if px[(yy * w + xx) * 4:(yy * w + xx) * 4 + 3] == b"###")
                   for xx in (int(v * sc) for v in range(300, 1000, 20))), reverse=True)
    fill = sum(cols[:3])
    return frame >= 40 and fill >= 60


def run_script(lines, key, out_dir, probe=True):
    release_stuck()
    focus_game()
    try:
        _run_script(lines, key, out_dir, probe)
    finally:
        release_held()   # never leave a key down for the next run, or for the person at the keyboard


def _run_script(lines, key, out_dir, probe):
    con = Console(key)
    state = {"probed": not probe}

    def proven():
        # before the first command of a script (not at its start: a script may first wait for the map to load)
        if state["probed"]:
            return
        state["probed"] = True
        try:
            con.probe()
        except RuntimeError:
            # leave the evidence: the game window, and the whole desktop (which window is where)
            from capture import write_png, grab, find_window, client_rect
            try:
                hwnd = find_window()
                x, y, w, h = client_rect(hwnd)
                write_png(os.path.join(out_dir, "harness_fail_game.png"), w, h, grab(x, y, w, h, hwnd))
                vx, vy = user32.GetSystemMetrics(76), user32.GetSystemMetrics(77)
                vw, vh = user32.GetSystemMetrics(78), user32.GetSystemMetrics(79)
                write_png(os.path.join(out_dir, "harness_fail_desktop.png"), vw, vh, grab(vx, vy, vw, vh))
            except Exception as e:   # the evidence is a courtesy; the failure is what matters
                print(f"console: could not save the failure shots ({e})")
            raise
    variables = {}
    script_start = log_size()
    for raw in lines:
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        start = log_size()
        if line.startswith("@"):
            parts = line[1:].split()
            op = parts[0]
            if op in ("turntable", "orbit"):
                proven()
            if op == "wait":
                time.sleep(float(parts[1]))
            elif op == "log":
                timeout = float(parts[-1]) if parts[-1].replace(".", "").isdigit() and len(parts) > 2 else 60
                text = " ".join(parts[1:-1] if len(parts) > 2 and parts[-1].replace(".", "").isdigit() else parts[1:])
                print(f"@log {text!r}: {'seen' if wait_log(text, timeout, start) else 'TIMEOUT'}")
            elif op == "shot":
                subprocess.run([sys.executable, "-I", os.path.join(HERE, "capture.py"), "shot",
                                os.path.join(out_dir, parts[1] + ".png")], check=True)
            elif op == "burst":
                subprocess.run([sys.executable, "-I", os.path.join(HERE, "capture.py"), "burst",
                                os.path.join(out_dir, parts[1]), "--frames", parts[2], "--fps", parts[3]]
                               + (["--full"] if parts[4:5] == ["full"] else []), check=True)
            elif op == "hold":   # @hold <key[+key]> <name> <frames> <fps>: keys down for the whole burst
                keys = parts[1].split("+")
                for k in keys:
                    _send(_key(SC[k], False))
                try:
                    subprocess.run([sys.executable, "-I", os.path.join(HERE, "capture.py"), "burst",
                                    os.path.join(out_dir, parts[2]), "--frames", parts[3], "--fps", parts[4]], check=True)
                finally:
                    for k in keys:
                        _send(_key(SC[k], True))
            elif op in ("down", "up") and (ensure_focus() or True):   # @down <key[+key]> / @up <key[+key]>: hold game keys across later directives
                for k in parts[1].split("+"):   # (a real key held while @mouse turns the view: no console on screen)
                    _send(_key(SC[k], op == "up"))
                    (HELD.discard if op == "up" else HELD.add)(k)
            elif op == "entity":   # @entity <name>: the bot the last citadel_create_unit made -> {entity}
                import re
                found = re.findall(r"Created bot (\d+)/" + re.escape(parts[1]) + "/", log_since(script_start))
                if not found:
                    raise RuntimeError(f"@entity: no 'Created bot N/{parts[1]}' line since the script started")
                variables["entity"] = found[-1]
                print(f"@entity {parts[1]} -> {variables['entity']}")
            elif op == "turntable":   # @turntable <steps> <yaw facing the camera> <tag>
                steps, face, tag = int(parts[1]), float(parts[2]), parts[3]
                for k in range(steps):
                    turn = 360.0 * k / steps
                    cmd = f"ent_setang {variables['entity']} 0 {(face + turn) % 360:.1f} 0"
                    con.run(cmd)
                    time.sleep(0.5)
                    con.run(cmd)   # the bot's look-at drifts it back toward the player; pin it again
                    time.sleep(0.35)
                    subprocess.run([sys.executable, "-I", os.path.join(HERE, "capture.py"), "shot",
                                    os.path.join(out_dir, tag, f"{tag}_{int(turn):03d}.png")], check=True)
            elif op == "orbit":   # @orbit <steps> <radius> <cx> <cy> <z> <pitch> <tag>: walk the player round a point
                import math
                steps, radius, cx, cy, z, pitch, tag = int(parts[1]), *map(float, parts[2:7]), parts[7]
                for k in range(steps):
                    deg = 360.0 * k / steps
                    r = math.radians(deg)
                    con.run(f"setpos {cx + radius * math.cos(r):.1f} {cy + radius * math.sin(r):.1f} {z:.1f};"
                            f"setang {pitch:.1f} {(deg + 180) % 360:.1f} 0")
                    time.sleep(0.9)
                    if "setpos into world" in log_since(start):
                        print(f"@orbit {deg:.0f}: blocked by world geometry, skipped")
                        start = log_size()
                        continue
                    subprocess.run([sys.executable, "-I", os.path.join(HERE, "capture.py"), "shot",
                                    os.path.join(out_dir, tag, f"{tag}_{int(deg):03d}.png")], check=True)
            elif op == "mouse":   # @mouse <dx> <dy> [seconds]: relative move, spread over the time (inspect orbit)
                dx, dy = int(parts[1]), int(parts[2])
                secs = float(parts[3]) if len(parts) > 3 else 0.0
                steps = max(1, int(secs * 60))
                done_x = done_y = 0
                for k in range(1, steps + 1):
                    sx, sy = dx * k // steps - done_x, dy * k // steps - done_y
                    done_x += sx; done_y += sy
                    i = INPUT(type=0)
                    i.u.mi = MOUSEINPUT(sx, sy, 0, 0x1, 0, 0)   # MOUSEEVENTF_MOVE
                    _send(i)
                    if steps > 1:
                        time.sleep(secs / steps)
            elif op == "wheel":   # @wheel <notches>: mouse wheel, + away from you (120 per notch)
                for _ in range(abs(int(parts[1]))):
                    i = INPUT(type=0)
                    i.u.mi = MOUSEINPUT(0, 0, ctypes.c_ulong(120 if int(parts[1]) > 0 else -120).value, 0x0800, 0, 0)
                    _send(i)
                    time.sleep(0.05)
            elif op == "key":
                ensure_focus()
                hold = float(parts[2]) if len(parts) > 2 else 0.05
                name = con.key if parts[1] == "console" else parts[1]
                mouse(name, hold) if name.startswith("mouse") else tap(name, hold)
            else:
                raise ValueError(f"unknown directive {line}")
        else:
            for k, v in variables.items():
                line = line.replace("{" + k + "}", v)
            proven()
            con.run(line)
            time.sleep(0.3)
            new = log_since(start).strip()
            print(f"> {line}" + (f"\n  {new[-400:]}" if new else ""))


if __name__ == "__main__":
    a = sys.argv[1:]
    if a[:1] and a[0] in ("--help", "-h", "help"):
        print(__doc__)
        sys.exit(0)
    key = None   # console_key(): MAGICIAN_CONSOLE_KEY, else the player's own bind, else grave
    if "--key" in a:
        i = a.index("--key"); key = a[i + 1]; del a[i:i + 2]
    # shots and the probe's failure evidence go to --out, else <reference>\work\console (never the caller's folder:
    # a failed probe once left harness_fail_*.png in the repository root)
    out = gamepaths.work("console")
    if "--out" in a:
        i = a.index("--out"); out = a[i + 1]; del a[i:i + 2]
    os.makedirs(out, exist_ok=True)
    if not a:
        sys.exit(__doc__)
    # One user of the game at a time (gamelock.py): take the game lock for --holder (else MAGICIAN_GAME_HOLDER, else
    # "anonymous") before typing anything. Exit 3 when another holder has the game; --wait-lock SECS waits instead.
    # The lock stays with the holder across runs until stop_game.ps1 (or --release-lock on the last run).
    holder, wait_lock, release_lock = None, 0.0, "--release-lock" in a
    if "--holder" in a:
        i = a.index("--holder"); holder = a[i + 1]; del a[i:i + 2]
    if "--wait-lock" in a:
        i = a.index("--wait-lock"); wait_lock = float(a[i + 1]); del a[i:i + 2]
    if release_lock:
        a.remove("--release-lock")
    PROBE = "--no-probe" not in a   # the probe before a run's first command (off: a harness self-test)
    if not PROBE:
        a.remove("--no-probe")
    sys.path.insert(0, HERE)
    import gamelock
    holder = gamelock.holder_name(holder)
    os.environ["MAGICIAN_GAME_HOLDER"] = holder   # guard() checks every key against this holder's bound game
    if a and a[0] != "--tail" and not gamelock.acquire(
            holder, os.path.basename(a[1]) if a[0] == "--script" and len(a) > 1 else "console", wait_lock):
        sys.exit(3)
    if a[0] == "--tail":
        n = int(a[1]) if len(a) > 1 else 30
        print("\n".join(log_since(max(0, log_size() - 200000)).splitlines()[-n:]))
    elif a[0] == "--script":
        run_script(open(a[1], encoding="utf-8-sig").read().splitlines(), key, out, PROBE)
    elif a[0] == "--wait-log":
        text, timeout, cmds = a[1], float(a[2]), a[3:]
        start = log_size()
        run_script(cmds, key, out, PROBE)
        ok = wait_log(text, timeout, start)
        print(f"wait-log {text!r}: {'seen' if ok else 'TIMEOUT'}")
        if release_lock:
            gamelock.release(holder, leave_running=True)
        sys.exit(0 if ok else 1)
    else:
        run_script(a, key, out, PROBE)
    if release_lock:
        gamelock.release(holder, leave_running=True)

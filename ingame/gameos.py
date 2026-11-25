"""Everything the in-game harness asks of the operating system, in one place: the game's processes, its window, the
keys and mouse it is sent, the pixels of its client area, and Steam to launch it.

    python -I gameos.py            which backends this machine has, or what is missing (exit 0, or 2 with the reason)

Two layers, each chosen once at import:

- **Processes** (find the running game, when it started, stop it, whether its files are let go). Windows: tasklist
  and kernel32. Linux: /proc, where the game runs under Proton as `deadlock.exe` (or `project8`, its old name). Needs
  no desktop, so the game lock works on a headless machine too.
- **Display** (find the window, bring it forward, send keys and the mouse, capture the client area). Windows: user32
  and gdi32, typing hardware scancodes with SendInput and reading the window through PrintWindow. Linux: X11 through
  libX11 and libXtst (XTest), on an X server or XWayland (DISPLAY). Without one of those, `display_problem()` says
  what is missing and every harness tool that would type or look refuses with that reason, before it launches or
  touches anything.

Key names are console.py's (`a`..`z`, `0`..`9`, `enter`, `space`, `grave`, `f1`..`f12`, `rshift`, ...); mouse buttons
are 1 left, 2 right, 3 middle, 4 and 5 the side buttons. Standard library only.
"""
import os
import subprocess
import sys
import time

GAME_EXE = "deadlock.exe"
APP_ID = "1422450"
WINDOWS = sys.platform == "win32"
LINUX = sys.platform.startswith("linux")


class Unsupported(SystemExit):
    """This system lacks something the harness needs; the message says what (exit 2)."""

    def __init__(self, why):
        super().__init__(why)
        self.code = 2
        self.why = why

    def __str__(self):
        return self.why


# ---- processes -----------------------------------------------------------------------------------------------------

def is_game_process(comm, cmdline):
    """A process is the game when its name, or its program in its command line, is deadlock.exe (Windows, or Proton)
    or project8. `cmdline` is /proc/<pid>/cmdline: arguments separated by NULs; only the program (the first one, or a
    Wine loader's second) counts, so an editor with deadlock.exe in an argument is not the game. The same rule as the
    CLI's seat check (crates/magician/src/install.rs)."""
    def is_game(s):
        name = s.strip().lower().replace("\\", "/").rsplit("/", 1)[-1]
        return name in ("deadlock.exe", "project8", "project8.exe")
    args = cmdline.split("\0")
    program = args[0] if args else ""
    base = program.replace("\\", "/").rsplit("/", 1)[-1]
    loader = base.startswith("wine") or "preloader" in base
    return is_game(comm) or is_game(program) or (loader and len(args) > 1 and is_game(args[1]))


def _proc_read(pid, name, binary=False):
    try:
        with open(f"/proc/{pid}/{name}", "rb") as f:
            data = f.read()
    except OSError:
        return None
    return data if binary else data.decode("utf-8", "replace")


def _proc_game_pids():
    pids = []
    for e in os.listdir("/proc"):
        if not e.isdigit():
            continue
        comm = _proc_read(e, "comm") or ""
        cmd = _proc_read(e, "cmdline") or ""
        if is_game_process(comm, cmd):
            pids.append(int(e))
    return sorted(pids)


def _proc_boot_time():
    with open("/proc/stat", encoding="ascii", errors="replace") as f:
        for line in f:
            if line.startswith("btime "):
                return float(line.split()[1])
    raise OSError("/proc/stat has no btime")


def _proc_started(pid):
    stat = _proc_read(pid, "stat")
    if not stat or ")" not in stat:
        return None
    fields = stat.rsplit(")", 1)[1].split()   # the name in brackets may hold spaces; what follows does not
    if len(fields) < 20 or fields[0] in ("Z", "X"):   # a zombie has exited; only its parent has not read it yet
        return None
    try:
        return _proc_boot_time() + int(fields[19]) / os.sysconf("SC_CLK_TCK")
    except (OSError, ValueError):
        return None


def _proc_open_by_anyone(path):
    """Whether any process this user can see has `path` open (/proc/*/fd)."""
    want = os.path.realpath(path)
    for e in os.listdir("/proc"):
        if not e.isdigit():
            continue
        try:
            fds = os.listdir(f"/proc/{e}/fd")
        except OSError:
            continue
        for fd in fds:
            try:
                if os.readlink(f"/proc/{e}/fd/{fd}") == want:
                    return True
            except OSError:
                continue
    return False


if WINDOWS:
    import ctypes
    import ctypes.wintypes as wt

    _kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
    _kernel32.OpenProcess.restype = wt.HANDLE
    _kernel32.CreateFileW.restype = wt.HANDLE
    _kernel32.CreateFileW.argtypes = [wt.LPCWSTR, wt.DWORD, wt.DWORD, ctypes.c_void_p, wt.DWORD, wt.DWORD, wt.HANDLE]

    def game_pids():
        """The pids of every running deadlock.exe (tasklist, so no extra modules and no admin rights)."""
        try:
            out = subprocess.run(["tasklist", "/FI", f"IMAGENAME eq {GAME_EXE}", "/NH", "/FO", "CSV"],
                                 capture_output=True, text=True).stdout
        except OSError:
            return []
        pids = []
        for line in out.splitlines():
            cols = [c.strip('"') for c in line.split('","')]
            if len(cols) > 1 and cols[0].lower().strip('"') == GAME_EXE and cols[1].isdigit():
                pids.append(int(cols[1]))
        return pids

    def process_started(pid):
        """When process `pid` was created (Unix time), or None when it is not running (or cannot be asked)."""
        if not isinstance(pid, int) or pid <= 0:
            return None
        h = _kernel32.OpenProcess(0x1000, False, pid)   # PROCESS_QUERY_LIMITED_INFORMATION
        if not h:
            return None
        try:
            code = wt.DWORD()
            if not _kernel32.GetExitCodeProcess(wt.HANDLE(h), ctypes.byref(code)) or code.value != 259:   # STILL_ACTIVE
                return None
            made, a, b, c = wt.FILETIME(), wt.FILETIME(), wt.FILETIME(), wt.FILETIME()
            if not _kernel32.GetProcessTimes(wt.HANDLE(h), ctypes.byref(made), ctypes.byref(a), ctypes.byref(b), ctypes.byref(c)):
                return None
            return ((made.dwHighDateTime << 32) | made.dwLowDateTime) / 1e7 - 11644473600.0
        finally:
            _kernel32.CloseHandle(wt.HANDLE(h))

    def terminate(pid):
        """The hard stop of one process (as Stop-Process -Force). False when it could not be asked to end."""
        h = _kernel32.OpenProcess(0x0001, False, pid)   # PROCESS_TERMINATE
        if not h:
            return False
        try:
            return bool(_kernel32.TerminateProcess(wt.HANDLE(h), 1))
        finally:
            _kernel32.CloseHandle(wt.HANDLE(h))

    def file_free(path):
        """Whether `path` can be opened for writing with no sharing: Windows holds a closed game's addon paks for a
        few seconds after its process ends."""
        h = _kernel32.CreateFileW(path, 0xC0000000, 0, None, 3, 0x80, None)   # GENERIC_READ|WRITE, no sharing, OPEN_EXISTING
        if h in (None, wt.HANDLE(-1).value, -1):
            return ctypes.get_last_error() == 2   # gone is free
        _kernel32.CloseHandle(wt.HANDLE(h))
        return True

    def process_problem():
        return None

elif LINUX:
    def game_pids():
        """The pids of every running game: deadlock.exe under Proton (or project8), read from /proc."""
        return _proc_game_pids()

    def process_started(pid):
        """When process `pid` was created (Unix time, from /proc/<pid>/stat), or None when it is not running."""
        if not isinstance(pid, int) or pid <= 0:
            return None
        return _proc_started(pid)

    def terminate(pid):
        """The hard stop of one process (SIGKILL, as Stop-Process -Force on Windows)."""
        import signal
        try:
            os.kill(pid, signal.SIGKILL)
            return True
        except OSError:
            return False

    def file_free(path):
        """Whether no process has `path` open any more (Linux never locks a file against writing, so the one sign that a
        closing game is done with its paks is that nothing holds them open)."""
        return not _proc_open_by_anyone(path)

    def process_problem():
        return None if os.path.isdir("/proc/self") else "no /proc: the game's processes can't be listed"

else:
    def _no_processes(*_a, **_k):
        raise Unsupported(f"the in-game harness can't list processes on {sys.platform}: it knows Windows and Linux")

    game_pids = process_started = terminate = file_free = _no_processes

    def process_problem():
        return f"the in-game harness can't list processes on {sys.platform}: it knows Windows and Linux"


def process_alive(pid, started=None):
    """Is `pid` the same live process that was created at `started`? (A pid is reused after its process exits: the
    start time tells a new process with an old number from the one the lock was bound to.)"""
    t = process_started(pid)
    if t is None:
        return False
    return started is None or not t or abs(t - started) < 2.0


def wait_exit(procs, timeout):
    """Wait up to `timeout` seconds for every (pid, started) in `procs` to end. True when none is left."""
    end = time.time() + timeout
    while True:
        left = [(p, s) for p, s in procs if process_alive(p, s)]
        if not left or time.time() >= end:
            return not left
        time.sleep(0.5)


# ---- display: Windows ----------------------------------------------------------------------------------------------

# US layout set-1 scancodes: the names every harness tool uses for keys
SC = {"esc": 0x01, "1": 0x02, "2": 0x03, "3": 0x04, "4": 0x05, "5": 0x06, "6": 0x07, "7": 0x08, "8": 0x09,
      "9": 0x0A, "0": 0x0B, "-": 0x0C, "=": 0x0D, "backspace": 0x0E, "tab": 0x0F, "q": 0x10, "w": 0x11,
      "e": 0x12, "r": 0x13, "t": 0x14, "y": 0x15, "u": 0x16, "i": 0x17, "o": 0x18, "p": 0x19, "[": 0x1A,
      "]": 0x1B, "enter": 0x1C, "ctrl": 0x1D, "a": 0x1E, "s": 0x1F, "d": 0x20, "f": 0x21, "g": 0x22,
      "h": 0x23, "j": 0x24, "k": 0x25, "l": 0x26, ";": 0x27, "'": 0x28, "grave": 0x29, "shift": 0x2A,
      "\\": 0x2B, "z": 0x2C, "x": 0x2D, "c": 0x2E, "v": 0x2F, "b": 0x30, "n": 0x31, "m": 0x32, ",": 0x33,
      ".": 0x34, "/": 0x35, "alt": 0x38, "space": 0x39, "f1": 0x3B, "f2": 0x3C, "f3": 0x3D, "f4": 0x3E,
      "f5": 0x3F, "f6": 0x40, "f7": 0x41, "f8": 0x42, "f9": 0x43, "f10": 0x44, "f11": 0x57, "f12": 0x58, "scrolllock": 0x46, "rshift": 0x36}


class _Win:
    name = "windows (SendInput, PrintWindow)"

    def __init__(self):
        import ctypes
        import ctypes.wintypes as wt
        self.ct, self.wt = ctypes, wt
        u = self.user32 = ctypes.WinDLL("user32", use_last_error=True)
        g = self.gdi32 = ctypes.WinDLL("gdi32", use_last_error=True)
        self.kernel32 = ctypes.WinDLL("kernel32")
        u.SetProcessDPIAware()
        # Typed signatures: handles are pointer-sized, and the default int return truncates them on 64-bit.
        for fn, res, args in [
            (u.GetDC, wt.HDC, [wt.HWND]),
            (u.ReleaseDC, ctypes.c_int, [wt.HWND, wt.HDC]),
            (g.CreateCompatibleDC, wt.HDC, [wt.HDC]),
            (g.CreateCompatibleBitmap, wt.HBITMAP, [wt.HDC, ctypes.c_int, ctypes.c_int]),
            (g.SelectObject, wt.HGDIOBJ, [wt.HDC, wt.HGDIOBJ]),
            (g.DeleteObject, wt.BOOL, [wt.HGDIOBJ]),
            (g.DeleteDC, wt.BOOL, [wt.HDC]),
            (g.BitBlt, wt.BOOL, [wt.HDC] + [ctypes.c_int] * 4 + [wt.HDC, ctypes.c_int, ctypes.c_int, wt.DWORD]),
            (u.PrintWindow, wt.BOOL, [wt.HWND, wt.HDC, wt.UINT]),
            (g.GetDIBits, ctypes.c_int, [wt.HDC, wt.HBITMAP, wt.UINT, wt.UINT, ctypes.c_void_p, ctypes.c_void_p, wt.UINT]),
        ]:
            fn.restype, fn.argtypes = res, args
        u.GetForegroundWindow.restype = wt.HWND
        u.GetAsyncKeyState.restype = ctypes.c_short

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

        self.KEYBDINPUT, self.MOUSEINPUT, self.INPUT = KEYBDINPUT, MOUSEINPUT, INPUT
        u.SendInput.argtypes = [wt.UINT, ctypes.POINTER(INPUT), ctypes.c_int]
        self.WNDENUMPROC = ctypes.WINFUNCTYPE(wt.BOOL, wt.HWND, wt.LPARAM)

    # input
    def _send(self, *inputs):
        arr = (self.INPUT * len(inputs))(*inputs)
        if self.user32.SendInput(len(inputs), arr, self.ct.sizeof(self.INPUT)) != len(inputs):
            raise OSError(f"SendInput blocked (error {self.ct.get_last_error()}); is an elevated window in front?")

    def _keyin(self, name, up):
        i = self.INPUT(type=1)
        i.u.ki = self.KEYBDINPUT(0, SC[name], 0x8 | (0x2 if up else 0), 0, 0)   # KEYEVENTF_SCANCODE (| KEYUP)
        return i

    def key(self, name, up):
        self._send(self._keyin(name, up))

    def keys(self, names, up):
        if names:
            self._send(*[self._keyin(n, up) for n in names])

    _BUTTONS = {1: (0x0002, 0x0004, 0), 2: (0x0008, 0x0010, 0), 3: (0x0020, 0x0040, 0), 4: (0x0080, 0x0100, 1),
                5: (0x0080, 0x0100, 2)}

    def _mouse(self, dx, dy, data, flags):
        i = self.INPUT(type=0)
        i.u.mi = self.MOUSEINPUT(dx, dy, data, flags, 0, 0)
        self._send(i)

    def button(self, n, up):
        d, u, data = self._BUTTONS[n]
        self._mouse(0, 0, data, u if up else d)

    def mouse_move(self, dx, dy):
        self._mouse(dx, dy, 0, 0x1)   # MOUSEEVENTF_MOVE

    def wheel(self, notches):
        for _ in range(abs(notches)):
            self._mouse(0, 0, self.ct.c_ulong(120 if notches > 0 else -120).value, 0x0800)   # MOUSEEVENTF_WHEEL
            time.sleep(0.05)

    _VK = {"ctrl": 0xA2, "shift": 0xA0, "rshift": 0xA1, "alt": 0xA4, "w": 0x57, "a": 0x41, "s": 0x53, "d": 0x44, "space": 0x20}

    def keys_down(self, names):
        return [k for k in names if k in self._VK and self.user32.GetAsyncKeyState(self._VK[k]) & 0x8000]

    # windows
    def find_window(self, title_part="Deadlock"):
        u, ct = self.user32, self.ct
        found = []

        def cb(hwnd, _):
            if u.IsWindowVisible(hwnd):
                n = u.GetWindowTextLengthW(hwnd)
                buf = ct.create_unicode_buffer(n + 1)
                u.GetWindowTextW(hwnd, buf, n + 1)
                cls = ct.create_unicode_buffer(256)
                u.GetClassNameW(hwnd, cls, 256)
                # the game window's class is SDL_app; skip Steam, Explorer and editor windows that merely mention it
                if title_part.lower() in buf.value.lower() and cls.value == "SDL_app":
                    found.append(hwnd)
            return True

        u.EnumWindows(self.WNDENUMPROC(cb), 0)
        return found[0] if found else None

    def window_pid(self, hwnd):
        if not hwnd:
            return 0
        pid = self.wt.DWORD()
        self.user32.GetWindowThreadProcessId(hwnd, self.ct.byref(pid))
        return pid.value

    def foreground(self):
        return self.user32.GetForegroundWindow()

    def in_front(self, hwnd):
        return bool(hwnd) and self.user32.GetForegroundWindow() == hwnd

    def bring_forward(self, hwnd, attempt):
        """One try at the foreground. Windows only lets the foreground thread hand focus over: join its input queue
        for the switch, and tap Alt (which lifts the foreground lock) as a second route, on the first few tries only
        (the Alt tap goes to whatever is in front: a few, not a stream into someone's window)."""
        u = self.user32
        fg = u.GetForegroundWindow()
        fg_thread = u.GetWindowThreadProcessId(fg, None) if fg else 0
        me = self.kernel32.GetCurrentThreadId()
        if fg_thread and fg_thread != me:
            u.AttachThreadInput(me, fg_thread, True)
        if attempt <= 3:
            self._send(self._keyin("alt", False), self._keyin("alt", True))
        u.ShowWindow(hwnd, 9)
        u.BringWindowToTop(hwnd)
        u.SetForegroundWindow(hwnd)
        if fg_thread and fg_thread != me:
            u.AttachThreadInput(me, fg_thread, False)

    def describe(self, hwnd):
        if not hwnd:
            return "nothing"
        t = self.ct.create_unicode_buffer(200)
        self.user32.GetWindowTextW(hwnd, t, 200)
        c = self.ct.create_unicode_buffer(200)
        self.user32.GetClassNameW(hwnd, c, 200)
        return f"{t.value[:60]!r} (class {c.value})"

    def client_rect(self, hwnd):
        r = self.wt.RECT()
        self.user32.GetClientRect(hwnd, self.ct.byref(r))
        pt = self.wt.POINT(0, 0)
        self.user32.ClientToScreen(hwnd, self.ct.byref(pt))
        return pt.x, pt.y, r.right - r.left, r.bottom - r.top

    def desktop_rect(self):
        m = self.user32.GetSystemMetrics
        return m(76), m(77), m(78), m(79)   # the virtual screen: every monitor

    def grab(self, x, y, w, h, hwnd=None):
        """BGRA rows, top-down. With hwnd: the window's own content through PrintWindow (PW_RENDERFULLCONTENT), so
        windows stacked over the game (always-on-top tools, the desktop app) don't end up in the frame. Without: a
        copy of the screen region."""
        u, g, ct, wt = self.user32, self.gdi32, self.ct, self.wt
        sdc = u.GetDC(None)
        mdc = g.CreateCompatibleDC(sdc)
        bmp = g.CreateCompatibleBitmap(sdc, w, h)
        old = g.SelectObject(mdc, bmp)
        if hwnd is None or not u.PrintWindow(hwnd, mdc, 0x3):   # PW_CLIENTONLY | PW_RENDERFULLCONTENT
            g.BitBlt(mdc, 0, 0, w, h, sdc, x, y, 0x00CC0020 | 0x40000000)   # SRCCOPY | CAPTUREBLT

        class BMI(ct.Structure):
            _fields_ = [("biSize", wt.DWORD), ("biWidth", wt.LONG), ("biHeight", wt.LONG), ("biPlanes", wt.WORD),
                        ("biBitCount", wt.WORD), ("biCompression", wt.DWORD), ("biSizeImage", wt.DWORD),
                        ("biXPelsPerMeter", wt.LONG), ("biYPelsPerMeter", wt.LONG), ("biClrUsed", wt.DWORD),
                        ("biClrImportant", wt.DWORD)]

        bmi = BMI(ct.sizeof(BMI), w, -h, 1, 32, 0, 0, 0, 0, 0, 0)
        buf = ct.create_string_buffer(w * h * 4)
        g.GetDIBits(mdc, bmp, 0, h, buf, ct.byref(bmi), 0)
        g.SelectObject(mdc, old)
        g.DeleteObject(bmp)
        g.DeleteDC(mdc)
        u.ReleaseDC(None, sdc)
        return buf.raw


# ---- display: X11 --------------------------------------------------------------------------------------------------

# console.py's key names as X keysyms
KEYSYM = {"esc": "Escape", "-": "minus", "=": "equal", "backspace": "BackSpace", "tab": "Tab", "[": "bracketleft",
          "]": "bracketright", "enter": "Return", "ctrl": "Control_L", ";": "semicolon", "'": "apostrophe",
          "grave": "grave", "shift": "Shift_L", "\\": "backslash", ",": "comma", ".": "period", "/": "slash",
          "alt": "Alt_L", "space": "space", "scrolllock": "Scroll_Lock", "rshift": "Shift_R",
          **{f"f{n}": f"F{n}" for n in range(1, 13)}}

# Proton names a game's window class after the app (steam_app_<id>); plain Wine names it after the program. Both are
# what the harness looks for; neither has been seen with Deadlock itself (README.md, Platforms)
GAME_WINDOW_CLASSES = (f"steam_app_{APP_ID}", GAME_EXE)


class _X11:
    name = "x11 (XTest, XGetImage)"

    def __init__(self, x11, xtst):
        import ctypes as ct
        self.ct = ct
        self.x, self.xt = x11, xtst
        P, UL, I, UI = ct.c_void_p, ct.c_ulong, ct.c_int, ct.c_uint

        def sig(lib, name, res, args):
            f = getattr(lib, name)
            f.restype, f.argtypes = res, args

        sig(x11, "XOpenDisplay", P, [ct.c_char_p])
        sig(x11, "XDefaultRootWindow", UL, [P])
        sig(x11, "XInternAtom", UL, [P, ct.c_char_p, I])
        sig(x11, "XGetWindowProperty", I, [P, UL, UL, ct.c_long, ct.c_long, I, UL, ct.POINTER(UL), ct.POINTER(I),
                                           ct.POINTER(UL), ct.POINTER(UL), ct.POINTER(P)])
        sig(x11, "XFree", I, [P])
        sig(x11, "XQueryTree", I, [P, UL, ct.POINTER(UL), ct.POINTER(UL), ct.POINTER(P), ct.POINTER(UI)])
        sig(x11, "XGetGeometry", I, [P, UL, ct.POINTER(UL), ct.POINTER(I), ct.POINTER(I), ct.POINTER(UI),
                                     ct.POINTER(UI), ct.POINTER(UI), ct.POINTER(UI)])
        sig(x11, "XTranslateCoordinates", I, [P, UL, UL, I, I, ct.POINTER(I), ct.POINTER(I), ct.POINTER(UL)])
        sig(x11, "XGetImage", P, [P, UL, I, I, UI, UI, UL, I])
        sig(x11, "XGetInputFocus", I, [P, ct.POINTER(UL), ct.POINTER(I)])
        sig(x11, "XSetInputFocus", I, [P, UL, I, UL])
        sig(x11, "XRaiseWindow", I, [P, UL])
        sig(x11, "XSendEvent", I, [P, UL, I, ct.c_long, P])
        sig(x11, "XSync", I, [P, I])
        sig(x11, "XFlush", I, [P])
        sig(x11, "XStringToKeysym", UL, [ct.c_char_p])
        sig(x11, "XKeysymToKeycode", ct.c_ubyte, [P, UL])
        sig(x11, "XQueryKeymap", I, [P, ct.c_char_p])
        sig(xtst, "XTestFakeKeyEvent", I, [P, UI, I, UL])
        sig(xtst, "XTestFakeButtonEvent", I, [P, UI, I, UL])
        sig(xtst, "XTestFakeRelativeMotionEvent", I, [P, I, I, UL])
        sig(xtst, "XTestQueryExtension", I, [P, ct.POINTER(I), ct.POINTER(I), ct.POINTER(I), ct.POINTER(I)])

        # Xlib's default error handler ends the process; a window that closes while it is looked at (BadWindow) is
        # an ordinary event here, so errors are noted and the call that met one answers "nothing"
        self._errors = []
        HANDLER = ct.CFUNCTYPE(I, P, P)
        self._handler = HANDLER(lambda _d, _e: self._errors.append(1) or 0)
        sig(x11, "XSetErrorHandler", P, [HANDLER])
        x11.XSetErrorHandler(self._handler)

        self.d = x11.XOpenDisplay(None)
        if not self.d:
            raise OSError(f"can't open the X display {os.environ.get('DISPLAY')!r}")
        a, b, c, e = I(), I(), I(), I()
        if not xtst.XTestQueryExtension(self.d, ct.byref(a), ct.byref(b), ct.byref(c), ct.byref(e)):
            raise OSError("the X server has no XTEST extension, so keys can't be sent")
        self.root = x11.XDefaultRootWindow(self.d)
        self._atoms = {}
        # One connection, used from several threads (takes.py moves the mouse on one while the frozen-hero watch grabs
        # on another): every call holds the lock, on top of XInitThreads
        import threading
        self._lock = threading.RLock()
        for n in ("find_window", "window_pid", "foreground", "in_front", "bring_forward", "describe", "client_rect",
                  "desktop_rect", "grab", "key", "keys", "button", "mouse_move", "wheel", "keys_down"):
            setattr(self, n, self._locked(getattr(self, n)))

    def _locked(self, f):
        def call(*a, **k):
            with self._lock:
                return f(*a, **k)
        return call

    def atom(self, name):
        if name not in self._atoms:
            self._atoms[name] = self.x.XInternAtom(self.d, name.encode(), 0)
        return self._atoms[name]

    def _prop(self, w, name, kind=0):
        """(format, bytes) of a window property, or None. `kind` 0 is AnyPropertyType."""
        ct = self.ct
        t, f, n, after, data = ct.c_ulong(), ct.c_int(), ct.c_ulong(), ct.c_ulong(), ct.c_void_p()
        del self._errors[:]
        ok = self.x.XGetWindowProperty(self.d, w, self.atom(name), 0, 1 << 16, 0, kind, ct.byref(t), ct.byref(f),
                                       ct.byref(n), ct.byref(after), ct.byref(data))
        if ok != 0 or self._errors or not data.value:
            return None
        try:
            # a 32-bit format property is held as C longs in memory, whatever the size of a long
            unit = {8: 1, 16: 2, 32: ct.sizeof(ct.c_long)}.get(f.value, 1)
            return f.value, ct.string_at(data.value, n.value * unit)
        finally:
            self.x.XFree(data)

    def _longs(self, w, name):
        p = self._prop(w, name)
        if not p or p[0] != 32:
            return []
        import struct
        size = self.ct.sizeof(self.ct.c_long)
        return list(struct.unpack(f"{len(p[1]) // size}{'q' if size == 8 else 'l'}", p[1]))

    def _text(self, w, name):
        p = self._prop(w, name)
        return p[1].decode("utf-8", "replace") if p and p[0] == 8 else ""

    def title(self, w):
        return self._text(w, "_NET_WM_NAME") or self._text(w, "WM_NAME")

    def wm_class(self, w):
        return [s for s in self._text(w, "WM_CLASS").split("\0") if s]

    def _children(self, w):
        ct = self.ct
        r, p, kids, n = ct.c_ulong(), ct.c_ulong(), ct.c_void_p(), ct.c_uint()
        del self._errors[:]
        if not self.x.XQueryTree(self.d, w, ct.byref(r), ct.byref(p), ct.byref(kids), ct.byref(n)) or self._errors:
            return [], 0
        try:
            out = list((ct.c_ulong * n.value).from_address(kids.value)) if kids.value and n.value else []
        finally:
            if kids.value:
                self.x.XFree(kids)
        return out, p.value

    def _parent(self, w):
        return self._children(w)[1]

    def _viewable(self, w):
        """Mapped, on a mapped parent, and bigger than a pixel (as IsWindowVisible asks on Windows)."""
        ct = self.ct

        class Attrs(ct.Structure):
            _fields_ = [("x", ct.c_int), ("y", ct.c_int), ("width", ct.c_int), ("height", ct.c_int),
                        ("border_width", ct.c_int), ("depth", ct.c_int), ("visual", ct.c_void_p), ("root", ct.c_ulong),
                        ("class_", ct.c_int), ("bit_gravity", ct.c_int), ("win_gravity", ct.c_int),
                        ("backing_store", ct.c_int), ("backing_planes", ct.c_ulong), ("backing_pixel", ct.c_ulong),
                        ("save_under", ct.c_int), ("colormap", ct.c_ulong), ("map_installed", ct.c_int),
                        ("map_state", ct.c_int), ("all_event_masks", ct.c_long), ("your_event_mask", ct.c_long),
                        ("do_not_propagate_mask", ct.c_long), ("override_redirect", ct.c_int), ("screen", ct.c_void_p)]

        a = Attrs()
        del self._errors[:]
        self.x.XGetWindowAttributes.restype = ct.c_int
        self.x.XGetWindowAttributes.argtypes = [ct.c_void_p, ct.c_ulong, ct.POINTER(Attrs)]
        if not w or not self.x.XGetWindowAttributes(self.d, w, ct.byref(a)) or self._errors:
            return False
        return a.map_state == 2 and a.width > 1 and a.height > 1   # IsViewable

    def _candidates(self):
        """Top-level client windows: the window manager's list when it keeps one, else every window in the tree."""
        listed = [w for w in self._longs(self.root, "_NET_CLIENT_LIST") if w]
        if listed:
            return listed
        out, todo = [], [self.root]
        while todo:
            kids, _ = self._children(todo.pop())
            out += kids
            todo += kids
        return out

    def find_window(self, title_part="Deadlock"):
        for w in self._candidates():
            cls = [c.lower() for c in self.wm_class(w)]
            if any(c in GAME_WINDOW_CLASSES for c in cls) and title_part.lower() in self.title(w).lower() and self._viewable(w):
                return w
        return None

    def window_pid(self, w):
        v = self._longs(w, "_NET_WM_PID") if w else []
        return v[0] if v else 0

    def foreground(self):
        v = self._longs(self.root, "_NET_ACTIVE_WINDOW")
        if v and v[0]:
            return v[0]
        ct = self.ct
        f, rv = ct.c_ulong(), ct.c_int()
        self.x.XGetInputFocus(self.d, ct.byref(f), ct.byref(rv))
        return f.value if f.value > 1 else 0   # None (0) and PointerRoot (1) are no window

    def in_front(self, w):
        """The focus is the window or inside it (an X client's focus can sit on a child of its top-level window)."""
        f = self.foreground()
        for _ in range(64):
            if not w or not f or f == self.root:
                return False
            if f == w:
                return True
            f = self._parent(f)
        return False

    def bring_forward(self, w, attempt):
        """One try at the foreground: ask the window manager (_NET_ACTIVE_WINDOW, as a pager would), raise it, and set
        the input focus on it."""
        ct = self.ct

        class ClientMessage(ct.Structure):
            _fields_ = [("type", ct.c_int), ("serial", ct.c_ulong), ("send_event", ct.c_int), ("display", ct.c_void_p),
                        ("window", ct.c_ulong), ("message_type", ct.c_ulong), ("format", ct.c_int),
                        ("data", ct.c_long * 5), ("pad", ct.c_long * 24)]

        ev = ClientMessage(type=33, send_event=1, display=self.d, window=w, message_type=self.atom("_NET_ACTIVE_WINDOW"),
                           format=32)
        ev.data[0], ev.data[1] = 2, 0   # source: a pager; CurrentTime
        del self._errors[:]
        self.x.XSendEvent(self.d, self.root, 0, (1 << 20) | (1 << 19), ct.byref(ev))   # SubstructureRedirect|Notify
        self.x.XRaiseWindow(self.d, w)
        self.x.XSetInputFocus(self.d, w, 2, 0)   # RevertToParent, CurrentTime
        self.x.XSync(self.d, 0)

    def describe(self, w):
        if not w:
            return "nothing"
        return f"{self.title(w)[:60]!r} (class {'/'.join(self.wm_class(w)) or '?'})"

    def client_rect(self, w):
        ct = self.ct
        r, x, y, width, height, bw, depth = ct.c_ulong(), ct.c_int(), ct.c_int(), ct.c_uint(), ct.c_uint(), ct.c_uint(), ct.c_uint()
        del self._errors[:]
        if not self.x.XGetGeometry(self.d, w, ct.byref(r), ct.byref(x), ct.byref(y), ct.byref(width), ct.byref(height),
                                   ct.byref(bw), ct.byref(depth)) or self._errors:
            return 0, 0, 0, 0
        rx, ry, child = ct.c_int(), ct.c_int(), ct.c_ulong()
        self.x.XTranslateCoordinates(self.d, w, self.root, 0, 0, ct.byref(rx), ct.byref(ry), ct.byref(child))
        return rx.value, ry.value, width.value, height.value

    def desktop_rect(self):
        return self.client_rect(self.root)

    def grab(self, x, y, w, h, win=None):
        """BGRA rows, top-down: the window's own pixels (XGetImage of the window: with a compositor these are the
        window's even under another; without one, a covered part reads as what covers it), or the screen region."""
        ct = self.ct

        class XImage(ct.Structure):
            _fields_ = [("width", ct.c_int), ("height", ct.c_int), ("xoffset", ct.c_int), ("format", ct.c_int),
                        ("data", ct.c_void_p), ("byte_order", ct.c_int), ("bitmap_unit", ct.c_int),
                        ("bitmap_bit_order", ct.c_int), ("bitmap_pad", ct.c_int), ("depth", ct.c_int),
                        ("bytes_per_line", ct.c_int), ("bits_per_pixel", ct.c_int), ("red_mask", ct.c_ulong),
                        ("green_mask", ct.c_ulong), ("blue_mask", ct.c_ulong), ("obdata", ct.c_void_p),
                        ("funcs", ct.c_void_p * 6)]

        src, sx, sy = (win, 0, 0) if win else (self.root, x, y)
        del self._errors[:]
        p = self.x.XGetImage(self.d, src, sx, sy, w, h, ct.c_ulong(-1).value, 2)   # AllPlanes, ZPixmap
        if not p or self._errors:
            raise OSError(f"XGetImage of {w}x{h} failed (the window closed, or is not on screen)")
        img = XImage.from_address(p)
        try:
            if img.bits_per_pixel != 32 or img.byte_order != 0 or img.red_mask != 0xFF0000 or img.blue_mask != 0xFF:
                raise OSError(f"an X image of {img.bits_per_pixel} bits a pixel (masks {img.red_mask:x}/{img.green_mask:x}/"
                              f"{img.blue_mask:x}, byte order {img.byte_order}) is not the 32-bit BGRX this reads")
            stride = img.bytes_per_line
            raw = ct.string_at(img.data, stride * h)
            if stride == w * 4:
                return raw
            return b"".join(raw[r * stride:r * stride + w * 4] for r in range(h))
        finally:
            destroy = ct.CFUNCTYPE(ct.c_int, ct.c_void_p)(img.funcs[1]) if img.funcs[1] else None   # f.destroy_image
            if destroy:
                destroy(p)

    # input
    def _keycode(self, name):
        sym = KEYSYM.get(name, name)
        ks = self.x.XStringToKeysym(sym.encode())
        code = self.x.XKeysymToKeycode(self.d, ks) if ks else 0
        if not code:
            raise ValueError(f"no X keycode for the key {name!r} (keysym {sym}) in this keyboard map")
        return code

    def key(self, name, up):
        self.xt.XTestFakeKeyEvent(self.d, self._keycode(name), 0 if up else 1, 0)
        self.x.XFlush(self.d)

    def keys(self, names, up):
        for n in names:
            self.xt.XTestFakeKeyEvent(self.d, self._keycode(n), 0 if up else 1, 0)
        self.x.XFlush(self.d)

    _BUTTONS = {1: 1, 2: 3, 3: 2, 4: 8, 5: 9}   # left, right, middle, back, forward as X numbers them

    def button(self, n, up):
        self.xt.XTestFakeButtonEvent(self.d, self._BUTTONS[n], 0 if up else 1, 0)
        self.x.XFlush(self.d)

    def mouse_move(self, dx, dy):
        self.xt.XTestFakeRelativeMotionEvent(self.d, dx, dy, 0)
        self.x.XFlush(self.d)

    def wheel(self, notches):
        b = 4 if notches > 0 else 5   # away from you is up
        for _ in range(abs(notches)):
            self.xt.XTestFakeButtonEvent(self.d, b, 1, 0)
            self.xt.XTestFakeButtonEvent(self.d, b, 0, 0)
            self.x.XFlush(self.d)
            time.sleep(0.05)

    def keys_down(self, names):
        buf = self.ct.create_string_buffer(32)
        self.x.XQueryKeymap(self.d, buf)
        down = []
        for k in names:
            try:
                code = self._keycode(k)
            except ValueError:
                continue
            if buf.raw[code // 8] & (1 << (code % 8)):
                down.append(k)
        return down


def _load_x11():
    """The X11 backend, or (None, why not)."""
    if not os.environ.get("DISPLAY"):
        if os.environ.get("WAYLAND_DISPLAY"):
            return None, ("a Wayland session with no DISPLAY: the harness drives the game through X11, so it needs "
                          "XWayland (DISPLAY set), which Proton's game window lives on anyway")
        return None, "no display: DISPLAY is not set (the harness needs the desktop the game is shown on)"
    import ctypes
    import ctypes.util

    def lib(name, soname):
        for cand in (ctypes.util.find_library(name), soname):
            if cand:
                try:
                    return ctypes.CDLL(cand)
                except OSError:
                    continue
        return None

    x11, xtst = lib("X11", "libX11.so.6"), lib("Xtst", "libXtst.so.6")
    if not x11:
        return None, "libX11 is not installed (libx11-6 on Debian and Ubuntu)"
    if not xtst:
        return None, "libXtst is not installed, so keys can't be sent (libxtst6 on Debian and Ubuntu)"
    x11.XInitThreads()   # before any other Xlib call
    try:
        return _X11(x11, xtst), None
    except OSError as e:
        return None, str(e)


_DISPLAY = {"done": False, "backend": None, "why": None}


def display():
    """The display backend, loaded once; None when this machine has none (`display_problem()` says why)."""
    if not _DISPLAY["done"]:
        _DISPLAY["done"] = True
        if WINDOWS:
            _DISPLAY["backend"] = _Win()
        elif LINUX:
            _DISPLAY["backend"], _DISPLAY["why"] = _load_x11()
        else:
            _DISPLAY["why"] = f"no display backend for {sys.platform}: the harness knows Windows and X11 on Linux"
    return _DISPLAY["backend"]


def display_problem():
    """Why the harness can't type into or look at the game here, or None when it can."""
    display()
    return _DISPLAY["why"]


def need_display():
    """The display backend, or refuse (Unsupported, exit 2) with the reason."""
    b = display()
    if b is None:
        raise Unsupported(f"the in-game harness can't drive the game here: {_DISPLAY['why']}")
    return b


# ---- Steam ---------------------------------------------------------------------------------------------------------

def steam_command():
    """The command that starts a game through Steam (`<it> -applaunch <id> ...`), or raises Unsupported naming what is
    missing. MAGICIAN_STEAM, then `steam =` in the config file, then: Windows, the SteamExe Steam records in the
    registry, else Program Files; Linux, `steam` on PATH, else the Flatpak's."""
    sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
    import gamepaths
    import shutil
    explicit = gamepaths._env("MAGICIAN_STEAM") or gamepaths.config("steam")
    if explicit:
        return explicit.split() if not os.path.exists(explicit) else [explicit]
    if WINDOWS:
        try:
            import winreg
            with winreg.OpenKey(winreg.HKEY_CURRENT_USER, r"Software\Valve\Steam") as k:
                exe = winreg.QueryValueEx(k, "SteamExe")[0]
                if exe and os.path.isfile(exe):
                    return [exe]
        except OSError:
            pass
        exe = os.path.join(os.environ.get("ProgramFiles(x86)", r"C:\Program Files (x86)"), "Steam", "steam.exe")
        if os.path.isfile(exe):
            return [exe]
        raise Unsupported("Steam is not found (no SteamExe in the registry, no Program Files copy): set MAGICIAN_STEAM")
    if shutil.which("steam"):
        return [shutil.which("steam")]
    if shutil.which("flatpak") and os.path.isdir(os.path.expanduser("~/.var/app/com.valvesoftware.Steam")):
        return [shutil.which("flatpak"), "run", "com.valvesoftware.Steam"]
    raise Unsupported("Steam is not found (no `steam` on PATH, no Flatpak Steam): set MAGICIAN_STEAM, or `steam =` in "
                      "the config file, to the command that starts it")


def spawn(cmd):
    """Start `cmd` detached from this process (it outlives the caller, as Start-Process does), output discarded."""
    kw = {"stdin": subprocess.DEVNULL, "stdout": subprocess.DEVNULL, "stderr": subprocess.DEVNULL, "close_fds": True}
    if WINDOWS:
        kw["creationflags"] = 0x00000008 | 0x00000200   # DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP
    else:
        kw["start_new_session"] = True
    return subprocess.Popen(cmd, **kw)


def main(argv):
    if argv[:1] in (["--help"], ["-h"], ["help"]):
        print(__doc__)
        return 0
    p, d = process_problem(), display_problem()
    print(f"processes  {'ok' if not p else p}")
    print(f"display    {display().name if display() else d}")
    try:
        print(f"steam      {' '.join(steam_command())}")
    except Unsupported as e:
        print(f"steam      {e}")
    return 2 if (p or d) else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))

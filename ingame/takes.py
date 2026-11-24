"""Timed takes: keys go down and up on a clock, filmed live (a `.plan`) or staged for a SourceTV take filmed later in
playback (a `.stage`).

    python -I takes.py live  <plan.plan>  <out dir> [--key KEY]
    python -I takes.py stage <plan.stage> <out dir> [--key KEY]

Why not console.py: its `@burst` grabs its frames and then encodes them, and a key held with `@down` stays down for
the whole encode (a run meant to last a second lasted four, left the sandbox street and ended in the hideout,
2026-11-23). Here the film runs in its own process and this one presses and releases on time, so a move is as long
as the plan says. A stage plan films nothing: each beat's wall-clock start goes into <out>/marks.tsv, and
cinema/replay.py films the take afterwards from free cameras, where a beat's demo tick is
(wall time - take start) * 64, calibrated once.

The caller holds the game seat (mod_session.py up, or launch.ps1) and the hero is selected; every key goes through
console.py's guard, so nothing reaches a game this holder did not launch.

Plan lines (blank lines and # comments skipped), both kinds:
    > console command                  typed through console.py's Console (about 1.5 s each)
    wait S
    shot NAME                          one capture.py frame into the out dir
    home [X Y Z PITCH YAW]             back to the home spot (the five numbers set it)
    cam SIDE|FRONT|BACK|Q3|LOW|HIGH|SIDE_L|TURN_A..D   the inspect orbit set to that angle (off for BACK)
`.plan` only:
    take NAME FRAMES FPS [full] : EVENT ...
`.stage` only:
    beat NAME SECONDS : EVENT ...      the events on a clock for SECONDS; nothing is captured
EVENT is T:ACTION, T seconds from the start, ACTION one of
    +key / -key        a real key down / up (w a s d space shift ctrl r e v f q ...; m1..m5 are mouse buttons)
    key                a tap (down, 60 ms, up)
    mDX,DY[,SECS]      a relative mouse move spread over SECS (default 0.25)
Every key still down at the end of a take or beat is released then, whatever the plan says.

A frozen hero is noticed: while a movement key (w a s d) has been held for over a second, two frames grabbed a
second apart that are the same picture mean the hero is not moving (a `setpos` spot inside collision held one hero
frozen for an hour of film, 2026-11-23). The take is marked FROZEN in its output and in <out>/frozen.tsv, and the
run exits 8 at the end.
"""
import ctypes
import os
import subprocess
import sys
import threading
import time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

MOVE_KEYS = ("w", "a", "s", "d")
MOUSE = {"m1": (0x0002, 0x0004, 0), "m2": (0x0008, 0x0010, 0), "m3": (0x0020, 0x0040, 0), "m4": (0x0080, 0x0100, 1),
         "m5": (0x0080, 0x0100, 2)}
# degrees are approximate: the orbit's mouse units per degree vary by session (README: 15.5 one day, 23 another)
CAMS = {"SIDE": (2100, 0), "FRONT": (4200, 0), "Q3": (3150, 0), "LOW": (4200, -260), "HIGH": (0, 500), "SIDE_L": (-2100, 0),
        "TURN_A": (700, 0), "TURN_B": (1400, 0), "TURN_C": (2800, 0), "TURN_D": (3500, 0)}


# ---- pure parts (tested without a game: test_takes.py) ---------------------------------------------------------------

def parse_events(text):
    """`0:+w 1.5:-w 2:space 2.1:m300,0,0.5` -> [(0.0, '+w'), ...], sorted by time (stable)."""
    out = []
    for e in text.split():
        t, sep, a = e.partition(":")
        if not sep or not a:
            raise ValueError(f"an event is T:ACTION, got {e!r}")
        out.append((float(t), a))
    return sorted(out, key=lambda x: x[0])


def parse_plan(text, kind):
    """The plan as steps: (op, args). kind is 'live' (.plan) or 'stage' (.stage). Raises ValueError on a line it does
    not know, before anything is typed into the game."""
    steps = []
    timed = "take" if kind == "live" else "beat"
    for n, raw in enumerate(text.splitlines(), 1):
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        w = line.split()
        if line.startswith(">"):
            steps.append(("cmd", line[1:].strip()))
        elif w[0] == "wait" and len(w) == 2:
            steps.append(("wait", float(w[1])))
        elif w[0] == "shot" and len(w) == 2:
            steps.append(("shot", w[1]))
        elif w[0] == "home" and len(w) in (1, 6):
            steps.append(("home", w[1:] or None))
        elif w[0] == "cam" and len(w) == 2 and (w[1] == "BACK" or w[1] in CAMS):
            steps.append(("cam", w[1]))
        elif w[0] == timed and ":" in line:
            head, _, ev = line.partition(":")
            h = head.split()
            if kind == "live":
                if len(h) < 4:
                    raise ValueError(f"line {n}: take NAME FRAMES FPS [full] : EVENT ...")
                steps.append(("take", {"name": h[1], "frames": int(h[2]), "fps": int(h[3]), "full": "full" in h[4:],
                                       "events": parse_events(ev)}))
            else:
                if len(h) != 3:
                    raise ValueError(f"line {n}: beat NAME SECONDS : EVENT ...")
                steps.append(("beat", {"name": h[1], "secs": float(h[2]), "events": parse_events(ev)}))
        else:
            raise ValueError(f"line {n}: unknown plan line for a {'.plan' if kind == 'live' else '.stage'}: {line}")
    have_home = False
    for op, arg in steps:
        if op == "home":
            if arg is None and not have_home:
                raise ValueError("the first `home` must give X Y Z PITCH YAW")
            have_home = True
        elif op == "cam" and not have_home:
            raise ValueError("`cam` needs a `home X Y Z PITCH YAW` before it")
    return steps


def held_spans(events, end):
    """Where a movement key is held: [(start, stop)] in seconds from the take's start, merged across keys."""
    down, spans = {}, []
    for t, a in events:
        if a[:1] in "+-" and a[1:] in MOVE_KEYS:
            if a[0] == "+":
                down.setdefault(a[1:], t)
            elif a[1:] in down:
                spans.append((down.pop(a[1:]), t))
    spans += [(t, end) for t in down.values()]
    spans.sort()
    merged = []
    for s, e in spans:
        if merged and s <= merged[-1][1]:
            merged[-1] = (merged[-1][0], max(merged[-1][1], e))
        else:
            merged.append((s, e))
    return merged


def probe_times(spans, settle=1.0, gap=1.0):
    """For each held span long enough, the two moments to grab: `settle` after the key went down (the hero is up to
    speed), and `gap` later, both inside the span."""
    return [(s + settle, s + settle + gap) for s, e in spans if e - s >= settle + gap + 0.05]


def same_picture(a, b, tolerance=0.5):
    """Two frames (raw pixel bytes of one size) are the same picture: their mean absolute difference, sampled, is
    under `tolerance` grey levels. A moving hero under a following camera changes the street behind it by far more."""
    if not a or not b or len(a) != len(b):
        return False
    step = max(1, len(a) // 200000) * 4 + 1     # odd stride: every channel gets sampled
    total = n = 0
    for i in range(0, len(a), step):
        total += abs(a[i] - b[i])
        n += 1
    return total / max(n, 1) < tolerance


# ---- driving the game ----------------------------------------------------------------------------------------------

class Driver:
    def __init__(self, out, key=None):
        import console as C
        self.C = C
        self.out = out
        self.con = C.Console(key)
        self.down = set()
        self.home = None
        self.frozen = []

    # input
    def send_mouse(self, dx, dy, data, flags):
        C = self.C
        i = C.INPUT(type=0)
        i.u.mi = C.MOUSEINPUT(dx, dy, data, flags, 0, 0)
        ctypes.windll.user32.SendInput(1, ctypes.byref(i), ctypes.sizeof(i))

    def key(self, name, up):
        if name in MOUSE:
            d, u, data = MOUSE[name]
            self.send_mouse(0, 0, data, u if up else d)
        else:
            self.C._send(self.C._key(self.C.SC[name], up))
        (self.down.discard if up else self.down.add)(name)

    def move(self, dx, dy, secs=0.25):
        """A relative mouse move in steps of at most 3 counts. Windows' pointer acceleration multiplies an injected move
        of 6 counts or more ("enhance pointer precision") by an amount that depends on how the events fall in time: the
        same 2100-count orbit was a side view in one session and a three-quarter back view in the next (2026-11-23).
        Steps under the threshold are not accelerated, so the same counts are the same angle."""
        n = max(1, int(max(abs(dx), abs(dy)) / 3) + 1)
        step = max(secs / n, 0.0015)
        ax = ay = 0
        t = time.perf_counter()
        for k in range(1, n + 1):
            tx, ty = round(dx * k / n), round(dy * k / n)
            if tx != ax or ty != ay:
                self.send_mouse(tx - ax, ty - ay, 0, 0x0001)
            ax, ay = tx, ty
            t += step
            while time.perf_counter() < t:
                pass

    def release_all(self):
        for k in list(self.down):
            self.key(k, True)

    def act(self, a):
        if a[0] == "+":
            self.key(a[1:], False)
        elif a[0] == "-":
            self.key(a[1:], True)
        elif a[0] == "m" and "," in a:
            v = a[1:].split(",")
            threading.Thread(target=self.move, args=(int(v[0]), int(v[1]), float(v[2]) if len(v) > 2 else 0.25),
                             daemon=True).start()
        else:
            self.key(a, False)
            time.sleep(0.06)
            self.key(a, True)

    # the clock
    def play(self, name, events, length, t0):
        """Fire `events` on the clock from t0 (perf_counter) for `length` s, watching for a frozen hero."""
        watch = FrozenWatch(self.C, probe_times(held_spans(events, length), 1.0, 1.0), t0)
        watch.start()
        try:
            for t, a in events:
                while time.perf_counter() < t0 + t:
                    time.sleep(0.002)
                self.act(a)
            while time.perf_counter() < t0 + length:
                time.sleep(0.005)
        finally:
            self.release_all()
            watch.join()
        if watch.frozen:
            self.frozen.append((name, watch.frozen))
            print(f"FROZEN {name}: two frames {watch.frozen[0][1] - watch.frozen[0][0]:.1f} s apart are the same "
                  f"picture at {watch.frozen[0][0]:.1f} s while a movement key is held (a spot inside collision?)",
                  flush=True)

    def take(self, spec):
        self.C.ensure_focus()
        cmd = [sys.executable, "-I", os.path.join(HERE, "capture.py"), "burst", os.path.join(self.out, spec["name"]),
               "--frames", str(spec["frames"]), "--fps", str(spec["fps"])] + (["--full"] if spec["full"] else [])
        p = subprocess.Popen(cmd, stdout=subprocess.DEVNULL)
        t0 = time.perf_counter() + 0.30          # the capture process needs about this long to start grabbing
        try:
            self.play(spec["name"], spec["events"], spec["frames"] / spec["fps"], t0)
        finally:
            p.wait()

    def beat(self, spec, mark):
        self.C.ensure_focus()
        mark(spec["name"], spec["secs"])
        self.play(spec["name"], spec["events"], spec["secs"], time.perf_counter())

    def place(self, home):
        self.con.run("citadel_camera_inspect_mode 0;setpos %s %s %s;setang %s %s 0" % tuple(home))

    def run(self, steps, kind):
        C = self.C
        C.release_stuck()
        C.focus_game()
        marks = open(os.path.join(self.out, "marks.tsv"), "a", encoding="utf-8") if kind == "stage" else None

        def mark(name, secs=0.0):
            marks.write("%.3f\t%s\t%.2f\n" % (time.time(), name, secs))
            marks.flush()
        try:
            for op, arg in steps:
                if op == "cmd":
                    self.con.run(arg)
                    time.sleep(0.25)
                elif op == "wait":
                    time.sleep(arg)
                elif op == "shot":
                    C.ensure_focus()
                    subprocess.run([sys.executable, "-I", os.path.join(HERE, "capture.py"), "shot",
                                    os.path.join(self.out, arg + ".png")], stdout=subprocess.DEVNULL)
                elif op == "home":
                    if arg:
                        self.home = arg
                    self.place(self.home)
                    time.sleep(0.8)
                    if marks:
                        mark("home")
                    # ("setpos into world" is printed on good spots too; it is not a test: the frozen-hero watch is)
                elif op == "cam":
                    self.place(self.home)
                    time.sleep(0.6)
                    if arg != "BACK":
                        self.con.run("citadel_camera_inspect_mode 1")
                        time.sleep(0.7)
                        dx, dy = CAMS[arg]
                        if dx:
                            self.move(dx, 0, 0.8)
                            time.sleep(0.2)
                        if dy:
                            self.move(0, dy, 0.5)
                            time.sleep(0.2)
                elif op == "take":
                    self.take(arg)
                    print("take", arg["name"], flush=True)
                elif op == "beat":
                    self.beat(arg, mark)
                    print("beat", arg["name"], flush=True)
        finally:
            self.release_all()
            C.release_held()
            if marks:
                marks.close()
        if self.frozen:
            with open(os.path.join(self.out, "frozen.tsv"), "a", encoding="utf-8") as f:
                for name, hits in self.frozen:
                    f.write("%s\t%s\n" % (name, " ".join("%.1f-%.1f" % h for h in hits)))


class FrozenWatch(threading.Thread):
    """Grabs the game window at each probe pair's two moments (in memory, no encode) and compares them."""

    def __init__(self, C, pairs, t0):
        super().__init__(daemon=True)
        self.C, self.pairs, self.t0 = C, pairs, t0
        self.frozen = []

    def grab(self):
        from capture import find_window, client_rect, grab
        hwnd = find_window()
        if not hwnd:
            return None
        x, y, w, h = client_rect(hwnd)
        return grab(x, y, w, h, hwnd)

    def at(self, t):
        while time.perf_counter() < self.t0 + t:
            time.sleep(0.003)
        return self.grab()

    def run(self):
        for a, b in self.pairs:
            first, second = self.at(a), self.at(b)
            if same_picture(first, second):
                self.frozen.append((a, b))


def main(a):
    if len(a) < 3 or a[0] not in ("live", "stage"):
        sys.exit(__doc__)
    kind, plan, out = a[0], a[1], a[2]
    key = a[a.index("--key") + 1] if "--key" in a else None   # console.py's console_key()
    try:
        steps = parse_plan(open(plan, encoding="utf-8-sig").read(), kind)
    except (OSError, ValueError) as e:
        print(f"takes: {plan}: {e}")
        return 2
    os.makedirs(out, exist_ok=True)
    d = Driver(out, key)
    d.run(steps, kind)
    print(("STAGE_DONE " if kind == "stage" else "TAKES_DONE ") + out)
    return 8 if d.frozen else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))

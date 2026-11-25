"""One in-game session for any mod with a mod.toml: take the seat, install the mod's pak, launch, wait for the game,
select the hero, run scenarios and timed takes, film a SourceTV take from free cameras, stop the game, give the seat
back, cut contact sheets. Standard library only; no Git Bash.

    python -I mod_session.py up     --out DIR [--mod DIR|mod.toml] [--pak FILE [--as pakNN_dir.vpk]] [--vanilla] [--restore]
                                [--holder NAME] [--map NAME | --menu] [--hero HERO | --no-hero] [--no-tv] [--wait SECS]
                                [--size 1920x1080] [--timeout SECS] [--key KEY]
    python -I mod_session.py run    --out DIR <item>... [--no-probe]
    python -I mod_session.py film   --out DIR <shots.txt> [--take FILE.dem] [--marks marks.tsv] [--w0 SECS] [--load]
                                [--actor NAME] [--fps 30]
    python -I mod_session.py down   --out DIR
    python -I mod_session.py sheets --out DIR
    python -I mod_session.py all    --out DIR [up options] <item>...      up, run, down, sheets; the seat always comes back
    python -I mod_session.py qa     --out DIR [up options] [<item>...]    all, then crash dumps, the mod's warnings and
                                                                      logdiff.py into <out>/index.md
    python -I mod_session.py where  [--mod DIR] <item>                    which file an item resolves to

The mod is --mod, else the mod.toml in the working folder or above it. From it: the hero to select ([mod] hero), the
ledger owner its paks install under (the slug with '-' as '_'), and an optional [ingame] table:

    [ingame]
    holder = "my-skin"                 # the game-seat holder (default: $MAGICIAN_GAME_HOLDER, else the slug)
    pass   = ["angles", "motion"]      # what `qa` runs when no item is given
    map    = "new_player_basics"       # the map `up` loads (default new_player_basics; --menu for none)

An item is a console.py scenario (`.txt`), a live timed plan (`.plan`: keys on a clock while capture.py films) or a
stage plan (`.stage`: a SourceTV take's beats, filmed afterwards with `film`), see takes.py. It is a path, or a name
looked up in the mod's own folders first (source/ingame/scenarios, source/ingame/plans, source/ingame/takes,
ingame/scenarios, scenarios), then in this toolkit's ingame/scenarios (the stock-hero examples). Each item writes into
<out>/<name>/ with its console output (run_<name>.log) and its slice of console.log (console_<name>.txt).

State between the verbs is <out>/session.json. Every verb after `up` first checks that the seat is still this
session's and the running game the one its launch bound (gamelock.may_drive): if not, it does nothing and exits 9.
`up --pak` installs the pak over the mod's slot (`magician install --as`), `--vanilla` uninstalls the mod's paks for a
vanilla pass, and `--restore` puts back at `down` exactly the paks that were there (copies are kept in <out>/restore/;
`down` compares `magician install --status` with the one taken before). Without --pak or --vanilla nothing is
installed: what is in the addons folder is what is filmed (listed in <out>/installed_before.txt).

Exit codes: 0 ok, 1 failed (said why), 2 usage or no mod, 3 the seat is someone else's, 4 install failed, 5 a game
this session did not start is running, 6 the demo ended before the film did, 7 the hero never loaded, 8 a frozen hero
(takes.py), 9 the seat or game is no longer this session's.
"""
import glob
import json
import os
import re
import shutil
import subprocess
import sys
import time
import zlib

HERE = os.path.dirname(os.path.abspath(__file__))
TOOLKIT = os.path.dirname(HERE)
sys.path.insert(0, HERE)
import gamelock  # noqa: E402
import gamepaths  # noqa: E402
import logdiff  # noqa: E402

TV = ["+tv_enable", "1", "+tv_delay", "0", "+tv_transmitall", "1", "+tv_snapshotrate", "64"]
SCENARIO_DIRS = ("source/ingame/scenarios", "source/ingame/plans", "source/ingame/takes", "ingame/scenarios", "scenarios")
KINDS = (".txt", ".plan", ".stage")


class Stop(Exception):
    """A verb stops here with an exit code and a reason (said once, by main)."""

    def __init__(self, code, why):
        super().__init__(why)
        self.code = code


def say(*a):
    print("session:", *a, flush=True)


# ---- the mod -------------------------------------------------------------------------------------------------------

def find_mod_toml(start):
    """`start` if it is a mod.toml, its mod.toml if a folder, else the first mod.toml in a folder above it."""
    p = os.path.abspath(start)
    if os.path.isfile(p):
        return p
    while True:
        if os.path.isfile(os.path.join(p, "mod.toml")):
            return os.path.join(p, "mod.toml")
        up = os.path.dirname(p)
        if up == p:
            return None
        p = up


def read_mod(path):
    import tomllib
    with open(path, "rb") as f:
        d = tomllib.load(f)
    m = d.get("mod", {})
    slug = m.get("slug") or os.path.basename(os.path.dirname(os.path.abspath(path)))
    ing = d.get("ingame", {}) if isinstance(d.get("ingame"), dict) else {}
    passes = ing.get("pass", [])
    return {"toml": os.path.abspath(path), "dir": os.path.dirname(os.path.abspath(path)), "slug": slug,
            "owner": slug.replace("-", "_"), "owners": [slug.replace("-", "_")] + list(m.get("owners", [])),
            "hero": m.get("hero") or None, "holder": ing.get("holder"), "map": ing.get("map"),
            "pass": [passes] if isinstance(passes, str) else list(passes)}


def resolve_item(name, mod_dir=None):
    """The file an item names: a path, else the mod's own folders, else the toolkit's examples. None when absent."""
    if os.path.isfile(name):
        return os.path.abspath(name)
    roots = [os.path.join(mod_dir, d) for d in SCENARIO_DIRS] if mod_dir else []
    roots.append(os.path.join(HERE, "scenarios"))
    tries = [name] if name.endswith(KINDS) else [name + k for k in KINDS]
    for r in roots:
        for t in tries:
            p = os.path.join(r, t)
            if os.path.isfile(p):
                return os.path.abspath(p)
    return None


def item_name(path):
    return os.path.splitext(os.path.basename(path))[0]


# ---- the install ---------------------------------------------------------------------------------------------------

STATUS_ROW = re.compile(r"^\s+(\S+)\s+crc ([0-9a-f]{8})\s+for (\S+)\s+(\S+)", re.M)


def status_rows(text):
    """`magician install --status` -> [(name, crc, owner, state)]."""
    return STATUS_ROW.findall(text)


def slot_of(rows, owner):
    """The pak slot `owner` has installed (pakNN_dir.vpk), or None."""
    for name, _, own, _ in rows:
        if own == owner and re.fullmatch(r"pak\d\d_dir\.vpk", name):
            return name
    return None


def crc_of(path):
    c = 0
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            c = zlib.crc32(chunk, c)
    return "%08x" % c


def magician_exe():
    return gamepaths.magician()


def magician(*args):
    r = subprocess.run([magician_exe(), *args], capture_output=True, text=True)
    return r.returncode, r.stdout + r.stderr


def install_status():
    rc, out = magician("install", "--status")
    if rc != 0:
        raise Stop(1, "magician install --status failed: " + out.strip()[-300:])
    return out


def addons_dir():
    return os.path.join(gamepaths.citadel(), "addons")


def install_for(st, pak, slot, holder, restore):
    """Install `pak` as `slot` for the mod's owner; with `restore`, keep a copy of what that slot held first."""
    before = status_rows(st["installed_before"])
    if restore:
        keep_copies(st, [r for r in before if r[0] == slot])
    want = crc_of(pak)
    rc, out = magician("install", pak, "--as", slot, "--mod", st["owner"], "--holder", holder)
    say(out.strip().splitlines()[-1] if out.strip() else f"install rc {rc}")
    now = status_rows(install_status())
    if not any(r[0] == slot and r[1] == want for r in now):
        raise Stop(4, f"install failed: {slot} does not hold {want} ({os.path.basename(pak)})")


def keep_copies(st, rows):
    """Copy the installed paks in `rows` to <out>/restore/ and remember them for `down`."""
    d = os.path.join(st["out"], "restore")
    os.makedirs(d, exist_ok=True)
    for name, crc, owner, _ in rows:
        src = os.path.join(addons_dir(), name.replace("/", os.sep))
        dst = os.path.join(d, name.replace("/", "__"))
        shutil.copy2(src, dst)
        if crc_of(dst) != crc:
            raise Stop(4, f"the copy of {name} kept for --restore does not match its ledger crc {crc}")
        st.setdefault("restore", []).append({"name": name, "crc": crc, "owner": owner, "copy": dst})


def uninstall_mod(st, holder, restore):
    rows = [r for r in status_rows(st["installed_before"]) if r[2] in st["owners"] and re.fullmatch(r"pak\d\d_dir\.vpk", r[0])]
    if restore:
        keep_copies(st, rows)
    for name, _, _, _ in rows:
        rc, out = magician("uninstall", name, "--holder", holder)
        say(out.strip().splitlines()[-1] if out.strip() else f"uninstall rc {rc}")
    left = [r for r in status_rows(install_status()) if r[0] in {x[0] for x in rows}]
    if left:
        raise Stop(4, "uninstall failed: still installed: " + ", ".join(r[0] for r in left))


def restore_installs(st, holder):
    """Put back every pak kept at `up` (as the same slot, for the same owner), then compare the status with before."""
    for r in st.get("restore", []):
        rc, out = magician("install", r["copy"], "--as", r["name"], "--mod", r["owner"], "--holder", holder)
        say("restore: " + (out.strip().splitlines()[-1] if out.strip() else f"rc {rc}"))
    want = {(n, c, o) for n, c, o, _ in status_rows(st["installed_before"])}
    have = {(n, c, o) for n, c, o, _ in status_rows(install_status())}
    if want != have:
        raise Stop(1, "the install is not as it was: missing " + ", ".join(sorted(f"{n} {c}" for n, c, _ in want - have))
                   + "; extra " + ", ".join(sorted(f"{n} {c}" for n, c, _ in have - want)))
    say("restore: the install is exactly as it was before the session")


# ---- state ---------------------------------------------------------------------------------------------------------

def state_path(out):
    return os.path.join(out, "session.json")


def load(out):
    try:
        with open(state_path(out), encoding="utf-8") as f:
            return json.load(f)
    except (OSError, ValueError):
        raise Stop(2, f"no session in {out} (run `mod_session.py up --out {out}` first)")


def save(st):
    tmp = state_path(st["out"]) + ".tmp"
    with open(tmp, "w", encoding="utf-8") as f:
        json.dump(st, f, indent=1)
    os.replace(tmp, state_path(st["out"]))


def ours(st):
    """Raise Stop(9) unless the seat is this session's and the running game the one its launch bound."""
    lock = gamelock.read()
    why = gamelock.may_drive(lock, st["holder"], gamelock.running_game(lock))
    if why:
        raise Stop(9, f"NOT OURS: {why}; nothing done")


def tool(script, *args, timeout=None):
    """One of this folder's tools (launch.py, stop_game.py) in this Python, its output kept."""
    return subprocess.run([sys.executable, "-I", os.path.join(HERE, script)] + [str(a) for a in args],
                          capture_output=True, text=True, timeout=timeout)


def last_line(text, default="(no output)"):
    lines = [x for x in (text or "").strip().splitlines() if x.strip()]
    return lines[-1] if lines else default


# ---- the verbs -----------------------------------------------------------------------------------------------------

def up(o):
    toml = find_mod_toml(o.get("mod") or os.getcwd())
    if not toml:
        raise Stop(2, "no mod.toml here or above (give --mod DIR)")
    mod = read_mod(toml)
    holder = o.get("holder") or os.environ.get("MAGICIAN_GAME_HOLDER") or mod["holder"] or mod["slug"]
    os.environ["MAGICIAN_GAME_HOLDER"] = holder
    out = os.path.abspath(o["out"])
    os.makedirs(out, exist_ok=True)
    hero = None if o.get("no-hero") else (o.get("hero") or mod["hero"])
    if hero and not hero.startswith("hero_"):
        hero = "hero_" + hero.lower()
    st = {"out": out, "holder": holder, "mod": mod["toml"], "mod_dir": mod["dir"], "slug": mod["slug"],
          "owner": mod["owner"], "owners": mod["owners"], "hero": hero, "key": o.get("key"), "runs": [], "up": False}
    say(f"mod {mod['slug']} ({mod['toml']}), holder {holder}, hero {hero or 'none'}")
    import gameos
    why = gameos.process_problem() or gameos.display_problem()
    if why:   # before the seat is taken or anything is installed: a session that can't type or look is no session
        raise Stop(2, f"can't drive the game on this machine: {why}")
    try:
        gameos.steam_command()
    except gameos.Unsupported as e:
        raise Stop(2, f"can't launch the game on this machine: {e}")
    if not gamelock.acquire(holder, f"session {mod['slug']}", float(o.get("wait", 1800)), launching=True, say=say):
        raise Stop(3, "the game seat is someone else's (see above)")
    st["held"] = True
    save(st)
    try:
        _up(st, o, mod)
    except BaseException:
        if st.get("held"):
            say("up failed: handing the seat back")
            try:
                down_state(st)
            except Stop as e:
                say(f"hand-back: {e}")
        raise
    return 0


def _up(st, o, mod):
    holder = st["holder"]
    t = time.time()
    while gamelock.game_running() and time.time() - t < 60:   # a previous holder's game still exiting
        time.sleep(2)
    if gamelock.game_running():
        gamelock.release(holder)
        st["held"] = False
        save(st)
        raise Stop(5, "a game this session did not start is running (stop it first, or wait for its holder)")
    st["installed_before"] = install_status()
    with open(os.path.join(st["out"], "installed_before.txt"), "w", encoding="utf-8") as f:
        f.write(st["installed_before"])
    rows = status_rows(st["installed_before"])
    for name, crc, owner, state in rows:
        say(f"installed {name} {crc} {owner}")
    if o.get("pak"):
        slot = o.get("as") or slot_of(rows, st["owner"])
        if not slot:
            raise Stop(2, f"{st['owner']} has no pak installed: say which slot with --as pakNN_dir.vpk")
        install_for(st, os.path.abspath(o["pak"]), slot, holder, bool(o.get("restore")))
    elif o.get("vanilla"):
        uninstall_mod(st, holder, bool(o.get("restore")))
        st["vanilla"] = True
    save(st)
    extra = [] if o.get("no-tv") or o.get("menu") else list(TV)
    game_map = None if o.get("menu") else (o.get("map") or mod["map"] or "new_player_basics")
    if game_map:
        extra += ["+map", game_map]
    st["map"] = game_map
    st["build"] = gamepaths.build()
    w, h = (o.get("size") or "1920x1080").lower().split("x")
    st["launched_at"] = time.time()
    save(st)
    say("launching " + " ".join(extra))
    r = tool("launch.py", "--holder", holder, "--timeout", o.get("timeout", 900), "--size", f"{w}x{h}",
             *(["--extra", " ".join(extra)] if extra else []))
    say(last_line(r.stdout))
    if r.returncode == 3:
        st["held"] = False
        save(st)
        raise Stop(3, "the seat was taken during the launch")
    if r.returncode == 4:
        st["held"] = False
        save(st)
        raise Stop(5, "a game this session did not start is running")
    if r.returncode != 0:
        raise Stop(1, f"launch failed (rc {r.returncode})")
    lock = gamelock.read()
    st["game_pid"] = lock.get("game_pid") if lock else None
    save(st)
    ours(st)
    import console as C
    if game_map:
        if not C.wait_log("ChangeGameState: GameInProgress", 300, 0):
            raise Stop(1, "the map never reached GameInProgress")
        time.sleep(3)
    con = C.Console(st["key"])
    deadline = time.time() + (120 if not game_map else 30)   # the menu answers once it has finished loading
    while True:
        C.release_stuck()
        C.focus_game()
        try:
            con.probe()
            break
        except RuntimeError:
            if time.time() > deadline:
                raise Stop(1, "the console does not answer")
            say("the console does not answer yet; waiting")
            time.sleep(10)
    if st["hero"] and game_map:
        start = C.log_size()
        con.run("sv_cheats 1")
        con.run("selecthero " + st["hero"])
        loaded = re.compile(r"Loaded hero \d+/" + re.escape(st["hero"]) + r"\b")
        t = time.time()
        while time.time() - t < 360 and not loaded.search(C.log_since(start)):   # a cold load took 4 minutes
            time.sleep(2)
        if not loaded.search(C.log_since(start)):
            shot(os.path.join(st["out"], "harness_fail_enter.png"))
            raise Stop(7, f"{st['hero']} never loaded")
        say(f"{st['hero']} loaded")
    st["take"] = newest_take(st["launched_at"]) if not (o.get("no-tv") or o.get("menu")) else None
    st["up"] = True
    save(st)
    say(f"UP: holder {holder}, game pid {st['game_pid']}" + (f", take {st['take']}" if st["take"] else ""))


def shot(path):
    subprocess.run([sys.executable, "-I", os.path.join(HERE, "capture.py"), "shot", path], capture_output=True)


def newest_take(since):
    files = [f for f in glob.glob(os.path.join(gamepaths.citadel(), "replays", "*.dem")) if os.path.getmtime(f) >= since - 5]
    files.sort(key=os.path.getmtime)
    return files[-1] if files else None


def run(o, items):
    st = load(o["out"])
    os.environ["MAGICIAN_GAME_HOLDER"] = st["holder"]
    import console as C
    worst = 0
    for it in items:
        ours(st)
        path = resolve_item(it, st["mod_dir"])
        if not path:
            raise Stop(2, f"no scenario, plan or stage {it!r} (looked in the mod's own folders and {os.path.join(HERE, 'scenarios')})")
        n = item_name(path)
        d = os.path.join(st["out"], n)
        os.makedirs(d, exist_ok=True)
        off = C.log_size()
        key = ["--key", st["key"]] if st.get("key") else []
        if path.endswith((".plan", ".stage")):
            cmd = [sys.executable, "-I", os.path.join(HERE, "takes.py"), "live" if path.endswith(".plan") else "stage", path, d] + key
        else:
            cmd = ([sys.executable, "-I", os.path.join(HERE, "console.py"), "--holder", st["holder"], "--script", path, "--out", d]
                   + key + (["--no-probe"] if o.get("no-probe") else []))
        with open(os.path.join(st["out"], f"run_{n}.log"), "w", encoding="utf-8") as f:
            rc = subprocess.run(cmd, stdout=f, stderr=subprocess.STDOUT).returncode
        with open(os.path.join(st["out"], f"console_{n}.txt"), "w", encoding="utf-8") as f:
            f.write(C.log_since(off))
        with open(os.path.join(st["out"], f"run_{n}.log"), encoding="utf-8", errors="replace") as f:
            tail = last_line(f.read())
        say(f"== {n}: {'ok' if rc == 0 else f'rc {rc}'}: {tail}")
        st = load(o["out"])
        st["runs"].append({"item": path, "name": n, "rc": rc})
        if path.endswith(".stage"):
            st["marks"] = os.path.join(d, "marks.tsv")
        save(st)
        if rc == 3:
            raise Stop(9, f"{n}: the game refused this holder (see run_{n}.log)")
        worst = max(worst, 8 if rc == 8 else (1 if rc else 0))
    return worst


def film(o, shots):
    st = load(o["out"])
    os.environ["MAGICIAN_GAME_HOLDER"] = st["holder"]
    ours(st)
    take = o.get("take") or st.get("take")
    if not take:
        raise Stop(2, "no take: give --take (this session recorded none)")
    marks = o.get("marks") or st.get("marks") or os.path.join(st["out"], "marks.tsv")
    actor = o.get("actor") or hero_display(st["hero"])
    cmd = [sys.executable, "-I", os.path.join(TOOLKIT, "cinema", "replay.py"), "film", take, marks, str(o.get("w0", 0)),
           shots, os.path.join(st["out"], "film")] + (["--actor", actor] if actor else []) + (["--load"] if o.get("load") else [])
    cmd += ["--fps", str(o.get("fps", 30))] + (["--key", st["key"]] if st.get("key") else [])
    rc = subprocess.run(cmd).returncode
    if rc == 6:
        raise Stop(6, "the demo ended before the film did (see above)")
    return 0 if rc == 0 else 1


def hero_display(hero):
    """The hero's display name as track.py names actors (Haze, Lash ...), from `magician footprint`."""
    if not hero:
        return None
    rc, out = magician("footprint", hero)
    m = re.match(r"\s*(.+?) \(" + re.escape(hero) + r"\)", out)
    return m.group(1) if rc == 0 and m else None


def down(o):
    st = load(o["out"])
    return down_state(st)


def down_state(st):
    holder = st["holder"]
    os.environ["MAGICIAN_GAME_HOLDER"] = holder
    lock = gamelock.read()
    if lock is None or lock["holder"] != holder:
        st["held"] = False
        save(st)
        if st.get("restore"):
            raise Stop(9, f"the seat is no longer {holder}'s: the kept paks in {os.path.join(st['out'], 'restore')} were NOT put back")
        say("the seat is not this session's any more; nothing to stop")
        return 0
    try:
        shutil.copy2(gamepaths.console_log(), os.path.join(st["out"], "console_full.log"))
    except OSError:
        pass
    if not gamelock.acquire(holder, "session hand-back", 0, hold=600, say=say):
        raise Stop(9, "the seat was taken during the hand-back")
    if gamelock.game_running():
        r = tool("stop_game.py", "--holder", holder, "--keep-lock", timeout=360)
        say(last_line(r.stdout))
        if r.returncode == 3:
            raise Stop(9, "stop_game refused: the running game is not this session's (left alone)")
    t = time.time()
    while gamelock.bound_alive(gamelock.read()) and time.time() - t < 120:
        gamelock.acquire(holder, "session hand-back", 0, hold=600, say=say)
        time.sleep(3)
    if gamelock.bound_alive(gamelock.read()):
        raise Stop(1, "GAME STILL RUNNING: the seat is kept; stop it with stop_game.py --holder " + holder)
    if st.get("restore"):
        restore_installs(st, holder)
    if not gamelock.release(holder, say=say):
        raise Stop(1, "the seat was not released (see above)")
    st["held"] = False
    st["up"] = False
    save(st)
    say(f"DOWN: game closed, seat released ({'free' if gamelock.is_stale(gamelock.read(), gamelock.game_running()) else 'taken by another holder'})")
    sheets(st["out"])
    return 0


def burst_dirs(out):
    """Folders under `out` holding a capture.py burst (f000.png ...)."""
    found = []
    for root, dirs, files in os.walk(out):
        if "restore" in os.path.relpath(root, out).split(os.sep):
            continue
        if any(re.fullmatch(r"f\d{3,}\.png", f) for f in files):
            found.append(root)
    return sorted(found)


def sheets(out):
    made = []
    for d in burst_dirs(out):
        sheet = d.rstrip("\\/") + "_sheet.png"
        r = subprocess.run([sys.executable, "-I", os.path.join(HERE, "capture.py"), "sheet", d, sheet, "--cols", "6"],
                           capture_output=True, text=True)
        if r.returncode == 0:
            made.append(sheet)
    films = sorted(glob.glob(os.path.join(out, "film", "*.mkv")))
    if films and shutil.which("ffmpeg"):
        sys.path.insert(0, os.path.join(TOOLKIT, "cinema"))
        import frames
        for mkv in films:
            sheet = os.path.splitext(mkv)[0] + "_sheet.png"
            try:
                frames.sheet(mkv, sheet)
                made.append(sheet)
            except (subprocess.CalledProcessError, ValueError) as e:
                say(f"no sheet for {os.path.basename(mkv)} ({e})")
    if made:
        say(f"{len(made)} contact sheet(s): " + ", ".join(os.path.relpath(m, out) for m in made))
    return made


def write_index(st, extra=()):
    lines = [f"# In-game session: {st['slug']} ({time.strftime('%Y-%m-%d %H:%M')})", "",
             f"- holder `{st['holder']}`, hero `{st['hero']}`, take `{st.get('take') or 'none'}`",
             f"- installed: see installed_before.txt" + (" (restored at the end)" if st.get("restore") else ""), ""]
    for r in st.get("runs", []):
        lines.append(f"- {r['name']}: {'ok' if r['rc'] == 0 else 'FROZEN HERO' if r['rc'] == 8 else 'rc %d' % r['rc']}")
    for s in sheets_list(st["out"]):
        lines.append(f"- sheet: {os.path.relpath(s, st['out'])}")
    lines += list(extra)
    with open(os.path.join(st["out"], "index.md"), "w", encoding="utf-8", newline="\n") as f:
        f.write("\n".join(lines) + "\n")


def sheets_list(out):
    return sorted(glob.glob(os.path.join(out, "**", "*_sheet.png"), recursive=True))


def all_(o, items, qa=False):
    up(o)
    rc = 0
    try:
        rc = run(o, items)
    finally:
        st = load(o["out"])
        if st.get("held"):
            try:
                down_state(st)
            except Stop as e:
                say(f"hand-back: {e}")
                rc = rc or e.code
    st = load(o["out"])
    extra = qa_findings(st) if qa else []
    write_index(st, extra)
    say(f"index: {os.path.join(st['out'], 'index.md')}")
    return rc


def qa_findings(st):
    """Crash dumps since the launch, warnings naming the mod's files, and what logdiff.py says the mod adds."""
    out = []
    dumps = [f for f in glob.glob(os.path.join(gamepaths.game(), "bin", "win64", "*.mdmp")) if os.path.getmtime(f) >= st.get("launched_at", 0) - 5]
    if dumps:
        out += ["", "## CRASH DUMPS", ""] + [f"- {d}" for d in dumps]
    log = os.path.join(st["out"], "console_full.log")
    tokens = {t for t in re.split(r"[-_]", st["slug"]) if len(t) >= 4}
    if os.path.exists(log):
        pat = re.compile(r'Failed loading resource "[^"]*(' + "|".join(map(re.escape, sorted(tokens) or [st["slug"]]))
                         + r')|AddTextureToSet|error texture|Missing|ERROR', re.I)
        build, scene = st.get("build") or gamepaths.build() or "0", logdiff.scene_name(st.get("hero"), st.get("map"))
        if st.get("vanilla"):
            # A pass with the mod's paks out is the baseline every later pass on this build is read against.
            dst = logdiff.save_vanilla(log, build, scene)
            return out + ["", "## Vanilla baseline", "", f"- saved for build {build}, scene `{scene}`: {dst}"]
        base = logdiff.pick_base(build, scene)
        vanilla = logdiff.warnings(base[0]) if base else {}
        with open(log, encoding="utf-8", errors="replace") as f:
            hits = [line.rstrip() for line in f if pat.search(line) and "PredictionError" not in line]
        out += ["", "## Console warnings touching the mod's files or rendering", ""] + warning_lines(hits, vanilla, bool(base))
        r = subprocess.run([sys.executable, "-I", os.path.join(HERE, "logdiff.py"), "--mod", log, "--build", build, "--scene", scene],
                           capture_output=True, text=True)
        out += ["", "## Warnings the mod adds over vanilla (logdiff.py)", ""] + [f"    {x}" for x in (r.stdout + r.stderr).splitlines()]
    return out


def warning_lines(hits, vanilla, have_base, limit=40):
    """Console lines for the index: each distinct warning once with how often it came, in first-seen order. With a
    vanilla baseline only the ones vanilla does not print are listed (the mod's to answer for), and the rest are
    counted: they are the game's own. Without one, every line is listed and nothing is claimed about whose it is."""
    seen = {}
    for line in hits:
        k = logdiff.norm(line)
        if k not in seen:
            seen[k] = [0, line]
        seen[k][0] += 1
    if not seen:
        return ["    none"]
    ours = [(n, line) for k, (n, line) in seen.items() if not (have_base and k in vanilla)]
    out = [f"    x{n:<3} {line}" for n, line in ours[:limit]]
    if len(ours) > limit:
        out.append(f"    ... {len(ours) - limit} more distinct line(s) in console_full.log")
    if have_base:
        theirs = len(seen) - len(ours)
        if not ours:
            out.append("    none that vanilla does not print too")
        if theirs:
            out.append(f"    ({theirs} distinct warning(s) vanilla prints too are left out: the game's own)")
    return out


# ---- the command line ----------------------------------------------------------------------------------------------

VALUED = {"--out", "--mod", "--pak", "--as", "--holder", "--map", "--hero", "--wait", "--size", "--timeout", "--key",
          "--take", "--marks", "--w0", "--actor", "--fps"}
FLAGS = {"--vanilla", "--restore", "--menu", "--no-hero", "--no-tv", "--load", "--no-probe"}
VERBS = ("up", "run", "film", "down", "sheets", "all", "qa", "where")


def parse(argv):
    if argv and argv[0] in ("--help", "-h", "help"):
        raise Stop(0, __doc__)
    if not argv or argv[0] not in VERBS:
        raise Stop(2, __doc__)
    o, rest = {}, []
    i = 1
    while i < len(argv):
        a = argv[i]
        if a in VALUED:
            if i + 1 >= len(argv):
                raise Stop(2, f"{a} needs a value")
            o[a[2:]] = argv[i + 1]
            i += 2
        elif a in FLAGS:
            o[a[2:]] = True
            i += 1
        elif a.startswith("--"):
            raise Stop(2, f"unknown option {a} (mod_session.py has: {', '.join(sorted(VALUED | FLAGS))})")
        else:
            rest.append(a)
            i += 1
    if argv[0] != "where" and "out" not in o:
        raise Stop(2, "--out DIR is required")
    if o.get("pak") and o.get("vanilla"):
        raise Stop(2, "--pak and --vanilla are two different passes: give one")
    return argv[0], o, rest


def main(argv):
    try:
        verb, o, rest = parse(argv)
        if verb == "where":
            if not rest:
                raise Stop(2, "where <item>")
            toml = find_mod_toml(o.get("mod") or os.getcwd())
            p = resolve_item(rest[0], read_mod(toml)["dir"] if toml else None)
            if not p:
                raise Stop(2, f"no scenario, plan or stage {rest[0]!r}")
            print(p)
            return 0
        if verb == "up":
            return up(o)
        if verb == "run":
            if not rest:
                raise Stop(2, "run needs at least one item")
            return run(o, rest)
        if verb == "film":
            if len(rest) != 1:
                raise Stop(2, "film <shots.txt>")
            return film(o, os.path.abspath(rest[0]))
        if verb == "down":
            return down(o)
        if verb == "sheets":
            sheets(os.path.abspath(o["out"]))
            return 0
        if verb in ("all", "qa"):
            items = rest
            if verb == "qa" and not items:
                toml = find_mod_toml(o.get("mod") or os.getcwd())
                items = read_mod(toml)["pass"] if toml else []
                if not items:
                    raise Stop(2, "qa: no items given and the mod.toml has no [ingame] pass")
            if not items:
                raise Stop(2, "all needs at least one item")
            toml = find_mod_toml(o.get("mod") or os.getcwd())
            missing = [i for i in items if not resolve_item(i, read_mod(toml)["dir"] if toml else None)]
            if missing:
                raise Stop(2, "not found before launching: " + ", ".join(missing))
            return all_(o, items, qa=verb == "qa")
    except Stop as e:
        print(str(e) if str(e) == __doc__ else f"session: {e}", flush=True)
        return e.code
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))

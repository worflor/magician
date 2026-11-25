"""Where the game, the reference folder and the game lock are, resolved exactly as the magician CLI resolves them.

The harness, the QA scripts, the bundler and the film kit all ask this module, so they agree with the CLI. Each
location is an environment variable, then a line in the config file, then discovery or a default (the CLI also takes
flags; `magician paths` prints the same answers with their sources):

    game       DEADLOCK_GAME        config `game`       else Deadlock found through Steam's library list
    reference  MAGICIAN_REFERENCE   config `reference`  else the platform's data folder: %LOCALAPPDATA%/magician/reference
                                                        on Windows, $XDG_DATA_HOME/magician/reference (~/.local/share/...)
    work       <reference>/work (harness, QA and film output)
    game_lock  MAGICIAN_GAME_LOCK   config `game_lock`  else <reference>/work/ingame-game.lock
    magician   MAGICIAN_EXE                             else this toolkit's target/release build, else `magician` on PATH

The config file is MAGICIAN_CONFIG, else %APPDATA%/magician/config.txt on Windows, $XDG_CONFIG_HOME/magician/config.txt
(~/.config/magician/config.txt) elsewhere: one `key = value` per line, `#` comments. From a shell:

    python -I gamepaths.py game        (also: citadel, console_log, reference, work, game_lock, vrf, csdk, config, magician)

prints the path, or exits 2 with the reason on stderr. Standard library only.
"""
import os
import sys


def _env(name):
    v = os.environ.get(name, "").strip()
    return v or None


def config_file():
    if _env("MAGICIAN_CONFIG"):
        return _env("MAGICIAN_CONFIG")
    if os.name == "nt":
        return os.path.join(_env("APPDATA") or "", "magician", "config.txt") if _env("APPDATA") else None
    base = _env("XDG_CONFIG_HOME") or (os.path.join(_env("HOME"), ".config") if _env("HOME") else None)
    return os.path.join(base, "magician", "config.txt") if base else None


def config_lookup(text, key):
    """The last `key = value` line for key (case-insensitive); `#` lines are comments; quotes are optional."""
    found = None
    for line in text.splitlines():
        line = line.strip()
        if line.startswith("#") or "=" not in line:
            continue
        k, v = line.split("=", 1)
        if k.strip().lower() == key.lower():
            v = v.strip().strip('"').strip()
            found = v or None
    return found


def config(key):
    path = config_file()
    try:
        with open(path, encoding="utf-8-sig") as f:
            return config_lookup(f.read(), key)
    except (OSError, TypeError):
        return None


def _game_folder(p):
    """p if it is Deadlock's `game` folder, or its `game` child; else None."""
    for c in (p, os.path.join(p, "game")):
        if os.path.isfile(os.path.join(c, "citadel", "steam.inf")):
            return c
    return None


def library_paths(vdf):
    """Library roots in Steam's libraryfolders.vdf: the second quoted string of every "path" line."""
    out = []
    for line in vdf.splitlines():
        q = line.split('"')
        if len(q) >= 4 and q[1] == "path":
            out.append(q[3].replace("\\\\", "\\"))
    return out


def _steam_roots():
    roots = []
    if os.name == "nt":
        try:
            import winreg
            with winreg.OpenKey(winreg.HKEY_CURRENT_USER, r"Software\Valve\Steam") as k:
                roots.append(winreg.QueryValueEx(k, "SteamPath")[0].replace("/", "\\"))
        except OSError:
            pass
        for var in ("ProgramFiles(x86)", "ProgramFiles"):
            if _env(var):
                roots.append(os.path.join(_env(var), "Steam"))
    elif _env("HOME"):
        for rel in (".steam/steam", ".local/share/Steam", ".var/app/com.valvesoftware.Steam/.local/share/Steam",
                    "snap/steam/common/.local/share/Steam"):
            roots.append(os.path.join(_env("HOME"), rel))
    return roots


def _steam_game():
    for s in _steam_roots():
        libs = [s]
        try:
            with open(os.path.join(s, "steamapps", "libraryfolders.vdf"), encoding="utf-8", errors="replace") as f:
                libs += library_paths(f.read())
        except OSError:
            pass
        for lib in libs:
            g = _game_folder(os.path.join(lib, "steamapps", "common", "Deadlock"))
            if g:
                return g
    return None


def game():
    """Deadlock's `game` folder. Raises LookupError with the fix when it cannot be found."""
    for source, value in (("DEADLOCK_GAME", _env("DEADLOCK_GAME")), ("config game", config("game"))):
        if value:
            g = _game_folder(value)
            if not g:
                raise LookupError(f"{source} {value}: no citadel/steam.inf there (point it at Deadlock's `game` folder)")
            return g
    g = _steam_game()
    if not g:
        raise LookupError("could not find Deadlock through Steam; set DEADLOCK_GAME or add `game = ...` to "
                          f"{config_file() or 'the config file'}")
    return g


def try_game():
    """game(), or a placeholder path that names the problem, for module-level constants that must not raise at import."""
    try:
        return game()
    except LookupError:
        return os.path.join("<Deadlock not found - set DEADLOCK_GAME>", "game")


def citadel():
    return os.path.join(game(), "citadel")


def build():
    """The installed game's build number (`ClientVersion` in citadel/steam.inf), as text; None when it can't be read."""
    try:
        with open(os.path.join(citadel(), "steam.inf"), encoding="utf-8", errors="replace") as f:
            for line in f:
                if line.startswith("ClientVersion="):
                    return line.split("=", 1)[1].strip() or None
    except (OSError, LookupError):
        pass
    return None


def console_log():
    """The log the game writes with -condebug."""
    return os.path.join(citadel(), "console.log")


def reference():
    v = _env("MAGICIAN_REFERENCE") or config("reference")
    if v:
        return v
    if os.name == "nt":
        base = _env("LOCALAPPDATA") or ""
    else:
        base = _env("XDG_DATA_HOME") or os.path.join(_env("HOME") or "", ".local", "share")
    return os.path.join(base, "magician", "reference")


def work(*parts):
    """<reference>/work, where harness, QA and film output goes (joined with any parts given)."""
    return os.path.join(reference(), "work", *parts)


def vrf():
    """The Source 2 Viewer CLI: MAGICIAN_VRF, config `vrf`, else Source2Viewer-CLI from PATH."""
    return _env("MAGICIAN_VRF") or config("vrf") or "Source2Viewer-CLI"


def csdk():
    """Valve's CSDK folder: MAGICIAN_CSDK, else config `csdk`. Raises LookupError when neither is set."""
    v = _env("MAGICIAN_CSDK") or config("csdk")
    if not v:
        raise LookupError("no CSDK: set MAGICIAN_CSDK or add `csdk = ...` to " + (config_file() or "the config file"))
    return v


def game_lock():
    return _env("MAGICIAN_GAME_LOCK") or config("game_lock") or work("ingame-game.lock")


EXE = "magician.exe" if os.name == "nt" else "magician"


def magician():
    """The magician binary the Python tools run: MAGICIAN_EXE (the front door sets it to itself, so `magician session`
    drives the same binary that was called), else this toolkit's release build, else `magician` on PATH."""
    if _env("MAGICIAN_EXE") or _env("MAGICIAN_BIN"):   # MAGICIAN_BIN: the name the MCP server and the tests read
        return _env("MAGICIAN_EXE") or _env("MAGICIAN_BIN")
    for built in release_builds():
        if os.path.isfile(built):
            return built
    import shutil
    return shutil.which("magician") or release_builds()[0]


def release_builds():
    """Where cargo puts this checkout's release build: CARGO_TARGET_DIR when it is set, else the checkout's target/."""
    here = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "target")
    targets = [_env("CARGO_TARGET_DIR"), here] if _env("CARGO_TARGET_DIR") else [here]
    return [os.path.join(t, "release", EXE) for t in targets]


def main(argv):
    what = argv[1] if len(argv) > 1 else ""
    table = {"game": game, "citadel": citadel, "console_log": console_log, "reference": reference, "work": work,
             "game_lock": game_lock, "vrf": vrf, "csdk": csdk, "config": lambda: config_file() or "", "magician": magician}
    if what not in table:
        print("usage: gamepaths.py " + " | ".join(table), file=sys.stderr)
        return 2
    try:
        print(table[what]())
    except LookupError as e:
        print(f"gamepaths: {e}", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))

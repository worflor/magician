"""Start and stop the Source 2 Viewer Debug GUI with its MCP automation server (--mcp), and talk to it.

    python -I tools/s2v_mcp.py [start]       launch the GUI with --mcp=<port> detached, wait until the endpoint
                                              answers, print the URL
    python -I tools/s2v_mcp.py stop          ask it to quit over MCP (the `quit` tool); end the process if it stays
    python -I tools/s2v_mcp.py status        whether the endpoint answers (exit 0 up, 1 down)
    python -I tools/s2v_mcp.py tools         the tool names it exposes
    python -I tools/s2v_mcp.py call <tool> '{"json": "args"}'
    options: --port 13338  --timeout 300  --root DIR  --dotnet-root DIR

The viewer is a Debug build of ValveResourceFormat 21.1 (the pinned tag; its GUI has the --mcp switch), built with
the .NET 11 SDK. --root is that checkout: --root, else MAGICIAN_VRF_SRC, else `vrf_src = ...` in Magician's config
file. --dotnet-root is the SDK: --dotnet-root, else DOTNET_ROOT, else `dotnet_root = ...`, else none (a machine-wide
.NET). The GUI is a Windows Forms program (its GUI.csproj targets net*-windows with UseWindowsForms), so `start` runs
on Windows only; status, tools, call and stop speak HTTP and work from anywhere that reaches the port. It is
unsandboxed, like the UI itself: it can open any file the user can. It binds loopback only and never launches the
game. Register it once with your MCP client, as an HTTP server:  source2viewer  http://127.0.0.1:13338/mcp

Standard library only. tools/s2v_mcp.ps1 is the same with PowerShell's options.
"""
import json
import os
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "ingame"))
import gamepaths  # noqa: E402
import gameos  # noqa: E402


def mcp(url, method, params=None, timeout=120):
    body = json.dumps({"jsonrpc": "2.0", "id": 1, "method": method, "params": params or {}}).encode()
    req = urllib.request.Request(url, body, {"Content-Type": "application/json", "Accept": "application/json, text/event-stream"})
    with urllib.request.urlopen(req, timeout=timeout) as r:
        text = r.read().decode("utf-8", "replace")
    # a streamable-HTTP server may answer as an event stream: the JSON is on its data line
    if text.lstrip().startswith("event:") or "\ndata:" in text:
        text = next(l[5:].strip() for l in text.splitlines() if l.startswith("data:"))
    return json.loads(text)


def up(url):
    try:
        mcp(url, "tools/list", timeout=5)
        return True
    except (OSError, ValueError, urllib.error.URLError, StopIteration):
        return False


def main(argv):
    o = {"port": "13338", "timeout": "300", "root": None, "dotnet-root": None}
    rest, i = [], 0
    while i < len(argv):
        a = argv[i]
        if a in ("--help", "-h"):
            print(__doc__)
            return 0
        if a[2:] in o and a.startswith("--") and i + 1 < len(argv):
            o[a[2:]] = argv[i + 1]
            i += 2
            continue
        rest.append(a)
        i += 1
    cmd = rest[0] if rest else "start"
    url = f"http://127.0.0.1:{o['port']}/mcp"
    root = o["root"] or gamepaths._env("MAGICIAN_VRF_SRC") or gamepaths.config("vrf_src")
    pid_file = os.path.join(root or tempfile.gettempdir(), f".s2v_mcp_{o['port']}.pid")
    if cmd == "start":
        if up(url):
            print(f"already running: {url}")
            return 0
        if not gameos.WINDOWS:
            print("s2v_mcp: the Source 2 Viewer GUI is a Windows Forms program, so it starts on Windows only; "
                  "status, tools, call and stop work from here against one running there", file=sys.stderr)
            return 2
        if not root:
            print("s2v_mcp: no Source 2 Viewer checkout: pass --root <the ValveResourceFormat 21.1 checkout>, set "
                  "MAGICIAN_VRF_SRC, or add vrf_src = <path> to the config file", file=sys.stderr)
            return 2
        exe = os.path.join(root, "GUI", "bin", "Debug", "win-x64", "Source2Viewer.exe")
        if not os.path.isfile(exe):
            print(f"s2v_mcp: Debug GUI not built: {exe}. Build it with the .NET 11 SDK: dotnet build "
                  f"\"{os.path.join(root, 'GUI', 'GUI.csproj')}\" -c Debug", file=sys.stderr)
            return 2
        dotnet = o["dotnet-root"] or gamepaths._env("DOTNET_ROOT") or gamepaths.config("dotnet_root")
        if dotnet:   # a .NET outside the machine-wide install is found by the apphost through DOTNET_ROOT
            os.environ["DOTNET_ROOT"] = dotnet
        p = subprocess.Popen([exe, f"--mcp={o['port']}"], cwd=os.path.dirname(exe), close_fds=True,
                             creationflags=0x00000008 | 0x00000200)
        with open(pid_file, "w") as f:
            f.write(str(p.pid))
        deadline = time.time() + float(o["timeout"])
        while time.time() < deadline:
            if p.poll() is not None:
                print(f"s2v_mcp: viewer exited early (code {p.returncode})", file=sys.stderr)
                return 1
            if up(url):
                print(url)
                return 0
            time.sleep(0.5)
        print(f"s2v_mcp: endpoint did not answer within {o['timeout']}s: {url} (pid {p.pid} left running; use 'stop')",
              file=sys.stderr)
        return 1
    if cmd == "stop":
        if up(url):
            try:
                mcp(url, "tools/call", {"name": "quit", "arguments": {}}, timeout=15)
            except (OSError, ValueError, urllib.error.URLError, StopIteration):
                pass
        try:
            pid = int(open(pid_file).read().strip())
        except (OSError, ValueError):
            pid = 0
        if pid:
            started = gameos.process_started(pid)
            if started and not gameos.wait_exit([(pid, started)], 15):
                gameos.terminate(pid)
                print(f"killed pid {pid}")
        try:
            os.remove(pid_file)
        except OSError:
            pass
        print("stopped")
        return 0
    if cmd == "status":
        print(("up: " if up(url) else "down: ") + url)
        return 0 if up(url) else 1
    if cmd == "tools":
        for t in mcp(url, "tools/list")["result"]["tools"]:
            print(t["name"])
        return 0
    if cmd == "call":
        if len(rest) < 2:
            print("usage: s2v_mcp.py call <tool> [argsJson]", file=sys.stderr)
            return 2
        args = json.loads(rest[2] if len(rest) > 2 else os.environ.get("S2V_MCP_ARGS") or "{}")
        print(json.dumps(mcp(url, "tools/call", {"name": rest[1], "arguments": args}, timeout=300), indent=2))
        return 0
    print(__doc__, file=sys.stderr)
    return 2


if __name__ == "__main__":
    try:
        sys.exit(main(sys.argv[1:]))
    except (OSError, urllib.error.URLError, ValueError, StopIteration) as e:
        print(f"s2v_mcp: the viewer's MCP server does not answer ({e})", file=sys.stderr)
        sys.exit(1)

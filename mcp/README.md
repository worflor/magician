# Magician MCP server

Magician's CLI as structured tools for AI agents: call a function, get JSON, instead of parsing CLI text. It is the agent-facing half of Magician's headless contract (README, "Driving it from a script").
Python standard library only (3.8+); it implements MCP itself (JSON-RPC 2.0 over stdio: `initialize`, `ping`,
`tools/list`, `tools/call`), and every tool runs the release binary as a subprocess.

## Register it

Add this to the `.mcp.json` of the project (or to the `mcpServers` object of your user MCP settings), with the path to your checkout:

```json
{
  "mcpServers": {
    "magician": {
      "command": "python",
      "args": ["C:\\path\\to\\magician\\mcp\\server.py"]
    }
  }
}
```

From a terminal the same server is `python C:\path\to\magician\mcp\server.py`; any MCP client that speaks stdio can run it.
Nothing in this repo edits a settings file; registering is your call.

The server uses `target\release\magician.exe` (or `target\mcp-build\release\magician.exe` if that is newer). Set
`MAGICIAN_BIN` in the entry's `"env"` to pin another binary. Build with `cargo build --release`. Logs go to
`mcp\logs\` (`MAGICIAN_MCP_LOGS` moves them).

## Tools

| Tool | Arguments | Returns |
|---|---|---|
| `check` | `mod` (its folder or `mod.toml`), `light`, `policy` | the mod's one verdict on the installed build (`magician.check/1`): `verdict` (ok, re-prove, broken), `label`, `steps[]` (lint, grade, drift, scene, each with its state), the grade record, `next[]`; `ok` when the verdict is ok. The first call on a new build may capture its baseline into the reference folder |
| `doctor` | `mod` | read-only: is this machine ready (`magician.doctor/1`): `checks[]` (name, state ok, todo, skip or fail, detail, fix), `failing`, `todo`; `ok` when nothing fails |
| `rules` | | every lint rule: code, name, severity, in_game (`magician.rules/1`) |
| `lint` | `vpk`, `deep`, `allow`, `policy` | the lint run (`magician.lint/1`): `findings[]` (code, name, severity, path, message, allowed, reason), `counts`, `stale_allowances`, `depth`, `build`; `ok` when no error |
| `mesh` | `vpk`, `model`, `against`, `with[]` | `verdict` (PASS / ATTENTION / FAIL), `findings[]` with in_game, `report_head` (LODs, silhouettes). No PNG is written |
| `cloth` | `vpk`, `model`, `against`, `shapes` | `summary[]` (field, ours, valve) and the full per-chain `report` |
| `grade` | `vpk` or `vpks[]`, `hero` (lash, Lash, hero_lash or a model path) or `mod`, `light`, `target`, `allow[]`, `policy` | the grade record (`magician.grade/2`): `label`, `depth` (letter, score, archetype rubric, `criteria[]`, `caps[]`), `identity`, `fair_play` (state, checks run and not run, violations, allowances with value and limit, advisories), `freshness`, `rights`, `release_gate` (blockers, warnings), `next_steps`, `policy` fingerprint; `ok` when the gate passes |
| `release` | `addon`, `hero`, `with[]`, `out`, `mod`, `no_compile` | long-running: the release (`magician.release/1`): departments, the lint run and the grade record inside, the report path; `ok` when lint passed |
| `drift` | `vpks[]`, `from`, `to` | per pak (`magician.drift/1`): state (clean, re-prove, broken), changed and removed originals, broken references, lost bones and graph parameters, contract breaches |
| `patchday` | `capture`, `from`, `mods[]` | the patch-day check (`magician.patchday/1`): baseline, install state, each mod's drift from its last proven build, manifests, `verdict` (clear, re-prove, broken). `capture` writes only to the reference folder |
| `policy` | `policy` | every judgement value in force (`entries[]`: key, value, source), the `fingerprint` a grade records |
| `build` | `addon`, `out`, `no_compile` | long-running: `ok`, `summary`, `lint`, `run.log` |
| `install_status` | | read-only: build, search path, backup, `installed[]` (file, crc, mod) |
| `refs` | `vpk`, `path` | `blocks`, `references[]` (path, resolves), `unresolved` |
| `ls` | `vpk`, `prefix`, `limit` | `files[]` (path, crc, size), `total`, `truncated` |
| `dump` | `vpk`, `path`, `block`, `max_chars` | KV3 block text (or a texture header), `truncated` |
| `verify` | `vpk`, `summary` | `decoded`, `carry_mark`, `pooled` (z, p_below), `report` |

Every tool takes an optional `timeout` (seconds). Results carry `run` (`exit_code`, `meaning`: clean, broken, usage error, refused or re-prove, `timed_out`, `seconds`, `log`).
Failures come back as `isError: true` with `{"ok": false, "error": ...}`; a lint with errors is a normal result
with `ok: false`.

## Long-running commands

`release`, `build` and `lint` (especially `deep`) run as subprocesses with a timeout (defaults: release and build
1800 s, lint 300 s, grade 600 s). On timeout the whole process tree is killed and the call returns an error naming
the log. Release and build never wait on another build's lock (`--no-wait`): they fail at once and say who holds it.
The full output is always in the log file; the result carries the summary and the path.

## Safety

No tool installs, uninstalls or launches the game, and the server refuses any sub-command outside its allow-list
(`install` is reachable only as the read-only `--status`). Installing stays CLI-only, for a human or a pipeline to
run on purpose: `magician install`, `magician uninstall`, `ingame\launch.ps1`. Arguments that start with `-` are
refused so a path cannot smuggle in a flag.

## JSON from the CLI

`lint`, `grade`, `drift`, `patchday`, `release` and `rules` take `--json` and print one JSON document with a `schema`
string on stdout, and nothing else; the server reads that document from stdout and keeps stderr in the log. `mesh`
writes its verdict with `--json-out FILE`. The rest have no JSON form and the server reads their output (`cloth`,
`refs`, `ls`, `verify`, `install --status`, and `policy`, which is tab-separated), so a change to those layouts needs
a change here.

## Test

```
python mcp\test_server.py [path\to\small\pakNN_dir.vpk]
```

Starts the server, lists the tools, calls `rules`, `policy` (a good one and a bad file), `lint` (light and deep),
`grade` (a pak with nothing of the hero: not graded, the gate blocked), `patchday` and `ls`, and checks the JSON shape,
the error paths and that no install tool exists. Without an argument (or `MAGICIAN_TEST_PAK`) it packs a one-file pak
of its own; the lint, grade and patchday checks need the game (grade the Source 2 Viewer CLI too) and are skipped, with
a line saying so, when it isn't found.

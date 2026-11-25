# Where things are in the Magician repository

For reading code and rules when a finding needs explaining, or when you are asked to change Magician itself (then read `CONTRIBUTING.md` at the root first: it has the rules for changes).

| Path | What is there |
|---|---|
| `crates/magician/src/main.rs` | The CLI: dispatch, the usage text (`USAGE`), lint, grade, drift, build, release, install, policy commands |
| `crates/magician/src/cli.rs` | `--help`, `--version`, option checking and the exit codes |
| `crates/magician/src/paths.rs` | How the game, reference folder, CSDK, viewer, mods folder, toolkit, game lock and policy file are found |
| `crates/magician/src/frontdoor.rs`, `newcmd.rs` | The verbs that run the Python tools (`session`, `reprove`, `bundle`, `source`); `magician new` |
| `crates/magician/src/install.rs` | Install, uninstall, the ledger, slots, the `gameinfo.gi` backup, the seat check |
| `crates/magician/src/patchday.rs`, `patchcmd.rs`, `proofs.rs` | Baselines as eras, rig sidecars, drift verdicts, `patchday`, `prove`, mod discovery; the proof ledger (`proofs.tsv`: which bytes were proven on which build) |
| `crates/magician/src/scene.rs` | Scene patches; the patch format is documented at the top |
| `crates/magician/src/build.rs`, `lock.rs` | Compiling with resourcecompiler, packing, the per-addon build lock |
| `crates/lint/src/lib.rs` | The rule catalogue (`RULES`: code, name, severity, in-game consequence) and the light rules |
| `crates/lint/src/deep.rs`, `cloth.rs`, `pbrcombo.rs`, `distfield.rs` | Rules that read inside compiled files |
| `crates/lint/src/mesh/` | The mesh family M100-M148: `census.rs` measures, `rules.rs` judges (every threshold a named constant, the fair-play limits too), `openings.rs`, `views.rs` |
| `crates/lint/src/policy.rs` | The fair-play limits a lint run is handed, and their policy keys and ranges |
| `crates/lint/src/tests.rs`, `mesh_fixtures.rs` | A fixture that breaks each rule; `every_rule_has_a_fixture` enforces it |
| `crates/lint/src/allow.rs` | `magician.allow`: per-site allowances and stale detection |
| `crates/grade/src/rubric.rs` | Every grading number: criteria and their groups, weights within a group, the lines that name a group, coverage targets, identity requirements, fair-play rule classes |
| `crates/grade/src/footprint.rs` | A hero's footprint from the game's own data |
| `crates/grade/src/identity.rs`, `compliance.rs`, `record.rs` | What kind of mod it is; fair play; the record, its label and the release gate |
| `crates/grade/src/policy.rs` | The policy file: every key, its range, the reader, the fingerprint |
| `crates/vpk`, `resource`, `mesh`, `geom`, `expr`, `mark` | Readers and writers: VPK, resources and KV3, meshes and cloth, geometry, material expressions, the maker's mark |
| `mcp/server.py` | The MCP server (seventeen JSON tools; no install or launch tool) |
| `ingame/` | The in-game harness: `mod_session.py` (one session), `reprove.py` (patch day's passes), `logdiff.py` (warnings over vanilla), `gamelock.py` (the seat), `gamepaths.py` (locations), `scenarios/` (examples on a stock hero) |
| `qa/`, `harness/` | Offline Blender QA and renders; `qa/heroes/<hero>.json` per-hero configs |
| `tools/bundle.py`, `source.py`, `publish.py` | Release bundles, the static mod source with its pages, and the upload to Nexus Mods; each documents its spec keys at the top |
| `tools/scene_install.ps1`, `.py` | The player-side scene installers bundles ship |
| `docs/PATCH-DAY.md` | Every patch-day message and its fix |
| `docs/reference/engine.md` | What Deadlock, the CSDK compiler and Source 2 Viewer actually do |

## A mod's manifest

Every command run for a mod reads the `mod.toml` in its folder. It is read line by line: flat tables of strings, numbers and lists, the shape `magician new` and `magician init` write.

```toml
[mod]
slug = "maid-lash"
name = "Maid Lash"
author = "you"                                      # also the key of the maker's mark
hero = "hero_lash"                                  # the heroes.vdata key
kind = "model-replacement"
owners = ["lash_showcase"]                          # other ledger owners it installs under (patch day)

[target]
model    = "models/heroes_wip/lash/lash.vmdl"        # must still exist in a new build
skeleton = "models/heroes_wip/lash/lash.vnmskel"

[proven]
builds = [6864]                                      # the summary `magician prove` keeps, never by hand; the proofs
                                                     # of which bytes were seen on which build are in proofs.tsv

[identity]
voice = "new"                                        # a new voice identity, declared; a treatment of the lines is not one

[rights]
state = "unreviewed"                                 # unreviewed | cleared | denied: a person's call, never yours
notes = ""

[ingame]
holder = "maid-lash"                                 # the name its sessions take the game seat under
pass   = ["lash_angles", "lash_motion"]              # the scenarios an in-game pass runs; found in the mod's own folders first
```

`proofs.tsv` beside it is the proof ledger `magician prove` writes: one row per file per proof (build, date, owner, file, crc32, sha256, evidence). Read it; never edit it.

The ledger owner a pak was installed under (`--mod`, default the source folder's name) is what `uninstall --mod` and `patchday`'s per-mod grouping use; it can differ from the slug (`maid_lash` against `maid-lash`).

# The Deadlock modding toolchain, and where Magician fits

A survey of what Deadlock modders use today, what they report as broken or slow, and what a target-aware tool could do about it. Read from public pages on 2026-11-27, with installed build 6871. Nothing was downloaded or run for it.

How to read the marks:

- verified: the claim is stated by two or more independent public sources, or a source and Magician's own measurements ([engine.md](../reference/engine.md), the code).
- single report: one page or one post says it. Treat it as a lead.
- Where Magician already does something, the file or command is named.

Sources that could not be read: Reddit (r/DeadlockTheGame, r/DeadlockMods) refuses automated fetches, and the Valve Developer Community and the official Deadlock forums answered 403. Where a claim rests only on a search engine's summary of one of those pages, it says so. GameBanana's pages render in the browser, so its tutorials and comments were read through its public JSON API (`gamebanana.com/apiv11/...`).

## 1. The kits

### What CSDK means

The "CSDK" (Community Source Development Kit) is a community repack of one of Valve's games' Workshop Tools with Deadlock fixes merged in, distributed as a zip from Google Drive or Mega. Valve has released no Deadlock tools and no Deadlock SDK ([Divine Skins, Tools](https://wiki.divineskins.gg/en/docs/deadlock/tools); [Divine Skins, CS2 Workshop Tools](https://wiki.divineskins.gg/en/docs/deadlock/tools/cs2-workshop-tools)). Verified.

| Kit | Base | Status | Source |
|---|---|---|---|
| CSDK 10 | CS2 Workshop Tools plus community fixes | marked outdated, "Use CSDK 12 instead" | [deadlockmodding: CSDK 10](https://deadlockmodding.pages.dev/modding-tools/csdk-10) |
| CSDK 12 | CS2 Workshop Tools plus fixes for the "6 Heroes" update (2025-08-19) | marked outdated on deadlockmodding, still the documented kit on Divine Skins | [deadlockmodding: CSDK 12](https://deadlockmodding.pages.dev/modding-tools/csdk-12), [Divine Skins: CSDK 12](https://wiki.divineskins.gg/en/docs/deadlock/tools/csdk-12) |
| CSDK 13 | Dota 2 Workshop Tools plus fixes made after the Nurse Harrow update (2026-10-20); page dated 2026-11-12 | current on deadlockmodding | [deadlockmodding: CSDK 13](https://deadlockmodding.pages.dev/modding-tools/csdk-13) |

CSDK 14: none exists or is announced as of 2026-11-27. The deadlockmodding tool index lists CSDK 13 (2026-11-12) as its newest kit and has no CSDK 11 or 14 ([index](https://deadlockmodding.pages.dev/modding-tools/)); Divine Skins documents only CSDK 12 ([CSDK 12](https://wiki.divineskins.gg/en/docs/deadlock/tools/csdk-12)); a web search for "CSDK 14" Deadlock returns nothing related. Verified as far as public pages go; the kit's own Discord was not read.

The two main wikis disagree on which kit to use: deadlockmodding says CSDK 12 is outdated, while Divine Skins' 57-guide Deadlock wiki, including its packing and error pages, still walks through CSDK 12 and never mentions 13 ([Divine Skins, pack a VPK](https://wiki.divineskins.gg/en/docs/deadlock/basics/pack-a-vpk); [creator errors](https://wiki.divineskins.gg/en/docs/deadlock/errors/creator-errors)). Verified. A creator following one wiki's error page while running the other wiki's kit is a real source of confusion.

### Binary configurations

| Kit | Folder | What it is for | Documented breakage |
|---|---|---|---|
| 12 | `bin` | regular tools; compiles AnimGraph 1 | crashes previewing some projected particles |
| 12 | `bin_tools` | previewing projected particles | crashes compiling AnimGraph 1 |
| 12 | `bin_cs2` | only for the GUI map compiler (`CS2MapCompiler.exe`), lighting without the tools open | |
| 12 | `bin_server` | needs the full game files; runs Deadlock in server mode with the Asset Browser, Material Editor and some VData and Panorama compiles | "Everything else will crash" |
| 13 | `bin` | regular tools; compiles files and maps from Hammer | Hammer lightmaps dark (`SH2_lum` missing); light-source particles crash the preview |
| 13 | `bin_server` | needs the full game files; Asset Browser, Material Editor, ModelDoc, some VData and Panorama | any particle preview crashes, and heroes with `Particle_CFG` in a model's GameDataList crash on preview |

From [deadlockmodding: CSDK 12](https://deadlockmodding.pages.dev/modding-tools/csdk-12) and [CSDK 13](https://deadlockmodding.pages.dev/modding-tools/csdk-13); the CSDK 12 rows are repeated on [Divine Skins: CSDK 12](https://wiki.divineskins.gg/en/docs/deadlock/tools/csdk-12). The particle preview crash is on three community pages (verified as a report; Magician has not reproduced it, and `magician csdk` (`crates/magician/src/csdk.rs`) lists it as a community report, `seen_here: false`). The `SH2_lum` dark lightmaps are on the CSDK 13 page and, by that same file, in the kit's own readme (gap `sh2-lum`); not reproduced here.

Magician adds a configuration the kit doesn't ship: `bin/win64_nm`, a copy of `bin/win64` with Deadlock's own `resourcecompiler.dll` and `animationsystem.dll`, because CSDK 13's Dota compiler can't allocate `CNmClipDocEvent_TaggedSound` ([engine.md](../reference/engine.md#the-compiler-csdk-13)). Verified here.

### Setup rules the kits impose

All from the kits' pages and Divine's error page; each is repeated on at least two pages, so verified as community practice:

- unpack outside any OneDrive-synced folder (Desktop, Documents), or the tools fail on permissions and crash;
- launch through `csdkcfg.exe`, with Steam running;
- CSDK 13 only: rename `game/citadel/readonly_tools_asset_info.binOFF` to `.bin`, "Very-VERY IMPORTANT", or VPKs don't mount;
- addon names all lowercase, no spaces or special characters, or the addon silently does not appear in the launcher;
- sources in `content/citadel_addons/<addon>/`, outputs in `game/citadel_addons/<addon>/`, compiled files end in `_c`;
- the "No Addon" launch option can overwrite the kit's own `game/citadel` assets;
- CSDK 12's full game files come from DepotDownloader at pinned manifests, which Steam refuses with a 401 for builds older than the account's access, so the page ships a manifest archive as a workaround.

Sources: [CSDK 12](https://deadlockmodding.pages.dev/modding-tools/csdk-12), [CSDK 13](https://deadlockmodding.pages.dev/modding-tools/csdk-13), [Divine Skins creator errors](https://wiki.divineskins.gg/en/docs/deadlock/errors/creator-errors).

### The tools inside, and how each works for Deadlock

| Tool | Status for Deadlock |
|---|---|
| Asset Browser | works; the hub from which files are recompiled ("Recompile > Full") and the Workshop Manager is opened ([CSDK 12 packing](https://deadlockmodding.pages.dev/modding-guides/csdk12-packing)) |
| Material Editor | works; defaults new materials to a shader other than `pbr.vfx`, which every hero material uses, so each new material has to be checked ([material reference](https://wiki.divineskins.gg/en/docs/deadlock/textures/material-reference), [materials for custom models](https://wiki.divineskins.gg/en/docs/deadlock/textures/materials-for-custom-models)); purple checkerboards when a slot loses its image after files move ([creator errors](https://wiki.divineskins.gg/en/docs/deadlock/errors/creator-errors)) |
| ModelDoc | works for mesh swaps; crashes opening a decompiled hero until `NmSkeletonList` and `AnimGraph2List` are deleted, on italic nodes, on deleting bodygroups, after a bad edit; a "Fix Models" launcher box exists on some builds ([model problems](https://wiki.divineskins.gg/en/docs/deadlock/errors/model-problems)) |
| AnimGraph 2 | no editor for Deadlock at all; CSDK 12 compiles AG2 clips "in an outdated form" and cannot compile AG2 references into a model, so the final model compile goes through Valve's CS2 Workshop Tools, which lack Deadlock's shaders and show red checkerboards ([CSDK 12](https://wiki.divineskins.gg/en/docs/deadlock/tools/csdk-12), [CS2 Workshop Tools](https://wiki.divineskins.gg/en/docs/deadlock/tools/cs2-workshop-tools), [hero swap part 4](https://wiki.divineskins.gg/en/docs/deadlock/models/hero-swap-compile)). The VmClip guide repeats it: the conversion "WILL NOT MAKE A FUNCTIONAL ANIMGRAPH2 GRAPH" ([VmClip guide](https://deadlockmodding.pages.dev/modding-guides/Custom-animation-VmClip-guide)) |
| Particle Editor | works in CSDK 13's reduced kit apart from the preview crashes above; newer particles may not open until the decompiled `.vpcf`'s format header is edited (an untested video fix, single report, [creator errors](https://wiki.divineskins.gg/en/docs/deadlock/errors/creator-errors)) |
| Hammer | needs the full game files in 12; compiles in 13 with dark lightmaps |
| S2FM | needs the full game files to load maps; has a stretched-limb bug with a community fix ([guides index](https://deadlockmodding.pages.dev/modding-guides/)) |
| resourcecompiler | the CLI behind "Compile All Assets"; documented options on [source2.wiki](https://www.source2.wiki/EngineTools/ResourceCompiler) (`-i`, `-r`, `-f`, `-fshallow`, `-dependency_check_only`, `-crc`, `-skiptype`, `-filelist`); infers the mod from a path under `content/` |
| Launcher "Create VPK" | packs an addon's compiled folder; Divine calls it untested and recommends Source 2 Viewer instead ([pack a VPK](https://wiki.divineskins.gg/en/docs/deadlock/basics/pack-a-vpk)) |
| CS2 Workshop Manager | the CSDK 12 packing route: the upload "Submit" is expected to fail, and the VPK it writes on the way is the product; 2 GB per VPK ([packing guide](https://deadlockmodding.pages.dev/modding-guides/csdk12-packing), [creator errors](https://wiki.divineskins.gg/en/docs/deadlock/errors/creator-errors)) |

### What Valve's own Workshop Tools have that the Deadlock kits lack

- A tool build that ships with the game. Valve's tools for CS2, Dota 2 and Half-Life: Alyx install from Steam and update with their game. A CSDK is a frozen snapshot: CSDK 12 is pinned to the 2025-08-19 update and CSDK 13 to tools of early October 2026 ([CSDK 12](https://wiki.divineskins.gg/en/docs/deadlock/tools/csdk-12), [engine.md](../reference/engine.md#the-compiler-csdk-13)). Every Deadlock patch after that widens the gap between the compiler and the game. Verified.
- The game's own shaders. Compiled in CS2's tools, a Deadlock material shows red checkerboards ([CS2 Workshop Tools](https://wiki.divineskins.gg/en/docs/deadlock/tools/cs2-workshop-tools)); in CSDK 12 a model with no `DefaultMaterialGroup` is solid red ([model problems](https://wiki.divineskins.gg/en/docs/deadlock/errors/model-problems)).
- An AnimGraph editor. CS2 and Half-Life: Alyx document a node-based AnimGraph ([HL:A Workshop Tools](https://developer.valvesoftware.com/wiki/Half-Life:_Alyx_Workshop_Tools), from a search summary; page not readable here). Deadlock's AG2 graphs have no editor anywhere.
- A publishing path. All three of Valve's tool sets upload to the Steam Workshop ([PC Gamer on Dota 2's tools](https://pcgamer.com/dota-2-modding-tools-now-in-alpha-upload-custom-mapsmodes-to-steam-workshop), [PC Gamer on HL:A's](https://pcgamer.com/half-life-alyx-now-has-steam-workshop-support-and-official-mod-tools)). Deadlock has no Workshop, which is why the CS2 Workshop Manager's failed upload is a packing step.
- Scripting. Dota 2's tools give custom games Lua and Panorama JavaScript ([dota2.com, Reborn custom games](https://www.dota2.com/reborn/part2/?l=english)). Deadlock's client has none for mods; Panorama's JavaScript has no game API ([engine.md](../reference/engine.md#running-the-game)). Deadworks and Modlock fill this on the server side (section 4).
- A bug tracker. Dota's tools FAQ points to GitHub issues and a pinned bugs thread ([Dota Workshop Tools FAQ](https://developer.valvesoftware.com/wiki/Dota_2_Workshop_Tools/Dota_Workshop_Tools_FAQ), search summary). Deadlock's kits have a Discord.
- GPU lightmap baking in CS2's Hammer ([CS2 Workshop Tools](https://developer.valvesoftware.com/wiki/Counter-Strike_2_Workshop_Tools), search summary). CSDK 13's Hammer bakes dark.

One CS2 fact explains part of why CSDK 13 moved to Dota's tools: by a search summary of Valve's CS2 tools page, CS2's particle editor is "not currently enabled". Single report, and the reason for the move is not stated anywhere public; treat it as a guess.

Valve's tools have their own complaints, which the CSDKs inherit: CS2 tool crashes on certain Nvidia Studio drivers when previewing path-traced lighting ([Valve's CS2 tools issues page](https://developer.valvesoftware.com/wiki/Counter-Strike_2_Workshop_Tools/Issues), search summary), stalls on start ([Steam thread](https://steamcommunity.com/app/730/discussions/0/3821922030310050333)), and Valve's own Dota FAQ warns that file formats may change and force custom games to be rebuilt ([Dota Workshop Tools FAQ](https://developer.valvesoftware.com/wiki/Dota_2_Workshop_Tools/Dota_Workshop_Tools_FAQ), search summary).

## 2. What modders hate

Ordered by how often and how loudly the public pages raise it.

### 2.1 Patches break mods, silently

Every source agrees on this one. Verified.

- A mod ships whole copies of the files it replaces; a patch that moves a file leaves the copy at a dead path, and one that changes a file is masked by the copy ([Divine Skins, fix your mod after a patch](https://wiki.divineskins.gg/en/docs/deadlock/publishing/update-after-a-patch)). The City Never Sleeps update (2026-09-29) broke "most HUD mods, many animation mods, and some model mods" (same page, crediting [GameBanana's news post](https://gamebanana.com/news/21995)).
- The failure is quiet: "The mod does nothing and shows no error" ([mods not loading](https://wiki.divineskins.gg/en/docs/deadlock/errors/mods-not-loading)).
- Some failures are loud: an outdated HUD mod gives `FATAL ERROR: Unable to find child '...' in layout file '...'` at launch, and a mod carrying an old `scripts/abilities.vdata` gives `Error parsing KV3 data` ([crashes and matchmaking blocks](https://wiki.divineskins.gg/en/docs/deadlock/errors/game-crashes)). Old `scripts/*.vdata_c` also keeps new heroes locked and old stats live (update-after-a-patch page).
- Every update, or a "verify files", resets `gameinfo.gi` and drops the addons line, so every mod vanishes ([mods not loading](https://wiki.divineskins.gg/en/docs/deadlock/errors/mods-not-loading); [SteamDB's install guide](https://steamdb.com/en/deadlock/how-to-install-skins); [engine.md](../reference/engine.md#how-the-game-loads-a-mod)). City Never Sleeps also changed the stock SearchPaths (`Game_Language` became `Game_UILanguage`), so blocks pasted from older guides now half-work.
- How creators find out what changed today: Valve's changelog ("the notes never mention mods"), asking on Discord, opening the new VPK in Source 2 Viewer and checking each path by hand, or reading [GameTracking-Deadlock](https://github.com/SteamTracking/GameTracking-Deadlock)'s history of `heroes.vdata` (update-after-a-patch page). Valve has said nothing about mods: one guide reports no mention of mods, addons or VPKs in 40 Steam announcements from 2025-08-22 to 2026-11-05 ([phvieparchy guide](https://phvieparchy.org/articles/deadlock-mods-mod-manager.html), single report).
- Specific post-patch model breaks Divine lists: the model moved folders, the animation system changed (A-pose), the skeleton changed (and a shipped `.vnmskel_c` then breaks other mods for the hero too), camera attachments changed (camera in the middle of the screen), and Source 2 Viewer before 20.0 exported attachments wrongly ([update-after-a-patch](https://wiki.divineskins.gg/en/docs/deadlock/publishing/update-after-a-patch), [model problems](https://wiki.divineskins.gg/en/docs/deadlock/errors/model-problems)). One mod builder ships a repair for hero models compiled before "the October 2026 camera attachment change" ([deadlock-mod-builder](https://github.com/OverseerAlbedo/deadlock-mod-builder)), so that change was real and widespread enough to automate.

What Magician already does: `patchday` and `drift` say per mod what moved, down to bones and graph parameters; M040 and M041 name a changed or removed original; M070 refuses shipped skeletons, graphs and vdata; the baseline checks that the game still mounts the addons folder. What it does not do is listed in section 6.

### 2.2 The hero model swap is a multi-tool relay with many ways to fail

Verified: the same failures appear on Divine's model pages, GameBanana's model tutorial comments and two independent mod builders.

The documented route is Blender, then CSDK ModelDoc, then a second addon in CS2's Workshop Tools for the AG2 references, then back to CSDK for materials, then a hand-built staging folder ([hero model swap](https://wiki.divineskins.gg/en/docs/deadlock/models/hero-model-swap)). Each hand-off loses something:

- A-pose or a hero that doesn't turn: AG2 references missing, the final compile done in CSDK 12, a shipped `.vnmskel_c`, or the skeleton node not keeping its path ([model problems](https://wiki.divineskins.gg/en/docs/deadlock/errors/model-problems), [part 4](https://wiki.divineskins.gg/en/docs/deadlock/models/hero-swap-compile)).
- Camera in the middle of the screen: one of eleven camera attachments wrong, or a pivot angle of -89.972 where Valve has -90 from an old Source 2 Viewer decompile, or a reference copy overwritten by the creator's own compile ([part 3](https://wiki.divineskins.gg/en/docs/deadlock/models/hero-swap-attachments)). The rounding cause is one guide's account (single report).
- Health bar at the feet: CSDK 12 writes `CitadelUnitStatusSettings_t` as an array where the game expects one object ([DeadlockTools](https://github.com/dotryen/DeadlockTools), `fix unitstatus`; [model problems](https://wiki.divineskins.gg/en/docs/deadlock/errors/model-problems)). Two sources.
- Missing ability, voice or footstep sounds: `CitadelTaggedSoundSettings_t` or the feet nodes not copied ([model problems](https://wiki.divineskins.gg/en/docs/deadlock/errors/model-problems)).
- LODs break after the CS2 compile unless the DMX files are exported straight into the CS2 addon; ModelDoc can't load meshes from another addon ([part 4](https://wiki.divineskins.gg/en/docs/deadlock/models/hero-swap-compile)).
- Cloth lost: decompiled models "have no cloth simulation" ([GameBanana comment](https://gamebanana.com/tuts/19683), 2026-07-12). Magician's [engine.md](../reference/engine.md#the-compiler-csdk-13) records the same class of loss for distance fields (M142).
- Weights that won't attach, scale that differs between Blender's object and edit modes, broken bones ([GameBanana comments on the model tutorial](https://gamebanana.com/tuts/19683)), and `.001` material names leaving blank slots ([model problems](https://wiki.divineskins.gg/en/docs/deadlock/errors/model-problems)).

Two mod builders exist mainly to automate this relay ([Yelafree/DeadlockModBuilder](https://github.com/Yelafree/DeadlockModBuilder): ragdoll joints re-injected after decompile, jiggle bones, materials; [OverseerAlbedo/deadlock-mod-builder](https://github.com/OverseerAlbedo/deadlock-mod-builder): joint matching, cloth presets, the camera repair), and a third tool exists only to patch two of its outputs in a compiled model ([DeadlockTools](https://github.com/dotryen/DeadlockTools): `add ag2`, `fix unitstatus`). That three people wrote tools for it is the clearest evidence of the pain.

### 2.3 Source versus compiled, paths and names

Verified across Divine's creator errors, retexture and packing pages and GameBanana comments.

- Packing `.vmat`, `.png` or `.wav` from the content folder: "Nothing changes in game" ([retexture](https://wiki.divineskins.gg/en/docs/deadlock/textures/retexture)). DeadPacker users report the same with sound sources that stayed `.mp3` or `.wav` ([GameBanana audio tutorial comments](https://gamebanana.com/tuts/19072), 2025-10-04 and 2025-10-23).
- Compiled textures carry a hash, `abrams_coat_color_png_63291cfb.vtex_c`; a recompile makes a new one, so the new `.vtex_c` has to ship with the `.vmat_c` or the game shows a checkerboard. Shipping only a texture renamed to Valve's hashed name works "until an update changes the code" ([retexture](https://wiki.divineskins.gg/en/docs/deadlock/textures/retexture)).
- Paths must match Valve's byte for byte; staging-folder prefixes and skipped levels (`heroes_staging/...` without `models/`) are the usual errors; only `pak01_dir.vpk` to `pak99_dir.vpk` directly in `addons/` load; the lower number wins ([pack a VPK](https://wiki.divineskins.gg/en/docs/deadlock/basics/pack-a-vpk), [mods not loading](https://wiki.divineskins.gg/en/docs/deadlock/errors/mods-not-loading)).
- Template sample images left in an addon change other heroes' icons ([creator errors](https://wiki.divineskins.gg/en/docs/deadlock/errors/creator-errors)).
- Saving a source doesn't recompile it; the creator has to run "Recompile > Full" ([creator errors](https://wiki.divineskins.gg/en/docs/deadlock/errors/creator-errors)). Encoding changes only recompile while CSDK is open ([replacing sounds](https://deadlockmodding.pages.dev/modding-guides/replacing-sounds)).
- Tracing a sound runs vdata, then a sound event, then a `.vsnd`, plus a `.vrr` for voice lines ([replacing sounds](https://deadlockmodding.pages.dev/modding-guides/replacing-sounds)); recolouring a particle often hits an unused file, and the fix is typing `dumpparticlelist` in a match ([creator errors](https://wiki.divineskins.gg/en/docs/deadlock/errors/creator-errors)).

Magician catches most of this already (section 5).

### 2.4 What you see in the editor is not the game

Verified.

- "The preview in Source 2 Viewer does not show Deadlock's lighting exactly" ([retexture](https://wiki.divineskins.gg/en/docs/deadlock/textures/retexture)); its Deadlock shading is "still a work in progress" ([material reference](https://wiki.divineskins.gg/en/docs/deadlock/textures/material-reference)); black squares in its preview are a preview bug ([creator errors](https://wiki.divineskins.gg/en/docs/deadlock/errors/creator-errors)).
- CS2's tools have no Deadlock shaders (red checkerboards), and CSDK 13's Hammer bakes dark.
- Scene-wide NPR values (`g_flNPRRimLightStrength`, `g_flNPRDiffuseStepSharpness`) are set by the game for the whole scene; "adding them to your .vmat does nothing" ([material reference](https://wiki.divineskins.gg/en/docs/deadlock/textures/material-reference), a community reconstruction from build 6779; single source).
- Unconfirmed folklore travels between guides: three guides recommend "Write Depth Before Alpha Blending" on every hero material, while Valve's shader allows it only with Translucent on ([material reference](https://wiki.divineskins.gg/en/docs/deadlock/textures/material-reference)).

Every Divine guide ends in "check it in game". Magician's in-game pass and proofs exist for exactly this, and [engine.md](../reference/engine.md#source-2-viewer) says the same of Source 2 Viewer's renders.

### 2.5 Sound replacements that come out wrong

From [GameBanana's audio tutorial comments](https://gamebanana.com/tuts/19072), 44 posts on the second most-viewed Deadlock tutorial (76,936 views): the new sound plays "incredibly quiet and slightly sped up" (2026-07-28), "really really distorted" (2026-08-17), very quiet on one family of lines only (2026-07-07), or not at all (2025-10-27). Divine names two causes: outdated tools, and hero sound events that raise the pitch ([creator errors](https://wiki.divineskins.gg/en/docs/deadlock/errors/creator-errors)). Loop points need `.wav` sources and `lame_enc.dll` in the kit's `bin` ([replacing sounds](https://deadlockmodding.pages.dev/modding-guides/replacing-sounds)). The symptoms are reported several times; the causes are not established (single reports each).

### 2.6 Iteration is slow and packing is manual

- The loop for one change is edit, recompile in the Asset Browser, copy the `_c` files from the game folder into a staging tree that mirrors Valve's, pack in Source 2 Viewer or the Workshop Manager, rename to `pakNN_dir.vpk`, copy into `addons`, restart the game ([pack a VPK](https://wiki.divineskins.gg/en/docs/deadlock/basics/pack-a-vpk)). Verified.
- The tools people build fix this first: [DeadPacker](https://github.com/Artemon121/DeadPacker) (compile, pack, copy, close and launch Deadlock from one TOML), [Deadlock-Mod-Compiler](https://github.com/Predi-i/Deadlock-Mod-Compiler) (MD5 change tracking, batches of 25 to avoid Windows command-length crashes), and the builders above.
- Packing quirks: Source 2 Viewer's older releases fail to build VPKs; phantom duplicate entries appear and deleting one deleted the real file (one creator's report); the Workshop Manager caps a VPK at 2 GB ([creator errors](https://wiki.divineskins.gg/en/docs/deadlock/errors/creator-errors)).
- Installation itself is the managers' job and mostly works; its pains are pak number collisions, the 99-file cap, two managers fighting over `gameinfo.gi`, and binary-searching a folder of mods to find the one that crashes ([mods not loading](https://wiki.divineskins.gg/en/docs/deadlock/errors/mods-not-loading), [crashes](https://wiki.divineskins.gg/en/docs/deadlock/errors/game-crashes)).

### 2.7 Version drift between guides and tools

The same instructions exist in a CSDK 10, 12 and 13 form; GameBanana's guides cite CSDK 12; a crash page says its source guide "dates from December 2025, so results may differ"; Divine marks several fixes "untested on the current build" ([crashes](https://wiki.divineskins.gg/en/docs/deadlock/errors/game-crashes), [hero model swap](https://wiki.divineskins.gg/en/docs/deadlock/models/hero-model-swap)). A long-running community guide says of itself that it is "a lot of work to keep it up to date" (search summary of a public Google Doc that now returns 410). Verified.

### 2.8 Matchmaking blocks from tools and configs

`Unable to enter matchmaking while any party member is running Tools-mode, or has unsupported changes` when a party member has a creator tool open or has edited restricted `gameinfo.gi` sections (Engine2, MaterialSystem2, NetworkSystem, Particles, RenderSystem, SceneSystem, WorldRenderer), often through "performance" configs ([crashes and matchmaking blocks](https://wiki.divineskins.gg/en/docs/deadlock/errors/game-crashes)). Single source.

## 3. What modders like

Public praise is thin: the GameBanana comment threads are mostly requests for help, and Reddit could not be read. What follows is what the guides tell people to use, and what keeps getting rebuilt, which is a fair proxy.

- Source 2 Viewer. Every guide requires it, for browsing, exporting, decompiling, comparing with the original's Data tab and packing; its CLI does batch exports and chunked VPKs (`--vpk_create --vpk_create_chunk_size`) ([Divine tools](https://wiki.divineskins.gg/en/docs/deadlock/tools), [pack a VPK](https://wiki.divineskins.gg/en/docs/deadlock/basics/pack-a-vpk)). Keep: Magician already checks its decoders against it.
- Comparing with the original. Divine's fixes are almost all "open Valve's file beside yours and copy what differs": the Data tab, attachment counts, block-by-block text replacement ([part 3](https://wiki.divineskins.gg/en/docs/deadlock/models/hero-swap-attachments)). That is Magician's own principle (Valve's file is the reference), and creators already work that way by hand.
- No-install builders for simple mods: [Deadlock Forge](https://deadlockforge.net/) (sounds, item art, textures, merging; "no SDK, no BC7 encoder") and [DeadMod](https://redmser.github.io/DeadMod/). They exist because the CSDK route is heavy for a one-file change ([browser builders](https://wiki.divineskins.gg/en/docs/deadlock/tools/browser-builders)).
- One-click install, profiles and "Launch Modded" re-applying `gameinfo.gi` in Deadlock Mod Manager; replies to its Steam announcement call it "rock. solid." and "+1 for FOSS" ([Steam thread](https://steamcommunity.com/app/1422450/discussions/0/600789021066594301/)). Keep: Magician writes what it imports.
- Per-file conflict detection (Grimoire) over per-hero (DMM) is the point Divine's comparison picks out ([mod managers](https://wiki.divineskins.gg/en/docs/deadlock/tools/mod-managers)).
- Safety tags. Divine's material reference marks every flag Keep, OK, Careful or Avoid, with the visibility reason ([material reference](https://wiki.divineskins.gg/en/docs/deadlock/textures/material-reference)). The community's own fair-play line matches Magician's: never glow, go see-through, lose the outline or go very dark.
- Patch files for HUD mods. deadlock-modbox keeps each UI mod as a diff against decompiled game files, rebuilds byte-identical VPKs from the same mods and build, and names the mod whose patch no longer applies ([deadlock-modbox](https://github.com/rszyma/deadlock-modbox)); Divine recommends saving HUD changes as `.patch` files ([update-after-a-patch](https://wiki.divineskins.gg/en/docs/deadlock/publishing/update-after-a-patch)).
- Text-authorable effects and sounds. Recolouring and retuning through KV3 rather than the editors is common enough that vpkmerge, Deadlock Forge and DMM's Mod Foundry all do it in code.

## 4. The adjacent tools

| Tool | What gap it fills | What it exposes programmatically |
|---|---|---|
| [Source 2 Viewer](https://github.com/ValveResourceFormat/ValveResourceFormat) (VRF, MIT) | reading, decompiling, exporting and packing every Source 2 format | a CLI (exit codes 0, 1, 2; flags change between versions), the ValveResourceFormat C# libraries, and an opt-in MCP server in the Debug GUI ([engine.md](../reference/engine.md#source-2-viewer)) |
| [Deadlock Mod Manager](https://github.com/deadlock-mod-manager/deadlock-mod-manager) (GPL-3.0, about 630 stars) | installing, ordering, profiles, GameBanana browsing, `gameinfo.gi` upkeep, health badges after an update ("Can be fixed", "Can be partly fixed", [troubleshooting](https://docs.deadlockmods.app/using-mod-manager/troubleshooting)); an experimental Mod Foundry that imports, recolours and exports hero skins ([FAQ](https://docs.deadlockmods.app/using-mod-manager/faq)) | a backend API, plugin documentation, and a WebAssembly plugin system; the [creator-workspace-bridge](https://github.com/deadlock-mod-manager/deadlock-mod-manager/tree/main/plugins/creator-workspace-bridge) plugin stages each VPK a creator tool exports and deploys a chosen build to a testing profile. Magician's `bundle` and `source` already target DMM ([mod-source.md](../reference/mod-source.md)) |
| [Grimoire](https://github.com/Slush97/grimoire) (MIT) | a second manager: per-file conflicts, live 3D pose previews, merging | a Nix flake; no CLI or IPC documented |
| [vpkmerge](https://github.com/Slush97/vpkmerge) (MIT, Rust) | merging and splitting VPKs; recolouring textures and hero VFX; sound-event edits | a CLI and the `vpkmerge-core` crate (`merge`, `split`, `inspect`, `detect_conflicts`, `pack`), on the pure-Rust `morphic` decoder; DMM's Foundry uses its codecs |
| [Deadlock Mod Builder](https://github.com/OverseerAlbedo/deadlock-mod-builder) (AGPL-3.0, Tauri) | a five-step GUI from a rigged character to an installed hero replacement, driving Blender, the Source 2 Viewer CLI and CSDK 13; reports what each VPK in `addons` changes; repairs pre-October-2026 camera attachments | none documented; projects are `.dmb.json` |
| [DeadlockModBuilder](https://github.com/Yelafree/DeadlockModBuilder) (C#, WPF) | hero model replacement with ragdoll, jiggle bones and NPR material presets, driving S2V, CS2's tools and CSDK 12 | `build_mod.ps1 -Hero -Addon -DmxBody -DmxGun -VpkNum -GamePath`; paths hard-coded to the author's machine |
| [DeadlockTools](https://github.com/dotryen/DeadlockTools) (MIT) | writing AG2 references into a compiled model and fixing CSDK 12's unit-status array, so Linux users can skip CS2's tools | a CLI (`add ag2`, `fix unitstatus`, `print`); overwrites its input; outputs roughly double in size |
| [Deadlock Modworks](https://github.com/CyN1ckal/Deadlock-Modworks) (MIT) | a manager with model comparison, skeleton repair after updates, version history, a pack builder | `--version`, `--data-root` |
| [deadlock-modbox](https://github.com/rszyma/deadlock-modbox) | UI mods as diffs, rebuilt deterministically on Linux with the CSDK compiler under Wine | a Nix and `just` build; mods selected by name |
| [DeadPacker](https://github.com/Artemon121/DeadPacker) (MIT) | one-file compile, pack, copy and launch | a TOML of steps |
| [Deadlock-Mod-Compiler](https://github.com/Predi-i/Deadlock-Mod-Compiler) | incremental compile and pack with CSDK 12, MP3 to WAV | a PowerShell menu and a batch mode |
| [Deadlock Forge](https://deadlockforge.net/), [DeadMod](https://redmser.github.io/DeadMod/) | simple sound, image and texture mods in a browser | none |
| Moonah's Mod Maker ([GameBanana](https://gamebanana.com/tools/23422)) | a desktop app for sounds and other assets that downloads the compile tools on first run (per a search summary; the page text was not readable) | not known |
| Divine Skins ([wiki](https://github.com/DivineSkins/divine-wiki), [Celestial installer](https://divineskins.gg/install-mods)) | the largest written knowledge base (57 Deadlock guides), a creator panel for publishing; no building tool of its own | the wiki is Markdown on GitHub |
| [GameTracking-Deadlock](https://github.com/SteamTracking/GameTracking-Deadlock) | what each build changed, as git history of Valve's files | git |
| [Deadworks](https://deadworks.net/) | custom dedicated servers: a launcher, hosting, and a C# plugin SDK (events, entities, particles, modifiers, HUD messages); beta, custom models, sounds and vdata at alpha | C# `DeadworksPluginBase` |
| [Modlock](https://pkg.go.dev/github.com/paralin/modlock) (MIT) | custom game modes in TypeScript, JavaScript, Luau, Python or Go under a Wasmtime sandbox on the server; looks fields up by name so mods survive updates; pre-1.0 | `modlock new`, `dev`, `publish`; a protobuf schema |

What none of them does: judge a mod against the installed build's own files before it ships, or say after a patch which mods broke and why without a person opening Source 2 Viewer. DMM's badges and Modworks' skeleton repair come closest on the player side, and their rules for what "can be fixed" are not public.

## 5. Recurring creator errors, and whether a target-aware tool catches them

From Divine's [errors when making mods](https://wiki.divineskins.gg/en/docs/deadlock/errors/creator-errors), [model and skin problems](https://wiki.divineskins.gg/en/docs/deadlock/errors/model-problems), [mods not loading](https://wiki.divineskins.gg/en/docs/deadlock/errors/mods-not-loading), [crashes](https://wiki.divineskins.gg/en/docs/deadlock/errors/game-crashes) and [pack a VPK](https://wiki.divineskins.gg/en/docs/deadlock/basics/pack-a-vpk). "Catches" was checked against `magician rules` and the code on 2026-11-27.

| Error on the community pages | Magician today | Could, with the build's own data |
|---|---|---|
| Source files packed (`.vmat`, `.png`, `.wav`) | yes: M010 (error), M011 | |
| Path differs from Valve's, wrong case, staging prefix | partly: M001 case, M002 shape; a wrong-but-valid path is M031 (orphan) | suggest the Valve path it most likely meant (same file name elsewhere in the build) |
| Recompiled material without its new hashed `.vtex_c` (checkerboard) | yes: M030 | |
| Texture renamed to Valve's hashed name breaks on update | yes, at the patch: M041 | |
| Misnamed VPK, pak outside 01 to 99 | yes: M050; `install` picks slots | |
| `.vnmskel_c`, `.vnmgraph_c` or `scripts/*.vdata_c` shipped | yes: M070 (error) | |
| Unchanged compiled files shipped | yes: the release gate blocks files byte-identical to Valve's | |
| Compiler's default textures shipped | yes: M071 | |
| Template sample images changing other heroes' icons | yes: M090 when the file is shared, M092 when only another hero uses it (landed after this note) | |
| A-pose: AG2 references missing, skeleton path wrong | yes: M062 errors on a missing or other skeleton or AG2 reference (landed after this note) | |
| Camera centred: camera attachment wrong | yes: M062 compares every attachment with Valve's, and holds the camera attachments the game names to 1e-4 (landed after this note) | |
| Health bar at the feet: `CitadelUnitStatusSettings_t` an array | yes: M062 (landed after this note) | |
| Missing hero sounds: `TaggedSoundSettings_t` (the community pages say `CitadelTaggedSoundSettings_t`) or feet nodes lost | yes: M062, as fair play, since footsteps are a cue (landed after this note) | |
| Camera settings or hitboxes changed | yes: M062 and M129 for hitboxes, M062 for `CitadelCameraSettings_t` (fair play, landed after this note) | |
| Solid red mesh, blank slot from a `.001` material | yes: M153 names a draw call with no material, the error material or a bare slot name that is no material (landed after this note) | |
| New material on the wrong shader (Material Editor default) | partly: M131 and M132 fire on the missing flags | say the shader is the cause |
| NPR lighting or status proxy off | yes: M132, M131 | |
| Translucent, self-illum, outline off (Divine's Avoid list) | yes: M152, fair play, a feature on the Avoid list that no material of Valve's model at the path uses (landed after this note); albedo M136 and M137, alpha M138 and M139, silhouette M124 | |
| Odd texture sizes | yes: M073, M135, M147 | |
| Bad weights, more than four influences, weapon not parented | yes: M116, M117, M119, M111 | |
| Cloth or ragdoll lost after a decompile | yes: M142 for distance fields, M160 for cloth and ragdoll parts (landed after this note) | |
| Particle recolour on an unused file | yes: M084 (landed after this note) | |
| Particle function in the wrong list (crash) | yes: M083 | |
| Particle file won't open after decompile (header) | no | Magician builds particles from text; it could name the S2V decompile header if the compiler rejects it |
| Sound quieter or louder than Valve's | yes: M063, M064 | |
| Sound sped up, distorted, pitch raised by the event | a note: M065 says another rate or channel count than Valve's, with the event's pitch (landed after this note); heard on 6880, the format is not the cause | the cause, which is the content or the event |
| Outdated HUD layout (`Unable to find child` crash) | yes: M042, an error for an id the client code names (landed after this note); M040 warns that Valve changed the original | |
| Old vdata (`Error parsing KV3 data`, locked heroes) | yes: M070 | |
| `gameinfo.gi` reset after an update | yes: the baseline and `doctor` check the mount | |
| Broken `gameinfo.gi` edit, missing `Mod` or `Write` lines, restricted sections changed | yes: `doctor` reads it against the stock backup (landed after this note) | |
| CSDK setup: OneDrive, `.binOFF`, Steam not running, uppercase addon | mostly: `magician csdk` (`crates/magician/src/csdk.rs`) names OneDrive, `.binOFF`, the tagged-sound gap, `SH2_lum`, particle preview, and the CSDK's game build against the installed one; `doctor` raises the first three when they apply | add addon names and Steam running |
| Two mods ship the same file | yes: `install --status` lists shared paths and which pak wins (landed after this note) | |
| Mod over 2 GB, chunked sets shipped incomplete | yes: M051 | |

## 6. Improvements, ranked

Ranked by how much pain each answers (section 2) against its effort. Effort: small is days, medium a week or two, large longer.

1. Hero model game-data contract. Pain: the four most common model problems on Divine's page (A-pose, centred camera, health bar at the feet, missing sounds) all come from blocks of the model document that differ from Valve's. Deadlock-specific: the blocks are Deadlock's own classes (`CitadelUnitStatusSettings_t`, `CitadelCameraSettings_t`, `CitadelTaggedSoundSettings_t`, `CCitadelHeroModelGameData_t`, the skeleton and AG2 references), and the reference values are in Valve's compiled model at the same path on the installed build, which an engine-generic tool doesn't have. Extend M062 to compare them, an error for a missing skeleton or AG2 reference and for the unit-status array, fair play for camera settings. Medium.
2. Camera attachments held exactly. Pain: the centred camera, and the October 2026 camera change that a mod builder now repairs. Deadlock-specific: the eleven attachment names and their role are Deadlock's; Valve's values come from the build. Tighten M062 for those names to an exact match and say when a pivot is a near-integer angle off by a decompile's rounding. Small.
3. Panorama layout drift as an error. Pain: the launch crash after most patches for HUD mods, which was the City Never Sleeps update's biggest casualty. Deadlock-specific: which element ids the game's other layouts and code ask for is readable from the new build's decompiled `panorama/`; `patchday` already knows which originals changed. Error when a replaced `.vxml_c` lacks an id Valve's current copy has. A later step is a diff-based authoring route like modbox's, so a HUD change re-applies to the new file instead of masking it. Medium, then large.
4. Compiler provenance in every finding about a model. Pain: "final compile done in CSDK 12", CS2's compile breaking LODs, and guides that disagree on the kit. Deadlock-specific: `magician csdk` reads the kit's version, its game build and the installed build; a compiled file's edit info names its compiler. Record per file which compiler built it, and when a finding has a known kit cause (the unit-status array, AG2 clips in old form), say so. Small to medium.
5. Sound replacement parity. Pain: the most-commented GameBanana tutorial's complaints (quiet, sped up, distorted). Deadlock-specific: Valve's `.vsnd_c` at the same path gives the sample rate, channels and length to match, and the hero's sound event gives the pitch the game applies. Warn on a mismatch and print the event's pitch and volume beside it. The causes are not established, so ship it as a warning and let `witness` calibrate it. Medium.
6. Dropped cloth and ragdoll. Pain: "there is no cloth simulation" after a decompile, ragdoll joints lost (one builder re-injects them). Deadlock-specific: Valve's FeModel and ragdoll at the same path are on the build, and Magician already decodes both. A warning when the replacement has no cloth or fewer ragdoll parts where Valve's has them, like M142 for distance fields. Small to medium.
7. Materials named by cause. Pain: the Material Editor's default shader, red meshes, `.001` blank slots, and Divine's hand-kept Avoid list. Deadlock-specific: Valve's 776 hero materials on the build give the shader, flags and fair-play range; a generic linter can't know `F_USE_NPR_LIGHTING` matters. Say the shader is wrong when it is, name empty or error-material draw calls, and add a material-flag fair-play rule (translucent, self-illum, outline off) against Valve's hero materials. Small.
8. Particle reachability. Pain: recolouring an effect nothing plays, found today with `dumpparticlelist` in a match. Deadlock-specific: the hero's abilities and models name their particles, and `index` already builds the users index. Say when a replaced particle is reached by nothing. Small.
9. Collisions with other installed mods, and a sound `gameinfo.gi`. Pain: "another mod wins", binary-searching `addons`, broken SearchPaths after City Never Sleeps, matchmaking blocks. Deadlock-specific: the build's stock `gameinfo.gi` and the load-order rule are known. `install --status` and `doctor` could list every path two installed paks share and who wins, check the SearchPaths block against the stock file (the `Mod` and `Write` lines, `Game_UILanguage`), and name restricted sections a config changed. Small.
10. Cross-hero leftovers. Pain: template images that changed other heroes' icons. Deadlock-specific: `footprint` knows every file each hero is made of. Warn when a hero mod replaces another hero's files that are not shared. Small.
11. A per-hero changelog of each build. Pain: creators read Valve's changelog, which never mentions mods, and GameTracking's raw diffs. Deadlock-specific: `baseline` and `drift` already hold every path, CRC, bone and graph parameter per build. Print, per hero, what the new build changed in its footprint, as text a creator or a mod page can read, whether or not a Magician mod exists. Small to medium.
12. Hand-off to Deadlock Mod Manager's creator bridge. Pain: the copy-rename-restart loop. Deadlock-specific only in that DMM is the manager Deadlock players use. Let `release` drop its VPK where the [creator-workspace-bridge](https://github.com/deadlock-mod-manager/deadlock-mod-manager/tree/main/plugins/creator-workspace-bridge) stages builds, so a creator testing through DMM gets Magician's linted bytes. Small.
13. AG2 references without CS2's tools. Pain: the two-SDK relay, and Linux creators who can't run CS2's tools. Deadlock-specific: the right skeleton and graph paths come from Valve's model on the build (Kelvin, Viscous and Ivy use shared or renamed ones). Magician writes binary KV3 already; writing the references into a compiled model, as DeadlockTools does, then judging it with the contract above, removes the CS2 step. Medium to large, and it edits compiled output, which needs the same care as `scene`.

### What landed

Items 1, 2, 3, 5, 6, 8, 9 and 10 landed after this note, measured on 6877 against Valve's files, the 75 community mods and two finished skins; the record is [rule-calibration-6871.md](rule-calibration-6871.md), "The hero model contract and its neighbours (6877)". Item 4 and the rest of item 7 landed on 6880, measured on 144 community mods; the record is the same file's "Judgement round on 6880".

- 1: M062 compares the animation skeleton and AnimGraph 2 references, the unit-status block, the tagged-sound sets, the camera settings and every other game-data block with Valve's model at the same path (`crates/lint/src/gamedata.rs`). Tagged sounds turned out to be fair play: `heroes.vdata` names none, so a model without them has silent footsteps. The collision fields were left alone, because `heroes.vdata` sets its own.
- 2: the camera attachments are held to 1e-4 of Valve's numbers. Which ones comes from the model's `AttachmentCameraData` and from the hero-frame attachments the client library names, not from a list of names.
- 3: M042, an error for a missing id the client code names, a warning for the rest.
- 5: M065, a warning, then a note once heard in game (6880).
- 6: M160.
- 8: M084, from the list of particles nothing names that `magician index` now writes.
- 9: `install --status` lists the paths installed paks share and which one the game reads; `doctor` reads `gameinfo.gi` against the stock backup.
- 10: M092.
- 4: M170 says which compiler built each kind of file, from the versions in its edit info against the game's own files of that kind, and names CSDK 12 where its outdated AnimGraph 2 output is the known cause; M062's A-pose findings name CSDK 12's missing AnimGraph 2 references, as its unit-status finding already named the list. `build` names the kit it compiles with, as before. Which number each kit writes is not read from the kits here.
- 7: M151 says the shader (before this round), M153 the empty or error-material draw call, M152 the Avoid list as fair play.

What not to take on: Hammer lightmaps (`SH2_lum`), a particle editor preview, or an AnimGraph editor. Hero skins are lit by the game, and the hero-page scene uses dynamic lights ([engine.md](../reference/engine.md#the-hero-page)); the editors are GUI work outside Magician's non-goals ([PDD](../PDD.md#non-goals)). Magician's answer to "the editor doesn't look like the game" is the in-game pass, which every community guide already ends with by hand.

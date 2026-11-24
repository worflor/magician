# Deadlock and Source 2: what Magician relies on

Facts about the game, its compiler and the tools around it that Magician's code and docs depend on, with the scars that taught them. Everything here was checked on build 6861 or 6864 unless it says otherwise. **UNVERIFIED** marks a claim nobody has seen in the running game yet; everything else was read from Valve's files, from the compiler's output, or seen in game.

Facts about Magician itself (its files, commands and gotchas in its own code) live in [../TDD.md](../TDD.md) and [../../CONTRIBUTING.md](../../CONTRIBUTING.md). This page is about the platform underneath.

## How the game loads a mod

| Area | Fact |
|---|---|
| Install | The game is `...\steamapps\common\Deadlock\game`; its build number is `ClientVersion` in `citadel\steam.inf`. Assets are in `citadel\pak01_dir.vpk` (with `pak01_NNN.vpk` chunks) and `core\pak01_dir.vpk`. The retail install ships no `resourcecompiler.exe`. |
| Mount | Stock search paths are `Game_LowViolence citadel_lv`, `Game citadel`, `Game core`. A mod is mounted by adding `Game citadel/addons` above `Game citadel` in `citadel\gameinfo.gi`. Every Steam update or "verify files" resets that file, and the mods silently vanish while their paks stay in `addons\`. |
| Load order | Addon paks are `pak01_dir.vpk` to `pak99_dir.vpk`, and **the lowest number wins** a path both ship. Deadlock Mod Manager encodes the same rule. |
| Compiled files | Compiled resources end in `_c`: `.vmdl_c`, `.vmat_c`, `.vtex_c`, `.vpcf_c`, `.vsndevts_c`, `.vsnd_c`. |
| Heroes | A hero's model path is `m_strModelName` in `scripts/heroes.vdata`. Internal names differ from display names: Vindicta is `heroes_staging/hornet_v3/hornet`, Grey Talon `heroes_staging/archer`, Abrams `heroes_wip/abrams`. Keys can share a model: an unreleased key uses `lash.vmdl`, Paradox's `chrono.vmdl` serves three keys, about twelve test keys use `gen_man`. |
| Rig | Each hero ships `<name>.vmdl_c` and `<name>.vnmskel_c` (the AnimGraph2 skeleton); graphs live in `animgraphs/animgraph2/hero/` and compile under a `+`-joined name, `hero.vnmgraph+lash.vnmgraph_c`. A replacement must keep bone names, hierarchy and attachments. New animation states are not possible; replacing existing clips is. Patches have moved attachments and changed rigs before, so every build is a possible rig break: that is what `drift` and the rig sidecar watch. |
| Model document | A `.vmdl_c` holds far more than meshes. Lash's has 19 hitbox capsules, 49 attachments, 388 anim events, 15 cloth chains and a softbody, 3 LOD groups, and its reference list names the hero's ability particles and anim-sound tags. A model built from scratch silently drops all of that (lint M061), so a mesh swap goes inside Valve's model document. |
| Particles and sound | Ability particles are `particles/abilities/<hero>/*.vpcf_c`; sound events `soundevents/hero/<hero>.vsndevts_c`; voice lines `sounds/vo/<hero>/`. Sound-event files name their `.vsnd` files as strings in their data, not in the reference list. |
| Hero page | The hero page renders `m_strUIPortraitMap`, a small map VPK at `maps/ui/hero_prefabs/<hero>.vpk`: a camera, dynamic lights and a prop. An addon copy at `citadel/addons/maps/ui/hero_prefabs/<hero>.vpk` replaces Valve's and is read on the next hover. The hero drawn there is the panel's own, posed by the `hero_ui` AnimGraph, not the map's prop. The map is client-only, so no entity I/O (`logic_auto` and `logic_timer` are refused). The dark cut-paper silhouette behind the hero is a world mesh of the map (`worldnodes/..._hero_picker_shadow.vmdl_c`), and a model compiled at that path replaces it. This is why `magician scene` exists. |

## Formats

| Area | Fact |
|---|---|
| Resource blocks | Big hero models have 100 or more blocks (an MVTX, MIDX and MDAT set per mesh). Bound the block table by file size, never by a constant. |
| KV3 | Data blocks (`DATA`, `CTRL`, `PHYS`, `ASEQ`, `AGRP`, `MDAT`) are binary KV3 v4 or v5, LZ4 or uncompressed; some model `ANIM` blocks and map entity lumps use zstd. Magician decodes all 18,187 of the game's particles. |
| KV3 writing | A block with blobs keeps them in blocks: the count and bytes in the header, a u32 size per blob after the type bytes, `0xFFEEDD00`, the blob data, and a second `0xFFEEDD00`. Without the second marker Source 2 Viewer stops at the end of the stream; with blobs inline it indexes out of range. Magician's own decoder accepts every layout, so reading back its own write proves nothing. |
| Meshes | Build 6864 encodes model buffers with meshoptimizer codec version 1 (vertex `0xA1`, index `0xE1`), which older decoders refuse. Index buffers hold absolute vertex ids; `m_nAppliedIndexOffset` and `m_nVertexCount` are metadata. Blend indices count in the MDAT's own skeleton, matched to the model's by bone name. Vertex-colour streams are zstd. Hero models are hundreds of shells (Valve's Lash welds into 204), and the largest is often a garment, not the body. |
| Bones | Bone count is not capped at 256: Lady Geist's model has 622 skinned joints (her proxy cloth's nodes included). But Source 2 Viewer's glb export decoded Lash's retargeted clips exactly only while a replacement stayed small (215 bones); at 235 and above 43 clips differed (fingers, head up to 4.5 degrees). Cause unknown; the in-game effect is UNVERIFIED. |
| Textures | `VTexFormat` on 6864: 1 DXT1, 2 DXT5, 4 RGBA8888, 19 BC6H, 20 BC7, 21 ATI2N, 27 ATI1N, 28 BGRA8888. Hero colour maps are BC7 stored uncompressed, smallest mip first. A colour texture also carries what the compiler packs into its alpha (translucency or metalness), so a colour edit can move the packed alpha. Panorama's image-list route stores small images as YCoCg DXT5 (Y in alpha); read as plain DXT5 they look transparent and wrong. Of 12,598 game textures outside `panorama/`, 143 are not power-of-two, none under `models/`. |
| Valve's loose ends | The shipped game has references that resolve to nothing (`soundevents_manifest_devonly`, a test book, three Haze and Viscous particle refs), and Lash's `Lash_Use_Ground_Pound` response names a sound event nobody defines, so about one Ground Strike in eleven logs `Attempting to play invalid sound event`. That is vanilla. Lint blames a mod only for references it introduced. |

## The compiler (CSDK 13)

Valve's `resourcecompiler.exe` comes with the community CSDK (CSDK 13 is built on the Dota 2 Workshop Tools). Setup: unpack it outside the game folder and outside OneDrive, rename `readonly_tools_asset_info.binOFF` to `.bin`, keep addon names lowercase without spaces, and have Steam running. Sources live in `<csdk>\content\citadel_addons\<addon>\`, outputs land in `<csdk>\game\citadel_addons\<addon>\`.

| Issue | What happens | How Magician handles it |
|---|---|---|
| Byte-order mark | A UTF-8 BOM at the start of a vtex, vmat, vpcf, vsndevts, vmdl, vdata, vcss, svg, kv3 or dmx source makes it fail (`Expecting a .vtex file`). Windows PowerShell 5.1's `Set-Content -Encoding utf8` writes one. | `build` refuses such a source by name and never rewrites it. Write with `[IO.File]::WriteAllText`. |
| Hang | It can print `OK: N compiled, 0 failed` and then hang on exit. | `build` ends it 20 s after the summary. |
| Whole-addon input | Fed `-i <addon>\* -r` it tries every png, dmx and `.gitignore`. | `build` compiles real sources only. |
| Stale outputs | Outputs whose source was deleted stay in the game-side folder forever, and a live file that still names one pulls it into the pak. | `build` packs only outputs that map to a live source and fails on a live-to-stale reference; lint M032 catches it in a shipped pak. |
| Default textures | It writes its own copies of the engine's default textures (`materials/default/default_*_tga_*.vtex_c`) into every addon that compiles a material. Shipped, they replace the global fallbacks. | `build` leaves them out; lint M071. |
| Content tree | `-i` refuses a file outside the CSDK content tree (`Unable to determine mod from file`). An addon junctioned into `content\citadel_addons\` compiles through the junction path. | Keep addon sources in a repo and junction them in. |
| Roughness | Roughness textures must be power-of-two (`Cannot filter non-power-of-two texture`), or the vmat fails. | Bake at power-of-two sizes. |
| Zero-area triangles | A render mesh loses some zero-area triangles (46 of about 16k on one body); Valve's own bodies keep all of theirs. Nothing visible is lost. | Accept a shortfall up to the mesh's own zero-area count. |
| Model-only bones | Bones a render DMX declares (cloth chain bones) are sorted into the hierarchy, so later Valve bone indices shift. Animation binds by name and decodes identically. | Compare models by bone name, never by index. |
| Round trip | Recompiling Valve's own decompiled `lash.vmdl` unchanged is not byte-identical to the shipped file (renumbering, re-encoded animation, 1e-6 float noise). | Prove a mesh swap against that round trip, not the shipped file. |
| Quaternions | Attachment rotations can come out with either sign, `(0,0,1,0)` against Valve's `(-0,-0,-1,-0)`: the same rotation. Cloth compiles to `$`-named nodes in the bone list. | M062 compares rotations sign-free and skips `$` nodes. |
| Tagged-sound clips | Clips with a `CNmClipDocEvent_TaggedSound` event fail under CSDK 13's default `bin\win64` (the Dota tools of Oct 7): `Failed to allocate an instance of class 'CNmClipDocEvent_TaggedSound'`. | `build` picks `bin\win64_nm` for such clips: make it as a copy of `win64` with Deadlock's own (Oct 20) `resourcecompiler.dll` and `animationsystem.dll`. An addon's `.magician-compiler` file or `--bin` names another folder. |
| Panorama images | Images compiled through a `panorama_image_list` come out with one mip (Valve's hero cards carry nine), so they alias when drawn small. | Compile those through a per-image `.vtex` (`BGRA8888`, `Box` mips, `m_bNoLod 1`). |

### Skinning streams, hitboxes and body groups

- A mesh with eight influences per vertex does not store them as its attribute formats read. `BLENDINDICES` as `R16G16B16A16_UINT` is eight one-byte bone indices, as `R32G32B32A32_SINT` eight 16-bit ones (models past 255 bones: Abrams has 499); `BLENDWEIGHT` as `R16G16B16A16_UNORM` is eight one-byte weights. `R8G8B8A8_UINT`, `R16G16B16A16_SINT` and `R16G16_SINT` are four, four and two indices; `R8G8B8A8_UNORM` and `R16G16_UNORM` four and two weights. Checked against Source 2 Viewer's reader and on every hero of 6865 (weights sum to 1 on all of them).
- A hitbox (`m_nShapeType`) is a box in its bone's frame between `m_vMinBounds` and `m_vMaxBounds` (0), a sphere at `m_vMinBounds` (1) or a capsule between the two points (2), the last two with `m_flShapeRadius`. Most heroes are capsules only; Viscous's and Haze's heads and Dynamo's body are spheres, Haze's torso and Dynamo's pack boxes.
- A model draws one choice of each body group. `m_meshGroups` names the choices (Viscous: `default_@0_#&body`, the standing hero, and `default_@1_#&inflated`, the Goo Ball), `m_refMeshGroupMasks` gives each mesh the choices that draw it, and `m_nDefaultMeshGroupMask` the choices drawn by default. Meshes of two choices are never on screen together, and another choice has its own hitbox set (`inflated`). Magician measures the default choice against the `default` set; what another choice looks like is not judged.
- Valve's own surface lies outside its hitboxes on about half the heroes of 6865 (guns, Baba's bench, props laid on the ground in the bind pose, bodies the capsules were never fitted to): up to 48 units and half the visible surface. A fair-play measure of overflow is only meaningful against Valve's model at the same path.

### Cloth

What the compiler makes of a ClothChain joint, read back from the compiled FeModel (`magician cloth`, or `Source2Viewer-CLI -b PHYS`), measured on Valve's Lash and on lab compiles:

- `goal_strength` g compiles to `flAnimationForceAttraction` F = g³, exactly.
- `goal_damping` d compiles to `flAnimationVertexAttraction` V = F + h(g, d), and V is 0 whenever g is 0 (a goal-0 node has no damping at all). h is about 2d / g^1.5 while small and saturates toward 1. The old rule of thumb `1 - (1 - d)^64` is right only near g 0.1. Read V back instead of trusting a mapping.
- `gravity_z` compiles to `flGravity` = 360 × gravity_z; `stray_radius` to `m_AnimStrayRadii`; `bend_spring 1` to a rod two joints up plus a bend; `antishrink a` to a minimum rod length of a × rest; `suspender` to a stretch-only rod from the root; per-joint `drag` to nothing.
- `extrude_sides 2` turns a joint into two side nodes `$cc<joint>_0/_1` along the joint's local Y. A joint's parent may be any earlier joint, so chain trees compile.
- The FeModel carries each node's rest transform in model space (`m_InitPose`: position, scale, rotation), and a collision shape's spheres are in the frame of its node. Rods are compiled from that pose: on 6865 the rest distance lies inside the rod's interval on 44 of 47 heroes and on all three reference skins, so a rod that disagrees with the rest pose is not something a mod produces.
- Valve's cloth is not tidy, which is why Magician has no absolute rule on it. On 6865: free nodes rest inside a collision shape of their own layer on most heroes (Lash's neckties 3 units into the chest capsule, Werewolf's tail 10); the way out of that shape is further than the rod to a static node allows on 16 heroes; half of all rod and shape pairs could pass the shape between two nodes; goal-0 nodes with no damping exist on seven heroes; and a node's linear settling time runs from 0.2 s to over two minutes (Wraith). A cloth rule has to compare a mod's group with Valve's group at the same place, or stay silent.
- A cloth proxy DMX's per-vertex maps compile the same way (`cloth_goal_strength_v2` to g³, `cloth_goal_damping` by the law above, `cloth_gravity` as given).
- Valve's free nodes are not all "g 0.1": Lash's neckties, hair and rope are 0.375; Celeste's ponytail is 0.1.
- Shapes and nodes carry collision layer bits. That a node and a shape must share a layer to collide is UNVERIFIED in game.

## Particles and lights

| Issue | Fact |
|---|---|
| Function lists | A particle function in the wrong list is "not found while loading" and can crash the game: 13 particles with `C_OP_WorldTraceConstraint` in `m_Operators` crashed `particles.dll` the first time the ability played. All of Valve's 133 uses sit in `m_Constraints`. Lint M083 checks every list against Valve's classes. |
| Parents that fire | A child effect only plays if its parent emits. A decal hung on Lash's Ground Strike `m_StompImpactParticle` never showed; under `m_StompLineParticle` it did. Hang effects on a parent you have seen render in game, or ship tinted probes under each candidate and look. |
| Light cookies | A light cookie (gobo) must be BGRA8888 with a 512x512 mip, or the light ignores it (`wrong format DXT1 - expected BGRA8888`, `has no mip levels of size 512x512`). Valve's 15 cookies are all 512 BGRA8888. Lint M072. |
| Bone-pinned props | `C_INIT_CreateOnModel` with a hitbox plus `C_OP_LockToBone` (rigid rotation lock) keeps a `C_OP_RenderModels` prop on that bone. With `m_bOrientZ` off it renders as the bone's rotation turned +90° about local Y, so author the prop pre-turned −90°. |
| Inside-out surfaces | A surface wound inside out draws flat black in game and looks right in Source 2 Viewer and Blender, which draw back faces. A normals-against-winding check passes when a whole piece is consistently inward. Judge facing against the shape (normals against the limb's radial direction, or a closed piece's signed volume), and render proofs with back-face culling on. |

## Source 2 Viewer

Source 2 Viewer (ValveResourceFormat, MIT) is the independent reader every Magician decoder is checked against, and the exporter the Blender QA reads. Pin its version: CLI flags change between releases. 21.1 is the one used here.

- `Source2Viewer-CLI -i steam:1422450/game/citadel/pak01_dir.vpk -l` lists; `-d -o <dir>` decompiles; `--gltf_export_format glb` exports; `-b PHYS` prints a block.
- `-d` on a single compiled file also writes the exported PNGs next to the input. Decompiling inside an addon's game folder litters it (lint M011 catches what ships).
- The glb export carries LOD0 only. It loads a model's clips from the input VPK before the game, so a mod's model and its clips packed into one VPK export together.
- Blender truncates ID names to 63 bytes, so long clip paths collide on import and one clip silently stands in for another. `qa/glb_clips.py` renames them first.
- The Debug GUI has an MCP server (`--mcp`, `tools/s2v_mcp.ps1`). While it has had a model open it keeps the game's paks mapped, `citadel\addons` included, and `magician install` then fails with `os error 1224`. A game launched after that runs the old pak. Stop the viewer before installing or launching, open files from a scratch copy, and check `magician install --status` shows the crc you meant.
- Its renders are shading evidence, not proof of what the game does.

## Running the game

| Area | Fact |
|---|---|
| Test maps | The Hero Sandbox is `new_player_basics` (`selecthero hero_<name>` inside it). `map hero_testing` crashes `client.dll` about 25 s after load on 6864, mods or not; so does launching with `-dev` when it loads. |
| Commands | Retail has no netcon. Release commands include `map`, `exec`, `citadel_hud_visible`, `host_framerate`, `playdemo`, `demo_gototick`, `demo_pause`; with `sv_cheats 1` also `noclip`, `setpos`, `mat_wireframe`, `r_drawparticles`, `host_timescale`. `screenshot`, `startmovie`, `reload_model` and `freecamera_*` are dev-only and dead in retail: capture through the OS. `thirdperson` and `cam_*` do nothing. |
| Clean frame | `citadel_hud_visible 0` hides the HUD but not overhead bars. `r_drawpanorama 0` also hides your own bar and the console itself (the harness then drives it blind), but not other units' bars, which only an override of `panorama/styles/unit_status_v2.vcss` removes. `exec` finds `cfg/*.cfg` inside an addon VPK. |
| Repeatable shots | Record once, then `playdemo`, `demo_gototick` and `demo_pause` to the same tick on every build. A SourceTV take (`tv_enable 1` before `map`, recorded to `citadel/replays/auto-*.dem`, 64 ticks a second) is the one with a free camera (`spec_goto x y z pitch yaw`). Left Shift is the demo spectator's fly-up even with the console open, which is why `console.py` types with Right Shift. More in [../../cinema/README.md](../../cinema/README.md). |
| Lights at runtime | `ent_create` takes no keyvalues, and lights spawned that way light nothing. Particle lights are the only placeable ones. |

## Hazards outside Magician's own code

| Area | Issue | What to do |
|---|---|---|
| Steam | Updates stage on the library named by `StagingFolder` in `appmanifest_1422450.acf`; a full system drive fails them. A repair can be started headless with `steam.exe -silent steam://validate/1422450`. | Point staging at the library's own drive. |
| Mod managers | Deadlock Mod Manager 2.3.0 renumbers every `pakNN_dir.vpk` in `addons`, including files it didn't install, and rewrites `gameinfo.gi` in its own layout. Magician's ledger then names files that are gone. | Magician follows each ledger row's crc to the pak that now holds those bytes (`install --status` shows the move, `patchday` checks it under its new name, the next install or uninstall writes it down). An update of a renamed pak is refused with the `--as` that replaces it in place. |
| Two installers at once | Two installs or uninstalls at once each rewrote the ledger from what they read, and one row was lost. | Magician holds `.magician-installed.lock` in `addons` for each read, change and rewrite of the ledger; another installer that edits the folder at the same time is outside it, so don't run one beside Magician. |
| Stuck keys | A scenario that dies between `@down ctrl` and `@up ctrl` leaves Ctrl held for the whole desktop: every typed command becomes a chord and Esc opens the Start menu. A `.console-blind` file left by a closed game sends the next one blind from the wrong state. | `console.py` releases held keys however a run ends, clears stuck modifiers first, and trusts the blind-state file only if it is newer than the running game. |
| Backslashes | A Windows path typed into a Python string or a shell heredoc loses its backslashes to escapes (`\f`, `\v`, `\n`). | Write docs with file tools; in code use raw strings or forward slashes. |

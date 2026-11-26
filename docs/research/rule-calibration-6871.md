# Rule calibration on build 6871

Which rules spoke about things that are not defects, measured on Valve's own heroes and on three finished skins (Maid Lash, Lööps Viscous and a third skin), and what changed. The question behind it: is a finding something a player would see or an author should fix, or is it Valve's own way of building a hero read as a fault?

## How it was measured

- **Valve's heroes, absolute.** `magician mesh <pak01_dir.vpk> <model> --against none` on 51 hero models in `models/heroes_wip` and `models/heroes_staging`, plus the census of the 43 playable heroes (45 models: two heroes have a model in both folders) read directly, with every crossing, fold, contact and loose piece kept, not just the first twelve a report prints. With `--against none` nothing is subtracted as Valve's own, so this is what each rule says about a model nobody else drew.
- **Valve's files against themselves.** Valve's 160 sound-event files, its 11,070 ability particles and 58 hero models, each packed unchanged and linted deep. A replacement identical to Valve's must be silent on every fair-play rule.
- **The three skins**, deep-linted from their installed paks with no allowances, so every finding counts, before and after.
- **Community mods:** 75 of the most downloaded mods on GameBanana (121 paks, 10.2 GB: 50 skins, 9 quality-of-life, 5 HUD, 4 model replacements, 4 other, 2 gameplay modifications, 1 map), each pak linted deep with no allowances. See "The community corpus" below.

## The table

Valve's numbers are from the absolute pass: how many findings, on how many of the 51 models. A real lint subtracts Valve's own unchanged pieces, so Valve's own files linted against themselves stay silent on every rule (confirmed, all 58 models). The column says what the rule's test does on a model Valve built from scratch. The skins' numbers are each skin's findings with no allowances, before and after.

| Rule | Valve's heroes (absolute) | Skins before | Skins after | What the findings were | Changed |
|---|---|---|---|---|---|
| M061 dropped-dependency | n/a (relative by path) | 18 / 58 / 29 | 0 / 0 / 0 | All 105 were a material or texture the replacement draws its own instead of (vmat to vtex 61, vpcf to vtex 23, vmdl to vmat 15, vpcf to vmat 4, a particle's render model 2). Compiled texture names carry a hash of their settings, so even Valve's own material recompiled names other files. None was an effect or sound that stops playing | Judges only what plays: particles, sounds and sound events, anim graphs and clips, snapshots |
| M063 sound-cue-reduced | 0 on Valve's 160 event files | 0 | 0 | Any key naming a distance was read as reach, so changing `mono_downmix_min_distance`, `distance_lpf_on` or `limiter_global_event_distance_max` was a never-allowable violation, and adding one was "reach changed" | Reach is the falloff family, the two cut-offs and occlusion; the rest are not judged as reach |
| M109 self-intersection | 303 on 46 | 11 / 3 / 5 | 1 / 0 / 0 | Per site, Valve's folds and the skins' have the same spread of length | One finding when the model folds more than any Valve hero, in units of its size |
| M110 shell-intersection | 14,053 on 51 | 13 / 54 / 26 (capped at 13 a model) | 1 / 0 / 0 | Every Valve hero is built from pieces sunk 5 units or more into others: about 250 pairs a hero, and the same spread of crossing length as a skin's (Valve's median 5.0 units, 99th percentile 93; the skins' longest 258, 114, 219) | One finding when the model crosses more than any Valve hero, in units of its size |
| M111 floating-shell | 523 on 49 | 13 / 26 / 10 | 8 / 12 / 2 | Loose pieces: Valve has 141 past 3 mm, 108 of them under 6 mm, a gap no camera resolves. Lifting pieces: fingers resting on fingers, hands at their cuffs, a hat on its head, rings on bones of their own | Gap from one fair-play pixel (6 mm); a piece meeting its substrate at the joint between their bones, or on a bone that carries nothing else, is articulated |
| M112 coordinate-precision | 2 on 1 | 18 / 0 / 0 | 0 / 0 / 0 | Flat cards a few float32 quanta thick: float32 rounding a plane at 100 units from the origin, where a step is 1.2e-5 units | Speaks only where the step is an eighth of a pixel or more, at the piece's own distance |
| M135 texture-parity | n/a (relative by path) | 0 / 0 / 9 | 0 / 0 / 0 | All nine were a model's morph atlases (`_vmorf`), shipped with the model they belong to. The compiler sizes them to that model's morphs | A morph atlas shipped with its own model is not judged; shipped without it, it still is |
| M102 open-sheet-void | 103 on 25 | 6 / 6 / 0 | same | Real openings (a skirt hem, a gun's open ends, an open-bottomed dome prop) | Kept. Actionable or a taste call |
| M108 z-fighting | 99 on 28 | 5 / 0 / 0 | same | Layered lace over cloth sharing a plane (0.2 to 5.4 u²) | Kept: that flicker is what a player sees |
| M105, M117, M147, M114 | on most of Valve's heroes | 1 / 0 / 0 and the like | same | Relative to Valve's model at the same path in a real lint, so Valve's rate is subtracted | Kept |
| M129, M130, M140 | 38, 45, 7 | M140 1 / 0 / 1 | same | Relative to Valve's envelope in a real lint; absolute numbers mean nothing here | Kept, fair play keeps its strength |
| M062, M082, M124, M131 | 0 against themselves | 0 | 0 | No false positive found | Kept |

Warnings in total, no allowances: Maid Lash 107 to 44, Lööps Viscous 160 to 31, the third skin 85 to 17. With each skin's own allowances, `check` now reads 36, 17 and 6.

What fires now, and why it is worth reading:

- **Maid Lash, M109 and M110.** The skirt folds through itself along 2,202 units, 10.6 times the model's size where Valve's most is Frank's 4.8; 932 pairs cross along 12,309 units, 59.5 times where Hornet, Valve's most, has 52.5. The largest single fold is one cloth layer of 5,464 triangles along 1,210 units. This is layered frills by design or clipping in motion, and only a look in game says which.
- **Lööps Viscous, M111.** Ten cereal pieces float 1.4 to 3.1 units off the body (4 to 8 cm), and two gun parts rest on a moving barrel while weighted to the weapon bone. These are the real thing or a deliberate float, the author's call.
- **Maid Lash, M111.** The brass spools rest on the pelvis cloth while weighted to `spine_0`, 10 to 14 units from any joint between the two: they lift off when the hero bends.

## False positives found

Each has a test that fails without the fix, and a test that the true catch still fires.

1. **M063 read mono downmix, a distance filter switch and a limiter as reach.** A replaced event that changed `mono_downmix_min_distance` from 400 to 100 was a fair-play violation that no allowance can waive. Reproduce: Valve's `{ volume = 0.0 mono_downmix_min_distance = 400.0 }` against ours with 100.0, through `deep::sound_events`. Test: `m063_reads_only_the_keys_that_set_how_far_a_sound_carries`. Valve's files linted against themselves were already clean: this only shows on a changed file.
2. **M061 on every restyled material.** Reproduce: a replaced material that names its own texture where Valve's named `noise_shredded_psd_51ae548.vtex_c`. Test: `m061_silent_when_a_replacement_draws_with_its_own_art`; true catch: `m061_still_fires_when_a_particle_loses_a_child_or_a_model_its_effect`.
3. **M112 on flat cards.** Reproduce: a card horizontal at z 80, 1.6e-5 units thick, 105 units from the origin. Test: `m112_silent_on_a_flat_card_near_the_origin`.
4. **M111 on gaps under a pixel, joints and rig parts.** Tests: `m111_silent_under_a_pixel_and_at_a_hinge` and the second half of `m111_floating_shell`.
5. **M109 and M110 on Valve's way of building a hero.** Tests: `m109_silent_where_valve_folds_as_much`, `m110_silent_on_construction_like_valves`; the line itself: `m109_self_intersection` and `m110_shell_intersection` fire just past Valve's most and not just under it. `magician corpus` now measures every hero against both lines on each build.
6. **M135 on morph atlases.** Test: `m135_silent_on_a_morph_atlas_shipped_with_its_model`, which also shows that an atlas shipped without its model is still judged.

Removed: nothing. Every rule that spoke about Valve's construction could be measured against Valve instead.

## The first five minutes

`doctor`, `init` beside a pak and `check` take a minute or two together, and `check` prints about 15 lines. Before, its lint row was one line of 19 rule codes ordered by count, led by the mesh rules that fire on every Valve hero, and the headline said `NSFR` with nothing to say what it means. Now:

- the warnings are grouped by where a player meets them, in that order: on the hero, effects and sound, other heroes, frame cost, loading, the pak itself. Fair play and its advisories keep their own rows. The JSON's `by_rule` carries each rule's group as `seen`;
- `NSFR` is spelled out once as "not safe for ranked", and `FP?` as "not checked";
- Maid Lash's lint row went from 81 warnings to 36, of which "the pak" holds one.

Still open: the `next:` line points a newcomer at `magician session qa`, which needs the in-game harness. A first-time user who only wants to know whether their pak is sound has nothing in that line to tell them they can stop at the lint row.

## Allowances these changes make stale

The skins' `magician.allow` files accepted findings these rules no longer make: Maid Lash's 29 stale lines and Lööps Viscous's 33 are mostly M061, M110, M111 and M112 (some M090 lines went stale earlier, with the users-index fix), and the third skin's M140 line also no longer matches. `lint` names each one; the skins' own repositories are where they are removed.

## The community corpus

Every one of the 121 paks finished, none crashed or hung, and none asked for more than the time it was given; the slowest, a 21 MB model replacement, takes 7.8 s cold. The 121 s seen once were CPU contention with a build running beside it.

Fires per rule, as mods / findings, before and after the fixes below. A mod is counted once however many of its paks fire. Severity is as the default policy reports it (fair play is lenient, so M124 and M129 read as warnings).

| Rule | Severity | Before | After | What the sites are |
|---|---|---|---|---|
| M031 orphan-new-file | warn | 66 / 8,819 | 64 / 8,431 | Mostly real: leftover materials, whole development trees (one HUD mod ships a `node_modules`), voice files for lines no event plays. False where Panorama named the file in its own text (fixed) |
| M062 model-contract | error | 52 / 843 | 52 / 818 | Real: bones and attachments of older builds or lost on export, attachments turned by a Y-up export. False where a float32 step crossed the printed rounding of a hitbox (fixed) |
| M135 texture-parity | warn | 50 / 701 | 48 / 468 | Real: UI cards with one mip, images smaller than Valve's. False for larger images and PNG_RGBA8888 against BGRA8888 (fixed) |
| M061 dropped-dependency | warn | 43 / 2,663 | 43 / 2,663 | Real: 2,295 are child particles a replaced effect no longer spawns, then snapshots, anim clips and graphs |
| M090 shared-file | warn | 39 / 1,804 | 39 / 1,804 | Real |
| M030 dangling-ref | error | 36 / 2,229 | 36 / 2,229 | Real: mods built on earlier builds name sounds and materials the game no longer ships |
| M124 silhouette-fair-play | fair play | 34 / 261 | 34 / 261 | Real by the rule: thinner replacement heroes, and also troopers, a companion and ability props smaller than Valve's. A few read 0% (a model at the wrong scale or with no drawable geometry); they could not be shown to be a measurement fault |
| M129 hitbox-envelope | fair play | 30 / 46 | 25 / 33 | Real on heroes; false on models the game never loads (fixed) |
| M063 sound-cue-reduced | fair play | 23 / 9,689 | 22 / 7,366 | Real: voice packs and sound mods made for earlier builds lack 5,441 events Valve has since added, so those lines and sounds are silent. False where a voice line's event was judged by its length (fixed). 41 sites where a mod moved an event to a base Valve's does not inherit from are reported as unknown reach; a mod cannot be shown to be quieter or louder there, and they stay |
| M081 particle-energy | advisory | 23 / 959 | 23 / 1,626 | More now, and closer to the truth: the size a particle is drawn at is the one its initializers write (fixed), so effects drawn at 200 units where Valve's draws none, and Valve's smoke drawn at 30 to 40 rather than its constant of 2, are measured as drawn |
| mesh family (M102 to M147) | warn | 23 to 33 mods each | 13 to 30 | Fewer on every rule, because a model the game never loads is no longer judged (fixed); the rest are real or the author's call, as on the three skins |

False positives the corpus showed, each fixed with a test that fails without the fix and a test that the true catch still fires:

1. **M062, never allowable for hitboxes: a float32 step read as a moved hitbox.** A re-export writes Valve's capsule radius 5.774999618 as 5.775000095, which the signature printed as 5.78 against Valve's 5.77: a fair-play violation on Valve's own capsule, 25 sites. Signatures now compare within one step of the two decimals they are written to (0.01 units, a quarter of a millimetre). Tests: `m062_a_float32_step_across_a_rounding_line_is_not_a_change` (two hundredths wider still fires).
2. **The model rules on models the game never loads.** A skin left a copy of its hero under `panorama/`, which nothing names (M031). M129 said it was "a bit much", never allowable, on a model no one sees. Models that neither replace a game file nor are named by the mod are left to M031. Test: `a_model_the_game_never_loads_is_not_judged_by_the_model_rules` (the same model named by a particle is judged).
3. **M031 on files Panorama names in its own text.** A stylesheet's `url("s2r://...")`, a script's `file://{images}/...` and a compiled layout's bare script path are not in the compiled reference list, so 229 of a HUD mod's 452 Panorama files and an FOV slider's script read as never loaded; the other 223 are named nowhere in the pak. Test: `m031_counts_what_panorama_names_in_its_own_text` (an image nothing names still fires).
4. **M063 judged a voice line's event by its length,** where the line's file was already exempt, 2,241 sites. Test: `m063_a_voice_lines_event_is_not_judged_by_its_length` (a shorter ability sound still fires).
5. **M081 read a particle's constant radius, which an initializer replaces.** Test: `m081_reads_the_radius_an_initializer_writes` (the same smoke drawn at twice Valve's size still fires).
6. **M135 on larger images and on lossless formats.** Test: `m135_judges_what_shows_where_valve_draws_it` (a lossier format, a smaller image and a missing mip chain still fire).

The two gameplay modifications. One is a set of skin reworks filed in that category; it trips the most of any mod in the corpus (141 errors: 69 shipped skeletons, graphs and data files, 50 model-contract breaks, 20 particle classes in the wrong list; 16 reduced sound cues, one silhouette, one hitbox envelope). The other raises the field-of-view slider's cap to 150 through the settings page and a script. Magician says nothing about it, and that is a gap, not a pass: no rule reads what a Panorama script does to the game's settings, and a wider field of view is an advantage. Whether that belongs in lint is woflo's call.

After these fixes, the three skins: Maid Lash 50 to 51 warnings (M081 now measures the whip sparks at radius 22 where Valve's are 12, which it read as Valve's size before: worth a look), Lööps Viscous 70 unchanged, the third skin 20 to 11.

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

Left open in this round, and closed after 0.2.1: the `next:` line pointed a newcomer at `magician session qa`, which needs the in-game harness, and nothing in it told a first-time user who only wanted to know whether their pak was sound that they could stop at the lint row. On a mod nobody has proven, with nothing in its files that breaks it, the line now says that lint is as far as the files go, that the author can stop there, and that RE-PROVE means only that nobody has seen the mod in game yet; then it offers the in-game pass.

## Allowances these changes made stale

The skins' `magician.allow` files accepted findings these rules no longer make. The stale lines are removed in each skin's own repository: 29 from Maid Lash (M061, M110, M111, M112, and M090 lines the users-index fix had already made stale) and 33 from Lööps Viscous. Their allowed counts are unchanged at 4 and 14. The third skin's one stale line was Magician's mistake, not the file's. Its model's M140 allowance read stale when lint ran over the skin's music pak, which ships no model. An allowance for a file a pak does not ship is now neither used nor stale in that pak's lint, and `check`, which judges every pak of a mod together, reports an allowance no pak uses (test: `an_allowance_for_a_file_another_pak_ships_is_not_stale_in_this_one`). The line stays, still used, and all three skins read 0 stale.

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
| M124 silhouette-fair-play | fair play | 34 / 261 | 32 / 140 | Thinner replacement heroes are real. It also fired on ability props, a companion and a trooper's break-off pieces, which are effects, not targets. It now judges only models Valve gives hitboxes, what bullets can hit (see below) |
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

**M124's scope.** Fair play asks that a target stay as easy to see, and a target is what bullets can hit: a model whose Valve original carries hitboxes. That is read from the game's own models, so a new unit is covered without an edit. On the corpus it keeps heroes, McGinnis's turret, Necro's summoned skeleton and the gravestone and skull Valve gives hitboxes, and leaves out ability props (a teapot, a magazine, a chain, a jar), a trooper's break-off pieces (the `break_list` the game spawns when one dies) and a companion no one can shoot: 34 mods and 261 findings before, 32 and 140 after. Tests: `m124_judges_what_bullets_can_hit` (a thinner trooper fires, a thinner prop with no hitboxes does not).

The two gameplay modifications. One is a set of skin reworks filed in that category; it trips the most of any mod in the corpus (141 errors: 69 shipped skeletons, graphs and data files, 50 model-contract breaks, 20 particle classes in the wrong list; 16 reduced sound cues, one silhouette, one hitbox envelope). The other raises the field-of-view slider's cap to 150 through the settings page and a script. Magician says nothing about it: no rule reads what a Panorama script does to the game's settings. That is an open question, not a decision either way. Settings changes such as a field-of-view cap are not judged yet, and whether they should be is undecided (woflo, 2026-11-26: it may be revisited later); STATUS lists it under loose ends.

After these fixes, the three skins: Maid Lash 50 to 51 warnings (M081 now measures the whip sparks at radius 22 where Valve's are 12, which it read as Valve's size before: worth a look), Lööps Viscous 70 unchanged, the third skin 20 to 11.

## M062 and moved attachments (6875)

On the community corpus every skin read broken, and M062 was the most common reason: attachments a skin had moved to fit another body shape, and bones it had lost. woflo's call (2026-11-27): a lost bone, a lost attachment and any hitbox change stay errors, since they break animation or touch fair play; a moved attachment is a warning that says how far it moved, unless the move is extreme. The question for the record was where extreme begins, measured on Valve's own heroes rather than chosen.

### How it was measured

`lint::reach` places every attachment of a model at bind: each influence's offset carried into model space by its bone's bind transform. Off the body is the distance to the nearer of the skinned LOD0 surface and the nearest bone. The model's size is the square root of that surface's area, the measure M109 and M110 already judge by, so a large hero and a small one are held to the same proportion. The game build moved from 6873 to 6875 while this was measured; the numbers below are the same on both.

Valve's root bone is turned 120 degrees about (1, 1, 1) at bind, and the attachments on it only make sense unturned: Haze's aim sits 63.9 units up, the standing camera pivot 86.2 and the zipline hook 107. Turned, all three would lie on the ground in front of him. The root's attachments are placed in the hero's own frame. This is read from Valve's numbers, not seen in game: UNVERIFIED.

### Gameplay positions and effect points

The brief allowed a separate line for attachments the game uses to position gameplay, but only if the game's own files say which those are. The model files carry no such flag: an attachment has its influences, offsets, rotations, `m_bInfluenceRootTransform` and `m_bIgnoreRotation`, nothing more. The game's scripts name attachments by what they do with them (`m_strEffectsAttachName`, `m_strAttachmentName`, `m_strShootAttachment`, `m_sHealthBarAttachment`, `m_strHoldAttachment`), but none of those marks one attachment as positioning and another as cosmetic, and a list of names would be a guess.

What the model file does say is how each attachment is held. Of the 2,173 attachments on Valve's 43 playable heroes, 1,384 ride a body bone and 789 sit on the root bone or the root transform, in the hero's own frame. The two groups are nothing alike:

| | Attachments | Furthest off the body, in units of the hero's size | Median |
|---|---|---|---|
| On a body bone | 1,384 | 0.0931 (McGinnis's turret_carry_mid_fx, 18.2 units off a size of 195.2) | 0.0003 |
| In the hero's own frame | 789 | 3.14 (Viper's portrait camera, 373 units away) | |

The ones in the hero's frame are the camera poses (`near_`, `far_`, `gunaim_`, the pivots, the portrait camera), the aim point, the zipline hook and the test shooting points. They sit far from the body by design and are where the game positions things, so no distance from the body can judge them. Body attachments are effect points: palms, muzzles, the idol and carry points on the back. One line serves every body attachment, and a changed attachment in the hero's own frame stays an error.

The line is 0.094 of the model's size: McGinnis's turret carry point rounded up, as M109 and M110 round their heroes, so no Valve attachment is past it. On a hero of Lash's size (148) it is 13.9 units, about 35 cm. `magician corpus` measures every body attachment against it on each build; none is past it on 6873 or 6875.

### Fair play

The only part of M062 that is fair play is the hitbox comparison (`rubric::is_fair_play`: M062 when the finding is about a hitbox). It is untouched: the same signatures, the same rounding step, the same severity, never allowable. Tests: `m062_a_hitbox_change_beside_a_cosmetic_move_is_still_an_error`, with the existing `m062_model_contract` and `m062_a_float32_step_across_a_rounding_line_is_not_a_change`.

### Before and after

The 75 community mods (121 paks) linted deep with no allowances on 6875, before and after.

| | Before | After |
|---|---|---|
| Changed attachments | 214 errors | 108 warnings, 106 errors |
| Lost attachments | 81 errors | 81 errors |
| Lost or reparented bones | 403 errors | 403 errors |
| Hitboxes changed, added or lost | 120 (fair play, lenient) | 120 |
| Skins broken | 48 of 48 | 46 of 48 |
| Mods broken | 61 of 75 | 59 of 75 |

The 108 warnings are on 46 mods. Half of them moved 3.1 units or less; the furthest any sits off its body is 10.6 units, under that model's line. The largest move is 116 units: a muzzle point moved along a body that holds its weapon elsewhere, still on the body.

None of the 106 errors is past the line. All of them are in the hero's own frame: 86 are Valve's attachments turned by an exporter's Y-up/Z-up swap (the finding says so), 15 moved (zipline hooks 100 to 218 units, one skin's camera poses 3 to 46 units, an overhead bar moved off the head onto the root, a test shooting point 15 units), and 5 turned in place.

M062 was the only error on 6 of the 48 skins. 2 of those are now clean of errors; the other 4 lost or reparented bones, which stay errors. The other 42 skins are broken by M030 (references to files the build no longer ships), M070, M137 and the rest, as before.

Valve's own heroes against themselves are silent, as before: an unchanged attachment is no finding. Maid Lash, Lööps Viscous and two other finished skins keep Valve's attachments: no M062 finding before or after, and their `check` verdicts are unchanged.

Tests: `m062_a_moved_attachment_on_the_body_is_a_warning_that_says_how_far` (4 units off is a warning with the distance, 20 is an error, the line holds within a few tenths), `m062_a_moved_attachment_in_the_hero_s_frame_and_a_missing_one_stay_errors`, and in `report.rs` a cosmetic move read back from a findings file as a warning. Lint judgement revision 4.

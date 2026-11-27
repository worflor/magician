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

Valve's root bone is turned 120 degrees about (1, 1, 1) at bind, and the attachments on it only make sense unturned: Haze's aim sits 63.9 units up, the standing camera pivot 86.2 and the zipline hook 107. Turned, all three would lie on the ground in front of him. The root's attachments are placed in the hero's own frame. This was read from Valve's numbers, and seen in game on 6880: a raised standing pivot raised the camera straight up ("The 6877 round in game").

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

## M062 and lost or reparented bones (6875)

After moved attachments were calibrated, 46 of the 48 community skins still read broken, and 403 lost or reparented bones were the largest share of M062's errors. woflo's call (2026-11-27): a bone nothing hangs on can't break anything a player sees, a bone that carries something can, and the line between them comes from the game's files, not from a list of names.

### What a bone carries

`lint::carry` reads, for every bone of a model, what the model hangs on it: vertices weighted to it (every mesh, LOD and body-group choice), hitboxes, attachments, cloth control nodes (the FeModel's) and ragdoll parts (`PHYS`'s bones). The compiler marks the same uses on each bone (`m_nFlag`), so Valve's own files check the reading. On Valve's 107 hero models of 6875 (17,500 bones) the two agree on every bone for hitboxes (1,270), attachments (1,330), cloth (6,735) and ragdoll parts (1,420). Every one of the 13,071 bones that carry weighted vertices has the compiler's mesh mark; 1,539 more have that mark with no vertex weighted to them, because the mark means that a mesh lists the bone. One attachment needed a fix on the way: Valve writes a weight of 0 on some single-influence attachments, which still ride their bone. 2,810 of Valve's bones carry nothing, on them or below them: end bones, IK targets and offsets, aim helpers.

### How the hero moves a bone

The hero's animation is its animation skeleton (`m_vecNmSkeletonRefs`, a `.vnmskel`) and the clips its graphs play. `lint::carry` follows the model's graphs through their references to every clip of that skeleton (a hero's graphs reach 138 to 318; when they reach none, how the bone moves is said to be unknown, never still), decodes each clip's pose tracks and keeps, for each animation bone, the furthest any frame turns, shifts or scales it from its reference pose. The clip format is the one Source 2 Viewer reads (15-bit quaternions over plus or minus 1/sqrt 2, 16-bit translations over each track's range).

A model bone takes the model-space pose of the animation bone of its name, case aside, and a model bone the animation skeleton lacks follows its model parent at its bind offset. Valve's files hold that: 25 of Valve's hero models have a hierarchy that is not their animation skeleton's (a `weaponHand_L` under `weapon` where the animation holds it under `root_motion`, a hose under `hand_R` where the animation holds it under `weapon_hand_R`, Kelvin's and Viscous's `pelvis` named `Pelvis` in the animation), and those heroes play. Source 2 Viewer retargets clips the same way. A model's bind is not its animation reference pose either: Viscous's fingers sit 11 units from theirs, so a bind is only ever compared with Valve's model's bind. Nobody has watched a reparented bone on a mod in game: UNVERIFIED.

### The lines

- A lost bone stays an error when Valve's model hangs something on it, or on a bone lost with it below it, and the finding says what. One that Valve's model hangs nothing on, with nothing lost below it, is a warning that says so and says how the clips move it.
- A reparented bone is judged by what this model hangs on it and on the bones that follow it. One the animation skeleton has moves as Valve's whatever its parent, so only vertices bound to it at another place than Valve's bone can land elsewhere: an error when they land a fair-play pixel (0.25 units) or more away, counting the bind's turn at the furthest vertex. One the animation skeleton lacks follows its parent: an error when what hangs on it sits a pixel or more from Valve's at bind, or when its parent and Valve's move apart by a pixel at it in the clips. A pixel is the fair-play view's unit, the line M111 uses.
- A mod that ships the animation skeleton itself moves its bones its own way, which Valve's files cannot judge: its reparented bones stay errors that say so.
- A bone is still in the clips when no frame turns it more than 0.05 degrees or shifts or scales it more than 0.01: the clip format's own rounding. Of the 7,743 bones on Valve's heroes that take their pose from an animation bone, 2,154 turn not at all.

Hitboxes keep their own comparison and full strength: a lost bone with a hitbox is an error, and the hitbox finding beside it is fair play as before.

### Before and after

The 75 community mods (121 paks) linted deep with no allowances on 6875.

| | Before | After |
|---|---|---|
| Lost bones | 339 errors | 234 errors, 105 warnings |
| Reparented bones | 64 errors | 46 errors, 18 warnings |
| Skins broken | 46 of 48 | 44 of 48 |
| Mods broken | 59 of 75 | 57 of 75 |

The 105 lost-bone warnings are end bones (`*_end`, `ball_end_L`), aim helpers, offsets, IK targets and the like, which Valve's model weights no vertex to and hangs nothing on; the clips move 11 of them, and nothing moves with them. The 18 reparented warnings are IK targets and an owl's toes that sit where Valve's do and are posed by name, two bones a coat hangs from, a weapon pivot and two scapula helpers.

What stays an error, and why:

- 234 lost bones that Valve's model hangs something on: vertices on 157 (the hair, coat, scarf and frog chains of a skin that replaced them), cloth on 48 more, an attachment on 11, a ragdoll part on 6, and on 12 a bone lost below them that carries something. 116 of the 234 are bones the clips never move or the animation skeleton lacks; they are still errors under this round's line, because Valve's model hangs something on them.
- 4 reparented bones that a skin moved 27 to 184 units with its vertices on them (the props of one hero's bench).
- 42 reparented bones of two mods that ship their own animation skeleton.

Two skins were broken by one bone alone: each lost `attachWorld`, on which Valve's model hangs the portrait camera's attachment, and each still names that bone in the attachment. They stay broken.

Valve's own heroes against themselves are silent, as before: an unchanged skeleton has no lost or reparented bone. Tests: `m062_a_lost_bone_says_what_it_carried`, `m062_a_reparented_bone_breaks_only_where_it_moves_differently` (both sides of the pixel line, at bind and in motion), `m062_a_helper_bone_lost_in_a_whole_lint_is_a_warning`, and in `carry.rs` the clip reader. Lint judgement revision 5.
## The hero model contract and its neighbours (6877)

The toolchain survey ([toolchain-landscape.md](toolchain-landscape.md), section 6) ranked the problems modders hit because no tool knows Deadlock. This round took items 1, 2, 3, 5, 6, 8, 9 and 10. The game moved from 6875 to 6877 while it was measured; every number below is 6877's.

How it was measured, for every rule here:

- **Valve against itself.** All 231 models under `models/heroes_staging` and `models/heroes_wip`, every layout under `panorama/layout/`, Haze's voice and weapon sounds and his ability particles: 2,763 of Valve's files packed unchanged into one pak and linted. Nothing in this round fires on them except M084, which is right to (below).
- **The community corpus:** the 75 mods (121 paks), light lint with no allowances, read as data only.
- **woflo's skins:** Maid Lash (`pak10`) and Lööps Viscous (`pak20`) from the install, read in place.
- Before, for every rule here, is nothing: the findings are new. Where a rule can make a mod broken, the table says how many mods it newly breaks.

### M062 reads the model's game data

A compiled model carries more than its skeleton. Its DATA block names its animation skeleton (`m_vecNmSkeletonRefs`) and its AnimGraph 2 graphs (`m_animGraph2Refs`), and `m_modelInfo.m_keyValueText` holds ModelDoc's GameDataList as KV3 text: the unit status (where the health bar sits), the tagged sounds, the camera settings, the camera poses and more. `heroes.vdata` names no tagged sounds and no camera settings, so the model is the only place the game finds them; it does set its own collision radius and step height, so those fields were left alone.

| What differs from Valve's model at the same path | Severity | Sites | Mods |
|---|---|---|---|
| no animation skeleton reference | error | 4 | 4 |
| an AnimGraph 2 graph Valve's names, missing | error | 16 | 5 |
| unit status missing | error | 3 | 2 |
| unit status written as a list (a CSDK 12 compile) | error | 1 | 1 |
| tagged sounds missing (silent footsteps) | fair play | 19 | 16 |
| camera settings changed or missing | fair play | 3 | 3 |
| health bar placed otherwise (no attachment, other offsets) | warning | 28 | 23 |
| another block missing | warning | 66 | 36 |

The A-pose errors are on troopers whose match graph is gone (`npc_troopers.vnmgraph` and its medic and melee variations) and on summons and a hero with no skeleton reference at all. The graph identifiers are matched without case: Ivy's own model writes `UI` where Haze's writes `ui`, and a mod's `ui` for Valve's `UI` is no finding. The one list-shaped unit status is on an ability's model, the health-bar-at-the-feet case the community pages describe.

Tagged sounds were planned as an error and are fair play instead. `TaggedSoundSettings_t` names `scripts/tagged_sounds/<hero>_anim_sounds.vdata`, which maps the animation's tags to the footstep, landing, mantle and melee-swing events. Without it those make no sound, and footsteps are a cue enemies listen for, so it is judged with the hidden cues: never allowable. Seen on 6880: a skin without them makes no footstep sound ("The 6877 round in game").

The health bar warnings are an older build's game data: no attachment name and a fixed offset (`[0, 0, 95]`), where 6877 places the bar on the `healthbar` attachment with an offset of `[10, 0, 0]`. The bar then sits over the model's origin and does not follow the hero down when he crouches (on 6880 the bar was seen to sit lower over a standing hero; the crouch was not filmed). One of these was a false alarm in the first draft: Lööps Viscous's inflated model names `healthbar` and has no attachment of that name, and neither has Valve's model. That is Valve's own design, and a lost attachment of Valve's name is the contract's own finding anyway, so the rule now speaks only when a mod renames the attachment to one its model lacks. Test: `m062_reads_the_hero_s_game_data_against_valve_s`.

The other missing blocks are the hero page's scene settings and the camera poses' list (13 each), the cloth's ground plane (10), a breakable's piece lists (11), the effects a model plays on itself (8), the effect scale (6), and a few others. Each says what the game falls back to its defaults for.

A model's own skeleton and graph references are no longer M061's: the contract judges them whenever the game is there.

No mod is newly broken by these errors: every one is on a mod already broken by another rule. woflo's two skins have none of them.

### M062 holds the camera attachments the game names

The camera in the middle of the screen comes from a camera attachment off by a decompile's rounding: an older Source 2 Viewer wrote a pivot of -90 degrees as -89.972. The printed contract compares to two decimals, 0.01 a number, and a turn of 0.028 degrees changes a rotation's components by 1.7e-4, so it passed.

Which attachments are cameras comes from the game, not a list. Each hero model's game data lists its camera poses in `AttachmentCameraData` (1 to 12 a model, 521 on 66 models: the portrait cameras, `near_`, `far_` and `gunaim_`), and the client library names the pivots it reads (`standing_pivot`, `crouching_pivot`) and a few more hero-frame attachments as plain strings. Those in the hero's own frame that the client names are held as tightly.

The line, from Valve's own files and the mods that copy them:

| | Largest difference from Valve's numbers |
|---|---|
| Valve's own copies (every mesh repeats its attachments) | 0, bit-identical on all 66 models |
| community camera attachments kept through a Source 2 Viewer round trip | 6.1e-5 (62 sites) |
| community camera attachments that moved | 1 unit or more (48 sites) |
| a pivot of -89.972 for -90 | 1.7e-4 |

Nothing in the corpus falls between 1e-4 and 1, so the line is 1e-4 (`mesh::rules::CAMERA_ATTACH_STEP`): it passes every round trip and catches the rounding. The corpus has no site in that gap, so the line changes no verdict on it: every moved camera was already an error. Test: `m062_holds_the_camera_attachments_the_game_names_to_valve_s_numbers` (-89.972 fires, 90.007 does not, a pivot only fires when the client names it, and a larger turn is said once).

### M042 layout-id-missing

A HUD mod ships a whole copy of each layout it changes. A compiled layout keeps its tree in a `LaCo` block; the ids are its PANEL nodes' `id` attributes. On the corpus, 6 mods replace a layout that lacks ids Valve's layout has on 6877: 80 missing ids across 11 layouts, from one missing (a popup's binder) to 27 (a whole-HUD mod's `hud.xml`).

Not every missing id is the launch crash. The client library holds as strings the ids its code looks up, and 32 of the 80 are among them (`GameTime`, `UnitHealthbar`, `CitadelHudCombatLog`); the other 48 are not (`ScoreCounter`, `MatchStart`, decoration). A code-named id is an error, the rest a warning: 20 errors on 2 mods (one HUD mod in four variants, one health-bar mod in two), 24 warnings on 6. Those 2 mods are newly broken; in light lint 55 of the 75 mods are now broken, where 53 were. Valve's own layouts are silent. Whether a given missing id crashes the HUD or leaves it blank was not tried: the corpus is not run, and building a layout with an id removed was left for a later in-game pass (on 6880 a code-named id was seen to stop the game at launch, below). Test: `m042_a_replaced_layout_lacks_an_id_valve_s_current_one_has`.

### M065 sound-format-parity

The CTRL block of a `.vsnd_c` gives its rate, channels, sample count and length. On the 11,855 sounds the corpus replaces, the length always matches the samples and rate (2 exceptions), so nothing in the header shows a sound that plays sped up. What differs is the encoding: 5,700 replacements are stereo where Valve's are mono (13 mods), 1,655 are 48,000 Hz where Valve's are 44,100 (4 mods), and a few run at 22,050, 32,000 or odd rates such as 13,111. M065 says so, with both lengths and the pitch and volume of the event that plays the file: 6,005 warnings on 19 mods, none on woflo's skins. Whether stereo or another rate is the quiet or sped-up sound modders report is not settled, so it is a warning to listen, and `witness` is where it should be calibrated (heard on 6880: the format plays right, and M065 is now a note). Test: `m065_a_replaced_sound_encoded_unlike_valve_s`.

### M160 physics-dropped

The PHYS block holds the cloth (the FeModel) and the ragdoll or a prop's body (one part per bone). On the corpus, 13 replaced models have no cloth where Valve's has 9 to 264 nodes, and 29 models lack physics parts Valve's has: 35 findings, often one part (`root_motion`, a weapon or purse offset), sometimes whole limbs (9 of an archer's 19 parts). 48 warnings on 34 mods; none on woflo's skins. A skin's own cloth of another size is not judged here (M141 judges how it moves). What a lost part does on death was seen on 6880: it keeps its last pose. Test: `m160_valve_s_cloth_or_ragdoll_dropped`.

### M084 particle-unreached

`magician index` now writes, beside the users index, every particle nothing names: no compiled file references it (on 6877, 13,612 references from other particles, 331 from clips, 185 from models), no vdata (8,289), layout (21) or map entity lump (654) names it, and the client code does not. That is 3,758 of the 18,043 particles; many look like effects of older versions of abilities (Haze's `haze_stalk`, `haze_leap_cast`). A first count left out the abilities of heroes the users index skips, which named 191 more; every vdata is now read whoever owns it, and the list matches a full scan of every decoded string in the game.

Valve's own unreached particles, packed unchanged, fire M084 (21 of Haze's): that is the point, since replacing one does nothing, and it says nothing against Valve's file. On the corpus, 1,961 warnings on 26 mods. Maid Lash has two: `lash_ground_strike_v2_cone_line_pulse` and `lash_ultimate_locked_aura`, which the skin replaces and which nothing in the game plays. A particle whose name the code builds at run time cannot be seen this way; none was found on 6880. Test: `m084_a_replaced_particle_nothing_plays`.

### M092 other-hero-file

A mod about one hero is a mod that replaces that hero's model; the users index says which hero each replaced file belongs to. A file only another hero uses is said: 117 warnings on 5 mods. They are real: another hero's voice lines in a voice pack (86), another hero's particles and textures in an effect rework, a shared noise texture only one other hero still uses. None on woflo's skins. Test: `m092_a_hero_mod_replaces_another_hero_s_file`.

### Installed paks and `gameinfo.gi`

`install --status` now lists the paths two installed paks share and names the one the game reads them from, the lowest number (`install::collisions`; test: `installed_paks_that_share_paths_and_which_wins`). On this machine none of the installed paks share a path.

`doctor` reads `gameinfo.gi` against the stock file `install` backs up before its first edit of a build (`gameinfo.rs`): the SearchPaths lines the stock file has and this one lacks, the lines another tool added beside Magician's mount, and the sections a config must not touch (Engine2, MaterialSystem2, NetworkSystem, Particles, RenderSystem, SceneSystem, WorldRenderer; the matchmaking refusal they cause is one community page's report, UNVERIFIED). On this machine the newest backup is 6864's and the file matches it plus the mount. Tests: `the_stock_file_against_itself_and_with_magician_s_mount`, `a_block_pasted_from_an_older_build_and_a_performance_config`, and the `gameinfo` fuzz target.

### What stays UNVERIFIED

Nothing in this round was seen in game when it was written ("The 6877 round in game", below, puts it to the game): silent footsteps without tagged sounds, the health bar's place with an older unit status, which missing layout ids crash the HUD, whether a stereo or 48 kHz replacement plays wrong, what a lost ragdoll part does on death, and the matchmaking refusal for a restricted section. Lint judgement revision 6.

## Judgement round on 6880

The game moved to 6880 before this round started, and every number below is 6880's. How it was measured:

- **The community corpus:** 144 mods, 143 of them with paks (one holds only a source note), 285 paks, linted deep with no allowances, read as data only. 83 of the 143 replace a hero model and are counted as skins.
- **Valve against itself:** Haze, Lash, Viscous and Geist's model folders, Haze's ability particles, `scripts/heroes.vdata` and `scripts/abilities.vdata`, Haze's hero graph and the base hero graph, 555 of Valve's files packed unchanged into one pak and linted deep. Before this round it read 6 errors, all M070 on Valve's own skeletons, graphs and vdata; it now reads none.
- **woflo's skins:** Maid Lash (`pak10`) and Lööps Viscous (`pak20`) from the install: 0 errors before and after (47 and 31 warnings, unchanged), and no finding of the rules this round added.
- Valve's files were measured with a throwaway reader over the build's own paks (not committed); what each rule reads at lint time is in the code.

| Corpus, deep, no allowances | Before (6880, judgement 6) | After (judgement 7) |
|---|---|---|
| Errors | 4,881 | 4,244 |
| Warnings | 77,886 | 78,784 |
| Mods broken | 105 of 143 | 87 of 143 |
| Skins broken | 78 of 83 | 66 of 83 |

What breaks the 66 skins still broken: M062 on 47 (alone on 22), M030 on 42 (alone on 17), M070 on 5, M083 on 5, M153 on 2, one each M071, M010 and M143. Of M062's errors, 623 are lost attachments, which this round did not judge again.

### Lost bones, judged by who outside the model names them

A full skin brings its own mesh, cloth and ragdoll, so what Valve's model hung on a bone is not in the mod, and the last round's line (an error when Valve's model hangs something on the bone) kept every rebuilt hair chain and coat an error. What can still break is something that looks the bone up by name. `magician index` now writes `bone_users.tsv`: every string an anim graph, clip, particle or vdata holds under a key that says bone (a bone mask's id left out), and every bone name of Valve's hero models and their animation skeletons that the client library holds.

What the index found:

- Only the anim graphs name bones: `m_boneID` (256 uses), `m_effectorBoneID` (249), `m_followTargetBone` (184), `m_bone` (180), `m_endEffectorBoneID` (68), 53 names in all (IK targets, follow targets, root motion, the head for look-at). Particles, vdata and clips name attachments (`m_attachmentName`, `m_strAttachmentName`, `m_attachmentPoint0`) and hitbox sets, never a bone.
- A graph poses the animation skeleton, not the model. 25 of Valve's 65 hero models with an animation skeleton lack bones their own graphs name (`weaponHand_L` and `weaponHand_R` on most, the leg IK targets on one, `Pelvis` on the owl, a turret and a hand), and those heroes play; 383 animation-skeleton bones are absent from Valve's models. A graph naming a lost bone is said and never an error.
- The client library holds 28 of the bone names (`head`, `root`, `body`, `weapon`, `muzzle_fx`, `palm_l` and others, several of them attachment names too). Valve's hero models carry every one of them they are named on, but one effect model's `muzzle_fx`, which is an attachment there. What the code looks up by each name is not known: UNVERIFIED.

The line: a lost bone is an error when the model itself still names it (an attachment riding it, a hitbox, a physics part, a cloth node, a mesh's bone list: a reference to a bone the skeleton no longer has), or when the client code holds its name and the model has no attachment of that name for the lookup to find; the message names which. Otherwise it is a warning that says what Valve's model hung on it. No index, or one written without the client library, leaves it an error that says so. Hitboxes keep their own comparison, fair play as before.

| The 319 lost-bone errors of the baseline, by what Valve's model hung on the bone | Now an error | Now a warning |
|---|---|---|
| vertices | 4 | 183 |
| cloth | 0 | 50 |
| an attachment | 17 | 46 |
| a ragdoll part | 0 | 6 |
| only a bone lost below it | 0 | 13 |
| All | 21 | 298 |

The 21 errors: 18 where the mod's own model still names the bone, 17 of them the portrait camera's attachments still riding a lost `attachWorld`, and one a bag's attachment and hitbox on a lost `bag_main`; 3 where the client code holds the name (`root` and `base` on an ability's prop with no animation skeleton, `body` on a hero model). A sample of the warnings: `hair_0` on a rebuilt hero, where Valve's model hung 679 vertices and cloth, now the mod's own; `owl_ball_L`, where Valve's hung nothing; `attachWorld` on a skin that dropped the portrait camera's attachment with it, which the lost attachment's own finding keeps an error. Tests: `m062_a_lost_bone_is_judged_by_who_still_names_it`, `m062_a_helper_bone_lost_in_a_whole_lint_is_a_warning`, `bone_users_read_back_only_in_their_own_format`, and in `gamefiles.rs` `the_bone_users_are_the_names_graphs_and_the_code_hold`.

Nobody has seen a lost bone the code names, or an attachment riding a lost bone, in game: UNVERIFIED.

### Five error rules

| Rule | Before | After | What decided it |
|---|---|---|---|
| M030, an AnimGraph 1 graph (`.vanmgrph`) | 153 errors, 29 mods | 153 warnings | No build ships an AnimGraph 1 graph and none of Valve's files names one. The hero animates from the model's AnimGraph 2 references, which M062 judges. Every other M030 (3,046, sounds most of them) stays an error. |
| M137 albedo-absolute | 57 errors, 38 mods (6 skins broken by it alone) | 57 warnings | A look: the game draws the colours it is given. 12 of Valve's 78 hero models with hitboxes are past the line on their own art (Mirage 18.2% black, Haze 12.4% black, Geist 64% white). |
| M070 ships-protected-file | 110 errors, 33 mods | 6 errors, 104 warnings | Judged against Valve's file at the same path. Errors: 5 animation skeletons that lack Valve's bones (`attachWorld`, `weapon_hand_R`) and one `generic_data.vdata` that lacks 14 of Valve's entries. Warnings: 59 at paths Valve does not ship (the CSDK's `image_compiler.vdata` most of them), and 45 copies that hold everything Valve's file has, which play until a patch changes Valve's file. A key that starts with `_` (`_include`) is the file's own bookkeeping. |
| M001 path-case | 14 errors, 6 mods | 2 errors, 12 warnings | Nothing names the 12 (`- Copy` files, an exported folder, a renamed old event file, a loose image) and they replace no game file, so the game never looks them up. The 2 errors were fonts under `panorama/fonts/`; the next round found that Panorama reads fonts from disk, not from a pak, and made them warnings. |
| M139 alpha-coverage-gameplay | 15 errors, 7 mods | 15 warnings | A look, as M138 at 8 m is. No material on Valve's 78 hero models is past it at 4 m, so it still says something Valve's art does not do. |

Tests: `m001_uppercase_path`, `m030_an_animgraph_1_reference_is_a_warning`, `m070_ships_protected_file` (both sides of every kind), and the severity checks in `m136_m137_albedo` and `m138_m139_alpha_coverage`.

### Which compiler built each file (survey item 4)

A compiled file's `RED2` names each compiler with a version it raises when its output changes (`resource::compilers`). On 6880 every one of Valve's files of a kind carries one version: models 3, clips 21, graphs 27, animation skeletons 14, particles 2, layouts 16, styles 11, sound events 11, vdata 2, materials 25. Maid Lash and Lööps Viscous, built with `bin/win64_nm`, carry the same.

M170 (a note) says, per kind and version, how many of a mod's files another version built and names the known kit cause: 107 notes on 57 mods.

| Kind | Versions in the corpus (the game's) | Notes |
|---|---|---|
| clips | 10, 14 (21) | 6 |
| graphs | 20 (27) | 2 |
| animation skeletons | 10 (14) | 7 |
| layouts | 13, 14 (16) | 23 |
| styles | 9 (11) | 33 |
| sound events | 9, 10 (11) | 17 |
| vdata | 1 (2) | 19 |

Clips, graphs and skeletons of an older version name CSDK 12, whose AnimGraph 2 output the community pages call outdated; M062's A-pose findings now name CSDK 12's missing AnimGraph 2 references, as its unit-status finding named the list. Which number each kit writes was not read from the kits, and whether the game reads an older version as it was made is UNVERIFIED. `build` names the kit it compiles with, as before. Test: `m170_names_the_compiler_that_built_a_file`, and in `resource` `compilers_are_read_from_the_edit_info`.

### Materials by cause (the rest of survey item 7)

**M153 draw-error-material (error).** A LOD0 draw call with no material, the engine's error material, or a bare name with no folder that is no material in the mod or the game. On Valve's 12,429 draw calls of the citadel pak's models: none empty; 22 on `materials/error.vmat` or `materials/dev/default.vmat`, every one on an effect model a particle draws with its own material or a development prop, none on a hero model. A draw Valve's model at the same path makes is not said. On the corpus, 2 sites, `nada.vmat` and a bare `calico_clawball_bomb.vmat`, on mods already broken by other rules: it breaks none newly. Test: `m153_a_draw_with_no_material`.

**M152 material-flag-fair-play (fair play, never allowable).** Divine Skins' material reference marks 8 features Avoid for hero skins ("changes how easy a hero is to see"): F_TRANSLUCENT, F_ADDITIVE_BLEND, F_ADVANCED_TRANSLUCENCY, F_SELF_ILLUM, F_UNLIT, F_OVERRIDE_BLOOM_AMOUNT, F_OVERRIDE_NPR_OUTLINE, F_DISABLE_NPR_OUTLINE. Valve uses every one of them: of its 772 `pbr.vfx` hero materials (4,561 materials in the citadel pak), 346 glow, 101 are translucent, 59 additive, 50 drop the outline, 33 set their bloom, 22 use advanced translucency, 16 are unlit and 3 set their outline. So the rule says a feature only when a new material on a target switches it on and no material of Valve's model at that path does. Valve against itself is silent by construction. On the corpus: 259 findings on 41 mods (self-illumination 107 on 12, outline off 87 on 18, outline thickness 27 on 10, translucency 19 on 12, bloom 8 on 5, unlit 4 on 3, additive 4 on 2, advanced translucency 3 on 3). Every one of the 41 was already freeform by another fair-play rule, so no label changed, and it breaks nothing. Test: `m152_a_material_flag_on_the_avoid_list`.

### M031 and the scripts a compiled layout names

The dogfood lane found M031 on 15 of 23 files of one HUD mod: its own scripts, which its replaced `hud.vxml` loads. The game's compile keeps a layout's tree LZ4-compressed in its `LaCo` block, so the earlier fix's scan of the raw bytes saw `cripts/aspect_ratio.vj` and not the path. The tree is now decoded and its strings read as Panorama names (`panorama/scripts/aspect_ratio.vjs_c` under SCRIPTS). On the 12 HUD mods that had Panorama findings: scripts 294 to 9, styles 84 to 37, images 149 to 141; vector images and materials are unchanged (21, 4). The 9 scripts left are tool scripts and older numbered copies of a bootstrap that nothing names; the 37 styles are named by no file of the mod and are not Valve's paths; the next round searched Valve's own layouts and stylesheets for them and found none that does. Test: `m031_counts_the_scripts_a_compiled_layout_tree_names` (the fixture's KV3 is written uncompressed, so it holds the read, not the codec).

### Drift and the mod's own contract

The dogfood lane found `drift` calling a mod's long-standing hitbox changes BROKEN on patch day. Drift re-ran the whole contract against the new build's model, so every choice the mod had made since it was built read as a break the patch made. Measured: between 6877 and 6880 Valve changed one file in the whole build, and no hero model has changed since 6871, yet `drift --from 6877 --to 6880` over the 130 corpus paks that replace a model said 3,455 contract breaks and BROKEN on 102 of them (62 mods). Now a contract error is a drift break only when Valve's model at that path changed between the two builds (another CRC or size, or missing on one side); when it is the same bytes, the mod's errors are its own, which lint judges, and drift counts them (`contract_own`, 1,410 on the corpus). A hitbox change is fair play and never a drift break. After: 0 breaks, 0 paks BROKEN. Test: `drift_breaks_only_on_what_the_patch_moved`.

### What stays UNVERIFIED

Nothing here was seen in game (another lane holds the seat): what the client code looks up by a bone's name, where an attachment riding a lost bone ends up, whether an older compiler version plays as made, the error material on a bare-named draw, and how a copy of an older skeleton, graph or vdata fails. Lint judgement revision 7.

## The 6877 round in game (6880)

What the 6877 round left UNVERIFIED, put to the game. The game moved to 6880 before the first session, so everything below is 6880's; the lint findings it starts from read the same on 6880. Each case was one community mod that shows exactly the finding, installed for one session in a free slot below the skins (`magician install --as pak0N --force`), filmed with the harness and taken out again; where no community mod shows a case, a test pak built from Maid Lash with the one change, put over Lash's own slot for one session (`session up --pak --as pak10_dir.vpk --restore --force`). A vanilla session ran the same scenarios first. Game audio was recorded from the default output device (WASAPI loopback) and lined up with `snd_showstart`'s log lines. The stills and logs are in `<reference>/work/verify-6880/`; the films were deleted after the stills were cut.

| Claim | Result |
|---|---|
| A model without its tagged sounds has silent footsteps (M062) | Proven. Four seconds of walking and a jump: vanilla Doorman logged 13 footstep starts and a landing, a skin of him without `TaggedSoundSettings_t` none; the same for Apollo (17 against none). The recordings agree. Fair play, as written. |
| Where the health bar sits with an older build's unit status (no attachment, `[0, 0, 95]`) | Proven for a standing hero: the bar is placed from the model's origin and sits lower, on the top of the head, where Valve's floats clear above the hat. A bot does not crouch, so following the head down was not filmed. |
| Unit status missing, or written as a list | Still open. The ability models that carry them (a summoned keg, the goo ball) need the ability unlocked, and the sandbox hero starts with none; the session that tried was cut short by a key typed outside the console. |
| Which missing layout ids stop the HUD (M042) | Proven for a code-named id: a health-bar mod's `unit_status_overlay_v2` without `UnitHealthbar` gives `FATAL ERROR: Unable to find child 'UnitHealthbar'` and the game never leaves its loading screen. An id the code does not name was not tried. |
| A HUD mod from an older build | New: a whole-HUD mod's `hud.xml` uses eight panel types the client no longer registers; the game logs `Unrecognized panel type` for each, then `FATAL ERROR: Unable to load layout file 'hud.xml'`, marks the install corrupt, and Steam verifies it at the next launch (25 minutes on this disk), which resets `gameinfo.gi` and drops every mod's mount until `magician install` puts it back. |
| Stereo or 48 kHz replacements play wrong (M065) | Disproven. Test tones (1 kHz for 0.5 s, a gap, 2 kHz) mono and stereo, and a community replacement at 48,000 Hz: every one plays at its own speed and pitch (1,000 Hz, 0.50 s; the 48 kHz file matched at x1.000 against a search from x0.80 to x1.25). Through the hero's 3D event a stereo file is placed and as loud as the mono one (-41.7 and -42.0 dB, the same lean to one side); played flat with `play` it keeps its channels apart. CSDK 13 resamples a 48 kHz WAV to 44.1 kHz when it compiles one. |
| What a lost ragdoll part does on death (M160) | Proven for a weapon part: a Doorman skin without Valve's `weapon_offset` part leaves his gun hanging in the air where his hand was while the body is gone, until he comes back; vanilla's gun goes with him. |
| A moved attachment past the off-body line spawns its effect away from the hero (M062) | Still open. A Lash pak with `palm_R` moved 80 units off the hand (52 off the body, past the 19.5 line) was filmed meleeing a bot: the melee effect is a faint blur on the swing, and nothing in the frames tells its place apart from the real Lash's. `dumpparticlelist` gives its bounds in another frame than the world. |
| Root attachments are placed in the hero's own frame | Proven. The same pak raised `standing_pivot` 40 units along the hero's frame's up axis: the standing camera rose straight up over the head, and the crouching camera, whose pivot was left alone, did not move. Turned by the root's bind rotation the move would have pushed the camera forward along the street. |
| A particle whose name the code builds at run time (M084) | None found. The client holds one format for a particle path, `%s.vpcf`, for `info_particle_system`'s effect name; every map names its effects with the extension. `dumpparticlelist` (it reaches console.log) after every ability, melee and death of seven heroes showed 335 live systems, none of them in the unreached list. |
| The `look` scenario | Its full-bright sheet was the lit one: `mat_fullbright` is a cheat cvar that changes nothing (a lit and a full-bright frame at one angle differ no more than two lit frames do), and retail has no unlit view. 900 mouse units turned the inspect orbit about 45 degrees at this machine's sensitivity (3600 units came round to the front), so the four "sides" were 0 to 135 degrees and each light started half a turn on. `look` now takes three lights at 1800 units a step. |

Rule changes: M042 also says a replaced layout's panel types the game no longer registers (in neither Valve's layout at that path nor the client or Panorama UI library's strings), an error; on the corpus that is 6 paks of 3 mods (one HUD mod in four variants, another HUD collection, a config pack's settings popup), every one already broken by another rule, so no verdict changes. Its id error now says what the game does. M065 is a note, with what was heard. M160's and M084's texts say what was seen. Tests: `m042_a_replaced_layout_uses_a_panel_type_the_game_no_longer_has`, the M065 and M160 tests' new lines, and in `test_mod_session.py` the hand-back of a test pak put over another mod's slot, which failed once in these sessions (the kept copy was refused while the test pak still held the slot). Lint judgement revision 8.

What the harness learned on the way, kept in the harness README: `!picker` passes through players (a bot dies with `ent_kill <index>`), the sandbox hero has no ability until one is upgraded, and a fatal layout leaves the game on its loading screen with the console dead.

## Judgement round on 6880: lost attachments and what still breaks the skins

The same build, 6880, and the same dataset as the round above: the community corpus (144 mods, 143 with paks, 285 paks, 83 skins), linted deep with no allowances and read as data only; Valve against itself (Haze, Lash, Viscous and Geist's model folders, Haze's ability particles, `heroes.vdata`, `abilities.vdata`, Haze's hero graph and the base hero graph: 1,093 of Valve's files packed unchanged into one pak); and woflo's skins, Maid Lash and Lööps Viscous. Valve's files were measured with a throwaway reader over the build's own paks (not committed).

| Corpus, deep, no allowances | Before (judgement 7) | After (judgement 8) |
|---|---|---|
| Errors | 4,244 | 3,494 |
| Warnings | 78,420 | 79,170 |
| Mods broken | 87 of 143 | 77 of 143 |
| Skins broken | 66 of 83 | 61 of 83 |
| Lost attachments | 623 errors | 121 errors, 502 warnings |

Valve's own files packed unchanged read 0 errors before and after. Maid Lash (0 errors, 48 warnings) and Lööps Viscous (0 errors, 31 warnings) are unchanged.

### Lost attachments, judged by who names them

`magician index` now writes `attachment_users.tsv`: every name a vdata entry, particle, anim graph or clip holds under a key that says attach, with who the game runs it for (a vdata entry's owners, a file's users), and the attachment names of Valve's hero models that the client library holds, each with how many of the 42 playable heroes' own models carry it.

What the index found:

- vdata names attachments in abilities, items and NPCs: 157 uses (`m_strAttachmentName` beside `m_eAttachmentType`, `ability_apply` most often; `m_strCustomAttachmentSource`, `m_strEffectsAttachName`, `m_sHealthBarAttachment`, `m_strShootAttachment`).
- Clips name them in animation events: 96 uses (`m_attachmentName`, `m_attachmentPoint0`, `attachment_point`). The users index gives a clip to every hero whose data reaches it, so the line counts only a clip the model's own graphs play.
- Particles name attachments almost only in `m_controlPointConfigurations` (1,456 uses), the particle editor's preview (Valve names the configuration `preview`). The other 48 are `C_OP_SetControlPointsToModelParticles`, which reads the models the particle draws, not the hero. No particle function attaches to the hero by name at run time; the game attaches an effect where vdata or the code says.
- Anim graphs name no attachment.
- A hero model's own game data names its health bar's attachment (`m_strUnitStatusAttachmentName`), its muzzles, its zipline effect point and its camera poses (`AttachmentCameraData`).
- The client library holds 65 attachment names of Valve's hero models.

Calibrated on Valve, the 42 playable heroes' own models:

| What names an attachment the hero's own model lacks | Lacked | Heroes |
|---|---|---|
| the hero's own game data | 0 | 0 |
| a clip the hero's graphs play | 0 | 0 |
| the hero's own vdata | 1 (Seven's `melee_fx`, a melee ability's effect point) | 1 |
| an item any hero carries | 0 | 0 |
| a particle editor preview of the hero's own effects | 108 | 39 |
| the client code | 17 names are on all 42 heroes; the other 48 on 0 to 41 (`attach_hitloc` on 8, most on 1) | every hero lacks some |

Those heroes play. So a preview names nothing the game looks up, and the code's string alone does not say a hero needs the name: it copes without the 48 that only some heroes carry. The 17 every hero carries are `aim`, `healthbar`, `standing_pivot`, `crouching_pivot`, `ability_apply`, `ability_apply_ground`, `ability_cast`, `palm_l`, `palm_r`, `head_fx`, `idol`, `mask`, `sole_l`, `sole_r`, `zipline` and the two `shoot_test_` points.

The line: a lost attachment is an error when something the game runs for the model names it, and the message says what and where, and that what is placed there goes to the model's origin or nowhere (UNVERIFIED in game): the model's own game data; the vdata of its own unit's abilities, or of an item or cosmetic ability any hero carries (an NPC's, a boss's or another hero's own data is that unit's); an animation event in a clip its graphs play; or the client code, for a name every playable hero carries. A name Valve's game data gives a role (the health bar's attachment, a camera pose) that this model's game data gives no attachment or another is not looked up on this model: a warning that points at the game-data finding, which already says what moves. Otherwise a warning that says who does name it: a preview, another unit's data, animation played on another model, or nothing. With no attachment users index, or one written without the client library, a lost attachment stays an error that says so.

| The 623 lost-attachment errors of the baseline | Now |
|---|---|
| nothing in the game names it (`hero_attach_0` to `_4`, `placementOrigin` and the like) | 313 warnings |
| Valve's game data names it in a role this model's game data places elsewhere (`healthbar` 55, the camera poses `near_00`, `far_00`, `gunaim_00` 39 each, `portrait_camera_attach` 11) | 183 warnings |
| only particle editor previews name it | 6 warnings |
| the client code, for a name every hero carries (`mask` 41, `standing_pivot` 39, `crouching_pivot` 39, `head_fx` 2) | 121 errors |

The 55 `healthbar` sites are all models whose game data is an older build's, with no attachment for the bar: the game places the bar from the origin whether or not the attachment is there (seen on 6880 in the round above), and the game-data finding says so. The round above saw the standing camera follow `standing_pivot` in game, so the code does look that name up. All 121 are on models of abilities and summons: a rabbit form (117, one mod in 39 paks), a cat form (2) and a summon (2). The last 4 are in skins, none of them broken by it alone. Tests: `m062_a_lost_attachment_is_judged_by_who_names_it`, `attachment_users_read_back_only_in_their_own_format`, `attachment_names_are_what_a_key_that_says_attach_holds`.

### What still breaks the skins

Per skin, the error kinds that break it, after this round; 38 of the 61 are broken by one kind. A skin can be broken by several.

| Error kind | Skins | Broken by it alone | Sites | Call |
|---|---|---|---|---|
| M062 attachment moved in the hero's own frame | 22 | 8 | 242 | real |
| M030 texture a drawn material lacks | 21 | 8 | 75 | real (what the fallback looks like UNVERIFIED) |
| M062 lost bone the model or code still names | 20 | 13 | 21 | real (where the attachment lands UNVERIFIED) |
| M030 sound a replaced event file lacks | 11 | 6 | 1,831 | real |
| M062 animation references missing (the A-pose) | 5 | 2 | 16 | real |
| M062 game data (unit status missing or a list) | 5 | 1 | 6 | real |
| M070 skeleton, graph or vdata that lacks Valve's | 5 | 0 | 5 | real |
| M083 particle function in the wrong list | 5 | 0 | 26 | real |
| M030 material a model lacks | 4 | 0 | 5 | real |
| M062 lost attachment the code names, every hero carrying it | 4 | 0 | 4 | real (what the code looks up UNVERIFIED) |
| M062 bone reparented with its vertices | 3 | 0 | 46 | real |
| M030 particle a file lacks | 2 | 0 | 4 | real |
| M153 draw with the error material | 2 | 0 | 2 | real |
| M071, M010, M143, M030 on a graph | 1 each | 0 | 1 to 32 | real |

Why each is real:

- **Moved attachments in the hero's own frame** (`aim`, `root_aim`, `ability_apply_ground`, the camera poses): mostly Valve's attachments through a Y-up/Z-up export swap. The aim point moves from 67 units up to 67 units forward, a ground effect stands on its side. Kept an error by woflo's call of 2026-11-27.
- **M030 on textures**: the material is drawn by a model the game loads, and the texture is in neither the mod nor the build. The engine binds a fallback resource in its place (`resourcesystem.dll` warns "requested but is not in the system"; `materialsystem2.dll` holds "[Fallback Resource]"). What the fallback looks like was not seen (UNVERIFIED). This includes morph atlases a model names and does not ship (its faces stop moving) and the CSDK's own default textures (`default_mask_tga_2aa88af4` and the like), which the CSDK compiles under another hash than the build ships.
- **Lost bones**: 17 are `attachWorld` with the portrait camera's attachment still riding it. In Valve's animation `attachWorld` sits at the origin turned 120 degrees about (1, 1, 1) and never moves, so the camera's offsets are read in that frame; without the bone the attachment has no frame to be read in. Where the portrait camera lands then was not seen (UNVERIFIED), so it stays an error. The others are a bag's hitbox and attachment on a lost bone and three bones the client code names on effect models.
- **M030 on sounds**: replaced sound-event files name sound files the build no longer ships; those lines and sounds are silent.
- **M062 game data and A-pose**: no animation skeleton or AnimGraph 2 reference, or no unit status: the model stands in its bind pose or has no place for its health bar.
- The rest (M070 skeletons that lack Valve's bones, M083 particle classes in the wrong list, M153 error material, M071, M010, M143, reparented bones a skin moved with its vertices) are as the rounds above measured them.

Two kinds were suspicious and are fixed by measurement:

- **M030 in a file the game never reads.** A file that replaces no game file and that no file the game reads names, at any remove, is never opened, so what it lacks is never asked for. Valve ships the same: Infernus's unused arm material names three textures 6880 does not ship. Reached from the files that replace a game file, through what each mod file names (compiled references, Panorama text, sound files): 240 M030 errors became warnings that say so. Test: `m030_in_a_file_the_game_never_reads_is_a_warning` (the same material drawn by a replaced model stays an error).
- **M030 on a model's AnimGraph 1 include model** (`m_refAnimIncludeModels`, a `_backup` or `_temp` copy of the hero): none of Valve's 22,326 files under `models/` names one, and the hero animates from its AnimGraph 2 references, as with the AnimGraph 1 graphs the round above made warnings. 6 errors became warnings. Test: in `m030_an_animgraph_1_reference_is_a_warning`.

### M031 on the HUD mods' styles

The 37 style findings the round above left open (34 paths on 6 mods) were searched for in Valve's own Panorama files and in every file of the mod that ships them. Valve's stylesheets import with `@import url("s2r://panorama/styles/...")` (64 of them), which the reader already follows, and no Valve file holds `<include` in its text. None of Valve's layouts, stylesheets or scripts names any of the 34 paths, whether in its raw bytes, its decoded layout tree or its DATA strings. No file of the mods names them either, apart from one orphan naming another orphan (`base/citadel_hud_top_bar_chat` imports `base/citadel_hud_top_bar`, and nothing imports it). They are copies of Valve's stylesheets under a `base/` folder that the mod's own overrides import in other variants, older notifier styles and a numbered mist stylesheet. The reader is right and the findings stay.

### M001 and the fonts

Panorama finds fonts through fontconfig. `citadel/panorama/fonts/fonts.conf` lists folders on disk (`../../citadel/panorama/fonts`, the system's font folders) and the file names its `fontpattern`s allow, and includes `core/panorama/fonts/conf.d`. Valve ships its 82 fonts and `fonts.conf` loose in `game/citadel/panorama/fonts`; no font is in any pak but VGUI's `vgui/fonts/*.ttf` in core. Valve's stylesheets name fonts by family (207 Panorama files use `font-family`), and none of its layouts, stylesheets or scripts names a font file. So a font inside an addon pak is not on disk and fontconfig never sees it, whatever its case. The two M001 errors were one mod's `panorama/fonts/Mojang-Bold.ttf` and `Mojang-Regular.ttf`, asking for a rename that would not help; they are warnings that say the font is not read. Whether the engine's fontconfig also reads through the pak file system was not tried in game (UNVERIFIED). Test: the font case in `m001_uppercase_path`.

### Open

- What the engine's fallback resource for a missing texture looks like on a hero (M030 on textures, 21 skins, 8 broken by it alone).
- Where an attachment riding a lost `attachWorld` lands (17 sites, 13 skins broken by it alone).
- What the client code looks up by the other 16 names every hero carries (`standing_pivot` was seen), and whether it does so on the ability and summon models that lack them.
- Whether a font in a pak is ever read.

None of these was seen in game: another lane holds the seat. Lint judgement revision 9.

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

Tagged sounds were planned as an error and are fair play instead. `TaggedSoundSettings_t` names `scripts/tagged_sounds/<hero>_anim_sounds.vdata`, which maps the animation's tags to the footstep, landing, mantle and melee-swing events. Without it those make no sound, and footsteps are a cue enemies listen for, so it is judged with the hidden cues: never allowable. Nobody has heard one of these 16 mods in game: UNVERIFIED.

The health bar warnings are an older build's game data: no attachment name and a fixed offset (`[0, 0, 95]`), where 6877 places the bar on the `healthbar` attachment with an offset of `[10, 0, 0]`. The bar then sits over the model's origin and does not follow the hero down when he crouches (UNVERIFIED). One of these was a false alarm in the first draft: Lööps Viscous's inflated model names `healthbar` and has no attachment of that name, and neither has Valve's model. That is Valve's own design, and a lost attachment of Valve's name is the contract's own finding anyway, so the rule now speaks only when a mod renames the attachment to one its model lacks. Test: `m062_reads_the_hero_s_game_data_against_valve_s`.

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

Not every missing id is the launch crash. The client library holds as strings the ids its code looks up, and 32 of the 80 are among them (`GameTime`, `UnitHealthbar`, `CitadelHudCombatLog`); the other 48 are not (`ScoreCounter`, `MatchStart`, decoration). A code-named id is an error, the rest a warning: 20 errors on 2 mods (one HUD mod in four variants, one health-bar mod in two), 24 warnings on 6. Those 2 mods are newly broken; in light lint 55 of the 75 mods are now broken, where 53 were. Valve's own layouts are silent. Whether a given missing id crashes the HUD or leaves it blank was not tried: the corpus is not run, and building a layout with an id removed was left for a later in-game pass (UNVERIFIED). Test: `m042_a_replaced_layout_lacks_an_id_valve_s_current_one_has`.

### M065 sound-format-parity

The CTRL block of a `.vsnd_c` gives its rate, channels, sample count and length. On the 11,855 sounds the corpus replaces, the length always matches the samples and rate (2 exceptions), so nothing in the header shows a sound that plays sped up. What differs is the encoding: 5,700 replacements are stereo where Valve's are mono (13 mods), 1,655 are 48,000 Hz where Valve's are 44,100 (4 mods), and a few run at 22,050, 32,000 or odd rates such as 13,111. M065 says so, with both lengths and the pitch and volume of the event that plays the file: 6,005 warnings on 19 mods, none on woflo's skins. Whether stereo or another rate is the quiet or sped-up sound modders report is not settled, so it is a warning to listen, and `witness` is where it should be calibrated (UNVERIFIED). Test: `m065_a_replaced_sound_encoded_unlike_valve_s`.

### M160 physics-dropped

The PHYS block holds the cloth (the FeModel) and the ragdoll or a prop's body (one part per bone). On the corpus, 13 replaced models have no cloth where Valve's has 9 to 264 nodes, and 29 models lack physics parts Valve's has: 35 findings, often one part (`root_motion`, a weapon or purse offset), sometimes whole limbs (9 of an archer's 19 parts). 48 warnings on 34 mods; none on woflo's skins. A skin's own cloth of another size is not judged here (M141 judges how it moves). What a lost part does on death was not seen (UNVERIFIED). Test: `m160_valve_s_cloth_or_ragdoll_dropped`.

### M084 particle-unreached

`magician index` now writes, beside the users index, every particle nothing names: no compiled file references it (on 6877, 13,612 references from other particles, 331 from clips, 185 from models), no vdata (8,289), layout (21) or map entity lump (654) names it, and the client code does not. That is 3,758 of the 18,043 particles; many look like effects of older versions of abilities (Haze's `haze_stalk`, `haze_leap_cast`). A first count left out the abilities of heroes the users index skips, which named 191 more; every vdata is now read whoever owns it, and the list matches a full scan of every decoded string in the game.

Valve's own unreached particles, packed unchanged, fire M084 (21 of Haze's): that is the point, since replacing one does nothing, and it says nothing against Valve's file. On the corpus, 1,961 warnings on 26 mods. Maid Lash has two: `lash_ground_strike_v2_cone_line_pulse` and `lash_ultimate_locked_aura`, which the skin replaces and which nothing in the game plays. A particle whose name the code builds at run time cannot be seen this way (UNVERIFIED). Test: `m084_a_replaced_particle_nothing_plays`.

### M092 other-hero-file

A mod about one hero is a mod that replaces that hero's model; the users index says which hero each replaced file belongs to. A file only another hero uses is said: 117 warnings on 5 mods. They are real: another hero's voice lines in a voice pack (86), another hero's particles and textures in an effect rework, a shared noise texture only one other hero still uses. None on woflo's skins. Test: `m092_a_hero_mod_replaces_another_hero_s_file`.

### Installed paks and `gameinfo.gi`

`install --status` now lists the paths two installed paks share and names the one the game reads them from, the lowest number (`install::collisions`; test: `installed_paks_that_share_paths_and_which_wins`). On this machine none of the installed paks share a path.

`doctor` reads `gameinfo.gi` against the stock file `install` backs up before its first edit of a build (`gameinfo.rs`): the SearchPaths lines the stock file has and this one lacks, the lines another tool added beside Magician's mount, and the sections a config must not touch (Engine2, MaterialSystem2, NetworkSystem, Particles, RenderSystem, SceneSystem, WorldRenderer; the matchmaking refusal they cause is one community page's report, UNVERIFIED). On this machine the newest backup is 6864's and the file matches it plus the mount. Tests: `the_stock_file_against_itself_and_with_magician_s_mount`, `a_block_pasted_from_an_older_build_and_a_performance_config`, and the `gameinfo` fuzz target.

### What stays UNVERIFIED

Nothing in this round was seen in game: silent footsteps without tagged sounds, the health bar's place with an older unit status, which missing layout ids crash the HUD, whether a stereo or 48 kHz replacement plays wrong, what a lost ragdoll part does on death, and the matchmaking refusal for a restricted section. Lint judgement revision 6.
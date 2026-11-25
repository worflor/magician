# Made with Magician

Two of the three full hero skins built with this toolchain, and what Magician says about each. Every block below is
the tool's own words on Deadlock build 6868; the only cuts are the lines naming files on the author's disk and three
columns of the proof ledger. The third skin is not public yet and is not shown.

## Maid Lash

![Maid Lash in game](showcase/maid-lash.jpg)

A complete Lash skin: outfit, cloth, effects, HUD art, animation and sound. It is public, and installs through
Deadlock Mod Manager from [its page](https://woflo.dev/deadlock-skins/maid-lash/), which `magician source` wrote.

```
maid-lash: ok  Costume (look, effects, sound)  (build 6868)
  lint   warn   0 error(s), 343 warning(s), 20 allowed (deep; fair play pass)
  grade  ok     Costume (look, effects, sound) for Lash; release gate pass
  proof  ok     these bytes proven on 6865, 6866, 6868; 6864 (claimed for other bytes or never recorded)
  drift  ok     proven on 6868, the installed build
  scene  ok     lash: the stock map matches the patch
```

What stands behind each line:

- **The grade.** Lash is 2,201 files in the game's own data. The skin ships 644, and the grade says which parts of
  the hero they make its own:

  ```
  Costume: the same character, a new model (character rubric)
    look     model, materials, UI art 11 of 13, scenes 1 of 3
    effects  ability VFX, animation
    sound    kit sound, voice
  ```

- **Fair play.** The skirt is wider than Lash's hitboxes, which is a fair-play finding. It is accepted at that one
  site with a written reason and a measured size, and the grade record carries the numbers: 11.27 units at the worst
  point against a limit of 18, on 0.39% of the visible surface against a limit of 3.5%. Nothing else is allowed, and
  the rules that can never be allowed (a smaller silhouette, a hidden cue) found nothing.

- **The proofs.** A proof is of bytes, on a build, after someone looked at the frames. This is the skin's ledger:

  ```
  build  date        owner          file                             crc32
  6864   2026-11-21  maid_lash      pak10_dir.vpk                    3a6a5aff
  6865   2026-11-24  lash_showcase  maps/ui/hero_prefabs/lash.vpk    d287ca64
  6865   2026-11-24  maid_lash      pak10_dir.vpk                    24994007
  6866   2026-11-24  lash_showcase  maps/ui/hero_prefabs/lash.vpk    d287ca64
  6866   2026-11-24  maid_lash      pak10_dir.vpk                    24994007
  6868   2026-11-25  lash_showcase  maps/ui/hero_prefabs/lash.vpk    d287ca64
  6868   2026-11-25  maid_lash      pak10_dir.vpk                    24994007
  ```

  The 6864 row is an earlier pak. That is why `check` says 6864 is "claimed for other bytes": the release that is out
  now was never seen on that build, and Magician will not say it was.

- **Two game updates.** Builds 6866 and 6868 arrived a day apart. Each time `magician patchday` said what Valve had
  changed under the skin, `magician reprove` ran its in-game pass, and the proof was recorded after the contact
  sheets were read. The record of both is in [PATCH-DAY.md](PATCH-DAY.md#drills).

- **The hero page.** Its showcase scene is a map of 25 files, 22 of them Valve's. `magician scene` ships the three
  that are the skin's and builds the rest from the player's own game, so no Valve file is handed out.

## Lööps Viscous

![Lööps Viscous in game](showcase/loops-viscous.jpg)

Viscous, except a long-haired calico has got into the diving suit. The goo is milk and the skeleton is cereal. It is a
new character on Viscous's rig, 466 files, and it is still in production.

```
loops-viscous: RE-PROVE  Echo (look, effects, sound) · unproven  (build 6868)
  lint   warn   0 error(s), 76 warning(s), 64 allowed (deep; fair play pass)
  grade  warn   Echo (look, effects, sound) · unproven for Viscous; release gate blocked: freshness: not proven on 6868 (no proven build on record for these bytes)
  proof  warn   these bytes proven on none
  drift  warn   these bytes are proven on no build yet: nothing to compare from
  scene  ok     viscous: the stock map matches the patch
```

It loads with no lint errors, and that is all Magician will say for it. Nobody has recorded an in-game pass of these
bytes, so the label reads `unproven`, the release gate is closed, and `check` exits 4 until someone looks.

```
Echo: a new character (character rubric)
  look     model, materials, UI art, Valve's scenes
  effects  ability VFX, animation
  sound    kit sound, Valve's voice
  note: 18 dropped dependencies (M061): effects or sounds of Valve's originals that no longer play
```

## Try it on your own mod

`magician check` in a mod's folder prints the same five lines for it. A mod made without Magician gets there with
`magician init` beside its pak; [moving-to-magician](../skills/moving-to-magician/SKILL.md) is the whole route.

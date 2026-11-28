# Publishing to Divine Skins

`magician bundle <bundle.json> --target divine` writes an upload kit for [Divine Skins](https://divineskins.gg) from
the same `bundle.json` and paks as the Deadlock Mod Manager bundle, after the same pak checks (no path in two paks, no
file byte-identical to Valve's). It uploads nothing: the account, the form and the submit button are the author's.

Divine's rules come from its wiki ([Publish your mod on Divine](https://wiki.divineskins.gg/en/docs/deadlock/publishing/publish-on-divine)),
its [Content Rules](https://divineskins.gg/legal/content-rules), its [Creator Terms](https://divineskins.gg/legal/creator-terms)
and the upload form's own checks, read on 2026-11-27.

## What Divine takes

- **The upload:** a `.zip` or `.7z` of at most 300 MB, with the `.vpk` files and readme text or images inside, and
  nothing else: no programs, scripts or installers, no `.rar`, no archive inside the archive, no password. So a
  bundle's scene patch and its installers stay out; the kit says so in the readme.
- **The readme:** which pak number to use, what the mod conflicts with, how to install it by hand.
- **Pictures:** one thumbnail, 16:9, at least 960x540 (10 MB at most), and a gallery of at least two in-game
  screenshots (5 MB each at most; PNG, JPG, WEBP, AVIF or GIF). Renders only as extras, said to be renders. A
  YouTube link is optional.
- **The page:** a title that is the mod's name, a Markdown description, category (Hero Skins needs the hero), theme,
  features, tags, licence (CC BY-NC 4.0 preselected), links to the author and their work. Downloads stay on Divine:
  the page may not send players elsewhere for the mod.
- **Content labels:** 18+ or not, and the AI question: per part (Art, Models, Sound, Voice, Code, Text, Mod page) one
  of No AI, AI helped, Partly AI-made, Mostly AI-made, and the tools from Divine's list when a part is partly or
  mostly AI-made. Players see the answers on the page. Honest answers are allowed; undisclosed use is corrected by
  the moderators and, repeated, can cost the account.
- **Updates** are releases of the same mod in the Creator Panel: a version number (32 characters at most, no `<` or
  `>`, not used before) and a changelog of 1 to 30 lines, each tagged Added, Fixed, Changed, Removed or Improved.
- **Install:** players install by hand, or with the page's Install with Deadlock Mod Manager button (DMM 2.3.0 and
  newer, `deadlock-mod-manager://install?source=divineskins&...`): DMM fetches the release's file from Divine's CDN
  and checks its SHA-256. Divine's own launcher, Celestial, does not install Deadlock mods.

## The spec

A `divine` block in `bundle.json`. The full key list is at the top of `tools/bundle.py`.

```json
"divine": {
  "out": "D:/work/divine/maid-lash",
  "title": "Maid Lash: N°7, Grand Service",
  "hero": "Lash",
  "features": ["Model", "Textures", "VFX", "Animations", "Sounds", "HUD"],
  "tags": ["maid", "outfit", "cloth"],
  "thumbnail": {"src": "D:/shots/hero.jpg"},
  "gallery": [{"src": "D:/shots/hero.jpg", "kind": "game"}, {"src": "../proof/four-sides.png", "crop": [1240, 22, 1860, 1080], "kind": "game", "out": "back"}],
  "short": "One line.",
  "description": "DIVINE.md",
  "conflicts": "Any other Lash skin.",
  "tested": "6868, 6875 and 6880",
  "ai": {"art": "mostly", "models": "mostly", "sound": "mostly", "code": "mostly", "page": "mostly", "tools": ["claude"],
         "notes": ["what each answer rests on, for whoever fills the form"]}
}
```

## What it writes

```
<zip>.zip           the paks and readme.txt at the top level; the same inputs give the same bytes
thumbnail.jpg       cropped to 16:9 from the middle, 1920x1080 when the source is that big
gallery/NN-<name>.jpg   in the order given, no longer than 2560 on the long side
description.md      the page text
listing.txt         every field of the form in its order, the AI answers and their notes, the description
SHA256SUMS.txt
```

It refuses, before writing anything, what the form or the review would refuse: fewer than two in-game screenshots, a
thumbnail under 960x540, an unknown AI level or tool, a partly or mostly AI-made part without its tools, a changelog
Divine would not take, a zip over 300 MB. It replaces `out` only when that is an earlier kit (it holds `listing.txt`)
or empty.

## Not known from outside the form

The Deadlock theme and feature lists (seen on live pages: themes such as Anime; features Model, Textures, VFX, Voice
lines, Animations), whether the first upload asks for a version and changelog or only later releases do, and whether
the form has a short-description field. `listing.txt` marks those fields "pick on the form" or "if the form asks".

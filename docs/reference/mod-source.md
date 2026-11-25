# A mod source for Deadlock Mod Manager

`tools/source.py` turns release bundles into a static mod source: a few JSON files and the release zips on any HTTPS
host. A player adds the source to Deadlock Mod Manager (DMM) once; from then on the mods are in its Mods Store with a
one-click install, and a new release shows up as an update. The protocol is DMM's native source protocol,
[Build a mod registry](https://docs.deadlockmods.app/developer-docs/build-a-mod-registry). Nothing about it needs an
account, a key or DMM's own servers.

**Which DMM.** Mod sources (Settings > Mods > Mod sources, and the `deadlock-mod-manager://sources/...` links) landed in
DMM's main branch on 2026-11-03, after 2.3.0 (2026-11-01), so 2.3.0 does not have them. They are in the 2.4.0 nightlies
(main is versioned 2.4.0), so 2.4.0 should be the first release with them. Release builds accept only public HTTPS sources, so the first real test is on the hosted copy.

## The spec

A small JSON file beside, not inside, the folder it writes:

```json
{
  "source_id": "my-skins",
  "name": "My Deadlock skins",
  "base_url": "https://example.github.io/deadlock-skins/",
  "out": "site",
  "taxonomy": "taxonomy.json",
  "mods": [
    {"bundle": "~/model-replacements/maid-lash/package/bundle.json", "zip": "D:/packages/MaidLash_1.0.0.zip"},
    {"bundle": "~/model-replacements/another-skin/package/bundle.json", "bundle_dir": "D:/packages/another-skin-rc2",
     "sections": ["The voice box", "Not seen yet"], "preselect": []}
  ]
}
```

`source_id` is permanent: with the descriptor's origin it is the source's identity in every player's DMM. `base_url`
is where the folder is served (`--base-url` overrides it). Each mod is a `bundle.json` with the zip `bundle.py --zip`
wrote (`zip`, or `bundle_dir` and the zip is found beside it). `sections` are README sections copied into the
listing's description (default `Not seen yet`); `preselect` ticks optional mods by default. Optional keys:
`downloads_url` (default `base_url` + `files/`; may hold `{id}` and `{version}`), `media_url`, `download_origins`,
`icon`, `homepage`, `taxonomy` (a copy of `https://deadlockmods.app/v1/taxonomy.json`, to check hero and content-type
ids). The full list is at the top of `tools/source.py`.

From `bundle.json` a listing needs `release`, `date`, `hero_id`, `content_type` and an explicit `nsfw` (DMM hides an
unknown classification along with adult content); the tool refuses a spec without them, a zip built from another
spec or release, and a zip whose paks do not match its own `mod-interchange.json`.

The listing and the mod's page say which game builds the release was proven on in game, and only for the bytes in
the zip: the mod's proof ledger (`proofs.tsv` beside its `mod.toml`, an entry's `mod_toml` or the one beside the
bundle's folder, written by `magician prove`) is read, and a build counts when one complete proof of it holds the
sha256 of every VPK in the zip. `[proven] builds` names no bytes and is not read. A release whose paks have no proof
(a rebuild) says nothing about proofs; a re-prove after a game update shows on the next run without a new release.

## The command

```
magician source source.json [--out DIR] [--base-url URL] [--taxonomy FILE] [--prune]   # runs python -I tools/source.py
```

It writes `dmm-source.json` (the URL players add; also `.well-known/dmm-source.json` when the source is at the root of
its origin), `catalog.json`, `mods/<id>.json`, `mods/<id>/releases.json`, `media/<id>/*.jpg`, `files/<zip>` (when
downloads live on the same host), `index.html`, a page per mod (`<id>/index.html`), `feed.xml` and `.nojekyll`. URLs inside the tree are relative, so the folder works
wherever it is served. It writes only files it wrote before (listed in `.dmm-source-files`) or new ones, never over
anything else, so the folder can be a git repository with its own README. Same inputs, same bytes.

DMM's repository has an offline checker for static sources with relative URLs:
`node examples/mod-source/check.mjs <out>`. A tree whose downloads live on another host is outside what it checks;
the tool runs the same rules on its own output (origins for every URL, artifact names, the component plan, the size
limits).

## What a listing is

One listing per bundle. The artifact is the release zip itself, with its size and SHA-256, so a player gets the same
bytes from DMM as from the landing page's download link. The installation plan is `components`: each mod of the
bundle names its VPK inside the zip, in load order; the skin is required, and an optional pak (a voice box, say) is
its own choice with `minSelections: 0`, unticked unless `preselect` names it.

Separate `.vpk` downloads were the other way. They would let a player who skips the voice box skip its bytes, but
the paks are three times the size of the zip (Maid Lash's is 111 MB, its zip 46 MB), one of them is over GitHub's
100 MB file limit, and the zip would still have to be hosted for the scene. The zip wins on every count that matters.

A source can't run installation scripts, so DMM installs the paks but not a bundle's hero-page scene. The listing's
description says so and links the zip, which carries `install-scene.cmd` and `install-scene.sh`.

## The pages

The tree is also a small website. `index.html` has a card per mod and the Add source button; each mod has its own
page at `<id>/` (the listing's `homepageUrl`, which DMM shows as "View post on <source name>") with its pictures, the
Install with DMM button, the same description DMM shows, every release with its notes and zip, and the checksum.
`feed.xml` is an Atom feed of releases. Every page carries Open Graph tags, so a pasted link shows the cover.

The look is the spec's `page` block: `title` and `intro` for the front page, `css` (a stylesheet appended to the
built-in one, which is written on a handful of variables), and `head`, `header` and `footer` (HTML files put on every
page, with `{root}` standing for the way back to the top of the tree). That is enough to make the pages one with the
site they are served from. Per mod, `tagline`, `blurb` and `links` (a trailer) feed its card and page.

`upcoming` lists what is coming: `title`, `hero`, `when`, `note`, `image`. Each is a card on the front page. With
`in_manager` (plus `id`, `author`, `hero_id`, `nsfw`) it is also written as a listing with `availability:
"unavailable"`: DMM shows the card, picture and description and offers nothing to install. DMM has no "coming soon"
state, so the description says what it is. When the mod is released under the same `id`, that listing becomes the
download.

## What DMM shows of a source

Read from DMM's code at `d0c8f78` (2026-11-24); main may be ahead of any build a player has.

- The source is an entry in the Mods Store's source menu beside GameBanana, with its `iconUrl` (a raster image up to
  128 KiB; the tool writes `icon` as a 256 px JPEG) and `name`. Nothing else of the descriptor is shown: there is no
  description, banner, colour or link for a source.
- A card is the first image, the title, the author and `updatedAt`. Only the first four `images` are fetched; the
  first is also the wide banner of the mod's page, cropped hard top and bottom, so lead with a picture that survives
  a letterbox. Listings appear in catalog order; there is no sorting, featuring or hero filter for a source.
- The description is Markdown (GitHub flavour) with links; images and HTML in it are dropped. No video, donation
  links, author pages, download counts, likes or comments: those are GameBanana's.
- An update is any change of release id or `contentRevision`. DMM looks every five minutes while it is open, for
  sources with an installed mod; opening the Store does not refresh a source. It shows an update badge and never
  installs an update by itself. Older releases can be read (the Changelog tab) but not installed.
- There are no release channels, drafts, schedules or unlisted mods. What is not in the catalog does not exist to
  DMM; `unavailable` is the one in-between state.
- Nothing in DMM helps a player find a source: they need its address or a `sources/add` link.

## Updates

Build the new bundle and zip with `bundle.py --zip` (a new `release` in `bundle.json`), point the spec at the new zip and
run the tool again. The release's `contentRevision` is a hash of the zip's bytes and of the plan's ids and paths; the
release id is the version plus the start of that hash. New bytes are always a new release, and DMM offers it as an
update; a description, tag or screenshot change is not one. The previous release moves into `releases.json` (DMM's
Changelog tab). Its zip stays in `files/` for downloads already under way until `--prune`.

## Hosting on GitHub Pages

A public repository with Pages on, publishing the folder the tool wrote (branch `main`, folder `/`). The zips go in
`files/` in the same repository: tens of megabytes for a hero skin, under GitHub's 100 MB per-file limit (git warns over 50 MB); the
tool refuses a hosted zip over 100 MiB. A Pages site may be up to 1 GB, and each release adds its zip to the
repository's history for good, so prune old zips and keep an eye on the size after a few releases.

The descriptor, catalog and images must be served without a redirect: DMM does not follow redirects for JSON or
media. A user site with a custom domain redirects `<user>.github.io` to that domain, and project sites follow it, so
`base_url` must be the final address (`https://<custom domain>/<repo>/`, not the `github.io` one). Check with
`curl -sI <base_url>dmm-source.json`: it must be `200`.

Downloads may live on GitHub Releases instead (`downloads_url` like
`https://github.com/<user>/<repo>/releases/download/{id}-{version}/`). A release download answers with a redirect to
`https://release-assets.githubusercontent.com` (seen 2026-11-24; it was `objects.githubusercontent.com` before). DMM
follows artifact redirects only to declared download origins, so the tool declares both `https://github.com` and that
host and prints what to upload where. The cost is that GitHub owns the redirect: if it moves to another host, downloads
fail until the descriptor declares it, and any change to the declared origins makes every player remove and add the
source again. Keeping everything on the Pages origin avoids that.

## Verified, and not

Verified: DMM's offline checker passes on the tree, with and without an `unavailable` listing; the tree
serves over HTTP with the right types and the landing page renders in light and dark and at phone width;
`tools/test_source.py` holds the guards (spec checks, the same bytes twice, the revision moving only with the bytes,
history and pruning, foreign files untouched, GitHub release origins). The descriptor and listing shapes follow DMM's
own validation code (`apps/desktop/src-tauri/src/providers/native_sources/`).

In DMM `2.4.0-nightly.20261110` against a hosted source (2026-11-24): adding the source by address, the Store
listing with cover, hero, tags, description, changelog and files, an install whose pak is the release's bytes, delete,
and the `sources/install` link from the page.

Not verified in the app: the component picker with an optional pak, DMM offering an update from one release to the
next, and how an `unavailable` listing looks (the checker and DMM's validation code accept it; nobody has looked at
the card).

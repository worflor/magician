# Publishing to Nexus Mods

`magician publish nexus <bundle.json>` (`tools/publish.py`) uploads the release zip `magician bundle --zip` wrote as a
new version of a file on an existing Nexus Mods page. It speaks Nexus's v3 upload API the way Nexus's own GitHub
Action does ([Nexus-Mods/upload-action](https://github.com/Nexus-Mods/upload-action), `src/index.ts`, with the
request and answer shapes from its `src/openapi.schema.ts`): it is the same seven calls, in Python with the standard
library.

## What the API does, and what stays on the site

The API adds a version to a file that already exists. A mod page cannot be made through it, so the page, its first
file, the description, the images, the tags and the requirements are made by hand on the site, once. After that a
release is one command.

Nexus's schema also has calls to make a new file on an existing page and to edit a page's summary and description.
Magician uses neither: the page is the author's, and a second file is a choice made on the site.

**Generative AI.** Since 3 November 2026 every mod page on Nexus Mods must declare whether generative AI was used in
it, with Nexus's labels (AI Generated Content; AI Assisted, with a note of the tools and where they were used; AI
Media). The author sets it on the page. Magician does not set it and cannot. Nexus's own post is the policy:
[AI-Generated Content in Modding](https://www.nexusmods.com/news/14850).

## The ids

A block in the mod's `bundle.json`, so a release needs no flags:

```json
"nexus": {"mod_id": "...", "file_id": "...", "category": "main"}
```

`file_id` is the file the version goes onto; `mod_id` is the page, needed only for the changelog. Both are the ids
the mod page's Files tab shows with Advanced ticked (or the file's edit menu under Manage Files), not the number in
the page's address, which is the game's own count. The other keys take Nexus's names: `category` (`main`,
`optional` or `miscellaneous`; default `main`), `name` (default the bundle's `name`), `description` (default the
bundle's), `archive_existing_file` and `update_mod_version` (default false), and `primary_mod_manager_download`,
`allow_mod_manager_download` and `show_requirements_pop_up`, which are sent only when given so Nexus's own default
holds otherwise. `changelog: false` sends none. An unknown key is a usage error, so a misspelt switch is caught.

What goes up comes from the zip and the bundle: the zip itself, the bundle's `release` as the version string, the
description cut at a word to Nexus's 255 characters, and the section of the zip's `CHANGELOG.md` for that release,
one entry per line, posted to the page's changelog for that version. Nexus appends changelog text and never replaces
it, so it is sent only when the page has none for that version yet.

## The key

`NEXUSMODS_API_KEY`, from a personal key made at <https://www.nexusmods.com/settings/api-keys>. It is read from the
environment and nowhere else, and is never written to a file, a log or an error message: a server answer that quotes
it is printed with the key blanked. Without it a real run is a usage error that says where to get one. The key goes
only to the API; the parts go to the presigned storage addresses the API hands out, without it, and no redirect is
followed. `NEXUSMODS_API_BASE` points at another server, as the Action allows; plain `http` is refused anywhere but
this machine.

## The dry run

```
magician publish nexus package/bundle.json --dry-run
```

Says what would go where (the zip, its size and SHA-256, the version, the name, the description as it will be sent,
the changelog, the ids and every switch, the API base and whether the key is set), runs the gate, and contacts
nothing. It needs no key. It exits 2 when the bundle has no `file_id` and 3 when the gate would refuse, so it can stand
in a pipeline before the real step.

## The gate

Publishing is the most outward step Magician takes, so it holds the line `magician grade --gate` holds. The zip's own
paks (not the ones in the addon folder, not the installed ones) are taken out of the zip and graded with the mod's
`mod.toml` (the one beside the bundle's folder, or `--mod`): lint errors that are not fair play, any file
byte-identical to Valve's at the same path, fair play when the policy enforces it, a recorded proof of these bytes on
the installed build (`proofs.tsv`), and rights (denied always blocks; unreviewed blocks when the policy enforces
rights). A blocker refuses the upload with exit 3 and says which; a gate that cannot judge (no `mod.toml`, no game, no
magician) refuses too. `--ignore-gate` publishes anyway and says so, for the case where the person knows better. The
bundle's scene patch was held to the same no-Valve-bytes rule when `magician bundle` built it.

## Running it

```
NEXUSMODS_API_KEY=... magician publish nexus package/bundle.json [--zip FILE] [--mod mod.toml]
```

Before anything is uploaded the file's versions are read: the same version already there with the same size is said
and nothing is uploaded (a changelog missing for it is added), and with another size it is refused, because new bytes
need a new release. Then: create the multipart upload, put each part (a part that fails is tried four times), complete
it, finalise it, wait for it to be available, create the version, add the changelog. Every request has a time limit
(30 s for the API, 120 s without progress for a part, ten minutes for Nexus to make the upload available), and a step
that fails is named with the server's own words, exit 1. Nothing is resumed: a run that failed halfway leaves an
unfinished upload on Nexus's side that no version uses, and running it again starts over. Exit codes as the rest of Magician: 0 published or already
there, 1 a step failed, 2 usage, 3 refused.

## What has been proven

By test: `tools/test_publish.py` runs the tool against a fake Nexus on this machine that implements the seven calls
and checks the key header, the part sizes, the ETags and the completion XML, and the JSON bodies; each step failing,
a part failing and being tried again, a version already there, the dry run sending nothing, the missing key, the
gate's refusal and its override, the 255-character cut, plain `http` and redirects refused, and the key in no output.

Not yet: an upload to the real Nexus Mods. The first real one will say whether the parts of a real upload and the
changelog's line split behave as the schema reads.

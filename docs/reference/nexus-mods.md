# Publishing to Nexus Mods

`magician publish nexus <bundle.json>` (`tools/publish.py`) puts the release zip `magician bundle --zip` wrote on a
Nexus Mods page: the first time as the page's file, after that as a new version of it. It speaks Nexus's v3 API the
way Nexus's own GitHub Action does ([Nexus-Mods/upload-action](https://github.com/Nexus-Mods/upload-action),
`src/index.ts`), and takes the shapes of the two calls the Action does not make (a new file, the page's text) from
the schema in the same repository (`src/openapi.schema.ts`). Python, standard library only.

## What the API does, and what stays on the site

The page itself is made on the site: the schema has no call that creates a mod page, and none that publishes one.
Making it as a draft is enough to start (the title, the category, the images, the generative-AI declaration; a phone
will do). From there the tool can:

- make the page's first file from the zip (`POST /mod-files`), when bundle.json names the page and no file;
- add a version to that file (`POST /mod-files/{id}/versions`) on every later release;
- set the page's summary and its BBCode description from files beside the bundle (`PATCH /mods/{id}`), with `--page`
  only, so a routine release never rewrites the page (the schema: the caller must be on the mod's team, and a mod
  under moderation review takes edits only when staff allow them);
- add the release's changelog to the page (`POST /mods/{id}/changelogs`).

Making the page public is the Publish button on the site.

**Not known until it is tried against the real service:** whether these calls are accepted on a page that is still a
draft or unpublished, and whether the page lookup by address (below) answers for one. The schema does not say. If
they are refused, the step that failed is named with Nexus's own words; publish the page on the site, then run again.

**Generative AI.** Since 3 November 2026 every mod page on Nexus Mods must declare whether generative AI was used in
it, with Nexus's labels (AI Generated Content; AI Assisted, with a note of the tools and where they were used; AI
Media). The author sets it on the page. Magician does not set it and cannot. Nexus's own post is the policy:
[AI-Generated Content in Modding](https://www.nexusmods.com/news/14850).

## The page and the file in bundle.json

A block in the mod's `bundle.json`, so a release needs no flags:

```json
"nexus": {"mod_url": "https://www.nexusmods.com/deadlock/mods/123", "file_id": "...", "category": "main"}
```

- `mod_url` is the page's address. The number in it is the game's own count (a game-scoped id), and the API wants
  the page's own id: the tool looks it up with `GET /games/{game}/mods/{number}` and says what it found. `mod_id`
  gives that id directly (the page's Files tab shows it with Advanced ticked); with both, they must agree. The page
  is needed for a new file, the changelog and `--page`.
- `file_id` is the file a release becomes a version of. Without it the zip becomes a new file on the page, and the
  run ends by printing the new file's id and saying to put it in the block. The tool does not write bundle.json, as
  `bundle` and `source` never write their specs. A second run before the id is recorded finds the page already has a
  file of that name and refuses (exit 3) with its id, so the page never gets two.
- The other keys take Nexus's names: `category` (`main`, `optional` or `miscellaneous`; default `main`), `name`
  (default the bundle's `name`), `description` (default the bundle's), `update_mod_version` (default false),
  `archive_existing_file` (a new version only; default false), and `primary_mod_manager_download`,
  `allow_mod_manager_download` and `show_requirements_pop_up`. A new version sends those three only when given, so
  Nexus's own choice holds; a new file sends all of them, with the defaults the schema documents (false, true,
  false) when the block says nothing. `changelog: false` sends none.
- `summary_file` and `description_file` name text files (relative to bundle.json) for `--page`. The summary is sent
  as Nexus stores it, on one line; the description is BBCode as written. Either may be given alone, and only what is
  given is changed. Neither may be blank. The schema sets no length limit on either; the site's form may.

An unknown key is a usage error, so a misspelt switch is caught.

What goes up with the zip comes from the zip and the bundle: the bundle's `release` as the version string, the file
description cut at a word to Nexus's 255 characters, and the section of the zip's `CHANGELOG.md` for that release,
one entry per line. Nexus appends changelog text and never replaces it, so it is sent only when the page has none
for that version yet.

## The key

`NEXUSMODS_API_KEY`, from a personal key made at <https://www.nexusmods.com/settings/api-keys>. It is read from the
environment and nowhere else, and is never written to a file, a log or an error message: a server answer that quotes
it is printed with the key blanked. Without it a real run is a usage error that says where to get one. The key goes
only to the API; the parts go to the presigned storage addresses the API hands out, without it, and no redirect is
followed. `NEXUSMODS_API_BASE` points at another server, as the Action allows; plain `http` is refused anywhere but
this machine.

## The dry run

```
magician publish nexus package/bundle.json --dry-run [--page]
```

Says what would go where (the zip, its size and SHA-256, the version, the name, the description as it will be sent,
whether it becomes a new file or a version of which, the page, every switch, the changelog, and with `--page` the
summary and description as they will be sent with their lengths), runs the gate, and contacts nothing, so the page's
address is not looked up yet. It needs no key. It exits 2 when the block names neither a page nor a file and 3 when
the gate would refuse, so it can stand in a pipeline before the real step.

## The gate

Publishing is the most outward step Magician takes, so it holds the line `magician grade --gate` holds. The zip's own
paks (not the ones in the addon folder, not the installed ones) are taken out of the zip and graded with the mod's
`mod.toml` (the one beside the bundle's folder, or `--mod`): lint errors that are not fair play, any file
byte-identical to Valve's at the same path, fair play when the policy enforces it, a recorded proof of these bytes on
the installed build (`proofs.tsv`), and rights (denied always blocks; unreviewed blocks when the policy enforces
rights). A blocker refuses the whole run, page text included, with exit 3 and says which; a gate that cannot judge
(no `mod.toml`, no game, no magician) refuses too. `--ignore-gate` publishes anyway and says so, for the case where
the person knows better. The bundle's scene patch was held to the same no-Valve-bytes rule when `magician bundle`
built it.

## Running it

```
NEXUSMODS_API_KEY=... magician publish nexus package/bundle.json [--page] [--zip FILE] [--mod mod.toml]
```

In order: the page's id from its address; with `--page`, the page's text; for a version, the file's versions are read
(the same version already there with the same size is said and nothing is uploaded, a changelog missing for it is
added, and with another size it is refused, because new bytes need a new release); for a new file, the page's files
are read (one of the same name refuses). Then the upload: create the multipart upload, put each part (a part that
fails is tried four times), complete it, finalise it, wait for it to be available, create the file or the version,
add the changelog. Every request has a time limit (30 s for the API, 120 s without progress for a part, ten minutes
for Nexus to make the upload available), and a step that fails is named with the server's own words, exit 1. Nothing
is resumed: a run that failed halfway leaves an unfinished upload on Nexus's side that no file uses, and running it
again starts over. Exit codes as the rest of Magician: 0 published or already there, 1 a step failed, 2 usage, 3
refused.

## The rest of the API, for an author

The schema has more than the tool uses. What an author could reach for, none of it built:

- Reading: `GET /mods/{id}` and `GET /games/{game}/mods/{number}` (a page, with its status and main file),
  `GET /mods/{id}/files` (its files), `GET /mod-files/{id}` and `GET /mod-files/{id}/versions`,
  `GET /mod-file-versions/{id}`, `GET /mods/{id}/changelogs`, `GET /team-mods` (the pages one is on the team of),
  `GET /mods/search`, and `/mods/batch` and `/mod-file-versions/batch` for several at once.
- Files: `PUT /mod-files/{id}` (rename a file), `POST /mod-file-versions/move` and `.../move-to-new-mod-file`
  (reorder versions or split them into a new file), `POST /mod-file-update-groups/{group_id}/versions`, and
  `POST /uploads` (a single-part upload; the schema says it needs an MD5 from 1 December 2026; the tool uses the
  multipart upload, which does not).
- Requirements: `GET /mod-file-versions/{id}/dependencies` (as authored), `GET` and `PUT /mod-file-versions/{id}/dependencies/ranges` and `.../dependencies/dlc`, the
  materialized views of both, and `PUT /mods/{id}/toggle-legacy-mod-requirements`.
- Bug reports: `GET /mod-bugs`, `GET` and `PATCH /mod-bugs/{id}`, `POST /mod-bugs/{id}/replies`.
- Also there: collections (`POST /collections`, `.../revisions`, `PATCH /collections/{id}`), trending mods and DLCs
  per game, the caller's content settings, download links, and quarantine (moderation services only).

## What has been proven

By test: `tools/test_publish.py` runs the tool against a fake Nexus on this machine that implements the calls it
makes and checks the key header, the part sizes, the ETags and the completion XML, and every JSON body against the
schema's fields (the new file by the page's id, not its number); each step failing, a part failing and being tried
again, a version already there, a page's first file made from its address and never a second of that name, the page's
text sent only with `--page` and only the fields given, the dry run sending nothing, the missing key, the gate's
refusal and its override, the 255-character cut, plain `http` and redirects refused, and the key in no output.

Not yet: anything against the real Nexus Mods. The first real run will say whether a draft page takes these calls,
whether the parts of a real upload behave as the schema reads, and how Nexus splits a changelog into entries.

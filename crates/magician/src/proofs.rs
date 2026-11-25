//! The proof ledger: which bytes were seen working in game, on which build.
//!
//! `[proven] builds = [...]` in a `mod.toml` says that a mod was looked at in game on a build, but not which paks were
//! looked at, so a rebuilt pak inherited every proof of the one before it. A proof belongs to bytes. `magician prove`
//! writes one into `proofs.tsv` beside the `mod.toml`: the game build, the date, every file of the mod as installed at
//! that moment (ledger owner, file name, crc32 as the ledger has it, sha256) and, optionally, the session folder it
//! was seen in. Everything that asks "is this mod proven on build N" asks it of the bytes it is judging: a recorded
//! proof for N that holds every one of them (sha256). `[proven] builds` stays as the summary a person reads and older
//! tools take; a build listed there with no proof for these bytes is a claim for other bytes, or one never recorded,
//! and counts for nothing.
//!
//! The file is plain text, one row per file per proof, tab-separated, sorted, every row standing on its own:
//!
//! ```text
//! # magician proofs/1: one row per file per proof, written by `magician prove`; a row stands alone, so a union of two copies is the merge
//! build	date	proof	owner	file	crc32	sha256	evidence	evidence_sha256
//! 6866	2026-11-24	8c0f61b2a1d94e07	maid_lash	pak10_dir.vpk	24994007	3f9a...	D:\work\patchday-6866\maid-lash	51c2...
//! ```
//!
//! `proof` is the first 16 hex digits of a SHA-256 over the build and the proof's sorted (owner, sha256) pairs, so the
//! same bytes proven on the same build on two machines are one proof (two dates, two evidence folders), and a proof
//! that lost a row in a hand merge no longer adds up to its own id: it is reported as incomplete and counts for
//! nothing (it can only ever fail closed). The reader dedups and sorts; the writer writes the canonical form. A line
//! that does not read (a merge marker, a short row) is reported, never dropped, and `prove` refuses to rewrite a
//! ledger that has one.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

pub const FILE: &str = "proofs.tsv";
const HEADER: &str = "# magician proofs/1: one row per file per proof, written by `magician prove`; a row stands alone, so a union of two copies is the merge";
const COLUMNS: &str = "build\tdate\tproof\towner\tfile\tcrc32\tsha256\tevidence\tevidence_sha256";

/// One file of a mod, identified by its bytes.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct FileId {
    pub owner: String,
    /// The ledger's name for it (`pak10_dir.vpk`, `maps/ui/hero_prefabs/lash.vpk`), or the file's name.
    pub file: String,
    pub crc: String,
    pub sha256: String,
}

/// A row of the ledger.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Row {
    build_n: u64,
    pub build: String,
    pub date: String,
    pub proof: String,
    pub id: FileId,
    pub evidence: String,
    pub evidence_sha256: String,
}

/// The rows of one proof (one build, one id), gathered.
#[derive(Debug, Clone, PartialEq)]
pub struct Proof {
    pub build: String,
    pub id: String,
    pub dates: BTreeSet<String>,
    pub files: Vec<FileId>,
    /// (path, sha256 of its index.md or `-`), one per session it was seen in.
    pub evidence: BTreeSet<(String, String)>,
    /// Whether the rows add up to the id (a proof that lost a row in a merge does not, and counts for nothing).
    pub complete: bool,
}

impl Proof {
    pub fn holds(&self, sha: &str) -> bool {
        self.files.iter().any(|f| f.sha256 == sha)
    }
}

/// The id of a proof: the build and its (owner, sha256) pairs, sorted, hashed.
pub fn proof_id(build: &str, files: &[FileId]) -> String {
    let pairs: BTreeSet<(String, String)> = files.iter().map(|f| (f.owner.clone(), f.sha256.clone())).collect();
    let mut h = grade::sha256::Sha256::new();
    h.update(build.as_bytes());
    for (o, s) in &pairs {
        h.update(format!("\n{o}\t{s}").as_bytes());
    }
    h.hex()[..16].to_string()
}

fn is_hex(s: &str, n: usize) -> bool {
    s.len() == n && s.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

fn is_date(s: &str) -> bool {
    let b = s.as_bytes();
    s.len() == 10 && b[4] == b'-' && b[7] == b'-' && s.bytes().enumerate().all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit())
}

fn plain(s: &str) -> bool {
    !s.is_empty() && !s.contains(['\t', '\n', '\r'])
}

/// The proof ledger of one mod.
#[derive(Debug, Clone, Default)]
pub struct Ledger {
    pub path: Option<PathBuf>,
    rows: BTreeSet<Row>,
    /// Lines that did not read, with their line number: reported, never dropped silently.
    pub problems: Vec<String>,
}

impl Ledger {
    pub fn parse(text: &str) -> Ledger {
        let mut l = Ledger::default();
        for (n, raw) in text.lines().enumerate() {
            let line = raw.trim_end_matches('\r');
            if line.trim().is_empty() || line.starts_with('#') || line == COLUMNS {
                continue;
            }
            let c: Vec<&str> = line.split('\t').collect();
            let why = if c.len() != 9 {
                Some(format!("{} field(s), not 9", c.len()))
            } else if c[0].is_empty() || !c[0].bytes().all(|b| b.is_ascii_digit()) {
                Some(format!("build '{}' is not a build number", c[0]))
            } else if !is_date(c[1]) {
                Some(format!("date '{}' is not YYYY-MM-DD", c[1]))
            } else if !is_hex(c[2], 16) {
                Some(format!("proof '{}' is not 16 hex digits", c[2]))
            } else if !plain(c[3]) || !plain(c[4]) || !plain(c[7]) {
                Some("an empty owner, file or evidence field (`-` for none)".to_string())
            } else if !is_hex(c[5], 8) || !is_hex(c[6], 64) {
                Some("crc32 must be 8 and sha256 64 lowercase hex digits".to_string())
            } else if c[8] != "-" && !is_hex(c[8], 64) {
                Some(format!("evidence_sha256 '{}' is neither `-` nor a sha256", c[8]))
            } else {
                None
            };
            match why {
                Some(w) => l.problems.push(format!("line {}: {w}: {}", n + 1, line.chars().take(80).collect::<String>())),
                None => {
                    l.rows.insert(Row {
                        build_n: c[0].parse().unwrap_or(0),
                        build: c[0].to_string(),
                        date: c[1].to_string(),
                        proof: c[2].to_string(),
                        id: FileId { owner: c[3].to_string(), file: c[4].to_string(), crc: c[5].to_string(), sha256: c[6].to_string() },
                        evidence: c[7].to_string(),
                        evidence_sha256: c[8].to_string(),
                    });
                }
            }
        }
        l
    }

    /// The ledger beside a `mod.toml` (an absent file is an empty ledger; one that cannot be read is an error).
    pub fn for_toml(toml: &Path) -> Result<Ledger, String> {
        let path = toml.with_file_name(FILE);
        let mut l = match std::fs::read(&path) {
            Ok(b) => Ledger::parse(&String::from_utf8_lossy(&b)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ledger::default(),
            Err(e) => return Err(format!("{}: {e}", path.display())),
        };
        l.path = Some(path);
        Ok(l)
    }

    /// The canonical text: the header, then every row once, by build, proof, owner and file.
    pub fn text(&self) -> String {
        let mut s = format!("{HEADER}\n{COLUMNS}\n");
        for r in &self.rows {
            s += &format!("{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n", r.build, r.date, r.proof, r.id.owner, r.id.file, r.id.crc, r.id.sha256, r.evidence, r.evidence_sha256);
        }
        s
    }

    /// A ledger that could not be read at all: no proofs, and the reason as its one problem.
    pub fn unreadable(why: String) -> Ledger {
        Ledger { problems: vec![why], ..Ledger::default() }
    }

    /// Record a proof: `files` seen working in game on `build` on `date`, with the session it was seen in. Returns
    /// the proof's id. Refuses a ledger with lines that do not read (rewriting it would lose them) and a field that
    /// would not stay one field.
    pub fn add(&mut self, build: &str, date: &str, files: &[FileId], evidence: Option<(&str, Option<&str>)>) -> Result<String, String> {
        if let Some(p) = self.problems.first() {
            return Err(format!("{} has {} line(s) that do not read (first: {p}); fix them by hand before recording another proof", self.path.as_ref().map_or(FILE.to_string(), |p| p.display().to_string()), self.problems.len()));
        }
        if build.is_empty() || !build.bytes().all(|b| b.is_ascii_digit()) {
            return Err(format!("'{build}' is not a build number"));
        }
        if !is_date(date) {
            return Err(format!("'{date}' is not a date (YYYY-MM-DD)"));
        }
        if files.is_empty() {
            return Err("a proof needs at least one file".into());
        }
        for f in files {
            if !plain(&f.owner) || !plain(&f.file) || !is_hex(&f.crc, 8) || !is_hex(&f.sha256, 64) {
                return Err(format!("{} ({}): not a file a proof can name", f.file, f.owner));
            }
        }
        let (ev, ev_sha) = match evidence {
            Some((p, s)) => (p.to_string(), s.unwrap_or("-").to_string()),
            None => ("-".to_string(), "-".to_string()),
        };
        if !plain(&ev) || (ev_sha != "-" && !is_hex(&ev_sha, 64)) {
            return Err(format!("evidence '{ev}' can't be written as one field (a tab or a line break in it)"));
        }
        let id = proof_id(build, files);
        for f in files {
            self.rows.insert(Row { build_n: build.parse().unwrap_or(0), build: build.to_string(), date: date.to_string(), proof: id.clone(), id: f.clone(), evidence: ev.clone(), evidence_sha256: ev_sha.clone() });
        }
        Ok(id)
    }

    /// Every proof, gathered by build and id, oldest build first.
    pub fn proofs(&self) -> Vec<Proof> {
        let mut by: BTreeMap<(u64, String, String), Proof> = BTreeMap::new();
        for r in &self.rows {
            let p = by.entry((r.build_n, r.build.clone(), r.proof.clone())).or_insert_with(|| Proof { build: r.build.clone(), id: r.proof.clone(), dates: BTreeSet::new(), files: Vec::new(), evidence: BTreeSet::new(), complete: false });
            p.dates.insert(r.date.clone());
            if r.evidence != "-" {
                p.evidence.insert((r.evidence.clone(), r.evidence_sha256.clone()));
            }
            if !p.files.iter().any(|f| f.owner == r.id.owner && f.sha256 == r.id.sha256) {
                p.files.push(r.id.clone());
            }
        }
        by.into_values()
            .map(|mut p| {
                p.complete = proof_id(&p.build, &p.files) == p.id;
                p
            })
            .collect()
    }

    /// The builds a recorded, complete proof holds all of `shas` on, ascending. Nothing judged is proven nowhere.
    pub fn bound(&self, shas: &[String]) -> Vec<String> {
        if shas.is_empty() {
            return Vec::new();
        }
        let mut v: Vec<String> = Vec::new();
        for p in self.proofs().into_iter().filter(|p| p.complete && shas.iter().all(|s| p.holds(s))) {
            if !v.contains(&p.build) {
                v.push(p.build);
            }
        }
        v
    }

    /// Every build with at least one recorded proof (for any bytes), ascending.
    pub fn recorded(&self) -> Vec<String> {
        let mut v: Vec<String> = Vec::new();
        for r in &self.rows {
            if !v.contains(&r.build) {
                v.push(r.build.clone());
            }
        }
        v
    }

    /// Proofs that do not add up to their own id (a row lost in a merge): `(build, id)`.
    pub fn incomplete(&self) -> Vec<(String, String)> {
        self.proofs().into_iter().filter(|p| !p.complete).map(|p| (p.build, p.id)).collect()
    }
}

/// What a mod's `[proven] builds` and its proof ledger say about a set of bytes.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Binding {
    /// Builds a recorded proof holds these bytes on.
    pub bound: Vec<String>,
    /// Builds with a recorded proof, but for other bytes.
    pub other_bytes: Vec<String>,
    /// Builds `[proven] builds` lists with no proof recorded at all (a legacy claim).
    pub unrecorded: Vec<String>,
}

fn by_number(v: &mut Vec<String>) {
    v.sort_by_key(|b| b.parse::<u64>().unwrap_or(0));
    v.dedup();
}

impl Binding {
    pub fn of(ledger: &Ledger, claimed: &[String], shas: &[String]) -> Binding {
        let bound = ledger.bound(shas);
        let recorded = ledger.recorded();
        let mut other_bytes: Vec<String> = recorded.iter().filter(|b| !bound.contains(b)).cloned().collect();
        let mut unrecorded: Vec<String> = claimed.iter().filter(|b| !recorded.contains(b)).cloned().collect();
        by_number(&mut other_bytes);
        by_number(&mut unrecorded);
        Binding { bound, other_bytes, unrecorded }
    }

    pub fn holds(&self, build: &str) -> bool {
        self.bound.iter().any(|b| b == build)
    }

    pub fn last(&self) -> Option<&String> {
        self.bound.last()
    }

    /// Builds claimed or recorded that do not count for these bytes, ascending.
    pub fn unbound(&self) -> Vec<String> {
        let mut v: Vec<String> = self.other_bytes.iter().chain(&self.unrecorded).cloned().collect();
        by_number(&mut v);
        v
    }

    /// `6864 (claimed for other bytes or never recorded)`, or the empty string: said wherever the bound builds are, so a
    /// build the summary lists and the bytes do not have is never a surprise.
    pub fn unbound_note(&self) -> String {
        let u = self.unbound();
        if u.is_empty() { String::new() } else { format!("{} (claimed for other bytes or never recorded)", u.join(", ")) }
    }

    /// The bound builds, or `none`.
    pub fn bound_text(&self) -> String {
        if self.bound.is_empty() { "none".into() } else { self.bound.join(", ") }
    }
}

/// Why the files a proof of `build` holds are not `files`: which of `files` it lacks, by owner, and what the proof has
/// for that owner instead. Empty when one proof of the build holds them all, or there is no proof of the build at all.
pub fn differences(ledger: &Ledger, build: &str, files: &[FileId]) -> Vec<String> {
    let proofs: Vec<Proof> = ledger.proofs().into_iter().filter(|p| p.build == build && p.complete).collect();
    if proofs.is_empty() || proofs.iter().any(|p| files.iter().all(|f| p.holds(&f.sha256))) {
        return Vec::new();
    }
    // the proof that holds the most of them is the one to explain against
    let best = proofs.iter().max_by_key(|p| files.iter().filter(|f| p.holds(&f.sha256)).count()).unwrap_or(&proofs[0]);
    let mut out = Vec::new();
    for f in files.iter().filter(|f| !best.holds(&f.sha256)) {
        match best.files.iter().find(|x| x.owner == f.owner) {
            Some(x) => out.push(format!("{} ({}): crc {} is not the bytes proven on {build} (crc {}, {})", f.file, f.owner, f.crc, x.crc, x.file)),
            None => out.push(format!("{} ({}): crc {}: no file of {} was in the proof of {build}", f.file, f.owner, f.crc, f.owner)),
        }
    }
    out
}

// ------------------------------------------------------------------------------------------------ the mod's bytes

/// The files the install ledger holds for `owners`, as installed: each one's bytes read and checked against the crc
/// the ledger has for it. A file the ledger names that is missing or is not the bytes it installed is an error naming
/// it (the bytes in the game are then not the bytes anyone could prove).
pub fn installed(game: &Path, owners: &[String]) -> Result<Vec<FileId>, String> {
    let ip = crate::install::Paths { citadel: game.join("citadel") };
    let mut ledger = crate::install::read_installed(&ip.ledger());
    crate::install::reconcile(&mut ledger, &ip.addons());
    let mut out = Vec::new();
    for r in ledger.iter().filter(|r| owners.contains(&r.owner)) {
        let p = ip.addons().join(crate::build::native(&r.name));
        let bytes = std::fs::read(&p).map_err(|e| format!("{} ({}): the install ledger has it, and it can't be read: {e}", r.name, r.owner))?;
        let crc = format!("{:08x}", vpk::crc32(&bytes));
        if crc != r.crc {
            return Err(format!("{} ({}): on disk crc {crc}, the install ledger says {}: something else replaced it", r.name, r.owner, r.crc));
        }
        out.push(FileId { owner: r.owner.clone(), file: r.name.clone(), crc, sha256: grade::sha256::hex(&bytes) });
    }
    out.sort();
    Ok(out)
}

/// One file named outright (a release copy, a pak beside the mod.toml), under the owner given.
pub fn named(path: &Path, owner: &str) -> Result<FileId, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(FileId {
        owner: owner.to_string(),
        file: path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
        crc: format!("{:08x}", vpk::crc32(&bytes)),
        sha256: grade::sha256::hex(&bytes),
    })
}

/// The bytes a command judges for a mod: its own paks (`[mod] paks`, or the paks beside its `mod.toml`) when it has
/// any, else every file the install ledger holds for its owners (paks and hero-page scenes): what the game loads.
pub fn mod_bytes(ctx: &crate::modctx::ModCtx, game: Option<&Path>) -> Result<Vec<FileId>, String> {
    let own = ctx.own_paks();
    if !own.is_empty() {
        return own.iter().map(|f| named(&f.path, &ctx.owner_for(&f.path, game))).collect();
    }
    match game {
        Some(g) => installed(g, &ctx.owners()),
        None => Ok(Vec::new()),
    }
}

pub fn shas(files: &[FileId]) -> Vec<String> {
    files.iter().map(|f| f.sha256.clone()).collect()
}

// ------------------------------------------------------------------------------------------------ tests

#[cfg(test)]
mod tests {
    use super::*;

    fn f(owner: &str, file: &str, seed: &str) -> FileId {
        let sha = grade::sha256::hex(seed.as_bytes());
        FileId { owner: owner.into(), file: file.into(), crc: format!("{:08x}", vpk::crc32(seed.as_bytes())), sha256: sha }
    }
    fn names(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn a_proof_holds_the_bytes_it_was_recorded_for_and_round_trips() {
        let mut l = Ledger::default();
        let (skin, scene) = (f("x_skin", "pak10_dir.vpk", "b43"), f("x_scene", "maps/ui/hero_prefabs/x.vpk", "scene1"));
        let id = l.add("6866", "2026-11-24", &[skin.clone(), scene.clone()], Some(("D:\\work\\patchday-6866\\x", Some(&"a".repeat(64))))).unwrap();
        assert_eq!(id, proof_id("6866", &[scene.clone(), skin.clone()]), "the id does not depend on the order of the files");
        assert_eq!(l.bound(&shas(&[skin.clone(), scene.clone()])), names(&["6866"]));
        assert_eq!(l.bound(&shas(std::slice::from_ref(&skin))), names(&["6866"]), "a proof holds each of its files");
        assert!(l.bound(&[]).is_empty(), "nothing judged is proven nowhere");
        let text = l.text();
        assert!(text.starts_with("# magician proofs/1") && text.lines().nth(1) == Some(COLUMNS), "{text}");
        let back = Ledger::parse(&text);
        assert!(back.problems.is_empty(), "{:?}", back.problems);
        assert_eq!(back.text(), text, "the canonical form reads back to itself");
        let p = &back.proofs()[0];
        assert!(p.complete && p.evidence.len() == 1 && p.dates.len() == 1 && p.files.len() == 2, "{p:?}");
    }

    #[test]
    fn the_proof_id_is_pinned_for_the_other_readers() {
        // tools/source.py computes the same id (test_source.py pins the same value)
        let id = |o: &str, c: char| FileId { owner: o.into(), file: "f".into(), crc: "00000000".into(), sha256: c.to_string().repeat(64) };
        assert_eq!(proof_id("6866", &[id("voice", 'b'), id("skin", 'a')]), "7b1c92d29e5922d8");
    }

    #[test]
    fn a_rebuilt_pak_is_unproven_whatever_the_summary_claims() {
        let mut l = Ledger::default();
        let old = f("x_skin", "pak10_dir.vpk", "b42");
        l.add("6864", "2026-11-20", std::slice::from_ref(&old), None).unwrap();
        l.add("6865", "2026-11-22", std::slice::from_ref(&old), None).unwrap();
        let new = f("x_skin", "pak10_dir.vpk", "b43");
        let b = Binding::of(&l, &names(&["6864", "6865", "6866"]), &shas(std::slice::from_ref(&new)));
        assert!(b.bound.is_empty() && !b.holds("6866"), "{b:?}");
        assert_eq!(b.other_bytes, names(&["6864", "6865"]));
        assert_eq!(b.unrecorded, names(&["6866"]), "listed in [proven] builds, never recorded");
        assert_eq!(b.unbound_note(), "6864, 6865, 6866 (claimed for other bytes or never recorded)");
        // the old bytes keep their proofs
        assert_eq!(Binding::of(&l, &names(&["6864", "6865", "6866"]), &shas(&[old])).bound, names(&["6864", "6865"]));
    }

    #[test]
    fn a_legacy_list_counts_for_nothing_bound_and_is_said() {
        let l = Ledger::default();
        let b = Binding::of(&l, &names(&["6864", "6861"]), &shas(&[f("o", "pak10_dir.vpk", "x")]));
        assert_eq!((b.bound.len(), b.other_bytes.len(), b.unrecorded.clone()), (0, 0, names(&["6861", "6864"])));
        assert_eq!(b.bound_text(), "none");
        assert!(Binding::of(&l, &[], &[]).unbound_note().is_empty());
    }

    #[test]
    fn a_mod_with_two_paks_where_one_changed_is_unproven_and_the_changed_one_is_named() {
        let mut l = Ledger::default();
        let (skin, voice) = (f("x_skin", "pak12_dir.vpk", "skin1"), f("x_voice", "pak06_dir.vpk", "voice1"));
        l.add("6866", "2026-11-24", &[skin.clone(), voice.clone()], None).unwrap();
        let voice2 = f("x_voice", "pak06_dir.vpk", "voice2");
        assert!(l.bound(&shas(&[skin.clone(), voice2.clone()])).is_empty());
        let d = differences(&l, "6866", &[skin.clone(), voice2.clone()]);
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].starts_with(&format!("pak06_dir.vpk (x_voice): crc {} is not the bytes proven on 6866 (crc {}", voice2.crc, voice.crc)), "{}", d[0]);
        // a file of an owner the proof never had
        let extra = f("x_scene", "maps/ui/hero_prefabs/x.vpk", "s");
        assert!(differences(&l, "6866", &[skin.clone(), voice.clone(), extra])[0].contains("no file of x_scene was in the proof of 6866"));
        // nothing to explain when it holds, or when there is no proof of the build
        assert!(differences(&l, "6866", &[skin.clone(), voice]).is_empty() && differences(&l, "6865", &[skin]).is_empty());
    }

    #[test]
    fn two_machines_ledgers_merge_as_a_union_of_lines() {
        let (skin, scene) = (f("x_skin", "pak10_dir.vpk", "b43"), f("x_scene", "maps/ui/hero_prefabs/x.vpk", "s"));
        let base = {
            let mut l = Ledger::default();
            l.add("6865", "2026-11-22", &[skin.clone(), scene.clone()], None).unwrap();
            l
        };
        let mut a = base.clone();
        a.add("6866", "2026-11-24", &[skin.clone(), scene.clone()], Some(("D:\\a\\session", None))).unwrap();
        let mut b = base.clone();
        b.add("6866", "2026-11-25", &[skin.clone(), scene.clone()], Some(("E:\\b\\session", None))).unwrap();
        b.add("6867", "2026-11-26", &[skin.clone(), scene.clone()], None).unwrap();
        // what a union merge (or a person keeping both sides) leaves: every line of both, in any order, some twice
        let (ta, tb) = (a.text(), b.text());
        let mut lines: Vec<&str> = ta.lines().chain(tb.lines()).collect();
        lines.reverse();
        let merged = Ledger::parse(&lines.join("\n"));
        assert!(merged.problems.is_empty(), "{:?}", merged.problems);
        assert_eq!(merged.bound(&shas(&[skin.clone(), scene.clone()])), names(&["6865", "6866", "6867"]));
        let p6866: Vec<Proof> = merged.proofs().into_iter().filter(|p| p.build == "6866").collect();
        assert_eq!(p6866.len(), 1, "the same bytes on the same build are one proof, from either machine");
        assert_eq!((p6866[0].dates.len(), p6866[0].evidence.len()), (2, 2));
        // the merge written back is the same file whichever side it was made from
        let other = Ledger::parse(&format!("{}{}", b.text(), a.text()));
        assert_eq!(merged.text(), other.text());
    }

    #[test]
    fn a_proof_that_lost_a_row_counts_for_nothing_and_a_bad_line_is_kept_and_refuses_a_rewrite() {
        let (skin, voice) = (f("x_skin", "pak12_dir.vpk", "skin1"), f("x_voice", "pak06_dir.vpk", "voice1"));
        let mut l = Ledger::default();
        l.add("6866", "2026-11-24", &[skin.clone(), voice.clone()], None).unwrap();
        let text = l.text();
        let lost: String = text.lines().filter(|x| !x.contains("x_voice")).map(|x| format!("{x}\n")).collect();
        let damaged = Ledger::parse(&lost);
        assert!(damaged.bound(&shas(std::slice::from_ref(&skin))).is_empty(), "the skin alone was never what was proven");
        assert_eq!(damaged.incomplete().len(), 1);
        let marked = Ledger::parse(&format!("{text}<<<<<<< HEAD\n6866\tshort\n"));
        assert_eq!(marked.problems.len(), 2, "{:?}", marked.problems);
        assert_eq!(marked.bound(&shas(&[skin.clone(), voice.clone()])), names(&["6866"]), "the rows that read still count");
        let mut m = marked.clone();
        assert!(m.add("6867", "2026-11-25", &[skin], None).unwrap_err().contains("do not read"));
    }

    #[test]
    fn a_row_that_is_not_a_proof_is_refused() {
        let mut l = Ledger::default();
        let good = f("o", "pak10_dir.vpk", "x");
        assert!(l.add("68a6", "2026-11-24", std::slice::from_ref(&good), None).is_err());
        assert!(l.add("6866", "24/11/2026", std::slice::from_ref(&good), None).is_err());
        assert!(l.add("6866", "2026-11-24", &[], None).is_err());
        assert!(l.add("6866", "2026-11-24", std::slice::from_ref(&good), Some(("a\tb", None))).is_err());
        let bad = FileId { sha256: "XYZ".into(), ..good.clone() };
        assert!(l.add("6866", "2026-11-24", &[bad], None).is_err());
        assert!(l.proofs().is_empty());
        for line in ["6866\t2026-11-24\tzz\to\tf\t00000000\t" , "x\ty"] {
            assert_eq!(Ledger::parse(line).problems.len(), 1, "{line}");
        }
    }
}

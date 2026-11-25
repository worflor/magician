//! `magician init`: describe a mod in a `mod.toml`, from the pak (or the addon sources) it already has, so every
//! other command can be run from its folder with nothing after the verb.
//!
//! The hero comes from the files the mod replaces (heroes.rs `infer`), the slug and name from the folder, the paks
//! from what was given or what sits in the folder. What was decided, and from what, is printed; nothing is written
//! over an existing `mod.toml` without `--force`.

use super::*;
use crate::heroes::{Heroes, Inference};

/// What goes into a new `mod.toml`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Draft {
    pub slug: String,
    pub name: String,
    pub hero: Option<String>,
    pub model: Option<String>,
    /// Paths as the `mod.toml` writes them (relative to its folder where they are inside it, `/`-separated).
    pub paks: Vec<String>,
    pub addon: Option<String>,
}

/// A slug from a folder name: lowercase ASCII letters and digits, anything else one `-`, none at either end.
pub fn slug_of(name: &str) -> String {
    let mut s = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            s.push(c.to_ascii_lowercase());
        } else if !s.ends_with('-') && !s.is_empty() {
            s.push('-');
        }
    }
    let s = s.trim_end_matches('-').to_string();
    if s.is_empty() { "my-mod".into() } else { s }
}

/// A display name from a folder name: `maid-lash` is `Maid Lash`.
pub fn name_of(folder: &str) -> String {
    let words: Vec<String> = slug_of(folder)
        .split('-')
        .map(|w| {
            let mut c = w.chars();
            c.next().map(|f| f.to_ascii_uppercase().to_string() + c.as_str()).unwrap_or_default()
        })
        .collect();
    words.join(" ")
}

/// A value fit for the line reader of `mod.toml`: no quote, `#` or line break, and trimmed.
pub(crate) fn clean(v: &str) -> String {
    v.chars().filter(|c| !matches!(c, '"' | '#' | '\n' | '\r' | '\0')).collect::<String>().trim().to_string()
}

impl Draft {
    /// The `mod.toml` text, in the shape the reference skins use, every key the readers know commented.
    pub fn render(&self) -> String {
        let q = |v: &str| format!("\"{}\"", clean(v));
        let list = |v: &[String]| v.iter().map(|p| q(p)).collect::<Vec<_>>().join(", ");
        let mut s = String::new();
        s += &format!("# {}: read by Magician. Run `magician check` in this folder to lint, grade and drift-check it.\n\n", clean(&self.name));
        s += "[mod]\n";
        s += &format!("slug = {}\n", q(&self.slug));
        s += &format!("name = {}\n", q(&self.name));
        match &self.hero {
            Some(h) => s += &format!("hero = {}\n", q(h)),
            None => s += "# hero = \"hero_lash\"   # which hero this is for (`magician footprint <name>` checks a name)\n",
        }
        if !self.paks.is_empty() {
            s += &format!("paks = [{}]\n", list(&self.paks));
        }
        match &self.addon {
            Some(a) => s += &format!("addon = {}   # the CSDK addon `magician release` builds\n", q(a)),
            None => s += "# addon = \"my_addon\"   # the CSDK addon `magician release` builds, if this mod has sources\n",
        }
        s += "# owners = [\"my_voice\"]   # other install-ledger names its paks go in under (a voice pak, a scene)\n";
        if let Some(m) = &self.model {
            s += &format!("\n[target]\nmodel = {}\n", q(m));
        }
        s += "\n[proven]\nbuilds = []   # written by `magician prove` once the mod was seen working in game, never by hand\n";
        s += "\n[rights]\nstate = \"unreviewed\"   # unreviewed | cleared | denied: your call on what the mod ships\n";
        s
    }
}

/// `pak` as the `mod.toml` in `dir` names it: relative, with `/`, when it is inside `dir`.
fn rel_to(dir: &Path, pak: &Path) -> String {
    let abs = |p: &Path| std::path::absolute(p).unwrap_or_else(|_| p.to_path_buf());
    let (d, p) = (abs(dir), abs(pak));
    match p.strip_prefix(&d) {
        Ok(r) => r.to_string_lossy().replace('\\', "/"),
        Err(_) => p.to_string_lossy().into_owned(),
    }
}

/// A pak's data chunk (`pak01_000.vpk` beside `pak01_dir.vpk`): part of that pak, never a pak of its own.
fn is_chunk(p: &Path, all: &[PathBuf]) -> bool {
    let n = p.file_name().map(|n| n.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    let Some(stem) = n.strip_suffix(".vpk") else { return false };
    let Some((head, num)) = stem.rsplit_once('_') else { return false };
    num.len() == 3 && num.chars().all(|c| c.is_ascii_digit()) && all.iter().any(|o| o.file_name().is_some_and(|m| m.to_string_lossy().to_ascii_lowercase() == format!("{head}_dir.vpk")))
}

/// Every `.vpk` in `dir` that is a pak (not a chunk of one), sorted.
fn paks_in(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let all: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file() && p.extension().is_some_and(|e| e.eq_ignore_ascii_case("vpk")))
        .collect();
    let mut v: Vec<PathBuf> = all.iter().filter(|p| !is_chunk(p, &all)).cloned().collect();
    v.sort();
    Ok(v)
}

/// Put `from` into `dir` under its own name, and prove the copy is the same bytes. A file of that name already there
/// is kept when it is the same bytes and refused when it is not: init never writes over something it did not make.
fn place(from: &Path, dir: &Path, made: &mut Vec<PathBuf>) -> Result<PathBuf, Stop> {
    let name = from.file_name().ok_or_else(|| format!("{}: not a file", from.display()))?;
    let dest = dir.join(name);
    let sha = |p: &Path| grade::sha256::file(p).map_err(|e| format!("{}: {e}", p.display()));
    if std::path::absolute(&dest).ok() == std::path::absolute(from).ok() {
        return Ok(dest);
    }
    if dest.exists() {
        if sha(&dest)? == sha(from)? {
            return Ok(dest);
        }
        return Err(Stop::Refused(format!("{} exists and is not the same file as {}; nothing written (move it away, or give init another --out)", dest.display(), from.display())));
    }
    std::fs::copy(from, &dest).map_err(|e| format!("{} -> {}: {e}", from.display(), dest.display()))?;
    made.push(dest.clone());
    if sha(&dest)? != sha(from)? {
        return Err(format!("{}: the copy is not the same bytes as {} (a failing disk?)", dest.display(), from.display()).into());
    }
    Ok(dest)
}

/// One pak against another, by path, crc and size: how many of `b`'s files `a` ships too, and how many of those differ.
pub(crate) struct Overlap {
    pub shared: usize,
    pub differ: usize,
    pub of: usize,
}

pub(crate) fn overlap(a: &vpk::Vpk, b: &vpk::Vpk) -> Overlap {
    let theirs: std::collections::HashMap<&str, (u32, u64)> = a.entries.iter().map(|e| (e.path.as_str(), (e.crc, e.size()))).collect();
    let (mut shared, mut differ) = (0, 0);
    for e in &b.entries {
        if let Some(&(crc, len)) = theirs.get(e.path.as_str()) {
            shared += 1;
            if crc != e.crc || len != e.size() {
                differ += 1;
            }
        }
    }
    Overlap { shared, differ, of: b.entries.len() }
}

/// What the paks of one mod are to each other: a pak whose every file another ships byte for byte adds nothing (an
/// older part of the same mod, or the same pak twice), so it is left out; files two paks ship with different bytes
/// are said, since only one of them loads. Returns the paks kept and a line for each thing found.
pub(crate) fn sort_paks(paks: &[PathBuf]) -> Result<(Vec<PathBuf>, Vec<(PathBuf, String)>, Vec<String>), String> {
    let opened: Vec<vpk::Vpk> = paks.iter().map(|p| open(p)).collect::<Result<_, _>>()?;
    let name = |i: usize| paks[i].file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let mut gone: Vec<usize> = Vec::new();
    let mut left_out = Vec::new();
    for j in 0..paks.len() {
        // the largest pak that holds every file of j, byte for byte (the first, when two are the same)
        let holder = (0..paks.len()).filter(|&i| i != j && !gone.contains(&i)).find(|&i| {
            let o = overlap(&opened[i], &opened[j]);
            o.of > 0 && o.shared == o.of && o.differ == 0 && (opened[i].entries.len() > o.of || i < j)
        });
        if let Some(i) = holder {
            gone.push(j);
            left_out.push((paks[j].clone(), format!("all {} of its files are in {}, byte for byte, so it adds nothing", opened[j].entries.len(), name(i))));
        }
    }
    let kept: Vec<usize> = (0..paks.len()).filter(|i| !gone.contains(i)).collect();
    let mut notes = Vec::new();
    for (x, &i) in kept.iter().enumerate() {
        for &j in &kept[x + 1..] {
            let o = overlap(&opened[i], &opened[j]);
            if o.differ > 0 {
                notes.push(format!(
                    "{} and {} both ship {} file(s) with different bytes; in game the lower pak number's copy loads (`magician grade` says which wins)",
                    name(i),
                    name(j),
                    o.differ
                ));
            }
        }
    }
    Ok((kept.iter().map(|&i| paks[i].clone()).collect(), left_out, notes))
}

/// `magician init [<pak>... | <archive>... | <addon dir>] [--out DIR] [--hero HERO] [--slug SLUG] [--name NAME] [--force]`.
pub fn cmd_init(args: &[String]) -> R {
    let pos = positional(args);
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    let force = args.iter().any(|a| a == "--force");
    // What the mod is made of: an addon's source folder, paks and downloads (archives), or the paks in this folder.
    let addon_dir: Option<PathBuf> = match pos.as_slice() {
        [p] if Path::new(p.as_str()).is_dir() => Some(PathBuf::from(p.as_str())),
        _ => None,
    };
    let sources: Vec<PathBuf> = if addon_dir.is_some() {
        Vec::new()
    } else if pos.is_empty() {
        let v = paks_in(&cwd)?;
        if v.is_empty() {
            let downloads: Vec<String> = std::fs::read_dir(&cwd).map_err(|e| e.to_string())?.flatten().map(|e| e.path()).filter(|p| archive::is_archive(p)).filter_map(|p| p.file_name().map(|n| n.to_string_lossy().into_owned())).collect();
            return Err(Stop::Usage(if downloads.is_empty() {
                format!("no pak here to describe ({} has no .vpk): `magician init <pak or archive>` or `magician init <addon folder>`", cwd.display())
            } else {
                format!("no pak here, but {} here: `magician init \"{}\"` takes the paks out of it into a folder of their own", downloads.join(", "), downloads[0])
            }));
        }
        v
    } else {
        for p in &pos {
            if !Path::new(p.as_str()).is_file() {
                return Err(Stop::Usage(format!("{p}: no such file (init takes paks, archives, or one addon folder)")));
            }
        }
        pos.iter().map(PathBuf::from).collect()
    };
    let archives: Vec<&PathBuf> = sources.iter().filter(|p| archive::is_archive(p)).collect();
    // Where the mod's folder is: --out; else, for a download, a new folder beside it named for it; else the pak's own.
    let copying = flag(args, "--out").is_some() || !archives.is_empty();
    let out_dir = flag(args, "--out").map(PathBuf::from).unwrap_or_else(|| match (&addon_dir, archives.first(), sources.first()) {
        (Some(a), _, _) => a.clone(),
        (None, Some(a), _) => {
            let stem = a.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let stem = stem.split('.').next().unwrap_or(&stem).to_string();
            a.parent().filter(|d| !d.as_os_str().is_empty()).map(Path::to_path_buf).unwrap_or_else(|| cwd.clone()).join(slug_of(&stem))
        }
        (None, None, Some(p)) => p.parent().filter(|d| !d.as_os_str().is_empty()).map(Path::to_path_buf).unwrap_or_else(|| cwd.clone()),
        _ => cwd.clone(),
    });
    let out = out_dir.join(modctx::MANIFEST);
    if out.exists() && !force {
        return Err(Stop::Refused(format!("{} already exists; nothing written (--force replaces it)", out.display())));
    }
    // Copy the paks in (each proven the same bytes), or take them out of the download; then sort out how they relate.
    let mut made: Vec<PathBuf> = Vec::new();
    let mut from_lines: Vec<String> = Vec::new();
    let mut origin: std::collections::HashMap<PathBuf, String> = Default::default();
    let mut paks: Vec<PathBuf> = Vec::new();
    if copying {
        std::fs::create_dir_all(&out_dir).map_err(|e| format!("{}: {e}", out_dir.display()))?;
    }
    for s in &sources {
        let label = s.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        if archive::is_archive(s) {
            let stage = out_dir.join(format!(".magician-init-{}", std::process::id()));
            std::fs::create_dir_all(&stage).map_err(|e| format!("{}: {e}", stage.display()))?;
            let r = archive::unpack_vpks(s, &stage).map_err(Stop::from).and_then(|u| {
                let mut placed = Vec::new();
                for v in &u.vpks {
                    placed.push(place(v, &out_dir, &mut made)?);
                }
                Ok((u, placed))
            });
            let _ = std::fs::remove_dir_all(&stage);
            let (u, placed) = r?;
            from_lines.push(format!(
                "{label}: {} pak file(s) taken out{}",
                placed.len(),
                if u.left.is_empty() { String::new() } else { format!("; left in it: {}", u.left.join(", ")) }
            ));
            let chunks = placed.clone();
            for p in placed.into_iter().filter(|p| !is_chunk(p, &chunks)) {
                origin.insert(p.clone(), format!("from {label}"));
                paks.push(p);
            }
        } else if copying {
            let v = open(s)?;
            let mut chunks: Vec<u16> = v.entries.iter().filter(|e| !e.is_inline()).map(|e| e.archive_index).collect();
            chunks.sort();
            chunks.dedup();
            let dest = place(s, &out_dir, &mut made)?;
            for c in chunks {
                place(&v.chunk_path(c), &out_dir, &mut made)?;
            }
            origin.insert(dest.clone(), format!("copied from {}, the same bytes", s.display()));
            paks.push(dest);
        } else {
            paks.push(s.clone());
        }
    }
    if addon_dir.is_none() && paks.is_empty() {
        return Err(Stop::Usage(format!("no pak in {}: nothing to describe", sources.iter().map(|s| s.display().to_string()).collect::<Vec<_>>().join(", "))));
    }
    let (paks, left_out, pak_notes) = sort_paks(&paks)?;
    let mut left_lines = Vec::new();
    for (p, why) in &left_out {
        let n = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        if made.contains(p) {
            std::fs::remove_file(p).map_err(|e| format!("{}: {e}", p.display()))?;
            left_lines.push(format!("{n}: {why} (its copy removed)"));
        } else {
            left_lines.push(format!("{n}: {why} (not listed in paks; the file is untouched)"));
        }
    }
    // The files the mod ships, by their paths in the game.
    let mut files: Vec<String> = Vec::new();
    for p in &paks {
        let v = open(p)?;
        files.extend(v.entries.iter().map(|e| e.path.clone()));
    }
    if let Some(a) = &addon_dir {
        let mut rels = Vec::new();
        build::rel_files(a, a, &mut rels).map_err(|e| format!("{}: {e}", a.display()))?;
        files.extend(rels);
    }
    let folder = std::path::absolute(&out_dir).ok().and_then(|d| d.file_name().map(|n| n.to_string_lossy().into_owned())).unwrap_or_default();
    let mut draft = Draft {
        slug: flag(args, "--slug").map(|s| slug_of(&s)).unwrap_or_else(|| slug_of(&folder)),
        name: flag(args, "--name").map(|n| clean(&n)).unwrap_or_else(|| name_of(&folder)),
        hero: None,
        model: None,
        paks: paks.iter().map(|p| rel_to(&out_dir, p)).collect(),
        addon: addon_dir.as_ref().and_then(|a| a.file_name()).map(|n| n.to_string_lossy().into_owned()),
    };
    // The hero: as given, else from the files.
    let table = game_dir(args).ok().and_then(|g| Heroes::from_game(&g).ok());
    let hero_note;
    match (flag(args, "--hero"), &table) {
        (Some(h), Some(t)) => {
            let hero = t.resolve(&h).map_err(Stop::Usage)?;
            hero_note = format!("{} (--hero)", hero.label());
            draft.hero = Some(hero.key.clone());
            draft.model = Some(hero.model.clone()).filter(|m| files.iter().any(|f| f.trim_end_matches("_c") == m));
        }
        (Some(h), None) => {
            hero_note = format!("{h} (--hero; not checked: the game's hero table could not be read)");
            draft.hero = Some(clean(&h));
        }
        (None, Some(t)) => match t.infer(files.iter().map(String::as_str)) {
            Some(i) if i.clear() => {
                hero_note = describe(&i);
                draft.model = Some(i.hero.model.clone()).filter(|m| files.iter().any(|f| f.trim_end_matches("_c") == m));
                draft.hero = Some(i.hero.key.clone());
            }
            Some(i) => hero_note = format!("not decided: {} (and {}): pass --hero", describe(&i), i.runner_up.map(|(k, _)| k).unwrap_or_default()),
            None => hero_note = format!("none: no file is one of a hero's, so {}; it is linted and checked, not graded (pass --hero only if it is for a hero)", checkcmd::what_it_is(&files)),
        },
        (None, None) => hero_note = "not decided: the game's hero table could not be read (is Deadlock found? `magician doctor`); pass --hero".into(),
    }
    let text = draft.render();
    std::fs::create_dir_all(&out_dir).map_err(|e| format!("{}: {e}", out_dir.display()))?;
    install::write_atomic(&out, text.as_bytes()).map_err(|e| format!("{}: {e}", out.display()))?;
    say!("wrote {}", out.display());
    say!("  slug   {}{}", draft.slug, if flag(args, "--slug").is_some() { " (--slug)" } else { " (from the folder's name)" });
    say!("  hero   {hero_note}");
    for f in &from_lines {
        say!("  from   {f}");
    }
    // Each pak with its fingerprint: the bytes `check` judges and a proof will hold, and the proof of a copy.
    for (p, shown) in paks.iter().zip(&draft.paks) {
        let sha = grade::sha256::file(p).map_err(|e| format!("{}: {e}", p.display()))?;
        let files = open(p).map(|v| v.entries.len()).unwrap_or(0);
        say!("  pak    {shown}  {files} files, sha256 {sha}{}", origin.get(p).map(|o| format!(" ({o})")).unwrap_or_default());
    }
    for l in &left_lines {
        say!("  left out {l}");
    }
    for n in &pak_notes {
        say!("  note   {n}");
    }
    if let Some(a) = &draft.addon {
        say!("  addon  {a} (the folder given)");
    }
    say!("  rights unreviewed (yours to set: [rights] state)");
    say!("\nnext, from {}:", out_dir.display());
    say!("  magician check     lint, grade and drift-check this mod: one verdict");
    say!("  magician install   put it in the game (lints first, refuses while the game runs)");
    Ok(ExitCode::SUCCESS)
}

fn describe(i: &Inference) -> String {
    format!("{}: {} of {} files are its own", i.hero.label(), i.files, i.of)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paks_that_add_nothing_are_left_out_and_clashes_are_said() {
        let d = std::env::temp_dir().join(format!("magician-initpaks-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let f = |p: &str, b: &[u8]| (p.to_string(), b.to_vec());
        let full = d.join("Skin Full.vpk");
        let part = d.join("skin_particles.vpk");
        let clash = d.join("pak02_dir.vpk");
        vpk::write(&full, &[f("particles/a.vpcf_c", b"a"), f("particles/b.vpcf_c", b"b"), f("models/x.vmdl_c", b"x")]).unwrap();
        vpk::write(&part, &[f("particles/a.vpcf_c", b"a"), f("particles/b.vpcf_c", b"b")]).unwrap();
        vpk::write(&clash, &[f("models/x.vmdl_c", b"another x"), f("sounds/y.vsnd_c", b"y")]).unwrap();
        let (kept, left, notes) = sort_paks(&[full.clone(), part.clone(), clash.clone()]).unwrap();
        assert_eq!(kept, vec![full.clone(), clash.clone()]);
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].0, part);
        assert!(left[0].1.starts_with("all 2 of its files are in Skin Full.vpk, byte for byte"), "{left:?}");
        assert_eq!(notes.len(), 1);
        assert!(notes[0].starts_with("Skin Full.vpk and pak02_dir.vpk both ship 1 file(s) with different bytes"), "{notes:?}");
        // the same pak twice: the first is kept
        let twin = d.join("copy.vpk");
        std::fs::copy(&part, &twin).unwrap();
        let (kept, left, _) = sort_paks(&[part.clone(), twin.clone()]).unwrap();
        assert_eq!((kept, left.len()), (vec![part.clone()], 1));
        // a chunk of a pak is part of it, not a pak
        std::fs::write(d.join("pak02_000.vpk"), b"chunk").unwrap();
        let found = paks_in(&d).unwrap();
        assert!(!found.contains(&d.join("pak02_000.vpk")) && found.contains(&clash), "{found:?}");
        // a copy proves itself, and never writes over a different file of that name
        let into = d.join("mod");
        std::fs::create_dir_all(&into).unwrap();
        let mut made = Vec::new();
        assert_eq!(place(&full, &into, &mut made).unwrap(), into.join("Skin Full.vpk"));
        assert_eq!(place(&full, &into, &mut made).unwrap(), into.join("Skin Full.vpk"), "the same bytes again are fine");
        assert_eq!(made, vec![into.join("Skin Full.vpk")]);
        std::fs::write(into.join("copy.vpk"), b"someone else's").unwrap();
        assert!(matches!(place(&twin, &into, &mut made), Err(Stop::Refused(_))));
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn a_new_mod_toml_reads_back_as_it_was_drafted() {
        assert_eq!(slug_of("Maid Lash (v2)!"), "maid-lash-v2");
        assert_eq!(slug_of("___"), "my-mod");
        assert_eq!(name_of("maid-lash"), "Maid Lash");
        let d = Draft { slug: "maid-lash".into(), name: "Maid \"Lash\" #1".into(), hero: Some("hero_lash".into()), model: Some("models/heroes_wip/lash/lash.vmdl".into()), paks: vec!["pak33_dir.vpk".into()], addon: None };
        let t = d.render();
        assert_eq!(grade::manifest::string(&t, "mod", "slug").as_deref(), Some("maid-lash"));
        assert_eq!(grade::manifest::string(&t, "mod", "name").as_deref(), Some("Maid Lash 1"), "quotes and # can't break the line");
        assert_eq!(grade::manifest::string(&t, "mod", "hero").as_deref(), Some("hero_lash"));
        assert_eq!(grade::manifest::list(&t, "mod", "paks"), vec!["pak33_dir.vpk".to_string()]);
        assert_eq!(grade::manifest::string(&t, "target", "model").as_deref(), Some("models/heroes_wip/lash/lash.vmdl"));
        assert!(grade::manifest::proven(&t).is_empty());
        assert_eq!(grade::manifest::string(&t, "rights", "state").as_deref(), Some("unreviewed"));
        // `prove` can write into it
        assert!(patchday::add_proven(&t, "6865").is_ok_and(|n| patchday::proven_builds(&n) == vec!["6865".to_string()]));
        let bare = Draft { hero: None, model: None, paks: vec![], ..d }.render();
        assert_eq!(grade::manifest::string(&bare, "mod", "hero"), None, "an undecided hero is a comment, not a guess");
    }
}

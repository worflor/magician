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

/// `magician init [<pak> | <addon dir>] [--out DIR] [--hero HERO] [--slug SLUG] [--name NAME] [--force]`.
pub fn cmd_init(args: &[String]) -> R {
    let pos = positional(args);
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    // What the mod is made of: a pak, an addon's source folder, or the paks in this folder.
    let (paks, addon_dir): (Vec<PathBuf>, Option<PathBuf>) = match pos.first() {
        Some(p) if Path::new(p.as_str()).is_dir() => (Vec::new(), Some(PathBuf::from(p.as_str()))),
        Some(p) => (vec![PathBuf::from(p.as_str())], None),
        None => {
            let mut v: Vec<PathBuf> = std::fs::read_dir(&cwd)
                .map_err(|e| e.to_string())?
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.is_file() && p.file_name().is_some_and(|n| install::is_pak_name(&n.to_string_lossy().to_ascii_lowercase())))
                .collect();
            v.sort();
            if v.is_empty() {
                return Err(Stop::Usage(format!(
                    "no pak here to describe ({} has no pakNN_dir.vpk): `magician init <pak>` or `magician init <addon folder>`",
                    cwd.display()
                )));
            }
            (v, None)
        }
    };
    let out_dir = flag(args, "--out").map(PathBuf::from).unwrap_or_else(|| match (&addon_dir, paks.first()) {
        (Some(a), _) => a.clone(),
        (None, Some(p)) => p.parent().filter(|d| !d.as_os_str().is_empty()).map(Path::to_path_buf).unwrap_or_else(|| cwd.clone()),
        _ => cwd.clone(),
    });
    let out = out_dir.join(modctx::MANIFEST);
    if out.exists() && !args.iter().any(|a| a == "--force") {
        return Err(Stop::Refused(format!("{} already exists; nothing written (--force replaces it)", out.display())));
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
            None => hero_note = "not decided: no file is one of a hero's; pass --hero if the mod is for one".into(),
        },
        (None, None) => hero_note = "not decided: the game's hero table could not be read (is Deadlock found? `magician doctor`); pass --hero".into(),
    }
    let text = draft.render();
    std::fs::create_dir_all(&out_dir).map_err(|e| format!("{}: {e}", out_dir.display()))?;
    install::write_atomic(&out, text.as_bytes()).map_err(|e| format!("{}: {e}", out.display()))?;
    say!("wrote {}", out.display());
    say!("  slug   {}{}", draft.slug, if flag(args, "--slug").is_some() { " (--slug)" } else { " (from the folder's name)" });
    say!("  hero   {hero_note}");
    for p in &draft.paks {
        say!("  pak    {p}");
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

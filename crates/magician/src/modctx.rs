//! The mod a command is run for when it is not named: the nearest `mod.toml` at or above the working folder, and
//! what it leads to (its hero, its paks, its allowance files, its addon, its scene patches).
//!
//! Run from inside a mod's folder, `magician lint`, `grade`, `drift`, `release`, `install`, `prove`, `footprint` and
//! `scene check` need nothing after the verb: what they would otherwise be told is filled in from here, and only what
//! was not given (an explicit argument or flag always wins). Every answer carries where it came from, and the command
//! says on stderr what it used, so a discovery is never a silent guess; `magician paths` and `magician doctor` show
//! the same answers.
//!
//! Where each answer comes from, first that holds:
//!
//! | What | From |
//! |---|---|
//! | the mod | `--mod` (a `mod.toml` or its folder), else the nearest `mod.toml` at or above the working folder |
//! | its hero | `[mod] hero`, resolved against the game's hero table (heroes.rs) |
//! | its paks | `[mod] paks = ["pak33_dir.vpk"]` (relative to the `mod.toml`), else `pakNN_dir.vpk` files beside it, else the paks the install ledger holds for its owners |
//! | its owners | the slug with `-` as `_`, and `[mod] owners` (patchday.rs) |
//! | its addon | `[mod] addon`, else the CSDK addon named by the slug with `-` as `_` |
//! | its scenes | `[mod] scenes = ["package/scene"]`, else folders holding `scene-patch.txt` at `scene/` or `package/scene/` beside it, or one level inside them |

use crate::install;
use crate::paths::Found;
use std::path::{Path, PathBuf};

pub const MANIFEST: &str = "mod.toml";

/// One mod, read from its `mod.toml`.
#[derive(Debug, Clone)]
pub struct ModCtx {
    pub toml: PathBuf,
    pub dir: PathBuf,
    pub text: String,
    /// How the `mod.toml` was found: `--mod`, or `the nearest mod.toml above <folder>`.
    pub found: String,
}

/// The nearest `mod.toml` at or above `start`.
pub fn find_upward(start: &Path) -> Option<PathBuf> {
    start.ancestors().map(|d| d.join(MANIFEST)).find(|p| p.is_file())
}

impl ModCtx {
    /// The mod at `path`: a `mod.toml`, or a folder holding one.
    pub fn load(path: &Path, found: &str) -> Result<ModCtx, String> {
        let toml = if path.is_dir() { path.join(MANIFEST) } else { path.to_path_buf() };
        let text = std::fs::read_to_string(&toml).map_err(|e| format!("{}: {e}{}", toml.display(), if path.is_dir() { " (a mod's folder holds its mod.toml; `magician init` writes one)" } else { "" }))?;
        let toml = std::path::absolute(&toml).unwrap_or(toml);
        let dir = toml.parent().map(Path::to_path_buf).unwrap_or_default();
        Ok(ModCtx { toml, dir, text, found: found.to_string() })
    }

    /// The mod of the working folder, if there is one.
    pub fn discover() -> Option<ModCtx> {
        let cwd = std::env::current_dir().ok()?;
        let toml = find_upward(&cwd)?;
        let how = if toml.parent() == Some(cwd.as_path()) { "the mod.toml in this folder".to_string() } else { format!("the nearest mod.toml above {}", cwd.display()) };
        ModCtx::load(&toml, &how).ok()
    }

    pub fn slug(&self) -> Option<String> {
        grade::manifest::string(&self.text, "mod", "slug")
    }

    /// The slug, else the folder's name: what a report calls the mod.
    pub fn label(&self) -> String {
        self.slug().unwrap_or_else(|| self.dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "the mod".into()))
    }

    pub fn hero(&self) -> Option<String> {
        grade::manifest::string(&self.text, "mod", "hero")
    }

    /// The install-ledger owners the mod speaks for.
    pub fn owners(&self) -> Vec<String> {
        crate::patchday::mod_owners(&self.text)
    }

    pub fn proven(&self) -> Vec<String> {
        grade::manifest::proven(&self.text)
    }

    /// A path a `mod.toml` names, relative to its folder.
    fn rel(&self, p: &str) -> PathBuf {
        let p = Path::new(p);
        if p.is_absolute() { p.to_path_buf() } else { self.dir.join(crate::build::native(&p.to_string_lossy().replace('\\', "/"))) }
    }

    /// The paks the mod itself names or holds: `[mod] paks`, else the `pakNN_dir.vpk` beside its `mod.toml`. A pak
    /// `[mod] paks` names that is not there is listed all the same (the command reading it says so).
    pub fn own_paks(&self) -> Vec<Found> {
        let listed = grade::manifest::list(&self.text, "mod", "paks");
        if !listed.is_empty() {
            return listed.iter().map(|p| Found { path: self.rel(p), source: "[mod] paks in mod.toml".into() }).collect();
        }
        let mut here: Vec<PathBuf> = std::fs::read_dir(&self.dir)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_file() && p.file_name().is_some_and(|n| install::is_pak_name(&n.to_string_lossy().to_ascii_lowercase())))
            .collect();
        here.sort();
        here.into_iter().map(|path| Found { path, source: "beside the mod.toml".into() }).collect()
    }

    /// The paks to judge: the mod's own, else the ones the install ledger holds for its owners (what the game loads).
    pub fn paks(&self, game: Option<&Path>) -> Vec<Found> {
        let own = self.own_paks();
        if !own.is_empty() {
            return own;
        }
        let Some(game) = game else { return Vec::new() };
        let ip = install::Paths { citadel: game.join("citadel") };
        let mut ledger = install::read_installed(&ip.ledger());
        install::reconcile(&mut ledger, &ip.addons());
        let owners = self.owners();
        ledger
            .iter()
            .filter(|r| owners.contains(&r.owner) && install::is_pak_name(&r.name))
            .map(|r| Found { path: ip.addons().join(&r.name), source: format!("installed, the ledger's {} pak", r.owner) })
            .filter(|f| f.path.is_file())
            .collect()
    }

    /// Install-ledger owners that look like this mod's but that it does not claim: an owner sharing a word of four
    /// letters or more with the slug (`velvet_voice` beside `velvet-skin`). A grade or a drift of the mod leaves
    /// their paks out until `[mod] owners` names them, so a report says so rather than guess they belong.
    pub fn unclaimed_kin(&self, game: Option<&Path>) -> Vec<String> {
        let Some(g) = game else { return Vec::new() };
        // the hero's own name and words every mod uses say nothing about whose a pak is
        let hero = self.hero().unwrap_or_default().to_ascii_lowercase();
        let hero = hero.trim_start_matches("hero_");
        let common = ["skin", "mods", "pack", "voice", "scene", "showcase", "outfit", "model"];
        let words: Vec<String> = self.slug().unwrap_or_default().split(['-', '_']).filter(|w| w.len() >= 4 && *w != hero && !common.contains(w)).map(str::to_string).collect();
        let owners = self.owners();
        let mut out: Vec<String> = install::read_installed(&install::Paths { citadel: g.join("citadel") }.ledger())
            .into_iter()
            .map(|r| r.owner)
            .filter(|o| !owners.contains(o) && o.split(['-', '_']).any(|w| words.iter().any(|x| x == w)))
            .collect();
        out.sort();
        out.dedup();
        out
    }

    /// The ledger owner a pak of this mod installs under: the owner the ledger already has for that slot among the
    /// mod's owners, else the first owner (the slug with `_`), else the pak's folder name as `install` would choose.
    pub fn owner_for(&self, pak: &Path, game: Option<&Path>) -> String {
        let owners = self.owners();
        let name = pak.file_name().map(|n| n.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
        if let Some(g) = game {
            let ledger = install::read_installed(&install::Paths { citadel: g.join("citadel") }.ledger());
            if let Some(r) = ledger.iter().find(|r| r.name == name && owners.contains(&r.owner)) {
                return r.owner.clone();
            }
        }
        owners.first().cloned().unwrap_or_else(|| install::owner_for(pak, None))
    }

    /// The CSDK addon `release` builds: `[mod] addon`, else the slug with `-` as `_` when the CSDK has it.
    pub fn addon(&self, csdk: Option<&Path>) -> Option<Found> {
        if let Some(a) = grade::manifest::string(&self.text, "mod", "addon") {
            return Some(Found { path: PathBuf::from(a), source: "[mod] addon in mod.toml".into() });
        }
        let a = self.slug()?.replace('-', "_");
        let c = csdk?;
        c.join("content").join("citadel_addons").join(&a).is_dir().then(|| Found { path: PathBuf::from(&a), source: format!("the CSDK addon named by the slug, {}", c.display()) })
    }

    /// Other addons `release` folds in: `[mod] with`.
    pub fn with_addons(&self) -> Vec<String> {
        grade::manifest::list(&self.text, "mod", "with")
    }

    /// The mod's scene patches: `[mod] scenes`, else folders with `scene-patch.txt` at `scene/` or `package/scene/`,
    /// or one level inside either (a patch per hero).
    pub fn scenes(&self) -> Vec<Found> {
        let listed = grade::manifest::list(&self.text, "mod", "scenes");
        if !listed.is_empty() {
            return listed.iter().map(|p| Found { path: self.rel(p), source: "[mod] scenes in mod.toml".into() }).collect();
        }
        let mut out = Vec::new();
        for base in [self.dir.join("scene"), self.dir.join("package").join("scene")] {
            if base.join(crate::scene::MANIFEST).is_file() {
                out.push(base.clone());
            } else if let Ok(rd) = std::fs::read_dir(&base) {
                let mut inner: Vec<PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| p.join(crate::scene::MANIFEST).is_file()).collect();
                inner.sort();
                out.extend(inner);
            }
        }
        out.into_iter().map(|path| Found { path, source: "beside the mod.toml".into() }).collect()
    }
}

// ------------------------------------------------------------------------------------- filling in a command line

use crate::cli::Stop;

/// What a command fills in from the mod, said on stderr: `magician:   pak <path> (<where it came from>)`.
fn tell(what: &str, f: &Found) {
    errln!("magician:   {what} {} ({})", f.path.display(), f.source);
}

/// The commands a mod's folder can complete, and the one thing each needs that a bare verb lacks.
const FILLS: &[&str] = &["lint", "grade", "drift", "release", "install", "uninstall", "prove", "footprint", "scene"];

/// Commands whose `--mod` names a `mod.toml` (for the rest it is a ledger owner).
const TOML_MOD: &[&str] = &["grade", "release", "paths", "patchday", "check", "doctor"];

fn has(args: &[String], f: &str) -> bool {
    args.iter().any(|a| a == f)
}

/// The command line(s) to run for `args`: what was given, plus what the mod of the working folder (or `--mod`)
/// supplies for whatever was not given. Several when the mod has several paks to install or lint. A `--mod` that
/// names a mod's folder is read as its `mod.toml`.
pub fn fill(args: &[String]) -> Result<Vec<Vec<String>>, Stop> {
    let Some(cmd) = args.first().map(String::as_str) else { return Ok(vec![args.to_vec()]) };
    let mut a = args.to_vec();
    if TOML_MOD.contains(&cmd) {
        for i in 0..a.len().saturating_sub(1) {
            if a[i] == "--mod" && Path::new(&a[i + 1]).join(MANIFEST).is_file() {
                a[i + 1] = Path::new(&a[i + 1]).join(MANIFEST).to_string_lossy().into_owned();
            }
        }
    }
    if !FILLS.contains(&cmd) || has(&a, "--status") {
        return Ok(vec![a]);
    }
    let pos: Vec<String> = crate::positional(&a[1..]).into_iter().cloned().collect();
    let given_toml = if TOML_MOD.contains(&cmd) { crate::flag(&a, "--mod") } else { None };
    let ctx = match &given_toml {
        Some(t) => Some(ModCtx::load(Path::new(t), "--mod")?),
        None => ModCtx::discover(),
    };
    let needs = match cmd {
        "scene" => matches!(pos.first().map(String::as_str), Some("check" | "install")) && pos.len() == 1,
        "uninstall" => pos.is_empty() && !has(&a, "--all") && !has(&a, "--mod"),
        "grade" => pos.is_empty() || (!has(&a, "--mod") && !has(&a, "--hero")),
        _ => pos.is_empty(),
    };
    if !needs {
        return Ok(vec![a]);
    }
    let Some(ctx) = ctx else {
        if cmd == "grade" && !pos.is_empty() {
            return Ok(vec![a]); // grade says it needs --hero or --mod
        }
        let what = match cmd {
            "release" => "an addon",
            "prove" => "a mod.toml",
            "footprint" => "a hero",
            "scene" => "a scene patch",
            "uninstall" => "a pak, --mod NAME or --all",
            _ => "a pak",
        };
        return Err(Stop::Usage(format!(
            "`magician {cmd}` needs {what}, and there is no mod.toml in this folder or above it to take it from (`magician init <pak>` writes one; `magician {cmd} --help` shows the arguments)"
        )));
    };
    let game = crate::paths::game(crate::flag(&a, "--game")).ok().map(|f| f.path);
    let csdk = crate::paths::csdk(crate::flag(&a, "--csdk")).ok().map(|f| f.path);
    errln!("magician: {}: {} ({})", ctx.label(), ctx.toml.display(), ctx.found);
    let no_pak = |own_only: bool| {
        Stop::Usage(format!(
            "{} names no pak{}: list it under [mod] as paks = [\"pak33_dir.vpk\"], put it beside the mod.toml{}",
            ctx.label(),
            if own_only { " of its own to install" } else { "" },
            if own_only { ", or build one with `magician release`" } else { ", or install it (the ledger's paks for its owners count)" }
        ))
    };
    let s = |p: &Path| p.to_string_lossy().into_owned();
    match cmd {
        "lint" => {
            let paks = ctx.paks(game.as_deref());
            if paks.is_empty() {
                return Err(no_pak(false));
            }
            if paks.len() > 1 && has(&a, "--json") {
                let names: Vec<String> = paks.iter().map(|p| p.path.display().to_string()).collect();
                return Err(Stop::Usage(format!("{} has {} paks and --json is one document per lint: name one ({})", ctx.label(), paks.len(), names.join(", "))));
            }
            let mut extra: Vec<String> = Vec::new();
            if !has(&a, "--allow") && !has(&a, "--addon-root") {
                for f in crate::paths::mod_allowances(&ctx.toml, &ctx.text, csdk.as_deref()).0 {
                    tell("allowances", &f);
                    extra.extend(["--allow".into(), s(&f.path)]);
                }
            }
            Ok(paks
                .iter()
                .map(|p| {
                    tell("pak", p);
                    let mut v = vec![a[0].clone(), s(&p.path)];
                    v.extend(a[1..].iter().cloned());
                    v.extend(extra.iter().cloned());
                    v
                })
                .collect())
        }
        "grade" => {
            if pos.is_empty() {
                let paks = ctx.paks(game.as_deref());
                if paks.is_empty() {
                    return Err(no_pak(false));
                }
                for p in &paks {
                    tell("pak", p);
                    a.push(s(&p.path));
                }
            }
            if !has(&a, "--mod") && !has(&a, "--hero") {
                a.extend(["--mod".into(), s(&ctx.toml)]);
            }
            Ok(vec![a])
        }
        "drift" => {
            let paks = ctx.paks(game.as_deref());
            if paks.is_empty() {
                return Err(no_pak(false));
            }
            for p in &paks {
                tell("pak", p);
                a.push(s(&p.path));
            }
            if !has(&a, "--from") {
                // From the last build these bytes are proven on (the proof ledger), never a build [proven] claims for
                // other bytes; with none, the newest baseline older than the installed build, as patchday does.
                let bytes = crate::proofs::mod_bytes(&ctx, game.as_deref()).map_err(Stop::Usage)?;
                let ledger = crate::proofs::Ledger::for_toml(&ctx.toml).map_err(Stop::Usage)?;
                let b = crate::proofs::Binding::of(&ledger, &ctx.proven(), &crate::proofs::shas(&bytes));
                let installed = game.as_deref().and_then(|g| crate::game_build(g).ok());
                let note = b.unbound_note();
                let from = match (b.last(), &installed) {
                    (Some(last), _) => {
                        errln!("magician:   from build {last} (the last build these paks are proven on, {})", crate::proofs::FILE);
                        last.clone()
                    }
                    (None, Some(i)) => {
                        let older = crate::patchday::default_from(&crate::patchday::builds_on_record(&crate::reference_root()), i);
                        if older == *i {
                            return Err(Stop::Usage(format!("{}: these paks are proven on no build{} and no baseline older than {i} is on record; pass --from BUILD", ctx.label(), if note.is_empty() { String::new() } else { format!(" ({note})") })));
                        }
                        errln!("magician:   from build {older} (these paks are proven on no build{}; the newest baseline older than {i})", if note.is_empty() { String::new() } else { format!(": {note}") });
                        older
                    }
                    (None, None) => return Err(Stop::Usage(format!("{}: these paks are proven on no build and there is no installed game to compare with; pass --from BUILD", ctx.label()))),
                };
                a.extend(["--from".into(), from]);
            }
            Ok(vec![a])
        }
        "release" => {
            let addon = ctx.addon(csdk.as_deref()).ok_or_else(|| {
                Stop::Usage(format!("{} names no addon: add addon = \"<CSDK addon folder>\" under [mod], or pass it (`magician release <addon>`)", ctx.label()))
            })?;
            tell("addon", &addon);
            a.insert(1, addon.path.to_string_lossy().into_owned());
            if !has(&a, "--hero") {
                let hero = ctx.hero().ok_or_else(|| Stop::Usage(format!("{} names no hero: add hero = \"<hero>\" under [mod], or pass --hero", ctx.label())))?;
                errln!("magician:   hero {hero} ([mod] hero)");
                a.extend(["--hero".into(), hero]);
            }
            if !has(&a, "--mod") {
                a.extend(["--mod".into(), s(&ctx.toml)]);
            }
            if !has(&a, "--with") {
                for w in ctx.with_addons() {
                    errln!("magician:   with {w} ([mod] with)");
                    a.extend(["--with".into(), w]);
                }
            }
            Ok(vec![a])
        }
        "install" => {
            let paks = ctx.own_paks();
            if paks.is_empty() {
                return Err(no_pak(true));
            }
            Ok(paks
                .iter()
                .map(|p| {
                    tell("pak", p);
                    let mut v = vec![a[0].clone(), s(&p.path)];
                    v.extend(a[1..].iter().cloned());
                    if !has(&a, "--mod") {
                        let owner = ctx.owner_for(&p.path, game.as_deref());
                        errln!("magician:   for {owner} (its ledger owner)");
                        v.extend(["--mod".into(), owner]);
                    }
                    v
                })
                .collect())
        }
        "uninstall" => {
            let ledger = game.as_deref().map(|g| install::read_installed(&install::Paths { citadel: g.join("citadel") }.ledger())).unwrap_or_default();
            let owners: Vec<String> = ctx.owners().into_iter().filter(|o| ledger.iter().any(|r| &r.owner == o)).collect();
            if owners.is_empty() {
                return Err(Stop::Refused(format!("the install ledger holds nothing for {} (its owners: {}); nothing to uninstall", ctx.label(), ctx.owners().join(", "))));
            }
            Ok(owners
                .into_iter()
                .map(|o| {
                    errln!("magician:   everything the ledger holds for {o}");
                    let mut v = a.clone();
                    v.extend(["--mod".into(), o]);
                    v
                })
                .collect())
        }
        "prove" => {
            a.insert(1, s(&ctx.toml));
            Ok(vec![a])
        }
        "footprint" => {
            let hero = ctx.hero().ok_or_else(|| Stop::Usage(format!("{} names no hero ([mod] hero); pass one", ctx.label())))?;
            errln!("magician:   hero {hero} ([mod] hero)");
            a.insert(1, hero);
            Ok(vec![a])
        }
        _ => {
            // scene check | install
            let scenes = ctx.scenes();
            if scenes.is_empty() {
                return Err(Stop::Usage(format!("{} has no scene patch (scene/ or package/scene/ beside its mod.toml, or [mod] scenes); pass the patch folder", ctx.label())));
            }
            Ok(scenes
                .iter()
                .map(|p| {
                    tell("scene patch", p);
                    let mut v = vec![a[0].clone(), a[1].clone(), s(&p.path)];
                    v.extend(a[2..].iter().cloned());
                    v
                })
                .collect())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("magician-modctx-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn the_mod_is_the_nearest_mod_toml_above_and_its_paks_come_from_it_its_folder_or_the_ledger() {
        let d = scratch("paks");
        let m = d.join("my-skin");
        std::fs::create_dir_all(m.join("source").join("deep")).unwrap();
        std::fs::write(m.join(MANIFEST), "[mod]\nslug = \"my-skin\"\nhero = \"lash\"\nowners = [\"my_voice\"]\n").unwrap();
        assert_eq!(find_upward(&m.join("source").join("deep")), Some(m.join(MANIFEST)));
        assert_eq!(find_upward(&d), None);
        let ctx = ModCtx::load(&m, "test").unwrap();
        assert_eq!((ctx.label(), ctx.hero().as_deref(), ctx.owners()), ("my-skin".to_string(), Some("lash"), vec!["my_skin".to_string(), "my_voice".to_string()]));
        // nothing beside it and no game: no paks
        assert!(ctx.paks(None).is_empty());
        // the installed paks of its owners, and nobody else's
        let game = d.join("game");
        let addons = game.join("citadel").join("addons");
        std::fs::create_dir_all(&addons).unwrap();
        for n in ["pak10_dir.vpk", "pak06_dir.vpk", "pak20_dir.vpk"] {
            std::fs::write(addons.join(n), n).unwrap();
        }
        let crc = |n: &str| format!("{:08x}", vpk::crc32(n.as_bytes()));
        std::fs::write(addons.join(".magician-installed.tsv"), format!("pak10_dir.vpk\t{}\tmy_skin\npak06_dir.vpk\t{}\tmy_voice\npak20_dir.vpk\t{}\tsomeone_else\n", crc("pak10_dir.vpk"), crc("pak06_dir.vpk"), crc("pak20_dir.vpk"))).unwrap();
        let got: Vec<(String, String)> = ctx.paks(Some(&game)).iter().map(|f| (f.path.file_name().unwrap().to_string_lossy().into_owned(), f.source.clone())).collect();
        assert_eq!(got, vec![("pak10_dir.vpk".into(), "installed, the ledger's my_skin pak".into()), ("pak06_dir.vpk".into(), "installed, the ledger's my_voice pak".into())]);
        assert_eq!(ctx.owner_for(Path::new("pak06_dir.vpk"), Some(&game)), "my_voice", "the owner the ledger already has for the slot");
        assert_eq!(ctx.owner_for(Path::new("pak33_dir.vpk"), Some(&game)), "my_skin");
        // owners that look like the mod's and that it does not claim are pointed out, by a word that is not the hero's
        let kin = ModCtx { text: "[mod]\nslug = \"velvet-lash\"\nhero = \"hero_lash\"\n".into(), ..ctx.clone() };
        std::fs::write(addons.join(".magician-installed.tsv"), "pak10_dir.vpk\t0\tvelvet_voice\npak11_dir.vpk\t0\tmaid_lash\npak12_dir.vpk\t0\tvelvet_lash\n").unwrap();
        assert_eq!(kin.unclaimed_kin(Some(&game)), vec!["velvet_voice".to_string()]);
        assert!(kin.unclaimed_kin(None).is_empty());
        // a pak beside the mod.toml wins over the installed ones
        std::fs::write(m.join("pak33_dir.vpk"), "x").unwrap();
        std::fs::write(m.join("notes.vpk"), "x").unwrap();
        let got = ctx.paks(Some(&game));
        assert_eq!((got.len(), got[0].path.clone(), got[0].source.as_str()), (1, m.join("pak33_dir.vpk"), "beside the mod.toml"));
        // and [mod] paks wins over both
        std::fs::write(m.join(MANIFEST), "[mod]\nslug = \"my-skin\"\npaks = [\"build/pak40_dir.vpk\"]\n").unwrap();
        let ctx = ModCtx::load(&m.join(MANIFEST), "test").unwrap();
        assert_eq!(ctx.paks(Some(&game))[0].path, m.join("build").join("pak40_dir.vpk"));
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn its_scenes_and_addon_are_found_beside_it() {
        let d = scratch("scenes");
        std::fs::write(d.join(MANIFEST), "[mod]\nslug = \"maid-lash\"\n").unwrap();
        for p in [d.join("package").join("scene").join("lash"), d.join("package").join("scene").join("abrams")] {
            std::fs::create_dir_all(&p).unwrap();
            std::fs::write(p.join(crate::scene::MANIFEST), "").unwrap();
        }
        let ctx = ModCtx::load(&d, "test").unwrap();
        let s: Vec<PathBuf> = ctx.scenes().into_iter().map(|f| f.path).collect();
        assert_eq!(s, vec![d.join("package").join("scene").join("abrams"), d.join("package").join("scene").join("lash")]);
        let csdk = d.join("csdk");
        assert!(ctx.addon(Some(&csdk)).is_none(), "no such CSDK addon");
        std::fs::create_dir_all(csdk.join("content").join("citadel_addons").join("maid_lash")).unwrap();
        assert_eq!(ctx.addon(Some(&csdk)).map(|f| f.path), Some(PathBuf::from("maid_lash")));
        assert!(ctx.addon(None).is_none());
        std::fs::remove_dir_all(&d).unwrap();
    }
}

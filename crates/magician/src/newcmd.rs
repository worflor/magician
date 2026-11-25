//! `magician new <hero> [--slug NAME] [--dir DIR] [--name NAME] [--author NAME]`: start a skin from nothing.
//!
//! `init` describes a pak that exists; `new` is the step before: from the installed game's own data (the hero's block
//! of `heroes.vdata` and its compiled model, read natively) it scaffolds a mod folder in the shape the reference skins
//! converged on, so a newcomer does not copy someone's folder:
//!
//! ```text
//! <slug>/
//!   mod.toml                      [mod], [target] from the hero's real model and skeleton, [proven], [rights], [ingame]
//!   README.md                     what this is and the next commands
//!   .gitignore                    blends, exports, compiled files, paks, bundles: never in git
//!   addon/                        the CSDK addon (linked into the CSDK's content tree once)
//!     .gitignore                  paks, build stamps, every DMX under models/
//!     magician.allow              per-site allowances, with the format and the rule that a reason is mandatory
//!     <model folder>/.gitignore   Valve's model document with your meshes in it, and meshes on Valve's skeleton
//!     particles/abilities/        Valve's decompiled particles with your edits: everything there is ignored
//!     particles/weapon_fx/        the same
//!     panorama/images/.gitignore  generated PSDs (renders on Valve's body)
//!     sounds/.gitignore           WAV mixes over Valve's decoded audio
//!   package/                      bundle.json, README.template.md and CHANGELOG.md for tools/bundle.py
//!   source/ingame/scenarios/      enter_<hero>.txt and <hero>_angles.txt for the in-game session
//! ```
//!
//! It writes no Valve byte (only paths and names the game data gives), never writes into a folder that exists, and
//! prints what it wrote and what to run next.

use super::*;
use crate::heroes::{Hero, Heroes};
use crate::initcmd::{clean, name_of, slug_of};

/// Everything a scaffold is made from: decided before anything is written.
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    pub slug: String,
    pub name: String,
    pub author: Option<String>,
    pub hero: Hero,
    /// The hero's short name, as console commands and scenario names use it (`lash` for `hero_lash`).
    pub short: String,
    /// The model's skeleton as its compiled document references it (`.vnmskel`), when the game could say.
    pub skeleton: Option<String>,
}

impl Plan {
    pub fn addon(&self) -> String {
        self.slug.replace('-', "_")
    }

    fn model_dir(&self) -> String {
        self.hero.model.rsplit_once('/').map(|(d, _)| d.to_string()).unwrap_or_default()
    }

    fn model_file(&self) -> String {
        self.hero.model.rsplit('/').next().unwrap_or("").to_string()
    }

    /// Every file of the scaffold: (path relative to the mod folder, text).
    pub fn files(&self, csdk: Option<&Path>) -> Vec<(String, String)> {
        let q = |v: &str| format!("\"{}\"", clean(v));
        let (name, hero, short, addon) = (clean(&self.name), clean(&self.hero.name), self.short.clone(), self.addon());
        let mut v: Vec<(String, String)> = Vec::new();

        // mod.toml
        let mut t = format!("# {name}: a {hero} skin, read by Magician. Started with `magician new`; README.md says what to run next.\n\n[mod]\n");
        t += &format!("slug    = {}\n", q(&self.slug));
        t += &format!("name    = {}\n", q(&self.name));
        match &self.author {
            Some(a) => t += &format!("author  = {}   # the maker's mark key (magician sign / verify)\n", q(a)),
            None => t += "author  = \"\"   # TO FILL IN: your name. The maker's mark signs under it and package/bundle.json needs it\n",
        }
        t += &format!("hero    = {}   # heroes.vdata key: {hero}\n", q(&self.hero.key));
        t += "kind    = \"model-replacement\"\n";
        t += &format!("addon   = {}   # the CSDK addon `magician release` builds: this folder's addon\\, linked into the CSDK (README.md)\n", q(&addon));
        t += "# owners = [\"my_voice\"]   # other install-ledger names its paks go in under (a voice pak, a hero-page scene)\n";
        t += "\n[target]\n";
        t += &format!("model    = {}   # from heroes.vdata m_strModelName\n", q(&self.hero.model));
        match &self.skeleton {
            Some(s) => t += &format!("skeleton = {}   # the rig contract, from the compiled model: keep its bone names; never ship it\n", q(s)),
            None => t += "# skeleton = \"\"   # the game's compiled model named no skeleton Magician could read; `magician refs` on it shows them\n",
        }
        t += "\n[proven]\nbuilds = []   # the summary `magician prove` keeps after an in-game pass was looked at; the proofs are in proofs.tsv\n";
        t += "\n[rights]\nstate = \"unreviewed\"   # unreviewed | cleared | denied: your call on what the mod ships\nnotes = \"\"\n";
        t += "\n[fair_play]\nnotes = \"\"   # anything that changes the outline (a skirt, a hat), and why it earns its place\n";
        t += &format!("\n[ingame]\n# the in-game session (ingame/mod_session.py): the game-seat holder, and the scenarios `qa` runs\nholder = {}\npass   = [\"{short}_angles\"]\n", q(&self.slug));
        v.push(("mod.toml".into(), t));

        // README.md
        let csdk_addons = csdk.map(|c| c.join("content").join("citadel_addons").join(&addon).display().to_string()).unwrap_or_else(|| format!("<CSDK>\\content\\citadel_addons\\{addon}"));
        let mut r = format!("# {name}\n\nA {hero} skin, started with `magician new {short}`. Nothing in this folder is Valve's: the scaffold holds only\npaths the game's own data names, and the `.gitignore` files keep anything derived from Valve's files out of git.\n\n");
        r += "## Next\n\nOnce: link `addon\\` into the CSDK's content tree, so its compiler can build it (Windows, `cmd`):\n\n";
        r += &format!("```\nmklink /J \"{csdk_addons}\" \"<this folder>\\addon\"\n```\n\n");
        r += &format!(
            "Put your model, materials and the rest under `addon\\` at the game's own paths (the model at `addon\\{}`). Then, from this\nfolder:\n\n",
            self.hero.model.replace('/', "\\")
        );
        r += "```\nmagician release --out pak10_dir.vpk   build the pak from addon\\, lint and grade it (needs the CSDK)\nmagician check                         lint, grade and drift-check it on the installed build: one verdict\nmagician install                       put it in the game (lints first, never under a running game)\n```\n\n";
        r += &format!(
            "Then the in-game pass (`magician session qa --mod . --out <dir>` runs `{short}_angles` from `source\\ingame\\scenarios\\`),\nlook at the frames, and record it: `magician prove --evidence <dir>`.\n\n"
        );
        r += "## What not to commit\n\nThe skin is built on Valve's mesh, textures and audio. Keep every file derived from them out of git: a model document\nwith your meshes patched in, meshes bound to Valve's skeleton (`*.dmx`), re-dyed textures, renders on Valve's body\n(the hero cards), mixes over Valve's audio (`*.wav`), Valve's decompiled particles with your edits. The nested\n`.gitignore` files already cover the usual places; a new generated file that comes from Valve's goes into the nearest\n`.gitignore` first. Never weaken them to make a file stage. The pak holds only compiled files, and `tools/bundle.py`\nrefuses a file byte-identical to the game's.\n";
        v.push(("README.md".into(), r));

        v.push((
            ".gitignore".into(),
            "# never committed: working files, exports, compiled output, paks and bundles\n__pycache__/\n*.pyc\n*.blend\n*.blend1\n*.fbx\n*.dmx\n*.glb\n*_c\n*.vpk\ndist/\n".into(),
        ));

        // the addon
        v.push(("addon/.gitignore".into(), "# generated or Valve-derived: never committed\n*.vpk\nmagician_build.stamp\nmodels/**/*.dmx\n".into()));
        v.push((
            "addon/magician.allow".into(),
            format!("# {name}: per-site allowances, read by `magician build`, `release` and `check` (and `magician lint --allow`).\n# One line per accepted finding: code <TAB> path <TAB> match <TAB> reason. A reason is mandatory.\n# Rules are never weakened; an allowance is shown in the lint output, and one that matches nothing is reported.\n"),
        ));
        if !self.model_dir().is_empty() {
            v.push((
                format!("addon/{}/.gitignore", self.model_dir()),
                format!(
                    "# Valve-derived: the model document made from {hero}'s with your meshes in it, and meshes bound to Valve's skeleton.\n# Regenerate them from your sources; only the compiled model ships.\n{}\n*.dmx\n",
                    self.model_file()
                ),
            ));
        }
        for d in ["abilities", "weapon_fx"] {
            v.push((format!("addon/particles/{d}/.gitignore"), "# Valve-derived: Valve's decompiled particle texts with your edits. Regenerate them from a script; only the compiled files ship.\n*\n!.gitignore\n".into()));
        }
        v.push(("addon/panorama/images/.gitignore".into(), format!("# generated: the hero cards and portraits are renders of the skin on {hero}'s body. Only the compiled files ship.\n*.psd\n")));
        v.push(("addon/sounds/.gitignore".into(), "# mixes over Valve's decoded audio: only the compiled .vsnd_c ship\n*.wav\n".into()));

        // the package
        let s = jsonout::s;
        let author = self.author.clone().unwrap_or_default();
        let bundle = format!(
            "{{\n  \"name\": {},\n  \"slug\": {},\n  \"author\": {},\n  \"hero\": {},\n  \"category\": \"Skins\",\n  \"description\": {},\n  \"version\": \"b1\",\n  \"release\": \"0.1.0\",\n  \"out\": {},\n  \"readme\": \"README.template.md\",\n  \"changelog\": \"CHANGELOG.md\",\n  \"mods\": [\n    {{\n      \"slug\": {},\n      \"name\": {},\n      \"order\": 0,\n      \"description\": {},\n      \"files\": [{{\"name\": \"pak10_dir.vpk\", \"src\": \"../pak10_dir.vpk\"}}]\n    }}\n  ]\n}}\n",
            s(&self.name),
            s(&self.slug),
            s(&author),
            s(&self.hero.name),
            s(&format!("A {hero} skin. Cosmetic only.")),
            s(&format!("../dist/{}", self.slug)),
            s(&self.slug),
            s(&self.name),
            s(&format!("{hero} skin. Cosmetic only."))
        );
        v.push(("package/bundle.json".into(), bundle));
        v.push((
            "package/README.template.md".into(),
            format!(
                "# {name}\n\nA hero skin for {hero}. (Say what it is, in a few plain sentences.)\n\nIt changes how {hero} looks and sounds. It does not change what {hero} does: no stats, no hitbox tricks, no removed\neffects or audio cues.\n\n## Quick start\n\n1. Close Deadlock.\n2. Either import this folder into Deadlock Mod Manager, or copy `files/0000-{}/pak10_dir.vpk` into\n   `Deadlock/game/citadel/addons/`.\n\n## Not seen yet\n\n(What was not looked at in game.)\n",
                self.slug
            ),
        ));
        v.push(("package/CHANGELOG.md".into(), "# Changelog\n\nNewest first. Each version says which Deadlock build it was tested on.\n\n## 0.1.0\n\nFirst build.\n".into()));

        // the in-game scenarios, adapted from the toolkit's stock-hero examples
        v.push((
            format!("source/ingame/scenarios/enter_{short}.txt"),
            format!("# Enter the Hero Sandbox (new_player_basics) as {hero}. The session driver selects the mod's hero itself; this file\n# is for driving console.py by hand after: launch.ps1 -Extra \"+map new_player_basics\"\n@log ChangeGameState: GameInProgress 300\n@wait 3\nsv_cheats 1\nselecthero {}\n@wait 6\n", self.hero.key),
        ));
        let mut a = format!("# {hero} from the gameplay camera at every pitch players see: from below-behind (looking up, as in a jump) to straight\n# down. Catches holes, seams and layering a level camera hides. Run after entering the sandbox as {hero}.\nsv_cheats 1\ncitadel_hud_visible 0\ncitadel_camera_overrides_enabled 0\nsetpos 1375 -1360 20;setang -45 54 0\n@wait 1\n@shot angles/up45\n");
        for (ang, shot) in [("-25 54 0", "up25"), ("0 54 0", "level"), ("35 54 0", "down35"), ("70 54 0", "down70"), ("89 54 0", "top")] {
            a += &format!("setang {ang}\n@wait 0.8\n@shot angles/{shot}\n");
        }
        a += "setang -45 234 0\n@wait 1\n@shot angles/up45_side\nsetang 30 144 0\n@wait 1\n@shot angles/down30_side\n";
        v.push((format!("source/ingame/scenarios/{short}_angles.txt"), a));
        v
    }
}

/// The skeleton a compiled model references (`m_vecNmSkeletonRefs`), as a source path (`.vnmskel`).
fn skeleton_of(game: &Path, model: &str) -> Option<String> {
    let pak = open(&game.join("citadel").join("pak01_dir.vpk")).ok()?;
    let compiled = patchday::compiled_target(model);
    let e = pak.entries.iter().find(|e| e.path == compiled)?;
    let data = pak.read(e).ok()?;
    let kv = resource::kv3_block(&data, b"DATA")?.ok()?;
    let refs = kv.get("m_vecNmSkeletonRefs")?;
    refs.as_array().iter().find_map(|v| v.as_str().map(|s| s.trim_end_matches("_c").to_string()))
}

pub fn cmd_new(args: &[String]) -> R {
    let usage = "new <hero> [--slug NAME] [--dir DIR] [--name NAME] [--author NAME]";
    let pos = positional(args);
    let arg = pos.first().ok_or(usage)?;
    let game = game_dir(args)?;
    let table = Heroes::from_game(&game).map_err(|e| Stop::Usage(format!("the game's hero table could not be read ({e}); `magician doctor` says how to point Magician at the game")))?;
    let hero = table.resolve(arg).map_err(Stop::Usage)?.clone();
    if hero.model.is_empty() {
        return Err(Stop::Usage(format!("{} names no model in this build's heroes.vdata: there is nothing to make a skin of", hero.label())));
    }
    let short = hero.key.trim_start_matches("hero_").to_string();
    let slug = flag(args, "--slug").map(|s| slug_of(&s)).unwrap_or_else(|| format!("{}-skin", slug_of(&short)));
    let plan = Plan {
        name: flag(args, "--name").map(|n| clean(&n)).filter(|n| !n.is_empty()).unwrap_or_else(|| name_of(&slug)),
        author: flag(args, "--author").map(|a| clean(&a)).filter(|a| !a.is_empty()),
        skeleton: skeleton_of(&game, &hero.model),
        short,
        hero,
        slug,
    };
    let base = flag(args, "--dir").map(PathBuf::from).unwrap_or(std::env::current_dir().map_err(|e| e.to_string())?);
    let root = std::path::absolute(base.join(&plan.slug)).unwrap_or_else(|_| base.join(&plan.slug));
    if root.exists() {
        return Err(Stop::Refused(format!("{} already exists; `magician new` never writes into a folder that is there (pick another --slug or --dir)", root.display())));
    }
    if modctx::find_upward(&base).is_some() {
        return Err(Stop::Refused(format!("{} is inside a mod already ({}); start a new one beside it, not in it", base.display(), modctx::find_upward(&base).map(|p| p.display().to_string()).unwrap_or_default())));
    }
    let csdk = paths::csdk(flag(args, "--csdk")).ok().map(|f| f.path);
    let files = plan.files(csdk.as_deref());
    // Written into a temporary folder beside it and renamed into place: a run that stops leaves nothing half-made.
    let tmp = root.with_file_name(format!("{}.magician-partial", plan.slug));
    let _ = std::fs::remove_dir_all(&tmp);
    let write = || -> std::io::Result<()> {
        for (rel, text) in &files {
            let p = tmp.join(build::native(rel));
            if let Some(d) = p.parent() {
                std::fs::create_dir_all(d)?;
            }
            std::fs::write(&p, text)?;
        }
        std::fs::rename(&tmp, &root)
    };
    if let Err(e) = write() {
        let _ = std::fs::remove_dir_all(&tmp);
        return Err(format!("{}: {e}", root.display()).into());
    }
    say!("new {} skin: {}", plan.hero.name, root.display());
    say!("  hero     {} from this build's heroes.vdata", plan.hero.label());
    say!("  model    {}", plan.hero.model);
    match &plan.skeleton {
        Some(s) => say!("  skeleton {s} (from the compiled model)"),
        None => say!("  skeleton not read from the compiled model; [target] skeleton is left as a comment"),
    }
    say!("  addon    {} (addon\\ in the folder; link it into the CSDK once, README.md)", plan.addon());
    match &plan.author {
        Some(a) => say!("  author   {a}"),
        None => say!("  author   left blank: fill in [mod] author in mod.toml and \"author\" in package/bundle.json"),
    }
    say!("wrote:");
    for (rel, _) in &files {
        say!("  {rel}");
    }
    say!("\nnext, from {}:", root.display());
    say!("  (once) link addon\\ into the CSDK: the mklink line in README.md");
    say!("  magician release --out pak10_dir.vpk   build the first pak");
    say!("  magician check                         one verdict on the installed build");
    say!("  magician install                       put it in the game");
    Ok(ExitCode::SUCCESS)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan(author: Option<&str>) -> Plan {
        Plan {
            slug: "lash-skin".into(),
            name: "Lash Skin".into(),
            author: author.map(String::from),
            hero: Hero { key: "hero_lash".into(), name: "Lash".into(), model: "models/heroes_wip/lash/lash.vmdl".into(), playable: true },
            short: "lash".into(),
            skeleton: Some("models/heroes_wip/lash/lash.vnmskel".into()),
        }
    }

    #[test]
    fn the_scaffold_is_a_mod_every_reader_takes() {
        let files = plan(Some("someone")).files(None);
        let get = |p: &str| files.iter().find(|(r, _)| r == p).map(|(_, t)| t.clone()).unwrap_or_else(|| panic!("no {p}"));
        let t = get("mod.toml");
        assert_eq!(grade::manifest::string(&t, "mod", "slug").as_deref(), Some("lash-skin"));
        assert_eq!(grade::manifest::string(&t, "mod", "hero").as_deref(), Some("hero_lash"));
        assert_eq!(grade::manifest::string(&t, "mod", "addon").as_deref(), Some("lash_skin"));
        assert_eq!(grade::manifest::string(&t, "mod", "author").as_deref(), Some("someone"));
        assert_eq!(patchday::target_paths(&t), vec![("model".to_string(), "models/heroes_wip/lash/lash.vmdl".to_string()), ("skeleton".to_string(), "models/heroes_wip/lash/lash.vnmskel".to_string())]);
        assert!(grade::manifest::proven(&t).is_empty() && patchday::add_proven(&t, "6868").is_ok(), "prove can write into it");
        assert_eq!(grade::manifest::string(&t, "rights", "state").as_deref(), Some("unreviewed"));
        assert_eq!(grade::manifest::list(&t, "ingame", "pass"), vec!["lash_angles".to_string()]);
        // the model's own folder keeps the Valve-derived model document and meshes out of git
        let g = get("addon/models/heroes_wip/lash/.gitignore");
        assert!(g.lines().any(|l| l == "lash.vmdl") && g.lines().any(|l| l == "*.dmx"), "{g}");
        assert!(get("addon/particles/abilities/.gitignore").lines().any(|l| l == "*"));
        assert!(get("source/ingame/scenarios/enter_lash.txt").contains("selecthero hero_lash"));
        assert!(get("source/ingame/scenarios/lash_angles.txt").contains("@shot angles/top"));
        assert!(get("package/bundle.json").contains("\"src\": \"../pak10_dir.vpk\"") && get("package/bundle.json").contains("\"author\": \"someone\""));
        // no author given: a marked blank, never a guess
        let t = plan(None).files(None)[0].1.clone();
        assert!(t.contains("author  = \"\"   # TO FILL IN"), "{t}");
    }
}

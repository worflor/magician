//! `magician corpus`: the rules calibrated on Valve's own content of the installed build (`lint::corpus`). Reads every
//! playable hero's model and every hero's emote voice lines from the game, measures each hero's geometry for the
//! lines M109 and M110 draw (several minutes: the census of every hero), writes `<reference>/<build>/corpus.tsv`, and
//! prints each calibration: how often a rule's own test fires on content that is known to work.

use crate::gamefiles::GameFiles;
use lint::corpus::{self, Corpus};
use lint::mesh::rules;
use lint::Game;
use std::path::Path;
use std::process::ExitCode;

pub fn run(game: &Path, build: &str, out: &Path, baseline: &lint::Baseline) -> Result<ExitCode, String> {
    let heroes = crate::heroes::Heroes::from_game(game)?;
    let gf = GameFiles::open(game, None);
    let mut paths: Vec<String> = heroes.list.iter().filter(|h| h.playable && !h.model.is_empty()).map(|h| format!("{}_c", h.model)).collect();
    paths.sort();
    paths.dedup();
    let models: Vec<(String, Vec<u8>)> = lint::par::map(&paths, |p| gf.read(p).map(|d| (p.clone(), d))).into_iter().flatten().collect();
    let attach: Vec<(String, Vec<String>)> = models
        .iter()
        .filter_map(|(p, d)| lint::deep::contract(d).map(|c| (p.clone(), c.attachments.into_iter().map(|(n, _)| n).collect())))
        .collect();
    let (attachments, census) = corpus::attachment_census(&attach);
    let (cloth_rows, cloth) = corpus::cloth_census(&models);
    let mut voices: Vec<&String> = baseline.files.keys().filter(|p| p.starts_with("sounds/vo/") && p.contains("/emote/") && p.ends_with(".vsnd_c")).collect();
    voices.sort();
    let durations: Vec<(String, f64)> = lint::par::map(&voices, |p| {
        let d = gf.read(p)?;
        let ctrl = resource::kv3_block(&d, b"CTRL")?.ok()?;
        lint::deep::vsnd_measure(&ctrl).0.map(|s| ((*p).clone(), s))
    })
    .into_iter()
    .flatten()
    .collect();
    let voice = corpus::voice_calibration(&durations, lint::policy::FairPlay::default().sound_duration_ratio);
    // M109 and M110: each hero measured whole (the census, about 8 s a hero), its fold and crossing line in its size
    let hint = crate::progress::start(&format!("measuring {} hero models for M109 and M110", models.len()));
    let vm = |p: &str| gf.read(&lint::deep::compiled(p)).and_then(|d| lint::mesh::mat_info(&d));
    let vt = |p: &str| gf.read(p).and_then(|d| lint::deep::tex_info(&d));
    let vf = |p: &str| gf.read(p);
    let env = lint::mesh::Env { material: &vm, texture: &vt, file: &vf };
    let (mut folds, mut crossings) = (Vec::new(), Vec::new());
    for (p, d) in &models {
        if let Ok(c) = lint::mesh::Census::build(d, &env) {
            let size = rules::model_size(&c);
            folds.push((p.clone(), rules::folds(&c, None).iter().map(|x| x.length).sum::<f64>() / size));
            crossings.push((p.clone(), rules::crossings(&c, None).iter().map(|x| x.length).sum::<f64>() / size));
        }
    }
    drop(hint);
    let fold = corpus::line_calibration("M109", "visible fold line in units of the hero's size", &folds, rules::FOLD_RATE_VALVE);
    let cross = corpus::line_calibration("M110", "visible crossing line in units of the hero's size", &crossings, rules::CROSS_RATE_VALVE);
    let c = Corpus { build: build.to_string(), calibrations: vec![cloth, voice, census, fold, cross], attachments, models: models.iter().map(|(p, _)| p.clone()).collect(), cloth: cloth_rows };
    if let Some(dir) = out.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let tmp = out.with_extension("tsv.partial");
    std::fs::write(&tmp, corpus::to_tsv(&c)).and_then(|_| std::fs::rename(&tmp, out)).map_err(|e| format!("{}: {e}", out.display()))?;
    say!("build {build}: {} hero models, {} emote voice lines read from the game", c.models.len(), durations.len());
    for k in &c.calibrations {
        say!("{} {}: fires on {} of {} of Valve's own (worst {:.2}{})", k.rule, k.measure, k.fires, k.of, k.worst, if k.worst_at.is_empty() { String::new() } else { format!(", {}", k.worst_at) });
        say!("     {}", k.reading);
    }
    say!("-> {}", out.display());
    Ok(ExitCode::SUCCESS)
}

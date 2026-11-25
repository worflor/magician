//! `magician corpus`: the rules calibrated on Valve's own content of the installed build (`lint::corpus`). Reads every
//! playable hero's model and every hero's emote voice lines from the game, writes `<reference>/<build>/corpus.tsv`, and
//! prints each calibration: how often a rule's own test fires on content that is known to work.

use crate::gamefiles::GameFiles;
use lint::corpus::{self, Corpus};
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
    let c = Corpus { build: build.to_string(), calibrations: vec![cloth, voice, census], attachments, models: models.iter().map(|(p, _)| p.clone()).collect(), cloth: cloth_rows };
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

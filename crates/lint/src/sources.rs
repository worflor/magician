//! Compiled outputs and the content-tree sources they come from. Shared by `magician build` (what to package)
//! and lint M032 (a referenced mod file whose source is gone), so both answer "is this output still made from
//! something" the same way.
//!
//! Names, as resourcecompiler writes them:
//! - `x.vmat_c` from `x.vmat` (every document type the same way);
//! - `x.vsnd_c` from `x.wav` / `x.mp3`; `x.vsvg_c` from `x.svg`;
//! - `<stem>_<ext>_<hash8>.vtex_c` from an image a material names, `<stem>_<ext>.vtex_c` from an image list;
//! - `<vmat>_vmat_<param>_<hash8>.vtex_c` generated from a material constant;
//! - `a.vnmclip+non_additive.vnmclip_c` (a `+`-joined composite) from `a.vnmclip`;
//! - `*_vmorf.vtex_c`, a morph atlas the model compile generates (judged by its referrer, see `generated_by`).

use std::collections::HashSet;

/// The content-tree source a compiled output came from, if it still exists. `all` holds the tree's files,
/// relative, forward slashes, lowercase.
pub fn source_of(out: &str, all: &HashSet<String>) -> Option<String> {
    let base = out.strip_suffix("_c")?;
    let has = |p: &str| all.contains(&p.to_ascii_lowercase());
    if has(base) {
        return Some(base.to_string());
    }
    if let Some((first, _)) = base.split_once('+') {
        // composite outputs: `a.vnmclip+non_additive.vnmclip_c` is compiled from `a.vnmclip`
        if has(first) {
            return Some(first.to_string());
        }
    }
    let (stem, ext) = base.rsplit_once('.')?;
    match ext {
        "vsnd" => ["wav", "mp3"].iter().map(|e| format!("{stem}.{e}")).find(|p| has(p)),
        "vsvg" => Some(format!("{stem}.svg")).filter(|p| has(p)),
        // Panorama sources keep their web extensions: scripts are .js / .ts, layouts .xml, styles .css
        "vjs" => Some(format!("{stem}.js")).filter(|p| has(p)),
        "vts" => Some(format!("{stem}.ts")).filter(|p| has(p)),
        "vxml" => Some(format!("{stem}.xml")).filter(|p| has(p)),
        "vcss" => Some(format!("{stem}.css")).filter(|p| has(p)),
        "vtex" => {
            let (dir, name) = stem.rsplit_once('/').map(|(d, n)| (format!("{d}/"), n)).unwrap_or((String::new(), stem));
            let parts: Vec<&str> = name.split('_').collect();
            // the hash is printed without leading zeros: 8 hex digits mostly, sometimes 7 (`cannon_wrap_color_png_3dc6515`)
            let hashed = parts.last().is_some_and(|h| (5..=8).contains(&h.len()) && h.chars().all(|c| c.is_ascii_hexdigit()));
            let body = if hashed { &parts[..parts.len() - 1] } else { &parts[..] };
            if let Some(i) = body.iter().position(|p| *p == "vmat") {
                let vmat = format!("{dir}{}.vmat", body[..i].join("_"));
                return Some(vmat).filter(|p| has(p));
            }
            if body.len() >= 2 {
                let e = body[body.len() - 1];
                let s = body[..body.len() - 1].join("_");
                // a texture can be named from any folder (a vmat names its images by path); try its own folder first
                let cand = format!("{dir}{s}.{e}");
                if has(&cand) {
                    return Some(cand);
                }
                let wanted = format!("/{s}.{e}");
                return all.iter().find(|p| p.ends_with(&wanted) || **p == format!("{s}.{e}")).cloned();
            }
            None
        }
        _ => None,
    }
}

/// Outputs a compile generates for its referrer with no source of their own (a model's morph atlas). Such an
/// output is live exactly when the referrer that names it is.
pub fn generated_by(out: &str, referrer: &str) -> bool {
    out.ends_with("_vmorf.vtex_c") && referrer.ends_with(".vmdl_c")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn composites_and_generated_outputs() {
        let all: HashSet<String> = ["models/x/clips/primary_shoot.vnmclip".to_string()].into_iter().collect();
        assert!(source_of("models/x/clips/primary_shoot.vnmclip+non_additive.vnmclip_c", &all).is_some());
        assert!(source_of("models/x/clips/gone.vnmclip+non_additive.vnmclip_c", &all).is_none());
        assert!(generated_by("models/x/x/main_model_vmorf.vtex_c", "models/x/x.vmdl_c"));
        assert!(!generated_by("models/x/x/main_model_vmorf.vtex_c", "particles/x.vpcf_c"));
        // a hash printed without its leading zero (a skin's cannon_wrap)
        let t: HashSet<String> = ["materials/example/fx/cannon_wrap_color.png".to_string()].into_iter().collect();
        assert!(source_of("materials/example/fx/cannon_wrap_color_png_3dc6515.vtex_c", &t).is_some());
        // Panorama scripts compile from .js (resourcecompiler refuses a .vjs source: "Unknown extension")
        let js: HashSet<String> = ["panorama/scripts/cinema/cinema_bridge.js".to_string()].into_iter().collect();
        assert!(source_of("panorama/scripts/cinema/cinema_bridge.vjs_c", &js).is_some());
        assert!(source_of("panorama/scripts/cinema/gone.vjs_c", &js).is_none());
    }
}

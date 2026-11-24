//! `magician sign` and `magician verify`: the maker's mark (crates/mark) on an addon's colour sources and in
//! compiled VPKs, plus the report lines `build` and `release` print.

use mark::addon::{self, Status, TexResult};
use crate::modctx::{self, ModCtx};
use mark::scheme::{self, Key};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

type R = Result<ExitCode, String>;

/// Why there is no key, and what to do. There is no built-in key: a mark signed under someone else's name would
/// claim their work.
pub const NO_KEY: &str = "no maker's mark key: give --key NAME, or name the author in the mod's mod.toml (`key = \"...\"` under [mark], or `author = \"...\"` under [mod])";

/// A key named in a mod.toml: `[mark] key`, else `[mod] author`, with which of them it was.
pub fn key_in_toml(text: &str) -> Option<(String, &'static str)> {
    let nonempty = |s: String| (!s.trim().is_empty()).then(|| s.trim().to_string());
    grade::manifest::string(text, "mark", "key").and_then(nonempty).map(|k| (k, "[mark] key"))
        .or_else(|| grade::manifest::string(text, "mod", "author").and_then(nonempty).map(|k| (k, "[mod] author")))
}

/// The mod a sign or verify is for: the nearest mod.toml at or above `near` (an addon inside a mod's folder, a pak
/// beside its mod.toml), else the working folder's.
fn mod_near(near: Option<&Path>) -> Option<ModCtx> {
    let abs = near.map(|p| std::path::absolute(p).unwrap_or_else(|_| p.to_path_buf()));
    abs.as_deref().and_then(modctx::find_upward).and_then(|t| ModCtx::load(&t, "beside").ok()).or_else(ModCtx::discover)
}

/// The key and where it came from: `--key`, else the mod's mod.toml, else `fallback` (the key the addon's
/// magician.signed manifest says it was signed with), else NO_KEY.
fn mark_key(args: &[String], ctx: Option<&ModCtx>, fallback: Option<(String, String)>) -> Result<(String, String), String> {
    if let Some(k) = flag(args, "--key") {
        return Ok((k, "--key".into()));
    }
    if let Some((k, which)) = ctx.and_then(|c| key_in_toml(&c.text)) {
        return Ok((k, format!("{which} in {}", ctx.map(|c| c.toml.display().to_string()).unwrap_or_default())));
    }
    fallback.ok_or_else(|| NO_KEY.to_string())
}

/// Names a decoded mod id can be matched against: the mod's slug and ledger owners, and any given.
fn known_mods(ctx: Option<&ModCtx>, extra: &[String]) -> Vec<String> {
    let mut v: Vec<String> = extra.to_vec();
    if let Some(c) = ctx {
        v.extend(c.slug().map(|s| s.replace('-', "_")));
        v.extend(c.owners());
    }
    v
}

fn flag(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1).cloned())
}
fn flags(args: &[String], name: &str) -> Vec<String> {
    args.windows(2).filter(|w| w[0] == name).map(|w| w[1].clone()).collect()
}
fn positional(args: &[String]) -> Vec<&String> {
    let mut out = Vec::new();
    let mut skip = false;
    for a in args {
        if skip {
            skip = false;
            continue;
        }
        if a.starts_with("--") {
            // boolean flags take no value
            skip = !matches!(a.as_str(), "--dry-run" | "--force" | "--summary" | "--all");
            continue;
        }
        out.push(a);
    }
    out
}

/// The private secondary key, if the environment holds one. Never printed, never stored.
fn secret_key() -> Option<Key> {
    std::env::var(mark::SECRET_ENV).ok().filter(|s| !s.is_empty()).map(|s| Key::new(&s))
}

fn mod_name_of(id: u16, known: &[String]) -> String {
    known.iter().find(|n| scheme::mod_id(n) == id).cloned().unwrap_or_else(|| "unknown mod".into())
}

/// "p < 10^-123" for a log10 bound.
pub fn p_text(log10_p: f64) -> String {
    if log10_p > -3.0 {
        format!("p < {:.3}", 10f64.powf(log10_p).max(1e-3))
    } else {
        format!("p < 1e{}", log10_p.ceil() as i64)
    }
}

/// An addon given by CSDK name or by path; the mod name is the addon folder's name (a repo `addon` folder
/// takes its mod folder's name).
fn resolve_addon(arg: &str, args: &[String]) -> Result<(PathBuf, String), String> {
    let p = Path::new(arg);
    if p.is_dir() {
        let name = p.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let name = if name == "addon" { p.parent().and_then(|q| q.file_name()).map(|s| s.to_string_lossy().replace('-', "_")).unwrap_or(name) } else { name };
        return Ok((p.to_path_buf(), name));
    }
    crate::paths::addon_name(arg)?;
    let csdk = crate::paths::csdk(flag(args, "--csdk"))?.path;
    let dir = csdk.join("content").join("citadel_addons").join(arg);
    if !dir.is_dir() {
        return Err(format!("no addon sources at {}", dir.display()));
    }
    Ok((dir, arg.to_string()))
}

pub fn cmd_sign(args: &[String]) -> R {
    let usage = "sign <addon|dir> [--mod NAME] [--date YYYY-MM-DD] [--key NAME] [--dry-run] [--force]";
    let p = positional(args);
    let (root, name) = resolve_addon(p.first().ok_or(usage)?, args)?;
    let ctx = mod_near(Some(&root));
    let signed_with = addon::manifest_key(&root).map(|(k, _)| (k, format!("the key {} was signed with", root.join(addon::MANIFEST).display())));
    let (key_name, from) = mark_key(args, ctx.as_ref(), signed_with)?;
    errln!("magician:   mark key {key_name} ({from})");
    let day = match flag(args, "--date") {
        Some(d) => scheme::parse_date(&d).ok_or(format!("--date {d}: want YYYY-MM-DD from 2024-01-01"))?,
        None => scheme::today(),
    };
    let secret = secret_key();
    let o = addon::SignOptions {
        key: Key::new(&key_name),
        secret,
        mod_name: flag(args, "--mod").unwrap_or(name),
        day,
        dry_run: args.iter().any(|a| a == "--dry-run"),
        force: args.iter().any(|a| a == "--force"),
        target_rms: scheme::TARGET_RMS,
    };
    outln!(
        "sign {}: key {key_name}, mod {} ({:04x}), date {}{}{}",
        root.display(),
        o.mod_name,
        scheme::mod_id(&o.mod_name),
        scheme::date_text(o.day),
        if secret.is_some() { format!(", secondary key from {}", mark::SECRET_ENV) } else { String::new() },
        if o.dry_run { " (dry run: nothing written)" } else { "" }
    );
    outln!("{:<8} {:>9} {:>6} {:>6} {:>6} {:>6} {:>6} {:>6} {:>6}  source", "status", "size", "cells", "z", "z_bc1", "z_bc7", "PSNR", "dE", "dEmip");
    let started = std::time::Instant::now();
    let entries = addon::sign_addon(&root, &o, |e| {
        if e.status == Status::Skipped {
            outln!("{:<8} {:>9} {:>6} {:>6} {:>6} {:>6} {:>6} {:>6} {:>6}  {}: {}", "skipped", if e.size.0 > 0 { format!("{}x{}", e.size.0, e.size.1) } else { "-".into() }, if e.cells > 0 { e.cells.to_string() } else { "-".into() }, "-", "-", "-", "-", "-", "-", e.path, e.note);
        } else if e.status == Status::Already {
            outln!("{:<8} {:>9} {:>6} {:>6.1} {:>6} {:>6} {:>6} {:>6} {:>6}  {} ({})", "already", format!("{}x{}", e.size.0, e.size.1), e.cells, e.z, "-", "-", "-", "-", "-", e.path, e.note);
        } else {
            outln!(
                "{:<8} {:>9} {:>6} {:>6.1} {:>6.1} {:>6.1} {:>6.1} {:>6.3} {:>6.3}  {}{}",
                "signed",
                format!("{}x{}", e.size.0, e.size.1),
                e.cells,
                e.z,
                e.z_bc1,
                e.z_bc7,
                e.psnr,
                e.de_mean,
                e.de_mips,
                e.path,
                if e.note.is_empty() { String::new() } else { format!(" ({})", e.note) }
            );
        }
    })?;
    let n = |s: Status| entries.iter().filter(|e| e.status == s).count();
    let signed: Vec<&addon::Entry> = entries.iter().filter(|e| e.status == Status::Signed).collect();
    let worst_psnr = signed.iter().map(|e| e.psnr).fold(f64::INFINITY, f64::min);
    let worst_de = signed.iter().map(|e| e.de_mean).fold(0.0, f64::max);
    outln!(
        "{} colour source(s): {} signed now, {} already signed, {} skipped ({:.1}s){}",
        entries.len(),
        n(Status::Signed),
        n(Status::Already),
        n(Status::Skipped),
        started.elapsed().as_secs_f64(),
        if signed.is_empty() { String::new() } else { format!("; worst PSNR {worst_psnr:.1} dB, worst mean dE2000 {worst_de:.3}") }
    );
    if !o.dry_run {
        let m = addon::write_manifest(&root, &entries, &o, &key_name)?;
        outln!("manifest -> {}", m.display());
    }
    Ok(ExitCode::SUCCESS)
}

struct Verified {
    results: Vec<TexResult>,
    pool: scheme::Pool,
}

/// Decode and detect over many compiled textures on every core; the pool takes each texture's
/// canonical-grid weights as they arrive, so memory stays flat over a whole game pack.
fn verify_many(items: impl Iterator<Item = (String, Vec<u8>)>, key: Key, secret: Option<Key>, keep: impl Fn(&str) -> bool + Sync) -> Verified {
    let workers = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).clamp(1, 16);
    let (tx, rx) = std::sync::mpsc::sync_channel::<(String, Vec<u8>)>(workers * 2);
    let rx = std::sync::Arc::new(std::sync::Mutex::new(rx));
    let (rtx, rrx) = std::sync::mpsc::channel::<TexResult>();
    let keep = &keep;
    let (mut results, pool) = std::thread::scope(|s| {
        for _ in 0..workers {
            let rx = rx.clone();
            let rtx = rtx.clone();
            s.spawn(move || loop {
                let job = rx.lock().unwrap().recv();
                let Ok((path, data)) = job else { break };
                if let Some(r) = addon::verify_vtex(&path, &data, key, secret) {
                    let _ = rtx.send(r);
                }
            });
        }
        drop(rtx);
        let collector = s.spawn(move || {
            let mut results = Vec::new();
            let mut pool = scheme::Pool::default();
            for mut r in rrx {
                if let Some(w) = r.found.as_mut().and_then(|f| f.canonical.take()) {
                    if keep(&r.path) {
                        pool.add(&w);
                    }
                }
                results.push(r);
            }
            (results, pool)
        });
        // the reader stays on this thread (the VPK's file handles are not shareable) and in archive order
        for it in items {
            if tx.send(it).is_err() {
                break;
            }
        }
        drop(tx);
        collector.join().expect("collector")
    });
    results.sort_by(|a, b| a.path.cmp(&b.path));
    Verified { results, pool }
}

fn payload_text(d: &scheme::Decoded, extra: &[String]) -> String {
    format!(
        "mod {} ({:04x}, z {:.1}), signed {} (z {:.1}); wrong-decode {}",
        mod_name_of(d.mod_id.value, extra),
        d.mod_id.value,
        d.mod_id.z,
        scheme::date_text(d.day.value),
        d.day.z,
        p_text(d.log10_p())
    )
}

/// Compare alpha and colour of the same texture in two compiled forms (signed vs unsigned sources).
fn against(a: &[u8], b: &[u8]) -> Option<String> {
    let (ta, tb) = (resource::texture::info(a)?, resource::texture::info(b)?);
    if (ta.width, ta.height) != (tb.width, tb.height) {
        return Some(format!("size differs ({}x{} vs {}x{})", ta.width, ta.height, tb.width, tb.height));
    }
    let mips = ta.mips.min(tb.mips).max(1) as u32;
    let (mut bits, mut cov, mut amax) = (0usize, 0usize, 0u8);
    let mut top = None;
    let mut per_mip: Vec<String> = Vec::new();
    for m in 0..mips {
        let before = cov;
        let (Some((w, h, pa)), Some((_, _, pb))) = (addon::decode_mip(a, m), addon::decode_mip(b, m)) else { break };
        for (x, y) in pa.chunks(4).zip(pb.chunks(4)) {
            bits += (x[3] != y[3]) as usize;
            amax = amax.max(x[3].abs_diff(y[3]));
            cov += ((x[3] >= 128) != (y[3] >= 128)) as usize;
        }
        if cov > before {
            per_mip.push(format!("mip {m} ({}x{}): {}", w, h, cov - before));
        }
        if m == 0 {
            top = Some(mark::metrics::diff(&mark::image::Image::new(w, h, pb), &mark::image::Image::new(w, h, pa)));
        }
    }
    let d = top?;
    if addon::is_ycocg(a) {
        return Some(format!("vs the unsigned build (both encoded; block choices differ): PSNR {:.1} dB, mean dE2000 {:.3}; stored YCoCg (alpha holds luma, no coverage)", d.psnr, d.de_mean));
    }
    Some(format!(
        "vs the unsigned build (both encoded; block choices differ): PSNR {:.1} dB, mean dE2000 {:.3}; alpha {} over {mips} mip(s), coverage at 0.5 {}",
        d.psnr,
        d.de_mean,
        if bits == 0 { "bit-identical".to_string() } else { format!("{bits} texel(s) differ by at most {amax}") },
        if cov == 0 { "identical".to_string() } else { format!("{cov} texel(s) flip [{}]", per_mip.join(", ")) }
    ))
}

fn strip_hash(p: &str) -> String {
    let base = p.strip_suffix(".vtex_c").unwrap_or(p);
    match base.rsplit_once('_') {
        Some((stem, h)) if h.len() == 8 && h.chars().all(|c| c.is_ascii_hexdigit()) => stem.to_string(),
        _ => base.to_string(),
    }
}

pub fn cmd_verify(args: &[String]) -> R {
    let usage = "verify <vpk> [--key NAME] [--prefix P] [--sample N] [--summary] [--against <unsigned.vpk>] [--mod NAME]...";
    let p = positional(args);
    let vpath = PathBuf::from(p.first().ok_or(usage)?.as_str());
    let v = vpk::Vpk::open(&vpath).map_err(|e| format!("{}: {e}", vpath.display()))?;
    let ctx = mod_near(vpath.parent());
    let (key_name, from) = mark_key(args, ctx.as_ref(), None)?;
    errln!("magician:   mark key {key_name} ({from})");
    let key = Key::new(&key_name);
    let secret = secret_key();
    let prefix = flag(args, "--prefix").unwrap_or_default();
    let sample: usize = flag(args, "--sample").and_then(|s| s.parse().ok()).unwrap_or(0);
    let summary = args.iter().any(|a| a == "--summary");
    let extra = known_mods(ctx.as_ref(), &flags(args, "--mod"));
    let mut todo: Vec<&vpk::Entry> = v.entries.iter().filter(|e| e.path.starts_with(&prefix) && e.path.ends_with(".vtex_c")).collect();
    let total = todo.len();
    if sample > 0 && sample < todo.len() {
        // a deterministic, evenly spread sample by path order
        todo.sort_by(|a, b| a.path.cmp(&b.path));
        let step = todo.len() as f64 / sample as f64;
        todo = (0..sample).map(|i| todo[(i as f64 * step) as usize]).collect();
    }
    todo.sort_by_key(|e| (e.archive_index, e.offset));
    outln!(
        "verify {}: key {key_name}{}, {} of {total} texture(s){}",
        vpath.display(),
        if secret.is_some() { format!(" + secondary key from {}", mark::SECRET_ENV) } else { String::new() },
        todo.len(),
        if prefix.is_empty() { String::new() } else { format!(" under {prefix}") }
    );
    let started = std::time::Instant::now();
    let vref = &v;
    let items = todo.into_iter().filter_map(move |e| vref.read(e).ok().map(|d| (e.path.clone(), d)));
    let res = verify_many(items, key, secret, |_| true);
    let other = match flag(args, "--against") {
        Some(a) => Some(vpk::Vpk::open(&a).map_err(|e| format!("{a}: {e}"))?),
        None => None,
    };
    let other_by_stem: BTreeMap<String, &vpk::Entry> = other.as_ref().map(|o| o.entries.iter().filter(|e| e.path.ends_with(".vtex_c")).map(|e| (strip_hash(&e.path), e)).collect()).unwrap_or_default();
    let mut zs = Vec::new();
    let (mut signed, mut decoded) = (0usize, 0usize);
    if !summary {
        outln!("{:>7} {:>9} {:<9} {:>5} {:>7} {:<15}  texture", "z", "size", "format", "scale", "p", "payload");
    }
    for r in &res.results {
        let Some(f) = &r.found else {
            if !summary {
                outln!("{:>7} {:>9} {:<9} {:>5} {:>7} {:<15}  {} ({})", "-", format!("{}x{}", r.w, r.h), r.format, "-", "-", "-", r.path, r.note.as_deref().unwrap_or(""));
            }
            continue;
        };
        decoded += 1;
        zs.push(f.z);
        if r.signed() {
            signed += 1;
        }
        if !summary {
            let pay = f.payload.as_ref().map(|d| format!("{} {}", mod_name_of(d.mod_id.value, &extra), scheme::date_text(d.day.value))).unwrap_or_else(|| "-".into());
            let sec = f.secret_z.map(|s| format!(", secondary z {s:.1}")).unwrap_or_default();
            let scale = match f.step {
                0 => "1",
                1 => "1/2",
                2 => "1/4",
                _ => "2",
            };
            outln!("{:>7.1} {:>9} {:<9} {:>5} {:>7} {:<15}  {}{}", f.z, format!("{}x{}", r.w, r.h), r.format, scale, if f.log10_p <= -3.0 { format!("1e{}", f.log10_p.ceil() as i64) } else { format!("{:.3}", 10f64.powf(f.log10_p)) }, pay, r.path, sec);
            if let (Some(o), Some(e)) = (&other, other.as_ref().and_then(|o| o.find(&r.path)).or_else(|| other_by_stem.get(&strip_hash(&r.path)).copied())) {
                if let (Ok(a), Ok(b)) = (v.read(v.find(&r.path).unwrap()), o.read(e)) {
                    if let Some(t) = against(&a, &b) {
                        outln!("{:>48}  {t}", "");
                    }
                }
            }
        }
    }
    outln!(
        "{} texture(s) decoded in {:.1}s, {} carry the mark (p < 1e-9 each); {} not decodable or not colour",
        decoded,
        started.elapsed().as_secs_f64(),
        signed,
        res.results.len() - decoded
    );
    if !zs.is_empty() {
        let n = zs.len() as f64;
        let max = zs.iter().cloned().fold(f64::MIN, f64::max);
        let mean = zs.iter().sum::<f64>() / n;
        let sd = (zs.iter().map(|z| (z - mean).powi(2)).sum::<f64>() / n).sqrt();
        outln!("z over textures: mean {mean:.2}, sd {sd:.2}, max {max:.2}");
        for t in [3.0f64, 4.0, 5.0, 6.0] {
            let k = zs.iter().filter(|&&z| z >= t).count();
            let bound = (8.0 * (-t * t / 2.0).exp()).min(1.0);
            outln!("  z >= {t}: {k} ({:.4}%; the Hoeffding bound over at most 8 tries allows {:.4}%)", 100.0 * k as f64 / n, 100.0 * bound);
        }
    }
    let (zp, lp) = res.pool.presence(key);
    outln!("combined over {} texture(s) on their marked grid: z {zp:.1}, {}", res.pool.textures, p_text(lp));
    if let Some(s) = secret {
        let (zs2, lp2) = res.pool.presence(s);
        outln!("combined, secondary key: z {zs2:.1}, {}", p_text(lp2));
    }
    if zp >= scheme::DECODE_Z {
        if let Some(d) = res.pool.payload(key) {
            outln!("payload (pooled): {}", payload_text(&d, &extra));
        }
    }
    Ok(ExitCode::SUCCESS)
}

/// Lines for `build` and `release`: are the sources still signed, and does the package carry the mark?
/// `files` are the packaged compiled files; `roots` the addons' content folders (with their manifests).
pub fn mark_report(files: &[(String, Vec<u8>)], roots: &[PathBuf]) -> Vec<String> {
    let mut lines = Vec::new();
    // colour sources by lowercase path; a .vtex maps to its input; manifest status per source
    let mut sources: BTreeMap<String, String> = BTreeMap::new();
    let mut status: BTreeMap<String, String> = BTreeMap::new();
    let mut changed = Vec::new();
    let mut manifests = 0;
    for root in roots {
        let Ok(found) = addon::discover(root) else { continue };
        for s in &found {
            sources.insert(s.path.to_ascii_lowercase(), s.path.clone());
            if let addon::Role::Vtex(v) = &s.role {
                if !v.is_empty() && s.skip.is_none() {
                    sources.insert(v.to_ascii_lowercase(), s.path.clone());
                }
            }
            if let Some(why) = &s.skip {
                status.insert(s.path.clone(), format!("skipped: {why}"));
            }
        }
        let c = addon::check_sources(root);
        if c.manifest {
            manifests += 1;
            for l in addon::read_manifest(root).unwrap_or_default() {
                status.insert(l.path.clone(), if l.status == "skipped" { format!("skipped: {}", l.note) } else { l.status.clone() });
            }
            for p in &c.changed {
                status.insert(p.clone(), "changed since signing".into());
            }
            changed.extend(c.changed.iter().map(|p| format!("{p} ({})", root.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default())));
            for p in &c.new {
                status.entry(p.clone()).or_insert_with(|| "new since signing".into());
            }
        }
    }
    // the key the sources were signed with (their manifest says), else the mod's own; with neither, nothing to check
    let signed_with = roots.iter().find_map(|r| addon::manifest_key(r));
    let ctx = mod_near(roots.first().map(PathBuf::as_path));
    let key_name = match signed_with.as_ref().map(|(k, _)| k.clone()).or_else(|| ctx.as_ref().and_then(|c| key_in_toml(&c.text)).map(|(k, _)| k)) {
        Some(k) => k,
        None => {
            lines.push(format!("signed: not checked ({NO_KEY}; then `magician sign <addon>` after the generators)"));
            return lines;
        }
    };
    let mut names = known_mods(ctx.as_ref(), &[]);
    names.extend(signed_with.map(|(_, m)| m));
    let key = Key::new(&key_name);
    let colour: Vec<(String, Vec<u8>)> = files.iter().filter(|(p, _)| p.ends_with(".vtex_c") && addon::source_for_output(p, &sources).is_some()).cloned().collect();
    let src_of: BTreeMap<String, String> = colour.iter().map(|(p, _)| (p.clone(), addon::source_for_output(p, &sources).unwrap())).collect();
    // pool the textures whose source the manifest calls signed: chosen by the manifest, not by correlation
    let signed_src = |p: &str| src_of.get(p).and_then(|s| status.get(s)).is_some_and(|st| st == "signed" || st == "already");
    let res = verify_many(colour.into_iter(), key, secret_key(), |p| signed_src(p));
    let m = res.results.len();
    let n = res.results.iter().filter(|r| r.signed()).count();
    let (zp, lp) = res.pool.presence(key);
    lines.push(format!(
        "signed: {n}/{m} colour textures, combined {} (z {zp:.1} over {} signed texture(s)){}",
        p_text(lp),
        res.pool.textures,
        if manifests == 0 { "; no magician.signed manifest: run `magician sign <addon>` after the generators" } else { "" }
    ));
    if zp >= scheme::DECODE_Z {
        if let Some(d) = res.pool.payload(key) {
            lines.push(format!("  payload: {}", payload_text(&d, &names)));
        }
    }
    if !changed.is_empty() {
        lines.push(format!("  {} source(s) changed since signing (a generator re-ran): run `magician sign` again: {}", changed.len(), changed.join(", ")));
    }
    let mut why: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for r in &res.results {
        if r.signed() {
            continue;
        }
        let s = src_of.get(&r.path).cloned().unwrap_or_default();
        let st = status.get(&s).cloned().unwrap_or_else(|| "not in the manifest".into());
        let reason = if st == "signed" || st == "already" {
            "MISSING: the source is signed but the compiled texture does not carry the mark".to_string()
        } else if st.contains("too small") {
            "skipped: too small, or too little opaque area clear of alpha edges".to_string()
        } else if st.contains("sprite-sheet") {
            "skipped: sprite-sheet frames".to_string()
        } else if st.contains("light cookie") {
            "skipped: light cookie".to_string()
        } else if st.contains("also used as") {
            "skipped: also a non-colour input".to_string()
        } else {
            st
        };
        let z = r.found.as_ref().map_or(String::new(), |f| format!(" z {:.1}", f.z));
        why.entry(reason).or_default().push(format!("{}{z}", r.path.rsplit('/').next().unwrap_or(&r.path)));
    }
    for (reason, list) in why {
        let shown: Vec<&str> = list.iter().take(6).map(String::as_str).collect();
        lines.push(format!("  unsigned {} ({}): {}{}", list.len(), reason, shown.join(", "), if list.len() > 6 { ", ..." } else { "" }));
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_is_the_mods_own_and_the_given_one_wins() {
        assert_eq!(key_in_toml("[mod]\nauthor = \"Jane\"\n"), Some(("Jane".to_string(), "[mod] author")));
        assert_eq!(key_in_toml("[mod]\nauthor = \"Jane\"\n[mark]\nkey = \"jx\"\n"), Some(("jx".to_string(), "[mark] key")));
        assert_eq!(key_in_toml("[mod]\nauthor = \"  \"\n[mark]\nkey = \"\"\n"), None, "blank is no key");
        assert_eq!(key_in_toml("[mod]\nslug = \"x\"\n"), None);
        let ctx = ModCtx { toml: PathBuf::from("m/mod.toml"), dir: PathBuf::from("m"), text: "[mod]\nauthor = \"Jane\"\n".into(), found: "t".into() };
        let args = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(mark_key(&args(&["--key", "given"]), Some(&ctx), None).unwrap().0, "given");
        assert_eq!(mark_key(&args(&[]), Some(&ctx), Some(("old".into(), "manifest".into()))).unwrap().0, "Jane", "the mod.toml before the old manifest");
        assert_eq!(mark_key(&args(&[]), None, Some(("old".into(), "manifest".into()))).unwrap().0, "old");
        assert_eq!(mark_key(&args(&[]), None, None).unwrap_err(), NO_KEY);
        assert!(!NO_KEY.contains("woflo"));
    }

    #[test]
    fn a_decoded_mod_is_named_from_the_mod_and_the_names_given() {
        let ctx = ModCtx { toml: PathBuf::from("m/mod.toml"), dir: PathBuf::from("m"), text: "[mod]\nslug = \"example-skin\"\nowners = [\"example_voice\"]\n".into(), found: "t".into() };
        let names = known_mods(Some(&ctx), &["other".to_string()]);
        assert_eq!(mod_name_of(scheme::mod_id("example_skin"), &names), "example_skin");
        assert_eq!(mod_name_of(scheme::mod_id("example_voice"), &names), "example_voice");
        assert_eq!(mod_name_of(scheme::mod_id("other"), &names), "other");
        assert_eq!(mod_name_of(scheme::mod_id("nobody"), &names), "unknown mod");
    }
}

//! The mark applied to an addon: which sources are colour, signing them (idempotent), the sidecar manifest
//! that lets a build check the sources stayed signed, and verification of compiled textures.
//!
//! Colour sources are found from the addon's own files, never guessed from names:
//! - a material's `TextureColor[N]` image (albedo); its `TextureTranslucency[N]` image is the coverage the
//!   compiler packs into the same texture's alpha, so the mark keeps clear of its edges;
//! - an sRGB image named by a `.vtex` (particle and UI textures); sprite-sheet frames (`.mks`) are listed
//!   and skipped, because packing frames into a sheet moves them off the grid;
//! - a loose panorama PSD (UI art).
//!
//! An image used anywhere as a non-colour input (normal, roughness, mask...) is never marked, and light
//! cookies (a mask the light projects) are skipped.

use crate::image::{self, Image};
use crate::metrics;
use crate::scheme::{self, Found, Key, Payload, Pool};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

pub const MANIFEST: &str = "magician.signed";
/// Fewest markable cells (opaque, clear of alpha edges) a texture needs to carry the mark.
pub const MIN_CELLS: usize = 2048;
/// Projected z, after the block compression the source compiles to, a texture must reach to be signed:
/// p <= 8 exp(-32) ~ 1e-13 on its own. Below it the mark would be there but not provable texture by texture.
pub const MIN_Z: f64 = 8.0;
/// A source already detecting at this z is treated as signed and left alone.
pub const ALREADY_Z: f64 = 8.0;
/// Per-texture detection threshold for "signed" in reports: p ≤ 1e-9.
pub const SIGNED_LOG10_P: f64 = -9.0;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Role {
    /// `TextureColor` of these materials.
    Albedo(Vec<String>),
    /// The sRGB input of a `.vtex`.
    Vtex(String),
    /// A panorama PSD.
    Ui,
}

impl Role {
    pub fn label(&self) -> String {
        match self {
            Role::Albedo(v) => format!("albedo ({})", v.iter().map(|p| p.rsplit('/').next().unwrap_or(p)).collect::<Vec<_>>().join(", ")),
            Role::Vtex(v) => format!("vtex {}", v.rsplit('/').next().unwrap_or(v)),
            Role::Ui => "ui".into(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Source {
    /// Path relative to the addon root, forward slashes, as on disk.
    pub path: String,
    pub role: Role,
    /// Translucency maps packed into this texture's alpha.
    pub guards: Vec<String>,
    /// Non-coverage maps packed into its alpha (metalness).
    pub packed: Vec<String>,
    /// The `.vtex` output format it compiles to (`DXT1`, `DXT5`, `BC7`, `BGRA8888`...); materials compile
    /// colour to BC7.
    pub format: Option<String>,
    /// Why this colour source is not marked, decided before reading pixels.
    pub skip: Option<String>,
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<String>) -> std::io::Result<()> {
    walk_at(root, dir, out, 0)
}

/// Folders past this depth are a link that loops back (a junction to a parent), not sources.
const MAX_DEPTH: usize = 64;

fn walk_at(root: &Path, dir: &Path, out: &mut Vec<String>, depth: usize) -> std::io::Result<()> {
    if depth > MAX_DEPTH {
        return Err(std::io::Error::other(format!("{}: folders nested past {MAX_DEPTH} levels (a link or junction that loops back?)", dir.display())));
    }
    let mut ents: Vec<_> = std::fs::read_dir(dir)?.filter_map(|e| e.ok()).collect();
    ents.sort_by_key(|e| e.file_name());
    for e in ents {
        let p = e.path();
        let name = e.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        if p.is_dir() {
            walk_at(root, &p, out, depth + 1)?;
        } else if let Ok(rel) = p.strip_prefix(root) {
            out.push(rel.to_string_lossy().replace('\\', "/"));
        }
    }
    Ok(())
}

/// Quoted tokens of a line, for KeyValues (vmat) and keyvalues2 (vtex) text.
fn quoted(line: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut rest = line;
    while let Some(a) = rest.find('"') {
        let after = &rest[a + 1..];
        let Some(b) = after.find('"') else { break };
        out.push(&after[..b]);
        rest = &after[b + 1..];
    }
    out
}

fn is_image(p: &str) -> bool {
    matches!(image::ext(p).as_str(), "png" | "tga" | "psd")
}

/// Every colour source of an addon, with the reasons some are skipped.
pub fn discover(root: &Path) -> Result<Vec<Source>, String> {
    let mut files = Vec::new();
    walk(root, root, &mut files).map_err(|e| format!("{}: {e}", root.display()))?;
    let on_disk: BTreeMap<String, String> = files.iter().map(|f| (f.to_ascii_lowercase(), f.clone())).collect();
    let mut albedo: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut guards: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut packs: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut other_use: BTreeMap<String, String> = BTreeMap::new();
    let mut vtex_in: BTreeMap<String, String> = BTreeMap::new();
    let mut vtex_fmt: BTreeMap<String, String> = BTreeMap::new();
    let mut frames: BTreeMap<String, String> = BTreeMap::new();
    let mut cookies: BTreeSet<String> = BTreeSet::new();
    for f in &files {
        let lower = f.to_ascii_lowercase();
        if lower.ends_with(".vmat") {
            let text = std::fs::read_to_string(root.join(f)).unwrap_or_default();
            let mut colour: BTreeMap<String, String> = BTreeMap::new();
            let mut trans: BTreeMap<String, String> = BTreeMap::new();
            let mut metal: BTreeMap<String, String> = BTreeMap::new();
            for line in text.lines() {
                if line.trim_start().starts_with("//") {
                    continue;
                }
                let q = quoted(line);
                if q.len() != 2 || !is_image(q[1]) {
                    continue;
                }
                let (k, v) = (q[0].to_ascii_lowercase(), q[1].to_ascii_lowercase());
                if let Some(sfx) = k.strip_prefix("texturecolor").filter(|s| s.chars().all(|c| c.is_ascii_digit())) {
                    colour.insert(sfx.to_string(), v);
                } else if let Some(sfx) = k.strip_prefix("texturetranslucency").filter(|s| s.chars().all(|c| c.is_ascii_digit())) {
                    trans.insert(sfx.to_string(), v.clone());
                    other_use.entry(v).or_insert_with(|| q[0].to_string());
                } else if k.starts_with("texture") {
                    if let Some(sfx) = k.strip_prefix("texturemetalness").filter(|s| s.chars().all(|c| c.is_ascii_digit())) {
                        metal.insert(sfx.to_string(), v.clone());
                    }
                    other_use.entry(v).or_insert_with(|| q[0].to_string());
                }
            }
            for (sfx, img) in colour {
                albedo.entry(img.clone()).or_default().push(f.clone());
                if let Some(t) = trans.get(&sfx) {
                    guards.entry(img.clone()).or_default().insert(t.clone());
                }
                if let Some(m) = metal.get(&sfx) {
                    packs.entry(img).or_default().insert(m.clone());
                }
            }
        } else if lower.ends_with(".vtex") {
            let text = std::fs::read_to_string(root.join(f)).unwrap_or_default();
            let mut inputs = Vec::new();
            let mut srgb = false;
            let mut fmt = String::new();
            for line in text.lines() {
                let q = quoted(line);
                if q.len() >= 3 && q[0] == "m_outputFormat" {
                    fmt = q[2].to_string();
                }
                if q.len() >= 3 && q[0] == "m_fileName" {
                    inputs.push(q[2].to_ascii_lowercase());
                }
                if q.len() >= 3 && q[0] == "m_colorSpace" && q[2] == "srgb" {
                    srgb = true;
                }
            }
            let cookie = lower.contains("gobo") || lower.contains("cookie");
            for inp in inputs {
                if inp.ends_with(".mks") {
                    // frames named in the sheet, relative to its folder
                    let dir = inp.rsplit_once('/').map(|(d, _)| format!("{d}/")).unwrap_or_default();
                    let mks = std::fs::read_to_string(root.join(&inp)).unwrap_or_default();
                    for l in mks.lines() {
                        let t: Vec<&str> = l.split_whitespace().collect();
                        if t.first() == Some(&"frame") && t.len() >= 2 {
                            frames.insert(format!("{dir}{}", t[1].to_ascii_lowercase()), f.clone());
                        }
                    }
                } else if is_image(&inp) {
                    if cookie {
                        cookies.insert(inp.clone());
                    } else if srgb {
                        vtex_fmt.insert(inp.clone(), fmt.clone());
                        vtex_in.insert(inp, f.clone());
                    } else {
                        other_use.entry(inp).or_insert_with(|| "linear vtex input".into());
                    }
                }
            }
        }
    }
    let mut out = Vec::new();
    let mut add = |lower: &str, role: Role, skip: Option<String>, g: Vec<String>| {
        let Some(path) = on_disk.get(lower) else { return };
        let skip = skip.or_else(|| other_use.get(lower).map(|u| format!("also used as {u} (not colour only)")));
        let packed = packs.get(lower).map(|s| s.iter().filter_map(|x| on_disk.get(x).cloned()).collect()).unwrap_or_default();
        let format = vtex_fmt.get(lower).cloned().filter(|f| !f.is_empty());
        out.push(Source { path: path.clone(), role, guards: g.iter().filter_map(|x| on_disk.get(x).cloned()).collect(), packed, format, skip });
    };
    for (img, vmats) in &albedo {
        let g: Vec<String> = guards.get(img).map(|s| s.iter().cloned().collect()).unwrap_or_default();
        add(img, Role::Albedo(vmats.clone()), None, g);
    }
    for (img, vtex) in &vtex_in {
        if !albedo.contains_key(img) {
            add(img, Role::Vtex(vtex.clone()), None, vec![]);
        }
    }
    for (img, vtex) in &frames {
        add(img, Role::Vtex(vtex.clone()), Some("sprite-sheet frame: packing frames into a sheet moves them off the cell grid".into()), vec![]);
    }
    for img in &cookies {
        add(img, Role::Vtex(String::new()), Some("light cookie: a mask the light projects, not colour".into()), vec![]);
    }
    for f in &files {
        let lower = f.to_ascii_lowercase();
        if lower.starts_with("panorama/") && lower.ends_with(".psd") && !albedo.contains_key(&lower) && !vtex_in.contains_key(&lower) {
            add(&lower, Role::Ui, None, vec![]);
        }
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(out)
}

pub fn fnv64(b: &[u8]) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    for x in b {
        h ^= *x as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Signed,
    Already,
    Skipped,
}

impl Status {
    pub fn word(self) -> &'static str {
        match self {
            Status::Signed => "signed",
            Status::Already => "already",
            Status::Skipped => "skipped",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Entry {
    pub path: String,
    pub role: String,
    pub status: Status,
    /// FNV-1a 64 of the file as signed (0 when skipped).
    pub hash: u64,
    pub size: (usize, usize),
    pub cells: usize,
    /// Detection z on the signed source, and after our own BC1 and BC7 round trips.
    pub z: f64,
    pub z_bc1: f64,
    pub z_bc7: f64,
    pub psnr: f64,
    pub de_mean: f64,
    pub de_max: f64,
    /// Worst mean CIEDE2000 over mips 1-3.
    pub de_mips: f64,
    pub max_abs: u8,
    pub note: String,
}

pub struct SignOptions {
    pub key: Key,
    pub secret: Option<Key>,
    pub mod_name: String,
    pub day: u16,
    pub dry_run: bool,
    pub force: bool,
    pub target_rms: f64,
}

/// Presence z on the grid the image was marked on.
pub fn z_of(img: &Image, key: Key) -> f64 {
    let g = scheme::grid_for(img.w, img.h);
    scheme::weights(&img.rgba, img.w, img.h, g, true).map_or(0.0, |w| {
        let (s, v) = scheme::presence(key, &w);
        if v > 0.0 {
            s / v.sqrt()
        } else {
            0.0
        }
    })
}

/// Sign one image in memory: returns the signed image and its entry (status Signed or Skipped).
/// `format`: what the source compiles to (None: a material's colour, BC7).
pub fn sign_image(src: &Image, guard: Option<&[u8]>, packed: Option<&[u8]>, format: Option<&str>, o: &SignOptions, path: &str, role: &str) -> (Option<Image>, Entry) {
    let mut e = Entry {
        path: path.into(),
        role: role.into(),
        status: Status::Skipped,
        hash: 0,
        size: (src.w, src.h),
        cells: 0,
        z: 0.0,
        z_bc1: 0.0,
        z_bc7: 0.0,
        psnr: f64::INFINITY,
        de_mean: 0.0,
        de_max: 0.0,
        de_mips: 0.0,
        max_abs: 0,
        note: String::new(),
    };
    let mut img = src.clone();
    let p = scheme::EmbedParams { key: o.key, secret: o.secret, payload: Payload { mod_id: scheme::mod_id(&o.mod_name), day: o.day }, target_rms: o.target_rms };
    let r = match scheme::embed_guarded(&mut img, guard, packed, &p) {
        Ok(r) => r,
        Err(err) => {
            e.note = err;
            return (None, e);
        }
    };
    e.cells = r.marked;
    if r.marked < MIN_CELLS {
        e.note = format!("{} markable cells (opaque, clear of alpha edges), {} needed: too small to carry the mark", r.marked, MIN_CELLS);
        return (None, e);
    }
    let d = metrics::diff(src, &img);
    e.psnr = d.psnr;
    e.de_mean = d.de_mean;
    e.de_max = d.de_max;
    e.max_abs = d.max_abs;
    let (mut a, mut b) = (src.clone(), img.clone());
    for _ in 0..3 {
        if a.w < 2 || a.h < 2 {
            break;
        }
        a = metrics::mip(&a);
        b = metrics::mip(&b);
        e.de_mips = e.de_mips.max(metrics::diff(&a, &b).de_mean);
    }
    e.z = z_of(&img, o.key);
    e.z_bc1 = z_of(&crate::bc::round_trip(&img, crate::bc::Format::Bc1), o.key);
    e.z_bc7 = z_of(&crate::bc::round_trip(&img, crate::bc::Format::Bc7), o.key);
    let (projected, as_) = match format.map(|f| f.to_ascii_uppercase()) {
        Some(f) if f == "DXT1" || f == "DXT5" => (e.z_bc1, f),
        Some(f) if f.contains("8888") => (e.z, f),
        Some(f) => (e.z_bc7, f),
        None => (e.z_bc7, "BC7".to_string()),
    };
    if projected < MIN_Z {
        e.note = format!("projected z {projected:.1} after {as_} is under {MIN_Z}: too small, busy or flat to carry a mark provable on its own");
        e.cells = r.marked;
        return (None, e);
    }
    e.status = Status::Signed;
    (Some(img), e)
}

/// The coverage the compiled texture will carry: the image's own alpha, and every translucency map the
/// materials pack into it (nearest-resampled to the colour map's size), whichever is lower.
fn guard_alpha(root: &Path, img: &Image, guards: &[String]) -> Result<Option<Vec<u8>>, String> {
    if guards.is_empty() {
        return Ok(None);
    }
    let mut a = img.alpha();
    for g in guards {
        let (t, _) = image::load(&root.join(g))?;
        // a translucency map is read from its first channel (greyscale or RGB grey)
        let lum: Vec<u8> = t.rgba.chunks(4).map(|p| p[0].min(p[3])).collect();
        let r = image::resample_nearest(&lum, t.w, t.h, img.w, img.h);
        for (x, y) in a.iter_mut().zip(r) {
            *x = (*x).min(y);
        }
    }
    Ok(Some(a))
}

/// The material's metalness map, which the compiler packs into this texture's alpha (nearest-resampled).
fn packed_map(root: &Path, img: &Image, packed: &[String]) -> Result<Option<Vec<u8>>, String> {
    let Some(first) = packed.first() else { return Ok(None) };
    let (t, _) = image::load(&root.join(first))?;
    let lum: Vec<u8> = t.rgba.chunks(4).map(|p| p[0]).collect();
    Ok(Some(image::resample_nearest(&lum, t.w, t.h, img.w, img.h)))
}

/// Sign every colour source of an addon. Idempotent: a source that already carries the mark is left alone.
pub fn sign_addon(root: &Path, o: &SignOptions, mut progress: impl FnMut(&Entry)) -> Result<Vec<Entry>, String> {
    let mut out = Vec::new();
    for s in discover(root)? {
        let role = s.role.label();
        let blank = |note: String| Entry {
            path: s.path.clone(),
            role: role.clone(),
            status: Status::Skipped,
            hash: 0,
            size: (0, 0),
            cells: 0,
            z: 0.0,
            z_bc1: 0.0,
            z_bc7: 0.0,
            psnr: f64::INFINITY,
            de_mean: 0.0,
            de_max: 0.0,
            de_mips: 0.0,
            max_abs: 0,
            note,
        };
        if let Some(why) = &s.skip {
            let e = blank(why.clone());
            progress(&e);
            out.push(e);
            continue;
        }
        let path = root.join(&s.path);
        let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let (img, container) = match image::decode(&s.path, &bytes) {
            Ok(x) => x,
            Err(err) => {
                let e = blank(err);
                progress(&e);
                out.push(e);
                continue;
            }
        };
        if !o.force {
            if let Some(f) = scheme::detect(&img.rgba, img.w, img.h, o.key, None) {
                if f.step == 0 && f.z >= ALREADY_Z {
                    let mut e = blank(String::new());
                    e.status = Status::Already;
                    e.hash = fnv64(&bytes);
                    e.size = (img.w, img.h);
                    e.cells = f.cells;
                    e.z = f.z;
                    if let Some(p) = f.payload {
                        e.note = format!("mod {:04x}, signed {}", p.mod_id.value, scheme::date_text(p.day.value));
                    }
                    progress(&e);
                    out.push(e);
                    continue;
                }
            }
        }
        let guard = guard_alpha(root, &img, &s.guards)?;
        let packed = packed_map(root, &img, &s.packed)?;
        let (signed, mut e) = sign_image(&img, guard.as_deref(), packed.as_deref(), s.format.as_deref(), o, &s.path, &role);
        if !s.guards.is_empty() {
            e.note = format!("{}{}alpha guard from {}", e.note, if e.note.is_empty() { "" } else { "; " }, s.guards.join(", "));
        }
        if !s.packed.is_empty() {
            e.note = format!("{}{}clear of the edges of {} (packed into alpha)", e.note, if e.note.is_empty() { "" } else { "; " }, s.packed.join(", "));
        }
        if let Some(si) = signed {
            match image::encode(&si, &container) {
                Ok(b) => {
                    // the written file must still decode to the signed pixels, alpha untouched
                    let (back, _) = image::decode(&s.path, &b).map_err(|err| format!("{}: re-read failed: {err}", s.path))?;
                    if back.rgba != si.rgba || back.alpha() != img.alpha() {
                        return Err(format!("{}: the re-encoded file does not read back as signed", s.path));
                    }
                    e.hash = fnv64(&b);
                    if !o.dry_run {
                        std::fs::write(&path, &b).map_err(|err| format!("{}: {err}", path.display()))?;
                    }
                }
                Err(err) => {
                    e.status = Status::Skipped;
                    e.note = err;
                }
            }
        }
        progress(&e);
        out.push(e);
    }
    Ok(out)
}

fn fmt_f(x: f64) -> String {
    if x.is_finite() {
        format!("{x:.2}")
    } else {
        "inf".into()
    }
}

/// Write the sidecar manifest: one line per colour source.
pub fn write_manifest(root: &Path, entries: &[Entry], o: &SignOptions, key_name: &str) -> Result<PathBuf, String> {
    let mut t = String::new();
    t.push_str("# magician sign: the colour sources this addon's maker's mark lives in. Written by `magician sign`; read by\n");
    t.push_str("# `magician build` and `release` to check the sources are still signed. Do not hand-edit.\n");
    t.push_str(&format!(
        "# key {key_name} (public identity) | mod {} ({:04x}) | date {} | secondary key {}\n",
        o.mod_name,
        scheme::mod_id(&o.mod_name),
        scheme::date_text(o.day),
        if o.secret.is_some() { "yes" } else { "no" }
    ));
    t.push_str("# path\tstatus\tfnv64\tsize\tcells\tz\tz_bc1\tz_bc7\tpsnr_db\tde2000_mean\tde2000_max\tde2000_mips\tmax_levels\trole\tnote\n");
    for e in entries {
        t.push_str(&format!(
            "{}\t{}\t{:016x}\t{}x{}\t{}\t{:.1}\t{:.1}\t{:.1}\t{}\t{:.3}\t{:.3}\t{:.3}\t{}\t{}\t{}\n",
            e.path,
            e.status.word(),
            e.hash,
            e.size.0,
            e.size.1,
            e.cells,
            e.z,
            e.z_bc1,
            e.z_bc7,
            fmt_f(e.psnr),
            e.de_mean,
            e.de_max,
            e.de_mips,
            e.max_abs,
            e.role,
            e.note
        ));
    }
    let p = root.join(MANIFEST);
    std::fs::write(&p, t).map_err(|e| format!("{}: {e}", p.display()))?;
    Ok(p)
}

/// The key and mod name an addon's manifest says it was signed with (its `# key K (public identity) | mod M (..)`
/// header line), so a check after the build verifies with the key that signed, not a guess.
pub fn manifest_key(root: &Path) -> Option<(String, String)> {
    let t = std::fs::read_to_string(root.join(MANIFEST)).ok()?;
    manifest_key_of(&t)
}

pub fn manifest_key_of(text: &str) -> Option<(String, String)> {
    let line = text.lines().find_map(|l| l.strip_prefix("# key "))?;
    let (key, rest) = line.split_once(" (public identity) | mod ")?;
    let name = rest.split(" (").next()?.trim();
    (!key.trim().is_empty()).then(|| (key.trim().to_string(), name.to_string()))
}

#[derive(Clone, Debug)]
pub struct ManifestLine {
    pub path: String,
    pub status: String,
    pub hash: u64,
    pub note: String,
}

pub fn read_manifest(root: &Path) -> Option<Vec<ManifestLine>> {
    let t = std::fs::read_to_string(root.join(MANIFEST)).ok()?;
    Some(
        t.lines()
            .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
            .filter_map(|l| {
                let c: Vec<&str> = l.split('\t').collect();
                Some(ManifestLine { path: c.first()?.to_string(), status: c.get(1)?.to_string(), hash: u64::from_str_radix(c.get(2)?, 16).ok()?, note: c.last()?.to_string() })
            })
            .collect(),
    )
}

/// Are the sources still the files that were signed?
#[derive(Default, Debug)]
pub struct SourceCheck {
    pub manifest: bool,
    pub signed: Vec<String>,
    /// Signed once, since overwritten (a generator re-ran): needs `magician sign` again.
    pub changed: Vec<String>,
    pub missing: Vec<String>,
    /// Colour sources the manifest does not know (new since signing).
    pub new: Vec<String>,
    pub skipped: usize,
}

pub fn check_sources(root: &Path) -> SourceCheck {
    let mut c = SourceCheck::default();
    let Some(m) = read_manifest(root) else { return c };
    c.manifest = true;
    let known: BTreeSet<String> = m.iter().map(|l| l.path.clone()).collect();
    for l in &m {
        match l.status.as_str() {
            "signed" | "already" => match std::fs::read(root.join(&l.path)) {
                Ok(b) if fnv64(&b) == l.hash => c.signed.push(l.path.clone()),
                Ok(_) => c.changed.push(l.path.clone()),
                Err(_) => c.missing.push(l.path.clone()),
            },
            _ => c.skipped += 1,
        }
    }
    if let Ok(srcs) = discover(root) {
        for s in srcs {
            if !known.contains(&s.path) {
                c.new.push(s.path);
            }
        }
    }
    c
}

// ---------------------------------------------------------------------------------------------- verification

#[derive(Clone, Debug)]
pub struct TexResult {
    pub path: String,
    pub w: usize,
    pub h: usize,
    pub format: &'static str,
    pub found: Option<Found>,
    pub note: Option<String>,
}

impl TexResult {
    pub fn signed(&self) -> bool {
        self.found.as_ref().is_some_and(|f| f.log10_p <= SIGNED_LOG10_P)
    }
}

/// Formats that can carry colour (normal maps and single-channel masks are ATI1N/ATI2N; BC6H is HDR).
pub fn colour_format(f: u8) -> bool {
    matches!(f, 1 | 2 | 4 | 20 | 28)
}

/// The compiler's edit-info (the RED2 block, LZ4 KV3) names this conversion when it stored a DXT5 texture as YCoCg (Panorama's
/// image-list route does it for small images such as the HUD's vertical portrait).
const YCOCG_TAG: &[u8] = b"YCoCg Conversion";

/// Mip 0 as RGBA8, undoing YCoCg storage (Y in alpha, Co and Cg in red and green, the scale in blue: van
/// Waveren and Castaño's YCoCg-DXT5, as Source 2 Viewer decodes it).
pub fn is_ycocg(data: &[u8]) -> bool {
    data.windows(YCOCG_TAG.len()).any(|x| x == YCOCG_TAG)
        || [b"RED2", b"REDI"].iter().any(|t| matches!(resource::kv3_block(data, t), Some(Ok(kv)) if kv.to_text().contains("YCoCg Conversion")))
}

pub fn decode_colour(data: &[u8]) -> Option<(usize, usize, Vec<u8>)> {
    decode_mip(data, 0)
}

/// Any mip as RGBA8, YCoCg undone.
pub fn decode_mip(data: &[u8], mip: u32) -> Option<(usize, usize, Vec<u8>)> {
    let (w, h, mut px) = resource::pixels::decode(data, mip)?;
    if is_ycocg(data) {
        for p in px.chunks_mut(4) {
            let s = ((p[2] >> 3) + 1) as f64;
            let co = (p[0] as f64 - 128.0) / s;
            let cg = (p[1] as f64 - 128.0) / s;
            let y = p[3] as f64;
            let c = |v: f64| v.round().clamp(0.0, 255.0) as u8;
            p.copy_from_slice(&[c(y + co - cg), c(y + cg), c(y - co - cg), 255]);
        }
    }
    Some((w, h, px))
}

/// Decode a compiled texture's top mip and detect.
pub fn verify_vtex(path: &str, data: &[u8], key: Key, secret: Option<Key>) -> Option<TexResult> {
    let t = resource::texture::info(data)?;
    let mut r = TexResult { path: path.into(), w: t.width as usize, h: t.height as usize, format: t.format_name(), found: None, note: None };
    if !colour_format(t.format) {
        r.note = Some("not a colour format".into());
        return Some(r);
    }
    match decode_colour(data) {
        Some((w, h, px)) => {
            r.found = scheme::detect(&px, w, h, key, secret);
            if r.found.is_none() {
                r.note = Some("too small for the cell grid".into());
            }
        }
        None => r.note = Some("pixels not decodable (compressed mips or unknown layout)".into()),
    }
    Some(r)
}

/// Pool every result's canonical-grid weights.
pub fn pool<'a>(results: impl Iterator<Item = &'a TexResult>) -> Pool {
    let mut p = Pool::default();
    for r in results {
        if let Some(w) = r.found.as_ref().and_then(|f| f.canonical.as_ref()) {
            p.add(w);
        }
    }
    p
}

/// A compiled texture's colour source: `<stem>_<ext>_<hash8>.vtex_c` (a material's image),
/// `<stem>_<ext>.vtex_c` (an image list), `<stem>.vtex_c` (a `.vtex`), matched against the manifest's paths.
pub fn source_for_output(out: &str, sources: &BTreeMap<String, String>) -> Option<String> {
    let base = out.strip_suffix(".vtex_c")?.to_ascii_lowercase();
    let parts: Vec<&str> = base.split('_').collect();
    let hashed = parts.last().is_some_and(|h| h.len() == 8 && h.chars().all(|c| c.is_ascii_hexdigit()));
    let body = if hashed { parts[..parts.len() - 1].join("_") } else { base.clone() };
    for ext in ["png", "tga", "psd"] {
        if let Some(stem) = body.strip_suffix(&format!("_{ext}")) {
            if let Some(s) = sources.get(&format!("{stem}.{ext}")) {
                return Some(s.clone());
            }
        }
    }
    // a .vtex compiles to the same name; its input is in the manifest note-free map under the vtex path
    sources.get(&format!("{body}.vtex")).cloned()
}

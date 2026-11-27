//! Rules that read inside compiled files (KV3 data blocks, texture headers) and, where a file replaces a
//! game file, compare it with Valve's original. Each measurement here was taken by hand first, during a
//! real mod (Maid Lash and another skin), and is enforced here so the next mod doesn't relearn it.

use resource::kv3::Value;
use resource::texture::{self, TexInfo};

// ------------------------------------------------------------------------------------------ helpers

/// A float input as the compiler stores it: a plain number, or a `CParticleCollectionFloatInput` object.
/// Only literal values are comparable; anything driven by a curve, control point or random range returns
/// its upper bound when one is stored, else `None`.
pub fn float_input(v: Option<&Value>) -> Option<f64> {
    let v = v?;
    if let Some(x) = v.as_f64() {
        return Some(x);
    }
    match v.get("m_nType").and_then(Value::as_str) {
        Some("PF_TYPE_LITERAL") | None => v.get("m_flLiteralValue").and_then(Value::as_f64),
        Some("PF_TYPE_RANDOM_UNIFORM") | Some("PF_TYPE_RANDOM_BIASED") => v.get("m_flRandomMax").and_then(Value::as_f64),
        _ => None,
    }
}

/// A colour: a plain `[r, g, b(, a)]` array, or a vector input carrying `m_LiteralColor`.
pub fn color(v: Option<&Value>) -> Option<[f64; 3]> {
    let v = v?;
    let n = v.numbers();
    let n = if n.len() >= 3 { n } else { v.get("m_LiteralColor")?.numbers() };
    if n.len() >= 3 {
        Some([n[0], n[1], n[2]])
    } else {
        None
    }
}

fn srgb_to_linear(c: f64) -> f64 {
    let c = (c / 255.0).clamp(0.0, 1.0);
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

/// Relative luminance (linear, Rec. 709) of an sRGB 0-255 colour.
pub fn luminance(c: [f64; 3]) -> f64 {
    0.2126 * srgb_to_linear(c[0]) + 0.7152 * srgb_to_linear(c[1]) + 0.0722 * srgb_to_linear(c[2])
}

fn class(v: &Value) -> &str {
    v.get("_class").and_then(Value::as_str).unwrap_or("")
}

// --------------------------------------------------------------------------------------- particles

#[derive(Debug, Clone, PartialEq)]
pub struct Light {
    pub class: String,
    pub lumens: Option<f64>,
    pub range: Option<f64>,
    pub color: Option<[f64; 3]>,
    pub shadows: bool,
}

impl Light {
    /// Emitted light, lumens times the colour's luminance (the measure that caught Death Slam's gobo at 2.7x).
    pub fn output(&self) -> Option<f64> {
        Some(self.lumens? * luminance(self.color.unwrap_or([255.0, 255.0, 255.0])))
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ParticleProfile {
    pub lights: Vec<Light>,
    /// Brightest colour luminance x strongest alpha x strongest overbright of any non-light renderer.
    pub energy: f64,
    /// Largest sprite radius the system can reach (constant or random max, times the largest radius scale).
    pub radius: f64,
    /// Light-cookie textures (compiled paths) with the field that names them.
    pub cookies: Vec<(String, String)>,
    /// Child systems (compiled paths).
    pub children: Vec<String>,
}

pub fn particle_profile(v: &Value) -> ParticleProfile {
    let mut p = ParticleProfile::default();
    let mut max_y: f64 = 0.0;
    let mut max_a: f64 = 0.0;
    let constant = v.get("m_ConstantColor").map(|c| c.numbers()).unwrap_or_else(|| vec![255.0; 4]);
    if constant.len() >= 3 {
        max_y = max_y.max(luminance([constant[0], constant[1], constant[2]]));
    }
    max_a = max_a.max(*constant.get(3).unwrap_or(&255.0));

    let mut strength: f64 = 0.0;
    for r in v.get("m_Renderers").map(Value::as_array).unwrap_or(&[]) {
        let c = class(r);
        if c.contains("Light") {
            p.lights.push(Light {
                class: c.to_string(),
                lumens: float_input(r.get("m_flBrightnessLumens")).or_else(|| float_input(r.get("m_flBrightness"))),
                range: float_input(r.get("m_flRange")).or_else(|| float_input(r.get("m_flRadiusScale"))),
                color: color(r.get("m_vColorBlend")).or_else(|| color(r.get("m_ColorScale"))),
                shadows: r.get("m_bCastShadows").and_then(Value::as_bool).unwrap_or(false),
            });
        } else {
            let ob = float_input(r.get("m_flOverbrightFactor")).unwrap_or(1.0);
            let al = float_input(r.get("m_flAlphaScale")).unwrap_or(1.0);
            strength = strength.max(ob.max(0.0) * al.max(0.0));
        }
        // cookies: any string under a key naming a cookie, anywhere in the renderer
        r.walk(&mut |k, x| {
            if k.to_ascii_lowercase().contains("cookie") {
                if let Some(s) = x.as_str().filter(|s| !s.is_empty()) {
                    p.cookies.push((compiled(s), k.to_string()));
                }
            }
        });
    }
    // A particle is born at the system's constant radius (5 when the file leaves it out) unless an initializer writes
    // the radius: `C_INIT_RandomRadius`, or a `C_INIT_InitFloat` whose output field is the radius (field 3, which the
    // compiler leaves out as the default). Then the constant is never drawn. A Valve smoke with a constant of 2 and an
    // InitFloat of 30 to 40 read as radius 2; a community copy with no constant and an InitFloat of 5 to 10 read as 5,
    // "2.5 times the size" of a cloud a quarter of Valve's (6871). Operators that set the radius later count too.
    let mut born: Option<f64> = None;
    let mut later: f64 = 0.0;
    let mut scale: f64 = 1.0;
    for list in ["m_Initializers", "m_Operators"] {
        for op in v.get(list).map(Value::as_array).unwrap_or(&[]) {
            for key in ["m_ColorMin", "m_ColorMax", "m_ColorFade", "m_TintMin", "m_TintMax"] {
                if let Some(c) = color(op.get(key)) {
                    max_y = max_y.max(luminance(c));
                }
            }
            if let Some(a) = op.get("m_nAlphaMax").and_then(Value::as_f64) {
                max_a = max_a.max(a);
            }
            let writes_radius = matches!(class(op), "C_INIT_InitFloat" | "C_OP_SetFloat") && op.get("m_nOutputField").and_then(Value::as_f64).unwrap_or(3.0) == 3.0;
            let set = float_input(op.get("m_flRadiusMax")).or_else(|| writes_radius.then(|| float_input(op.get("m_InputValue"))).flatten());
            if let Some(r) = set {
                if list == "m_Initializers" {
                    born = Some(born.map_or(r, |b| b.max(r)));
                } else {
                    later = later.max(r);
                }
            }
            if class(op).contains("InterpolateRadius") {
                for k in ["m_flStartScale", "m_flEndScale"] {
                    scale = scale.max(float_input(op.get(k)).unwrap_or(1.0));
                }
            }
        }
    }
    p.energy = max_y * (max_a / 255.0) * strength;
    let radius = born.unwrap_or_else(|| float_input(v.get("m_flConstantRadius")).unwrap_or(5.0)).max(later);
    p.radius = radius * scale;
    for c in v.get("m_Children").map(Value::as_array).unwrap_or(&[]) {
        if let Some(s) = c.get("m_ChildRef").and_then(Value::as_str).filter(|s| !s.is_empty()) {
            p.children.push(compiled(s));
        }
    }
    p
}

pub fn compiled(s: &str) -> String {
    let s = s.to_ascii_lowercase().replace('\\', "/");
    if s.ends_with("_c") { s } else { format!("{s}_c") }
}

/// Lights in a system and every child under it, following children through `read` (depth-limited).
pub fn lights_in_tree(root: &Value, read: &dyn Fn(&str) -> Option<Value>) -> Vec<Light> {
    fn go(v: &Value, read: &dyn Fn(&str) -> Option<Value>, depth: usize, out: &mut Vec<Light>) {
        let p = particle_profile(v);
        out.extend(p.lights);
        if depth >= 6 {
            return;
        }
        for c in p.children {
            if let Some(cv) = read(&c) {
                go(&cv, read, depth + 1, out);
            }
        }
    }
    let mut out = Vec::new();
    go(root, read, 0, &mut out);
    out
}

/// Light caps against Valve's original (M082), at the limits in `fp`. Returns one message per breach.
pub fn light_caps(ours: &[Light], valve: &[Light], fp: &crate::policy::FairPlay) -> Vec<String> {
    let mut out = Vec::new();
    if ours.len() > valve.len() {
        out.push(format!("{} light renderer(s) in the system tree, Valve's has {}", ours.len(), valve.len()));
    }
    for (i, (a, b)) in ours.iter().zip(valve).enumerate() {
        let n = i + 1;
        if let (Some(x), Some(y)) = (a.lumens, b.lumens) {
            if x > y * fp.light_lumens_ratio {
                out.push(format!("light {n}: {x} lumens, Valve's {y}"));
            }
        }
        if let (Some(x), Some(y)) = (a.output(), b.output()) {
            if y > 0.0 && x > y * fp.light_output_ratio && a.lumens.zip(b.lumens).is_none_or(|(p, q)| p <= q * fp.light_lumens_ratio) {
                out.push(format!(
                    "light {n}: colour {:?} gives {:.2}x the luminous output of Valve's {:?} at the same lumens",
                    a.color.unwrap_or([255.0; 3]),
                    x / y,
                    b.color.unwrap_or([255.0; 3])
                ));
            }
        }
        if let (Some(x), Some(y)) = (a.range, b.range) {
            if x > y * fp.light_range_ratio {
                out.push(format!("light {n}: range {x}, Valve's {y}"));
            }
        }
        if a.shadows && !b.shadows {
            out.push(format!("light {n}: casts shadows, Valve's doesn't"));
        }
    }
    out
}

/// Sprite energy and size against Valve's original (M081), at the limits in `fp`.
pub fn particle_energy(ours: &ParticleProfile, valve: &ParticleProfile, fp: &crate::policy::FairPlay) -> Vec<String> {
    let mut out = Vec::new();
    if valve.energy > 0.0 && ours.energy > valve.energy * fp.particle_energy_ratio {
        out.push(format!("colour x alpha x overbright {:.3}, Valve's {:.3} ({:.1}x)", ours.energy, valve.energy, ours.energy / valve.energy));
    }
    if valve.radius > 0.0 && ours.radius > valve.radius * fp.particle_radius_ratio {
        out.push(format!("largest sprite radius {:.1}, Valve's {:.1} ({:.1}x the area)", ours.radius, valve.radius, (ours.radius / valve.radius).powi(2)));
    }
    out
}

/// The size of the light cookie texture set's slots. Valve's 15 cookies are all 512x512 BGRA8888.
pub const COOKIE_SIZE: u16 = 512;

/// What keeps a texture out of the light cookie texture set, each with the game's own log line (both seen in
/// game on Maid Lash's lace gobo): the set takes BGRA8888 only, and it copies a 512x512 mip level.
pub fn cookie_problems(t: &TexInfo) -> Vec<String> {
    let mut out = Vec::new();
    if t.format != texture::BGRA8888 {
        out.push(format!(
            "it is {}; the set takes BGRA8888 (game log: \"Unable to AddTextureToSet(...): wrong format {} - expected BGRA8888!\"); set m_outputFormat \"BGRA8888\"",
            t.format_name(),
            t.format_name()
        ));
    }
    let has_512 = (0..t.mips.max(1) as u32).any(|k| (t.width as u32 >> k) == COOKIE_SIZE as u32 && (t.height as u32 >> k) == COOKIE_SIZE as u32);
    if !has_512 {
        out.push(format!(
            "it is {}x{} with {} mip(s) and none is 512x512 (game log: \"Unable to AddTextureToSet(...): ... has no mip levels of size 512x512\"); make it 512x512",
            t.width, t.height, t.mips
        ));
    }
    out
}

pub fn tex_info(data: &[u8]) -> Option<TexInfo> {
    texture::info(data)
}

// ------------------------------------------------------------------------------------ sound events

/// Every `.vsnd` a sound-event file names (pools, tracks, last-shot files). These are strings in the KV3
/// data, not RERL entries, so the reference walk has to read them here.
pub fn vsnd_refs(v: &Value) -> Vec<String> {
    let mut out: Vec<String> = v
        .strings()
        .into_iter()
        .filter(|s| s.to_ascii_lowercase().ends_with(".vsnd"))
        .map(compiled)
        .collect();
    out.sort();
    out.dedup();
    out
}

/// Whether a sound event key sets how far the sound carries. Valve's 160 event files (6871) use the falloff family
/// (`volume_falloff_min`, `_max`, `_floor`, each also per team, opponent, neutral and player; the falloff curves and
/// whether the curve editor drives them), two cut-offs (`cull_at_distance`, `max_distance_recipient_filter`) and how a
/// wall muffles it (`occlusion_radius`, `occlusion_volume_blend_distance_min` and `_max`). Other keys that name a
/// distance set something else: where a sound folds to mono (`mono_downmix_min_distance`), whether a distance filter
/// colours it (`distance_lpf_on`), how far a limiter counts instances (`limiter_global_event_distance_max`).
const REACH_ORDERED: &[&str] = &["volume_falloff_min", "volume_falloff_max", "volume_falloff_floor", "cull_at_distance", "max_distance_recipient_filter", "occlusion_radius", "occlusion_volume_blend_distance"];

fn is_reach_key(k: &str) -> bool {
    k.to_ascii_lowercase().starts_with("volume_falloff_") || reach_is_ordered(k)
}

/// A reach key whose number is a distance or a floor, so a smaller one carries less far; the falloff curves and the
/// editor switch are judged by being there or not.
fn reach_is_ordered(k: &str) -> bool {
    let k = k.to_ascii_lowercase();
    REACH_ORDERED.iter().any(|p| k.starts_with(p))
}

/// The texture slots a compiled material sets (`m_textureParams[].m_name`, `g_tColor` and the like), sorted.
pub fn texture_params(v: &Value) -> Vec<String> {
    let mut out: Vec<String> = v.get("m_textureParams").map(Value::as_array).unwrap_or(&[]).iter().filter_map(|t| t.get("m_name").and_then(Value::as_str).map(String::from)).collect();
    out.sort();
    out.dedup();
    out
}

/// An event's direct bases: `base` is one event's name or a list of `{ event_name }` (Valve writes both). Sorted, so
/// the same bases in another order or form compare equal.
pub fn bases(base: Option<&Value>) -> Vec<String> {
    let mut out: Vec<String> = match base {
        Some(Value::Str(s)) if !s.is_empty() => vec![s.clone()],
        Some(Value::Array(a)) => a.iter().filter_map(|x| x.as_str().or_else(|| x.get("event_name").and_then(Value::as_str))).filter(|s| !s.is_empty()).map(String::from).collect(),
        _ => Vec::new(),
    };
    out.sort();
    out.dedup();
    out
}

/// The events an event inherits from, itself included: `base` is one event's name or a list of `{ event_name }`.
pub fn base_closure(name: &str, events: &dyn Fn(&str) -> Option<Value>) -> std::collections::BTreeSet<String> {
    let mut seen = std::collections::BTreeSet::new();
    let mut todo = vec![name.to_string()];
    while let Some(n) = todo.pop() {
        if seen.len() > 64 || !seen.insert(n.clone()) {
            continue;
        }
        let Some(def) = events(&n) else { continue };
        match def.get("base") {
            Some(Value::Str(s)) => todo.push(s.clone()),
            Some(Value::Array(a)) => todo.extend(a.iter().filter_map(|x| x.get("event_name").and_then(Value::as_str).map(String::from))),
            _ => {}
        }
    }
    seen
}

/// When our base is one Valve's base inherits from, the difference is exactly the fields the events in between set:
/// (event, key, value) for each, from the game's own base events. None when ours is not one of its ancestors (another
/// branch, or a base the game does not define).
pub fn base_difference(ours: &str, valve: &str, events: &dyn Fn(&str) -> Option<Value>) -> Option<Vec<(String, String, Value)>> {
    let theirs = base_closure(valve, events);
    if ours.is_empty() || !theirs.contains(ours) {
        return None;
    }
    let mine = base_closure(ours, events);
    let mut out = Vec::new();
    for e in theirs.difference(&mine) {
        let def = events(e)?;
        for (k, v) in def.members() {
            if k != "base" && k != "type" {
                out.push((e.clone(), k.clone(), v.clone()));
            }
        }
    }
    Some(out)
}

/// Compare a replaced sound-event file with Valve's. Returns (reduced cues, louder cues): the first at the fair-play
/// limits in `fp` (M063), the second at the mix limits in `mesh::rules` (M064). `events` gives an event's definition
/// from the game's sound-event files, to read what a changed base really changes.
pub fn sound_events(ours: &Value, valve: &Value, fp: &crate::policy::FairPlay, events: &dyn Fn(&str) -> Option<Value>) -> (Vec<String>, Vec<String>) {
    use crate::mesh::rules::SOUND_LOUDER_SLACK_DB;
    let (mut less, mut more) = (Vec::new(), Vec::new());
    for (name, ve) in valve.members() {
        let Some(oe) = ours.get(name) else {
            less.push(format!("{name}: event removed"));
            continue;
        };
        let num = |e: &Value, k: &str| e.get(k).and_then(Value::as_f64);
        let ov = num(oe, "volume").unwrap_or(0.0);
        let vv = num(ve, "volume").unwrap_or(0.0);
        if ov < vv - fp.sound_volume_slack_db {
            less.push(format!("{name}: volume {ov} dB, Valve's {vv}"));
        } else if ov > vv + SOUND_LOUDER_SLACK_DB {
            more.push(format!("{name}: volume {ov} dB, Valve's {vv}"));
        }
        let ob = bases(oe.get("base"));
        let vb = bases(ve.get("base"));
        if ob != vb {
            let (o, v) = (ob.join(" + "), vb.join(" + "));
            // `whose` is (event, key, value) from the base events in between; `sign` is +1 when they belong to Valve's
            // side (a level there is what ours lacks), -1 when they are extra on ours (a level there is what ours adds)
            let mut judge = |fields: Vec<(String, String, Value)>, sign: f64| {
                for (e, k, val) in fields {
                    let lk = k.to_ascii_lowercase();
                    let level = lk == "volume" || lk.starts_with("volume_offset");
                    let raw = val.as_f64();
                    match raw.map(|db| db * sign) {
                        Some(db) if level && db < 0.0 => more.push(format!("{name}: base {o:?}, Valve's {v:?}, where {e} sets {k} {}: ours plays {:.1} dB louder", raw.unwrap_or(0.0), -db)),
                        Some(db) if level && db > 0.0 => less.push(format!("{name}: base {o:?}, Valve's {v:?}, where {e} sets {k} {}: ours plays {db:.1} dB quieter", raw.unwrap_or(0.0))),
                        _ if is_reach_key(&k) => less.push(format!("{name}: base {o:?}, Valve's {v:?}, where {e} sets {k}: how far it carries differs")),
                        _ => {}
                    }
                }
            };
            if let ([o1], [v1]) = (ob.as_slice(), vb.as_slice()) {
                match base_difference(o1, v1, events) {
                    // ours is an ancestor of Valve's base: only the fields in between differ, and only level and reach are heard
                    Some(fields) => judge(fields, 1.0),
                    None => less.push(format!("{name}: base {o:?}, Valve's {v:?} (the base stack sets falloff and perspective)")),
                }
            } else if !vb.is_empty() && vb.iter().all(|b| ob.contains(b)) {
                // ours keeps every base Valve's has and adds more: only what the added bases set beyond Valve's can differ
                let theirs: std::collections::BTreeSet<String> = vb.iter().flat_map(|b| base_closure(b, events)).collect();
                let (mut extra, mut unknown) = (Vec::new(), Vec::new());
                for x in ob.iter().filter(|x| !vb.contains(x)) {
                    for e in base_closure(x, events).difference(&theirs) {
                        match events(e) {
                            Some(def) => extra.extend(def.members().iter().filter(|(k, _)| k != "base" && k != "type").map(|(k, val)| (e.clone(), k.clone(), val.clone()))),
                            None => unknown.push(format!("{name}: base {o:?}, Valve's {v:?}: {e} is not a sound event the game defines")),
                        }
                    }
                }
                judge(extra, -1.0);
                less.extend(unknown);
            } else {
                less.push(format!("{name}: base {o:?}, Valve's {v:?} (the base stack sets falloff and perspective)"));
            }
        }
        let keys: Vec<&str> = ve.members().iter().chain(oe.members()).map(|(k, _)| k.as_str()).collect();
        let mut seen = std::collections::BTreeSet::new();
        for k in keys {
            if !seen.insert(k) {
                continue;
            }
            if is_reach_key(k) {
                if let (true, Some(a), Some(b)) = (reach_is_ordered(k), num(oe, k), num(ve, k)) {
                    if a < b - 1e-3 {
                        less.push(format!("{name}: {k} {a}, Valve's {b}"));
                    } else if a > b + 1e-3 {
                        more.push(format!("{name}: {k} {a}, Valve's {b}"));
                    }
                } else if oe.get(k).is_some() != ve.get(k).is_some() {
                    less.push(format!("{name}: {k} {} (Valve's {})", if oe.get(k).is_some() { "added" } else { "removed" }, if ve.get(k).is_some() { "has it" } else { "doesn't" }));
                }
            }
            if k.starts_with("block_") {
                let a = oe.get(k).and_then(Value::as_bool).unwrap_or(false);
                let b = ve.get(k).and_then(Value::as_bool).unwrap_or(false);
                if a && !b {
                    less.push(format!("{name}: {k} turned on (fewer players hear it)"));
                }
            }
        }
        // a voice line's event is not judged by its length, as its file is not (`vsnd_compare`): Valve's own voices
        // fail that test against each other on 46% of shared lines (`magician corpus`)
        let speaks = |e: &Value| e.get("vsnd_files").map(vsnd_refs).is_some_and(|f| !f.is_empty() && f.iter().all(|x| x.starts_with("sounds/vo/")));
        if let (false, Some(a), Some(b)) = (speaks(ve), num(oe, "vsnd_duration"), num(ve, "vsnd_duration")) {
            if b > 0.0 && a < b * fp.sound_duration_ratio {
                less.push(format!("{name}: plays {a:.2} s, Valve's {b:.2} s"));
            }
        }
    }
    (less, more)
}

/// From a `.vsnd_c` CTRL block: duration in seconds and the envelope's peak (linear).
pub fn vsnd_measure(ctrl: &Value) -> (Option<f64>, Option<f64>) {
    let dur = ctrl.at("m_vSound.m_flDuration").and_then(Value::as_f64);
    let peak = ctrl
        .at("m_pEnvelopeAnalyzer.m_curve.m_spline")
        .map(|s| s.as_array().iter().filter_map(|p| p.get("y").and_then(Value::as_f64)).fold(0.0, f64::max))
        .filter(|&p| p > 0.0);
    (dur, peak)
}

/// Compare a replaced sound file with Valve's: shorter or quieter hides the cue (M063, at the limits in `fp`); longer
/// or louder is a mix problem (M064). Envelope peaks are compared in dB. A voice line (`voice`) is not judged by its
/// length: Valve's own voices fail the duration test against each other on 46% of the lines two heroes share
/// (`magician corpus`, build 6868), so a line's length is the voice, not a cue.
pub fn vsnd_compare(ours: &Value, valve: &Value, fp: &crate::policy::FairPlay, voice: bool) -> (Vec<String>, Vec<String>) {
    use crate::mesh::rules::{SOUND_LONGER_RATIO, SOUND_PEAK_LOUDER_DB};
    let (mut less, mut more) = (Vec::new(), Vec::new());
    let (od, op) = vsnd_measure(ours);
    let (vd, vp) = vsnd_measure(valve);
    if let (Some(a), Some(b), false) = (od, vd, voice) {
        if b > 0.0 && a < b * fp.sound_duration_ratio {
            less.push(format!("{a:.2} s long, Valve's {b:.2} s"));
        } else if b > 0.0 && a > b * SOUND_LONGER_RATIO {
            more.push(format!("{a:.2} s long, Valve's {b:.2} s"));
        }
    }
    if let (Some(a), Some(b)) = (op, vp) {
        let db = 20.0 * (a / b).log10();
        if db < -fp.sound_peak_slack_db {
            less.push(format!("envelope peak {db:+.1} dB against Valve's"));
        } else if db > SOUND_PEAK_LOUDER_DB {
            more.push(format!("envelope peak {db:+.1} dB against Valve's"));
        }
    }
    (less, more)
}

// ------------------------------------------------------------------------------------------ models
// (quaternion canonical form is tested in tests.rs: m062_quaternion_sign_is_not_a_change)

/// What a model document promises the game: bone names and parents, attachments, hitboxes.
#[derive(Debug, Default)]
pub struct Contract {
    pub bones: Vec<(String, String)>,
    pub attachments: Vec<(String, String)>,
    pub hitboxes: Vec<(String, String)>,
}

fn r2(v: &Value) -> String {
    nums(&v.numbers())
}
fn nums(n: &[f64]) -> String {
    // -0.004 and 0.004 are the same position: never let a sign survive rounding to zero.
    n.iter().map(|x| format!("{:.2}", x).replace("-0.00", "0.00")).collect::<Vec<_>>().join(",")
}
/// q and -q are the same rotation; the compiler writes either. Canonical form: w >= 0 (first non-zero
/// component positive when w is 0).
fn quat(n: &[f64]) -> String {
    let mut q = n.to_vec();
    let lead = [q[3], q[0], q[1], q[2]].into_iter().find(|x| x.abs() > 5e-4).unwrap_or(0.0);
    if lead < 0.0 {
        q.iter_mut().for_each(|x| *x = -*x);
    }
    nums(&q)
}
fn nested(v: &Value) -> String {
    match v {
        Value::Array(a) if a.iter().any(|x| matches!(x, Value::Array(_))) => a.iter().map(r2).collect::<Vec<_>>().join(";"),
        _ => r2(v),
    }
}
fn rotations(v: &Value) -> String {
    v.as_array().iter().map(|r| {
        let n = r.numbers();
        if n.len() == 4 { quat(&n) } else { nums(&n) }
    }).collect::<Vec<_>>().join(";")
}

pub fn contract(data: &[u8]) -> Option<Contract> {
    let r = resource::parse(data).ok()?;
    let mut c = Contract::default();
    if let Some(Ok(d)) = resource::kv3_block(data, b"DATA") {
        if let Some(sk) = d.get("m_modelSkeleton") {
            let names: Vec<String> = sk.get("m_boneName").map(|a| a.as_array().iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default();
            let parents = sk.get("m_nParent").map(|a| a.numbers()).unwrap_or_default();
            for (i, n) in names.iter().enumerate() {
                let p = parents.get(i).copied().unwrap_or(-1.0);
                let pn = if p >= 0.0 { names.get(p as usize).cloned().unwrap_or_default() } else { String::new() };
                c.bones.push((n.clone(), pn));
            }
        }
    }
    for b in r.blocks_tagged(b"MDAT") {
        let bytes = resource::block_bytes(data, b);
        if !resource::kv3::is_kv3(bytes) {
            continue;
        }
        let Ok(m) = resource::kv3::decode(bytes) else { continue };
        for a in m.get("m_attachments").map(Value::as_array).unwrap_or(&[]) {
            let v = a.get("value").unwrap_or(a);
            let name = v.get("m_name").and_then(Value::as_str).unwrap_or("").to_string();
            let infl: Vec<&str> = v.get("m_influenceNames").map(|x| x.as_array().iter().filter_map(Value::as_str).collect()).unwrap_or_default();
            let sig = format!(
                "on {} at {} rot {}",
                infl.iter().filter(|s| !s.is_empty()).cloned().collect::<Vec<_>>().join("+"),
                v.get("m_vInfluenceOffsets").map(nested).unwrap_or_default(),
                v.get("m_vInfluenceRotations").map(rotations).unwrap_or_default()
            );
            c.attachments.push((name, sig));
        }
        for set in m.get("m_hitboxsets").map(Value::as_array).unwrap_or(&[]) {
            let v = set.get("value").unwrap_or(set);
            let sn = v.get("m_name").and_then(Value::as_str).unwrap_or("");
            for h in v.get("m_HitBoxes").map(Value::as_array).unwrap_or(&[]) {
                let name = format!("{sn}/{}", h.get("m_name").and_then(Value::as_str).unwrap_or(""));
                let sig = format!(
                    "bone {} min {} max {} radius {:.2} shape {} group {}",
                    h.get("m_sBoneName").and_then(Value::as_str).unwrap_or(""),
                    h.get("m_vMinBounds").map(r2).unwrap_or_default(),
                    h.get("m_vMaxBounds").map(r2).unwrap_or_default(),
                    h.get("m_flShapeRadius").and_then(Value::as_f64).unwrap_or(0.0),
                    h.get("m_nShapeType").and_then(Value::as_f64).unwrap_or(0.0),
                    h.get("m_nGroupId").and_then(Value::as_f64).unwrap_or(0.0),
                );
                c.hitboxes.push((name, sig));
            }
        }
    }
    Some(c)
}

/// The first position of an attachment signature (`on <bones> at x,y,z;...`).
fn first_position(sig: &str) -> Option<[f64; 3]> {
    let at = sig.split(" at ").nth(1)?.split([';', ' ']).next()?;
    let v: Vec<f64> = at.split(',').map(|x| x.parse().ok()).collect::<Option<_>>()?;
    (v.len() == 3).then(|| [v[0], v[1], v[2]])
}

/// The first rotation of an attachment signature (`... rot x,y,z,w;...`), as a quaternion.
fn first_rotation(sig: &str) -> Option<[f64; 4]> {
    let r = sig.split(" rot ").nth(1)?.split([';', ' ']).next()?;
    let v: Vec<f64> = r.split(',').map(|x| x.parse().ok()).collect::<Option<_>>()?;
    (v.len() == 4).then(|| [v[0], v[1], v[2], v[3]])
}

/// The Hamilton product a * b of two quaternions written x, y, z, w.
fn qmul(a: [f64; 4], b: [f64; 4]) -> [f64; 4] {
    let ([ax, ay, az, aw], [bx, by, bz, bw]) = (a, b);
    [
        aw * bx + ax * bw + ay * bz - az * by,
        aw * by - ax * bz + ay * bw + az * bx,
        aw * bz + ax * by - ay * bx + az * bw,
        aw * bw - ax * bx - ay * by - az * bz,
    ]
}

/// The message said once an attachment is Valve's own, turned by an exporter's change of frame.
pub const AXES_TURNED: &str = "that is Valve's attachment with its axes in another order: it came through a Y-up/Z-up swap on export (a compile of Valve's own attachments keeps Valve's numbers)";

/// Whether our attachment is Valve's with its axes turned round: at Valve's position with x, y, z read as y, z, x (or
/// z, x, y), or at Valve's position (the origin, often) with Valve's rotation turned by the same 120 degrees about
/// (1, 1, 1). The mark of an export that moved the attachments between a Y-up and a Z-up frame. Said beside the
/// finding, since the cause and the fix are the same for every attachment it explains.
fn axes_turned(ours: &str, valve: &str) -> Option<&'static str> {
    let (o, v) = (first_position(ours)?, first_position(valve)?);
    let near = |a: &[f64], b: &[f64]| a.iter().zip(b).all(|(x, y)| (x - y).abs() <= 0.02);
    // the two cycles of the axes: where Valve's position goes, and the rotation that takes it there
    let cycles: [([f64; 3], [f64; 4]); 2] = [([v[1], v[2], v[0]], [-0.5, -0.5, -0.5, 0.5]), ([v[2], v[0], v[1]], [0.5, 0.5, 0.5, 0.5])];
    let at_origin = v.iter().all(|x| x.abs() <= 0.02);
    if !near(&o, &v) && !at_origin && cycles.iter().any(|(p, _)| near(&o, p)) {
        return Some(AXES_TURNED);
    }
    // the same place, the rotation turned: compared up to sign, since q and -q are one rotation
    let (Some(qo), Some(qv)) = (first_rotation(ours), first_rotation(valve)) else { return None };
    let same_q = |a: [f64; 4], b: [f64; 4]| near(&a, &b) || near(&a, &b.map(|x| -x));
    let turned = cycles.iter().any(|(p, c)| (near(&o, p) || near(&o, &v)) && same_q(qo, qmul(*c, qv)));
    (turned && !same_q(qo, qv)).then_some(AXES_TURNED)
}

/// An attachment signature as a person reads it: only the influences it has, so `on root_motion at 0,46,0;0,0,0;0,0,0
/// rot ...;0,0,0,1;0,0,0,1` (three slots, one used) reads `on root_motion at 0,46,0 rot ...`.
pub fn show_attachment(sig: &str) -> String {
    let Some((head, rest)) = sig.split_once(" at ") else { return sig.to_string() };
    let Some((at, rot)) = rest.split_once(" rot ") else { return sig.to_string() };
    let used = head.trim_start_matches("on ").split('+').filter(|s| !s.is_empty()).count().max(1);
    let take = |s: &str| s.split(';').take(used).collect::<Vec<_>>().join(";");
    format!("{head} at {} rot {}", take(at), take(rot))
}

/// Whether two attachment or hitbox signatures say the same thing: the words equal and every number within one step of
/// the two decimals they are written to. A re-export writes Valve's 5.774999618 as 5.775000095, one float32 step that
/// rounds to 5.78 where Valve's rounds to 5.77 (a community skin's pelvis capsule, 6871); a step of 0.01 units is a
/// quarter of a millimetre.
pub fn same_signature(a: &str, b: &str) -> bool {
    let toks = |s: &str| s.split([' ', ',', ';']).filter(|t| !t.is_empty()).map(String::from).collect::<Vec<_>>();
    let (x, y) = (toks(a), toks(b));
    x.len() == y.len()
        && x.iter().zip(&y).all(|(p, q)| match (p.parse::<f64>(), q.parse::<f64>()) {
            (Ok(m), Ok(n)) => (m - n).abs() <= 0.0101,
            _ => p == q,
        })
}

/// Contract breaches of `ours` against Valve's model (M062). Added bones are allowed (cloth chains, model-only
/// bones; docs/reference/engine.md, the compiler); every Valve bone, attachment and hitbox must be there, unchanged; no hitbox may be added.
pub fn contract_breaches(ours: &Contract, valve: &Contract) -> Vec<String> {
    let mut out = Vec::new();
    let find = |list: &[(String, String)], n: &str| list.iter().filter(|(k, _)| k == n).map(|(_, s)| s.clone()).collect::<Vec<_>>();
    // `$`-named bones are the compiler's cloth nodes (`$cloth_m0p12`, `$cc<joint>_0`), generated from the cloth
    // setup, not animated or attached to: a replacement with its own cloth has its own.
    for (n, parent) in valve.bones.iter().filter(|(n, _)| !n.starts_with('$')) {
        match ours.bones.iter().find(|(k, _)| k == n) {
            None => out.push(format!("bone '{n}' missing")),
            Some((_, p)) if p != parent => out.push(format!("bone '{n}' reparented: '{p}', Valve's '{parent}'")),
            _ => {}
        }
    }
    for (kind, mine, theirs) in [("attachment", &ours.attachments, &valve.attachments), ("hitbox", &ours.hitboxes, &valve.hitboxes)] {
        let mut done = std::collections::BTreeSet::new();
        for (n, sig) in theirs.iter() {
            if !done.insert(n.clone()) {
                continue;
            }
            // a name can be carried by several that differ (Haze has two hitboxes called spine_1, a box and a
            // capsule), and every mesh of a model repeats its attachments: the ones of a name are compared as a set,
            // whatever number of copies each side holds
            let have = find(mine, n);
            let want = find(theirs, n);
            if have.is_empty() {
                out.push(format!("{kind} '{n}' missing"));
            } else if let Some(bad) = have.iter().find(|h| !want.iter().any(|w| same_signature(h, w))) {
                let theirs = want.iter().find(|w| !have.iter().any(|h| same_signature(h, w))).unwrap_or(sig);
                if kind == "attachment" {
                    let turned = axes_turned(bad, theirs);
                    out.push(format!("{kind} '{n}' changed: {} (Valve's {}){}", show_attachment(bad), show_attachment(theirs), turned.map(|t| format!("; {t}")).unwrap_or_default()));
                } else {
                    out.push(format!("{kind} '{n}' changed: {bad} (Valve's {theirs})"));
                }
            } else if let Some(lost) = want.iter().find(|w| !have.iter().any(|h| same_signature(h, w))) {
                let lost = if kind == "attachment" { show_attachment(lost) } else { lost.clone() };
                out.push(format!("{kind} '{n}' missing: Valve's model has another of that name, {lost}"));
            }
        }
        if kind == "hitbox" {
            let mut added = std::collections::BTreeSet::new();
            for (n, _) in mine.iter() {
                if !theirs.iter().any(|(k, _)| k == n) && added.insert(n.clone()) {
                    out.push(format!("hitbox '{n}' added"));
                }
            }
        }
    }
    out
}

/// Said of a moved attachment that stays on the body: the mark of an M062 finding that is a warning (`severity`).
pub const COSMETIC_MOVE: &str = "a cosmetic move";

/// The contract breaches of a model, with each changed attachment judged by where it now sits (`reach::judge`):
/// `ours` and `valve` are the two models' bytes. A changed attachment on the body that stays as near it as Valve keeps
/// any is a cosmetic move, said with its distances; one past that line, one held in the hero's own frame, and one
/// that could not be measured stay contract breaks. Every other breach (bones, missing attachments, hitboxes) is
/// returned as it is.
pub fn judge_breaches(breaches: Vec<String>, ours: &[u8], valve: &[u8]) -> Vec<String> {
    use crate::reach::{self, Move};
    let rate = crate::mesh::rules::ATTACH_OFF_BODY_VALVE;
    let changed = |m: &str| m.strip_prefix("attachment '").and_then(|r| r.split_once("' changed: ")).map(|(n, rest)| (n.to_string(), rest.to_string()));
    if !breaches.iter().any(|m| changed(m).is_some()) {
        return breaches;
    }
    let (o, v) = (reach::read(ours), reach::read(valve));
    let pct = |x: f64| format!("{:.1}%", 100.0 * x);
    breaches
        .into_iter()
        .map(|m| {
            let Some((n, rest)) = changed(&m) else { return m };
            // the signatures, then any shared explanation (`; ...`) the breach already carries
            let (sigs, tail) = rest.split_once("; ").map_or((rest.as_str(), String::new()), |(s, t)| (s, format!("; {t}")));
            match reach::judge(&n, &o, &v, rate) {
                Move::Cosmetic { by, off, line } => format!(
                    "attachment '{n}' moved {by:.1} units, {off:.1} off the body: {sigs}; {COSMETIC_MOVE}, within {line:.1} units of the body ({} of the model's size, past which Valve keeps no attachment on any hero's body){tail}",
                    pct(rate)
                ),
                Move::OffBody { by, off, line } => format!(
                    "attachment '{n}' moved {by:.1} units, {off:.1} off the body: {sigs}; past {line:.1} units off the body ({} of the model's size, further than Valve keeps any attachment on any hero's body): what the game spawns there shows away from the hero{tail}",
                    pct(rate)
                ),
                Move::HeroFrame { by } => format!(
                    "attachment '{n}' changed: {sigs}; it moved {by:.1} units in the hero's own frame (it rides the root, not the body: the frame Valve places its camera poses, aim and zipline points in){tail}"
                ),
                Move::Unmeasured(why) => format!("attachment '{n}' changed: {sigs}; where it sits against the body could not be measured ({why}){tail}"),
            }
        })
        .collect()
}

/// The severity of a contract finding (M062): a cosmetic move of an attachment on the body is a warning; every other
/// breach keeps the rule's own, an error.
pub fn contract_severity(message: &str) -> crate::Severity {
    if message.starts_with("attachment '") && message.contains(&format!("; {COSMETIC_MOVE}, ")) {
        crate::Severity::Warn
    } else {
        crate::rule("M062").severity
    }
}

// ------------------------------------------------------------------------------- particle class lists

/// The function lists of a particle system that lint checks, and `m_Constraints`, which it reads to explain a
/// misplaced constraint. The engine looks each element's `_class` up in the list's own registry: an operator
/// class in `m_Constraints` and a constraint class in `m_Operators` are both "not found while loading".
pub const PARTICLE_LISTS: &[&str] = &["m_Operators", "m_Initializers", "m_Emitters", "m_Renderers", "m_ForceGenerators"];
pub const CONSTRAINT_LIST: &str = "m_Constraints";

/// Which `_class` values Valve uses in each list, across every particle in the game build.
pub type ParticleClasses = std::collections::HashMap<String, std::collections::BTreeSet<String>>;

/// Add one particle system's classes per list (the checked lists and `m_Constraints`).
pub fn collect_particle_classes(v: &Value, into: &mut ParticleClasses) {
    for list in PARTICLE_LISTS.iter().chain([&CONSTRAINT_LIST]) {
        for op in v.get(list).map(Value::as_array).unwrap_or(&[]) {
            let c = class(op);
            if !c.is_empty() {
                into.entry(list.to_string()).or_default().insert(c.to_string());
            }
        }
    }
}

pub fn particle_classes_tsv(c: &ParticleClasses, build: &str) -> String {
    let mut t = format!("# magician particle classes build={build}\n");
    let mut lists: Vec<_> = c.keys().collect();
    lists.sort();
    for l in lists {
        for cl in &c[l] {
            t.push_str(&format!("{l}\t{cl}\n"));
        }
    }
    t
}

pub fn parse_particle_classes(text: &str) -> ParticleClasses {
    let mut c = ParticleClasses::new();
    for line in text.lines().filter(|l| !l.starts_with('#')) {
        if let Some((l, cl)) = line.split_once('\t') {
            c.entry(l.to_string()).or_default().insert(cl.trim().to_string());
        }
    }
    c
}

/// Elements of a system's function lists whose class Valve never uses in that list (M083), each with the
/// reason: a constraint in the wrong list crashed the game (one skin: 13 particles with C_OP_WorldTraceConstraint
/// in m_Operators; "Function ... not found while loading particle system", then an access violation executing a
/// null pointer in particles.dll when the effect played).
pub fn particle_class_problems(v: &Value, valve: &ParticleClasses) -> Vec<String> {
    let mut out = Vec::new();
    let empty = std::collections::BTreeSet::new();
    for list in PARTICLE_LISTS {
        for (i, op) in v.get(list).map(Value::as_array).unwrap_or(&[]).iter().enumerate() {
            let c = class(op);
            if c.is_empty() || valve.get(*list).unwrap_or(&empty).contains(c) {
                continue;
            }
            let home: Vec<&str> =
                PARTICLE_LISTS.iter().chain([&CONSTRAINT_LIST]).filter(|l| valve.get(**l).is_some_and(|s| s.contains(c))).copied().collect();
            let msg = if home.contains(&CONSTRAINT_LIST) {
                format!("{list}[{i}] is {c}, which belongs in m_Constraints; the game executes a null operator and crashes particles.dll")
            } else if !home.is_empty() {
                format!("{list}[{i}] is {c}, which Valve only uses in {}; here the game doesn't find it (not found while loading): it is skipped or crashes", home.join(", "))
            } else {
                format!("{list}[{i}] is {c}: the game doesn't know this class (not found while loading); it will be skipped or crash")
            };
            out.push(msg);
        }
    }
    out
}

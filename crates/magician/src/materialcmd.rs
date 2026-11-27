//! `magician material`: a material as data. `inspect` shows one (a `.vmat` source, a compiled `.vmat_c`, one in a
//! pak or Valve's in the installed game); `diff` holds two side by side in the compiled material's terms; `export`
//! writes a portable preset of its setup and `apply` writes that setup as a new `.vmat` for another material path,
//! texture paths rewritten and the inputs (so the channel packing) kept; `verify` holds every compiled material of an
//! addon to its source, down to the pixels of each packed channel.
//!
//! A material is named as a file (`x.vmat`, `x.vmat_c`), as `<pak.vpk>:<path>`, or as `valve:<path>` (the installed
//! game's file at that path). The packing (which input lands in which slot and channel) is `resource::material::ROUTES`.

use crate::gamefiles::GameFiles;
use resource::material::{self as mat, Material};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

pub const USAGE: &str = "material inspect|diff|export|apply|verify ... (`magician help material`)";

/// Where a material's related files (its textures, its source images) are read from, first that has one.
enum Src {
    Dir(PathBuf),
    Pak(vpk::Vpk),
    Game(Box<GameFiles>),
}

struct Loaded {
    m: Material,
    /// The material's game path (`materials/x/y.vmat`).
    path: String,
    label: String,
    srcs: Vec<Src>,
}

impl Loaded {
    fn read(&self, rel: &str) -> Option<Vec<u8>> {
        let rel = rel.replace('\\', "/");
        for s in &self.srcs {
            let got = match s {
                Src::Dir(d) => std::fs::read(d.join(crate::build::native(&rel))).ok().or_else(|| std::fs::read(d.join(crate::build::native(&rel.to_ascii_lowercase()))).ok()),
                Src::Pak(p) => p.find(&rel).or_else(|| p.find(&rel.to_ascii_lowercase())).and_then(|e| p.read(e).ok()),
                Src::Game(g) => lint::Game::read(g.as_ref(), &rel.to_ascii_lowercase()),
            };
            if got.is_some() {
                return got;
            }
        }
        None
    }
}

/// `materials/x/y.vmat` for a typed path (`.vmat_c`, backslashes, upper case kept).
fn source_path(p: &str) -> String {
    let p = p.replace('\\', "/");
    p.strip_suffix("_c").map(str::to_string).unwrap_or(p)
}

/// The game-relative part of a file path: from its last folder named like a top game folder.
fn game_rel(file: &Path) -> Option<(PathBuf, String)> {
    let comps: Vec<String> = file.components().map(|c| c.as_os_str().to_string_lossy().to_string()).collect();
    let tops = ["materials", "models", "particles", "panorama", "maps"];
    let i = comps.iter().rposition(|c| tops.contains(&c.to_ascii_lowercase().as_str()))?;
    let root: PathBuf = file.components().take(i).collect();
    Some((root, comps.get(i..)?.join("/")))
}

fn game_src(args: &[String]) -> Option<Src> {
    crate::game_dir(args).ok().filter(|g| g.is_dir()).map(|g| Src::Game(Box::new(GameFiles::open(&g, None))))
}

fn load(spec: &str, args: &[String]) -> Result<Loaded, String> {
    if let Some(p) = spec.strip_prefix("valve:") {
        let g = crate::game_dir(args)?;
        let gf = GameFiles::open(&g, None);
        let path = lint::deep::compiled(&source_path(p)).to_ascii_lowercase();
        let data = lint::Game::read(&gf, &path).ok_or_else(|| format!("the game has no {path}"))?;
        let m = mat::from_compiled(&data)?;
        return Ok(Loaded { m, path: source_path(&path), label: format!("Valve's {path}"), srcs: vec![Src::Game(Box::new(gf))] });
    }
    let lower = spec.to_ascii_lowercase();
    if let Some(i) = lower.find(".vpk:") {
        let (vp, rest) = (spec.get(..i + 4).unwrap_or(spec), spec.get(i + 5..).unwrap_or(""));
        let pak = vpk::Vpk::open(vp).map_err(|e| format!("{vp}: {e}"))?;
        let path = crate::pak_path(&|x| pak.find(x).is_some(), &lint::deep::compiled(&source_path(rest)));
        let e = pak.find(&path).ok_or_else(|| crate::not_in(&path, Path::new(vp)))?;
        let data = pak.read(e).map_err(|e| e.to_string())?;
        let m = mat::from_compiled(&data).map_err(|e| format!("{path}: {e}"))?;
        let mut srcs = vec![Src::Pak(pak)];
        srcs.extend(game_src(args));
        return Ok(Loaded { m, path: source_path(&path), label: format!("{path} in {vp}"), srcs });
    }
    let file = PathBuf::from(spec);
    let bytes = std::fs::read(&file).map_err(|e| format!("{spec}: {e}"))?;
    if lower.ends_with(".vmat_c") {
        let m = mat::from_compiled(&bytes).map_err(|e| format!("{spec}: {e}"))?;
        let path = if m.name.is_empty() { game_rel(&file).map(|(_, r)| source_path(&r)).unwrap_or_default() } else { m.name.clone() };
        let depth = path.split('/').count();
        let root: PathBuf = file.ancestors().nth(depth).map(Path::to_path_buf).unwrap_or_default();
        let mut srcs = vec![Src::Dir(root)];
        srcs.extend(game_src(args));
        return Ok(Loaded { m, path, label: spec.to_string(), srcs });
    }
    let text = String::from_utf8_lossy(&bytes);
    let m = mat::from_source(&text).map_err(|e| format!("{spec}: {e}"))?;
    let (root, path) = game_rel(&file).ok_or_else(|| format!("{spec}: no materials/ or models/ folder in its path, so its game path is not known"))?;
    Ok(Loaded { m, path, label: spec.to_string(), srcs: vec![Src::Dir(root)] })
}

/// A compiled texture's header, as `512x512 BC7 8 mips` (YCoCg named).
fn tex_desc(data: &[u8]) -> Option<(resource::texture::TexInfo, String)> {
    let t = resource::texture::info(data)?;
    let ycocg = if mark::addon::is_ycocg(data) { " YCoCg" } else { "" };
    Some((t, format!("{}x{} {}{ycocg} {} mip{}", t.width, t.height, t.format_name(), t.mips, if t.mips == 1 { "" } else { "s" })))
}

fn fmt4(v: &[f64; 4]) -> String {
    format!("[{}]", v.iter().map(|x| num(*x)).collect::<Vec<_>>().join(" "))
}

/// A float as the compiler must read it: with a decimal point, or it compiles as an integer (M151).
fn fnum(x: f64) -> String {
    let s = num(x);
    if s.contains('.') || s.contains('e') { s } else { format!("{s}.0") }
}

fn num(x: f64) -> String {
    let s = format!("{x:.6}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".into() } else { s.to_string() }
}

/// The meanings a compiled slot's channels carry under the material's features.
fn slot_meanings(m: &Material, slot: &str) -> Vec<String> {
    // the colour's alpha is coverage only when the material is translucent or alpha-tested, else metalness
    let cut = ["F_TRANSLUCENT", "F_ALPHA_TEST"].iter().any(|f| m.int(f).unwrap_or(0) != 0);
    mat::ROUTES
        .iter()
        .filter(|r| r.input != "TextureTranslucency" || cut)
        .filter_map(|r| {
            let (s, c) = r.place(m);
            (s.eq_ignore_ascii_case(slot)).then(|| format!("{c}: {} ({})", r.meaning, r.input))
        })
        .collect()
}

fn is_default(tex: &str) -> bool {
    tex.starts_with("materials/default/")
}

/// The names Valve's materials of this shader never set, when the game can be read.
fn unread(m: &Material, args: &[String]) -> Option<Vec<String>> {
    let g = crate::game_dir(args).ok()?;
    let build = crate::game_build(&g).ok()?;
    let mp = crate::gamefiles::material_params(&g, &crate::reference_root().join(&build).join("material_params.tsv"), &build).ok()?;
    let compiled = if m.compiled { m.clone() } else { as_compiled_names(m) };
    lint::matparams::unread(&compiled, &mp)
}

/// A source's names as the compiler writes them: parameters and features as they are, each input's slot instead of
/// the input, and a colour input as the vector parameter it compiles to.
fn as_compiled_names(src: &Material) -> Material {
    let mut m = src.clone();
    m.textures = src.textures.iter().filter_map(|(k, _)| mat::expected_slot(src, k).map(|(s, _, _)| (s.to_string(), String::new()))).collect();
    m.vectors.extend(src.constants.iter().cloned());
    m.compiled = true;
    m
}

pub fn cmd_material(args: &[String]) -> crate::R {
    let pos = crate::positional(args);
    let verb = pos.first().map(|s| s.as_str()).ok_or(USAGE)?;
    let rest: Vec<String> = args.iter().filter(|a| a.as_str() != verb).cloned().collect();
    match verb {
        "inspect" => inspect(&rest),
        "diff" => diff(&rest),
        "export" => export(&rest),
        "apply" => apply(&rest),
        "verify" => verify(&rest),
        v => Err(crate::Stop::Usage(format!("material {v:?}: the verbs are inspect, diff, export, apply and verify"))),
    }
}

fn json() -> bool {
    !crate::TEXT.load(std::sync::atomic::Ordering::Relaxed)
}

fn inspect(args: &[String]) -> crate::R {
    let pos = crate::positional(args);
    let spec = pos.first().ok_or("material inspect <x.vmat | x.vmat_c | pak.vpk:path | valve:path>")?;
    let l = load(spec, args)?;
    let m = &l.m;
    let unread = unread(m, args);
    let mut slots = Vec::new();
    for (slot, tex) in &m.textures {
        let desc = if m.compiled { l.read(&format!("{tex}_c")).and_then(|d| tex_desc(&d)).map(|x| x.1) } else { None };
        slots.push((slot.clone(), tex.clone(), desc));
    }
    if json() {
        use crate::jsonout::{arr, Obj};
        let kv = |v: &[(String, String)]| arr(v.iter().map(|(k, x)| Obj::new().str("name", k).str("value", x).build()));
        let o = Obj::new()
            .str("schema", "magician.material/1")
            .str("material", &l.path)
            .str("from", &l.label)
            .bool("compiled", m.compiled)
            .str("shader", &m.shader)
            .raw("ints", arr(m.ints.iter().map(|(k, v)| Obj::new().str("name", k).int("value", *v).build())))
            .raw("floats", arr(m.floats.iter().map(|(k, v)| Obj::new().str("name", k).num("value", *v).build())))
            .raw("vectors", arr(m.vectors.iter().map(|(k, v)| Obj::new().str("name", k).raw("value", arr(v.iter().map(|x| crate::jsonout::num(*x)))).build())))
            .raw(
                "textures",
                arr(slots.iter().map(|(s, t, d)| {
                    let (slot, ch) = if m.compiled { (s.as_str(), String::new()) } else { mat::expected_slot(m, s).map(|(a, b, _)| (a, b.to_string())).unwrap_or(("", String::new())) };
                    Obj::new()
                        .str("name", s)
                        .str("value", t)
                        .opt_str("texture", d.as_deref())
                        .str("slot", slot)
                        .str("channels", &ch)
                        .strs("holds", &if m.compiled { slot_meanings(m, s) } else { mat::route_of(s).map(|r| vec![r.meaning.to_string()]).unwrap_or_default() })
                        .build()
                })),
            )
            .raw("constants", arr(m.constants.iter().map(|(k, v)| Obj::new().str("name", k).raw("value", arr(v.iter().map(|x| crate::jsonout::num(*x)))).build())))
            .raw("dynamic", kv(&m.dynamic))
            .strs("inputs", &m.inputs)
            .strs("render_attributes", &m.render_attributes)
            .raw("other", kv(&m.other))
            .raw("unread", unread.as_ref().map(|u| grade::json::strs(u)).unwrap_or_else(|| "null".into()))
            .build();
        outln!("{o}");
        return Ok(ExitCode::SUCCESS);
    }
    outln!("material\t{}\t{}{}", l.path, l.label, if m.compiled { " (compiled)" } else { " (source)" });
    outln!("shader\t{}", m.shader);
    for (k, v) in &m.ints {
        outln!("{}\t{k}\t{v}", if k.starts_with("F_") { "feature" } else { "int" });
    }
    for (k, v) in &m.floats {
        outln!("float\t{k}\t{}", num(*v));
    }
    for (k, v) in &m.vectors {
        outln!("vector\t{k}\t{}", fmt4(v));
    }
    for (s, t, d) in &slots {
        if m.compiled {
            let what = slot_meanings(m, s);
            outln!("slot\t{s}\t{t}\t{}\t{}", d.as_deref().unwrap_or(if is_default(t) { "engine default" } else { "not found" }), if what.is_empty() { "not a pbr.vfx slot Magician knows".to_string() } else { what.join("; ") });
        } else {
            match mat::expected_slot(m, s) {
                Some((slot, ch, r)) => outln!("input\t{s}\t{t}\t-> {slot}.{ch}\t{}", r.meaning),
                None => outln!("input\t{s}\t{t}\t-> ?\tnot a pbr.vfx input Magician knows"),
            }
        }
    }
    for (k, v) in &m.constants {
        let to = mat::expected_slot(m, k).map(|(s, c, _)| format!("-> {s}.{c} (a generated one-colour texture) and the vector {k}")).unwrap_or_default();
        outln!("constant\t{k}\t{}\t{to}", fmt4(v));
    }
    for (k, v) in &m.dynamic {
        outln!("dynamic\t{k}\t{}", v.replace('\n', "\\n"));
    }
    for a in &m.render_attributes {
        outln!("reads\t{a}");
    }
    for i in &m.inputs {
        outln!("compiled from\t{i}");
    }
    for (k, v) in &m.other {
        outln!("other\t{k}\t{v}");
    }
    match unread {
        Some(u) if !u.is_empty() => outln!("unread\t{}\tno Valve material of {} sets {} on this build: the compiler keeps any name, so the shader most likely ignores {} (M150)", u.join(", "), m.shader, if u.len() == 1 { "it" } else { "them" }, if u.len() == 1 { "it" } else { "them" }),
        Some(_) => {}
        None => outln!("unread\tnot checked (no game, or Valve ships no material of {})", m.shader),
    }
    Ok(ExitCode::SUCCESS)
}

/// A material in the compiled material's terms, for a diff: every name with its value as text, and each slot with
/// what fills it (an engine default, a generated colour, a texture with its header).
fn terms(l: &Loaded) -> Vec<(String, String)> {
    let m = if l.m.compiled { l.m.clone() } else { as_compiled_names(&l.m) };
    let mut out = vec![("shader".to_string(), m.shader.to_ascii_lowercase())];
    out.extend(m.ints.iter().map(|(k, v)| (k.clone(), if k.starts_with("F_") { v.to_string() } else { format!("int {v}") })));
    out.extend(m.floats.iter().map(|(k, v)| (k.clone(), format!("float {}", num(*v)))));
    out.extend(m.vectors.iter().map(|(k, v)| (k.clone(), fmt4(v))));
    for (slot, tex) in &m.textures {
        let v = if !l.m.compiled {
            "from the source's inputs".to_string()
        } else if is_default(tex) {
            format!("engine default {}", tex.trim_start_matches("materials/default/"))
        } else if tex.contains("_vmat_g_t") {
            "a generated one-colour texture".to_string()
        } else {
            l.read(&format!("{tex}_c")).and_then(|d| tex_desc(&d)).map(|x| format!("a texture, {}", x.1)).unwrap_or_else(|| "a texture (not found)".into())
        };
        out.push((slot.clone(), v));
    }
    out.extend(m.dynamic.iter().map(|(k, _)| (format!("dynamic {k}"), "set".to_string())));
    out.sort_by_key(|(k, _)| k.to_ascii_lowercase());
    out
}

fn diff(args: &[String]) -> crate::R {
    let pos = crate::positional(args);
    let a = pos.first().ok_or("material diff <a> <b> | <a> --against valve")?;
    let la = load(a, args)?;
    let lb = match (pos.get(1), crate::flag(args, "--against").as_deref()) {
        (Some(b), None) => load(b, args)?,
        (None, Some("valve")) => load(&format!("valve:{}", la.path), args)?,
        _ => return Err(crate::Stop::Usage("material diff takes a second material or --against valve".into())),
    };
    let (ta, tb) = (terms(&la), terms(&lb));
    let get = |t: &[(String, String)], k: &str| t.iter().find(|(x, _)| x.eq_ignore_ascii_case(k)).map(|(_, v)| v.clone());
    let mut keys: Vec<String> = ta.iter().chain(tb.iter()).map(|(k, _)| k.clone()).collect();
    keys.sort_by_key(|k| k.to_ascii_lowercase());
    keys.dedup_by(|x, y| x.eq_ignore_ascii_case(y));
    let mut rows = Vec::new();
    for k in &keys {
        let (va, vb) = (get(&ta, k), get(&tb, k));
        let state = match (&va, &vb) {
            (Some(x), Some(y)) if x == y => "same",
            (Some(_), Some(_)) => "differs",
            (Some(_), None) => "only-a",
            (None, Some(_)) => "only-b",
            _ => continue,
        };
        rows.push((k.clone(), state, va, vb));
    }
    let differ = rows.iter().filter(|r| r.1 != "same").count();
    if json() {
        use crate::jsonout::{arr, Obj};
        outln!(
            "{}",
            Obj::new()
                .str("schema", "magician.material-diff/1")
                .str("a", &la.label)
                .str("b", &lb.label)
                .int("differ", differ as i64)
                .raw("rows", arr(rows.iter().map(|(k, s, x, y)| Obj::new().str("name", k).str("state", s).opt_str("a", x.as_deref()).opt_str("b", y.as_deref()).build())))
                .build()
        );
    } else {
        outln!("a\t{}", la.label);
        outln!("b\t{}", lb.label);
        for (k, s, x, y) in &rows {
            if *s != "same" || args.iter().any(|a| a == "--verbose") {
                outln!("{s}\t{k}\t{}\t{}", x.as_deref().unwrap_or("-"), y.as_deref().unwrap_or("-"));
            }
        }
        outln!("{} name(s) compared, {differ} differ{}", rows.len(), if args.iter().any(|a| a == "--verbose") { "" } else { " (--verbose lists the same ones too)" });
    }
    Ok(if differ == 0 { ExitCode::SUCCESS } else { ExitCode::from(crate::cli::BROKEN) })
}

/// Words in a source image's name that say which input it is, longest first so `_sheenrough` is not `_sheen`.
const ROLE_WORDS: &[(&str, &[&str])] = &[
    ("TextureSheenRoughness", &["_sheenrough", "_sheen_rough"]),
    ("TextureSelfIllumMask", &["_selfillum", "_illum", "_emissive"]),
    ("TextureRimLightMask", &["_rim"]),
    ("TextureSheenColor", &["_sheen"]),
    ("TextureMetalness", &["_metal"]),
    ("TextureRoughness", &["_rough"]),
    ("TextureNormal", &["_normal", "_nrm"]),
    ("TextureAmbientOcclusion", &["_ao", "_occlusion"]),
    ("TextureTranslucency", &["_trans", "_opacity", "_alpha"]),
    ("TextureTintMask", &["_tint"]),
    ("TextureNprOutlineMask", &["_outline"]),
    ("TextureColor", &["_color", "_albedo", "_basecolor", "_diffuse"]),
];

/// The source inputs a compiled material was most likely made from: each source image its edit info lists, named by
/// the input its file name says (a guess, said as one).
fn guessed_inputs(m: &Material) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for img in &m.inputs {
        let stem = img.rsplit('/').next().unwrap_or(img).rsplit_once('.').map(|x| x.0).unwrap_or(img).to_ascii_lowercase();
        if let Some((role, _)) = ROLE_WORDS.iter().find(|(_, w)| w.iter().any(|w| stem.ends_with(w) || stem.contains(&format!("{w}_")))) {
            if !out.iter().any(|(r, _)| r == role) {
                out.push((role.to_string(), img.clone()));
            }
        }
    }
    out.sort();
    out
}

/// A path written against the material's own: `{base}_color.png` for one beside it with its name, `{dir}/x.png` for
/// one in its folder, the path itself otherwise.
fn tokenise(p: &str, material: &str) -> String {
    let base = material.strip_suffix(".vmat").unwrap_or(material);
    let dir = base.rsplit_once('/').map(|x| x.0).unwrap_or("");
    let (pl, bl, dl) = (p.to_ascii_lowercase(), base.to_ascii_lowercase(), dir.to_ascii_lowercase());
    if pl.starts_with(&bl) && pl.len() > bl.len() {
        format!("{{base}}{}", p.get(base.len()..).unwrap_or(""))
    } else if !dl.is_empty() && pl.starts_with(&format!("{dl}/")) && !pl.get(dl.len() + 1..).unwrap_or("").contains('/') {
        format!("{{dir}}{}", p.get(dir.len()..).unwrap_or(""))
    } else {
        p.to_string()
    }
}

fn untokenise(p: &str, material: &str) -> String {
    let base = material.strip_suffix(".vmat").unwrap_or(material);
    let dir = base.rsplit_once('/').map(|x| x.0).unwrap_or("");
    p.replace("{base}", base).replace("{dir}", dir)
}

fn esc(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\t', "\\t").replace('\n', "\\n").replace('\r', "")
}

fn unesc(s: &str) -> String {
    let mut out = String::new();
    let mut it = s.chars();
    while let Some(c) = it.next() {
        if c == '\\' {
            match it.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some(x) => out.push(x),
                None => {}
            }
        } else {
            out.push(c);
        }
    }
    out
}

pub const PRESET_HEAD: &str = "magician-material-preset\t1";

/// The preset text of a material: its setup, with paths written against its own (`tokenise`).
fn preset(l: &Loaded) -> String {
    let m = &l.m;
    let mut o = format!("{PRESET_HEAD}\n# {} ({})\n", l.path, l.label);
    o += &format!("shader\t{}\n", m.shader);
    for (k, v) in &m.ints {
        o += &format!("{}\t{k}\t{v}\n", if k.starts_with("F_") { "feature" } else { "int" });
    }
    for (k, v) in &m.floats {
        o += &format!("float\t{k}\t{}\n", fnum(*v));
    }
    let constants: Vec<&String> = m.constants.iter().map(|(k, _)| k).collect();
    for (k, v) in &m.vectors {
        // a colour input compiles to a vector of its own name: carry it as the input it was
        if m.compiled && k.starts_with("Texture") {
            o += &format!("constant\t{k}\t{}\n", fmt4(v));
        } else if !constants.contains(&k) {
            o += &format!("vector\t{k}\t{}\n", fmt4(v));
        }
    }
    for (k, v) in &m.constants {
        o += &format!("constant\t{k}\t{}\n", fmt4(v));
    }
    if m.compiled {
        for (role, img) in guessed_inputs(m) {
            let (slot, ch) = mat::route_of(&role).map(|r| r.place(m)).unwrap_or(("", ""));
            o += &format!("input\t{role}1\t{}\t# guessed from the compiled-from image {img}; lands in {slot}.{ch}\n", tokenise(&img, &l.path));
        }
        for (slot, tex) in &m.textures {
            if let Some((_, d)) = (!is_default(tex)).then(|| l.read(&format!("{tex}_c"))).flatten().and_then(|d| tex_desc(&d)) {
                o += &format!("expect\t{slot}\t{d}\n");
            }
        }
    } else {
        for (k, v) in &m.textures {
            let t = tokenise(v, &l.path);
            let (slot, ch) = mat::expected_slot(m, k).map(|(s, c, _)| (s, c)).unwrap_or(("?", ""));
            o += &format!("input\t{k}\t{t}\t# lands in {slot}.{ch}\n");
            // what the compiler reads beside an image (its settings), and a .vtex given as an input
            if v.to_ascii_lowercase().ends_with(".vtex") {
                if let Some(b) = l.read(v) {
                    o += &format!("file\t{t}\t{}\n", esc(&tokenise_text(&String::from_utf8_lossy(&b), &l.path)));
                }
            } else if let Some(stem) = v.rsplit_once('.').map(|x| x.0) {
                let side = format!("{stem}.txt");
                if let Some(b) = l.read(&side) {
                    o += &format!("file\t{}\t{}\n", tokenise(&side, &l.path), esc(&String::from_utf8_lossy(&b)));
                }
            }
        }
    }
    for (k, v) in &m.dynamic {
        if !m.compiled {
            o += &format!("dynamic\t{k}\t{}\n", esc(v));
        } else {
            o += &format!("# dynamic {k}: compiled only, its expression is not in the file\n");
        }
    }
    for (k, v) in &m.other {
        o += &format!("other\t{k}\t{}\n", esc(v));
    }
    o
}

/// Paths inside a text file (a .vtex) written against the material's own.
fn tokenise_text(t: &str, material: &str) -> String {
    t.split('"').enumerate().map(|(i, s)| if i % 2 == 1 && s.contains('/') { tokenise(s, material) } else { s.to_string() }).collect::<Vec<_>>().join("\"")
}

fn export(args: &[String]) -> crate::R {
    let pos = crate::positional(args);
    let spec = pos.first().ok_or("material export <material> [--out FILE]")?;
    let l = load(spec, args)?;
    let text = preset(&l);
    match crate::flag(args, "--out") {
        Some(o) => {
            std::fs::write(&o, &text).map_err(|e| format!("{o}: {e}"))?;
            say!("wrote {o}: {} line(s), the setup of {}", text.lines().count(), l.path);
        }
        None => out!("{text}"),
    }
    Ok(ExitCode::SUCCESS)
}

/// A preset back into a source material for `material` (the new game path), plus the files it carries.
pub fn from_preset(text: &str, material: &str) -> Result<(String, Vec<(String, String)>, Vec<String>), String> {
    let mut lines = text.lines();
    if lines.next().map(str::trim_end) != Some(PRESET_HEAD) {
        return Err(format!("not a material preset (its first line is not {PRESET_HEAD:?})"));
    }
    let mut body = String::from("// Written by magician material apply from a preset.\n\"Layer0\"\n{\n");
    let mut dynamic = Vec::new();
    let mut files = Vec::new();
    let mut inputs = Vec::new();
    for (n, l) in lines.enumerate() {
        if l.trim().is_empty() || l.starts_with('#') {
            continue;
        }
        let l = l.split("\t# ").next().unwrap_or(l);
        let f: Vec<&str> = l.splitn(3, '\t').collect();
        let q = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
        match (f.first().copied(), f.get(1), f.get(2)) {
            (Some("shader"), Some(v), None) => body += &format!("\t\"shader\"\t\"{}\"\n", q(v)),
            (Some("float"), Some(k), Some(v)) => body += &format!("\t\"{}\"\t\"{}\"\n", q(k), q(&v.trim().parse::<f64>().map(fnum).unwrap_or_else(|_| v.to_string()))),
            (Some("feature" | "int" | "other"), Some(k), Some(v)) => body += &format!("\t\"{}\"\t\"{}\"\n", q(k), q(&unesc(v))),
            (Some("vector" | "constant"), Some(k), Some(v)) => body += &format!("\t\"{}\"\t\"{}\"\n", q(k), q(v)),
            (Some("input"), Some(k), Some(v)) => {
                let p = untokenise(v, material);
                body += &format!("\t\"{}\"\t\"{}\"\n", q(k), q(&p));
                inputs.push(p);
            }
            (Some("dynamic"), Some(k), Some(v)) => dynamic.push(format!("\t\t\"{}\"\t\"{}\"\n", q(k), q(&unesc(v)).replace('\n', "\\n"))),
            (Some("file"), Some(p), Some(v)) => {
                let text = untokenise(&unesc(v), material);
                // a .vtex input names the images it is made of: those are needed too
                inputs.extend(text.split('"').filter(|s| [".png", ".tga", ".psd"].iter().any(|e| s.to_ascii_lowercase().ends_with(e))).map(str::to_string));
                files.push((untokenise(p, material), text));
            }
            (Some("expect"), _, _) => {}
            _ => return Err(format!("preset line {}: {l:?} is not one this preset format has", n + 2)),
        }
    }
    if !dynamic.is_empty() {
        body += "\t\"DynamicParams\"\n\t{\n";
        dynamic.iter().for_each(|d| body += d);
        body += "\t}\n";
    }
    body += "}\n";
    Ok((body, files, inputs))
}

fn apply(args: &[String]) -> crate::R {
    let pos = crate::positional(args);
    let preset_file = pos.first().ok_or("material apply <preset> --to <.../materials/.../new.vmat> [--force]")?;
    let to = PathBuf::from(crate::flag(args, "--to").ok_or("material apply needs --to <the new .vmat>")?);
    let force = args.iter().any(|a| a == "--force");
    let (root, path) = game_rel(&to).ok_or_else(|| format!("{}: no materials/ or models/ folder in its path, so its game path is not known", to.display()))?;
    if !path.to_ascii_lowercase().ends_with(".vmat") {
        return Err(crate::Stop::Usage(format!("--to {} is not a .vmat", to.display())));
    }
    let text = std::fs::read_to_string(preset_file).map_err(|e| format!("{preset_file}: {e}"))?;
    let (vmat, files, inputs) = from_preset(&text, &path)?;
    let mut writes = vec![(to.clone(), vmat)];
    writes.extend(files.into_iter().map(|(p, t)| (root.join(crate::build::native(&p)), t)));
    let clash: Vec<String> = writes.iter().filter(|(p, _)| p.exists()).map(|(p, _)| p.display().to_string()).collect();
    if !clash.is_empty() && !force {
        return Err(crate::Stop::Refused(format!("{} already there (--force replaces): {}", clash.len(), clash.join(", "))));
    }
    for (p, t) in &writes {
        if let Some(d) = p.parent() {
            std::fs::create_dir_all(d).map_err(|e| format!("{}: {e}", d.display()))?;
        }
        // no byte-order mark: the compiler refuses a source that starts with one
        std::fs::write(p, t.as_bytes()).map_err(|e| format!("{}: {e}", p.display()))?;
        say!("wrote {}", p.display());
    }
    let mut missing = 0;
    for i in &inputs {
        let f = root.join(crate::build::native(i));
        if !f.is_file() {
            missing += 1;
            say!("needs   {i}   (not there yet: the setup reads this image; the preset's input line says which channel it fills)");
        }
    }
    say!("{path}: the setup of the preset, {} input(s), {missing} still to make; compile it, then `magician material verify` holds the compile to it", inputs.len());
    Ok(ExitCode::SUCCESS)
}

/// How far one channel of a source image is from one channel of a compiled texture, in 0..255 levels: the mean and
/// the largest difference, plain and inverted. The source is resampled (nearest) to the compiled mip of its own size
/// when there is one, else to the largest.
struct Match {
    mean: f64,
    max: u8,
    inverted: bool,
}

fn compare(src: &mark::image::Image, sc: usize, tex: &[u8], tc: usize) -> Option<Match> {
    let t = resource::texture::info(tex)?;
    let mut pick = 0;
    for mip in 0..t.mips.max(1) as u32 {
        let (w, h) = ((t.width as usize >> mip).max(1), (t.height as usize >> mip).max(1));
        if w == src.w && h == src.h {
            pick = mip;
        }
    }
    let (w, h, px) = mark::addon::decode_mip(tex, pick)?;
    let chan: Vec<u8> = src.rgba.chunks(4).map(|p| p[sc]).collect();
    let s = if (src.w, src.h) == (w, h) { chan } else { mark::image::resample_nearest(&chan, src.w, src.h, w, h) };
    let (mut sum, mut sum_i, mut max, mut max_i) = (0u64, 0u64, 0u8, 0u8);
    for (i, p) in px.chunks(4).enumerate() {
        let (a, b) = (s[i], p[tc]);
        let d = a.abs_diff(b);
        let di = (255 - a).abs_diff(b);
        sum += d as u64;
        sum_i += di as u64;
        max = max.max(d);
        max_i = max_i.max(di);
    }
    let n = (w * h).max(1) as f64;
    let (m, mi) = (sum as f64 / n, sum_i as f64 / n);
    Some(if mi + 1.0 < m { Match { mean: mi, max: max_i, inverted: true } } else { Match { mean: m, max, inverted: false } })
}

/// What a parameter missing from the compiled file means. On CSDK 13 the compiler kept every name it was given in
/// every compile measured here (a float written without a decimal point comes out an integer instead, M151), so a
/// missing one is a stale compile or another compiler.
const DROPPED: &str = "the CSDK 13 compiler keeps every name it is given, so this compile is stale or came from another compiler; the game draws the shader's default";

const CHANNELS: [char; 4] = ['r', 'g', 'b', 'a'];
/// A packed channel further than this from its source, on average, is not that source (BC7 at 4 bits a texel keeps a
/// mask within 1 to 3 levels on Valve's and our files; a colour edit moves packed alpha up to 10 levels at an edge).
pub const CHANNEL_TOLERANCE: f64 = 6.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Level {
    Ok,
    Note,
    Warn,
    Fail,
}

impl Level {
    fn word(self) -> &'static str {
        match self {
            Level::Ok => "ok",
            Level::Note => "note",
            Level::Warn => "WARN",
            Level::Fail => "FAIL",
        }
    }
}

struct Check {
    material: String,
    level: Level,
    what: String,
    detail: String,
}

fn verify(args: &[String]) -> crate::R {
    let pos = crate::positional(args);
    let target = pos.first().ok_or("material verify <addon | content folder> [--pak FILE]")?;
    let (content, compiled_dir) = if Path::new(target.as_str()).is_dir() {
        let c = PathBuf::from(target.as_str());
        let name = c.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let out = crate::paths::csdk(crate::flag(args, "--csdk")).ok().map(|f| f.path.join("game").join("citadel_addons").join(&name));
        (c, out)
    } else {
        crate::paths::addon_name(target)?;
        let csdk = crate::paths::csdk(crate::flag(args, "--csdk"))?.path;
        (csdk.join("content").join("citadel_addons").join(target.as_str()), Some(csdk.join("game").join("citadel_addons").join(target.as_str())))
    };
    let pak = match crate::flag(args, "--pak") {
        Some(p) => Some(vpk::Vpk::open(&p).map_err(|e| format!("{p}: {e}"))?),
        None => None,
    };
    let compiled_from = match (&pak, &compiled_dir) {
        (Some(_), _) => crate::flag(args, "--pak").unwrap_or_default(),
        (None, Some(d)) => d.display().to_string(),
        (None, None) => return Err(crate::Stop::Usage("no compiled side: give --pak, or --csdk for the addon's game folder".into())),
    };
    let mut rels = Vec::new();
    crate::build::rel_files(&content, &content, &mut rels).map_err(|e| format!("{}: {e}", content.display()))?;
    let vmats: Vec<String> = rels.into_iter().filter(|r| r.to_ascii_lowercase().ends_with(".vmat")).collect();
    let read_compiled = |rel: &str| -> Option<Vec<u8>> {
        match &pak {
            Some(p) => p.find(&rel.to_ascii_lowercase()).and_then(|e| p.read(e).ok()),
            None => compiled_dir.as_ref().and_then(|d| std::fs::read(d.join(crate::build::native(rel))).ok()),
        }
    };
    let params = crate::game_dir(args).ok().and_then(|g| {
        let b = crate::game_build(&g).ok()?;
        crate::gamefiles::material_params(&g, &crate::reference_root().join(&b).join("material_params.tsv"), &b).ok()
    });
    let mut checks: Vec<Check> = Vec::new();
    for rel in &vmats {
        verify_one(rel, &content, &read_compiled, params.as_ref(), &mut checks);
    }
    let worst = checks.iter().map(|c| c.level).max().unwrap_or(Level::Ok);
    let fails = checks.iter().filter(|c| c.level == Level::Fail).count();
    let warns = checks.iter().filter(|c| c.level == Level::Warn).count();
    if json() {
        use crate::jsonout::{arr, Obj};
        outln!(
            "{}",
            Obj::new()
                .str("schema", "magician.material-verify/1")
                .str("content", &content.display().to_string())
                .str("compiled", &compiled_from)
                .int("materials", vmats.len() as i64)
                .int("fail", fails as i64)
                .int("warn", warns as i64)
                .raw("checks", arr(checks.iter().map(|c| Obj::new().str("material", &c.material).str("level", &c.level.word().to_ascii_lowercase()).str("what", &c.what).str("detail", &c.detail).build())))
                .build()
        );
    } else {
        say!("sources  {}", content.display());
        say!("compiled {compiled_from}");
        let verbose = args.iter().any(|a| a == "--verbose");
        for c in checks.iter().filter(|c| verbose || c.level > Level::Ok) {
            outln!("{:<4}  {}  {}: {}", c.level.word(), c.material, c.what, c.detail);
        }
        say!("{} material(s): {} check(s), {fails} failed, {warns} warning(s){}", vmats.len(), checks.len(), if verbose { "" } else { " (--verbose lists the ones that hold)" });
    }
    Ok(if worst == Level::Fail { ExitCode::from(crate::cli::BROKEN) } else { ExitCode::SUCCESS })
}

fn verify_one(rel: &str, content: &Path, read_compiled: &dyn Fn(&str) -> Option<Vec<u8>>, params: Option<&lint::matparams::MaterialParams>, out: &mut Vec<Check>) {
    let mut push = |level: Level, what: &str, detail: String| out.push(Check { material: rel.to_string(), level, what: what.to_string(), detail });
    let text = match std::fs::read(content.join(crate::build::native(rel))) {
        Ok(b) => String::from_utf8_lossy(&b).into_owned(),
        Err(e) => return push(Level::Fail, "source", e.to_string()),
    };
    let src = match mat::from_source(&text) {
        Ok(m) => m,
        Err(e) => return push(Level::Fail, "source", format!("does not read: {e}")),
    };
    let Some(cdata) = read_compiled(&format!("{rel}_c")) else {
        return push(Level::Fail, "compiled", "no .vmat_c: not compiled (build the addon)".into());
    };
    let c = match mat::from_compiled(&cdata) {
        Ok(m) => m,
        Err(e) => return push(Level::Fail, "compiled", format!("does not read: {e}")),
    };
    if !src.shader.eq_ignore_ascii_case(&c.shader) {
        push(Level::Fail, "shader", format!("the source asks for {}, the compiled file has {}", src.shader, c.shader));
    } else {
        push(Level::Ok, "shader", c.shader.clone());
    }
    for (k, v) in &src.ints {
        match c.int(k) {
            Some(x) if x == *v => push(Level::Ok, k, v.to_string()),
            Some(x) => push(Level::Fail, k, format!("the source sets {v}, the compiled file has {x}")),
            None if !k.starts_with("F_") => push(Level::Warn, k, format!("the source sets {v}, the compiled file has none: {DROPPED}")),
            None => push(Level::Fail, k, format!("the source sets {v}, the compiled file has no {k}")),
        }
    }
    for (k, v) in &src.floats {
        match c.float(k) {
            Some(x) if (x - v).abs() <= 1e-4 * v.abs().max(1.0) => push(Level::Ok, k, num(*v)),
            Some(x) => push(Level::Fail, k, format!("the source sets {}, the compiled file has {}", num(*v), num(x))),
            None if c.int(k).is_some() => push(Level::Warn, k, format!("the source sets {} with no decimal point, so it compiled as an integer, where Valve's materials store a float: the shader most likely draws its default instead (UNVERIFIED in game); write {} (M151)", num(*v), fnum(*v))),
            None => push(Level::Warn, k, format!("the source sets {}, the compiled file has none: {DROPPED}", num(*v))),
        }
    }
    for (k, v) in src.vectors.iter().chain(src.constants.iter()) {
        match c.vector(k) {
            Some(x) if x.iter().zip(v).all(|(a, b)| (a - b).abs() <= 1e-4) => push(Level::Ok, k, fmt4(v)),
            Some(x) => push(Level::Fail, k, format!("the source sets {}, the compiled file has {}", fmt4(v), fmt4(&x))),
            None => push(Level::Warn, k, format!("the source sets {}, the compiled file has none: {DROPPED}", fmt4(v))),
        }
    }
    for (k, _) in &src.dynamic {
        if c.dynamic.iter().any(|(n, _)| n.eq_ignore_ascii_case(k)) {
            push(Level::Ok, &format!("dynamic {k}"), "compiled".into());
        } else {
            push(Level::Fail, &format!("dynamic {k}"), "the source's expression is not in the compiled file".into());
        }
    }
    if let Some(u) = params.and_then(|p| lint::matparams::unread(&c, p)) {
        for n in u {
            let rim = if n.to_ascii_lowercase().contains("rimlight") && !n.starts_with("g_t") { "; a rim light's strength is the scene's, not the material's" } else { "" };
            push(Level::Warn, &n, format!("no Valve material of {} sets it on this build: the compiler kept it, the shader most likely ignores it (M150){rim}", c.shader));
        }
    }
    // each input: its slot, the slot's texture, and the pixels of the channels it should fill
    for (k, img) in &src.textures {
        let Some((slot, chans, route)) = mat::expected_slot(&src, k) else {
            let read = c.inputs.iter().any(|i| i.eq_ignore_ascii_case(img));
            push(Level::Note, k, format!("{img}: not a {} input Magician knows the packing of; the compile {} it", if src.shader.eq_ignore_ascii_case("pbr.vfx") { "pbr.vfx" } else { "this shader's" }, if read { "read" } else { "did not list" }));
            continue;
        };
        let Some(tex) = c.texture(slot) else {
            push(Level::Fail, k, format!("{img} should land in {slot}.{chans}, and the compiled file has no {slot}"));
            continue;
        };
        if is_default(tex) && is_default(img) {
            push(Level::Ok, k, format!("{img}: the engine default, as {slot} has"));
            continue;
        }
        if is_default(tex) {
            push(Level::Fail, k, format!("{img} should land in {slot}.{chans}, and {slot} is the engine default {tex}"));
            continue;
        }
        let Some(tdata) = read_compiled(&format!("{tex}_c")) else {
            push(Level::Fail, k, format!("{slot} names {tex}, which is not in the compiled files"));
            continue;
        };
        let desc = tex_desc(&tdata).map(|x| x.1).unwrap_or_else(|| "unreadable header".into());
        if route.input == "TextureNormal" {
            push(Level::Ok, k, format!("{img} -> {slot}.{chans} ({desc}); a normal is re-encoded hemi-octahedral, so its pixels are not compared"));
            continue;
        }
        let image_path = if img.to_ascii_lowercase().ends_with(".vtex") {
            std::fs::read_to_string(content.join(crate::build::native(img))).ok().and_then(|t| t.split('"').find(|s| [".png", ".tga", ".psd"].iter().any(|e| s.to_ascii_lowercase().ends_with(e))).map(str::to_string))
        } else {
            Some(img.clone())
        };
        let Some((image, _)) = image_path.as_ref().and_then(|p| mark::image::load(&content.join(crate::build::native(p))).ok()) else {
            push(Level::Note, k, format!("{img} -> {slot}.{chans} ({desc}); the source image can't be read here, so its pixels are not compared"));
            continue;
        };
        if route.input == "TextureRoughness" && !(image.w.is_power_of_two() && image.h.is_power_of_two()) {
            push(Level::Warn, k, format!("{img} is {}x{}: roughness mips are filtered only at power-of-two sizes (the compile fails otherwise)", image.w, image.h));
        }
        let mut worst: Option<(char, Match)> = None;
        let grey = image.rgba.chunks(4).all(|p| p[0] == p[1] && p[1] == p[2]);
        for (i, ch) in chans.chars().enumerate() {
            let Some(tc) = CHANNELS.iter().position(|x| *x == ch) else { continue };
            let sc = if grey { 0 } else { i.min(3) };
            if let Some(mm) = compare(&image, sc, &tdata, tc) {
                if worst.as_ref().is_none_or(|w| mm.mean > w.1.mean) {
                    worst = Some((ch, mm));
                }
            }
        }
        match worst {
            Some((_, mm)) if mm.mean <= CHANNEL_TOLERANCE => {
                let inv = if mm.inverted { ", inverted" } else { "" };
                let alpha = chans == "a" && mm.max > 8;
                push(if alpha { Level::Note } else { Level::Ok }, k, format!("{img} -> {slot}.{chans} ({desc}): within {:.1} levels on average, {} at most{inv}{}", mm.mean, mm.max, if alpha { "; the packed alpha moves with the colour beside it (BC7 codes them together)" } else { "" }));
            }
            Some((ch, mm)) => {
                // where is it, then? every channel of every slot texture
                let mut best: Option<(String, f64)> = None;
                for (s, t) in &c.textures {
                    let Some(d) = (!is_default(t)).then(|| read_compiled(&format!("{t}_c"))).flatten() else { continue };
                    for (tc, cn) in CHANNELS.iter().enumerate() {
                        if let Some(x) = compare(&image, 0, &d, tc) {
                            if best.as_ref().is_none_or(|b| x.mean < b.1) {
                                best = Some((format!("{s}.{cn}{}", if x.inverted { " inverted" } else { "" }), x.mean));
                            }
                        }
                    }
                }
                let found = best.filter(|b| b.1 <= CHANNEL_TOLERANCE).map(|b| format!("; it is in {} ({:.1} levels)", b.0, b.1)).unwrap_or_else(|| "; no compiled channel holds it".into());
                push(Level::Fail, k, format!("{img} should fill {slot}.{chans} ({desc}), but {slot}.{ch} is {:.1} levels off on average{found}", mm.mean));
            }
            None => push(Level::Note, k, format!("{img} -> {slot}.{chans} ({desc}); not compared (the texture did not decode)")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_preset_carries_a_setup_to_another_material() {
        let src = "\"Layer0\"\n{\n\t\"shader\"\t\"pbr.vfx\"\n\t\"F_SHEEN\"\t\"1\"\n\t\"TextureColor1\"\t\"materials/a/cloth_color.png\"\n\t\"TextureMetalness1\"\t\"materials/a/cloth_metal.png\"\n\t\"TextureAmbientOcclusion1\"\t\"materials/shared/ao.png\"\n\t\"TextureNprTramsissiveColor1\"\t\"[0 0.5 0 0]\"\n\t\"g_flSelfIllumScale1\"\t\"2\"\n\t\"DynamicParams\"\n\t{\n\t\t\"g_vSheenColorTint1\"\t\"v0 = 1;\\nreturn float3(1,1,1)*v0;\"\n\t}\n}\n";
        let m = mat::from_source(src).unwrap();
        let l = Loaded { m, path: "materials/a/cloth.vmat".into(), label: "test".into(), srcs: vec![] };
        let p = preset(&l);
        assert!(p.starts_with(PRESET_HEAD));
        assert!(p.contains("input\tTextureColor1\t{base}_color.png\t# lands in g_tColor.rgb"), "{p}");
        assert!(p.contains("input\tTextureMetalness1\t{base}_metal.png\t# lands in g_tColor.a"), "{p}");
        assert!(p.contains("input\tTextureAmbientOcclusion1\tmaterials/shared/ao.png"), "a shared texture keeps its path: {p}");
        let (vmat, files, inputs) = from_preset(&p, "materials/b/silk.vmat").unwrap();
        assert!(files.is_empty());
        assert!(inputs.contains(&"materials/b/silk_color.png".to_string()) && inputs.contains(&"materials/b/silk_metal.png".to_string()), "{inputs:?}");
        let back = mat::from_source(&vmat).unwrap();
        assert_eq!(back.texture("TextureColor1"), Some("materials/b/silk_color.png"));
        assert_eq!(back.texture("TextureMetalness1"), Some("materials/b/silk_metal.png"));
        assert_eq!(back.features(), [("F_SHEEN", 1)]);
        assert_eq!(back.float("g_flSelfIllumScale1"), Some(2.0));
        assert_eq!(back.constants, [("TextureNprTramsissiveColor1".to_string(), [0.0, 0.5, 0.0, 0.0])]);
        assert_eq!(back.dynamic, [("g_vSheenColorTint1".to_string(), "v0 = 1;\nreturn float3(1,1,1)*v0;".to_string())]);
        // the same setup, every name in the compiled material's terms
        let l2 = Loaded { m: back, path: "materials/b/silk.vmat".into(), label: "b".into(), srcs: vec![] };
        let names = |l: &Loaded| terms(l).into_iter().map(|(k, _)| k).collect::<Vec<_>>();
        assert_eq!(names(&l), names(&l2));
        assert!(from_preset("nope", "materials/x.vmat").is_err());
        assert!(from_preset(&format!("{PRESET_HEAD}\nbogus\tline\n"), "materials/x.vmat").is_err());
    }

    #[test]
    fn paths_are_written_against_the_material() {
        assert_eq!(tokenise("materials/a/cloth_color.png", "materials/a/cloth.vmat"), "{base}_color.png");
        assert_eq!(tokenise("materials/a/trim.png", "materials/a/cloth.vmat"), "{dir}/trim.png");
        assert_eq!(tokenise("materials/a/sub/trim.png", "materials/a/cloth.vmat"), "materials/a/sub/trim.png");
        assert_eq!(untokenise("{base}_color.png", "materials/b/silk.vmat"), "materials/b/silk_color.png");
        assert_eq!(untokenise("{dir}/trim.png", "materials/b/silk.vmat"), "materials/b/trim.png");
        assert_eq!(unesc(&esc("a\tb\nc\\d")), "a\tb\nc\\d");
        let m = Material { inputs: vec!["models/x/body_color.png".into(), "models/x/body_metal.png".into(), "models/x/body_sheenrough.png".into(), "models/x/body_sheen.png".into()], ..Material::default() };
        let g = guessed_inputs(&m);
        assert_eq!(g.iter().map(|(r, _)| r.as_str()).collect::<Vec<_>>(), ["TextureColor", "TextureMetalness", "TextureSheenColor", "TextureSheenRoughness"]);
        assert_eq!(game_rel(Path::new("D:/c/content/citadel_addons/x/materials/a/b.vmat")).map(|x| x.1).as_deref(), Some("materials/a/b.vmat"));
    }

    #[test]
    fn a_channel_is_found_by_its_pixels() {
        // an 8x8 compiled texture whose alpha holds a pattern: found plain, found inverted, and a flat colour is not it
        let pattern: Vec<u8> = (0..64).map(|i| if (i / 8 + i % 8) % 2 == 0 { 240 } else { 16 }).collect();
        let mut px = Vec::new();
        let (mut plain, mut inverse) = (Vec::new(), Vec::new());
        for p in &pattern {
            px.extend_from_slice(&[128, 128, 128, *p]);
            plain.extend_from_slice(&[*p, *p, *p, 255]);
            inverse.extend_from_slice(&[255 - p, 255 - p, 255 - p, 255]);
        }
        let tex = mark::bc::vtex_c(&[mark::image::Image::new(8, 8, px)], mark::bc::Format::Bc3);
        let a = compare(&mark::image::Image::new(8, 8, plain.clone()), 0, &tex, 3).unwrap();
        assert!(a.mean <= CHANNEL_TOLERANCE && !a.inverted, "{}", a.mean);
        let i = compare(&mark::image::Image::new(8, 8, inverse), 0, &tex, 3).unwrap();
        assert!(i.inverted && i.mean <= CHANNEL_TOLERANCE, "{}", i.mean);
        let g = compare(&mark::image::Image::new(8, 8, plain), 0, &tex, 1).unwrap().mean;
        assert!(g > 50.0, "a flat channel is not the pattern: {g}");
    }
}
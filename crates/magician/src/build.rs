//! `magician build`: compile a CSDK content addon and package what belongs in a release.
//!
//! Lessons this encodes (Maid Lash and another skin):
//! - resourcecompiler is fed **compilable sources only**. Given the whole tree (`-i <addon>\* -r`) it tries
//!   every png, dmx, fbx and .gitignore and prints hundreds of "Failed to find compiler" lines. Textures,
//!   meshes and wavs referenced by a document compile through that document.
//! - Panorama PSDs only compile through a `panorama_image_list` vdata (the CSDK hero_image_template
//!   pattern). The build writes a temporary list, compiles it and removes both the list and its `vdata_c`,
//!   which nothing loads (lint M013 + M070, and the grade notes a vdata).
//! - Compile-only outputs never ship: skeletons (`.vnmskel_c`, needed to compile clips), graphs, vdata, and
//!   the compiler's own `materials/default/*` copies (lint M071).
//! - **Stale outputs never ship**: a compiled file whose source is gone (a renamed texture, a dropped
//!   take) stays in the CSDK game tree forever. Only outputs that map back to a source in the content tree,
//!   or are referenced by one that does, are packaged.
//! - resourcecompiler can finish ("OK: N compiled, 0 failed") and then hang on exit ("Timed out waiting for
//!   thread to exit"). The build waits 20 s after the summary line, then ends the process.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Source extensions resourcecompiler compiles directly. Everything else in a content tree is an input
/// of one of these (png/tga/psd through vmat/vtex, dmx/fbx through vmdl, wav through vsndevts or itself).
pub const ROOT_EXTS: &[&str] = &[
    "js", "ts", "vmdl", "vmat", "vpcf", "vsndevts", "vcss", "vxml", "vjs", "vts", "vtex", "vnmclip", "vnmskel", "vsvg", "svg", "wav", "mp3",
    "vdata", "vpost", "vrr", "vmix", "vsndstck", "vsurf",
];

/// Compiled outputs that are build-only: never packaged.
pub fn build_only(path: &str) -> Option<&'static str> {
    if path.starts_with("materials/default/") {
        return Some("compiler's engine-default textures (M071)");
    }
    match path.rsplit('.').next().unwrap_or("") {
        "vnmskel_c" => Some("compile-only skeletons (M070)"),
        "vnmgraph_c" => Some("anim graphs (M070)"),
        "vdata_c" => Some("vdata: image lists and data (M013/M070)"),
        _ => None,
    }
}

#[derive(Default, Debug)]
pub struct Sources {
    pub roots: Vec<String>,
    /// PSDs under panorama/images that no image list in the tree names: the build lists them itself.
    pub loose_psds: Vec<String>,
    /// Files the game reads as they are, packed verbatim: console configs under `cfg/` (`exec` finds a cfg in a
    /// mounted addon VPK; proven with the cinema kit, build 6864).
    pub verbatim: Vec<String>,
    pub inputs: usize,
    pub ignored: BTreeMap<String, usize>,
    /// Every file in the content tree (relative, forward slashes, lowercase), for output-to-source mapping.
    pub all: HashSet<String>,
}

/// A `/`-separated relative path (a pak path, a source under an addon) as this system's path: `a/b` is `a\b` on
/// Windows and stays `a/b` elsewhere. A backslash written in by hand is a file name character off Windows.
pub fn native(rel: &str) -> PathBuf {
    rel.split('/').filter(|s| !s.is_empty()).collect()
}

pub fn rel_files(root: &Path, dir: &Path, out: &mut Vec<String>) -> std::io::Result<()> {
    rel_files_at(root, dir, out, 0)
}

/// Folders deeper than this under an addon are a link that loops back on itself (a junction to a parent), not
/// sources; following one would recurse until the stack runs out.
pub const MAX_DEPTH: usize = 64;

pub fn too_deep(dir: &Path) -> std::io::Error {
    std::io::Error::other(format!("{}: folders nested past {MAX_DEPTH} levels (a link or junction that loops back?)", dir.display()))
}

fn rel_files_at(root: &Path, dir: &Path, out: &mut Vec<String>, depth: usize) -> std::io::Result<()> {
    if depth > MAX_DEPTH {
        return Err(too_deep(dir));
    }
    for e in std::fs::read_dir(dir)? {
        let p = e?.path();
        if p.is_dir() {
            rel_files_at(root, &p, out, depth + 1)?;
        } else {
            out.push(p.strip_prefix(root).unwrap_or(&p).to_string_lossy().replace('\\', "/"));
        }
    }
    Ok(())
}

/// A content file that ships byte for byte: a console config under `cfg/` (lowercase path, `.cfg`).
pub fn is_verbatim(rel: &str) -> bool {
    let l = rel.to_ascii_lowercase();
    l.starts_with("cfg/") && l.ends_with(".cfg")
}

const INPUT_EXTS: &[&str] = &["png", "tga", "psd", "jpg", "jpeg", "exr", "dmx", "fbx", "obj", "smd", "vmorf"];

pub fn classify(files: &[String]) -> Sources {
    let mut s = Sources::default();
    let mut listed: HashSet<String> = HashSet::new();
    for f in files {
        s.all.insert(f.to_ascii_lowercase());
    }
    for f in files {
        let ext = f.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
        if ROOT_EXTS.contains(&ext.as_str()) {
            s.roots.push(f.clone());
        } else if is_verbatim(f) {
            s.verbatim.push(f.clone());
        } else if INPUT_EXTS.contains(&ext.as_str()) {
            s.inputs += 1;
            if ext == "psd" && f.starts_with("panorama/images/") {
                listed.insert(f.clone());
            }
        } else {
            *s.ignored.entry(if ext.is_empty() { "(none)".into() } else { ext }).or_default() += 1;
        }
    }
    // A PSD with its own `<image>_psd.vtex` beside it is compiled by that file, to the very output the image list
    // would write (`<image>_psd.vtex_c`). The list's version has one mip and its own format; the per-image one is how
    // a hero card gets Valve's nine mips. Two sources for one output is a race the list must not enter.
    s.loose_psds = listed.into_iter().filter(|p| !s.all.contains(&own_vtex(p))).collect();
    s.loose_psds.sort();
    s.verbatim.sort();
    // vnmskel first: clips compile against the compiled skeleton.
    s.roots.sort_by_key(|r| (!r.ends_with(".vnmskel"), r.clone()));
    s
}

/// The per-image texture source that compiles a panorama PSD to the same output as an image list would:
/// `heroes/lash_card.psd` is `heroes/lash_card_psd.vtex`.
fn own_vtex(psd: &str) -> String {
    let p = psd.to_ascii_lowercase();
    format!("{}_psd.vtex", p.strip_suffix(".psd").unwrap_or(&p))
}

/// The temporary image list for loose panorama PSDs (the CSDK hero_image_template pattern).
pub fn image_list(psds: &[String]) -> String {
    let mut t = String::from(
        "<!-- kv3 encoding:text:version{e21c7f3c-8a33-41c5-9977-a76d3a32aa0d} format:generic:version{7412167c-06e9-4698-aff2-e63eb59037e7} -->\n{\n\tgeneric_data_type = \"panorama_image_list\"\n\timage_list =\n\t[\n",
    );
    for p in psds {
        t.push_str(&format!("\t\tpanorama:\"file://{{images}}/{}\",\n", p.trim_start_matches("panorama/images/")));
    }
    t.push_str("\t]\n}\n");
    t
}

/// The content-tree source a compiled output came from, if it still exists (`lint::sources`, shared with M032).
pub fn source_of(out: &str, all: &HashSet<String>) -> Option<String> {
    lint::sources::source_of(out, all)
}

#[derive(Default, Debug)]
pub struct CompileSummary {
    pub compiled: usize,
    pub failed: usize,
    pub skipped: usize,
    pub errors: Vec<String>,
    pub warnings: usize,
    pub hung_killed: usize,
}

fn parse_summary(line: &str, s: &mut CompileSummary) -> bool {
    // " OK: 6 compiled, 0 failed, 5 skipped, 12m:41s" or " ERROR: 3 compiled, 1 failed, ..."
    let l = line.trim();
    if !(l.contains(" compiled, ") && l.contains(" failed")) {
        return false;
    }
    let num_before = |w: &str| -> usize {
        l.split(',').find(|p| p.contains(w)).and_then(|p| p.split_whitespace().rev().nth(1).and_then(|n| n.parse().ok())).unwrap_or(0)
    };
    s.compiled += num_before(" compiled");
    s.failed += num_before(" failed");
    s.skipped += num_before(" skipped");
    true
}

/// Run resourcecompiler on a batch, streaming its output to `log`, ending it if it hangs after its summary.
/// `game` is a `-game` folder (distance-field documents: `distance_field_game`), else the compiler's own default.
pub fn compile_batch(rc: &Path, files: &[PathBuf], game: Option<&Path>, log: &mut std::fs::File, s: &mut CompileSummary) -> Result<(), String> {
    let mut child = std::process::Command::new(rc)
        .arg("-nop4")
        .args(game.map(|g| vec![std::ffi::OsString::from("-game"), g.as_os_str().to_os_string()]).unwrap_or_default())
        .args(files)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("{}: {e}", rc.display()))?;
    let lines: Arc<Mutex<Vec<String>>> = Arc::default();
    let done_at: Arc<Mutex<Option<Instant>>> = Arc::default();
    let mut readers = Vec::new();
    for pipe in [child.stdout.take().map(|p| Box::new(p) as Box<dyn std::io::Read + Send>), child.stderr.take().map(|p| Box::new(p) as Box<dyn std::io::Read + Send>)]
        .into_iter()
        .flatten()
    {
        let lines = lines.clone();
        let done_at = done_at.clone();
        readers.push(std::thread::spawn(move || {
            for l in std::io::BufReader::new(pipe).lines().map_while(Result::ok) {
                if l.contains(" compiled, ") && l.contains(" failed") {
                    *done_at.lock().unwrap() = Some(Instant::now());
                }
                lines.lock().unwrap().push(l);
            }
        }));
    }
    let mut killed = false;
    loop {
        if child.try_wait().map_err(|e| e.to_string())?.is_some() {
            break;
        }
        if done_at.lock().unwrap().is_some_and(|t| t.elapsed() > Duration::from_secs(20)) {
            let _ = child.kill();
            let _ = child.wait();
            killed = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    if !killed {
        for r in readers {
            let _ = r.join();
        }
    } else {
        s.hung_killed += 1;
        std::thread::sleep(Duration::from_millis(300));
    }
    let lines = std::mem::take(&mut *lines.lock().unwrap());
    let mut saw_summary = false;
    for l in &lines {
        writeln!(log, "{l}").map_err(|e| e.to_string())?;
    }
    s.errors.extend(error_lines(&lines));
    for l in &lines {
        if parse_summary(l, s) {
            saw_summary = true;
            continue;
        }
        let lower = l.trim().to_ascii_lowercase();
        if lower.starts_with("warning") || lower.contains(" warning:") {
            s.warnings += 1;
        }
    }
    if !saw_summary {
        s.errors.push(format!("resourcecompiler printed no summary for a batch of {} file(s) (see the log)", files.len()));
    }
    Ok(())
}

/// The compiler's error lines, each tagged with the file it was compiling when it printed them (the last
/// `citadel_addons\...` path seen), so a failure names its file without opening the log.
pub fn error_lines(lines: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    let mut file = String::new();
    for l in lines {
        let t = l.trim();
        if let Some(i) = t.find("citadel_addons") {
            let path = t[i..].split_whitespace().next().unwrap_or("").trim_end_matches(['"', ':', ',']);
            if path.contains('.') {
                file = path.to_string();
            }
        }
        let lower = t.to_ascii_lowercase();
        if parse_summary(t, &mut CompileSummary::default()) || lower.contains("leaked keyvalues") {
            continue;
        }
        let is_err = lower.starts_with("error")
            || lower.contains(" error:")
            || lower.contains(" error ")
            || lower.contains("fatal")
            || lower.contains("failed to compile")
            || lower.contains("expecting a ");
        if is_err {
            out.push(if file.is_empty() || t.contains(&file) { t.to_string() } else { format!("{file}: {t}") });
        }
    }
    out
}

/// Text sources resourcecompiler reads. One that starts with a UTF-8 byte-order mark is rejected ("Error reading
/// texture compile settings ... Expecting a .vtex file"); Windows PowerShell's `Set-Content -Encoding utf8` writes one.
pub const TEXT_SOURCE_EXTS: &[&str] = &[
    "vtex", "vmat", "vpcf", "vsndevts", "vmdl", "vdata", "vcss", "vxml", "vjs", "vts", "svg", "vsvg", "vnmclip", "vnmskel", "vpost",
    "vrr", "vmix", "vsndstck", "vsurf", "kv3", "dmx", "cfg",
];

pub fn has_bom(bytes: &[u8]) -> bool {
    bytes.starts_with(&[0xEF, 0xBB, 0xBF])
}

/// Every text source under `root` (relative paths `rels`) that starts with a BOM.
pub fn bom_sources(root: &Path, rels: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    for r in rels {
        let ext = r.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
        if !TEXT_SOURCE_EXTS.contains(&ext.as_str()) {
            continue;
        }
        let mut head = [0u8; 3];
        let ok = std::fs::File::open(root.join(r)).and_then(|mut f| std::io::Read::read_exact(&mut f, &mut head)).is_ok();
        if ok && has_bom(&head) {
            out.push(r.clone());
        }
    }
    out
}

/// Classes only Deadlock's own compiler dlls can allocate. CSDK 13's default `bin\win64` (the Dota tools, Oct 7)
/// fails with "Failed to allocate an instance of class 'CNmClipDocEvent_TaggedSound'"; `bin\win64_nm` (win64 with
/// Deadlock's Oct 20 resourcecompiler.dll and animationsystem.dll) compiles them (docs/reference/engine.md, the compiler).
pub const NM_ONLY_CLASSES: &[&str] = &["CNmClipDocEvent_TaggedSound"];
pub const NM_BIN: &str = "win64_nm";
pub const STOCK_BIN: &str = "win64";
/// The per-addon compiler marker: one line naming a folder under `<csdk>\game\bin\` (or an absolute path).
pub const COMPILER_MARKER: &str = ".magician-compiler";

/// The first clip source that uses a class only `win64_nm` can compile, with that class.
pub fn needs_nm(root: &Path, rels: &[String]) -> Option<(String, &'static str)> {
    for r in rels.iter().filter(|r| r.to_ascii_lowercase().ends_with(".vnmclip")) {
        let Ok(text) = std::fs::read(root.join(r)) else { continue };
        let text = String::from_utf8_lossy(&text);
        if let Some(c) = NM_ONLY_CLASSES.iter().find(|c| text.contains(*c)) {
            return Some((r.clone(), c));
        }
    }
    None
}

/// The compiler bin for one addon. `explicit` is `--bin` (single-addon builds only); `marker` is the addon's
/// `.magician-compiler`; `needs` comes from `needs_nm`. A clip that needs win64_nm never compiles with anything else.
pub fn choose_bin(bin_root: &Path, explicit: Option<&str>, marker: Option<&str>, needs: Option<&(String, &str)>) -> Result<(PathBuf, String), String> {
    let resolve = |v: &str| {
        let p = Path::new(v.trim());
        if p.is_absolute() { p.to_path_buf() } else { bin_root.join(p) }
    };
    let (bin, why) = match (explicit, marker) {
        (Some(e), _) => (resolve(e), "--bin".to_string()),
        (None, Some(m)) => {
            // The marker ships inside the addon, which may be someone else's download: it names a folder of the
            // CSDK's own game\bin and nothing else, so it can't point the build at an executable of its choosing.
            let name = m.trim();
            if name.is_empty() || name == "." || name == ".." || name.contains(['/', '\\', ':']) || Path::new(name).is_absolute() {
                return Err(format!("{COMPILER_MARKER} names {name:?}: it must be the name of a folder in the CSDK's game\\bin ({STOCK_BIN}, {NM_BIN}); --bin takes a path"));
            }
            (bin_root.join(name), COMPILER_MARKER.to_string())
        }
        (None, None) => match needs {
            Some((f, c)) => (bin_root.join(NM_BIN), format!("auto: {f} uses {c}")),
            None => (bin_root.join(STOCK_BIN), "default".to_string()),
        },
    };
    if let Some((f, c)) = needs {
        let is_nm = bin.file_name().is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(NM_BIN));
        if !is_nm {
            return Err(format!(
                "{f} uses {c}, which only bin\\{NM_BIN} (Deadlock's own resourcecompiler.dll and animationsystem.dll) can compile, \
                 but this addon's compiler is {} ({why}). Point {COMPILER_MARKER} or --bin at {NM_BIN}",
                bin.display()
            ));
        }
    }
    Ok((bin, why))
}

/// A model document's `DistanceFieldList` node (the compiled DSTF block: per-bone signed distance grids for ambient
/// occlusion) only allocates when the compiling gameinfo's ModelDoc features include `distancefield`. Retail
/// Deadlock's has it; the CSDK's `game/citadel/gameinfo.gi` does not, and the compile fails with "Failed to allocate an
/// instance of class 'DistanceFieldList'" (docs/reference/engine.md, the compiler). Such documents compile with
/// `-game <csdk>/game/citadel_dstf`: the CSDK's gameinfo byte for byte with that one feature appended, so the shared
/// configuration is never edited. Its search paths are relative to game/, so it mounts exactly what the original
/// does and the outputs land in game/citadel_addons/<addon> as usual.
pub const DSTF_NODE: &str = "DistanceFieldList";
pub const DSTF_GAME: &str = "citadel_dstf";

/// The model documents (relative paths) that ask for distance fields.
pub fn needs_distance_fields(root: &Path, rels: &[String]) -> Vec<String> {
    rels.iter()
        .filter(|r| r.to_ascii_lowercase().ends_with(".vmdl"))
        .filter(|r| std::fs::read(root.join(native(r))).is_ok_and(|t| t.windows(DSTF_NODE.len()).any(|w| w == DSTF_NODE.as_bytes())))
        .cloned()
        .collect()
}

/// The byte range of the ModelDoc block's `"features"` value in a gameinfo, if it has one.
fn modeldoc_features(gi: &[u8]) -> Option<std::ops::Range<usize>> {
    let find = |hay: &[u8], needle: &[u8], from: usize| hay.get(from..)?.windows(needle.len()).position(|w| w == needle).map(|i| i + from);
    let md = find(gi, b"ModelDoc", 0)?;
    let open = find(gi, b"{", md)?;
    let close = find(gi, b"}", open)?;
    let key = find(gi, b"\"features\"", open).filter(|&k| k < close)?;
    let start = find(gi, b"\"", key + "\"features\"".len())? + 1;
    let end = find(gi, b"\"", start)?;
    (end < close).then_some(start..end)
}

/// The gameinfo with `distancefield` added to its ModelDoc features: `None` when it already has it, an error when
/// it has no ModelDoc features line to add it to.
pub fn with_distancefield(gi: &[u8]) -> Result<Option<Vec<u8>>, String> {
    let r = modeldoc_features(gi).ok_or("the CSDK's gameinfo.gi has no ModelDoc \"features\" line")?;
    if gi[r.clone()].split(|&b| b == b';').any(|f| f == b"distancefield") {
        return Ok(None);
    }
    let mut out = gi[..r.end].to_vec();
    if r.end > r.start {
        out.push(b';');
    }
    out.extend_from_slice(b"distancefield");
    out.extend_from_slice(&gi[r.end..]);
    Ok(Some(out))
}

/// The `-game` folder distance-field documents compile with: None when the CSDK's own gameinfo already has the
/// feature, else `<csdk>/game/citadel_dstf` (written, or rewritten when the CSDK's gameinfo changed).
pub fn distance_field_game(csdk: &Path) -> Result<Option<PathBuf>, String> {
    let src = csdk.join("game").join("citadel").join("gameinfo.gi");
    let gi = std::fs::read(&src).map_err(|e| format!("{}: {e}", src.display()))?;
    let Some(new) = with_distancefield(&gi).map_err(|e| format!("{}: {e}", src.display()))? else { return Ok(None) };
    let dir = csdk.join("game").join(DSTF_GAME);
    let p = dir.join("gameinfo.gi");
    if std::fs::read(&p).ok().as_deref() != Some(&new[..]) {
        std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let tmp = dir.join("gameinfo.gi.magician-tmp");
        std::fs::write(&tmp, &new).map_err(|e| format!("{}: {e}", tmp.display()))?;
        std::fs::rename(&tmp, &p).map_err(|e| format!("{}: {e}", p.display()))?;
    }
    Ok(Some(dir))
}

/// Split files into batches whose command line stays under Windows' 32k limit.
pub fn batches(files: &[PathBuf]) -> Vec<Vec<PathBuf>> {
    let mut out: Vec<Vec<PathBuf>> = Vec::new();
    let mut len = 0usize;
    for f in files {
        let n = f.as_os_str().len() + 3;
        if out.is_empty() || len + n > 24_000 {
            out.push(Vec::new());
            len = 0;
        }
        out.last_mut().unwrap().push(f.clone());
        len += n;
    }
    out
}

pub struct Package {
    pub files: Vec<(String, Vec<u8>)>,
    pub left_out: BTreeMap<&'static str, Vec<String>>,
}

/// Name of the stamp a successful compile leaves in the addon's output tree.
pub const BUILD_STAMP: &str = "magician_build.stamp";

/// Fingerprint of a file list: each path with its size and modification time, in path order, folded with
/// `extra` (the compiler's identity). Changing, adding, removing or touching any file changes it.
pub fn tree_fingerprint(root: &Path, rels: &[String], extra: &str) -> u64 {
    let mut sorted: Vec<&String> = rels.iter().collect();
    sorted.sort();
    let mut h = lint::cache::hash64(extra.as_bytes(), 7);
    for r in sorted {
        let (len, t) = std::fs::metadata(root.join(native(r)))
            .map(|m| (m.len(), m.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_nanos())))
            .unwrap_or((u64::MAX, 0));
        h = lint::cache::hash64(format!("{r}|{len}|{t}").as_bytes(), h);
    }
    h
}

/// A file's size and modification time as text (a compiler binary's identity for the stamp).
pub fn file_id(p: &Path) -> String {
    match std::fs::metadata(p) {
        Ok(m) => format!("{}:{}:{}", p.display(), m.len(), m.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_nanos())),
        Err(_) => format!("{}:none", p.display()),
    }
}

/// The `_c` outputs under an addon's game tree, relative.
pub fn compiled_outputs(game_out: &Path) -> Vec<String> {
    let mut rels = Vec::new();
    let _ = rel_files(game_out, game_out, &mut rels);
    rels.retain(|p| p.ends_with("_c"));
    rels
}

/// Choose what ships from a CSDK game-output tree: outputs that map to a live source, plus what those reference
/// (RERL) in the same tree when it is live too or generated by its referrer; never build-only outputs.
/// `in_game` tells which paths the game build ships. A live output that references an output
/// whose source is gone is a broken build: the stale file is never packed (a released skin shipped five deleted effects,
/// one a shock ring at 1.3x Valve's brightness, because a live particle still named them). If the game ships that
/// path, the reference falls back to Valve's file and the stale copy is just left out; otherwise the build fails
/// naming the referrer and the target.
pub fn package_with(game_out: &Path, sources: &Sources, in_game: &dyn Fn(&str) -> bool) -> Result<Package, String> {
    let mut rels = Vec::new();
    rel_files(game_out, game_out, &mut rels).map_err(|e| format!("{}: {e}", game_out.display()))?;
    let compiled: BTreeSet<String> = rels.into_iter().filter(|p| p.ends_with("_c")).collect();
    let mut left_out: BTreeMap<&'static str, Vec<String>> = BTreeMap::new();
    let mut keep: BTreeSet<String> = BTreeSet::new();
    let mut frontier: Vec<String> = Vec::new();
    for p in &compiled {
        if let Some(why) = build_only(p) {
            left_out.entry(why).or_default().push(p.clone());
        } else if source_of(p, &sources.all).is_some()
            && keep.insert(p.clone()) {
                frontier.push(p.clone());
            }
    }
    // Each file is read once (and kept for the package): a level of the reference walk is read and parsed on
    // threads, then the references found there form the next level.
    let mut bytes: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    let mut shadowed: BTreeSet<String> = BTreeSet::new();
    let mut broken: Vec<(String, String)> = Vec::new();
    while !frontier.is_empty() {
        let level: Vec<Result<(String, Vec<u8>, Vec<String>), String>> = lint::par::map(&frontier, |p| {
            let data = std::fs::read(game_out.join(p)).map_err(|e| e.to_string())?;
            let refs = resource::parse(&data).map(|r| lint::ref_targets(&r)).unwrap_or_default();
            Ok((p.clone(), data, refs))
        });
        frontier = Vec::new();
        for r in level {
            let (p, data, refs) = r?;
            bytes.insert(p.clone(), data);
            for t in refs {
                if !compiled.contains(&t) || build_only(&t).is_some() || keep.contains(&t) {
                    continue;
                }
                if source_of(&t, &sources.all).is_some() || lint::sources::generated_by(&t, &p) {
                    keep.insert(t.clone());
                    frontier.push(t);
                } else if in_game(&t) {
                    shadowed.insert(t);
                } else {
                    broken.push((p.clone(), t));
                }
            }
        }
    }
    if !broken.is_empty() {
        broken.sort();
        broken.dedup();
        let list: Vec<String> = broken.iter().map(|(r, t)| format!("  {r} -> {t}")).collect();
        return Err(format!(
            "{} reference(s) from live outputs to compiled leftovers whose source was deleted (never packed; restore the source or drop the reference):\n{}",
            broken.len(),
            list.join("\n")
        ));
    }
    for p in &compiled {
        if !keep.contains(p) && build_only(p).is_none() {
            let why = if shadowed.contains(p) {
                "stale copies of game files (source gone; references fall back to the game's)"
            } else {
                "stale outputs (their source is gone)"
            };
            left_out.entry(why).or_default().push(p.clone());
        }
    }
    let mut files = Vec::new();
    for p in keep {
        let data = bytes.remove(&p).ok_or_else(|| format!("{p}: not read"))?;
        files.push((p, data));
    }
    Ok(Package { files, left_out })
}

/// The department a packaged file belongs to, for release reports.
pub fn department(path: &str) -> &'static str {
    let ext = path.rsplit('.').next().unwrap_or("");
    if path.starts_with("panorama/") {
        "ui"
    } else if path.starts_with("sounds/") || path.starts_with("soundevents/") {
        "sound"
    } else if ext == "vnmclip_c" {
        "animation"
    } else if path.starts_with("particles/") || path.contains("/particle/") || path.contains("/fx/") || path.starts_with("models/abilities/") || (path.starts_with("models/") && !path.starts_with("models/heroes")) {
        "vfx"
    } else {
        "model"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all(v: &[&str]) -> HashSet<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn only_compilable_sources_are_roots() {
        let files: Vec<String> = [
            "models/heroes_wip/lash/lash.vmdl",
            "models/heroes_wip/lash/maid_body.dmx",
            "materials/maid_lash/outfit/maid_cloth.vmat",
            "materials/maid_lash/outfit/maid_cloth_color.png",
            "materials/.gitignore",
            "magician.allow",
            "sounds/maid_lash/x.wav",
            "panorama/images/heroes/lash_card.psd",
            "models/heroes_staging/x/x.vnmskel",
            "cfg/cinema.cfg",
            "notes/cfg/readme.cfg",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let s = classify(&files);
        assert_eq!(s.verbatim, vec!["cfg/cinema.cfg"], "configs under cfg/ ship as they are; elsewhere they are not sources");
        assert_eq!(s.roots[0], "models/heroes_staging/x/x.vnmskel", "skeletons compile first");
        assert_eq!(s.roots.len(), 4);
        assert!(!s.roots.iter().any(|r| r.ends_with(".png") || r.ends_with(".dmx") || r.ends_with(".gitignore")));
        assert_eq!(s.loose_psds, vec!["panorama/images/heroes/lash_card.psd"]);
        assert_eq!(s.ignored.get("allow"), Some(&1));
        assert!(image_list(&s.loose_psds).contains("panorama:\"file://{images}/heroes/lash_card.psd\""));
        // a card with its own .vtex is that file's to compile (nine mips), never the image list's (one)
        let mut both = files.clone();
        both.push("panorama/images/heroes/lash_card_psd.vtex".into());
        both.push("panorama/images/heroes/lash_sm.psd".into());
        let s = classify(&both);
        assert_eq!(s.loose_psds, vec!["panorama/images/heroes/lash_sm.psd"]);
        assert!(s.roots.iter().any(|r| r == "panorama/images/heroes/lash_card_psd.vtex"));
        // Real case (lint notes, 2026-11-24): a released skin's four hero cards each with a .vtex beside them, whatever
        // the case of either name, are none of them the list's (the build had written them with 1 mip, Valve's 9)
        let cards = ["lash_card", "Lash_Card_Gloat", "lash_card_critical", "lash_vertical"];
        let mut four: Vec<String> = cards.iter().map(|c| format!("panorama/images/heroes/{c}.psd")).collect();
        four.extend(cards.iter().map(|c| format!("panorama/images/heroes/{}_psd.vtex", c.to_ascii_lowercase())));
        let s = classify(&four);
        assert!(s.loose_psds.is_empty(), "{:?}", s.loose_psds);
        assert_eq!(s.roots.iter().filter(|r| r.ends_with("_psd.vtex")).count(), 4);
    }

    // The CSDK's gameinfo as it ships (two `features` lines; only ModelDoc's counts).
    const CSDK_GI: &str = "\"GameInfo\"\n{\n\tFileSystem\n\t{\n\t\tSearchPaths\n\t\t{\n\t\t\tGame\tcitadel\n\t\t}\n\t}\n\tModelDoc\n\t{\n\t\t\"GameFeatureSet\"\t\t\"Citadel\"\n\t\t\"features\"\t\t\t\t\t\"modelconfig;wireframe_backfaces;animgraph;animgraph_compatibility_force;editorconfig;gamepreview\"\n\t}\n\tRenderPipelineAliases\n\t{\n\t\t\"Features\" \"non_homogenous_forward_layer_only\"\n\t}\n}\n";

    #[test]
    fn a_model_with_distance_fields_compiles_against_a_copy_of_the_gameinfo_that_allows_them() {
        let out = String::from_utf8(with_distancefield(CSDK_GI.as_bytes()).unwrap().unwrap()).unwrap();
        assert!(out.contains("editorconfig;gamepreview;distancefield\""), "{out}");
        assert_eq!(out.replace(";distancefield", ""), CSDK_GI, "byte for byte the CSDK's, plus the one feature");
        assert!(out.contains("\"Features\" \"non_homogenous_forward_layer_only\""), "the other features line is untouched");
        assert_eq!(with_distancefield(out.as_bytes()).unwrap(), None, "already there: nothing to add");
        assert!(with_distancefield(b"\"GameInfo\" { }").is_err());

        let d = std::env::temp_dir().join(format!("magician-dstf-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let gi = d.join("csdk").join("game").join("citadel");
        std::fs::create_dir_all(&gi).unwrap();
        std::fs::write(gi.join("gameinfo.gi"), CSDK_GI).unwrap();
        let game = distance_field_game(&d.join("csdk")).unwrap().unwrap();
        assert!(game.ends_with(DSTF_GAME));
        assert_eq!(std::fs::read_to_string(game.join("gameinfo.gi")).unwrap(), out);
        assert_eq!(std::fs::read_to_string(gi.join("gameinfo.gi")).unwrap(), CSDK_GI, "the CSDK's own gameinfo is never edited");
        // a CSDK whose gameinfo already allows them needs no copy
        std::fs::write(gi.join("gameinfo.gi"), &out).unwrap();
        assert_eq!(distance_field_game(&d.join("csdk")).unwrap(), None);

        let addon = d.join("addon");
        std::fs::create_dir_all(addon.join("models")).unwrap();
        std::fs::write(addon.join("models").join("a.vmdl"), "{ rootNode = { children = [ { _class = \"DistanceFieldList\" } ] } }").unwrap();
        std::fs::write(addon.join("models").join("b.vmdl"), "{ rootNode = { children = [ { _class = \"RenderMeshList\" } ] } }").unwrap();
        let rels = vec!["models/a.vmdl".to_string(), "models/b.vmdl".to_string(), "models/c.vmat".to_string()];
        assert_eq!(needs_distance_fields(&addon, &rels), vec!["models/a.vmdl"]);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn outputs_map_to_their_sources() {
        let a = all(&[
            "materials/maid_lash/outfit/maid_cloth.vmat",
            "materials/maid_lash/outfit/maid_cloth_color.png",
            "sounds/maid_lash/a.wav",
            "panorama/images/heroes/lash_card.psd",
            "panorama/images/heroes/hero_names/lash.svg",
        ]);
        assert!(source_of("materials/maid_lash/outfit/maid_cloth.vmat_c", &a).is_some());
        assert!(source_of("materials/maid_lash/outfit/maid_cloth_color_png_f6ecf355.vtex_c", &a).is_some());
        assert!(source_of("materials/maid_lash/outfit/maid_cloth_vmat_g_tmetalness_bdaa4c92.vtex_c", &a).is_some());
        assert!(source_of("sounds/maid_lash/a.vsnd_c", &a).is_some());
        assert!(source_of("panorama/images/heroes/lash_card_psd.vtex_c", &a).is_some());
        assert!(source_of("panorama/images/heroes/hero_names/lash.vsvg_c", &a).is_some());
        // stale: the take and the texture were renamed away
        assert!(source_of("sounds/maid_lash/b.vsnd_c", &a).is_none());
        assert!(source_of("materials/maid_lash/fx/ribbon_lined_color_png_0badf00d.vtex_c", &a).is_none());
    }

    #[test]
    fn a_deleted_source_stays_out_even_when_referenced() {
        // Real case: a skin's build packed five effects whose sources were deleted, because a live particle still
        // named them (one was a bolt shock ring at 1.3x Valve's brightness).
        let dir = std::env::temp_dir().join(format!("magician_stale_{}", std::process::id()));
        let out = dir.join("game");
        std::fs::create_dir_all(out.join("particles/example")).unwrap();
        std::fs::write(out.join("particles/example/bolt.vpcf_c"), resource::synth(&["particles/example/bolt_shock_ring.vpcf"])).unwrap();
        std::fs::write(out.join("particles/example/bolt_shock_ring.vpcf_c"), resource::synth(&[])).unwrap();
        std::fs::write(out.join("particles/example/old_ult_aura.vpcf_c"), resource::synth(&[])).unwrap();
        let mut sources = Sources::default();
        sources.all.insert("particles/example/bolt.vpcf".into());

        let e = package_with(&out, &sources, &|_| false).err().expect("a live output naming a leftover fails the build");
        assert!(e.contains("particles/example/bolt.vpcf_c -> particles/example/bolt_shock_ring.vpcf_c"), "{e}");

        // the same leftover at a path the game ships: left out, the reference resolves to Valve's file
        let pk = package_with(&out, &sources, &|p| p == "particles/example/bolt_shock_ring.vpcf_c").unwrap();
        let names: Vec<&str> = pk.files.iter().map(|(p, _)| p.as_str()).collect();
        assert_eq!(names, vec!["particles/example/bolt.vpcf_c"]);
        assert!(pk.left_out.values().flatten().any(|p| p.ends_with("bolt_shock_ring.vpcf_c")));
        assert!(pk.left_out.values().flatten().any(|p| p.ends_with("old_ult_aura.vpcf_c")), "unreferenced leftovers stay out too");

        // restoring the source packs it again
        sources.all.insert("particles/example/bolt_shock_ring.vpcf".into());
        let pk = package_with(&out, &sources, &|_| false).unwrap();
        assert_eq!(pk.files.len(), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn build_only_outputs() {
        assert!(build_only("materials/default/default_color_tga_96b3aa03.vtex_c").is_some());
        assert!(build_only("models/heroes_wip/lash/lash.vnmskel_c").is_some());
        assert!(build_only("panorama/maid_lash_ui_images.vdata_c").is_some());
        assert!(build_only("models/heroes_wip/lash/lash.vmdl_c").is_none());
    }

    #[test]
    fn bom_sources_are_found() {
        // Real case: a PowerShell Set-Content -Encoding utf8 edit of a .vtex; resourcecompiler then failed with
        // "Error reading texture compile settings ... Expecting a .vtex file".
        let dir = std::env::temp_dir().join(format!("magician_bom_{}", std::process::id()));
        std::fs::create_dir_all(dir.join("materials")).unwrap();
        std::fs::write(dir.join("materials/gobo.vtex"), b"\xEF\xBB\xBF<!-- dmx encoding keyvalues2_noids 1 format vtex 1 -->").unwrap();
        std::fs::write(dir.join("materials/clean.vmat"), b"// clean").unwrap();
        std::fs::write(dir.join("materials/gobo.png"), b"\xEF\xBB\xBFnot a text source").unwrap();
        let rels: Vec<String> = ["materials/gobo.vtex", "materials/clean.vmat", "materials/gobo.png"].iter().map(|s| s.to_string()).collect();
        assert_eq!(bom_sources(&dir, &rels), vec!["materials/gobo.vtex"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_pak_path_is_a_path_of_this_system() {
        // `maps\ui\...` was written on every system: a file name with backslashes in it off Windows
        assert_eq!(native("maps/ui/hero_prefabs/lash.vpk"), Path::new("maps").join("ui").join("hero_prefabs").join("lash.vpk"));
        assert_eq!(native("a//b/"), Path::new("a").join("b"));
    }

    #[test]
    fn a_folder_link_that_loops_back_is_an_error_not_a_stack_overflow() {
        let d = std::env::temp_dir().join(format!("magician-loop-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("addon").join("materials")).unwrap();
        std::fs::write(d.join("addon").join("materials").join("a.vmat"), "x").unwrap();
        let link = d.join("addon").join("materials").join("back");
        #[cfg(windows)]
        let made = std::process::Command::new("cmd").args(["/c", "mklink", "/J"]).arg(&link).arg(d.join("addon")).output().is_ok_and(|o| o.status.success());
        #[cfg(not(windows))]
        let made = std::os::unix::fs::symlink(d.join("addon"), &link).is_ok();
        if made {
            let mut out = Vec::new();
            let e = rel_files(&d.join("addon"), &d.join("addon"), &mut out).unwrap_err();
            assert!(e.to_string().contains("nested past 64 levels"), "{e}");
            // the junction goes first, so removing the folder doesn't follow it
            #[cfg(windows)]
            let _ = std::fs::remove_dir(&link);
            #[cfg(not(windows))]
            let _ = std::fs::remove_file(&link);
        }
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn error_lines_name_their_file() {
        let lines: Vec<String> = [
            r"  +- citadel_addons\maid_lash\materials\maid_lash\fx\lace_gobo.vtex",
            "Error reading texture compile settings from file, Expecting a .vtex file",
            " ERROR: 0 compiled, 1 failed, 0 skipped, 1s",
            "Leaked KeyValues blocks: 440",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let e = error_lines(&lines);
        assert_eq!(e.len(), 1, "{e:?}");
        assert!(e[0].starts_with(r"citadel_addons\maid_lash\materials\maid_lash\fx\lace_gobo.vtex: Error reading"), "{e:?}");
    }

    #[test]
    fn tagged_sound_clips_get_the_deadlock_compiler() {
        // Real case: maid_lash_anim, 31 of 44 clips "Failed to allocate an instance of class 'CNmClipDocEvent_TaggedSound'"
        // under the stock bin.
        let dir = std::env::temp_dir().join(format!("magician_nm_{}", std::process::id()));
        std::fs::create_dir_all(dir.join("clips")).unwrap();
        std::fs::write(dir.join("clips/run.vnmclip"), "{ m_events = [ { _class = \"CNmClipDocEvent_TaggedSound\" m_name = \"footstep\" } ] }").unwrap();
        std::fs::write(dir.join("clips/idle.vnmclip"), "{ m_events = [ ] }").unwrap();
        let rels: Vec<String> = ["clips/idle.vnmclip", "clips/run.vnmclip"].iter().map(|s| s.to_string()).collect();
        let needs = needs_nm(&dir, &rels);
        assert_eq!(needs.as_ref().map(|(f, _)| f.as_str()), Some("clips/run.vnmclip"));
        let _ = std::fs::remove_dir_all(&dir);

        let root = Path::new(r"D:\csdk\game\bin");
        let (b, why) = choose_bin(root, None, None, needs.as_ref()).unwrap();
        assert!(b.ends_with(NM_BIN) && why.starts_with("auto"), "{b:?} {why}");
        assert!(choose_bin(root, None, Some("win64_nm\n"), needs.as_ref()).unwrap().0.ends_with(NM_BIN));
        let e = choose_bin(root, Some("win64"), None, needs.as_ref()).unwrap_err();
        assert!(e.contains("CNmClipDocEvent_TaggedSound") && e.contains("win64_nm"), "{e}");
        assert!(choose_bin(root, None, Some("win64"), needs.as_ref()).is_err(), "a marker can't force the wrong compiler either");
        assert!(choose_bin(root, None, None, None).unwrap().0.ends_with(STOCK_BIN), "no such clips: the stock compiler");
        // an addon's marker is a folder name in the CSDK's bin, never a path out of it; --bin (the caller's own) may be
        for bad in [r"C:\Users\x\evil", r"..\..\..\evil", "../evil", "win64/../../x", "..", "D:evil", "  "] {
            let e = choose_bin(root, None, Some(bad), None).unwrap_err();
            assert!(e.contains("must be the name of a folder"), "{bad}: {e}");
        }
        #[cfg(windows)]
        assert_eq!(choose_bin(root, Some(r"E:\tools\bin"), None, None).unwrap().0, PathBuf::from(r"E:\tools\bin"));
    }

    #[test]
    fn summary_line() {
        let mut s = CompileSummary::default();
        assert!(parse_summary(" OK: 6 compiled, 0 failed, 5 skipped, 12m:41s", &mut s));
        assert!(parse_summary(" ERROR: 3 compiled, 1 failed, 0 skipped, 2s", &mut s));
        assert_eq!((s.compiled, s.failed, s.skipped), (9, 1, 5));
        assert!(!parse_summary("Compile of 1 file(s) matching", &mut s));
    }

    #[test]
    fn departments() {
        assert_eq!(department("models/heroes_wip/lash/lash.vmdl_c"), "model");
        assert_eq!(department("particles/abilities/lash/x.vpcf_c"), "vfx");
        assert_eq!(department("materials/maid_lash/fx/lace_gobo.vtex_c"), "vfx");
        assert_eq!(department("soundevents/hero/lash.vsndevts_c"), "sound");
        assert_eq!(department("panorama/images/heroes/lash_card_psd.vtex_c"), "ui");
        assert_eq!(department("models/heroes_staging/lash_v2/clips/x.vnmclip_c"), "animation");
    }
}

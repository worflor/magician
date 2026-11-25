//! `magician diff <a.vpk> <b.vpk>`: whether two paks are the same mod, the proof a move or a rebuild needs.
//!
//! The same file (one sha256) is the end of it. Otherwise every path is one of: only in the first, only in the second,
//! the same bytes, or different bytes; and a compiled file with different bytes is either the same contents (every
//! block but the compiler's own record, RED2 or REDI, and any data after the blocks, byte for byte: what two compiles
//! of one source give) or different contents, with the blocks that differ. Exit 0 when no file differs in what it
//! holds, 1 when one does.

use super::*;

/// How one path compares across the two paks.
#[derive(Debug, PartialEq)]
pub(crate) enum Change {
    OnlyFirst,
    OnlySecond,
    /// Different bytes, the same contents: only the compiler's record differs.
    Recompiled,
    /// Different contents, with the blocks that differ (or why they could not be compared).
    Changed(String),
}

/// The compiler's own record of a compile (its inputs, their CRCs, the compiler's version): differs between any two
/// compiles of one source.
const COMPILE_RECORD: [&[u8; 4]; 2] = [b"RED2", b"REDI"];

/// Whether two compiled files hold the same thing, and if not, what differs.
pub(crate) fn compare_resource(a: &[u8], b: &[u8]) -> Change {
    let (Ok(ra), Ok(rb)) = (resource::parse(a), resource::parse(b)) else {
        return Change::Changed("different bytes (not compiled resources, so nothing finer to compare)".into());
    };
    let mut differ: Vec<String> = Vec::new();
    let tags = |r: &resource::Resource| r.blocks.iter().filter(|x| !COMPILE_RECORD.contains(&&x.tag)).map(|x| x.tag_str().to_string()).collect::<Vec<_>>();
    if tags(&ra) != tags(&rb) {
        return Change::Changed(format!("different blocks ({} against {})", tags(&ra).join(" "), tags(&rb).join(" ")));
    }
    for (x, y) in ra.blocks.iter().filter(|x| !COMPILE_RECORD.contains(&&x.tag)).zip(rb.blocks.iter().filter(|y| !COMPILE_RECORD.contains(&&y.tag))) {
        if resource::block_bytes(a, x) != resource::block_bytes(b, y) {
            differ.push(x.tag_str().to_string());
        }
    }
    // What follows the last block (a texture's pixels, a sound's samples).
    let tail = |d: &[u8], r: &resource::Resource| r.blocks.iter().map(|x| x.offset + x.size).max().and_then(|end| d.get(end..)).unwrap_or(&[]).to_vec();
    if tail(a, &ra) != tail(b, &rb) {
        differ.push("the data after the blocks".into());
    }
    if differ.is_empty() {
        Change::Recompiled
    } else {
        Change::Changed(format!("{} differ{}", differ.join(", "), if differ.len() == 1 { "s" } else { "" }))
    }
}

/// Every path whose file is not the same in both paks, sorted by path.
pub(crate) fn compare(a: &vpk::Vpk, b: &vpk::Vpk) -> Result<Vec<(String, Change)>, String> {
    let theirs: std::collections::BTreeMap<&str, &vpk::Entry> = b.entries.iter().map(|e| (e.path.as_str(), e)).collect();
    let ours: std::collections::BTreeMap<&str, &vpk::Entry> = a.entries.iter().map(|e| (e.path.as_str(), e)).collect();
    let mut out = Vec::new();
    for (p, e) in &ours {
        match theirs.get(p) {
            None => out.push((p.to_string(), Change::OnlyFirst)),
            Some(f) if f.crc == e.crc && f.size() == e.size() => {}
            Some(f) => {
                let x = a.read(e).map_err(|err| format!("{}: {p}: {err}", a.dir_path.display()))?;
                let y = b.read(f).map_err(|err| format!("{}: {p}: {err}", b.dir_path.display()))?;
                out.push((p.to_string(), if x == y { Change::Recompiled } else { compare_resource(&x, &y) }));
            }
        }
    }
    for p in theirs.keys().filter(|p| !ours.contains_key(*p)) {
        out.push((p.to_string(), Change::OnlySecond));
    }
    out.sort_by(|x, y| x.0.cmp(&y.0));
    Ok(out)
}

pub fn cmd_diff(args: &[String]) -> R {
    let p = positional(args);
    let (Some(pa), Some(pb)) = (p.first(), p.get(1)) else {
        return Err(Stop::Usage("diff <a.vpk> <b.vpk>".into()));
    };
    let (pa, pb) = (Path::new(pa.as_str()), Path::new(pb.as_str()));
    let sha = |x: &Path| grade::sha256::file(x).map_err(|e| format!("{}: {e}", x.display()));
    let (sa, sb) = (sha(pa)?, sha(pb)?);
    let (a, b) = (open(pa)?, open(pb)?);
    let label = |x: &Path| x.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let (na, nb) = (label(pa), label(pb));
    if sa == sb {
        say!("the same file: {} files, sha256 {sa}", a.entries.len());
        return Ok(ExitCode::SUCCESS);
    }
    let changes = compare(&a, &b)?;
    let count = |f: &dyn Fn(&Change) -> bool| changes.iter().filter(|(_, c)| f(c)).count();
    let only_a = count(&|c| *c == Change::OnlyFirst);
    let only_b = count(&|c| *c == Change::OnlySecond);
    let recompiled = count(&|c| *c == Change::Recompiled);
    let changed = count(&|c| matches!(c, Change::Changed(_)));
    let same = a.entries.len() - only_a - recompiled - changed;
    let differs = only_a + only_b + changed > 0;
    say!(
        "{}: {na} has {} files, {nb} {}; {same} the same bytes, {recompiled} the same contents in different bytes, {changed} different, {only_a} only in {na}, {only_b} only in {nb}",
        if differs { "different" } else { "the same files" },
        a.entries.len(),
        b.entries.len()
    );
    for (what, pick) in [
        (format!("only in {na}"), &(|c: &Change| *c == Change::OnlyFirst) as &dyn Fn(&Change) -> bool),
        (format!("only in {nb}"), &|c: &Change| *c == Change::OnlySecond),
        ("different contents".to_string(), &|c: &Change| matches!(c, Change::Changed(_))),
        ("the same contents, recompiled (only the compiler's record differs)".to_string(), &|c: &Change| *c == Change::Recompiled),
    ] {
        let these: Vec<&(String, Change)> = changes.iter().filter(|(_, c)| pick(c)).collect();
        if these.is_empty() {
            continue;
        }
        say!("{what} ({}):", these.len());
        for (p, c) in these {
            match c {
                Change::Changed(why) => say!("  {p}  ({why})"),
                _ => say!("  {p}"),
            }
        }
    }
    say!("sha256 {sa}  {}\nsha256 {sb}  {}", pa.display(), pb.display());
    Ok(ExitCode::from(if differs { cli::BROKEN } else { cli::CLEAN }))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A compiled resource with the given blocks after its RERL, and `tail` after them (resource::synth_with).
    fn res(blocks: &[(&[u8; 4], &[u8])], tail: &[u8]) -> Vec<u8> {
        let mut d = resource::synth_with(&[], &blocks.iter().map(|(t, b)| (**t, b.to_vec())).collect::<Vec<_>>());
        d.extend_from_slice(tail);
        d
    }

    #[test]
    fn a_recompile_differs_only_in_the_compiler_s_record() {
        let a = res(&[(b"RED2", b"compiled on monday"), (b"DATA", b"the same kv3")], b"pixels");
        let b = res(&[(b"RED2", b"compiled on tuesday, elsewhere"), (b"DATA", b"the same kv3")], b"pixels");
        assert_eq!(compare_resource(&a, &b), Change::Recompiled);
        let c = res(&[(b"RED2", b"x"), (b"DATA", b"another kv3")], b"pixels");
        assert_eq!(compare_resource(&a, &c), Change::Changed("DATA differs".into()));
        let d = res(&[(b"RED2", b"x"), (b"DATA", b"the same kv3")], b"other pixels");
        assert_eq!(compare_resource(&a, &d), Change::Changed("the data after the blocks differs".into()));
        assert!(matches!(compare_resource(b"plain", b"text"), Change::Changed(w) if w.contains("not compiled resources")));
    }

    #[test]
    fn two_paks_compare_path_by_path() {
        let d = std::env::temp_dir().join(format!("magician-diff-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let same = res(&[(b"DATA", b"kept")], b"");
        let a = d.join("pak01_dir.vpk");
        let b = d.join("pak10_dir.vpk");
        vpk::write(&a, &[
            ("materials/a.vmat_c".to_string(), same.clone()),
            ("materials/b.vmat_c".to_string(), res(&[(b"RED2", b"one"), (b"DATA", b"b")], b"")),
            ("materials/c.vmat_c".to_string(), res(&[(b"DATA", b"c")], b"")),
            ("materials/old.vmat_c".to_string(), same.clone()),
        ]).unwrap();
        vpk::write(&b, &[
            ("materials/a.vmat_c".to_string(), same.clone()),
            ("materials/b.vmat_c".to_string(), res(&[(b"RED2", b"two"), (b"DATA", b"b")], b"")),
            ("materials/c.vmat_c".to_string(), res(&[(b"DATA", b"C!")], b"")),
            ("materials/new.vmat_c".to_string(), same),
        ]).unwrap();
        let c = compare(&open(&a).unwrap(), &open(&b).unwrap()).unwrap();
        assert_eq!(c, vec![
            ("materials/b.vmat_c".to_string(), Change::Recompiled),
            ("materials/c.vmat_c".to_string(), Change::Changed("DATA differs".into())),
            ("materials/new.vmat_c".to_string(), Change::OnlySecond),
            ("materials/old.vmat_c".to_string(), Change::OnlyFirst),
        ]);
        assert!(compare(&open(&a).unwrap(), &open(&a).unwrap()).unwrap().is_empty());
        std::fs::remove_dir_all(&d).unwrap();
    }
}

//! Taking the paks out of a download: a mod site's `.zip`, `.7z` or `.rar` holds one or more VPKs and, often, a readme
//! or a preview image. `magician init <archive>` uses this to put the VPKs, and nothing else, into the mod's folder.
//!
//! A zip is read here (stored and deflated members, each checked against its CRC). Any other archive goes through
//! `bsdtar` (libarchive), which Windows 10 and later ship as `tar.exe` and Linux has as `bsdtar` (`libarchive-tools`);
//! when there is none, the message says so and how to unpack it by hand. Nothing in an archive is ever run, and only
//! a member's file name is used, so a path in an archive can't write outside the folder.

use std::path::{Path, PathBuf};

/// What came out of an archive: the VPKs written (in member order) and every other member, left in the archive.
#[derive(Debug, Default, PartialEq)]
pub struct Unpacked {
    pub vpks: Vec<PathBuf>,
    pub left: Vec<String>,
}

/// The largest member taken out: a VPK past this is not a mod, and a member that claims more is refused before it is read.
const MAX_MEMBER: u64 = 8 << 30;

/// Whether `p` is an archive `init` can take paks out of, by its extension.
pub fn is_archive(p: &Path) -> bool {
    let n = p.file_name().map(|n| n.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    [".zip", ".7z", ".rar", ".tar", ".tar.gz", ".tgz", ".tar.xz", ".tar.bz2"].iter().any(|e| n.ends_with(e))
}

/// The last component of a member's name, whichever separator the archive used.
fn base(name: &str) -> &str {
    name.rsplit(['/', '\\']).next().unwrap_or(name)
}

fn is_vpk(name: &str) -> bool {
    base(name).to_ascii_lowercase().ends_with(".vpk")
}

/// Members of one name in two places would be written over each other: say which, and stop.
fn one_of_each(names: &[String]) -> Result<(), String> {
    let mut seen: std::collections::BTreeMap<String, &str> = Default::default();
    for n in names.iter().filter(|n| is_vpk(n)) {
        if let Some(first) = seen.insert(base(n).to_ascii_lowercase(), n) {
            return Err(format!("two paks named {} in the archive ({first} and {n}): unpack it yourself and give each its own folder", base(n)));
        }
    }
    Ok(())
}

/// Take every VPK out of `archive` into `into` (which must exist), by its file name only.
pub fn unpack_vpks(archive: &Path, into: &Path) -> Result<Unpacked, String> {
    let lower = archive.to_string_lossy().to_ascii_lowercase();
    if lower.ends_with(".zip") {
        let data = std::fs::read(archive).map_err(|e| format!("{}: {e}", archive.display()))?;
        let members = zip_members(&data).map_err(|e| format!("{}: {e}", archive.display()))?;
        one_of_each(&members.iter().map(|m| m.name.clone()).collect::<Vec<_>>())?;
        let mut out = Unpacked::default();
        for m in &members {
            if m.name.ends_with('/') {
                continue;
            }
            if !is_vpk(&m.name) {
                out.left.push(m.name.clone());
                continue;
            }
            let bytes = zip_read(&data, m).map_err(|e| format!("{}: {}: {e}", archive.display(), m.name))?;
            let dest = into.join(base(&m.name));
            std::fs::write(&dest, bytes).map_err(|e| format!("{}: {e}", dest.display()))?;
            out.vpks.push(dest);
        }
        return Ok(out);
    }
    with_bsdtar(archive, into)
}

// ---------------------------------------------------------------------------------------------------------- zip

#[derive(Debug)]
pub(crate) struct Member {
    name: String,
    method: u16,
    crc: u32,
    packed: u64,
    size: u64,
    local: u64,
}

fn u16_at(d: &[u8], at: usize) -> Option<u16> {
    d.get(at..at + 2).map(|b| u16::from_le_bytes([b[0], b[1]]))
}
fn u32_at(d: &[u8], at: usize) -> Option<u32> {
    d.get(at..at + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

/// The central directory: every member's name, method, CRC, sizes and where its local header is.
pub(crate) fn zip_members(d: &[u8]) -> Result<Vec<Member>, String> {
    // The end record is in the last 64 KiB + 22 bytes (its comment is at most 64 KiB).
    let lo = d.len().saturating_sub(0xFFFF + 22);
    let eocd = (lo..d.len().saturating_sub(21)).rev().find(|&i| u32_at(d, i) == Some(0x0605_4b50)).ok_or("not a zip (no end of central directory)")?;
    let count = u16_at(d, eocd + 10).ok_or("truncated end record")? as usize;
    let mut at = u32_at(d, eocd + 16).ok_or("truncated end record")? as usize;
    if count == 0xFFFF || at == 0xFFFF_FFFF {
        return Err("a zip64 archive (over 4 GB or 65535 members): unpack it yourself".into());
    }
    let mut out = Vec::with_capacity(count.min(4096));
    for _ in 0..count {
        if u32_at(d, at) != Some(0x0201_4b50) {
            return Err("a damaged central directory".into());
        }
        let field = |o: usize| u16_at(d, at + o).ok_or("truncated central directory");
        let word = |o: usize| u32_at(d, at + o).ok_or("truncated central directory");
        let (flags, method) = (field(8)?, field(10)?);
        let (crc, packed, size) = (word(16)?, word(20)? as u64, word(24)? as u64);
        let (name_len, extra_len, comment_len) = (field(28)? as usize, field(30)? as usize, field(32)? as usize);
        let local = word(42)? as u64;
        let raw = d.get(at + 46..at + 46 + name_len).ok_or("truncated member name")?;
        // bit 11: the name is UTF-8; otherwise it is code page 437, which is ASCII for every name a mod has
        let name = if flags & 0x800 != 0 { String::from_utf8_lossy(raw).into_owned() } else { raw.iter().map(|&b| if b < 0x80 { b as char } else { '_' }).collect() };
        if flags & 1 != 0 && is_vpk(&name) {
            return Err(format!("{name} is encrypted: unpack it yourself"));
        }
        out.push(Member { name, method, crc, packed, size, local });
        at += 46 + name_len + extra_len + comment_len;
    }
    Ok(out)
}

pub(crate) fn zip_read(d: &[u8], m: &Member) -> Result<Vec<u8>, String> {
    if m.size > MAX_MEMBER || m.packed > MAX_MEMBER {
        return Err(format!("{} bytes: larger than any mod", m.size.max(m.packed)));
    }
    let at = usize::try_from(m.local).map_err(|_| "member out of range")?;
    if u32_at(d, at) != Some(0x0403_4b50) {
        return Err("no local header where the directory says".into());
    }
    let start = at + 30 + u16_at(d, at + 26).ok_or("truncated local header")? as usize + u16_at(d, at + 28).ok_or("truncated local header")? as usize;
    let packed = d.get(start..start + m.packed as usize).ok_or("the member runs past the end of the archive")?;
    let bytes = match m.method {
        0 => packed.to_vec(),
        8 => mark::zlib::inflate(packed)?,
        n => return Err(format!("compression method {n} is not one this reads (stored or deflate): unpack it yourself")),
    };
    if bytes.len() as u64 != m.size {
        return Err(format!("{} bytes, the directory says {}", bytes.len(), m.size));
    }
    if vpk::crc32(&bytes) != m.crc {
        return Err("its CRC does not match: a damaged download".into());
    }
    Ok(bytes)
}

// ------------------------------------------------------------------------------------------------------- bsdtar

/// A tar that reads `archive`: `bsdtar` where it is on PATH, else Windows' own `tar.exe` (which is bsdtar), else, for
/// a tar archive only, the system's `tar`. `None` when there is none: GNU tar reads no .7z or .rar.
fn bsdtar(archive: &Path) -> Option<PathBuf> {
    if crate::updateapply::run(std::process::Command::new("bsdtar").arg("--version"), Some(10)).is_ok_and(|r| r.ok) {
        return Some(PathBuf::from("bsdtar"));
    }
    let tar = crate::update::system_tool("tar");
    let n = archive.to_string_lossy().to_ascii_lowercase();
    (tar.is_absolute() || [".tar", ".tar.gz", ".tgz", ".tar.xz", ".tar.bz2"].iter().any(|e| n.ends_with(e))).then_some(tar)
}

fn with_bsdtar(archive: &Path, into: &Path) -> Result<Unpacked, String> {
    let how = "a .7z or .rar needs bsdtar (Windows 10 and later have it as tar.exe; on Linux install libarchive-tools), or unpack it yourself and give init the .vpk";
    let tar = bsdtar(archive).ok_or_else(|| format!("{}: no bsdtar on this machine to read it: {how}", archive.display()))?;
    let listed = crate::updateapply::run(std::process::Command::new(&tar).arg("-tf").arg(archive), Some(120)).map_err(|e| format!("{} did not run ({e}): {how}", tar.display()))?;
    if !listed.ok {
        return Err(format!("{} could not read {}: {}; {how}", tar.display(), archive.display(), listed.err.trim()));
    }
    let names: Vec<String> = listed.out.lines().map(str::to_string).filter(|n| !n.is_empty()).collect();
    one_of_each(&names)?;
    let mut out = Unpacked { left: names.iter().filter(|n| !n.ends_with('/') && !is_vpk(n)).cloned().collect(), ..Default::default() };
    let wanted: Vec<&String> = names.iter().filter(|n| is_vpk(n) && !n.ends_with('/')).collect();
    if wanted.is_empty() {
        return Ok(out);
    }
    // Into a fresh folder of its own, then each pak up by its file name; tar refuses `..` and absolute member paths.
    let tmp = into.join(format!(".magician-unpack-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).map_err(|e| format!("{}: {e}", tmp.display()))?;
    let mut cmd = std::process::Command::new(&tar);
    cmd.arg("-xf").arg(archive).arg("-C").arg(&tmp);
    cmd.args(wanted.iter().map(|s| s.as_str()));
    let r = crate::updateapply::run(&mut cmd, Some(600));
    let moved = (|| {
        let r = r.map_err(|e| format!("{} did not run ({e}): {how}", tar.display()))?;
        if !r.ok {
            return Err(format!("{} could not unpack {}: {}", tar.display(), archive.display(), r.err.trim()));
        }
        for n in &wanted {
            let from = tmp.join(n.replace('\\', "/"));
            let dest = into.join(base(n));
            std::fs::rename(&from, &dest).map_err(|e| format!("{}: {e}", from.display()))?;
            out.vpks.push(dest);
        }
        Ok(())
    })();
    let _ = std::fs::remove_dir_all(&tmp);
    moved.map(|_| out)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A zip of `(name, data, deflate?)` members, written the way any zip tool writes one.
    pub(crate) fn zip(members: &[(&str, &[u8], bool)]) -> Vec<u8> {
        let (mut body, mut dir) = (Vec::new(), Vec::new());
        for (name, data, deflate) in members {
            let packed = if *deflate { mark::zlib::deflate(data) } else { data.to_vec() };
            let at = body.len() as u32;
            let head = |sig: u32, central: bool| {
                let mut h = sig.to_le_bytes().to_vec();
                if central {
                    h.extend(20u16.to_le_bytes());
                }
                h.extend(20u16.to_le_bytes());
                h.extend(0x800u16.to_le_bytes());
                h.extend((if *deflate { 8u16 } else { 0 }).to_le_bytes());
                h.extend([0u8; 4]);
                h.extend(vpk::crc32(data).to_le_bytes());
                h.extend((packed.len() as u32).to_le_bytes());
                h.extend((data.len() as u32).to_le_bytes());
                h.extend((name.len() as u16).to_le_bytes());
                h.extend(0u16.to_le_bytes());
                h
            };
            body.extend(head(0x0403_4b50, false));
            body.extend(name.as_bytes());
            body.extend(&packed);
            let mut c = head(0x0201_4b50, true);
            c.extend([0u8; 10]);
            c.extend(at.to_le_bytes());
            c.extend(name.as_bytes());
            dir.extend(c);
        }
        let (dir_at, dir_len) = (body.len() as u32, dir.len() as u32);
        body.extend(dir);
        body.extend(0x0605_4b50u32.to_le_bytes());
        body.extend([0u8; 4]);
        body.extend((members.len() as u16).to_le_bytes());
        body.extend((members.len() as u16).to_le_bytes());
        body.extend(dir_len.to_le_bytes());
        body.extend(dir_at.to_le_bytes());
        body.extend(0u16.to_le_bytes());
        body
    }

    fn scratch(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("magician-archive-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn a_zip_gives_up_its_paks_and_nothing_else() {
        let d = scratch("zip");
        let big: Vec<u8> = (0..20000u32).map(|i| (i % 7) as u8).collect();
        let z = d.join("Cool Skin v2.zip");
        std::fs::write(&z, zip(&[("Cool Skin/pak01_dir.vpk", &big, true), ("Cool Skin/readme.txt", b"hi", false), ("extra.vpk", b"vpk", false), ("Cool Skin/", b"", false)])).unwrap();
        let into = d.join("out");
        std::fs::create_dir_all(&into).unwrap();
        let u = unpack_vpks(&z, &into).unwrap();
        assert_eq!(u.vpks, vec![into.join("pak01_dir.vpk"), into.join("extra.vpk")]);
        assert_eq!(u.left, vec!["Cool Skin/readme.txt".to_string()]);
        assert_eq!(std::fs::read(into.join("pak01_dir.vpk")).unwrap(), big);
        assert!(!into.join("readme.txt").exists(), "only paks come out");
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn a_damaged_or_ambiguous_zip_is_refused_with_the_reason() {
        let d = scratch("bad");
        let into = d.join("out");
        std::fs::create_dir_all(&into).unwrap();
        let mut z = zip(&[("pak01_dir.vpk", b"0123456789", false)]);
        let at = z.windows(10).position(|w| w == b"0123456789").unwrap();
        z[at] = b'X';
        std::fs::write(d.join("a.zip"), &z).unwrap();
        assert!(unpack_vpks(&d.join("a.zip"), &into).unwrap_err().contains("CRC does not match"));
        std::fs::write(d.join("b.zip"), zip(&[("a/pak01_dir.vpk", b"1", false), ("b/pak01_dir.vpk", b"2", false)])).unwrap();
        assert!(unpack_vpks(&d.join("b.zip"), &into).unwrap_err().contains("two paks named pak01_dir.vpk"));
        std::fs::write(d.join("c.zip"), b"not a zip at all").unwrap();
        assert!(unpack_vpks(&d.join("c.zip"), &into).unwrap_err().contains("not a zip"));
        // a member path that climbs out is written by its name only, inside the folder
        std::fs::write(d.join("d.zip"), zip(&[("../../escape.vpk", b"x", false)])).unwrap();
        assert_eq!(unpack_vpks(&d.join("d.zip"), &into).unwrap().vpks, vec![into.join("escape.vpk")]);
        assert!(!d.join("escape.vpk").exists());
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn archives_are_known_by_their_extension() {
        assert!(is_archive(Path::new("x/Mod.ZIP")) && is_archive(Path::new("m.7z")) && is_archive(Path::new("m.tar.gz")));
        assert!(!is_archive(Path::new("pak01_dir.vpk")) && !is_archive(Path::new("zip")));
    }
}

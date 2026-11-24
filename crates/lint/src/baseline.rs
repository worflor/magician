//! A baseline is one game build's file table: path -> (crc32, size). It is the "era" every mod
//! is checked against. Two on-disk forms are read:
//!   - Magician TSV:   `# magician baseline build=6861` then `path<TAB>crc_hex<TAB>size`
//!   - SteamDB GameTracking `pak01_dir.txt`: `path CRC:00xxxxxxxx size:N`
//!
//! so a baseline for any historical build can be built without that build installed.

use std::collections::HashMap;
use std::io::{self, Write};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileInfo {
    pub crc: u32,
    pub size: u64,
}

#[derive(Debug, Default, Clone)]
pub struct Baseline {
    pub build: String,
    pub files: HashMap<String, FileInfo>,
}

impl Baseline {
    pub fn get(&self, path: &str) -> Option<FileInfo> {
        self.files.get(path).copied()
    }
    pub fn contains(&self, path: &str) -> bool {
        self.files.contains_key(path)
    }

    pub fn from_vpks(build: &str, vpks: &[&vpk::Vpk]) -> Baseline {
        let mut files = HashMap::new();
        // Earlier archives win, matching search-path order (citadel before core).
        for v in vpks {
            for e in &v.entries {
                files.entry(e.path.clone()).or_insert(FileInfo { crc: e.crc, size: e.size() });
            }
        }
        Baseline { build: build.to_string(), files }
    }

    pub fn parse(text: &str) -> Result<Baseline, String> {
        let mut b = Baseline::default();
        for (n, line) in text.lines().enumerate() {
            let line = line.trim_end();
            if line.is_empty() {
                continue;
            }
            if let Some(rest) = line.strip_prefix('#') {
                if let Some(i) = rest.find("build=") {
                    b.build = rest[i + 6..].split_whitespace().next().unwrap_or("").to_string();
                }
                continue;
            }
            if let Some((path, rest)) = line.split_once('\t') {
                let mut it = rest.split('\t');
                let crc = u32::from_str_radix(it.next().unwrap_or(""), 16).map_err(|_| format!("line {}: bad crc", n + 1))?;
                let size = it.next().unwrap_or("0").parse().map_err(|_| format!("line {}: bad size", n + 1))?;
                b.files.insert(path.to_string(), FileInfo { crc, size });
            } else if let Some(ci) = line.find(" CRC:") {
                // SteamDB form. CRC is printed as 10 hex digits with a leading "00".
                let path = &line[..ci];
                let rest = &line[ci + 5..];
                let (crc_s, rest) = rest.split_once(' ').unwrap_or((rest, ""));
                let crc = u64::from_str_radix(crc_s, 16).map_err(|_| format!("line {}: bad crc", n + 1))? as u32;
                let size = rest.strip_prefix("size:").and_then(|s| s.parse().ok()).unwrap_or(0);
                b.files.insert(path.to_string(), FileInfo { crc, size });
            } else {
                return Err(format!("line {}: unrecognised baseline line", n + 1));
            }
        }
        Ok(b)
    }

    pub fn load(path: impl AsRef<Path>) -> Result<Baseline, String> {
        let p = path.as_ref();
        let text = std::fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()))?;
        Baseline::parse(&text)
    }

    pub fn write(&self, mut w: impl Write) -> io::Result<()> {
        writeln!(w, "# magician baseline build={} files={}", self.build, self.files.len())?;
        let mut keys: Vec<_> = self.files.keys().collect();
        keys.sort();
        for k in keys {
            let f = self.files[k];
            writeln!(w, "{k}\t{:08x}\t{}", f.crc, f.size)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_both_forms() {
        let tsv = "# magician baseline build=6861\nmodels/a.vmdl_c\t0034b4e1\t12\n";
        let b = Baseline::parse(tsv).unwrap();
        assert_eq!(b.build, "6861");
        assert_eq!(b.get("models/a.vmdl_c"), Some(FileInfo { crc: 0x0034b4e1, size: 12 }));

        let steamdb = "models/heroes_wip/lash/lash.vmdl_c CRC:006711352c size:5490631\n";
        let s = Baseline::parse(steamdb).unwrap();
        assert_eq!(s.get("models/heroes_wip/lash/lash.vmdl_c"), Some(FileInfo { crc: 0x6711352c, size: 5490631 }));
    }
}

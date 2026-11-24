//! Fingerprints the code whose output the findings cache stores (the lint rules and everything they read
//! through: mesh decode, geometry, resources, VPK), so a rebuild after a rule change can't serve findings
//! from the older rules. Tests and fixtures are left out: they don't change what the rules say.

use std::path::{Path, PathBuf};

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    let mut entries: Vec<PathBuf> = rd.filter_map(|e| e.ok().map(|e| e.path())).collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            collect(&p, out);
        } else if p.extension().is_some_and(|e| e == "rs") {
            let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            if name != "tests.rs" && name != "mesh_fixtures.rs" {
                out.push(p);
            }
        }
    }
}

fn main() {
    let root = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".into()));
    let mut files = Vec::new();
    for krate in ["lint", "mesh", "geom", "resource", "vpk"] {
        let dir = root.join("..").join(krate).join("src");
        println!("cargo:rerun-if-changed={}", dir.display());
        collect(&dir, &mut files);
    }
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut eat = |bytes: &[u8]| {
        for &b in bytes {
            h ^= b as u64;
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
    };
    for f in &files {
        let rel = f.strip_prefix(&root).unwrap_or(f);
        eat(rel.to_string_lossy().replace('\\', "/").as_bytes());
        if let Ok(b) = std::fs::read(f) {
            // line endings differ between checkouts: they don't change behaviour
            let b: Vec<u8> = b.into_iter().filter(|c| *c != b'\r').collect();
            eat(&b);
        }
    }
    println!("cargo:rustc-env=LINT_CODE_HASH={h:016x}");
}

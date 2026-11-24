//! The VPK reader on hostile archives: every directory tree and entry it is handed must come back as an error or as
//! data, never a panic, a hang or a buffer sized by a length the archive made up. And what `write` packs, `open` reads
//! back exactly, whatever the names and contents. See `crates/fuzz`.

#![allow(clippy::unwrap_used, clippy::expect_used)] // a test that cannot unwrap has failed

#[global_allocator]
static CAP: fuzz::Capped = fuzz::Capped;

/// A scratch folder of its own for every caller (the tests run in parallel).
fn scratch(name: &str) -> std::path::PathBuf {
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let d = std::env::temp_dir().join(format!("magician-vpk-fuzz-{name}-{}-{n}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn packed(files: &[(&str, &[u8])]) -> Vec<u8> {
    let d = scratch("seed");
    let p = d.join("seed_dir.vpk");
    let owned: Vec<(String, Vec<u8>)> = files.iter().map(|(n, b)| (n.to_string(), b.to_vec())).collect();
    vpk::write(&p, &owned).unwrap();
    let b = std::fs::read(&p).unwrap();
    let _ = std::fs::remove_dir_all(&d);
    b
}

#[test]
fn archives() {
    let mut seeds = vec![
        packed(&[]),
        packed(&[("materials/a.vmat_c", b"mat"), ("materials/b.vtex_c", &[0u8; 300]), ("models/x/y.vmdl_c", b"model bytes")]),
        packed(&[("noext", b"1"), ("dir/only", b""), ("a.b.c", b"dots"), ("deep/er/still/x.txt", &[9u8; 40])]),
    ];
    seeds.extend(fuzz::corpus(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("fuzz-cases").join("archives")));
    let d = scratch("cases");
    let path = d.join("case_dir.bin");
    let cleanup = d.clone();
    // one file per case: the reader opens archives by path, so fewer cases than the in-memory targets
    fuzz::run("vpk", fuzz::DEFAULT_CASES / 8, seeds, move |b| {
        std::fs::write(&path, b).unwrap();
        let Ok(v) = vpk::Vpk::open(&path) else { return };
        for e in v.entries.iter().take(64) {
            let _ = v.read(e);
            let _ = v.read_range(e, 0, 16);
            let _ = v.read_range(e, u64::MAX - 4, 16);
            let _ = v.find(&e.path);
        }
    });
    let _ = std::fs::remove_dir_all(cleanup);
}

#[test]
fn what_write_packs_open_reads_back() {
    let d = scratch("round");
    let p = d.join("round_dir.bin");
    let mut rng = fuzz::Rng::new(5);
    let parts = ["materials", "models", "a", "x_y", "sounds/vo", "é", "dir.with.dots", " "];
    for _ in 0..50 {
        let mut files: Vec<(String, Vec<u8>)> = Vec::new();
        for _ in 0..rng.below(12) {
            let dir = (0..rng.below(3)).map(|_| *rng.pick(&parts)).collect::<Vec<_>>().join("/");
            let name = format!("{}{}", rng.pick(&["f", "file", "x"]), rng.below(1000));
            let ext = *rng.pick(&["", ".vmat_c", ".txt", ".a.b"]);
            let path = if dir.is_empty() { format!("{name}{ext}") } else { format!("{dir}/{name}{ext}") };
            if path.contains(" /") || path.starts_with(' ') || files.iter().any(|(p, _)| *p == path) {
                continue;
            }
            files.push((path, (0..rng.below(2000)).map(|_| rng.next_u64() as u8).collect()));
        }
        vpk::write(&p, &files).unwrap();
        let v = vpk::Vpk::open(&p).unwrap();
        let mut back: Vec<(String, Vec<u8>)> = v.entries.iter().map(|e| (e.path.clone(), v.read(e).unwrap())).collect();
        back.sort();
        files.sort();
        assert_eq!(back, files);
        for e in &v.entries {
            assert_eq!(e.crc, vpk::crc32(&v.read(e).unwrap()), "{}", e.path);
        }
        // packing is deterministic
        let first = std::fs::read(&p).unwrap();
        files.reverse();
        vpk::write(&p, &files).unwrap();
        assert_eq!(std::fs::read(&p).unwrap(), first);
    }
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn a_tree_or_an_entry_longer_than_the_archive_is_an_error_not_a_buffer() {
    let d = scratch("hostile");
    let p = d.join("x_dir.vpk");
    let mut b = packed(&[("a/b.txt", b"hello")]);
    // the tree size field (bytes 8..12) says 4 GiB
    b[8..12].copy_from_slice(&u32::MAX.to_le_bytes());
    std::fs::write(&p, &b).unwrap();
    let e = vpk::Vpk::open(&p).unwrap_err().to_string();
    assert!(e.contains("runs past the end"), "{e}");
    // an entry whose length says 4 GiB: the archive opens, the read is refused
    let good = packed(&[("a/b.txt", b"hello")]);
    let at = good.windows(2).rposition(|w| w == [0xff, 0xff]).unwrap() - 4; // length, just before the terminator
    let mut bad = good.clone();
    bad[at..at + 4].copy_from_slice(&u32::MAX.to_le_bytes());
    std::fs::write(&p, &bad).unwrap();
    let v = vpk::Vpk::open(&p).unwrap();
    let e = v.read(&v.entries[0]).unwrap_err().to_string();
    assert!(e.contains("a/b.txt") && e.contains("run past the end"), "{e}");
    let _ = std::fs::remove_dir_all(&d);
}

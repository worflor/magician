//! The maker's mark reads a mod's own colour sources (PNG, TGA, PSD, through its zlib inflate) to sign and detect.
//! On hostile files each reader must return, never panic, hang or allocate from a header's dimensions; and what the
//! writers write, the readers read back. See `crates/fuzz`.

#![allow(clippy::unwrap_used, clippy::expect_used)] // a test that cannot unwrap has failed

#[global_allocator]
static CAP: fuzz::Capped = fuzz::Capped;

fn rgba(w: usize, h: usize, rng: &mut fuzz::Rng) -> Vec<u8> {
    (0..w * h * 4).map(|i| if i % 4 == 3 { 255 } else { (rng.next_u64() as u8) >> 2 << 2 }).collect()
}

#[test]
fn image_readers() {
    let mut rng = fuzz::Rng::new(2);
    let px = rgba(9, 7, &mut rng);
    let mut seeds = Vec::new();
    for ct in [2u8, 6] {
        seeds.push(mark::png::write(9, 7, &px, ct, &[]));
    }
    let tm = mark::tga::Meta { image_type: 2, bpp: 32, descriptor: 8, id: vec![] };
    seeds.push(mark::tga::write(9, 7, &px, &tm));
    seeds.push(mark::zlib::zlib(&px));
    seeds.extend(fuzz::corpus(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("fuzz-cases").join("images")));
    fuzz::run("images", fuzz::DEFAULT_CASES * 2, seeds, |b| {
        let _ = mark::png::read(b);
        let _ = mark::tga::read(b);
        let _ = mark::psd::read(b);
        let _ = mark::zlib::unzlib(b);
        let _ = mark::zlib::inflate(b);
    });
}

#[test]
fn deflate_and_png_read_back_what_they_write() {
    let mut rng = fuzz::Rng::new(8);
    for _ in 0..100 {
        let n = rng.below(5000);
        let data: Vec<u8> = (0..n).map(|i| if rng.chance(2) { (i % 7) as u8 } else { rng.next_u64() as u8 }).collect();
        assert_eq!(mark::zlib::unzlib(&mark::zlib::zlib(&data)).unwrap(), data);
        let (w, h) = (1 + rng.below(20), 1 + rng.below(20));
        let px = rgba(w, h, &mut rng);
        let (rw, rh, back, _) = mark::png::read(&mark::png::write(w, h, &px, 6, &[])).unwrap();
        assert_eq!((rw, rh, back), (w, h, px));
    }
}

#[test]
fn an_image_header_larger_than_its_data_sizes_nothing() {
    // TGA, 65535 x 32767 at 32 bits (8.6 GB) in 30 bytes: found by the long fuzz
    let mut tga = vec![0u8, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    tga.extend(65535u16.to_le_bytes());
    tga.extend(32767u16.to_le_bytes());
    tga.extend([32, 8]);
    tga.extend([0u8; 12]);
    assert!(mark::tga::read(&tga).unwrap_err().contains("larger than its"));
    tga[2] = 10; // the same, RLE
    assert!(mark::tga::read(&tga).is_err());
    // PSD, one row four billion pixels wide, RLE: three row counts and six bytes of data
    let mut psd = b"8BPS".to_vec();
    psd.extend(1u16.to_be_bytes());
    psd.extend([0u8; 6]);
    psd.extend(3u16.to_be_bytes());
    psd.extend(1u32.to_be_bytes());
    psd.extend(u32::MAX.to_be_bytes());
    psd.extend(8u16.to_be_bytes());
    psd.extend(3u16.to_be_bytes());
    psd.extend([0u8; 12]);
    psd.extend(1u16.to_be_bytes());
    for _ in 0..3 {
        psd.extend(2u16.to_be_bytes());
    }
    psd.extend([0x81, 7, 0x81, 7, 0x81, 7]);
    assert!(mark::psd::read(&psd).is_err());
}

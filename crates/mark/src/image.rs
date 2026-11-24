//! A colour source on disk: PNG, TGA or PSD, decoded to RGBA8 and written back in its own container.

use crate::{png, psd, tga};

#[derive(Debug, Clone)]
pub enum Container {
    Png(png::Meta),
    Tga(tga::Meta),
    Psd(psd::Meta),
}

#[derive(Debug, Clone)]
pub struct Image {
    pub w: usize,
    pub h: usize,
    /// RGBA8, row-major, top row first.
    pub rgba: Vec<u8>,
}

impl Image {
    pub fn new(w: usize, h: usize, rgba: Vec<u8>) -> Image {
        assert_eq!(rgba.len(), w * h * 4);
        Image { w, h, rgba }
    }
    pub fn alpha(&self) -> Vec<u8> {
        self.rgba.chunks(4).map(|p| p[3]).collect()
    }
}

pub fn ext(path: &str) -> String {
    path.rsplit('.').next().unwrap_or("").to_ascii_lowercase()
}

pub fn decode(path: &str, bytes: &[u8]) -> Result<(Image, Container), String> {
    match ext(path).as_str() {
        "png" => png::read(bytes).map(|(w, h, p, m)| (Image::new(w, h, p), Container::Png(m))),
        "tga" => tga::read(bytes).map(|(w, h, p, m)| (Image::new(w, h, p), Container::Tga(m))),
        "psd" => psd::read(bytes).map(|(w, h, p, m)| (Image::new(w, h, p), Container::Psd(m))),
        e => Err(format!("unsupported image type .{e}")),
    }
}

pub fn encode(img: &Image, c: &Container) -> Result<Vec<u8>, String> {
    match c {
        Container::Png(m) => Ok(png::write(img.w, img.h, &img.rgba, png::fit_color_type(m.color_type, &img.rgba), &m.ancillary)),
        Container::Tga(m) => Ok(tga::write(img.w, img.h, &img.rgba, m)),
        Container::Psd(m) => {
            if m.has_layers {
                return Err("layered PSD: a reader that composites the layers would not see the mark".into());
            }
            psd::write(img.w, img.h, &img.rgba, m)
        }
    }
}

pub fn load(path: &std::path::Path) -> Result<(Image, Container), String> {
    let b = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    decode(&path.to_string_lossy(), &b).map_err(|e| format!("{}: {e}", path.display()))
}

/// Nearest-neighbour resample (for a translucency map whose size differs from its colour map's).
pub fn resample_nearest(src: &[u8], sw: usize, sh: usize, w: usize, h: usize) -> Vec<u8> {
    let mut out = vec![0u8; w * h];
    for y in 0..h {
        let sy = ((2 * y + 1) * sh / (2 * h)).min(sh - 1);
        for x in 0..w {
            let sx = ((2 * x + 1) * sw / (2 * w)).min(sw - 1);
            out[y * w + x] = src[sy * sw + sx];
        }
    }
    out
}

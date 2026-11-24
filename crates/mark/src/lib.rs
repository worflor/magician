//! Magician's maker's mark: an invisible, keyed authorship watermark in the colour textures a mod ships.
//!
//! The identity key is the author's public name (a mod's `[mark] key`, else its `[mod] author`): the mark proves
//! authorship by being there, with a payload (mod id and signing date) that only the key decodes, not by secrecy.
//! There is no default key: a mark signed under someone else's name would claim their authorship. A private secondary key, read from an environment
//! variable and never written anywhere, adds a second pattern for a claim nobody else can reproduce.
//!
//! - `scheme`: the maths (grid, patterns, embedding, blind detection, Hoeffding bounds, payload by Hadamard
//!   code division, pooling across a pack).
//! - `png`, `tga`, `psd`, `zlib`, `image`: the source containers, read and written natively.
//! - `bc`: BC1/BC3/BC7 encoders and a compiled-texture container, for measuring survival without the compiler.
//! - `metrics`: PSNR, CIEDE2000, linear-light mips and resizing.
//! - `addon`: which sources in an addon are colour, the sidecar manifest, `sign` and `verify`.

#![forbid(unsafe_code)]

pub mod addon;
pub mod bc;
pub mod image;
pub mod metrics;
pub mod png;
pub mod psd;
pub mod scheme;
pub mod tga;
pub mod zlib;

#[cfg(test)]
mod tests;

/// The environment variable a private secondary key is read from (never stored).
pub const SECRET_ENV: &str = "MAGICIAN_MARK_SECRET";

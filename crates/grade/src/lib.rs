//! Grading. `footprint` discovers what a hero is made of from the game's own data; `rubric` judges a mod's depth
//! against it; `identity` decides what kind of mod it is; `compliance` reads fair play from the lint run; `record`
//! puts the three axes, freshness, rights and the release gate into one reproducible record and its label. `policy`
//! holds every judgement number, defaulting to the constants in `rubric.rs`.

#![forbid(unsafe_code)]

pub mod compliance;
pub mod evidence;
pub mod footprint;
pub use footprint::{Cat, Footprint, Item, Sources};
pub mod identity;
pub mod json;
pub mod lintfacts;
pub mod manifest;
pub mod policy;
pub mod record;
pub mod rubric;
pub mod sha256;
pub use policy::Policy;
pub use rubric::{grade, Archetype, ModFacts, Report, CRITERIA};

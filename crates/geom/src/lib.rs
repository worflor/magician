//! Magician's geometry kernel: the math of scene3d's compiler, ported to Rust and pointed at compiled
//! game meshes. Which mesh rule uses which module is in `crates/lint/src/mesh/`.

#![forbid(unsafe_code)]

pub mod bvh;
pub mod clip;
pub mod exact;
pub mod font;
pub mod image;
pub mod intersect;
pub mod mass;
pub mod quality;
pub mod raster;
pub mod shape;
pub mod topo;
pub mod triangulate;
pub mod v3;

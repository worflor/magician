//! Triangle quality, dimensionless so a 3 mm buckle and a 2 m cape are judged alike
//! (scene3d runner: zero-area as area < 1e-6 x longest edge squared; sliver aspect as
//! longest^2 / (2 x area), ~1.15 equilateral, unbounded as a triangle degenerates; role budget
//! for a hero/character: 20).

use crate::v3::{self, V3};

pub const ZERO_AREA_RATIO: f64 = 1e-6;
/// scene3d `ROLE_PROFILES.hero.maxAspectRatio`: a hero is looked at closely and deforms.
pub const HERO_MAX_ASPECT: f64 = 20.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TriQuality {
    pub area: f64,
    pub longest: f64,
    pub aspect: f64,
    pub zero_area: bool,
    /// Smallest corner angle in degrees.
    pub min_angle: f64,
}

pub fn quality(a: V3, b: V3, c: V3) -> TriQuality {
    let (ab, bc, ca) = (v3::dist(a, b), v3::dist(b, c), v3::dist(c, a));
    let longest = ab.max(bc).max(ca);
    let area = v3::tri_area(a, b, c);
    let zero_area = longest <= 0.0 || area < ZERO_AREA_RATIO * longest * longest;
    let aspect = if zero_area { f64::INFINITY } else { longest * longest / (2.0 * area) };
    let ang = |p: V3, q: V3, r: V3| {
        let (u, w) = (v3::sub(q, p), v3::sub(r, p));
        let d = v3::len(u) * v3::len(w);
        if d > 0.0 {
            (v3::dot(u, w) / d).clamp(-1.0, 1.0).acos().to_degrees()
        } else {
            0.0
        }
    };
    let min_angle = ang(a, b, c).min(ang(b, c, a)).min(ang(c, a, b));
    TriQuality { area, longest, aspect, zero_area, min_angle }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn known_answers() {
        let e = quality([0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.5, 3f64.sqrt() / 2.0, 0.0]);
        assert!((e.aspect - 2.0 / 3f64.sqrt()).abs() < 1e-12 && (e.min_angle - 60.0).abs() < 1e-9);
        let s = quality([0.0, 0.0, 0.0], [100.0, 0.0, 0.0], [50.0, 1.0, 0.0]);
        assert!((s.aspect - 100.0).abs() < 1e-9 && !s.zero_area);
        let z = quality([0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [2.0, 0.0, 0.0]);
        assert!(z.zero_area);
        // scale-free: the same shape at 1e-3 scale reads the same
        let t = quality([0.0, 0.0, 0.0], [0.1, 0.0, 0.0], [0.05, 0.001, 0.0]);
        assert!((t.aspect - 100.0).abs() < 1e-6);
    }
}

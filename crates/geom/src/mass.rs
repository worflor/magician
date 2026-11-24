//! Mass properties of a closed triangle surface (scene3d `kernel/mass.ts`): volume, centroid and
//! inertia tensor by the signed-tetrahedron decomposition (Tonon 2004: the barycentric second
//! moment integral is V(1 + delta_kl)/20), the principal moments' characteristic cubic and its
//! discriminant (zero iff two principal moments coincide: an axis of mass symmetry).
//!
//! What stays exact: the signed volume, summed as an `Exact` expansion of float32-exact vertex
//! products (scene3d's rational volume, without big integers), so "encloses no volume" and the
//! winding sign are decided exactly. The first and second moments are float sums (scene3d keeps
//! them rational; a 300k-triangle shell does not need it, and the report rounds anyway).
//! `conditioning` is scene3d's sum of |tet| / 6, the scale of the float error bound.

use crate::exact::{det3, Exact};
use crate::v3::V3;

#[derive(Debug, Clone, PartialEq)]
pub struct Mass {
    pub volume: f64,
    /// The exact sign of the raw signed volume: +1 outward-wound, -1 inward-wound, 0 none.
    pub winding: i8,
    pub centroid: V3,
    pub inertia: [[f64; 3]; 3],
    pub char_poly: [f64; 3],
    pub discriminant: f64,
    pub conditioning: f64,
}

pub fn mass(pos: &[V3], tris: &[[u32; 3]]) -> Option<Mass> {
    let mut six = Exact::zero();
    let mut six_f = 0.0;
    let mut cond = 0.0;
    let mut m1 = [0.0; 3];
    let mut c120 = [[0.0; 3]; 3];
    for t in tris {
        let (a, b, c) = (pos[t[0] as usize], pos[t[1] as usize], pos[t[2] as usize]);
        let d = det3(a, b, c);
        let df = d.approx();
        six.add_assign(&d);
        six_f += df;
        cond += df.abs();
        let s = [a[0] + b[0] + c[0], a[1] + b[1] + c[1], a[2] + b[2] + c[2]];
        for i in 0..3 {
            m1[i] += df * s[i];
            for j in 0..3 {
                let q = a[i] * a[j] + b[i] * b[j] + c[i] * c[j];
                c120[i][j] += df * (s[i] * s[j] + q);
            }
        }
    }
    let winding = six.sign();
    if winding == 0 {
        return None;
    }
    let sign = winding as f64;
    let six_v = six.approx();
    let volume = six_v * sign / 6.0;
    let _ = six_f;
    let centroid = [m1[0] / 24.0 / volume * sign, m1[1] / 24.0 / volume * sign, m1[2] / 24.0 / volume * sign];
    let mut cm = [[0.0; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            cm[i][j] = c120[i][j] / 120.0 * sign;
        }
    }
    let tr = cm[0][0] + cm[1][1] + cm[2][2];
    let cc = centroid[0] * centroid[0] + centroid[1] * centroid[1] + centroid[2] * centroid[2];
    let mut inertia = [[0.0; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            let iorg = if i == j { tr - cm[i][j] } else { -cm[i][j] };
            let shell = if i == j { cc } else { 0.0 } - centroid[i] * centroid[j];
            inertia[i][j] = iorg - volume * shell;
        }
    }
    let m = inertia;
    let a = -(m[0][0] + m[1][1] + m[2][2]);
    let b = m[0][0] * m[1][1] - m[0][1] * m[1][0] + m[0][0] * m[2][2] - m[0][2] * m[2][0] + m[1][1] * m[2][2] - m[1][2] * m[2][1];
    let det = m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1]) - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0]) + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0]);
    let c = -det;
    let disc = 18.0 * a * b * c - 4.0 * a * a * a * c + a * a * b * b - 4.0 * b * b * b - 27.0 * c * c;
    Some(Mass { volume, winding, centroid, inertia, char_poly: [a, b, c], discriminant: disc, conditioning: cond / 6.0 })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::topo::tests::cube;

    #[test]
    fn cube_known_answers() {
        let (p, t) = cube([1.0, 2.0, 3.0], 2.0);
        let m = mass(&p, &t).unwrap();
        assert_eq!(m.winding, 1, "the test cube is outward-wound");
        assert!((m.volume - 8.0).abs() < 1e-12);
        assert!(m.centroid.iter().zip([2.0, 3.0, 4.0]).all(|(a, b)| (a - b).abs() < 1e-12));
        // solid cube side s, mass s^3: I = m s^2 / 6 on each axis
        for k in 0..3 {
            assert!((m.inertia[k][k] - 8.0 * 4.0 / 6.0).abs() < 1e-9, "{:?}", m.inertia);
        }
        assert!(m.discriminant.abs() < 1e-6, "a cube is a spherical top");
        let flipped: Vec<[u32; 3]> = t.iter().map(|x| [x[0], x[2], x[1]]).collect();
        let f = mass(&p, &flipped).unwrap();
        assert_eq!(f.winding, -1);
        assert!((f.volume - 8.0).abs() < 1e-12);
        // an open fan encloses nothing exactly when it is flat
        let (p0, t0) = cube([0.0; 3], 1.0);
        assert!(mass(&p0, &t0[0..2]).is_none());
    }
}

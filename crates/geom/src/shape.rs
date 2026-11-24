//! Shape facts that need no camera: bilateral symmetry (scene3d runner `symmetry_facts`) and the
//! spectral shape-DNA of a shell (scene3d runner `spectrum_facts`).
//!
//! Symmetry: for sampled vertices, the distance from the mirrored point to the nearest actual
//! vertex, across the best of six candidate planes (three axis planes through the bounds centre,
//! three principal planes of the vertex covariance, from Jacobi rotations). Exact symmetry reads
//! ~0. A fact, never a verdict: an asymmetric outfit is a design.
//!
//! Spectrum: eigenvalues of the uniform graph Laplacian L = D - A of a shell's edge graph (topology
//! and wiring, not millimetres: cotangent weights blow up on slivers), divided by the first nonzero
//! one so a scaled copy matches, rounded to six significant digits. Two shells with the same
//! spectrum are the same wiring: a copied accessory, a mirrored pair. Eigenvalues only (no
//! eigenvector sign or rotation freedom), by cyclic Jacobi on the dense matrix, capped at
//! `SPECTRUM_VERT_CAP` vertices (scene3d's cap is 2000 with LAPACK; Jacobi here is O(n^3) per
//! sweep, so the cap is lower and a skipped shell says so).

use crate::v3::{self, V3};
use std::collections::HashMap;

pub const SYMMETRY_PROBES: usize = 2048;
pub const SPECTRUM_VERT_CAP: usize = 200;
pub const SPECTRUM_K: usize = 12;

#[derive(Debug, Clone, PartialEq)]
pub struct Symmetry {
    /// Nearest world axis to the winning plane's normal.
    pub axis: char,
    pub normal: V3,
    pub max_error: f64,
    pub mean_error: f64,
    pub sampled: bool,
}

struct Grid {
    cell: f64,
    map: HashMap<(i64, i64, i64), Vec<u32>>,
}

impl Grid {
    fn new(pts: &[V3], cell: f64) -> Grid {
        let mut map: HashMap<(i64, i64, i64), Vec<u32>> = HashMap::new();
        for (i, p) in pts.iter().enumerate() {
            map.entry(Self::k(p, cell)).or_default().push(i as u32);
        }
        Grid { cell, map }
    }
    fn k(p: &V3, cell: f64) -> (i64, i64, i64) {
        ((p[0] / cell).floor() as i64, (p[1] / cell).floor() as i64, (p[2] / cell).floor() as i64)
    }
    /// Nearest point, searching rings outward until a ring cannot improve.
    fn nearest(&self, pts: &[V3], q: V3) -> f64 {
        let c = Self::k(&q, self.cell);
        let mut best = f64::INFINITY;
        for r in 0..64i64 {
            if (r - 1) as f64 * self.cell > best {
                break;
            }
            for dx in -r..=r {
                for dy in -r..=r {
                    for dz in -r..=r {
                        if dx.abs().max(dy.abs()).max(dz.abs()) != r {
                            continue;
                        }
                        if let Some(v) = self.map.get(&(c.0 + dx, c.1 + dy, c.2 + dz)) {
                            for &i in v {
                                best = best.min(v3::dist(pts[i as usize], q));
                            }
                        }
                    }
                }
            }
        }
        best
    }
}

pub fn symmetry(pts: &[V3]) -> Option<Symmetry> {
    let n = pts.len();
    if n < 4 {
        return None;
    }
    let stride = (n / SYMMETRY_PROBES).max(1);
    let mut bb = v3::Aabb::empty();
    pts.iter().for_each(|p| bb.grow(*p));
    let diag = v3::len(bb.size()).max(1e-9);
    let cell = (diag / (n as f64).cbrt()).max(diag * 1e-4);
    let grid = Grid::new(pts, cell);
    // principal axes of the sampled covariance
    let mut mean = [0.0; 3];
    let mut m = 0.0;
    for p in pts.iter().step_by(stride) {
        mean = v3::add(mean, *p);
        m += 1.0;
    }
    mean = v3::scale(mean, 1.0 / m);
    let mut c = [[0.0; 3]; 3];
    for p in pts.iter().step_by(stride) {
        let d = v3::sub(*p, mean);
        for a in 0..3 {
            for b in 0..3 {
                c[a][b] += d[a] * d[b];
            }
        }
    }
    let (_, vecs) = v3::jacobi3(c);
    let mut cands: Vec<V3> = vec![[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    for v in vecs {
        let v = v3::norm(v);
        if cands[..3].iter().all(|c| v3::dot(*c, v).abs() < 0.9998) {
            cands.push(v);
        }
    }
    let mut best: Option<Symmetry> = None;
    for nrm in cands {
        let proj: Vec<f64> = pts.iter().map(|p| v3::dot(nrm, *p)).collect();
        let mid = (proj.iter().cloned().fold(f64::INFINITY, f64::min) + proj.iter().cloned().fold(f64::NEG_INFINITY, f64::max)) / 2.0;
        let (mut tot, mut worst, mut k) = (0.0, 0.0f64, 0);
        for (i, p) in pts.iter().enumerate().step_by(stride) {
            let off = 2.0 * (proj[i] - mid);
            let q = v3::sub(*p, v3::scale(nrm, off));
            let d = grid.nearest(pts, q);
            if d.is_finite() {
                tot += d;
                worst = worst.max(d);
                k += 1;
            }
        }
        if k == 0 {
            continue;
        }
        let mean = tot / k as f64;
        if best.as_ref().is_none_or(|b| mean < b.mean_error) {
            let ax = (0..3).max_by(|&a, &b| nrm[a].abs().partial_cmp(&nrm[b].abs()).unwrap_or(std::cmp::Ordering::Equal)).unwrap();
            best = Some(Symmetry { axis: ['x', 'y', 'z'][ax], normal: nrm, max_error: worst, mean_error: mean, sampled: stride > 1 });
        }
    }
    best
}

/// Normalised low spectrum of a graph's uniform Laplacian, or why it was skipped.
pub fn spectrum(n: usize, edges: &[(u32, u32)]) -> Result<Vec<f64>, String> {
    if n == 0 {
        return Err("no vertices".into());
    }
    if n > SPECTRUM_VERT_CAP {
        return Err(format!("{n} vertices exceed the {SPECTRUM_VERT_CAP}-vertex spectrum cap"));
    }
    let mut l = vec![vec![0.0f64; n]; n];
    for &(a, b) in edges {
        let (a, b) = (a as usize, b as usize);
        if a != b && l[a][b] == 0.0 {
            l[a][b] = -1.0;
            l[b][a] = -1.0;
        }
    }
    for i in 0..n {
        l[i][i] = -l[i].iter().sum::<f64>();
    }
    let mut vals = jacobi(l);
    vals.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let lmax = *vals.last().unwrap();
    if lmax.is_nan() || lmax <= 0.0 {
        return Err("no positive spectrum (no edges)".into());
    }
    let cut = 1e-8 * lmax;
    let nz: Vec<f64> = vals.into_iter().filter(|v| v.abs() >= cut).collect();
    let first = *nz.first().ok_or("no nonzero eigenvalues")?;
    Ok(nz.iter().take(SPECTRUM_K).map(|v| sig6(v / first)).collect())
}

fn sig6(v: f64) -> f64 {
    if v == 0.0 || !v.is_finite() {
        return v;
    }
    let e = v.abs().log10().floor() as i32;
    let f = 10f64.powi(5 - e);
    (v * f).round() / f
}

/// Eigenvalues of a dense symmetric matrix: Householder reduction to tridiagonal form, then the
/// implicit QL iteration (Numerical Recipes tred2/tqli, eigenvalues only). O(n^3) once.
fn jacobi(mut a: Vec<Vec<f64>>) -> Vec<f64> {
    let n = a.len();
    let mut d = vec![0.0; n];
    let mut e = vec![0.0; n];
    for i in (1..n).rev() {
        let l = i - 1;
        let mut h = 0.0;
        if l > 0 {
            let scale: f64 = (0..=l).map(|k| a[i][k].abs()).sum();
            if scale == 0.0 {
                e[i] = a[i][l];
            } else {
                for k in 0..=l {
                    a[i][k] /= scale;
                    h += a[i][k] * a[i][k];
                }
                let f = a[i][l];
                let g = if f >= 0.0 { -h.sqrt() } else { h.sqrt() };
                e[i] = scale * g;
                h -= f * g;
                a[i][l] = f - g;
                let mut f2 = 0.0;
                for j in 0..=l {
                    let mut g2 = 0.0;
                    for k in 0..=j {
                        g2 += a[j][k] * a[i][k];
                    }
                    for k in j + 1..=l {
                        g2 += a[k][j] * a[i][k];
                    }
                    e[j] = g2 / h;
                    f2 += e[j] * a[i][j];
                }
                let hh = f2 / (h + h);
                for j in 0..=l {
                    let f = a[i][j];
                    let g = e[j] - hh * f;
                    e[j] = g;
                    for k in 0..=j {
                        a[j][k] -= f * e[k] + g * a[i][k];
                    }
                }
            }
        } else {
            e[i] = a[i][l];
        }
        d[i] = h;
    }
    for i in 0..n {
        d[i] = a[i][i];
    }
    for i in 1..n {
        e[i - 1] = e[i];
    }
    if n > 0 {
        e[n - 1] = 0.0;
    }
    for l in 0..n {
        let mut iter = 0;
        loop {
            let mut m = l;
            while m + 1 < n {
                let dd = d[m].abs() + d[m + 1].abs();
                if e[m].abs() <= f64::EPSILON * dd {
                    break;
                }
                m += 1;
            }
            if m == l || iter > 60 {
                break;
            }
            iter += 1;
            let mut g = (d[l + 1] - d[l]) / (2.0 * e[l]);
            let mut r = g.hypot(1.0);
            g = d[m] - d[l] + e[l] / (g + if g >= 0.0 { r.abs() } else { -r.abs() });
            let (mut s, mut c, mut p) = (1.0, 1.0, 0.0);
            let mut i = m;
            let mut early = false;
            while i > l {
                i -= 1;
                let mut f = s * e[i];
                let b = c * e[i];
                r = f.hypot(g);
                e[i + 1] = r;
                if r == 0.0 {
                    d[i + 1] -= p;
                    e[m] = 0.0;
                    early = true;
                    break;
                }
                s = f / r;
                c = g / r;
                g = d[i + 1] - p;
                r = (d[i] - g) * s + 2.0 * c * b;
                p = s * r;
                d[i + 1] = g + p;
                g = c * r - b;
                f = 0.0;
                let _ = f;
            }
            if early {
                continue;
            }
            d[l] -= p;
            e[l] = g;
            e[m] = 0.0;
        }
    }
    d
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn symmetric_and_asymmetric() {
        let mut pts = Vec::new();
        for i in 0..40 {
            let x = i as f64 * 0.1;
            pts.push([x - 2.0, (x * 1.7).sin(), x * x * 0.1]);
            pts.push([2.0 - x, (x * 1.7).sin(), x * x * 0.1]);
        }
        let s = symmetry(&pts).unwrap();
        assert!(s.mean_error < 1e-9 && s.axis == 'x', "{s:?}");
        pts[0][1] += 0.5;
        let s2 = symmetry(&pts).unwrap();
        assert!(s2.max_error > 0.1);
    }

    #[test]
    fn cycle_and_path_spectra() {
        // C4: eigenvalues 0, 2, 2, 4 -> normalised [1, 1, 2]
        let c4 = spectrum(4, &[(0, 1), (1, 2), (2, 3), (3, 0)]).unwrap();
        assert_eq!(c4, vec![1.0, 1.0, 2.0]);
        // two disjoint edges: two zero modes
        let two = spectrum(4, &[(0, 1), (2, 3)]).unwrap();
        assert_eq!(two, vec![1.0, 1.0]);
        // a relabelled C4 has the same DNA
        assert_eq!(spectrum(4, &[(2, 0), (0, 3), (3, 1), (1, 2)]).unwrap(), c4);
        assert!(spectrum(SPECTRUM_VERT_CAP + 1, &[]).is_err());
    }
}

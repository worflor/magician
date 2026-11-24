//! Exact arithmetic on floating-point inputs: Shewchuk's expansion arithmetic.
//!
//! scene3d's kernel is exact over the rationals (BigInt `Rational`), because its meshes are minted
//! at rational coordinates. A compiled game mesh is minted at **float32** coordinates, and every
//! float is an exact dyadic rational, so the same guarantee needs no big integers: an *expansion*
//! (a sum of non-overlapping doubles, smallest first) represents sums and products of floats
//! exactly, and its sign is the sign of its largest component. So `orient2d` / `orient3d` here are
//! exact for any f64 inputs (a float filter answers the easy cases; the expansion decides the rest),
//! and a mesh's signed volume is summed exactly (`Exact`). The one rounding is `Exact::approx`,
//! taken only for reporting, as in scene3d's `toNumber`.
//!
//! Range: exact unless an intermediate overflows (|x| > ~1e300) or underflows below ~1e-300, which
//! model coordinates (|x| < 1e5, steps > 1e-30) never approach.

const SPLITTER: f64 = 134_217_729.0; // 2^27 + 1
const EPS: f64 = 1.1102230246251565e-16; // 2^-53
const CCW_ERRBOUND_A: f64 = (3.0 + 16.0 * EPS) * EPS;
const O3D_ERRBOUND_A: f64 = (7.0 + 56.0 * EPS) * EPS;

#[inline]
fn two_sum(a: f64, b: f64) -> (f64, f64) {
    let x = a + b;
    let bv = x - a;
    let av = x - bv;
    (x, (a - av) + (b - bv))
}

#[inline]
fn two_diff(a: f64, b: f64) -> (f64, f64) {
    let x = a - b;
    let bv = a - x;
    let av = x + bv;
    (x, (a - av) + (bv - b))
}

#[inline]
fn split(a: f64) -> (f64, f64) {
    let c = SPLITTER * a;
    let big = c - a;
    let hi = c - big;
    (hi, a - hi)
}

#[inline]
fn two_product(a: f64, b: f64) -> (f64, f64) {
    let x = a * b;
    let (ah, al) = split(a);
    let (bh, bl) = split(b);
    let err = x - ah * bh - al * bh - ah * bl;
    (x, al * bl - err)
}

/// An exact sum of non-overlapping doubles, increasing magnitude, zeros eliminated.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Exact(pub Vec<f64>);

impl Exact {
    pub fn zero() -> Exact {
        Exact(Vec::new())
    }
    pub fn from(x: f64) -> Exact {
        if x == 0.0 {
            Exact(Vec::new())
        } else {
            Exact(vec![x])
        }
    }
    /// a * b, exactly.
    pub fn product(a: f64, b: f64) -> Exact {
        let (x, y) = two_product(a, b);
        let mut v = Vec::with_capacity(2);
        if y != 0.0 {
            v.push(y);
        }
        if x != 0.0 {
            v.push(x);
        }
        Exact(v)
    }
    /// a - b, exactly.
    pub fn diff(a: f64, b: f64) -> Exact {
        let (x, y) = two_diff(a, b);
        let mut v = Vec::with_capacity(2);
        if y != 0.0 {
            v.push(y);
        }
        if x != 0.0 {
            v.push(x);
        }
        Exact(v)
    }
    /// Add one double (Shewchuk's GROW-EXPANSION with zero elimination).
    pub fn add_f64(&mut self, b: f64) {
        if b == 0.0 {
            return;
        }
        let mut q = b;
        let mut out = Vec::with_capacity(self.0.len() + 1);
        for &e in &self.0 {
            let (s, h) = two_sum(q, e);
            q = s;
            if h != 0.0 {
                out.push(h);
            }
        }
        if q != 0.0 {
            out.push(q);
        }
        self.0 = out;
    }
    pub fn add(&self, o: &Exact) -> Exact {
        let mut r = self.clone();
        for &x in &o.0 {
            r.add_f64(x);
        }
        r
    }
    pub fn add_assign(&mut self, o: &Exact) {
        for &x in &o.0 {
            self.add_f64(x);
        }
        // Keep accumulators short: re-normalise when components pile up.
        if self.0.len() > 64 {
            self.compress();
        }
    }
    pub fn neg(&self) -> Exact {
        Exact(self.0.iter().map(|x| -x).collect())
    }
    pub fn sub(&self, o: &Exact) -> Exact {
        self.add(&o.neg())
    }
    /// Multiply by one double (SCALE-EXPANSION with zero elimination).
    pub fn scale(&self, b: f64) -> Exact {
        if self.0.is_empty() || b == 0.0 {
            return Exact::zero();
        }
        let mut out = Vec::with_capacity(self.0.len() * 2);
        let (mut q, h) = two_product(self.0[0], b);
        if h != 0.0 {
            out.push(h);
        }
        for &e in &self.0[1..] {
            let (p1, p0) = two_product(e, b);
            let (s, h) = two_sum(q, p0);
            if h != 0.0 {
                out.push(h);
            }
            let (s2, h2) = two_sum(p1, s);
            // fast_two_sum is valid here (|p1| >= |s|)
            if h2 != 0.0 {
                out.push(h2);
            }
            q = s2;
        }
        if q != 0.0 {
            out.push(q);
        }
        let mut r = Exact(out);
        r.compress();
        r
    }
    pub fn mul(&self, o: &Exact) -> Exact {
        let mut acc = Exact::zero();
        for &x in &o.0 {
            acc.add_assign(&self.scale(x));
        }
        acc
    }
    /// Re-normalise into a shorter non-overlapping expansion (Shewchuk's COMPRESS).
    pub fn compress(&mut self) {
        let e = &self.0;
        if e.len() < 2 {
            return;
        }
        let m = e.len();
        let mut g = vec![0.0; m];
        let mut bottom = m - 1;
        let mut q = e[m - 1];
        for i in (0..m - 1).rev() {
            let (s, small) = {
                let x = q + e[i];
                let bv = x - q;
                (x, e[i] - bv)
            };
            q = s;
            if small != 0.0 {
                g[bottom] = q;
                bottom = bottom.wrapping_sub(1);
                q = small;
            }
        }
        g[bottom] = q;
        let mut out = Vec::with_capacity(m);
        let mut q = g[bottom];
        for i in bottom + 1..m {
            let x = g[i] + q;
            let bv = x - g[i];
            let small = q - bv;
            q = x;
            if small != 0.0 {
                out.push(small);
            }
        }
        if q != 0.0 {
            out.push(q);
        }
        self.0 = out;
    }
    /// -1, 0 or 1: the sign of the exact value.
    pub fn sign(&self) -> i8 {
        match self.0.iter().rev().find(|x| **x != 0.0) {
            Some(x) if *x > 0.0 => 1,
            Some(_) => -1,
            None => 0,
        }
    }
    /// The one rounding: the nearest double (components summed smallest first).
    pub fn approx(&self) -> f64 {
        self.0.iter().sum()
    }
    pub fn is_zero(&self) -> bool {
        self.sign() == 0
    }
}

/// Exact sign of the 2D orientation: +1 if a, b, c turn counter-clockwise, -1 clockwise, 0 collinear.
pub fn orient2d(a: [f64; 2], b: [f64; 2], c: [f64; 2]) -> i8 {
    let detleft = (a[0] - c[0]) * (b[1] - c[1]);
    let detright = (a[1] - c[1]) * (b[0] - c[0]);
    let det = detleft - detright;
    let detsum = if detleft > 0.0 {
        if detright <= 0.0 {
            return sgn(det);
        }
        detleft + detright
    } else if detleft < 0.0 {
        if detright >= 0.0 {
            return sgn(det);
        }
        -detleft - detright
    } else {
        return sgn(det);
    };
    if det.abs() >= CCW_ERRBOUND_A * detsum {
        return sgn(det);
    }
    let acx = Exact::diff(a[0], c[0]);
    let bcy = Exact::diff(b[1], c[1]);
    let acy = Exact::diff(a[1], c[1]);
    let bcx = Exact::diff(b[0], c[0]);
    acx.mul(&bcy).sub(&acy.mul(&bcx)).sign()
}

fn sgn(x: f64) -> i8 {
    if x > 0.0 {
        1
    } else if x < 0.0 {
        -1
    } else {
        0
    }
}

/// Exact sign of `(b - a) x (c - a) . (d - a)`: +1 when d lies on the side the counter-clockwise
/// triangle a, b, c faces (its normal side), -1 behind it, 0 coplanar.
pub fn orient3d(a: [f64; 3], b: [f64; 3], c: [f64; 3], d: [f64; 3]) -> i8 {
    let adx = a[0] - d[0];
    let bdx = b[0] - d[0];
    let cdx = c[0] - d[0];
    let ady = a[1] - d[1];
    let bdy = b[1] - d[1];
    let cdy = c[1] - d[1];
    let adz = a[2] - d[2];
    let bdz = b[2] - d[2];
    let cdz = c[2] - d[2];
    let bdxcdy = bdx * cdy;
    let cdxbdy = cdx * bdy;
    let cdxady = cdx * ady;
    let adxcdy = adx * cdy;
    let adxbdy = adx * bdy;
    let bdxady = bdx * ady;
    let det = adz * (bdxcdy - cdxbdy) + bdz * (cdxady - adxcdy) + cdz * (adxbdy - bdxady);
    let permanent = (bdxcdy.abs() + cdxbdy.abs()) * adz.abs() + (cdxady.abs() + adxcdy.abs()) * bdz.abs() + (adxbdy.abs() + bdxady.abs()) * cdz.abs();
    if det.abs() > O3D_ERRBOUND_A * permanent {
        return -sgn(det);
    }
    -orient3d_exact(a, b, c, d)
}

/// Shewchuk's determinant det(a - d, b - d, c - d), exactly.
fn orient3d_exact(a: [f64; 3], b: [f64; 3], c: [f64; 3], d: [f64; 3]) -> i8 {
    let adx = Exact::diff(a[0], d[0]);
    let bdx = Exact::diff(b[0], d[0]);
    let cdx = Exact::diff(c[0], d[0]);
    let ady = Exact::diff(a[1], d[1]);
    let bdy = Exact::diff(b[1], d[1]);
    let cdy = Exact::diff(c[1], d[1]);
    let adz = Exact::diff(a[2], d[2]);
    let bdz = Exact::diff(b[2], d[2]);
    let cdz = Exact::diff(c[2], d[2]);
    let t1 = bdx.mul(&cdy).sub(&cdx.mul(&bdy));
    let t2 = cdx.mul(&ady).sub(&adx.mul(&cdy));
    let t3 = adx.mul(&bdy).sub(&bdx.mul(&ady));
    adz.mul(&t1).add(&bdz.mul(&t2)).add(&cdz.mul(&t3)).sign()
}

/// Exact `det[a, b, c]` (= 6 x signed volume of the tetrahedron origin, a, b, c) as an expansion.
pub fn det3(a: [f64; 3], b: [f64; 3], c: [f64; 3]) -> Exact {
    let x = Exact::product(b[1], c[2]).sub(&Exact::product(b[2], c[1])).scale(a[0]);
    let y = Exact::product(b[2], c[0]).sub(&Exact::product(b[0], c[2])).scale(a[1]);
    let z = Exact::product(b[0], c[1]).sub(&Exact::product(b[1], c[0])).scale(a[2]);
    x.add(&y).add(&z)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expansion_is_exact_where_doubles_are_not() {
        // 1e16 + 1 - 1e16 is 0 in doubles, 1 exactly.
        let mut e = Exact::from(1e16);
        e.add_f64(1.0);
        e.add_f64(-1e16);
        assert_eq!(e.approx(), 1.0);
        // (1 + 2^-30)^2 - 1 - 2^-29 = 2^-60
        let x = 1.0 + 2f64.powi(-30);
        let mut p = Exact::product(x, x);
        p.add_f64(-1.0);
        p.add_f64(-(2f64.powi(-29)));
        assert_eq!(p.approx(), 2f64.powi(-60));
        assert_eq!(p.sign(), 1);
    }

    #[test]
    fn orient2d_decides_near_collinear_points_exactly() {
        // Points on the line y = x, perturbed by one ulp: naive arithmetic loses the sign.
        let a = [0.5, 0.5];
        let b = [12.0, 12.0];
        let c = [24.0, 24.0];
        assert_eq!(orient2d(a, b, c), 0);
        let c_up = [24.0, f64::from_bits(24.0f64.to_bits() + 1)];
        assert_eq!(orient2d(a, b, c_up), 1);
        let c_dn = [24.0, f64::from_bits(24.0f64.to_bits() - 1)];
        assert_eq!(orient2d(a, b, c_dn), -1);
        // Shewchuk's classic: many nearby points, the exact predicate must be antisymmetric.
        for i in 0..64 {
            let p = [0.5 + i as f64 * 2f64.powi(-53), 0.5];
            let s1 = orient2d(p, [12.0, 12.0], [24.0, 24.0]);
            let s2 = orient2d([12.0, 12.0], p, [24.0, 24.0]);
            assert_eq!(s1, -s2);
        }
    }

    #[test]
    fn orient3d_signs() {
        let a = [0.0, 0.0, 0.0];
        let b = [1.0, 0.0, 0.0];
        let c = [0.0, 1.0, 0.0];
        assert_eq!(orient3d(a, b, c, [0.0, 0.0, 1.0]), 1);
        assert_eq!(orient3d(a, b, c, [0.0, 0.0, -1.0]), -1);
        assert_eq!(orient3d(a, b, c, [0.3, 0.3, 0.0]), 0);
        // Degenerate in exact arithmetic at a large offset (c = 2b - a, all dyadic): zero, not noise.
        let a = [1000.125, 0.25, 0.375];
        let b = [1000.625, 0.875, 1.125];
        let c = [1001.125, 1.5, 1.875];
        assert_eq!(orient3d(a, b, c, [5.0, -3.0, 2.0]), 0);
        // One ulp off the plane z = x at 1e3 is still decided.
        let p = [0.0, 0.0, 0.0];
        let q = [1000.0, 0.0, 1000.0];
        let r = [0.0, 1.0, 0.0];
        let above = [500.0, 0.25, f64::from_bits(500.0f64.to_bits() - 1)];
        assert_ne!(orient3d(p, q, r, above), 0);
        assert_eq!(orient3d(p, q, r, [500.0, 0.25, 500.0]), 0);
    }

    #[test]
    fn det3_is_six_volumes() {
        let d = det3([1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]);
        assert_eq!(d.approx(), 1.0);
        let d = det3([1.0, 2.0, 3.0], [4.0, 5.0, 6.0], [7.0, 8.0, 9.0]);
        assert_eq!(d.sign(), 0, "{:?}", d);
    }
}

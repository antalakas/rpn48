//! The HP 48G's matrix commands beyond arithmetic: taking arrays apart by
//! rows and columns, row operations, the factorizations (LU, QR, LQ, SVD,
//! SCHUR), eigenvalues, norms, least squares, polynomials and the FFT.

// The algorithms read most plainly with the textbooks' row and column indices.
#![allow(clippy::needless_range_loop)]

use std::f64::consts::PI;
use std::ops::{Add, Div, Mul, Neg, Sub};

use crate::calc::{Calc, Error, OpFn, Res, index, one, op, real_of};
use crate::carray;
use crate::obj::{Obj, r12};

type M = Vec<Vec<f64>>;

/// A complex number for the eigenvalue and FFT arithmetic.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Cx(f64, f64);

impl Cx {
    fn abs(self) -> f64 {
        self.0.hypot(self.1)
    }
    fn conj(self) -> Cx {
        Cx(self.0, -self.1)
    }
    fn sqrt(self) -> Cx {
        let r = self.abs();
        let re = ((r + self.0) / 2.0).sqrt();
        let im = ((r - self.0) / 2.0).sqrt();
        Cx(re, if self.1 < 0.0 { -im } else { im })
    }
    fn scale(self, k: f64) -> Cx {
        Cx(self.0 * k, self.1 * k)
    }
    fn cis(t: f64) -> Cx {
        Cx(t.cos(), t.sin())
    }
}

impl Add for Cx {
    type Output = Cx;
    fn add(self, b: Cx) -> Cx {
        Cx(self.0 + b.0, self.1 + b.1)
    }
}
impl Sub for Cx {
    type Output = Cx;
    fn sub(self, b: Cx) -> Cx {
        Cx(self.0 - b.0, self.1 - b.1)
    }
}
impl Mul for Cx {
    type Output = Cx;
    fn mul(self, b: Cx) -> Cx {
        Cx(self.0 * b.0 - self.1 * b.1, self.0 * b.1 + self.1 * b.0)
    }
}
impl Div for Cx {
    type Output = Cx;
    fn div(self, b: Cx) -> Cx {
        let d = b.0 * b.0 + b.1 * b.1;
        Cx(
            (self.0 * b.0 + self.1 * b.1) / d,
            (self.1 * b.0 - self.0 * b.1) / d,
        )
    }
}
impl Neg for Cx {
    type Output = Cx;
    fn neg(self) -> Cx {
        Cx(-self.0, -self.1)
    }
}

type CM = Vec<Vec<Cx>>;

fn mat_of(o: &Obj) -> Res<&M> {
    match o {
        Obj::Mat(m) => Ok(m),
        _ => Err(Error::BadType),
    }
}

fn square(m: &M) -> Res<usize> {
    if m.len() == m[0].len() {
        Ok(m.len())
    } else {
        Err(Error::Dim)
    }
}

fn identity(n: usize) -> M {
    (0..n)
        .map(|i| (0..n).map(|j| if i == j { 1.0 } else { 0.0 }).collect())
        .collect()
}

fn transpose(m: &M) -> M {
    (0..m[0].len())
        .map(|j| m.iter().map(|r| r[j]).collect())
        .collect()
}

fn mul(a: &M, b: &M) -> Res<M> {
    if a[0].len() != b.len() {
        return Err(Error::Dim);
    }
    Ok(a.iter()
        .map(|r| {
            (0..b[0].len())
                .map(|j| r.iter().zip(b).map(|(x, br)| x * br[j]).sum())
                .collect()
        })
        .collect())
}

fn norm(m: &M) -> f64 {
    m.iter().flatten().map(|x| x * x).sum::<f64>().sqrt()
}

/// A computed element, checked and rounded like a real; anything smaller
/// than 1E-12 of `scale` is rounding noise and becomes 0.
fn fin(x: f64, scale: f64) -> Res<f64> {
    if x.is_nan() {
        Err(Error::UndefinedResult)
    } else if x.is_infinite() {
        Err(Error::Infinite)
    } else if x.abs() <= 1e-12 * scale {
        Ok(0.0)
    } else {
        Ok(r12(x))
    }
}

fn mat(m: M) -> Res<Obj> {
    let s = norm(&m).max(f64::MIN_POSITIVE);
    Ok(Obj::Mat(
        m.into_iter()
            .map(|r| r.into_iter().map(|x| fin(x, s)).collect::<Res<_>>())
            .collect::<Res<_>>()?,
    ))
}

fn vect(v: Vec<f64>) -> Res<Obj> {
    let s = v
        .iter()
        .map(|x| x * x)
        .sum::<f64>()
        .sqrt()
        .max(f64::MIN_POSITIVE);
    Ok(Obj::Vect(
        v.into_iter().map(|x| fin(x, s)).collect::<Res<_>>()?,
    ))
}

/// Complex results, real when every imaginary part is rounding noise.
fn cvect(v: Vec<Cx>) -> Res<Obj> {
    let s = v.iter().map(|z| z.abs()).fold(0.0, f64::max);
    if v.iter().all(|z| z.1.abs() <= 1e-10 * s) {
        return vect(v.into_iter().map(|z| z.0).collect());
    }
    let zs: Vec<_> = v
        .iter()
        .map(|z| (fin(z.0, s).unwrap_or(z.0), fin(z.1, s).unwrap_or(z.1)))
        .collect();
    carray::shaped(&[zs.len()], &zs)
}

fn cmat(m: CM) -> Res<Obj> {
    let s = m.iter().flatten().map(|z| z.abs()).fold(0.0, f64::max);
    if m.iter().flatten().all(|z| z.1.abs() <= 1e-10 * s) {
        return mat(m
            .into_iter()
            .map(|r| r.into_iter().map(|z| z.0).collect())
            .collect());
    }
    let ds = [m.len(), m[0].len()];
    let zs: Vec<_> = m
        .iter()
        .flatten()
        .map(|z| (fin(z.0, s).unwrap_or(z.0), fin(z.1, s).unwrap_or(z.1)))
        .collect();
    carray::shaped(&ds, &zs)
}

/// Always complex, as the FFT's results are.
fn complex_arr(ds: &[usize], v: &[Cx]) -> Res<Obj> {
    let s = v.iter().map(|z| z.abs()).fold(0.0, f64::max);
    let zs: Vec<_> = v
        .iter()
        .map(|z| Ok((fin(z.0, s)?, fin(z.1, s)?)))
        .collect::<Res<_>>()?;
    carray::shaped(ds, &zs)
}

/// The elements of a real or complex array, and its dimensions.
fn cx_elems(o: &Obj) -> Res<(Vec<Cx>, Vec<usize>)> {
    Ok(match o {
        Obj::Vect(v) => (v.iter().map(|&x| Cx(x, 0.0)).collect(), vec![v.len()]),
        Obj::Mat(m) => (
            m.iter().flatten().map(|&x| Cx(x, 0.0)).collect(),
            vec![m.len(), m[0].len()],
        ),
        Obj::CVect(v) => (v.iter().map(|&(a, b)| Cx(a, b)).collect(), vec![v.len()]),
        Obj::CMat(m) => (
            m.iter().flatten().map(|&(a, b)| Cx(a, b)).collect(),
            vec![m.len(), m[0].len()],
        ),
        _ => return Err(Error::BadType),
    })
}

/// Crout's LU with partial pivoting: P·A = L·U, U with a unit diagonal.
fn lu(a: &M) -> Res<(M, M, M)> {
    let n = square(a)?;
    let mut a = a.clone();
    let mut p = identity(n);
    for k in 0..n {
        // Column k of L.
        for i in k..n {
            a[i][k] -= (0..k).map(|j| a[i][j] * a[j][k]).sum::<f64>();
        }
        let piv = (k..n)
            .max_by(|&i, &j| a[i][k].abs().total_cmp(&a[j][k].abs()))
            .unwrap();
        a.swap(k, piv);
        p.swap(k, piv);
        // Row k of U.
        for j in k + 1..n {
            let s = a[k][j] - (0..k).map(|i| a[k][i] * a[i][j]).sum::<f64>();
            a[k][j] = if a[k][k] == 0.0 { 0.0 } else { s / a[k][k] };
        }
    }
    let l = (0..n)
        .map(|i| (0..n).map(|j| if j <= i { a[i][j] } else { 0.0 }).collect())
        .collect();
    let u = (0..n)
        .map(|i| {
            (0..n)
                .map(|j| match j.cmp(&i) {
                    std::cmp::Ordering::Less => 0.0,
                    std::cmp::Ordering::Equal => 1.0,
                    std::cmp::Ordering::Greater => a[i][j],
                })
                .collect()
        })
        .collect();
    Ok((l, u, p))
}

/// Householder QR with column pivoting: A·P = Q·R.
fn qr(a: &M) -> (M, M, M) {
    let (m, n) = (a.len(), a[0].len());
    let mut r = a.clone();
    let mut q = identity(m);
    let mut perm: Vec<usize> = (0..n).collect();
    for k in 0..m.min(n) {
        // Bring the remaining column of largest norm to k.
        let cn = |r: &M, j: usize| (k..m).map(|i| r[i][j] * r[i][j]).sum::<f64>();
        let best = (k..n)
            .max_by(|&i, &j| cn(&r, i).total_cmp(&cn(&r, j)))
            .unwrap();
        if best != k {
            for row in r.iter_mut() {
                row.swap(k, best);
            }
            perm.swap(k, best);
        }
        let alpha = cn(&r, k).sqrt();
        if alpha == 0.0 {
            continue;
        }
        let alpha = if r[k][k] > 0.0 { -alpha } else { alpha };
        let mut v: Vec<f64> = (0..m).map(|i| if i < k { 0.0 } else { r[i][k] }).collect();
        v[k] -= alpha;
        let vv: f64 = v.iter().map(|x| x * x).sum();
        if vv == 0.0 {
            continue;
        }
        // H = I - 2vvᵀ/vᵀv, applied to R from the left and Q from the right.
        for j in 0..n {
            let d = 2.0 * (k..m).map(|i| v[i] * r[i][j]).sum::<f64>() / vv;
            for i in k..m {
                r[i][j] -= d * v[i];
            }
        }
        for row in q.iter_mut() {
            let d = 2.0 * (k..m).map(|i| row[i] * v[i]).sum::<f64>() / vv;
            for i in k..m {
                row[i] -= d * v[i];
            }
        }
        for i in k + 1..m {
            r[i][k] = 0.0;
        }
    }
    let mut p = vec![vec![0.0; n]; n];
    for (j, &c) in perm.iter().enumerate() {
        p[c][j] = 1.0;
    }
    (q, r, p)
}

/// One-sided Jacobi on a matrix at least as tall as it is wide: B = U·diag(s)·Wᵀ,
/// U with orthonormal columns where s is not 0.
fn jacobi_svd(b: &M) -> (M, Vec<f64>, M) {
    let (m, n) = (b.len(), b[0].len());
    let mut u = b.clone();
    let mut w = identity(n);
    for _ in 0..60 {
        let mut off = 0.0f64;
        for p in 0..n {
            for q in p + 1..n {
                let (mut al, mut be, mut ga) = (0.0, 0.0, 0.0);
                for row in &u {
                    al += row[p] * row[p];
                    be += row[q] * row[q];
                    ga += row[p] * row[q];
                }
                if ga == 0.0 {
                    continue;
                }
                off = off.max(ga.abs() / (al * be).sqrt());
                let zeta = (be - al) / (2.0 * ga);
                let t = zeta.signum() / (zeta.abs() + (1.0 + zeta * zeta).sqrt());
                let t = if zeta == 0.0 { 1.0 } else { t };
                let c = 1.0 / (1.0 + t * t).sqrt();
                let s = c * t;
                for row in u.iter_mut().chain(w.iter_mut()) {
                    let (x, y) = (row[p], row[q]);
                    row[p] = c * x - s * y;
                    row[q] = s * x + c * y;
                }
            }
        }
        if off < 1e-15 {
            break;
        }
    }
    let mut s: Vec<f64> = (0..n)
        .map(|j| (0..m).map(|i| u[i][j] * u[i][j]).sum::<f64>().sqrt())
        .collect();
    // Largest first.
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&i, &j| s[j].total_cmp(&s[i]));
    let tiny = s.iter().copied().fold(0.0, f64::max) * 1e-14;
    let u = (0..m)
        .map(|i| {
            order
                .iter()
                .map(|&j| if s[j] > tiny { u[i][j] / s[j] } else { 0.0 })
                .collect()
        })
        .collect();
    let w = (0..n)
        .map(|i| order.iter().map(|&j| w[i][j]).collect())
        .collect();
    s = order
        .iter()
        .map(|&j| if s[j] > tiny { s[j] } else { 0.0 })
        .collect();
    (u, s, w)
}

/// Extends the columns of `u` that go with nonzero `s` to an orthonormal
/// basis of all `m` dimensions.
fn complete(u: &M, s: &[f64], m: usize) -> M {
    let mut cols: Vec<Vec<f64>> = (0..s.len())
        .filter(|&j| s[j] > 0.0)
        .map(|j| u.iter().map(|r| r[j]).collect())
        .collect();
    for e in 0..m {
        if cols.len() == m {
            break;
        }
        let mut v: Vec<f64> = (0..m).map(|i| if i == e { 1.0 } else { 0.0 }).collect();
        for _ in 0..2 {
            for c in &cols {
                let d: f64 = c.iter().zip(&v).map(|(a, b)| a * b).sum();
                for (x, y) in v.iter_mut().zip(c) {
                    *x -= d * y;
                }
            }
        }
        let l = v.iter().map(|x| x * x).sum::<f64>().sqrt();
        if l > 1e-8 {
            cols.push(v.into_iter().map(|x| x / l).collect());
        }
    }
    (0..m)
        .map(|i| cols.iter().map(|c| c[i]).collect())
        .collect()
}

/// A = U·diag(S)·V, with U m×m and V n×n orthogonal, S largest first.
fn svd(a: &M) -> (M, M, Vec<f64>) {
    let (m, n) = (a.len(), a[0].len());
    if m >= n {
        let (u, s, w) = jacobi_svd(a);
        (complete(&u, &s, m), transpose(&w), s)
    } else {
        // Aᵀ = U'·S·Wᵀ, so A = W·S·U'ᵀ.
        let (u, s, w) = jacobi_svd(&transpose(a));
        (w, transpose(&complete(&u, &s, n)), s)
    }
}

fn to_cm(a: &M) -> CM {
    a.iter()
        .map(|r| r.iter().map(|&x| Cx(x, 0.0)).collect())
        .collect()
}

/// Reduces `h` to upper Hessenberg form by Householder similarities,
/// accumulating them in `z`.
fn hessenberg(h: &mut CM, z: &mut CM) {
    let n = h.len();
    for k in 0..n.saturating_sub(2) {
        let alpha = (k + 1..n)
            .map(|i| h[i][k].abs().powi(2))
            .sum::<f64>()
            .sqrt();
        if alpha == 0.0 {
            continue;
        }
        let x0 = h[k + 1][k];
        let phase = if x0.abs() == 0.0 {
            Cx(1.0, 0.0)
        } else {
            x0.scale(1.0 / x0.abs())
        };
        let mut v = vec![Cx::default(); n];
        for i in k + 1..n {
            v[i] = h[i][k];
        }
        v[k + 1] = v[k + 1] + phase.scale(alpha);
        let vv: f64 = v.iter().map(|z| z.abs().powi(2)).sum();
        // H = I - 2vv*/v*v on both sides.
        for j in 0..n {
            let d = (k + 1..n)
                .fold(Cx::default(), |s, i| s + v[i].conj() * h[i][j])
                .scale(2.0 / vv);
            for i in k + 1..n {
                h[i][j] = h[i][j] - v[i] * d;
            }
        }
        for row in h.iter_mut().chain(z.iter_mut()) {
            let d = (k + 1..n)
                .fold(Cx::default(), |s, i| s + row[i] * v[i])
                .scale(2.0 / vv);
            for i in k + 1..n {
                row[i] = row[i] - d * v[i].conj();
            }
        }
    }
}

/// The Schur form: A = Z·T·Z*, T upper triangular. With `real_shifts` the
/// arithmetic stays real, which only converges when the eigenvalues are real.
fn schur(a: &M, real_shifts: bool) -> Option<(CM, CM)> {
    let n = a.len();
    let mut t = to_cm(a);
    let mut z = to_cm(&identity(n));
    hessenberg(&mut t, &mut z);
    let scale = t
        .iter()
        .flatten()
        .map(|z| z.abs())
        .fold(0.0, f64::max)
        .max(f64::MIN_POSITIVE);
    let mut hi = n - 1;
    let mut iters = 0;
    while hi > 0 {
        let mut lo = hi;
        while lo > 0 {
            let s = t[lo][lo].abs() + t[lo - 1][lo - 1].abs();
            if t[lo][lo - 1].abs() <= f64::EPSILON * s.max(scale * 1e-3) {
                t[lo][lo - 1] = Cx::default();
                break;
            }
            lo -= 1;
        }
        if lo == hi {
            hi -= 1;
            iters = 0;
            continue;
        }
        iters += 1;
        if iters > 60 {
            return None;
        }
        // Wilkinson's shift: the eigenvalue of the trailing 2×2 nearer its corner.
        let (p, q, r, d) = (t[hi - 1][hi - 1], t[hi - 1][hi], t[hi][hi - 1], t[hi][hi]);
        let half = (p - d).scale(0.5);
        let disc = half * half + q * r;
        let mu = if iters % 11 == 10 {
            d + Cx(t[hi][hi - 1].abs(), 0.0)
        } else if real_shifts {
            if disc.0 >= 0.0 {
                let root = disc.0.sqrt();
                let (a1, a2) = (d.0 + half.0 + root, d.0 + half.0 - root);
                Cx(
                    if (a1 - d.0).abs() < (a2 - d.0).abs() {
                        a1
                    } else {
                        a2
                    },
                    0.0,
                )
            } else {
                Cx(d.0, 0.0)
            }
        } else {
            let root = disc.sqrt();
            let (a1, a2) = (d + half + root, d + half - root);
            if (a1 - d).abs() < (a2 - d).abs() {
                a1
            } else {
                a2
            }
        };
        for i in 0..n {
            t[i][i] = t[i][i] - mu;
        }
        // QR of the active window by Givens rotations, then RQ.
        let mut rots = vec![];
        for k in lo..hi {
            let (x, y) = (t[k][k], t[k + 1][k]);
            let rr = (x.abs().powi(2) + y.abs().powi(2)).sqrt();
            if rr == 0.0 {
                rots.push((1.0, Cx::default()));
                continue;
            }
            let (c, s) = if x.abs() == 0.0 {
                (0.0, Cx(1.0, 0.0))
            } else {
                (
                    x.abs() / rr,
                    x.scale(1.0 / x.abs()) * y.conj().scale(1.0 / rr),
                )
            };
            for j in 0..n {
                let (u, v) = (t[k][j], t[k + 1][j]);
                t[k][j] = u.scale(c) + s * v;
                t[k + 1][j] = v.scale(c) - s.conj() * u;
            }
            rots.push((c, s));
        }
        for (k, &(c, s)) in (lo..hi).zip(&rots) {
            for row in t.iter_mut().chain(z.iter_mut()) {
                let (u, v) = (row[k], row[k + 1]);
                row[k] = u.scale(c) + v * s.conj();
                row[k + 1] = v.scale(c) - u * s;
            }
        }
        for i in 0..n {
            t[i][i] = t[i][i] + mu;
        }
    }
    for i in 1..n {
        for j in 0..i {
            t[i][j] = Cx::default();
        }
    }
    Some((z, t))
}

/// The Schur form, kept real when the eigenvalues are.
fn schur_any(a: &M) -> Res<(CM, CM)> {
    square(a)?;
    schur(a, true)
        .or_else(|| schur(a, false))
        .ok_or(Error::UndefinedResult)
}

/// Eigenvalues in ascending order and, for each, a unit eigenvector.
fn eigen(a: &M) -> Res<(Vec<Cx>, CM)> {
    let (z, t) = schur_any(a)?;
    let n = t.len();
    let scale = t
        .iter()
        .flatten()
        .map(|z| z.abs())
        .fold(0.0, f64::max)
        .max(f64::MIN_POSITIVE);
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&i, &j| {
        let (x, y) = (t[i][i], t[j][j]);
        x.0.total_cmp(&y.0).then(x.1.total_cmp(&y.1))
    });
    let mut vals = vec![];
    let mut vecs = vec![];
    for &k in &order {
        let l = t[k][k];
        // Back substitution in (T - λI)·y = 0 with y_k = 1.
        let mut y = vec![Cx::default(); n];
        y[k] = Cx(1.0, 0.0);
        for i in (0..k).rev() {
            let s = (i + 1..=k).fold(Cx::default(), |s, j| s + t[i][j] * y[j]);
            let mut d = t[i][i] - l;
            if d.abs() < 1e-14 * scale {
                d = Cx(1e-14 * scale, 0.0);
            }
            y[i] = -(s / d);
        }
        let mut x: Vec<Cx> = (0..n)
            .map(|i| (0..n).fold(Cx::default(), |s, j| s + z[i][j] * y[j]))
            .collect();
        // Scaled so that the largest element is 1.
        let big = x.iter().fold(Cx::default(), |b, &e| {
            if e.abs() > b.abs() * (1.0 + 1e-9) {
                e
            } else {
                b
            }
        });
        for e in x.iter_mut() {
            *e = *e / big;
        }
        vals.push(l);
        vecs.push(x);
    }
    let cols = (0..n)
        .map(|i| vecs.iter().map(|v| v[i]).collect())
        .collect();
    Ok((vals, cols))
}

/// Gauss-Jordan reduction to reduced row echelon form.
fn rref(a: &M) -> M {
    let mut a = a.clone();
    let (m, n) = (a.len(), a[0].len());
    let tol = norm(&a) * 1e-12;
    let mut r = 0;
    for c in 0..n {
        if r == m {
            break;
        }
        let p = (r..m)
            .max_by(|&i, &j| a[i][c].abs().total_cmp(&a[j][c].abs()))
            .unwrap();
        if a[p][c].abs() <= tol {
            for row in &mut a[r..] {
                row[c] = 0.0;
            }
            continue;
        }
        a.swap(r, p);
        let d = a[r][c];
        for x in a[r].iter_mut() {
            *x /= d;
        }
        for i in 0..m {
            if i != r {
                let f = a[i][c];
                let pivot = a[r].clone();
                for (x, p) in a[i].iter_mut().zip(pivot) {
                    *x -= f * p;
                }
            }
        }
        r += 1;
    }
    a
}

fn pinv(a: &M) -> M {
    let (u, v, s) = svd(a);
    let (m, n) = (a.len(), a[0].len());
    let tol = s.first().copied().unwrap_or(0.0) * 1e-12 * m.max(n) as f64;
    // A⁺ = Vᵀ·S⁺·Uᵀ.
    (0..n)
        .map(|i| {
            (0..m)
                .map(|j| {
                    (0..s.len())
                        .filter(|&k| s[k] > tol)
                        .map(|k| v[k][i] * u[j][k] / s[k])
                        .sum()
                })
                .collect()
        })
        .collect()
}

fn fft(x: &mut [Cx], inverse: bool) {
    let n = x.len();
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            x.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let w = Cx::cis(if inverse { 2.0 } else { -2.0 } * PI / len as f64);
        for start in (0..n).step_by(len) {
            let mut wk = Cx(1.0, 0.0);
            for k in 0..len / 2 {
                let (a, b) = (x[start + k], x[start + k + len / 2] * wk);
                x[start + k] = a + b;
                x[start + k + len / 2] = a - b;
                wk = wk * w;
            }
        }
        len <<= 1;
    }
    if inverse {
        for z in x.iter_mut() {
            *z = z.scale(1.0 / n as f64);
        }
    }
}

fn fft_obj(o: &Obj, inverse: bool) -> Res<Obj> {
    let (mut e, ds) = cx_elems(o)?;
    if ds.iter().any(|d| !d.is_power_of_two()) {
        return Err(Error::Dim);
    }
    match ds[..] {
        [_] => fft(&mut e, inverse),
        [r, c] => {
            for row in e.chunks_mut(c) {
                fft(row, inverse);
            }
            for j in 0..c {
                let mut col: Vec<Cx> = (0..r).map(|i| e[i * c + j]).collect();
                fft(&mut col, inverse);
                for i in 0..r {
                    e[i * c + j] = col[i];
                }
            }
        }
        _ => return Err(Error::Dim),
    }
    complex_arr(&ds, &e)
}

fn coeffs(o: &Obj) -> Res<Vec<f64>> {
    match o {
        Obj::Vect(v) => Ok(v.clone()),
        _ => Err(Error::BadType),
    }
}

/// The roots of the polynomial with coefficients `c`, highest power first,
/// as the eigenvalues of its companion matrix.
fn proot(c: &[f64]) -> Res<Vec<Cx>> {
    let c: Vec<f64> = c.iter().copied().skip_while(|&x| x == 0.0).collect();
    if c.len() < 2 {
        return Err(Error::BadValue);
    }
    let n = c.len() - 1;
    let mut m = vec![vec![0.0; n]; n];
    for j in 0..n {
        m[0][j] = -c[j + 1] / c[0];
    }
    for i in 1..n {
        m[i][i - 1] = 1.0;
    }
    let (_, t) = schur_any(&m)?;
    let mut roots: Vec<Cx> = (0..n).map(|i| t[i][i]).collect();
    roots.sort_by(|x, y| x.0.total_cmp(&y.0).then(x.1.total_cmp(&y.1)));
    Ok(roots)
}

pub fn pure(w: &str) -> Option<(usize, OpFn)> {
    use Obj::*;
    match w {
        "→DIAG" => op(1, |_, a| {
            let m = mat_of(&a[0])?;
            one(vect(
                (0..m.len().min(m[0].len())).map(|i| m[i][i]).collect(),
            ))
        }),
        "DIAG→" => op(2, |_, a| {
            let Vect(v) = &a[0] else {
                return Err(Error::BadType);
            };
            let (r, c) = match &a[1] {
                Real(n) => (*n, *n),
                List(l) if l.len() == 1 => (real_of(&l[0])?, real_of(&l[0])?),
                List(l) if l.len() == 2 => (real_of(&l[0])?, real_of(&l[1])?),
                _ => return Err(Error::BadType),
            };
            if r < 1.0 || c < 1.0 || r.fract() != 0.0 || c.fract() != 0.0 {
                return Err(Error::BadValue);
            }
            let (r, c) = (r as usize, c as usize);
            one(mat((0..r)
                .map(|i| {
                    (0..c)
                        .map(|j| {
                            if i == j {
                                v.get(i).copied().unwrap_or(0.0)
                            } else {
                                0.0
                            }
                        })
                        .collect()
                })
                .collect()))
        }),
        "RCI" => op(3, |_, a| {
            let (x, n) = (real_of(&a[1])?, real_of(&a[2])?);
            match &a[0] {
                Vect(v) => {
                    let mut v = v.clone();
                    let i = index(n, v.len())?;
                    v[i] *= x;
                    one(vect(v))
                }
                Mat(m) => {
                    let mut m = m.clone();
                    let i = index(n, m.len())?;
                    for e in m[i].iter_mut() {
                        *e *= x;
                    }
                    one(mat(m))
                }
                _ => Err(Error::BadType),
            }
        }),
        "RCIJ" => op(4, |_, a| {
            let (x, i, j) = (real_of(&a[1])?, real_of(&a[2])?, real_of(&a[3])?);
            match &a[0] {
                Vect(v) => {
                    let mut v = v.clone();
                    let (i, j) = (index(i, v.len())?, index(j, v.len())?);
                    v[j] += x * v[i];
                    one(vect(v))
                }
                Mat(m) => {
                    let mut m = m.clone();
                    let (i, j) = (index(i, m.len())?, index(j, m.len())?);
                    let ri = m[i].clone();
                    for (e, s) in m[j].iter_mut().zip(ri) {
                        *e += x * s;
                    }
                    one(mat(m))
                }
                _ => Err(Error::BadType),
            }
        }),
        // B A Z RSD is the residual B - A·Z.
        "RSD" => op(3, |_, a| {
            let am = mat_of(&a[1])?;
            match (&a[0], &a[2]) {
                (Vect(b), Vect(z)) => {
                    let az = mul(am, &z.iter().map(|&x| vec![x]).collect())?;
                    if az.len() != b.len() {
                        return Err(Error::Dim);
                    }
                    one(vect(b.iter().zip(az).map(|(x, r)| x - r[0]).collect()))
                }
                (Mat(b), Mat(z)) => {
                    let az = mul(am, z)?;
                    if az.len() != b.len() || az[0].len() != b[0].len() {
                        return Err(Error::Dim);
                    }
                    one(mat(b
                        .iter()
                        .zip(az)
                        .map(|(x, r)| x.iter().zip(r).map(|(p, q)| p - q).collect())
                        .collect()))
                }
                _ => Err(Error::BadType),
            }
        }),
        "LU" => op(1, |_, a| {
            let (l, u, p) = lu(mat_of(&a[0])?)?;
            Ok(vec![mat(l)?, mat(u)?, mat(p)?])
        }),
        "QR" => op(1, |_, a| {
            let (q, r, p) = qr(mat_of(&a[0])?);
            Ok(vec![mat(q)?, mat(r)?, mat(p)?])
        }),
        // P·A = L·Q, from the QR of the transpose.
        "LQ" => op(1, |_, a| {
            let (q, r, p) = qr(&transpose(mat_of(&a[0])?));
            Ok(vec![
                mat(transpose(&r))?,
                mat(transpose(&q))?,
                mat(transpose(&p))?,
            ])
        }),
        "SVD" => op(1, |_, a| {
            let (u, v, s) = svd(mat_of(&a[0])?);
            Ok(vec![mat(u)?, mat(v)?, vect(s)?])
        }),
        "SVL" => op(1, |_, a| one(vect(svd(mat_of(&a[0])?).2))),
        "RANK" => op(1, |_, a| {
            let m = mat_of(&a[0])?;
            let s = svd(m).2;
            let tol = s.first().copied().unwrap_or(0.0) * 1e-11 * m.len().max(m[0].len()) as f64;
            Ok(vec![Real(s.iter().filter(|&&x| x > tol).count() as f64)])
        }),
        "RREF" => op(1, |_, a| one(mat(rref(mat_of(&a[0])?)))),
        // Spectral norm: the largest singular value.
        "SNRM" => op(1, |_, a| match &a[0] {
            Vect(v) => one(crate::calc::real(
                v.iter().map(|x| x * x).sum::<f64>().sqrt(),
            )),
            Mat(m) => one(crate::calc::real(svd(m).2[0])),
            _ => Err(Error::BadType),
        }),
        // Spectral radius: the largest eigenvalue's magnitude.
        "SRAD" => op(1, |_, a| {
            let (_, t) = schur_any(mat_of(&a[0])?)?;
            one(crate::calc::real(
                (0..t.len()).map(|i| t[i][i].abs()).fold(0.0, f64::max),
            ))
        }),
        // Condition number in the column norm, CNRM(A)·CNRM(A⁻¹).
        "COND" => op(1, |_, a| {
            let m = mat_of(&a[0])?;
            let n = square(m)?;
            let (l, u, p) = lu(m)?;
            if (0..n).any(|i| l[i][i] == 0.0) {
                return Err(Error::Infinite);
            }
            let inv = solve_lu(&l, &u, &p, &identity(n));
            let cnrm = |m: &M| {
                (0..m[0].len())
                    .map(|j| m.iter().map(|r| r[j].abs()).sum::<f64>())
                    .fold(0.0, f64::max)
            };
            one(crate::calc::real(cnrm(m) * cnrm(&inv)))
        }),
        "SCHUR" => op(1, |_, a| {
            let (z, t) = schur_any(mat_of(&a[0])?)?;
            Ok(vec![cmat(z)?, cmat(t)?])
        }),
        "EGVL" => op(1, |_, a| one(cvect(eigen(mat_of(&a[0])?)?.0))),
        "EGV" => op(1, |_, a| {
            let (vals, vecs) = eigen(mat_of(&a[0])?)?;
            Ok(vec![cmat(vecs)?, cvect(vals)?])
        }),
        // B A LSQ: the least-squares solution of A·X = B of smallest norm.
        "LSQ" => op(2, |_, a| {
            let p = pinv(mat_of(&a[1])?);
            match &a[0] {
                Vect(b) => {
                    let x = mul(&p, &b.iter().map(|&x| vec![x]).collect())?;
                    one(vect(x.into_iter().map(|r| r[0]).collect()))
                }
                Mat(b) => one(mat(mul(&p, b)?)),
                _ => Err(Error::BadType),
            }
        }),
        "FFT" => op(1, |_, a| one(fft_obj(&a[0], false))),
        "IFFT" => op(1, |_, a| one(fft_obj(&a[0], true))),
        "PROOT" => op(1, |_, a| one(cvect(proot(&coeffs(&a[0])?)?))),
        "PCOEF" => op(1, |_, a| {
            let roots: Vec<Cx> = match &a[0] {
                Vect(v) => v.iter().map(|&x| Cx(x, 0.0)).collect(),
                CVect(v) => v.iter().map(|&(p, q)| Cx(p, q)).collect(),
                _ => return Err(Error::BadType),
            };
            let mut c = vec![Cx(1.0, 0.0)];
            for r in roots {
                let mut next = c.clone();
                next.push(Cx::default());
                for (i, &x) in c.iter().enumerate() {
                    next[i + 1] = next[i + 1] - x * r;
                }
                c = next;
            }
            one(cvect(c))
        }),
        "PEVAL" => op(2, |_, a| {
            let x = real_of(&a[1])?;
            one(crate::calc::real(
                coeffs(&a[0])?.iter().fold(0.0, |s, c| s * x + c),
            ))
        }),
        _ => None,
    }
}

fn solve_lu(l: &M, u: &M, p: &M, b: &M) -> M {
    let n = l.len();
    let pb = mul(p, b).unwrap();
    let mut out = vec![vec![0.0; b[0].len()]; n];
    for c in 0..b[0].len() {
        let mut y = vec![0.0; n];
        for i in 0..n {
            y[i] = (pb[i][c] - (0..i).map(|j| l[i][j] * y[j]).sum::<f64>()) / l[i][i];
        }
        for i in (0..n).rev() {
            out[i][c] = y[i] - (i + 1..n).map(|j| u[i][j] * out[j][c]).sum::<f64>();
        }
    }
    out
}

impl Calc {
    /// The next index after `i` in an object of dimensions `ds`, and whether
    /// it wrapped round to the first.
    fn next_index(i: &Obj, ds: &[usize]) -> Res<(Obj, bool)> {
        let real = |n: f64| Obj::Real(n);
        Ok(match (i, ds) {
            (Obj::Real(n), _) => {
                let total: usize = ds.iter().product();
                if *n as usize >= total {
                    (real(1.0), true)
                } else {
                    (real(n + 1.0), false)
                }
            }
            (Obj::List(l), [len]) if l.len() == 1 => {
                let n = real_of(&l[0])?;
                let (k, wrap) = if n as usize >= *len {
                    (1.0, true)
                } else {
                    (n + 1.0, false)
                };
                (Obj::List(vec![real(k)]), wrap)
            }
            (Obj::List(l), [r, c]) if l.len() == 2 => {
                let (mut i, mut j) = (real_of(&l[0])?, real_of(&l[1])?);
                j += 1.0;
                if j as usize > *c {
                    (i, j) = (i + 1.0, 1.0);
                }
                let wrap = i as usize > *r;
                if wrap {
                    i = 1.0;
                }
                (Obj::List(vec![real(i), real(j)]), wrap)
            }
            _ => return Err(Error::BadValue),
        })
    }

    fn dims_of(o: &Obj) -> Res<Vec<usize>> {
        Ok(match o {
            Obj::List(l) => vec![l.len()],
            Obj::Vect(v) => vec![v.len()],
            Obj::CVect(v) => vec![v.len()],
            Obj::Mat(m) => vec![m.len(), m[0].len()],
            Obj::CMat(m) => vec![m.len(), m[0].len()],
            _ => return Err(Error::BadType),
        })
    }

    /// Sets or clears flag -64, the index-wrap flag of GETI and PUTI.
    fn set_wrap(&mut self, wrap: bool) {
        let bit = 1 << 63;
        if wrap {
            self.flags[1] |= bit;
        } else {
            self.flags[1] &= !bit;
        }
    }

    /// Takes a matrix apart into its columns (or rows), or a vector into its
    /// elements, with the count.
    fn explode(&mut self, rows: bool) -> Res<()> {
        let o = self.pop()?;
        let parts: Vec<Obj> = match &o {
            Obj::Vect(v) => v.iter().map(|&x| Obj::Real(x)).collect(),
            Obj::Mat(m) if rows => m.iter().map(|r| Obj::Vect(r.clone())).collect(),
            Obj::Mat(m) => transpose(m).into_iter().map(Obj::Vect).collect(),
            _ => {
                self.stack.push(o);
                return Err(Error::BadType);
            }
        };
        let n = parts.len();
        self.stack.extend(parts);
        self.stack.push(Obj::Real(n as f64));
        Ok(())
    }

    /// Builds a matrix from n vectors as its columns (or rows), or a vector
    /// from n reals.
    fn implode(&mut self, rows: bool) -> Res<()> {
        let n = self.pop_count(0)?;
        let items = self.stack[self.stack.len() - n..].to_vec();
        let built = if n == 0 {
            Err(Error::BadValue)
        } else if items.iter().all(|o| matches!(o, Obj::Real(_))) {
            vect(items.iter().map(|o| real_of(o).unwrap()).collect())
        } else {
            items
                .iter()
                .map(|o| match o {
                    Obj::Vect(v) => Ok(v.clone()),
                    _ => Err(Error::BadType),
                })
                .collect::<Res<M>>()
                .and_then(|m| {
                    if m.iter().any(|r| r.len() != m[0].len()) {
                        Err(Error::Dim)
                    } else {
                        mat(if rows { m } else { transpose(&m) })
                    }
                })
        };
        match built {
            Ok(o) => {
                self.stack.truncate(self.stack.len() - n);
                self.stack.push(o);
                Ok(())
            }
            Err(e) => {
                self.stack.push(Obj::Real(n as f64));
                Err(e)
            }
        }
    }

    /// COL- and ROW-: [[A]] n → [[A']] v, or [v] n → [v'] x.
    fn take_out(&mut self, rows: bool) -> Res<()> {
        self.need(2)?;
        let n = real_of(self.peek(1))?;
        let (rest, part) = match self.peek(2) {
            Obj::Vect(v) if v.len() > 1 => {
                let mut v = v.clone();
                let x = v.remove(index(n, v.len())?);
                (Obj::Vect(v), Obj::Real(x))
            }
            Obj::Mat(m) => {
                let mut m = if rows { m.clone() } else { transpose(m) };
                if m.len() < 2 {
                    return Err(Error::Dim);
                }
                let v = m.remove(index(n, m.len())?);
                (Obj::Mat(if rows { m } else { transpose(&m) }), Obj::Vect(v))
            }
            Obj::Vect(_) => return Err(Error::Dim),
            _ => return Err(Error::BadType),
        };
        self.stack.truncate(self.stack.len() - 2);
        self.stack.push(rest);
        self.stack.push(part);
        Ok(())
    }

    /// COL+ and ROW+: [[A]] v n → [[A']], or [v] x n → [v'].
    fn put_in(&mut self, rows: bool) -> Res<()> {
        self.need(3)?;
        let n = real_of(self.peek(1))?;
        let out = match (self.peek(3), self.peek(2)) {
            (Obj::Vect(v), Obj::Real(x)) => {
                let mut v = v.clone();
                v.insert(index(n, v.len() + 1)?, *x);
                Obj::Vect(v)
            }
            (Obj::Mat(m), Obj::Vect(r)) => {
                let mut m = if rows { m.clone() } else { transpose(m) };
                if r.len() != m[0].len() {
                    return Err(Error::Dim);
                }
                m.insert(index(n, m.len() + 1)?, r.clone());
                Obj::Mat(if rows { m } else { transpose(&m) })
            }
            (Obj::Vect(_) | Obj::Mat(_), _) => return Err(Error::BadType),
            _ => return Err(Error::BadType),
        };
        self.stack.truncate(self.stack.len() - 3);
        self.stack.push(out);
        Ok(())
    }

    /// CSWP and RSWP: columns or rows of a matrix, elements of a vector.
    fn swap_lines(&mut self, rows: bool) -> Res<()> {
        self.need(3)?;
        let (i, j) = (real_of(self.peek(2))?, real_of(self.peek(1))?);
        let out = match self.peek(3) {
            Obj::Vect(v) => {
                let mut v = v.clone();
                let (i, j) = (index(i, v.len())?, index(j, v.len())?);
                v.swap(i, j);
                Obj::Vect(v)
            }
            Obj::Mat(m) => {
                let mut m = if rows { m.clone() } else { transpose(m) };
                let (i, j) = (index(i, m.len())?, index(j, m.len())?);
                m.swap(i, j);
                Obj::Mat(if rows { m } else { transpose(&m) })
            }
            _ => return Err(Error::BadType),
        };
        self.stack.truncate(self.stack.len() - 3);
        self.stack.push(out);
        Ok(())
    }

    pub(crate) fn matrix_cmd(&mut self, w: &str) -> Option<Res<()>> {
        Some(match w {
            "→COL" => self.explode(false),
            "→ROW" => self.explode(true),
            "COL→" => self.implode(false),
            "ROW→" => self.implode(true),
            "COL-" => self.take_out(false),
            "ROW-" => self.take_out(true),
            "COL+" => self.put_in(false),
            "ROW+" => self.put_in(true),
            "CSWP" => self.swap_lines(false),
            "RSWP" => self.swap_lines(true),
            // obj i GETI → obj next-i element, flag -64 set on wrapping round.
            "GETI" => self.need(2).and_then(|_| {
                let (o, i) = (self.peek(2).clone(), self.peek(1).clone());
                let ds = Self::dims_of(&o)?;
                let (_, get) = crate::calc::pure_any("GET").unwrap();
                let e = self.compute("GET", get, &[o.clone(), i.clone()])?;
                let (next, wrap) = Self::next_index(&i, &ds)?;
                self.set_wrap(wrap);
                self.stack.truncate(self.stack.len() - 2);
                self.stack.push(o);
                self.stack.push(next);
                self.stack.extend(e);
                Ok(())
            }),
            // obj i x PUTI → obj' next-i.
            "PUTI" => self.need(3).and_then(|_| {
                let (o, i, x) = (
                    self.peek(3).clone(),
                    self.peek(2).clone(),
                    self.peek(1).clone(),
                );
                let ds = Self::dims_of(&o)?;
                let (_, put) = crate::calc::pure_any("PUT").unwrap();
                let r = self.compute("PUT", put, &[o, i.clone(), x])?;
                let (next, wrap) = Self::next_index(&i, &ds)?;
                self.set_wrap(wrap);
                self.stack.truncate(self.stack.len() - 3);
                self.stack.extend(r);
                self.stack.push(next);
                Ok(())
            }),
            // { rows cols } RANM: a matrix of random integers from -9 to 9.
            "RANM" => self.need(1).and_then(|_| {
                let ds = match self.peek(1) {
                    Obj::Mat(m) => vec![m.len(), m[0].len()],
                    Obj::Vect(v) => vec![v.len()],
                    Obj::Real(n) if *n >= 1.0 && n.fract() == 0.0 => vec![*n as usize],
                    Obj::List(l) if (1..=2).contains(&l.len()) => l
                        .iter()
                        .map(|o| match o {
                            Obj::Real(n) if *n >= 1.0 && n.fract() == 0.0 => Ok(*n as usize),
                            _ => Err(Error::BadValue),
                        })
                        .collect::<Res<_>>()?,
                    _ => return Err(Error::BadType),
                };
                let total: usize = ds.iter().product();
                let xs: Vec<f64> = (0..total)
                    .map(|_| (self.random() * 19.0).floor() - 9.0)
                    .collect();
                self.stack.pop();
                self.stack.push(match ds[..] {
                    [_] => Obj::Vect(xs),
                    [_, c] => Obj::Mat(xs.chunks(c).map(<[f64]>::to_vec).collect()),
                    _ => unreachable!(),
                });
                Ok(())
            }),
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::calc::Calc;

    fn run(src: &str) -> Vec<String> {
        let mut c = Calc::new();
        c.exec_line(src).unwrap();
        c.stack.iter().map(|o| o.show(c.fmt)).collect()
    }

    #[test]
    fn rows_and_columns() {
        assert_eq!(run("[[1 2][3 4]] →COL"), ["[ 1 3 ]", "[ 2 4 ]", "2"]);
        assert_eq!(run("[1 3] [2 4] 2 COL→"), ["[[ 1 2 ] [ 3 4 ]]"]);
        assert_eq!(run("[1 2] [3 4] 2 ROW→ →ROW"), ["[ 1 2 ]", "[ 3 4 ]", "2"]);
        assert_eq!(run("1 2 3 3 ROW→"), ["[ 1 2 3 ]"]);
        assert_eq!(
            run("[[1 2 3][4 5 6]] 2 COL-"),
            ["[[ 1 3 ] [ 4 6 ]]", "[ 2 5 ]"]
        );
        assert_eq!(
            run("[[1 2][3 4]] [9 9] 2 ROW+"),
            ["[[ 1 2 ] [ 9 9 ] [ 3 4 ]]"]
        );
        assert_eq!(run("[[1 2][3 4]] [9 9] 1 COL+"), ["[[ 9 1 2 ] [ 9 3 4 ]]"]);
        assert_eq!(run("[1 2 3] 1 ROW-"), ["[ 2 3 ]", "1"]);
        assert_eq!(run("[[1 2][3 4]] 1 2 CSWP"), ["[[ 2 1 ] [ 4 3 ]]"]);
        assert_eq!(run("[[1 2][3 4]] 1 2 RSWP"), ["[[ 3 4 ] [ 1 2 ]]"]);
        assert_eq!(
            run("[[1 2][3 4]] →DIAG [1 2] { 2 3 } DIAG→"),
            ["[ 1 4 ]", "[[ 1 0 0 ] [ 0 2 0 ]]"]
        );
        assert_eq!(run("[[1 2][3 4]] 10 2 RCI"), ["[[ 1 2 ] [ 30 40 ]]"]);
        assert_eq!(run("[[1 2][3 4]] -3 1 2 RCIJ"), ["[[ 1 2 ] [ 0 -2 ]]"]);
    }

    #[test]
    fn indexing() {
        assert_eq!(run("{ 5 6 } 2 GETI"), ["{ 5 6 }", "1", "6"]);
        assert_eq!(run("{ 5 6 } 2 GETI DROP2 -64 FS?"), ["{ 5 6 }", "1"]);
        assert_eq!(
            run("[[1 2][3 4]] { 1 2 } GETI"),
            ["[[ 1 2 ] [ 3 4 ]]", "{ 2 1 }", "2"]
        );
        assert_eq!(run("[1 2 3] 1 9 PUTI"), ["[ 9 2 3 ]", "2"]);
    }

    #[test]
    fn factorizations() {
        assert_eq!(run("[[1 2][2 4]] RANK [[1 2][3 4]] RANK"), ["1", "2"]);
        assert_eq!(run("[[1 2 3][2 4 7]] RREF"), ["[[ 1 2 0 ] [ 0 0 1 ]]"]);
        assert_eq!(
            run("[[4 3][6 3]] LU"),
            [
                "[[ 6 0 ] [ 4 1 ]]",
                "[[ 1 .5 ] [ 0 1 ]]",
                "[[ 0 1 ] [ 1 0 ]]"
            ]
        );
        // The factors multiply back to the matrix.
        assert_eq!(run("[[1 2][3 4][5 6]] DUP QR TRN * * - ABS 1E-9 <"), ["1"]);
        assert_eq!(run("[[3 0][0 4]] SVL [[3 0][0 4]] SNRM"), ["[ 4 3 ]", "4"]);
        assert_eq!(
            run("[[1 2][3 4][5 6]] DUP SVD { 3 2 } DIAG→ SWAP * * - ABS 1E-9 <"),
            ["1"]
        );
        assert_eq!(run("[[2 0][0 1]] COND"), ["2"]);
        assert_eq!(
            run("[1 2 2] [[1 1][1 2][1 3]] LSQ"),
            ["[ .666666666667 .5 ]"]
        );
        assert_eq!(run("[5 6] [[1 2][3 4]] [-4 4.5] RSD"), ["[ 0 0 ]"]);
    }

    #[test]
    fn eigenvalues() {
        assert_eq!(run("[[2 1][1 2]] EGVL"), ["[ 1 3 ]"]);
        assert_eq!(run("[[2 1][1 2]] EGV"), ["[[ 1 1 ] [ -1 1 ]]", "[ 1 3 ]"]);
        assert_eq!(run("[[0 -1][1 0]] EGVL"), ["[ (0,-1) (0,1) ]"]);
        assert_eq!(run("[[4 1][2 3]] SRAD"), ["5"]);
        assert_eq!(
            run("[[4 1][2 3]] DUP SCHUR OVER TRN * * - ABS 1E-9 <"),
            ["1"]
        );
        assert_eq!(
            run(
                "[[1 2 3][4 5 6][7 8 10]] DUP EGV { 3 3 } DIAG→ OVER SWAP * ROT ROT * - ABS 1E-9 <"
            ),
            ["1"]
        );
    }

    #[test]
    fn polynomials_and_fft() {
        assert_eq!(run("[1 -5 6] PROOT"), ["[ 2 3 ]"]);
        assert_eq!(run("[1 0 1] PROOT"), ["[ (0,-1) (0,1) ]"]);
        assert_eq!(run("[2 3] PCOEF"), ["[ 1 -5 6 ]"]);
        assert_eq!(run("[1 -5 6] 4 PEVAL"), ["2"]);
        assert_eq!(run("[1 1 1 1] FFT"), ["[ (4,0) (0,0) (0,0) (0,0) ]"]);
        assert_eq!(run("[1 2 3 4] FFT IFFT RE"), ["[ 1 2 3 4 ]"]);
    }
}

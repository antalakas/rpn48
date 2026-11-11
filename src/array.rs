//! Real vectors and matrices.

use crate::calc::{Calc, Error, OpFn, Res, index, one, op, real_of};
use crate::obj::{Obj, r12};

type M = Vec<Vec<f64>>;

/// A computed element, checked and rounded like a real.
fn fin(x: f64) -> Res<f64> {
    if x.is_nan() {
        Err(Error::UndefinedResult)
    } else if x.is_infinite() {
        Err(Error::Infinite)
    } else {
        Ok(if x == 0.0 { 0.0 } else { r12(x) })
    }
}

fn vect(v: Vec<f64>) -> Res<Obj> {
    Ok(Obj::Vect(v.into_iter().map(fin).collect::<Res<_>>()?))
}

fn mat(m: M) -> Res<Obj> {
    Ok(Obj::Mat(
        m.into_iter()
            .map(|r| r.into_iter().map(fin).collect::<Res<_>>())
            .collect::<Res<_>>()?,
    ))
}

fn square(m: &M) -> Res<usize> {
    if m.len() == m[0].len() {
        Ok(m.len())
    } else {
        Err(Error::Dim)
    }
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

/// Gaussian elimination with partial pivoting: makes the square `a` upper
/// triangular, doing the same row operations on `b`, and returns the
/// determinant.
fn eliminate(a: &mut M, b: &mut M) -> f64 {
    let n = a.len();
    let mut d = 1.0;
    for k in 0..n {
        let p = (k..n)
            .max_by(|&i, &j| a[i][k].abs().total_cmp(&a[j][k].abs()))
            .unwrap();
        if a[p][k] == 0.0 {
            return 0.0;
        }
        if p != k {
            a.swap(k, p);
            b.swap(k, p);
            d = -d;
        }
        d *= a[k][k];
        let (top, rest) = a.split_at_mut(k + 1);
        let (btop, brest) = b.split_at_mut(k + 1);
        for (row, brow) in rest.iter_mut().zip(brest) {
            let f = row[k] / top[k][k];
            for (x, p) in row[k..].iter_mut().zip(&top[k][k..]) {
                *x -= f * p;
            }
            for (x, p) in brow.iter_mut().zip(&btop[k]) {
                *x -= f * p;
            }
        }
    }
    d
}

/// Solves a·x = b for each column of b. A singular a is an infinite result,
/// as on the calculator.
fn solve(a: &M, b: &M) -> Res<M> {
    let n = square(a)?;
    if b.len() != n {
        return Err(Error::Dim);
    }
    let (mut a, mut b) = (a.clone(), b.clone());
    if eliminate(&mut a, &mut b) == 0.0 {
        return Err(Error::Infinite);
    }
    for k in (0..n).rev() {
        for j in 0..b[0].len() {
            let s: f64 = (k + 1..n).map(|i| a[k][i] * b[i][j]).sum();
            b[k][j] = (b[k][j] - s) / a[k][k];
        }
    }
    Ok(b)
}

fn identity(n: usize) -> M {
    (0..n)
        .map(|i| (0..n).map(|j| if i == j { 1.0 } else { 0.0 }).collect())
        .collect()
}

fn det(a: &M) -> Res<f64> {
    let n = square(a)?;
    Ok(eliminate(&mut a.clone(), &mut vec![vec![]; n]))
}

fn col(v: &[f64]) -> M {
    v.iter().map(|&x| vec![x]).collect()
}

fn uncol(m: M) -> Vec<f64> {
    m.into_iter().map(|r| r[0]).collect()
}

fn elementwise(a: &Obj, b: &Obj, f: fn(f64, f64) -> f64) -> Res<Obj> {
    match (a, b) {
        (Obj::Vect(x), Obj::Vect(y)) if x.len() == y.len() => {
            vect(x.iter().zip(y).map(|(p, q)| f(*p, *q)).collect())
        }
        (Obj::Mat(x), Obj::Mat(y)) if x.len() == y.len() && x[0].len() == y[0].len() => mat(x
            .iter()
            .zip(y)
            .map(|(r, s)| r.iter().zip(s).map(|(p, q)| f(*p, *q)).collect())
            .collect()),
        (Obj::Vect(_) | Obj::Mat(_), Obj::Vect(_) | Obj::Mat(_)) => Err(Error::Dim),
        _ => Err(Error::BadType),
    }
}

fn scale(a: &Obj, k: f64) -> Res<Obj> {
    match a {
        Obj::Vect(v) => vect(v.iter().map(|x| x * k).collect()),
        Obj::Mat(m) => mat(m
            .iter()
            .map(|r| r.iter().map(|x| x * k).collect())
            .collect()),
        _ => Err(Error::BadType),
    }
}

/// + - * / where at least one side is an array.
pub fn arith(o: char, a: &Obj, b: &Obj) -> Res<Obj> {
    use Obj::*;
    match (o, a, b) {
        ('+', ..) => elementwise(a, b, |x, y| x + y),
        ('-', ..) => elementwise(a, b, |x, y| x - y),
        ('*', Real(k), arr) | ('*', arr, Real(k)) => scale(arr, *k),
        ('*', Mat(x), Mat(y)) => mat(mul(x, y)?),
        ('*', Mat(x), Vect(y)) => vect(uncol(mul(x, &col(y))?)),
        ('/', arr, Real(k)) => {
            if *k == 0.0 {
                Err(Error::Infinite)
            } else {
                scale(arr, 1.0 / k)
            }
        }
        // B A / solves A·X = B.
        ('/', Vect(b), Mat(a)) => vect(uncol(solve(a, &col(b))?)),
        ('/', Mat(b), Mat(a)) => mat(solve(a, b)?),
        ('*' | '/', Vect(_) | Mat(_), Vect(_) | Mat(_)) => Err(Error::Dim),
        _ => Err(Error::BadType),
    }
}

/// One-argument math that means something else for arrays.
pub fn over(w: &str, a: &[Obj]) -> Option<Res<Vec<Obj>>> {
    let arr = match a.first() {
        Some(o @ (Obj::Vect(_) | Obj::Mat(_))) if a.len() == 1 => o,
        _ => return None,
    };
    Some(one(match (w, arr) {
        ("NEG", _) => scale(arr, -1.0),
        ("ABS", _) => fin(elems(arr).iter().map(|x| x * x).sum::<f64>().sqrt()).map(Obj::Real),
        ("INV", Obj::Mat(m)) => square(m).and_then(|n| solve(m, &identity(n))).and_then(mat),
        ("SQ", Obj::Mat(m)) => mul(m, m).and_then(mat),
        ("INV" | "SQ", Obj::Vect(_)) => Err(Error::BadType),
        _ => return None,
    }))
}

fn mat_of(o: &Obj) -> Res<&M> {
    match o {
        Obj::Mat(m) => Ok(m),
        _ => Err(Error::BadType),
    }
}

fn vec_of(o: &Obj) -> Res<&Vec<f64>> {
    match o {
        Obj::Vect(v) => Ok(v),
        _ => Err(Error::BadType),
    }
}

/// Array dimensions given as n, { n } or { rows cols }.
fn dims(o: &Obj) -> Res<Vec<usize>> {
    let ds = match o {
        Obj::Real(n) => vec![*n],
        Obj::List(l) if (1..=2).contains(&l.len()) => {
            l.iter().map(real_of).collect::<Res<Vec<_>>>()?
        }
        _ => return Err(Error::BadType),
    };
    if ds.iter().any(|&d| d < 1.0 || d.fract() != 0.0) {
        return Err(Error::BadValue);
    }
    Ok(ds.into_iter().map(|d| d as usize).collect())
}

fn dims_of(o: &Obj) -> Vec<usize> {
    match o {
        Obj::Vect(v) => vec![v.len()],
        Obj::Mat(m) => vec![m.len(), m[0].len()],
        _ => vec![],
    }
}

fn shaped(ds: &[usize], mut elems: impl Iterator<Item = f64>) -> Res<Obj> {
    match ds {
        [n] => vect((0..*n).map(|_| elems.next().unwrap_or(0.0)).collect()),
        [r, c] => mat((0..*r)
            .map(|_| (0..*c).map(|_| elems.next().unwrap_or(0.0)).collect())
            .collect()),
        _ => Err(Error::Dim),
    }
}

fn elems(o: &Obj) -> Vec<f64> {
    match o {
        Obj::Vect(v) => v.clone(),
        Obj::Mat(m) => m.concat(),
        _ => vec![],
    }
}

/// The flat position of an element given as n, { n } or { row col }.
fn position(a: &Obj, i: &Obj) -> Res<usize> {
    let ds = dims_of(a);
    let total = ds.iter().product();
    match (i, &ds[..]) {
        (Obj::Real(n), _) => index(*n, total),
        (Obj::List(l), [n]) if l.len() == 1 => index(real_of(&l[0])?, *n),
        (Obj::List(l), [r, c]) if l.len() == 2 => {
            Ok(index(real_of(&l[0])?, *r)? * c + index(real_of(&l[1])?, *c)?)
        }
        (Obj::List(_), _) => Err(Error::BadValue),
        _ => Err(Error::BadType),
    }
}

pub fn get(a: &Obj, i: &Obj) -> Res<Obj> {
    Ok(Obj::Real(elems(a)[position(a, i)?]))
}

pub fn put(a: &Obj, i: &Obj, v: &Obj) -> Res<Obj> {
    let mut e = elems(a);
    e[position(a, i)?] = real_of(v)?;
    shaped(&dims_of(a), e.into_iter())
}

pub fn pure(w: &str) -> Option<(usize, OpFn)> {
    use Obj::*;
    match w {
        "DET" => op(1, |_, a| Ok(vec![Real(fin(det(mat_of(&a[0])?)?)?)])),
        "TRN" => op(1, |_, a| match &a[0] {
            Mat(m) => one(mat((0..m[0].len())
                .map(|j| m.iter().map(|r| r[j]).collect())
                .collect())),
            Vect(v) => one(mat(vec![v.clone()])),
            _ => Err(Error::BadType),
        }),
        "DOT" => op(2, |_, a| {
            let (x, y) = (vec_of(&a[0])?, vec_of(&a[1])?);
            if x.len() != y.len() {
                return Err(Error::Dim);
            }
            Ok(vec![Real(fin(x.iter().zip(y).map(|(p, q)| p * q).sum())?)])
        }),
        "CROSS" => op(2, |_, a| {
            let v3 = |o: &Obj| -> Res<[f64; 3]> {
                match &vec_of(o)?[..] {
                    [x, y] => Ok([*x, *y, 0.0]),
                    [x, y, z] => Ok([*x, *y, *z]),
                    _ => Err(Error::Dim),
                }
            };
            let (p, q) = (v3(&a[0])?, v3(&a[1])?);
            one(vect(vec![
                p[1] * q[2] - p[2] * q[1],
                p[2] * q[0] - p[0] * q[2],
                p[0] * q[1] - p[1] * q[0],
            ]))
        }),
        "IDN" => op(1, |_, a| {
            let n = match &a[0] {
                Mat(m) => square(m)?,
                o => match &dims(o)?[..] {
                    [n] => *n,
                    _ => return Err(Error::BadValue),
                },
            };
            one(mat(identity(n)))
        }),
        "CON" => op(2, |_, a| {
            let x = real_of(&a[1])?;
            let ds = match &a[0] {
                arr @ (Vect(_) | Mat(_)) => dims_of(arr),
                o => dims(o)?,
            };
            one(shaped(&ds, std::iter::repeat(x)))
        }),
        "RDM" => op(2, |_, a| {
            if !matches!(a[0], Vect(_) | Mat(_)) {
                return Err(Error::BadType);
            }
            one(shaped(&dims(&a[1])?, elems(&a[0]).into_iter()))
        }),
        // Row norm: the largest sum of absolute values across a row.
        "RNRM" => op(1, |_, a| {
            let r = match &a[0] {
                Vect(v) => v.iter().map(|x| x.abs()).fold(0.0, f64::max),
                Mat(m) => m
                    .iter()
                    .map(|r| r.iter().map(|x| x.abs()).sum())
                    .fold(0.0, f64::max),
                _ => return Err(Error::BadType),
            };
            Ok(vec![Real(fin(r)?)])
        }),
        "CNRM" => op(1, |_, a| {
            let r = match &a[0] {
                Vect(v) => v.iter().map(|x| x.abs()).sum(),
                Mat(m) => (0..m[0].len())
                    .map(|j| m.iter().map(|r| r[j].abs()).sum())
                    .fold(0.0, f64::max),
                _ => return Err(Error::BadType),
            };
            Ok(vec![Real(fin(r)?)])
        }),
        "TRACE" => op(1, |_, a| {
            let m = mat_of(&a[0])?;
            Ok(vec![Real(fin((0..square(m)?).map(|i| m[i][i]).sum())?)])
        }),
        "ARRY→" => op(1, |_, a| {
            if !matches!(a[0], Vect(_) | Mat(_)) {
                return Err(Error::BadType);
            }
            let mut out: Vec<Obj> = elems(&a[0]).into_iter().map(Real).collect();
            out.push(List(
                dims_of(&a[0]).into_iter().map(|d| Real(d as f64)).collect(),
            ));
            Ok(out)
        }),
        "V→" => op(1, |_, a| {
            Ok(vec_of(&a[0])?.iter().map(|x| Real(*x)).collect())
        }),
        "→V2" => op(2, |_, a| one(vect(vec![real_of(&a[0])?, real_of(&a[1])?]))),
        "→V3" => op(3, |_, a| {
            one(vect(vec![
                real_of(&a[0])?,
                real_of(&a[1])?,
                real_of(&a[2])?,
            ]))
        }),
        _ => None,
    }
}

impl Calc {
    pub(crate) fn array_cmd(&mut self, w: &str) -> Option<Res<()>> {
        Some(match w {
            // x1 … xn n →ARRY, or with { rows cols } for a matrix.
            "→ARRY" => self.need(1).and_then(|_| {
                let ds = dims(self.peek(1))?;
                let n: usize = ds.iter().product();
                self.need(n + 1)?;
                let items = &self.stack[self.stack.len() - 1 - n..self.stack.len() - 1];
                let xs = items.iter().map(real_of).collect::<Res<Vec<_>>>()?;
                let arr = shaped(&ds, xs.into_iter())?;
                self.stack.truncate(self.stack.len() - 1 - n);
                self.stack.push(arr);
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
    fn matrices() {
        assert_eq!(run("[[1 2][3 4]] DET"), ["-2"]);
        assert_eq!(run("[[1 2][3 4]] INV"), ["[[ -2 1 ] [ 1.5 -.5 ]]"]);
        assert_eq!(
            run("[[1 2][3 4]] [[5 6][7 8]] *"),
            ["[[ 19 22 ] [ 43 50 ]]"]
        );
        assert_eq!(run("[[1 2][3 4]] [5 6] *"), ["[ 17 39 ]"]);
        assert_eq!(run("[5 6] [[1 2][3 4]] /"), ["[ -4 4.5 ]"]);
        assert_eq!(run("[[1 2][3 4]] TRN"), ["[[ 1 3 ] [ 2 4 ]]"]);
        assert_eq!(run("[1 2 3] [4 5 6] DOT"), ["32"]);
        assert_eq!(run("[1 0 0] [0 1 0] CROSS"), ["[ 0 0 1 ]"]);
        assert_eq!(run("[3 4] ABS  2 IDN"), ["5", "[[ 1 0 ] [ 0 1 ]]"]);
        assert_eq!(run("1 2 3 4 { 2 2 } →ARRY { 1 2 } GET"), ["2"]);
        assert_eq!(run("[1 2 3] 2 *  [1 1 1] +"), ["[ 3 5 7 ]"]);
        assert_eq!(run("[1 2 3 4] { 2 2 } RDM"), ["[[ 1 2 ] [ 3 4 ]]"]);
    }
}

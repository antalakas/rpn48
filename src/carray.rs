//! Complex vectors and matrices. Real arrays and numbers meeting them are
//! promoted, and the results stay complex, as on the calculator.

use crate::calc::{Error, Res, index, real_of};
use crate::obj::{Obj, r12};

/// A complex element, re and im.
pub type Z = (f64, f64);

fn add(a: Z, b: Z) -> Z {
    (a.0 + b.0, a.1 + b.1)
}

fn sub(a: Z, b: Z) -> Z {
    (a.0 - b.0, a.1 - b.1)
}

fn mul(a: Z, b: Z) -> Z {
    (a.0 * b.0 - a.1 * b.1, a.0 * b.1 + a.1 * b.0)
}

fn div(a: Z, b: Z) -> Res<Z> {
    let d = b.0 * b.0 + b.1 * b.1;
    if d == 0.0 {
        return Err(Error::Infinite);
    }
    Ok(((a.0 * b.0 + a.1 * b.1) / d, (a.1 * b.0 - a.0 * b.1) / d))
}

fn abs(a: Z) -> f64 {
    a.0.hypot(a.1)
}

fn fin(z: Z) -> Res<Z> {
    let f = |x: f64| {
        if x.is_nan() {
            Err(Error::UndefinedResult)
        } else if x.is_infinite() {
            Err(Error::Infinite)
        } else {
            Ok(if x == 0.0 { 0.0 } else { r12(x) })
        }
    };
    Ok((f(z.0)?, f(z.1)?))
}

/// An array of either kind, as complex elements.
#[derive(Clone)]
enum Arr {
    V(Vec<Z>),
    M(Vec<Vec<Z>>),
}

use Arr::{M, V};

fn arr(o: &Obj) -> Option<Arr> {
    let z = |x: &f64| (*x, 0.0);
    Some(match o {
        Obj::Vect(v) => V(v.iter().map(z).collect()),
        Obj::Mat(m) => M(m.iter().map(|r| r.iter().map(z).collect()).collect()),
        Obj::CVect(v) => V(v.clone()),
        Obj::CMat(m) => M(m.clone()),
        _ => return None,
    })
}

fn scalar(o: &Obj) -> Option<Z> {
    match o {
        Obj::Real(x) => Some((*x, 0.0)),
        Obj::Complex(re, im) => Some((*re, *im)),
        _ => None,
    }
}

fn obj(a: Arr) -> Res<Obj> {
    Ok(match a {
        V(v) => Obj::CVect(v.into_iter().map(fin).collect::<Res<_>>()?),
        M(m) => Obj::CMat(
            m.into_iter()
                .map(|r| r.into_iter().map(fin).collect::<Res<_>>())
                .collect::<Res<_>>()?,
        ),
    })
}

/// A real array of one part of each element.
fn part(a: &Arr, f: fn(Z) -> f64) -> Obj {
    match a {
        V(v) => Obj::Vect(v.iter().map(|&z| f(z)).collect()),
        M(m) => Obj::Mat(
            m.iter()
                .map(|r| r.iter().map(|&z| f(z)).collect())
                .collect(),
        ),
    }
}

fn map(a: &Arr, f: impl Fn(Z) -> Z) -> Arr {
    match a {
        V(v) => V(v.iter().map(|&z| f(z)).collect()),
        M(m) => M(m
            .iter()
            .map(|r| r.iter().map(|&z| f(z)).collect())
            .collect()),
    }
}

fn zip(a: &Arr, b: &Arr, f: fn(Z, Z) -> Z) -> Res<Arr> {
    match (a, b) {
        (V(x), V(y)) if x.len() == y.len() => {
            Ok(V(x.iter().zip(y).map(|(p, q)| f(*p, *q)).collect()))
        }
        (M(x), M(y)) if x.len() == y.len() && x[0].len() == y[0].len() => Ok(M(x
            .iter()
            .zip(y)
            .map(|(r, s)| r.iter().zip(s).map(|(p, q)| f(*p, *q)).collect())
            .collect())),
        _ => Err(Error::Dim),
    }
}

fn elems(a: &Arr) -> Vec<Z> {
    match a {
        V(v) => v.clone(),
        M(m) => m.concat(),
    }
}

fn dims(a: &Arr) -> Vec<usize> {
    match a {
        V(v) => vec![v.len()],
        M(m) => vec![m.len(), m[0].len()],
    }
}

/// An array of the dimensions `ds` from its elements in order.
pub fn shaped(ds: &[usize], elems: &[Z]) -> Res<Obj> {
    let mut it = elems.iter().copied();
    let mut next = || it.next().unwrap_or((0.0, 0.0));
    obj(match ds {
        [n] => V((0..*n).map(|_| next()).collect()),
        [r, c] => M((0..*r).map(|_| (0..*c).map(|_| next()).collect()).collect()),
        _ => return Err(Error::Dim),
    })
}

fn position(a: &Arr, i: &Obj) -> Res<usize> {
    let ds = dims(a);
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

fn matmul(a: &[Vec<Z>], b: &[Vec<Z>]) -> Res<Vec<Vec<Z>>> {
    if a[0].len() != b.len() {
        return Err(Error::Dim);
    }
    Ok(a.iter()
        .map(|r| {
            (0..b[0].len())
                .map(|j| {
                    r.iter()
                        .zip(b)
                        .fold((0.0, 0.0), |s, (x, br)| add(s, mul(*x, br[j])))
                })
                .collect()
        })
        .collect())
}

/// Gaussian elimination with partial pivoting, as for real arrays: `a`
/// becomes upper triangular, `b` gets the same row operations, and the
/// determinant is returned.
fn eliminate(a: &mut [Vec<Z>], b: &mut [Vec<Z>]) -> Res<Z> {
    let n = a.len();
    let mut d = (1.0, 0.0);
    for k in 0..n {
        let p = (k..n)
            .max_by(|&i, &j| abs(a[i][k]).total_cmp(&abs(a[j][k])))
            .unwrap();
        if abs(a[p][k]) == 0.0 {
            return Ok((0.0, 0.0));
        }
        if p != k {
            a.swap(k, p);
            b.swap(k, p);
            d = (-d.0, -d.1);
        }
        d = mul(d, a[k][k]);
        let (top, rest) = a.split_at_mut(k + 1);
        let (btop, brest) = b.split_at_mut(k + 1);
        for (row, brow) in rest.iter_mut().zip(brest) {
            let f = div(row[k], top[k][k])?;
            for (x, p) in row[k..].iter_mut().zip(&top[k][k..]) {
                *x = sub(*x, mul(f, *p));
            }
            for (x, p) in brow.iter_mut().zip(&btop[k]) {
                *x = sub(*x, mul(f, *p));
            }
        }
    }
    Ok(d)
}

fn square(m: &[Vec<Z>]) -> Res<usize> {
    if m.len() == m[0].len() {
        Ok(m.len())
    } else {
        Err(Error::Dim)
    }
}

fn solve(a: &[Vec<Z>], b: &[Vec<Z>]) -> Res<Vec<Vec<Z>>> {
    let n = square(a)?;
    if b.len() != n {
        return Err(Error::Dim);
    }
    let (mut a, mut b) = (a.to_vec(), b.to_vec());
    if eliminate(&mut a, &mut b)? == (0.0, 0.0) {
        return Err(Error::Infinite);
    }
    for k in (0..n).rev() {
        for j in 0..b[0].len() {
            let s = (k + 1..n).fold((0.0, 0.0), |s, i| add(s, mul(a[k][i], b[i][j])));
            b[k][j] = div(sub(b[k][j], s), a[k][k])?;
        }
    }
    Ok(b)
}

fn col(v: &[Z]) -> Vec<Vec<Z>> {
    v.iter().map(|&z| vec![z]).collect()
}

fn uncol(m: Vec<Vec<Z>>) -> Vec<Z> {
    m.into_iter().map(|r| r[0]).collect()
}

fn real(x: f64) -> Res<Obj> {
    Ok(Obj::Real(fin((x, 0.0))?.0))
}

fn complex(z: Z) -> Res<Obj> {
    let z = fin(z)?;
    Ok(Obj::Complex(z.0, z.1))
}

/// What the array commands give when complex arrays are involved: a complex
/// array, a real array with a complex number, or the parts of real arrays
/// (RE, IM, CONJ, R→C). None leaves the command to the real arrays.
pub fn over(w: &str, args: &[Obj]) -> Option<Res<Vec<Obj>>> {
    let is = |o: &Obj| matches!(o, Obj::CVect(_) | Obj::CMat(_));
    let real_arr = |o: &Obj| matches!(o, Obj::Vect(_) | Obj::Mat(_));
    let any_complex = args.iter().any(is)
        || (args.iter().any(real_arr) && args.iter().any(|a| matches!(a, Obj::Complex(..))));
    let parts = matches!((w, args), ("RE" | "IM" | "CONJ", [a]) if real_arr(a))
        || matches!((w, args), ("R→C", [a, b]) if real_arr(a) && real_arr(b));
    if !any_complex && !parts {
        return None;
    }
    let one = |o: Res<Obj>| Some(o.map(|o| vec![o]));
    match (w, args) {
        ("R→C", [a, b]) => {
            let (Some(x), Some(y)) = (arr(a), arr(b)) else {
                return Some(Err(Error::BadType));
            };
            one(zip(&x, &y, |p, q| (p.0, q.0)).and_then(obj))
        }
        ("+" | "-", [a, b]) => {
            let (Some(x), Some(y)) = (arr(a), arr(b)) else {
                return Some(Err(Error::BadType));
            };
            one(zip(&x, &y, if w == "+" { add } else { sub }).and_then(obj))
        }
        ("*", [a, b]) => one(match (arr(a), arr(b), scalar(a), scalar(b)) {
            (Some(x), None, _, Some(k)) | (None, Some(x), Some(k), _) => {
                obj(map(&x, |z| mul(z, k)))
            }
            (Some(M(x)), Some(M(y)), ..) => matmul(&x, &y).map(M).and_then(obj),
            (Some(M(x)), Some(V(y)), ..) => matmul(&x, &col(&y)).map(|m| V(uncol(m))).and_then(obj),
            (Some(_), Some(_), ..) => Err(Error::Dim),
            _ => Err(Error::BadType),
        }),
        ("/", [a, b]) => one(match (arr(a), arr(b), scalar(b)) {
            (Some(_), None, Some((0.0, 0.0))) => Err(Error::Infinite),
            // Not zero, so each division succeeds.
            (Some(x), None, Some(k)) => obj(map(&x, |z| div(z, k).unwrap_or_default())),
            // B A / solves A·X = B.
            (Some(V(b)), Some(M(a)), _) => solve(&a, &col(&b)).map(|m| V(uncol(m))).and_then(obj),
            (Some(M(b)), Some(M(a)), _) => solve(&a, &b).map(M).and_then(obj),
            (Some(_), Some(_), _) => Err(Error::Dim),
            _ => Err(Error::BadType),
        }),
        (_, [a]) => {
            let x = arr(a)?;
            match w {
                "NEG" => one(obj(map(&x, |z| (-z.0, -z.1)))),
                "CONJ" if !any_complex => one(Ok(a.clone())),
                "CONJ" => one(obj(map(&x, |z| (z.0, -z.1)))),
                "RE" => one(Ok(part(&x, |z| z.0))),
                "IM" => one(Ok(part(&x, |z| z.1))),
                "C→R" => Some(Ok(vec![part(&x, |z| z.0), part(&x, |z| z.1)])),
                "ABS" => one(real(
                    elems(&x)
                        .iter()
                        .map(|z| z.0 * z.0 + z.1 * z.1)
                        .sum::<f64>()
                        .sqrt(),
                )),
                "RNRM" | "CNRM" => {
                    let rows = match (&x, w) {
                        (V(v), "RNRM") => v.iter().map(|z| vec![*z]).collect(),
                        (V(v), _) => vec![v.clone()],
                        (M(m), "RNRM") => m.clone(),
                        (M(m), _) => (0..m[0].len())
                            .map(|j| m.iter().map(|r| r[j]).collect())
                            .collect(),
                    };
                    let n = rows
                        .iter()
                        .map(|r: &Vec<Z>| r.iter().map(|z| abs(*z)).sum::<f64>())
                        .fold(0.0, f64::max);
                    one(real(n))
                }
                "INV" | "SQ" | "DET" | "TRACE" => {
                    let M(m) = x else {
                        return Some(Err(Error::BadType));
                    };
                    one(match w {
                        "INV" => square(&m)
                            .and_then(|n| {
                                let id = (0..n)
                                    .map(|i| (0..n).map(|j| ((i == j) as u8 as f64, 0.0)).collect())
                                    .collect::<Vec<_>>();
                                solve(&m, &id)
                            })
                            .map(M)
                            .and_then(obj),
                        "SQ" => matmul(&m, &m).map(M).and_then(obj),
                        "DET" => square(&m)
                            .and_then(|n| eliminate(&mut m.clone(), &mut vec![vec![]; n]))
                            .and_then(complex),
                        _ => square(&m)
                            .and_then(|n| complex((0..n).fold((0.0, 0.0), |s, i| add(s, m[i][i])))),
                    })
                }
                // The conjugate transpose, as TRN does for complex matrices.
                "TRN" => one(obj(M(match x {
                    V(v) => vec![v.iter().map(|z| (z.0, -z.1)).collect()],
                    M(m) => (0..m[0].len())
                        .map(|j| m.iter().map(|r| (r[j].0, -r[j].1)).collect())
                        .collect(),
                }))),
                "SIZE" => one(Ok(Obj::List(
                    dims(&x).into_iter().map(|d| Obj::Real(d as f64)).collect(),
                ))),
                "ARRY→" => {
                    let mut out: Vec<Obj> = elems(&x)
                        .into_iter()
                        .map(|z| Obj::Complex(z.0, z.1))
                        .collect();
                    out.push(Obj::List(
                        dims(&x).into_iter().map(|d| Obj::Real(d as f64)).collect(),
                    ));
                    Some(Ok(out))
                }
                _ => None,
            }
        }
        ("DOT", [a, b]) => {
            let (Some(V(x)), Some(V(y))) = (arr(a), arr(b)) else {
                return Some(Err(Error::BadType));
            };
            if x.len() != y.len() {
                return Some(Err(Error::Dim));
            }
            one(complex(
                x.iter()
                    .zip(&y)
                    .fold((0.0, 0.0), |s, (p, q)| add(s, mul(*p, *q))),
            ))
        }
        ("GET", [a, i]) => {
            let x = arr(a)?;
            one(position(&x, i).and_then(|p| complex(elems(&x)[p])))
        }
        ("PUT", [a, i, v]) => {
            let x = arr(a)?;
            one(position(&x, i).and_then(|p| {
                let mut e = elems(&x);
                e[p] = scalar(v).ok_or(Error::BadType)?;
                shaped(&dims(&x), &e)
            }))
        }
        _ => None,
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
    fn complex_arrays() {
        assert_eq!(run("[ (1,2) 3 ]"), ["[ (1,2) (3,0) ]"]);
        assert_eq!(
            run("[1 2] [3 4] R→C DUP C→R"),
            ["[ (1,3) (2,4) ]", "[ 1 2 ]", "[ 3 4 ]"]
        );
        assert_eq!(run("[1 2] (0,1) *"), ["[ (0,1) (0,2) ]"]);
        assert_eq!(
            run("[[(1,1) 0][0 1]] DUP *"),
            ["[[ (0,2) (0,0) ] [ (0,0) (1,0) ]]"]
        );
        assert_eq!(run("[[(0,1) 0][0 2]] DET"), ["(0,2)"]);
        assert_eq!(
            run("[(1,1) (2,0)] [[(0,1) 0][0 2]] /"),
            ["[ (1,-1) (1,0) ]"]
        );
        assert_eq!(
            run("[[(1,2) 3][4 5]] TRN"),
            ["[[ (1,-2) (4,0) ] [ (3,0) (5,0) ]]"]
        );
        assert_eq!(
            run("[(3,4)] ABS [(1,2)] { 1 } GET [ (1,2) ] 1 (5,6) PUT"),
            ["5", "(1,2)", "[ (5,6) ]"]
        );
        assert_eq!(run("[(1,2) (3,4)] RE [1 2] IM"), ["[ 1 3 ]", "[ 0 0 ]"]);
        assert_eq!(run("(1,2) 3 2 →ARRY TYPE"), ["4"]);
    }
}

//! Rearranging algebraics: ISOL solves for a name that appears once, QUAD
//! solves quadratics, EXPAN multiplies out products and powers.

use crate::alg::{Expr, contains, deriv, func, neg, op, subst};
use crate::calc::{Calc, Error, Res};

use Expr::{Fn, Name, Neg, Num, Op};

/// How many times `x` appears in `e`.
fn count(e: &Expr, x: &str) -> usize {
    match e {
        Name(n) => usize::from(n == x),
        Num(_) => 0,
        Neg(a) => count(a, x),
        Op(_, a, b) => count(a, x) + count(b, x),
        Fn(_, args) => args.iter().map(|a| count(a, x)).sum(),
    }
}

/// `e` as an expression equal to zero: 'A=B' is A-B.
fn as_zero(e: &Expr) -> Expr {
    match e {
        Op('=', l, r) => op('-', (**l).clone(), (**r).clone()),
        e => e.clone(),
    }
}

/// The general sign s1, which the calculator writes where a root has two
/// values: 'X=s1*√4'.
fn s1() -> Expr {
    Name("s1".into())
}

/// Solves `e = t` for `x`, which appears once in `e`, by undoing what is done
/// to it from the outside in. Inverse functions give their principal values.
fn undo(e: &Expr, t: Expr, x: &str) -> Res<Expr> {
    let has = |e: &Expr| contains(e, x);
    let c = |e: &Expr| e.clone();
    match e {
        Name(_) => Ok(t),
        Neg(a) => undo(a, neg(t), x),
        Op('+', a, b) if has(a) => undo(a, op('-', t, c(b)), x),
        Op('+', a, b) => undo(b, op('-', t, c(a)), x),
        Op('-', a, b) if has(a) => undo(a, op('+', t, c(b)), x),
        Op('-', a, b) => undo(b, op('-', c(a), t), x),
        Op('*', a, b) if has(a) => undo(a, op('/', t, c(b)), x),
        Op('*', a, b) => undo(b, op('/', t, c(a)), x),
        Op('/', a, b) if has(a) => undo(a, op('*', t, c(b)), x),
        Op('/', a, b) => undo(b, op('/', c(a), t), x),
        Op('^', a, b) if has(a) => {
            let root = match **b {
                Num(2.0) => op('*', s1(), func("√", vec![t])),
                Num(n) if n.fract() == 0.0 && n % 2.0 == 0.0 => {
                    op('*', s1(), func("XROOT", vec![t, c(b)]))
                }
                _ => func("XROOT", vec![t, c(b)]),
            };
            undo(a, root, x)
        }
        Op('^', a, b) => undo(b, op('/', func("LN", vec![t]), func("LN", vec![c(a)])), x),
        Fn(f, args) if args.len() == 1 => {
            let inverse = match f.as_str() {
                "SIN" => "ASIN",
                "COS" => "ACOS",
                "TAN" => "ATAN",
                "ASIN" => "SIN",
                "ACOS" => "COS",
                "ATAN" => "TAN",
                "SINH" => "ASINH",
                "COSH" => "ACOSH",
                "TANH" => "ATANH",
                "ASINH" => "SINH",
                "ACOSH" => "COSH",
                "ATANH" => "TANH",
                "LN" => "EXP",
                "EXP" => "LN",
                "LOG" => "ALOG",
                "ALOG" => "LOG",
                "LNP1" => "EXPM",
                "EXPM" => "LNP1",
                "√" => "SQ",
                "INV" => "INV",
                "NEG" => "NEG",
                "SQ" => return undo(&args[0], op('*', s1(), func("√", vec![t])), x),
                "ABS" => return undo(&args[0], op('*', s1(), t), x),
                _ => return Err(Error::BadValue),
            };
            undo(&args[0], func(inverse, vec![t]), x)
        }
        _ => Err(Error::BadValue),
    }
}

/// 'expr' 'X' ISOL: an equation X=… for an expression or equation in which
/// X appears once.
pub fn isolate(e: &Expr, x: &str) -> Res<Expr> {
    let (l, r) = match e {
        Op('=', l, r) => ((**l).clone(), (**r).clone()),
        e => (e.clone(), Num(0.0)),
    };
    let (side, other) = match (count(&l, x), count(&r, x)) {
        (1, 0) => (l, r),
        (0, 1) => (r, l),
        _ => return Err(Error::BadValue),
    };
    Ok(op('=', Name(x.into()), undo(&side, other, x)?))
}

/// Multiplies out sums inside products, quotients, negations and whole
/// powers, as far as it goes: '(X+1)^2' is 'X*X+X*1+(1*X+1*1)' before COLCT
/// tidies it.
pub fn expand(e: &Expr) -> Expr {
    let e = match e {
        Neg(a) => neg(expand(a)),
        Op(c, a, b) => op(*c, expand(a), expand(b)),
        Fn(f, args) => Fn(f.clone(), args.iter().map(expand).collect()),
        e => e.clone(),
    };
    let sum = |e: &Expr| matches!(e, Op('+' | '-', ..));
    match e {
        Neg(a) => match *a {
            Op(c @ ('+' | '-'), x, y) => op(c, expand(&neg(*x)), expand(&neg(*y))),
            a => neg(a),
        },
        Op('*', a, b) if sum(&a) => {
            let Op(c, x, y) = *a else { unreachable!() };
            op(
                c,
                expand(&op('*', *x, (*b).clone())),
                expand(&op('*', *y, *b)),
            )
        }
        Op('*', a, b) if sum(&b) => {
            let Op(c, x, y) = *b else { unreachable!() };
            op(
                c,
                expand(&op('*', (*a).clone(), *x)),
                expand(&op('*', *a, *y)),
            )
        }
        Op('/', a, b) if sum(&a) => {
            let Op(c, x, y) = *a else { unreachable!() };
            op(
                c,
                expand(&op('/', *x, (*b).clone())),
                expand(&op('/', *y, *b)),
            )
        }
        Op('^', a, b)
            if sum(&a) && matches!(*b, Num(n) if n.fract() == 0.0 && (2.0..=16.0).contains(&n)) =>
        {
            let Num(n) = *b else { unreachable!() };
            let mut p = (*a).clone();
            for _ in 1..n as u32 {
                p = expand(&op('*', p, (*a).clone()));
            }
            p
        }
        e => e,
    }
}

impl Calc {
    /// 'expr' 'X' QUAD: the roots of a polynomial of degree two (or one) in
    /// X, with s1 for the sign: 'X=(5+s1*1)/2'.
    pub(crate) fn quad(&self, e: &Expr, x: &str) -> Res<Expr> {
        let e = as_zero(e);
        let k = self.angle_factor();
        let at0 = [(x.to_string(), Num(0.0))];
        let d1 = self.collect(deriv(&e, x, &k)?);
        let d2 = self.collect(deriv(&d1, x, &k)?);
        if self.collect(deriv(&d2, x, &k)?) != Num(0.0) {
            return Err(Error::BadValue);
        }
        let c = self.collect(subst(&e, &at0));
        let b = self.collect(subst(&d1, &at0));
        let a = self.collect(op('/', d2, Num(2.0)));
        let root = if a == Num(0.0) {
            if b == Num(0.0) {
                return Err(Error::BadValue);
            }
            neg(op('/', c, b))
        } else {
            let disc = op(
                '-',
                op('^', b.clone(), Num(2.0)),
                op('*', op('*', Num(4.0), a.clone()), c),
            );
            op(
                '/',
                op('+', neg(b), op('*', s1(), func("√", vec![disc]))),
                op('*', Num(2.0), a),
            )
        };
        Ok(op('=', Name(x.into()), self.collect(root)))
    }
}

/// A term of a sum: a coefficient times factors raised to powers, the
/// factors kept in order of how they are written.
type Term = (f64, Vec<(Expr, f64)>);

fn key(e: &Expr) -> String {
    let mut s = String::new();
    crate::alg::write_expr(&mut s, e, crate::obj::Fmt::Std);
    s
}

fn atom(e: Expr) -> Vec<Term> {
    vec![(1.0, vec![(e, 1.0)])]
}

/// The terms of `e` as a sum of products, without multiplying out sums
/// raised to powers or divided by.
fn terms(e: &Expr) -> Vec<Term> {
    match e {
        Num(x) => vec![(*x, vec![])],
        Neg(a) => terms(a).into_iter().map(|(c, f)| (-c, f)).collect(),
        Op('+', a, b) => [terms(a), terms(b)].concat(),
        Op('-', a, b) => [terms(a), terms(&neg((**b).clone()))].concat(),
        Op('*', a, b) => {
            let (ta, tb) = (terms(a), terms(b));
            if ta.len() * tb.len() > 64 {
                return atom(e.clone());
            }
            ta.iter()
                .flat_map(|x| tb.iter().map(move |y| times(x, y)))
                .collect()
        }
        Op('/', a, b) => match &terms(b)[..] {
            [t] if t.0 != 0.0 => {
                let inv = (
                    1.0 / t.0,
                    t.1.iter().map(|(f, p)| (f.clone(), -p)).collect(),
                );
                terms(a).iter().map(|x| times(x, &inv)).collect()
            }
            _ => terms(a)
                .iter()
                .map(|x| times(x, &(1.0, vec![(colct(b), -1.0)])))
                .collect(),
        },
        Op('^', a, b) => match (&terms(a)[..], &**b) {
            ([(c, f)], Num(n)) if *c > 0.0 || n.fract() == 0.0 => {
                vec![(
                    c.powf(*n),
                    f.iter().map(|(f, p)| (f.clone(), p * n)).collect(),
                )]
            }
            _ => atom(op('^', colct(a), colct(b))),
        },
        Fn(f, args) => atom(Fn(f.clone(), args.iter().map(colct).collect())),
        e => atom(e.clone()),
    }
}

fn times(x: &Term, y: &Term) -> Term {
    let mut f = x.1.clone();
    for (g, p) in &y.1 {
        match f.iter_mut().find(|(h, _)| h == g) {
            Some((_, q)) => *q += p,
            None => f.push((g.clone(), *p)),
        }
    }
    f.retain(|(_, p)| *p != 0.0);
    (x.0 * y.0, f)
}

/// COLCT: like terms and like factors collected, 'X*X+X+X+1' is
/// 'X^2+2*X+1'. Terms of higher degree come first.
pub fn colct(e: &Expr) -> Expr {
    if let Op('=', l, r) = e {
        return op('=', colct(l), colct(r));
    }
    let mut sum: Vec<Term> = vec![];
    for (c, mut f) in terms(e) {
        f.sort_by_cached_key(|(g, _)| key(g));
        match sum.iter_mut().find(|t| t.1 == f) {
            Some(t) => t.0 += c,
            None => sum.push((c, f)),
        }
    }
    sum.retain(|t| crate::obj::r12(t.0) != 0.0);
    // Higher degree first, then alphabetically: 'Y+X' is 'X+Y'.
    let degree = |t: &Term| -t.1.iter().map(|(_, p)| p).sum::<f64>();
    let names = |t: &Term| t.1.iter().map(|(g, _)| key(g)).collect::<Vec<_>>();
    sum.sort_by(|a, b| {
        degree(a)
            .total_cmp(&degree(b))
            .then_with(|| names(a).cmp(&names(b)))
    });
    let mut out: Option<Expr> = None;
    for (c, f) in sum {
        let c = crate::obj::r12(c);
        let (num, den): (Vec<_>, Vec<_>) = f.into_iter().partition(|(_, p)| *p > 0.0);
        let power = |(g, p): (Expr, f64)| {
            if p.abs() == 1.0 {
                g
            } else {
                op('^', g, Num(p.abs()))
            }
        };
        let product = |fs: Vec<(Expr, f64)>| fs.into_iter().map(power).reduce(|a, b| op('*', a, b));
        let mag = match (product(num), c.abs()) {
            (None, m) => Num(m),
            (Some(p), 1.0) => p,
            (Some(p), m) => op('*', Num(m), p),
        };
        let t = match product(den) {
            Some(d) => op('/', mag, d),
            None => mag,
        };
        out = Some(match (out, t) {
            (None, Op('*', m, p)) if c < 0.0 && matches!(*m, Num(_)) => {
                let Num(m) = *m else { unreachable!() };
                op('*', Num(-m), *p)
            }
            (None, Num(m)) if c < 0.0 => Num(-m),
            (None, t) if c < 0.0 => neg(t),
            (None, t) => t,
            (Some(s), t) => op(if c < 0.0 { '-' } else { '+' }, s, t),
        });
    }
    out.unwrap_or(Num(0.0))
}

impl Calc {
    /// The coefficient m when `u` is m·x + k, so that a function of u can be
    /// integrated as one of x divided by m.
    fn slope(&self, u: &Expr, x: &str) -> Option<Expr> {
        let m = self.collect(deriv(u, x, &None).ok()?);
        (!contains(&m, x) && m != Num(0.0)).then_some(m)
    }

    /// An antiderivative of `e` in `x`, for polynomials and the elementary
    /// functions of linear arguments; None when there is no simple one.
    pub(crate) fn antideriv(&self, e: &Expr, x: &str) -> Option<Expr> {
        let has = |e: &Expr| contains(e, x);
        if !has(e) {
            return Some(op('*', e.clone(), Name(x.into())));
        }
        let f = |e: &Expr| self.antideriv(e, x);
        let n = Num;
        let r = match e {
            Name(_) => op('/', op('^', e.clone(), n(2.0)), n(2.0)),
            Neg(a) => neg(f(a)?),
            Op(c @ ('+' | '-'), a, b) => op(*c, f(a)?, f(b)?),
            Op('*', a, b) if !has(a) => op('*', (**a).clone(), f(b)?),
            Op('*', a, b) if !has(b) => op('*', f(a)?, (**b).clone()),
            Op('/', a, b) if !has(b) => op('/', f(a)?, (**b).clone()),
            // c/u for linear u is c·LN(u)/m.
            Op('/', a, b) if !has(a) => {
                let m = self.slope(b, x);
                match (m, &**b) {
                    (Some(m), _) => op(
                        '/',
                        op('*', (**a).clone(), func("LN", vec![(**b).clone()])),
                        m,
                    ),
                    (None, Op('^', u, p)) if !has(p) => {
                        return f(&op(
                            '*',
                            (**a).clone(),
                            op('^', (**u).clone(), neg((**p).clone())),
                        ));
                    }
                    _ => return self.by_expanding(e, x),
                }
            }
            // u^p for linear u: u^(p+1)/((p+1)·m), or LN(u)/m when p is -1.
            Op('^', u, p) if !has(p) => {
                let m = self.slope(u, x)?;
                if self.collect((**p).clone()) == Num(-1.0) {
                    op('/', func("LN", vec![(**u).clone()]), m)
                } else {
                    let p1 = op('+', (**p).clone(), n(1.0));
                    op('/', op('^', (**u).clone(), p1.clone()), op('*', p1, m))
                }
            }
            // a^u for constant a: a^u/(LN(a)·m).
            Op('^', a, u) if !has(a) => {
                let m = self.slope(u, x)?;
                op('/', e.clone(), op('*', func("LN", vec![(**a).clone()]), m))
            }
            Fn(name, args) if args.len() == 1 => {
                let u = &args[0];
                let m = self.slope(u, x)?;
                let g = |name: &str| func(name, vec![u.clone()]);
                // Trigonometric functions take angles in the angle mode.
                let k = match self.angle_factor() {
                    Some(k) => op('*', m.clone(), k),
                    None => m.clone(),
                };
                match name.as_str() {
                    "SIN" => neg(op('/', g("COS"), k)),
                    "COS" => op('/', g("SIN"), k),
                    "TAN" => neg(op('/', func("LN", vec![g("COS")]), k)),
                    "EXP" => op('/', g("EXP"), m),
                    "ALOG" => op('/', g("ALOG"), op('*', m, func("LN", vec![n(10.0)]))),
                    "LN" => op('/', op('-', op('*', u.clone(), g("LN")), u.clone()), m),
                    "SINH" => op('/', g("COSH"), m),
                    "COSH" => op('/', g("SINH"), m),
                    "√" => op(
                        '/',
                        op('*', n(2.0), op('^', u.clone(), n(1.5))),
                        op('*', n(3.0), m),
                    ),
                    "SQ" => op('/', op('^', u.clone(), n(3.0)), op('*', n(3.0), m)),
                    "INV" => op('/', func("LN", vec![u.clone()]), m),
                    _ => return None,
                }
            }
            _ => return self.by_expanding(e, x),
        };
        Some(r)
    }

    // Products of sums, as polynomials are, integrate term by term.
    fn by_expanding(&self, e: &Expr, x: &str) -> Option<Expr> {
        let x2 = colct(&expand(e));
        (x2 != *e).then(|| self.antideriv(&x2, x)).flatten()
    }

    /// lo hi 'expr' 'X' ∫ with a symbolic limit: F(hi)-F(lo).
    pub(crate) fn integral(&self, lo: &Expr, hi: &Expr, e: &Expr, x: &str) -> Res<Expr> {
        let big = self
            .antideriv(&self.collect(e.clone()), x)
            .ok_or(Error::BadValue)?;
        let at = |v: &Expr| subst(&big, &[(x.to_string(), v.clone())]);
        Ok(self.collect(colct(&self.collect(op('-', at(hi), at(lo))))))
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
    fn rearranging() {
        assert_eq!(run("'X^2-5*X+6' 'X' QUAD"), ["'X=(5+s1)/2'"]);
        assert_eq!(run("'X^2-5*X+6' 'X' QUAD { s1 -1 } | EVAL"), ["'X=2'"]);
        assert_eq!(run("'3*X-6' 'X' QUAD"), ["'X=2'"]);
        assert_eq!(run("'2*X+3=7' 'X' ISOL"), ["'X=2'"]);
        assert_eq!(run("'SIN(X)=.5' 'X' ISOL"), ["'X=30'"]);
        assert_eq!(run("'A*X^2=B' 'X' ISOL"), ["'X=s1*√(B/A)'"]);
        assert_eq!(run("'(X+1)^3' EXPAN"), ["'X^3+3*X^2+3*X+1'"]);
        assert_eq!(run("'(A+B)*(A-B)' EXPAN"), ["'A^2-B^2'"]);
        assert_eq!(run("'-(X-2)*3' EXPAN"), ["'-3*X+6'"]);
        assert_eq!(
            run("'X*X+X+X+1' COLCT 'Y+X' COLCT"),
            ["'X^2+2*X+1'", "'X+Y'"]
        );
        assert_eq!(run("0 'Y' '3*X^2+2*X+1' 'X' ∫"), ["'Y^3+Y^2+Y'"]);
        assert_eq!(run("RAD 0 'Y' 'SIN(X)' 'X' ∫"), ["'-COS(Y)+1'"]);
        assert_eq!(
            run("1 'Y' 'X^-2' 'X' ∫ 0 'Y' 'EXP(2*X)' 'X' ∫"),
            ["'1-1/Y'", "'.5*EXP(2*Y)-.5'"]
        );
        assert_eq!(run("0 1 'X^2' 'X' ∫"), [".333333333333"]);
        let mut c = Calc::new();
        assert!(c.exec_line("0 'Y' 'X*EXP(X)' 'X' ∫").is_err());
        assert!(c.exec_line("'X^2+X^2' 'X' ISOL").is_err());
        assert!(c.exec_line("'X^3' 'X' QUAD").is_err());
    }
}

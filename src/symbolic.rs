//! Commands on algebraic objects: differentiation, numeric integration and
//! summation, root finding, substitution, Taylor polynomials and fractions.

use crate::alg::{Expr, deriv, from_obj, neg, op, subst, tidy, to_obj};
use crate::calc::{Angle, Calc, Error, Res, fact, pure_any};
use crate::obj::{Node, Obj, r12};
use crate::rearrange::{colct, expand, isolate};

use Expr::{Fn, Name, Neg, Num, Op};

impl Calc {
    /// Folds the numeric parts of `e`, computing them as the stack would,
    /// then tidies away zeros and ones.
    pub(crate) fn collect(&self, e: Expr) -> Expr {
        let e = match e {
            Neg(a) => neg(self.collect(*a)),
            Op(c, a, b) => op(c, self.collect(*a), self.collect(*b)),
            Fn(f, args) => Fn(f, args.into_iter().map(|a| self.collect(a)).collect()),
            e => e,
        };
        let num = |e: &Expr| match e {
            Num(x) => Some(Obj::Real(*x)),
            _ => None,
        };
        let call: Option<(String, Vec<Obj>)> = match &e {
            Neg(a) => num(a).map(|x| ("NEG".into(), vec![x])),
            Op(c, a, b) if *c != '=' => {
                num(a).zip(num(b)).map(|(x, y)| (c.to_string(), vec![x, y]))
            }
            Fn(f, args) => args
                .iter()
                .map(num)
                .collect::<Option<Vec<_>>>()
                .map(|xs| (f.clone(), xs)),
            _ => None,
        };
        if let Some((w, args)) = call
            && let Some((n, f)) = pure_any(&w)
            && n == args.len()
            && let Ok(r) = self.compute(&w, f, &args)
            && let [Obj::Real(x)] = r[..]
        {
            return Num(x);
        }
        tidy(e)
    }

    /// The factor that turns the angle mode into radians, for derivatives
    /// of trigonometric functions.
    pub(crate) fn angle_factor(&self) -> Option<Expr> {
        let turn = match self.angle {
            Angle::Rad => return None,
            Angle::Deg => 180.0,
            Angle::Grad => 200.0,
        };
        Some(op('/', Name("π".into()), Num(turn)))
    }

    /// The value of `e` with `var` set to `x`, computed as →NUM would.
    pub(crate) fn eval_at(&mut self, e: &Obj, var: &str, x: f64) -> Res<f64> {
        let base = self.stack.len();
        self.stack.push(Obj::Real(x));
        let was = std::mem::replace(&mut self.numeric, true);
        let r = self.with_locals(&[var.to_string()], |c| c.eval_obj(e.clone()));
        self.numeric = was;
        let r = r.and_then(|_| match &self.stack[base..] {
            [Obj::Real(y)] => Ok(*y),
            [_] => Err(Error::BadType),
            _ => Err(Error::BadValue),
        });
        self.stack.truncate(base);
        r
    }

    // Adaptive Simpson's rule on [a, b], given f at a, the midpoint and b.
    #[allow(clippy::too_many_arguments)]
    fn simpson(
        &mut self,
        e: &Obj,
        v: &str,
        (a, b): (f64, f64),
        (fa, fm, fb): (f64, f64, f64),
        whole: f64,
        eps: f64,
        depth: u32,
    ) -> Res<f64> {
        let m = (a + b) / 2.0;
        let (lm, rm) = ((a + m) / 2.0, (m + b) / 2.0);
        let (flm, frm) = (self.eval_at(e, v, lm)?, self.eval_at(e, v, rm)?);
        let left = (m - a) / 6.0 * (fa + 4.0 * flm + fm);
        let right = (b - m) / 6.0 * (fm + 4.0 * frm + fb);
        let delta = left + right - whole;
        if depth == 0 || delta.abs() <= 15.0 * eps {
            return Ok(left + right + delta / 15.0);
        }
        Ok(
            self.simpson(e, v, (a, m), (fa, flm, fm), left, eps / 2.0, depth - 1)?
                + self.simpson(e, v, (m, b), (fm, frm, fb), right, eps / 2.0, depth - 1)?,
        )
    }

    fn integrate(&mut self, e: &Obj, v: &str, a: f64, b: f64) -> Res<f64> {
        let (fa, fb) = (self.eval_at(e, v, a)?, self.eval_at(e, v, b)?);
        let fm = self.eval_at(e, v, (a + b) / 2.0)?;
        let whole = (b - a) / 6.0 * (fa + 4.0 * fm + fb);
        let eps = 1e-12 * whole.abs().max(1.0);
        self.simpson(e, v, (a, b), (fa, fm, fb), whole, eps, 40)
    }

    /// A root of `e` in `var` near the guess, or between two guesses.
    /// Bracketed roots are narrowed by bisection; otherwise the secant method
    /// looks for one, switching to bisection once the sign changes.
    pub(crate) fn root(&mut self, e: &Obj, var: &str, (mut a, mut b): (f64, f64)) -> Res<f64> {
        let (mut fa, mut fb) = (self.eval_at(e, var, a)?, self.eval_at(e, var, b)?);
        for _ in 0..200 {
            if fa == 0.0 {
                return Ok(a);
            }
            if fb == 0.0 {
                return Ok(b);
            }
            if fa.signum() != fb.signum() {
                break;
            }
            let x = if fb != fa {
                b - fb * (b - a) / (fb - fa)
            } else {
                b + (b - a)
            };
            if !x.is_finite() {
                return Err(Error::BadValue);
            }
            (a, fa) = (b, fb);
            b = x;
            fb = self.eval_at(e, var, b)?;
            if (b - a).abs() <= 1e-15 * b.abs().max(1e-300) {
                return if fb.abs() < 1e-9 {
                    Ok(b)
                } else {
                    Err(Error::BadValue)
                };
            }
        }
        if fa.signum() == fb.signum() {
            return Err(Error::BadValue);
        }
        for _ in 0..200 {
            let m = (a + b) / 2.0;
            if m == a || m == b {
                break;
            }
            let fm = self.eval_at(e, var, m)?;
            if fm == 0.0 {
                return Ok(m);
            }
            if fm.signum() == fa.signum() {
                (a, fa) = (m, fm);
            } else {
                b = m;
            }
        }
        Ok(if fa.abs() < fb.abs() { a } else { b })
    }

    pub(crate) fn symbolic_cmd(&mut self, w: &str) -> Option<Res<()>> {
        Some(match w {
            "∂" => self.args(2, |c, a| {
                let Obj::Name(x) = &a[1] else {
                    return Err(Error::BadType);
                };
                let d = deriv(&from_obj(&a[0])?, x, &c.angle_factor())?;
                Ok(vec![to_obj(c.collect(d))])
            }),
            // 'F(X)=X^2' DEFINE makes F a user-defined function, « → X 'X^2' »;
            // 'A=5' DEFINE stores 5 in A.
            "DEFINE" => self.need(1).and_then(|_| {
                let Obj::Alg(Op('=', l, r)) = self.peek(1).clone() else {
                    return Err(Error::BadType);
                };
                let (name, v) = match *l {
                    Name(n) => (n, to_obj(*r)),
                    Fn(f, params) => {
                        let names = params
                            .into_iter()
                            .map(|p| match p {
                                Name(n) => Ok(n),
                                _ => Err(Error::BadValue),
                            })
                            .collect::<Res<Vec<_>>>()?;
                        let expr = to_obj(*r);
                        (f, Obj::Prog(vec![Node::LocalAlg { names, expr }]))
                    }
                    _ => return Err(Error::BadValue),
                };
                self.store(name, v)?;
                self.stack.pop();
                Ok(())
            }),
            "ISOL" | "QUAD" => self.args(2, |c, a| {
                let Obj::Name(x) = &a[1] else {
                    return Err(Error::BadType);
                };
                let e = from_obj(&a[0])?;
                let r = if w == "ISOL" {
                    c.collect(isolate(&e, x)?)
                } else {
                    c.quad(&e, x)?
                };
                Ok(vec![to_obj(r)])
            }),
            // EXPAN multiplies out, then collects like terms as COLCT does.
            "EXPAN" | "COLCT" => self.args(1, |c, a| {
                let mut e = c.collect(from_obj(&a[0])?);
                if w == "EXPAN" {
                    e = expand(&e);
                }
                Ok(vec![to_obj(c.collect(colct(&e)))])
            }),
            // 'X^2+Y' { X 3 } | substitutes 3 for X: '3^2+Y'.
            "|" => self.args(2, |_, a| {
                let Obj::List(l) = &a[1] else {
                    return Err(Error::BadType);
                };
                if l.len() % 2 != 0 {
                    return Err(Error::BadValue);
                }
                let subs = l
                    .chunks(2)
                    .map(|p| match &p[0] {
                        Obj::Name(n) => Ok((n.clone(), from_obj(&p[1])?)),
                        _ => Err(Error::BadType),
                    })
                    .collect::<Res<Vec<_>>>()?;
                Ok(vec![to_obj(subst(&from_obj(&a[0])?, &subs))])
            }),
            "EQ→" => self.args(1, |_, a| match &a[0] {
                Obj::Alg(Op('=', l, r)) => Ok(vec![to_obj((**l).clone()), to_obj((**r).clone())]),
                o @ (Obj::Alg(_) | Obj::Name(_) | Obj::Real(_)) => {
                    Ok(vec![o.clone(), Obj::Real(0.0)])
                }
                _ => Err(Error::BadType),
            }),
            "→Q" => self.args(1, |_, a| {
                let Obj::Real(x) = a[0] else {
                    return Err(Error::BadType);
                };
                Ok(vec![to_obj(fraction(x))])
            }),
            // lo hi 'expr' 'X' ∫ integrates numerically.
            // With a name or expression for a limit, symbolically: 0 'Y'
            // 'X^2' 'X' ∫ is 'Y^3/3'.
            "∫" => self.args(4, |c, a| {
                let Obj::Name(x) = &a[3] else {
                    return Err(Error::BadType);
                };
                if let (Obj::Real(lo), Obj::Real(hi)) = (&a[0], &a[1]) {
                    return Ok(vec![Obj::Real(r12(c.integrate(&a[2], x, *lo, *hi)?))]);
                }
                let (lo, hi, e) = (from_obj(&a[0])?, from_obj(&a[1])?, from_obj(&a[2])?);
                Ok(vec![to_obj(c.integral(&lo, &hi, &e, x)?)])
            }),
            // 'I' 1 10 'I^2' Σ sums over I = 1, 2, … 10.
            "Σ" => self.args(4, |c, a| {
                let (Obj::Name(i), Obj::Real(lo), Obj::Real(hi)) = (&a[0], &a[1], &a[2]) else {
                    return Err(Error::BadType);
                };
                if hi - lo > 1e7 {
                    return Err(Error::BadValue);
                }
                let mut s = 0.0;
                let mut k = *lo;
                while k <= *hi {
                    s += c.eval_at(&a[3], i, k)?;
                    k += 1.0;
                }
                Ok(vec![Obj::Real(r12(s))])
            }),
            // 'expr' 'X' guess ROOT, or { guess1 guess2 } to bracket the root.
            "ROOT" => self.args(3, |c, a| {
                let Obj::Name(x) = &a[1] else {
                    return Err(Error::BadType);
                };
                let guesses = match &a[2] {
                    Obj::Real(g) => (*g, g + (g.abs() * 1e-3).max(1e-3)),
                    Obj::List(l) => match &l[..] {
                        [Obj::Real(g)] => (*g, g + (g.abs() * 1e-3).max(1e-3)),
                        [Obj::Real(g), Obj::Real(h)] if g != h => (*g, *h),
                        _ => return Err(Error::BadValue),
                    },
                    _ => return Err(Error::BadType),
                };
                // An equation's root is where its sides are equal.
                let e = match &a[0] {
                    Obj::Alg(Op('=', l, r)) => Obj::Alg(op('-', (**l).clone(), (**r).clone())),
                    e => e.clone(),
                };
                Ok(vec![Obj::Real(r12(c.root(&e, x, guesses)?))])
            }),
            // 'expr' 'X' n TAYLR: the Taylor polynomial of degree n about X=0.
            "TAYLR" => self.args(3, |c, a| {
                let (Obj::Name(x), Obj::Real(n)) = (&a[1], &a[2]) else {
                    return Err(Error::BadType);
                };
                if *n < 0.0 || n.fract() != 0.0 || *n > 30.0 {
                    return Err(Error::BadValue);
                }
                let k = c.angle_factor();
                let mut d = from_obj(&a[0])?;
                let mut sum = Num(0.0);
                let at0 = [(x.clone(), Num(0.0))];
                for i in 0..=*n as u32 {
                    let coef = c.collect(subst(&d, &at0));
                    let Obj::Real(f) = fact(i as f64)? else {
                        unreachable!()
                    };
                    let power = op('^', Name(x.clone()), Num(i as f64));
                    sum = op('+', sum, op('*', op('/', coef, Num(f)), power));
                    d = c.collect(deriv(&d, x, &k)?);
                }
                Ok(vec![to_obj(c.collect(sum))])
            }),
            _ => return None,
        })
    }

    /// Runs `f` on the top `n` objects, which it replaces, or which are put
    /// back if it fails.
    pub(crate) fn args(
        &mut self,
        n: usize,
        f: impl FnOnce(&mut Self, &[Obj]) -> Res<Vec<Obj>>,
    ) -> Res<()> {
        self.need(n)?;
        let args = self.stack.split_off(self.stack.len() - n);
        let base = self.stack.len();
        match f(self, &args) {
            Ok(out) => {
                self.stack.truncate(base);
                self.stack.extend(out);
                self.last_args = args;
                Ok(())
            }
            Err(e) => {
                self.stack.truncate(base);
                self.stack.extend(args);
                Err(e)
            }
        }
    }
}

/// The fraction closest to `x` with a denominator the 12 digits can show,
/// by continued fractions: .75 →Q is '3/4'.
fn fraction(x: f64) -> Expr {
    if x.fract() == 0.0 {
        return Num(x);
    }
    // Convergents h/k of the continued fraction.
    let (mut h0, mut h1) = (0.0, 1.0);
    let (mut k0, mut k1) = (1.0, 0.0);
    let mut r = x.abs();
    for _ in 0..40 {
        let a = r.floor();
        (h0, h1) = (h1, a * h1 + h0);
        (k0, k1) = (k1, a * k1 + k0);
        if (h1 / k1 - x.abs()).abs() <= 1e-11 * x.abs() || r - a < 1e-12 || k1 > 1e11 {
            break;
        }
        r = 1.0 / (r - a);
    }
    op('/', Num(h1.copysign(x)), Num(k1))
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
    fn algebraics() {
        assert_eq!(run("'X' 2 +"), ["'X+2'"]);
        assert_eq!(run("'X' SIN 'Y' *"), ["'SIN(X)*Y'"]);
        assert_eq!(run("'X^2+1' 3 'X' STO EVAL"), ["10"]);
        assert_eq!(run("'A+B' EVAL"), ["'A+B'"]);
        assert_eq!(run("2 'A' STO 'A*B' EVAL"), ["'2*B'"]);
        assert_eq!(run("'π*2' →NUM"), ["6.28318530718"]);
        assert_eq!(run("« → x 'x^2+1' » 'F' STO 'F(3)' EVAL"), ["10"]);
        assert_eq!(run("'X^2' { X 3 } |"), ["'3^2'"]);
        assert_eq!(run("'A=B' EQ→"), ["'A'", "'B'"]);
        assert_eq!(
            run("'F(X,Y)=X*Y+1' DEFINE 3 4 F 'F(2,5)' EVAL"),
            ["13", "11"]
        );
    }

    #[test]
    fn calculus() {
        assert_eq!(run("'X^3' 'X' ∂"), ["'3*X^2'"]);
        assert_eq!(run("RAD 'SIN(X)*X' 'X' ∂"), ["'COS(X)*X+SIN(X)'"]);
        assert_eq!(run("'LN(X)' 'X' ∂"), ["'1/X'"]);
        assert_eq!(run("0 1 'X^2' 'X' ∫"), [".333333333333"]);
        assert_eq!(run("RAD 0 π 'SIN(X)' 'X' ∫"), ["2"]);
        assert_eq!(run("'I' 1 10 'I^2' Σ"), ["385"]);
        assert_eq!(run("'X^2-2' 'X' 1 ROOT"), ["1.41421356237"]);
        assert_eq!(run("'X^2=9' 'X' { 0 10 } ROOT"), ["3"]);
        assert_eq!(
            run("RAD 'EXP(X)' 'X' 3 TAYLR"),
            ["'1+X+.5*X^2+.166666666667*X^3'"]
        );
        assert_eq!(run(".75 →Q  -1.5 →Q  3 →Q"), ["'3/4'", "'-3/2'", "3"]);
    }
}

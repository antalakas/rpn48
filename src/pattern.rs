//! Rewriting algebraics by pattern (↑MATCH and ↓MATCH), and the smaller
//! symbolic commands: APPLY, QUOTE, SHOW and →Qπ.

use std::f64::consts::PI;

use crate::alg::{Expr, contains, from_obj, func, op, subst, to_obj};
use crate::calc::{Calc, Error, Res};
use crate::obj::{Obj, r12};

type Binds = Vec<(String, Expr)>;

/// A wildcard: a name starting with &, which matches any subexpression.
fn wildcard(e: &Expr) -> Option<&str> {
    match e {
        Expr::Name(n) if n.starts_with('&') => Some(n),
        _ => None,
    }
}

/// Whether `e` has the form of `p`, each wildcard standing for the same
/// subexpression wherever it appears.
fn matches(p: &Expr, e: &Expr, binds: &mut Binds) -> bool {
    if let Some(w) = wildcard(p) {
        return match binds.iter().find(|(n, _)| n == w) {
            Some((_, b)) => b == e,
            None => {
                binds.push((w.into(), e.clone()));
                true
            }
        };
    }
    match (p, e) {
        (Expr::Num(a), Expr::Num(b)) => a == b,
        (Expr::Name(a), Expr::Name(b)) => a == b,
        (Expr::Neg(a), Expr::Neg(b)) => matches(a, b, binds),
        (Expr::Op(c, a1, b1), Expr::Op(d, a2, b2)) => {
            c == d && matches(a1, a2, binds) && matches(b1, b2, binds)
        }
        (Expr::Fn(f, xs), Expr::Fn(g, ys)) => {
            f == g && xs.len() == ys.len() && xs.iter().zip(ys).all(|(x, y)| matches(x, y, binds))
        }
        _ => false,
    }
}

struct Rule {
    pat: Expr,
    rep: Expr,
    cond: Option<Expr>,
}

impl Calc {
    /// The replacement for `e` if it matches the rule and its condition,
    /// evaluated with the wildcards' values, holds.
    fn try_rule(&mut self, r: &Rule, e: &Expr) -> Res<Option<Expr>> {
        let mut binds = vec![];
        if !matches(&r.pat, e, &mut binds) {
            return Ok(None);
        }
        if let Some(c) = &r.cond {
            self.stack.push(to_obj(subst(c, &binds)));
            self.eval_word("→NUM")?;
            match self.pop()? {
                Obj::Real(0.0) => return Ok(None),
                Obj::Real(_) => {}
                _ => return Err(Error::BadType),
            }
        }
        Ok(Some(subst(&r.rep, &binds)))
    }

    /// Rewrites every subexpression that matches, from the top down (a
    /// replacement is not looked into again) or from the bottom up.
    fn rewrite(&mut self, r: &Rule, e: &Expr, top_down: bool, hit: &mut bool) -> Res<Expr> {
        if top_down && let Some(x) = self.try_rule(r, e)? {
            *hit = true;
            return Ok(x);
        }
        let mut go = |c: &mut Calc, x: &Expr| c.rewrite(r, x, top_down, hit);
        let inner = match e {
            Expr::Neg(a) => Expr::Neg(Box::new(go(self, a)?)),
            Expr::Op(c, a, b) => {
                let a = go(self, a)?;
                op(*c, a, go(self, b)?)
            }
            Expr::Fn(f, args) => func(f, args.iter().map(|a| go(self, a)).collect::<Res<_>>()?),
            e => e.clone(),
        };
        if !top_down && let Some(x) = self.try_rule(r, &inner)? {
            *hit = true;
            return Ok(x);
        }
        Ok(inner)
    }

    /// 'expr' { 'pattern' 'replacement' 'condition' } ↑MATCH or ↓MATCH:
    /// the rewritten expression, and 1 if anything was replaced, else 0.
    fn match_cmd(&mut self, top_down: bool) -> Res<()> {
        self.args(2, |c, a| {
            let e = from_obj(&a[0])?;
            let Obj::List(l) = &a[1] else {
                return Err(Error::BadType);
            };
            let rule = match &l[..] {
                [p, r] => Rule {
                    pat: from_obj(p)?,
                    rep: from_obj(r)?,
                    cond: None,
                },
                [p, r, k] => Rule {
                    pat: from_obj(p)?,
                    rep: from_obj(r)?,
                    cond: Some(from_obj(k)?),
                },
                _ => return Err(Error::BadValue),
            };
            let mut hit = false;
            let out = c.rewrite(&rule, &e, top_down, &mut hit)?;
            Ok(vec![to_obj(out), Obj::Real(if hit { 1.0 } else { 0.0 })])
        })
    }

    /// Makes `x` appear in `e` by putting in the variables that refer to it,
    /// as SHOW does.
    fn show_name(&self, e: &Expr, x: &str, depth: usize) -> Expr {
        match e {
            Expr::Name(n) if n != x && depth < 32 => match self.global(n) {
                Some(v @ (Obj::Alg(_) | Obj::Name(_))) => {
                    let v = from_obj(v).unwrap();
                    if self.refers_to(&v, x, depth + 1) {
                        self.show_name(&v, x, depth + 1)
                    } else {
                        e.clone()
                    }
                }
                _ => e.clone(),
            },
            Expr::Neg(a) => Expr::Neg(Box::new(self.show_name(a, x, depth))),
            Expr::Op(c, a, b) => op(*c, self.show_name(a, x, depth), self.show_name(b, x, depth)),
            Expr::Fn(f, args) => func(
                f,
                args.iter().map(|a| self.show_name(a, x, depth)).collect(),
            ),
            e => e.clone(),
        }
    }

    fn refers_to(&self, e: &Expr, x: &str, depth: usize) -> bool {
        if contains(e, x) {
            return true;
        }
        if depth >= 32 {
            return false;
        }
        let mut names = vec![];
        names_in(e, &mut names);
        names.iter().any(|n| match self.global(n) {
            Some(v @ (Obj::Alg(_) | Obj::Name(_))) => {
                self.refers_to(&from_obj(v).unwrap(), x, depth + 1)
            }
            _ => false,
        })
    }

    pub(crate) fn pattern_cmd(&mut self, w: &str) -> Option<Res<()>> {
        Some(match w {
            "↑MATCH" => self.match_cmd(true),
            "↓MATCH" => self.match_cmd(false),
            // { args } 'name' APPLY: 'name(args)'.
            "APPLY" => self.args(2, |_, a| {
                let (Obj::List(l), Obj::Name(f)) = (&a[0], &a[1]) else {
                    return Err(Error::BadType);
                };
                let args = l.iter().map(from_obj).collect::<Res<Vec<_>>>()?;
                Ok(vec![Obj::Alg(func(f, args))])
            }),
            // In an algebraic, QUOTE(x) keeps x from being evaluated; on the
            // stack it is the object itself.
            "QUOTE" => self.need(1),
            "SHOW" => self.args(2, |c, a| {
                let e = from_obj(&a[0])?;
                let names: Vec<String> = match &a[1] {
                    Obj::Name(n) => vec![n.clone()],
                    Obj::List(l) => l
                        .iter()
                        .map(|o| match o {
                            Obj::Name(n) => Ok(n.clone()),
                            _ => Err(Error::BadType),
                        })
                        .collect::<Res<_>>()?,
                    _ => return Err(Error::BadType),
                };
                let out = names.iter().fold(e, |e, n| c.show_name(&e, n, 0));
                Ok(vec![to_obj(out)])
            }),
            // x →Qπ: a fraction of π when that is simpler than a fraction.
            "→Qπ" => self.args(1, |c, a| {
                let Obj::Real(x) = a[0] else {
                    return Err(Error::BadType);
                };
                let q = |c: &mut Calc, x: f64| -> Res<Obj> {
                    c.stack.push(Obj::Real(x));
                    c.eval_word("→Q")?;
                    c.pop()
                };
                let den = |o: &Obj| match o {
                    Obj::Alg(Expr::Op('/', _, d)) => match **d {
                        Expr::Num(d) => d,
                        _ => f64::MAX,
                    },
                    _ => 1.0,
                };
                let (plain, by_pi) = (q(c, x)?, q(c, r12(x / PI))?);
                if x == 0.0 || den(&plain) <= den(&by_pi) {
                    return Ok(vec![plain]);
                }
                let pi = Expr::Name("π".into());
                Ok(vec![Obj::Alg(match from_obj(&by_pi)? {
                    Expr::Num(1.0) => pi,
                    e => op('*', e, pi),
                })])
            }),
            _ => return None,
        })
    }
}

fn names_in(e: &Expr, out: &mut Vec<String>) {
    match e {
        Expr::Name(n) => out.push(n.clone()),
        Expr::Neg(a) => names_in(a, out),
        Expr::Op(_, a, b) => {
            names_in(a, out);
            names_in(b, out);
        }
        Expr::Fn(_, args) => args.iter().for_each(|a| names_in(a, out)),
        Expr::Num(_) => {}
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
    fn matching() {
        assert_eq!(
            run("'SIN(A)^2+COS(A)^2' { 'SIN(&X)^2+COS(&X)^2' 1 } ↓MATCH"),
            ["1", "1"]
        );
        assert_eq!(run("'A+B' { '&X*&Y' 0 } ↑MATCH"), ["'A+B'", "0"]);
        assert_eq!(
            run("'LN(A*B)+LN(C)' { 'LN(&X*&Y)' 'LN(&X)+LN(&Y)' } ↑MATCH"),
            ["'LN(A)+LN(B)+LN(C)'", "1"]
        );
        // The condition sees the wildcards' values.
        assert_eq!(
            run("5 'X' STO '2^3+X^3' { '&A^3' 'CUBE(&A)' '&A>2' } ↓MATCH DROP"),
            ["'2^3+CUBE(X)'"]
        );
    }

    #[test]
    fn others() {
        assert_eq!(run("{ X 2 } 'F' APPLY"), ["'F(X,2)'"]);
        assert_eq!(run("5 'X' STO 'QUOTE(X)+X' EVAL"), ["'X+5'"]);
        assert_eq!(run("'B+1' 'A' STO 'A*2' 'B' SHOW"), ["'(B+1)*2'"]);
        assert_eq!(
            run(".785398163397 →Qπ .75 →Qπ π →NUM →Qπ"),
            ["'1/4*π'", "'3/4'", "'π'"]
        );
    }
}

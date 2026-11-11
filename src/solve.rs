//! The equation in EQ and the SOLVR menu built from it: a key for each of
//! its variables to store a value, solve for it or recall it.

use crate::alg::{Expr, op};
use crate::calc::{Calc, Error, Res};
use crate::obj::{Obj, r12};

pub const EQ: &str = "EQ";

/// Names that stand for constants rather than variables to solve for.
const CONSTANTS: &[&str] = &["π", "e", "i", "s1"];

/// The variables of `e`, in the order they first appear.
pub fn names(e: &Expr, out: &mut Vec<String>) {
    match e {
        Expr::Name(n) if !CONSTANTS.contains(&n.as_str()) => {
            if !out.contains(n) {
                out.push(n.clone());
            }
        }
        Expr::Name(_) | Expr::Num(_) => {}
        Expr::Neg(a) => names(a, out),
        Expr::Op(_, a, b) => {
            names(a, out);
            names(b, out);
        }
        Expr::Fn(_, args) => args.iter().for_each(|a| names(a, out)),
    }
}

impl Calc {
    fn equation(&self) -> Res<Expr> {
        match self.global(EQ) {
            Some(Obj::Alg(e)) => Ok(e.clone()),
            Some(Obj::Name(n)) => Ok(Expr::Name(n.clone())),
            Some(_) => Err(Error::BadType),
            None => Err(Error::Undefined),
        }
    }

    /// The SOLVR menu: EQ's variables, then EXPR=.
    pub fn solvr_items(&self) -> Vec<String> {
        let mut out = vec![];
        if let Ok(e) = self.equation() {
            names(&e, &mut out);
            out.push("EXPR=".into());
        }
        out
    }

    /// Solves EQ for `x`, starting from x's value, and stores the root in x.
    /// Leaves it tagged on the stack and says, as the calculator does,
    /// whether it is a zero or only where the sign changes.
    pub fn solve_for(&mut self, x: &str) -> Res<&'static str> {
        let f = match self.equation()? {
            Expr::Op('=', l, r) => Obj::Alg(op('-', *l, *r)),
            e => Obj::Alg(e),
        };
        let g = match self.global(x) {
            Some(Obj::Real(g)) => *g,
            _ => 0.0,
        };
        let root = self.root(&f, x, (g, g + (g.abs() * 1e-3).max(1e-3)))?;
        let root = r12(root);
        let fx = self.eval_at(&f, x, root)?;
        self.store(x.into(), Obj::Real(root))?;
        self.stack
            .push(Obj::Tag(x.into(), Box::new(Obj::Real(root))));
        Ok(if fx.abs() < 1e-9 {
            "Zero"
        } else {
            "Sign Reversal"
        })
    }

    pub(crate) fn solve_cmd(&mut self, w: &str) -> Option<Res<()>> {
        Some(match w {
            "STEQ" => self.need(1).and_then(|_| {
                let e = self.peek(1).clone();
                self.store(EQ.into(), e)?;
                self.stack.pop();
                Ok(())
            }),
            "RCEQ" => match self.global(EQ) {
                Some(e) => {
                    let e = e.clone();
                    self.stack.push(e);
                    Ok(())
                }
                None => Err(Error::Undefined),
            },
            // EQ's value with the variables as they are: both sides of an
            // equation, tagged Left and Right.
            "EXPR=" => self.equation().and_then(|e| {
                let num = |c: &mut Calc, e: Expr| {
                    c.eval_at(&crate::alg::to_obj(e), "", 0.0)
                        .map(|x| Obj::Real(r12(x)))
                };
                let tag = |t: &str, o: Obj| Obj::Tag(t.into(), Box::new(o));
                let out = match e {
                    Expr::Op('=', l, r) => {
                        vec![tag("Left", num(self, *l)?), tag("Right", num(self, *r)?)]
                    }
                    e => vec![tag("Expr", num(self, e)?)],
                };
                self.stack.extend(out);
                Ok(())
            }),
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::calc::Calc;

    #[test]
    fn solver() {
        let mut c = Calc::new();
        c.exec_line("'A*X^2=B' STEQ 2 'A' STO 18 'B' STO 1 'X' STO")
            .unwrap();
        assert_eq!(c.solvr_items(), ["A", "X", "B", "EXPR="]);
        assert_eq!(c.solve_for("X").unwrap(), "Zero");
        c.exec_line("EXPR=").unwrap();
        let s: Vec<String> = c.stack.iter().map(|o| o.show(c.fmt)).collect();
        assert_eq!(s, [":X:3", ":Left:18", ":Right:18"]);
    }
}

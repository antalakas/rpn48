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
        if self.menu == crate::menu::Menu::Msolvr {
            return self.mroot(x).map(|_| "Zero");
        }
        let e = self.equation()?;
        let (root, fx) = self.solve_eq(e, x)?;
        self.stack
            .push(Obj::Tag(x.into(), Box::new(Obj::Real(root))));
        Ok(if fx.abs() < 1e-9 {
            "Zero"
        } else {
            "Sign Reversal"
        })
    }

    /// Solves `e` for `x`, starting from x's value, and stores the root in
    /// x: the root, and e's value there.
    fn solve_eq(&mut self, e: Expr, x: &str) -> Res<(f64, f64)> {
        let f = match e {
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
        Ok((root, fx))
    }

    /// Mpar, the multiple-equation solver's state: { equations variables
    /// user-defined title labels }.
    fn mpar(&self) -> Res<Mpar> {
        let Some(Obj::List(l)) = self.global(MPAR) else {
            return Err(Error::Undefined);
        };
        let names = |o: Option<&Obj>| -> Vec<String> {
            match o {
                Some(Obj::List(l)) => l
                    .iter()
                    .filter_map(|o| match o {
                        Obj::Name(n) => Some(n.clone()),
                        _ => None,
                    })
                    .collect(),
                _ => vec![],
            }
        };
        let eqs = match l.first() {
            Some(Obj::List(e)) => e.iter().map(crate::alg::from_obj).collect::<Res<_>>()?,
            _ => return Err(Error::BadValue),
        };
        Ok(Mpar {
            eqs,
            vars: names(l.get(1)),
            user: names(l.get(2)),
            title: l.get(3).cloned().unwrap_or(Obj::Str(String::new())),
            labels: l.get(4).cloned().unwrap_or(Obj::List(vec![])),
        })
    }

    fn set_mpar(&mut self, m: &Mpar) -> Res<()> {
        let names = |v: &[String]| Obj::List(v.iter().map(|n| Obj::Name(n.clone())).collect());
        let l = vec![
            Obj::List(m.eqs.iter().cloned().map(Obj::Alg).collect()),
            names(&m.vars),
            names(&m.user),
            m.title.clone(),
            m.labels.clone(),
        ];
        self.store(MPAR.into(), Obj::List(l))
    }

    /// The labels of the MSOLVR menu: Mpar's own, or its variables, then ALL.
    pub fn msolvr_items(&self) -> Vec<String> {
        let Ok(m) = self.mpar() else {
            return vec![];
        };
        let mut out = match &m.labels {
            Obj::List(l) if !l.is_empty() => l
                .iter()
                .map(|o| self.str_of(o).trim_matches('\'').to_string())
                .collect(),
            _ => m.vars.clone(),
        };
        out.push("ALL".into());
        out
    }

    /// Solves for `x`, or with ALL for every variable not given, using each
    /// equation that has a single unknown in turn. The given variables are
    /// those MUSER marked, or that hold a value and were not solved for.
    pub fn mroot(&mut self, x: &str) -> Res<()> {
        let m = self.mpar()?;
        let mut known: Vec<String> = m.user.clone();
        let targets: Vec<String> = if x == "ALL" {
            m.vars
                .iter()
                .filter(|v| !known.contains(v))
                .cloned()
                .collect()
        } else {
            known.retain(|v| v != x);
            vec![x.to_string()]
        };
        // Variables neither given nor wanted count as known when they hold a value.
        for v in &m.vars {
            if !targets.contains(v)
                && !known.contains(v)
                && matches!(self.global(v), Some(Obj::Real(_)))
            {
                known.push(v.clone());
            }
        }
        loop {
            if targets.iter().all(|t| known.contains(t)) {
                break;
            }
            let mut progress = false;
            for e in &m.eqs {
                let mut vs = vec![];
                names(e, &mut vs);
                let unknown: Vec<&String> = vs.iter().filter(|v| !known.contains(v)).collect();
                if let [u] = unknown[..] {
                    let u = u.clone();
                    self.solve_eq(e.clone(), &u)?;
                    known.push(u);
                    progress = true;
                }
            }
            if !progress {
                return Err(Error::User("Too Many Unknowns".into()));
            }
        }
        for t in &targets {
            let v = self.global(t).cloned().unwrap_or(Obj::Real(0.0));
            self.stack.push(Obj::Tag(t.clone(), Box::new(v)));
        }
        Ok(())
    }

    /// The names in level 1, or every variable for 'ALL'.
    fn mes_names(&self, all: &[String]) -> Res<Vec<String>> {
        match self.peek(1) {
            Obj::Name(n) if n == "ALL" => Ok(all.to_vec()),
            Obj::Name(n) => Ok(vec![n.clone()]),
            Obj::List(l) => l
                .iter()
                .map(|o| match o {
                    Obj::Name(n) => Ok(n.clone()),
                    _ => Err(Error::BadType),
                })
                .collect(),
            _ => Err(Error::BadType),
        }
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
            // The multiple-equation solver, on the list of equations in EQ.
            "MINIT" => match self.global(EQ).cloned() {
                Some(Obj::List(l)) => l
                    .iter()
                    .map(crate::alg::from_obj)
                    .collect::<Res<Vec<_>>>()
                    .and_then(|eqs| {
                        let mut vars = vec![];
                        for e in &eqs {
                            names(e, &mut vars);
                        }
                        self.set_mpar(&Mpar {
                            eqs,
                            vars,
                            user: vec![],
                            title: Obj::Str(String::new()),
                            labels: Obj::List(vec![]),
                        })
                    }),
                Some(_) => Err(Error::BadType),
                None => Err(Error::Undefined),
            },
            "MSOLVR" => self.mpar().map(|_| {
                (self.menu, self.page) = (crate::menu::Menu::Msolvr, 0);
            }),
            "MROOT" => self.need(1).and_then(|_| {
                let Obj::Name(n) = self.peek(1).clone() else {
                    return Err(Error::BadType);
                };
                self.stack.pop();
                let r = self.mroot(&n);
                if r.is_err() {
                    self.stack.push(Obj::Name(n));
                }
                r
            }),
            "MUSER" | "MCALC" => self.need(1).and_then(|_| {
                let mut m = self.mpar()?;
                let ns = self.mes_names(&m.vars)?;
                if w == "MUSER" {
                    for n in ns {
                        if !m.user.contains(&n) {
                            m.user.push(n);
                        }
                    }
                } else {
                    m.user.retain(|u| !ns.contains(u));
                }
                self.set_mpar(&m)?;
                self.stack.pop();
                Ok(())
            }),
            // "title" { labels } MITM: the MSOLVR menu's title and keys.
            "MITM" => self.need(2).and_then(|_| {
                let mut m = self.mpar()?;
                let n = self.stack.len();
                if !matches!(self.stack[n - 1], Obj::List(_)) {
                    return Err(Error::BadType);
                }
                (m.title, m.labels) = (self.stack[n - 2].clone(), self.stack[n - 1].clone());
                self.set_mpar(&m)?;
                self.stack.truncate(n - 2);
                Ok(())
            }),
            _ => return None,
        })
    }
}

const MPAR: &str = "Mpar";

struct Mpar {
    eqs: Vec<Expr>,
    vars: Vec<String>,
    user: Vec<String>,
    title: Obj,
    labels: Obj,
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

    #[test]
    fn multiple_equations() {
        let mut c = Calc::new();
        // A rectangle: area and perimeter from its sides, then a side back.
        c.exec_line(
            "{ 'A=W*H' 'P=2*(W+H)' } STEQ MINIT 3 'W' STO 4 'H' STO { W H } MUSER 'ALL' MROOT",
        )
        .unwrap();
        let s: Vec<String> = c.stack.iter().map(|o| o.show(c.fmt)).collect();
        assert_eq!(s, [":A:12", ":P:14"]);
        c.exec_line("CLEAR 'H' MCALC 'A' MUSER 20 'A' STO 'H' MROOT 'P' MROOT")
            .unwrap();
        let s: Vec<String> = c.stack.iter().map(|o| o.show(c.fmt)).collect();
        assert_eq!(s, [":H:6.66666666667", ":P:19.3333333333"]);
        c.exec_line("MSOLVR").unwrap();
        assert_eq!(c.msolvr_items(), ["A", "W", "H", "P", "ALL"]);
    }
}

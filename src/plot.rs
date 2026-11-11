//! Plotting EQ (or the statistics in ΣDAT) with the ranges and type kept in
//! PPAR, as the calculator does. DRAW computes the plot; the display shows
//! it until a key other than the cursor keys and ENTER is pressed.

use crate::calc::{Calc, Error, Res};
use crate::obj::{Obj, r12};
use crate::solve::EQ;

const PPAR: &str = "PPAR";

type Curve = Vec<(f64, f64)>;

/// What DRAW computed, for the display to show.
pub struct Plot {
    pub x: (f64, f64),
    pub y: (f64, f64),
    /// Each curve is broken where it is undefined or runs off to infinity.
    pub curves: Vec<Curve>,
    pub points: Curve,
    pub cursor: (f64, f64),
}

/// PPAR: { (xmin,ymin) (xmax,ymax) indep res axes type depend }.
struct Ppar {
    x: (f64, f64),
    y: (f64, f64),
    indep: String,
    kind: String,
}

impl Default for Ppar {
    fn default() -> Self {
        Ppar {
            x: (-6.5, 6.5),
            y: (-3.1, 3.2),
            indep: "X".into(),
            kind: "FUNCTION".into(),
        }
    }
}

impl Calc {
    fn ppar(&self) -> Ppar {
        let mut p = Ppar::default();
        if let Some(Obj::List(l)) = self.global(PPAR) {
            if let (Some(Obj::Complex(x0, y0)), Some(Obj::Complex(x1, y1))) = (l.first(), l.get(1))
            {
                (p.x, p.y) = ((*x0, *x1), (*y0, *y1));
            }
            if let Some(Obj::Name(n)) = l.get(2) {
                p.indep = n.clone();
            }
            if let Some(Obj::Name(n)) = l.get(5) {
                p.kind = n.clone();
            }
        }
        p
    }

    fn set_ppar(&mut self, p: &Ppar) -> Res<()> {
        let l = vec![
            Obj::Complex(p.x.0, p.y.0),
            Obj::Complex(p.x.1, p.y.1),
            Obj::Name(p.indep.clone()),
            Obj::Real(0.0),
            Obj::Complex(0.0, 0.0),
            Obj::Name(p.kind.clone()),
            Obj::Name("Y".into()),
        ];
        self.store(PPAR.into(), Obj::List(l))
    }

    /// The expressions to plot: EQ, each of a list of them, both sides of an
    /// equation.
    fn plotted(&self) -> Res<Vec<Obj>> {
        let eq = self.global(EQ).ok_or(Error::Undefined)?.clone();
        let items = match eq {
            Obj::List(l) => l,
            o => vec![o],
        };
        Ok(items
            .into_iter()
            .flat_map(|o| match o {
                Obj::Alg(crate::alg::Expr::Op('=', l, r)) => {
                    vec![crate::alg::to_obj(*l), crate::alg::to_obj(*r)]
                }
                o => vec![o],
            })
            .collect())
    }

    /// The curves and points of the plot PPAR describes, unclipped.
    fn trace(&mut self, p: &Ppar) -> Res<(Vec<Curve>, Curve)> {
        match p.kind.as_str() {
            "SCATTER" => {
                let (x, y) = self.xy()?;
                Ok((vec![], x.into_iter().zip(y).collect()))
            }
            kind => {
                let polar = kind == "POLAR";
                let (lo, hi) = if polar {
                    (0.0, self.rad_to_angle(2.0 * std::f64::consts::PI))
                } else {
                    p.x
                };
                let n = 720;
                let mut curves = vec![];
                for e in self.plotted()? {
                    let mut seg = vec![];
                    for i in 0..=n {
                        let t = lo + (hi - lo) * i as f64 / n as f64;
                        match self.eval_at(&e, &p.indep, t) {
                            Ok(v) if v.is_finite() && v.abs() < 1e100 => seg.push(if polar {
                                let a = self.angle_to_rad(t);
                                (v * a.cos(), v * a.sin())
                            } else {
                                (t, v)
                            }),
                            Err(e @ (Error::Interrupted | Error::Killed)) => return Err(e),
                            _ if seg.is_empty() => {}
                            _ => curves.push(std::mem::take(&mut seg)),
                        }
                    }
                    if !seg.is_empty() {
                        curves.push(seg);
                    }
                }
                Ok((curves, vec![]))
            }
        }
    }

    pub(crate) fn plot_cmd(&mut self, w: &str) -> Option<Res<()>> {
        let range = |c: &mut Calc| -> Res<(f64, f64)> {
            c.need(2)?;
            let n = c.stack.len();
            match (&c.stack[n - 2], &c.stack[n - 1]) {
                (Obj::Real(a), Obj::Real(b)) if a < b => Ok((*a, *b)),
                (Obj::Real(_), Obj::Real(_)) => Err(Error::BadValue),
                _ => Err(Error::BadType),
            }
        };
        Some(match w {
            "XRNG" | "YRNG" => range(self).and_then(|r| {
                let mut p = self.ppar();
                if w == "XRNG" {
                    p.x = r;
                } else {
                    p.y = r;
                }
                self.set_ppar(&p)?;
                self.stack.truncate(self.stack.len() - 2);
                Ok(())
            }),
            "INDEP" => self.need(1).and_then(|_| {
                let Obj::Name(n) = self.peek(1).clone() else {
                    return Err(Error::BadType);
                };
                let mut p = self.ppar();
                p.indep = n;
                self.set_ppar(&p)?;
                self.stack.pop();
                Ok(())
            }),
            "FUNCTION" | "POLAR" | "SCATTER" => {
                let mut p = self.ppar();
                p.kind = w.into();
                self.set_ppar(&p)
            }
            // Ranges that fit what is plotted: y for a function, both for the
            // other types.
            "AUTO" => {
                let mut p = self.ppar();
                self.trace(&p).and_then(|(curves, points)| {
                    let all: Vec<(f64, f64)> = curves.into_iter().flatten().chain(points).collect();
                    if all.is_empty() {
                        return Err(Error::BadValue);
                    }
                    let fit = |v: Vec<f64>| {
                        let (lo, hi) = v
                            .iter()
                            .fold((f64::MAX, f64::MIN), |(a, b), &x| (a.min(x), b.max(x)));
                        let pad = ((hi - lo) * 0.05).max(1e-9 * lo.abs().max(1.0));
                        (r12(lo - pad), r12(hi + pad))
                    };
                    p.y = fit(all.iter().map(|q| q.1).collect());
                    if p.kind != "FUNCTION" {
                        p.x = fit(all.iter().map(|q| q.0).collect());
                    }
                    self.set_ppar(&p)
                })
            }
            "DRAW" => {
                let p = self.ppar();
                self.trace(&p).map(|(curves, points)| {
                    self.plot = Some(Plot {
                        x: p.x,
                        y: p.y,
                        curves,
                        points,
                        cursor: ((p.x.0 + p.x.1) / 2.0, (p.y.0 + p.y.1) / 2.0),
                    });
                })
            }
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::calc::Calc;

    #[test]
    fn plotting() {
        let mut c = Calc::new();
        c.exec_line("'SIN(X)' STEQ RAD -3 3 XRNG AUTO DRAW")
            .unwrap();
        let p = c.plot.as_ref().unwrap();
        assert_eq!(p.x, (-3.0, 3.0));
        assert!(p.y.0 < -0.99 && p.y.1 > 0.99 && p.y.1 < 1.2);
        assert_eq!(p.curves.len(), 1);
        c.exec_line("'INV(X)' STEQ DRAW").unwrap();
        assert_eq!(c.plot.as_ref().unwrap().curves.len(), 2);
        c.exec_line("[1 2] Σ+ [2 4] Σ+ SCATTER AUTO DRAW").unwrap();
        assert_eq!(c.plot.as_ref().unwrap().points.len(), 2);
        c.exec_line("'Y' INDEP PPAR").unwrap();
        assert_eq!(
            c.stack.last().unwrap().show(c.fmt),
            "{ (.95,1.9) (2.05,4.1) Y 0 (0,0) SCATTER Y }"
        );
    }
}

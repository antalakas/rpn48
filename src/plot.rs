//! Plotting and the graphics display, PICT. As on the calculator, DRAW adds
//! a plot of EQ (or of the statistics in ΣDAT) to PICT with the ranges and
//! type kept in PPAR, the drawing commands add lines, boxes, arcs and
//! points, and ERASE clears it. PICT is shown by DRAW, PVIEW and GRAPH until
//! a key other than the cursor keys and ENTER is pressed.

use std::f64::consts::PI;

use crate::alg::{Expr, to_obj};
use crate::calc::{Calc, Error, Res};
use crate::obj::{Obj, r12};
use crate::solve::EQ;

const PPAR: &str = "PPAR";

/// PICT's size in pixels, for coordinates given as { #x #y }.
const PIXELS: (f64, f64) = (131.0, 64.0);

pub type Curve = Vec<(f64, f64)>;

/// What PICT holds, in user coordinates.
pub enum Shape {
    /// Joined points; plots are broken where undefined.
    Curve(Curve),
    Points(Curve),
    /// Bars from the x axis: left edge, right edge, height.
    Bars(Vec<(f64, f64, f64)>),
}

/// PICT on the display: the ranges it is seen through, and the cursor.
pub struct Plot {
    pub x: (f64, f64),
    pub y: (f64, f64),
    pub cursor: (f64, f64),
}

/// PPAR: { (xmin,ymin) (xmax,ymax) indep res axes type depend }, where indep
/// is a name or { name min max }.
struct Ppar {
    x: (f64, f64),
    y: (f64, f64),
    indep: String,
    /// The independent variable's own range, if it has one.
    trange: Option<(f64, f64)>,
    res: f64,
    kind: String,
}

impl Default for Ppar {
    fn default() -> Self {
        Ppar {
            x: (-6.5, 6.5),
            y: (-3.1, 3.2),
            indep: "X".into(),
            trange: None,
            res: 0.0,
            kind: "FUNCTION".into(),
        }
    }
}

const KINDS: &[&str] = &[
    "FUNCTION",
    "POLAR",
    "PARAMETRIC",
    "SCATTER",
    "BAR",
    "HISTOGRAM",
    "TRUTH",
];

// Failing to compute a point leaves a gap in the plot; stopping stops.
fn gap<T>(r: Res<T>) -> Res<Option<T>> {
    match r {
        Ok(v) => Ok(Some(v)),
        Err(e @ (Error::Interrupted | Error::Killed)) => Err(e),
        Err(_) => Ok(None),
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
            match l.get(2) {
                Some(Obj::Name(n)) => p.indep = n.clone(),
                Some(Obj::List(v)) => {
                    if let [Obj::Name(n), Obj::Real(a), Obj::Real(b)] = &v[..] {
                        (p.indep, p.trange) = (n.clone(), Some((*a, *b)));
                    }
                }
                _ => {}
            }
            if let Some(Obj::Real(r)) = l.get(3) {
                p.res = *r;
            }
            if let Some(Obj::Name(n)) = l.get(5) {
                p.kind = n.clone();
            }
        }
        p
    }

    fn set_ppar(&mut self, p: &Ppar) -> Res<()> {
        let indep = match p.trange {
            Some((a, b)) => Obj::List(vec![Obj::Name(p.indep.clone()), Obj::Real(a), Obj::Real(b)]),
            None => Obj::Name(p.indep.clone()),
        };
        let l = vec![
            Obj::Complex(p.x.0, p.y.0),
            Obj::Complex(p.x.1, p.y.1),
            indep,
            Obj::Real(p.res),
            Obj::Complex(0.0, 0.0),
            Obj::Name(p.kind.clone()),
            Obj::Name("Y".into()),
        ];
        self.store(PPAR.into(), Obj::List(l))
    }

    /// The expressions to plot: EQ, each of a list of them, both sides of an
    /// equation (except for TRUTH plots, where the equation is the test).
    fn plotted(&self, kind: &str) -> Res<Vec<Obj>> {
        let eq = self.global(EQ).ok_or(Error::Undefined)?.clone();
        let items = match eq {
            Obj::List(l) => l,
            o => vec![o],
        };
        Ok(items
            .into_iter()
            .flat_map(|o| match o {
                Obj::Alg(Expr::Op('=', l, r)) if kind != "TRUTH" => vec![to_obj(*l), to_obj(*r)],
                o => vec![o],
            })
            .collect())
    }

    /// Samples a curve over the parameter's range: `point` gives the point
    /// for t, or None where the curve is undefined, which breaks it.
    fn sample(
        &mut self,
        (lo, hi): (f64, f64),
        mut point: impl FnMut(&mut Calc, f64) -> Res<Option<(f64, f64)>>,
    ) -> Res<Vec<Shape>> {
        let n = 720;
        let (mut out, mut seg) = (vec![], vec![]);
        for i in 0..=n {
            let t = lo + (hi - lo) * i as f64 / n as f64;
            match point(self, t)? {
                Some((x, y)) if x.is_finite() && y.is_finite() && x.abs().max(y.abs()) < 1e100 => {
                    seg.push((x, y))
                }
                _ if seg.is_empty() => {}
                _ => out.push(Shape::Curve(std::mem::take(&mut seg))),
            }
        }
        if !seg.is_empty() {
            out.push(Shape::Curve(seg));
        }
        Ok(out)
    }

    /// The shapes of the plot PPAR describes.
    fn trace(&mut self, p: &Ppar) -> Res<Vec<Shape>> {
        let x = p.indep.clone();
        let mut out = vec![];
        match p.kind.as_str() {
            "SCATTER" => {
                let (xs, ys) = self.xy()?;
                out.push(Shape::Points(xs.into_iter().zip(ys).collect()));
            }
            // A bar for each value of the x column, RES (or one unit) wide.
            "BAR" => {
                let xs = self.x_column()?;
                let w = if p.res > 0.0 { p.res } else { 1.0 };
                let bars = xs
                    .iter()
                    .enumerate()
                    .map(|(i, &v)| (p.x.0 + i as f64 * w, p.x.0 + (i as f64 + 0.8) * w, v))
                    .collect();
                out.push(Shape::Bars(bars));
            }
            // How many values of the x column fall in each bin of width RES
            // (or a thirteenth of the range) across the x range.
            "HISTOGRAM" => {
                let xs = self.x_column()?;
                let w = if p.res > 0.0 {
                    p.res
                } else {
                    (p.x.1 - p.x.0) / 13.0
                };
                let bins = ((p.x.1 - p.x.0) / w).ceil().clamp(1.0, 1000.0) as usize;
                let mut n = vec![0.0; bins];
                for v in xs {
                    let b = ((v - p.x.0) / w).floor();
                    if b >= 0.0 && (b as usize) < bins {
                        n[b as usize] += 1.0;
                    }
                }
                let bars = n
                    .into_iter()
                    .enumerate()
                    .map(|(i, h)| (p.x.0 + i as f64 * w, p.x.0 + (i + 1) as f64 * w, h))
                    .collect();
                out.push(Shape::Bars(bars));
            }
            // A point wherever the test is true, on a grid over both ranges,
            // with the independent variable across and Y up.
            "TRUTH" => {
                let mut pts = vec![];
                for e in self.plotted("TRUTH")? {
                    for i in 0..=130 {
                        let xv = p.x.0 + (p.x.1 - p.x.0) * i as f64 / 130.0;
                        for j in 0..=63 {
                            let yv = p.y.0 + (p.y.1 - p.y.0) * j as f64 / 63.0;
                            let r = self.with_value(&x, xv, |c| c.value_at(&e, "Y", yv));
                            if let Some(Obj::Real(t)) = gap(r)?
                                && t != 0.0
                            {
                                pts.push((xv, yv));
                            }
                        }
                    }
                }
                out.push(Shape::Points(pts));
            }
            kind => {
                let full_turn = self.rad_to_angle(2.0 * PI);
                let range = p.trange.unwrap_or(match kind {
                    "POLAR" => (0.0, full_turn),
                    _ => p.x,
                });
                for e in self.plotted(kind)? {
                    let shapes = self.sample(range, |c, t| {
                        Ok(gap(c.value_at(&e, &x, t))?.and_then(|v| match (kind, v) {
                            ("POLAR", Obj::Real(r)) => {
                                let a = c.angle_to_rad(t);
                                Some((r * a.cos(), r * a.sin()))
                            }
                            // x+iy traced over t.
                            ("PARAMETRIC", Obj::Complex(re, im)) => Some((re, im)),
                            ("PARAMETRIC", Obj::Real(re)) => Some((re, 0.0)),
                            ("POLAR" | "PARAMETRIC", _) => None,
                            (_, Obj::Real(v)) => Some((t, v)),
                            _ => None,
                        }))
                    })?;
                    out.extend(shapes);
                }
            }
        }
        Ok(out)
    }

    /// Runs `f` with the local variable `name` set to `v`.
    fn with_value<T>(&mut self, name: &str, v: f64, f: impl FnOnce(&mut Calc) -> Res<T>) -> Res<T> {
        self.stack.push(Obj::Real(v));
        let mut out = None;
        self.with_locals(&[name.to_string()], |c| {
            out = Some(f(c));
            Ok(())
        })?;
        out.unwrap()
    }

    /// A point in user coordinates: (x,y), or { #x #y } in pixels.
    fn point(&self, o: &Obj) -> Res<(f64, f64)> {
        let p = self.ppar();
        match o {
            Obj::Complex(x, y) => Ok((*x, *y)),
            Obj::List(l) => match &l[..] {
                [Obj::Bin(px), Obj::Bin(py)] => Ok((
                    p.x.0 + *px as f64 * (p.x.1 - p.x.0) / (PIXELS.0 - 1.0),
                    p.y.1 - *py as f64 * (p.y.1 - p.y.0) / (PIXELS.1 - 1.0),
                )),
                _ => Err(Error::BadValue),
            },
            _ => Err(Error::BadType),
        }
    }

    fn show_pict(&mut self) {
        let p = self.ppar();
        self.plot = Some(Plot {
            x: p.x,
            y: p.y,
            cursor: ((p.x.0 + p.x.1) / 2.0, (p.y.0 + p.y.1) / 2.0),
        });
    }

    /// The top `n` objects as points.
    fn points(&self, n: usize) -> Res<Vec<(f64, f64)>> {
        self.need(n)?;
        self.stack[self.stack.len() - n..]
            .iter()
            .map(|o| self.point(o))
            .collect()
    }

    fn drop_n(&mut self, n: usize) {
        self.stack.truncate(self.stack.len() - n);
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
                self.drop_n(2);
                Ok(())
            }),
            // 'T' INDEP, or { T 0 6.28 } with the range it runs over.
            "INDEP" => self.need(1).and_then(|_| {
                let mut p = self.ppar();
                match self.peek(1).clone() {
                    Obj::Name(n) => (p.indep, p.trange) = (n, None),
                    Obj::List(l) => match &l[..] {
                        [Obj::Name(n), Obj::Real(a), Obj::Real(b)] if a < b => {
                            (p.indep, p.trange) = (n.clone(), Some((*a, *b)));
                        }
                        _ => return Err(Error::BadValue),
                    },
                    _ => return Err(Error::BadType),
                }
                self.set_ppar(&p)?;
                self.drop_n(1);
                Ok(())
            }),
            "RES" => self.need(1).and_then(|_| {
                let Obj::Real(r) = *self.peek(1) else {
                    return Err(Error::BadType);
                };
                let mut p = self.ppar();
                p.res = r.max(0.0);
                self.set_ppar(&p)?;
                self.drop_n(1);
                Ok(())
            }),
            k if KINDS.contains(&k) => {
                let mut p = self.ppar();
                p.kind = k.into();
                self.set_ppar(&p)
            }
            // Ranges that fit what is plotted: y for functions and bars, both
            // for the other types.
            "AUTO" => {
                let mut p = self.ppar();
                self.trace(&p).and_then(|shapes| {
                    let mut all: Vec<(f64, f64)> = vec![];
                    for s in shapes {
                        match s {
                            Shape::Curve(c) | Shape::Points(c) => all.extend(c),
                            Shape::Bars(b) => {
                                all.extend(b.into_iter().flat_map(|(a, z, h)| [(a, 0.0), (z, h)]))
                            }
                        }
                    }
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
                    if !matches!(p.kind.as_str(), "FUNCTION" | "BAR" | "HISTOGRAM" | "TRUTH") {
                        p.x = fit(all.iter().map(|q| q.0).collect());
                    }
                    self.set_ppar(&p)
                })
            }
            "DRAW" => {
                let p = self.ppar();
                self.trace(&p).map(|shapes| {
                    self.pict.extend(shapes);
                    self.show_pict();
                })
            }
            "ERASE" => {
                self.pict.clear();
                Ok(())
            }
            // PVIEW takes a point (or { }) as the calculator does; both show
            // PICT through PPAR's ranges.
            "PVIEW" | "GRAPH" => {
                let n = usize::from(w == "PVIEW");
                self.need(n).map(|_| {
                    self.drop_n(n);
                    self.show_pict();
                })
            }
            "LINE" | "TLINE" => self.points(2).map(|p| {
                self.pict.push(Shape::Curve(p));
                self.drop_n(2);
            }),
            "BOX" => self.points(2).map(|p| {
                let [(x0, y0), (x1, y1)] = [p[0], p[1]];
                self.pict.push(Shape::Curve(vec![
                    (x0, y0),
                    (x1, y0),
                    (x1, y1),
                    (x0, y1),
                    (x0, y0),
                ]));
                self.drop_n(2);
            }),
            // center radius θ1 θ2 ARC, the angles in the angle mode.
            "ARC" => self.need(4).and_then(|_| {
                let n = self.stack.len();
                let c = self.point(&self.stack[n - 4])?;
                let (Obj::Real(r), Obj::Real(a), Obj::Real(b)) =
                    (&self.stack[n - 3], &self.stack[n - 2], &self.stack[n - 1])
                else {
                    return Err(Error::BadType);
                };
                let (r, a, b) = (*r, self.angle_to_rad(*a), self.angle_to_rad(*b));
                let arc = (0..=360)
                    .map(|i| {
                        let t = a + (b - a) * i as f64 / 360.0;
                        (c.0 + r * t.cos(), c.1 + r * t.sin())
                    })
                    .collect();
                self.pict.push(Shape::Curve(arc));
                self.drop_n(4);
                Ok(())
            }),
            "PIXON" => self.points(1).map(|p| {
                self.pict.push(Shape::Points(p));
                self.drop_n(1);
            }),
            // Points drawn within half a pixel of it are on; PIXOFF takes
            // them away.
            "PIXOFF" | "PIX?" => self.points(1).map(|p| {
                let q = self.ppar();
                let near = |a: &(f64, f64)| {
                    (a.0 - p[0].0).abs() <= (q.x.1 - q.x.0) / (2.0 * PIXELS.0)
                        && (a.1 - p[0].1).abs() <= (q.y.1 - q.y.0) / (2.0 * PIXELS.1)
                };
                let mut on = false;
                for s in &mut self.pict {
                    if let Shape::Points(pts) = s {
                        on |= pts.iter().any(near);
                        if w == "PIXOFF" {
                            pts.retain(|a| !near(a));
                        }
                    }
                }
                self.drop_n(1);
                if w == "PIX?" {
                    self.stack.push(Obj::Real(if on { 1.0 } else { 0.0 }));
                }
            }),
            "PX→C" => self.points(1).map(|p| {
                self.drop_n(1);
                self.stack.push(Obj::Complex(r12(p[0].0), r12(p[0].1)));
            }),
            "C→PX" => self.points(1).map(|p| {
                let q = self.ppar();
                let px = ((p[0].0 - q.x.0) / (q.x.1 - q.x.0) * (PIXELS.0 - 1.0)).round();
                let py = ((q.y.1 - p[0].1) / (q.y.1 - q.y.0) * (PIXELS.1 - 1.0)).round();
                self.drop_n(1);
                self.stack.push(Obj::List(vec![
                    Obj::Bin(px.max(0.0) as u64),
                    Obj::Bin(py.max(0.0) as u64),
                ]));
            }),
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::Shape;
    use crate::calc::Calc;

    fn curves(c: &Calc) -> usize {
        c.pict
            .iter()
            .filter(|s| matches!(s, Shape::Curve(_)))
            .count()
    }

    #[test]
    fn plotting() {
        let mut c = Calc::new();
        c.exec_line("'SIN(X)' STEQ RAD -3 3 XRNG AUTO DRAW")
            .unwrap();
        let p = c.plot.as_ref().unwrap();
        assert_eq!(p.x, (-3.0, 3.0));
        assert!(p.y.0 < -0.99 && p.y.1 > 0.99 && p.y.1 < 1.2);
        assert_eq!(curves(&c), 1);
        c.exec_line("ERASE 'INV(X)' STEQ DRAW").unwrap();
        assert_eq!(curves(&c), 2);
        c.exec_line("ERASE [1 2] Σ+ [2 4] Σ+ SCATTER AUTO DRAW")
            .unwrap();
        assert!(matches!(&c.pict[..], [Shape::Points(p)] if p.len() == 2));
        c.exec_line("'Y' INDEP PPAR").unwrap();
        assert_eq!(
            c.stack.last().unwrap().show(c.fmt),
            "{ (.95,1.9) (2.05,4.1) Y 0 (0,0) SCATTER Y }"
        );
    }

    #[test]
    fn graphics() {
        let mut c = Calc::new();
        c.exec_line("(0,0) (1,1) LINE (0,0) (2,1) BOX (0,0) 1 0 90 ARC (1,1) PIXON")
            .unwrap();
        assert_eq!(c.pict.len(), 4);
        c.exec_line("(1,1) PIX? (1,1) PIXOFF (1,1) PIX? { #0d #0d } PX→C (6.5,3.2) C→PX")
            .unwrap();
        let s: Vec<String> = c.stack.iter().map(|o| o.show(c.fmt)).collect();
        assert_eq!(s, ["1", "0", "(-6.5,3.2)", "{ # 130d # 0d }"]);
        c.exec_line("ERASE 'T+i*SIN(T)' STEQ PARAMETRIC { T 0 6 } INDEP DRAW")
            .unwrap();
        assert_eq!(curves(&c), 1);
        c.exec_line("ERASE 'X' INDEP 1 Σ+ 2 Σ+ 2 Σ+ 0 5 XRNG 1 RES HISTOGRAM DRAW")
            .unwrap();
        assert!(matches!(&c.pict[..], [Shape::Bars(b)] if b[2].2 == 2.0));
        c.exec_line("ERASE 'X^2+Y^2<4' STEQ TRUTH DRAW").unwrap();
        assert!(matches!(&c.pict[..], [Shape::Points(p)] if !p.is_empty()));
    }
}

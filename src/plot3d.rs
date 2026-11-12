//! The 48G's further plot types: CONIC, DIFFEQ, SLOPEFIELD and the 3D ones
//! (WIREFRAME, PARSURFACE, YSLICE, PCONTOUR, GRIDMAP), with the view volume
//! and eye point kept in VPAR as on the calculator.

use crate::alg::{Expr, op};
use crate::calc::{Calc, Error, Res};
use crate::obj::Obj;
use crate::plot::{Curve, Shape};
use crate::solve::EQ;

const VPAR: &str = "VPAR";

/// VPAR: { xleft xright ynear yfar zlow zhigh xmin xmax ymin ymax xeye yeye
/// zeye xstep ystep }: the view volume, the input plane of GRIDMAP and
/// PARSURFACE, the eye point and the number of grid lines.
pub struct Vpar {
    pub x: (f64, f64),
    pub y: (f64, f64),
    pub z: (f64, f64),
    pub xx: (f64, f64),
    pub yy: (f64, f64),
    pub eye: (f64, f64, f64),
    pub steps: (f64, f64),
}

impl Default for Vpar {
    fn default() -> Self {
        Vpar {
            x: (-1.0, 1.0),
            y: (-1.0, 1.0),
            z: (-1.0, 1.0),
            xx: (-1.0, 1.0),
            yy: (-1.0, 1.0),
            eye: (0.0, -3.0, 0.0),
            steps: (10.0, 8.0),
        }
    }
}

/// The points where a grid of values crosses zero, by marching squares:
/// segments between the crossings in each cell. `v[i][j]` is at
/// (xs[i], ys[j]).
fn contour(xs: &[f64], ys: &[f64], v: &[Vec<f64>], out: &mut Vec<Shape>) {
    let cross = |a: (f64, f64, f64), b: (f64, f64, f64)| {
        let t = a.2 / (a.2 - b.2);
        (a.0 + t * (b.0 - a.0), a.1 + t * (b.1 - a.1))
    };
    for i in 0..xs.len() - 1 {
        for j in 0..ys.len() - 1 {
            let c = [
                (xs[i], ys[j], v[i][j]),
                (xs[i + 1], ys[j], v[i + 1][j]),
                (xs[i + 1], ys[j + 1], v[i + 1][j + 1]),
                (xs[i], ys[j + 1], v[i][j + 1]),
            ];
            if c.iter().any(|p| !p.2.is_finite()) {
                continue;
            }
            let pts: Curve = (0..4)
                .filter_map(|k| {
                    let (a, b) = (c[k], c[(k + 1) % 4]);
                    ((a.2 < 0.0) != (b.2 < 0.0)).then(|| cross(a, b))
                })
                .collect();
            for pair in pts.chunks(2) {
                if let [p, q] = pair {
                    out.push(Shape::Curve(vec![*p, *q]));
                }
            }
        }
    }
}

fn steps(lo: f64, hi: f64, n: usize) -> Vec<f64> {
    (0..=n)
        .map(|i| lo + (hi - lo) * i as f64 / n as f64)
        .collect()
}

/// Where a point of the view volume appears, seen from the eye point,
/// in the plot's x and y ranges.
fn project(
    v: &Vpar,
    (x0, x1): (f64, f64),
    (y0, y1): (f64, f64),
) -> impl Fn((f64, f64, f64)) -> Option<(f64, f64)> {
    let norm = |p: f64, (lo, hi): (f64, f64)| 2.0 * (p - lo) / (hi - lo) - 1.0;
    let (vx, vy, vz, eye) = (v.x, v.y, v.z, v.eye);
    let persp = move |(px, py, pz): (f64, f64, f64)| {
        let (px, py, pz) = (norm(px, vx), norm(py, vy), norm(pz, vz));
        let d = py - eye.1;
        (d > 1e-9).then(|| ((px - eye.0) / d, (pz - eye.2) / d))
    };
    // The volume's corners fill the plot.
    let mut bx = (f64::MAX, f64::MIN);
    let mut by = (f64::MAX, f64::MIN);
    for &a in &[vx.0, vx.1] {
        for &b in &[vy.0, vy.1] {
            for &c in &[vz.0, vz.1] {
                if let Some((sx, sy)) = persp((a, b, c)) {
                    bx = (bx.0.min(sx), bx.1.max(sx));
                    by = (by.0.min(sy), by.1.max(sy));
                }
            }
        }
    }
    move |p| {
        let (sx, sy) = persp(p)?;
        Some((
            x0 + (sx - bx.0) / (bx.1 - bx.0) * (x1 - x0),
            y0 + (sy - by.0) / (by.1 - by.0) * (y1 - y0),
        ))
    }
}

impl Calc {
    pub(crate) fn vpar(&self) -> Vpar {
        let mut v = Vpar::default();
        if let Some(Obj::List(l)) = self.global(VPAR) {
            let r: Vec<f64> = l
                .iter()
                .map(|o| match o {
                    Obj::Real(x) => *x,
                    _ => f64::NAN,
                })
                .collect();
            if r.len() == 15 && r.iter().all(|x| x.is_finite()) {
                v = Vpar {
                    x: (r[0], r[1]),
                    y: (r[2], r[3]),
                    z: (r[4], r[5]),
                    xx: (r[6], r[7]),
                    yy: (r[8], r[9]),
                    eye: (r[10], r[11], r[12]),
                    steps: (r[13], r[14]),
                };
            }
        }
        v
    }

    fn set_vpar(&mut self, v: &Vpar) -> Res<()> {
        let r = [
            v.x.0, v.x.1, v.y.0, v.y.1, v.z.0, v.z.1, v.xx.0, v.xx.1, v.yy.0, v.yy.1, v.eye.0,
            v.eye.1, v.eye.2, v.steps.0, v.steps.1,
        ];
        self.store(
            VPAR.into(),
            Obj::List(r.into_iter().map(Obj::Real).collect()),
        )
    }

    /// EQ as one expression: an equation's sides subtracted, for the plots
    /// of where it holds.
    fn eq_expr(&self) -> Res<Obj> {
        Ok(match self.global(EQ).ok_or(Error::Undefined)? {
            Obj::Alg(Expr::Op('=', l, r)) => Obj::Alg(op('-', (**l).clone(), (**r).clone())),
            Obj::List(l) => l.first().cloned().ok_or(Error::BadValue)?,
            o => o.clone(),
        })
    }

    /// f at (x, y), with the independent and dependent variables set to
    /// them; None where it is undefined.
    fn at2(&mut self, f: &Obj, names: (&str, &str), x: f64, y: f64) -> Res<Option<Obj>> {
        self.stack.push(Obj::Real(x));
        self.stack.push(Obj::Real(y));
        let mut out = None;
        self.with_locals(&[names.0.to_string(), names.1.to_string()], |c| {
            c.stack.push(f.clone());
            out = Some(c.eval_word("→NUM").and_then(|_| c.pop()));
            Ok(())
        })?;
        match out.unwrap() {
            Ok(o) => Ok(Some(o)),
            Err(e @ (Error::Interrupted | Error::Killed)) => Err(e),
            Err(_) => Ok(None),
        }
    }

    fn real2(&mut self, f: &Obj, names: (&str, &str), x: f64, y: f64) -> Res<f64> {
        Ok(match self.at2(f, names, x, y)? {
            Some(Obj::Real(v)) => v,
            _ => f64::NAN,
        })
    }

    /// The shapes of one of these plot types, over the plot ranges x and y.
    pub(crate) fn trace_more(
        &mut self,
        kind: &str,
        names: (&str, &str),
        x: (f64, f64),
        y: (f64, f64),
        trange: Option<(f64, f64)>,
    ) -> Res<Vec<Shape>> {
        let mut out = vec![];
        let v = self.vpar();
        let to_screen = |p: (f64, f64)| {
            (
                x.0 + (p.0 - v.x.0) / (v.x.1 - v.x.0) * (x.1 - x.0),
                y.0 + (p.1 - v.y.0) / (v.y.1 - v.y.0) * (y.1 - y.0),
            )
        };
        match kind {
            // Where EQ, an equation in both variables, holds.
            "CONIC" => {
                let f = self.eq_expr()?;
                let (xs, ys) = (steps(x.0, x.1, 130), steps(y.0, y.1, 63));
                let mut vals = vec![];
                for &a in &xs {
                    vals.push(
                        ys.iter()
                            .map(|&b| self.real2(&f, names, a, b))
                            .collect::<Res<Vec<_>>>()?,
                    );
                }
                contour(&xs, &ys, &vals, &mut out);
            }
            // The solution of y' = EQ through the dependent variable's value
            // at the start of the independent variable's range.
            "DIFFEQ" => {
                let f = self.eq_expr()?;
                let (a, b) = trange.unwrap_or(x);
                let mut yv = match self.global(names.1) {
                    Some(Obj::Real(y0)) => *y0,
                    _ => 0.0,
                };
                let n = 360;
                let h = (b - a) / n as f64;
                let mut curve = vec![(a, yv)];
                for i in 0..n {
                    let t = a + i as f64 * h;
                    let k1 = self.real2(&f, names, t, yv)?;
                    let k2 = self.real2(&f, names, t + h / 2.0, yv + h * k1 / 2.0)?;
                    let k3 = self.real2(&f, names, t + h / 2.0, yv + h * k2 / 2.0)?;
                    let k4 = self.real2(&f, names, t + h, yv + h * k3)?;
                    yv += h * (k1 + 2.0 * k2 + 2.0 * k3 + k4) / 6.0;
                    if !yv.is_finite() {
                        break;
                    }
                    curve.push((t + h, yv));
                }
                out.push(Shape::Curve(curve));
            }
            // A short line of slope EQ at each point of a grid over the view
            // volume's x and y.
            "SLOPEFIELD" => {
                let f = self.eq_expr()?;
                let (nx, ny) = (v.steps.0.max(1.0) as usize, v.steps.1.max(1.0) as usize);
                let (dx, dy) = ((v.x.1 - v.x.0) / nx as f64, (v.y.1 - v.y.0) / ny as f64);
                for a in steps(v.x.0, v.x.1, nx) {
                    for b in steps(v.y.0, v.y.1, ny) {
                        let m = self.real2(&f, names, a, b)?;
                        if !m.is_finite() {
                            continue;
                        }
                        // As long as a third of a cell, in screen proportion.
                        let (ux, uy) = (dx, m * dx);
                        let len = ((ux / dx).powi(2) + (uy / dy).powi(2)).sqrt() * 3.0;
                        let (ux, uy) = (ux / len, uy / len);
                        out.push(Shape::Curve(vec![
                            to_screen((a - ux, b - uy)),
                            to_screen((a + ux, b + uy)),
                        ]));
                    }
                }
            }
            // z = EQ over the view volume, as a grid seen in perspective,
            // or only its lines along x for YSLICE.
            "WIREFRAME" | "YSLICE" => {
                let f = self.eq_expr()?;
                let proj = project(&v, x, y);
                let (nx, ny) = (v.steps.0.max(1.0) as usize, v.steps.1.max(1.0) as usize);
                let (xs, ys) = (steps(v.x.0, v.x.1, nx * 4), steps(v.y.0, v.y.1, ny * 4));
                let mut z = vec![];
                for &a in &xs {
                    z.push(
                        ys.iter()
                            .map(|&b| self.real2(&f, names, a, b))
                            .collect::<Res<Vec<_>>>()?,
                    );
                }
                let line = |pts: Vec<(f64, f64, f64)>| -> Vec<Shape> {
                    let mut out = vec![];
                    let mut seg = vec![];
                    for p in pts {
                        match (p.2.is_finite()).then(|| proj(p)).flatten() {
                            Some(q) => seg.push(q),
                            None if !seg.is_empty() => {
                                out.push(Shape::Curve(std::mem::take(&mut seg)))
                            }
                            None => {}
                        }
                    }
                    if !seg.is_empty() {
                        out.push(Shape::Curve(seg));
                    }
                    out
                };
                for j in (0..ys.len()).step_by(4) {
                    out.extend(line(
                        xs.iter()
                            .enumerate()
                            .map(|(i, &a)| (a, ys[j], z[i][j]))
                            .collect(),
                    ));
                }
                if kind == "WIREFRAME" {
                    for i in (0..xs.len()).step_by(4) {
                        out.extend(line(
                            ys.iter()
                                .enumerate()
                                .map(|(j, &b)| (xs[i], b, z[i][j]))
                                .collect(),
                        ));
                    }
                }
            }
            // Contour lines of z = EQ at levels across ZVOL, over the view
            // volume's x and y.
            "PCONTOUR" => {
                let f = self.eq_expr()?;
                let (xs, ys) = (steps(v.x.0, v.x.1, 60), steps(v.y.0, v.y.1, 40));
                let mut z = vec![];
                for &a in &xs {
                    z.push(
                        ys.iter()
                            .map(|&b| self.real2(&f, names, a, b))
                            .collect::<Res<Vec<_>>>()?,
                    );
                }
                let levels = v.steps.0.max(1.0) as usize;
                let mut shapes = vec![];
                for k in 0..levels {
                    let lv = v.z.0 + (v.z.1 - v.z.0) * (k as f64 + 0.5) / levels as f64;
                    let shifted: Vec<Vec<f64>> = z
                        .iter()
                        .map(|r| r.iter().map(|w| w - lv).collect())
                        .collect();
                    contour(&xs, &ys, &shifted, &mut shapes);
                }
                for s in shapes {
                    if let Shape::Curve(c) = s {
                        out.push(Shape::Curve(c.into_iter().map(to_screen).collect()));
                    }
                }
            }
            // The images under EQ, a complex function of X+iY, of a grid of
            // lines over the input plane XXRNG × YYRNG.
            "GRIDMAP" => {
                let f = self.eq_expr()?;
                let (nx, ny) = (v.steps.0.max(1.0) as usize, v.steps.1.max(1.0) as usize);
                let (xs, ys) = (steps(v.xx.0, v.xx.1, nx), steps(v.yy.0, v.yy.1, ny));
                let fine = |lo: f64, hi: f64| steps(lo, hi, 60);
                let map = |c: &mut Calc, a: f64, b: f64| -> Res<Option<(f64, f64)>> {
                    Ok(match c.at2(&f, names, a, b)? {
                        Some(Obj::Complex(re, im)) => Some((re, im)),
                        Some(Obj::Real(re)) => Some((re, 0.0)),
                        _ => None,
                    })
                };
                for &a in &xs {
                    let pts = fine(v.yy.0, v.yy.1)
                        .into_iter()
                        .map(|b| map(self, a, b))
                        .collect::<Res<Vec<_>>>()?;
                    out.push(Shape::Curve(pts.into_iter().flatten().collect()));
                }
                for &b in &ys {
                    let pts = fine(v.xx.0, v.xx.1)
                        .into_iter()
                        .map(|a| map(self, a, b))
                        .collect::<Res<Vec<_>>>()?;
                    out.push(Shape::Curve(pts.into_iter().flatten().collect()));
                }
            }
            // A surface { x y z } of two parameters over the input plane,
            // as a grid seen in perspective.
            "PARSURFACE" => {
                let f = match self.global(EQ).ok_or(Error::Undefined)? {
                    Obj::List(l) if l.len() == 3 => l.clone(),
                    _ => return Err(Error::BadValue),
                };
                let proj = project(&v, x, y);
                let (nx, ny) = (v.steps.0.max(1.0) as usize, v.steps.1.max(1.0) as usize);
                let point = |c: &mut Calc, a: f64, b: f64| -> Res<Option<(f64, f64)>> {
                    let mut p = [0.0; 3];
                    for (k, e) in f.iter().enumerate() {
                        p[k] = c.real2(e, names, a, b)?;
                    }
                    Ok(p.iter()
                        .all(|w| w.is_finite())
                        .then(|| proj((p[0], p[1], p[2])))
                        .flatten())
                };
                for a in steps(v.xx.0, v.xx.1, nx) {
                    let pts = steps(v.yy.0, v.yy.1, 40)
                        .into_iter()
                        .map(|b| point(self, a, b))
                        .collect::<Res<Vec<_>>>()?;
                    out.push(Shape::Curve(pts.into_iter().flatten().collect()));
                }
                for b in steps(v.yy.0, v.yy.1, ny) {
                    let pts = steps(v.xx.0, v.xx.1, 40)
                        .into_iter()
                        .map(|a| point(self, a, b))
                        .collect::<Res<Vec<_>>>()?;
                    out.push(Shape::Curve(pts.into_iter().flatten().collect()));
                }
            }
            _ => return Err(Error::BadValue),
        }
        out.retain(|s| !matches!(s, Shape::Curve(c) if c.is_empty()));
        Ok(out)
    }

    pub(crate) fn plot3d_cmd(&mut self, w: &str) -> Option<Res<()>> {
        let pair = |c: &Calc| -> Res<(f64, f64)> {
            c.need(2)?;
            match (c.peek(2), c.peek(1)) {
                (Obj::Real(a), Obj::Real(b)) if a < b => Ok((*a, *b)),
                (Obj::Real(_), Obj::Real(_)) => Err(Error::BadValue),
                _ => Err(Error::BadType),
            }
        };
        Some(match w {
            "XVOL" | "YVOL" | "ZVOL" | "XXRNG" | "YYRNG" => pair(self).and_then(|r| {
                let mut v = self.vpar();
                match w {
                    "XVOL" => v.x = r,
                    "YVOL" => v.y = r,
                    "ZVOL" => v.z = r,
                    "XXRNG" => v.xx = r,
                    _ => v.yy = r,
                }
                self.set_vpar(&v)?;
                self.stack.truncate(self.stack.len() - 2);
                Ok(())
            }),
            // x y z EYEPT, in the view volume's coordinates scaled to ±1.
            "EYEPT" => self.need(3).and_then(|_| {
                let (Obj::Real(a), Obj::Real(b), Obj::Real(c)) =
                    (self.peek(3), self.peek(2), self.peek(1))
                else {
                    return Err(Error::BadType);
                };
                let mut v = self.vpar();
                v.eye = (*a, *b, *c);
                if v.eye.1 >= -1.0 {
                    return Err(Error::BadValue);
                }
                self.set_vpar(&v)?;
                self.stack.truncate(self.stack.len() - 3);
                Ok(())
            }),
            "NUMX" | "NUMY" => self.need(1).and_then(|_| {
                let Obj::Real(n) = *self.peek(1) else {
                    return Err(Error::BadType);
                };
                if n < 1.0 || n.fract() != 0.0 {
                    return Err(Error::BadValue);
                }
                let mut v = self.vpar();
                if w == "NUMX" {
                    v.steps.0 = n;
                } else {
                    v.steps.1 = n;
                }
                self.set_vpar(&v)?;
                self.stack.pop();
                Ok(())
            }),
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::calc::Calc;
    use crate::plot::Shape;

    fn shapes(src: &str) -> usize {
        let mut c = Calc::new();
        c.exec_line(src).unwrap();
        c.pict
            .iter()
            .filter(|s| matches!(s, Shape::Curve(_)))
            .count()
    }

    #[test]
    fn plot_types() {
        assert!(shapes("'X^2+Y^2=4' STEQ CONIC DRAW") > 50);
        let mut c = Calc::new();
        c.exec_line("'Y' STEQ DIFFEQ 1 'Y' STO { X 0 1 } INDEP DRAW")
            .unwrap();
        let Shape::Curve(curve) = &c.pict[0] else {
            panic!()
        };
        assert!((curve.last().unwrap().1 - std::f64::consts::E).abs() < 1e-6);
        assert_eq!(shapes("'X*Y' STEQ SLOPEFIELD DRAW"), 11 * 9);
        assert_eq!(shapes("'X^2-Y^2' STEQ WIREFRAME DRAW"), 9 + 11);
        assert_eq!(shapes("'X^2-Y^2' STEQ YSLICE DRAW"), 9);
        assert!(shapes("'X^2+Y^2' STEQ 0 1 ZVOL PCONTOUR DRAW") > 20);
        assert_eq!(shapes("'(X+i*Y)^2' STEQ GRIDMAP DRAW"), 11 + 9);
        assert_eq!(shapes("{ 'X' 'Y' 'X*Y' } STEQ PARSURFACE DRAW"), 11 + 9);
    }

    #[test]
    fn view_volume() {
        let mut c = Calc::new();
        c.exec_line("0 2 XVOL 0 -5 1 EYEPT 4 NUMX VPAR").unwrap();
        assert_eq!(
            c.stack[0].show(c.fmt),
            "{ 0 2 -1 1 -1 1 -1 1 -1 1 0 -5 1 4 8 }"
        );
    }
}

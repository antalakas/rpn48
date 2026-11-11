//! Statistics on the data matrix ΣDAT, one observation per row, with the
//! columns and fitted line kept in ΣPAR as on the calculator.

use crate::calc::{Calc, Error, Res};
use crate::obj::{Obj, r12};

const DAT: &str = "ΣDAT";
const PAR: &str = "ΣPAR";

/// ΣPAR: { x-column y-column intercept slope model }.
struct Par {
    x: usize,
    y: usize,
    intercept: f64,
    slope: f64,
}

impl Calc {
    fn data(&self) -> Res<Vec<Vec<f64>>> {
        match self.global(DAT) {
            Some(Obj::Mat(m)) => Ok(m.clone()),
            Some(Obj::Vect(v)) => Ok(v.iter().map(|&x| vec![x]).collect()),
            Some(_) => Err(Error::BadType),
            None => Err(Error::Undefined),
        }
    }

    fn par(&self) -> Par {
        let mut p = Par {
            x: 1,
            y: 2,
            intercept: 0.0,
            slope: 0.0,
        };
        if let Some(Obj::List(l)) = self.global(PAR) {
            let r = |i: usize| match l.get(i) {
                Some(Obj::Real(x)) => Some(*x),
                _ => None,
            };
            p.x = r(0).map_or(1, |x| x as usize);
            p.y = r(1).map_or(2, |x| x as usize);
            p.intercept = r(2).unwrap_or(0.0);
            p.slope = r(3).unwrap_or(0.0);
        }
        p
    }

    fn set_par(&mut self, p: &Par) -> Res<()> {
        let l = vec![
            Obj::Real(p.x as f64),
            Obj::Real(p.y as f64),
            Obj::Real(p.intercept),
            Obj::Real(p.slope),
            Obj::Name("LIN".into()),
        ];
        self.store(PAR.into(), Obj::List(l))
    }

    fn column(&self, m: &[Vec<f64>], c: usize) -> Res<Vec<f64>> {
        if c == 0 || c > m[0].len() {
            return Err(Error::BadValue);
        }
        Ok(m.iter().map(|r| r[c - 1]).collect())
    }

    /// The x and y columns, for the two-variable statistics.
    pub(crate) fn xy(&self) -> Res<(Vec<f64>, Vec<f64>)> {
        let (m, p) = (self.data()?, self.par());
        Ok((self.column(&m, p.x)?, self.column(&m, p.y)?))
    }

    fn push_reals(&mut self, v: Vec<f64>) {
        let v: Vec<f64> = v.into_iter().map(r12).collect();
        self.stack.push(match &v[..] {
            [x] => Obj::Real(*x),
            _ => Obj::Vect(v),
        });
    }

    // A statistic of each column, a real for one column and a vector for more.
    fn per_column(&mut self, min_rows: usize, f: fn(&[f64]) -> f64) -> Res<()> {
        let m = self.data()?;
        if m.len() < min_rows {
            return Err(Error::BadValue);
        }
        let v = (0..m[0].len())
            .map(|j| f(&m.iter().map(|r| r[j]).collect::<Vec<_>>()))
            .collect();
        self.push_reals(v);
        Ok(())
    }

    pub(crate) fn stat_cmd(&mut self, w: &str) -> Option<Res<()>> {
        Some(match w {
            "Σ+" => self.need(1).and_then(|_| {
                let rows = match self.peek(1) {
                    Obj::Real(x) => vec![vec![*x]],
                    Obj::Vect(v) => vec![v.clone()],
                    Obj::Mat(m) => m.clone(),
                    _ => return Err(Error::BadType),
                };
                let mut m = match self.data() {
                    Err(Error::Undefined) => vec![],
                    r => r?,
                };
                if m.first().is_some_and(|r| r.len() != rows[0].len()) {
                    return Err(Error::Dim);
                }
                m.extend(rows);
                self.store(DAT.into(), Obj::Mat(m))?;
                self.stack.pop();
                Ok(())
            }),
            "Σ-" => self.data().and_then(|mut m| {
                let r = m.pop().ok_or(Error::BadValue)?;
                if m.is_empty() {
                    self.cur_vars_mut().retain(|(n, _)| n != DAT);
                } else {
                    self.store(DAT.into(), Obj::Mat(m))?;
                }
                self.push_reals(r);
                Ok(())
            }),
            "CLΣ" => {
                self.cur_vars_mut().retain(|(n, _)| n != DAT);
                Ok(())
            }
            "NΣ" => self
                .data()
                .map(|m| self.stack.push(Obj::Real(m.len() as f64))),
            "TOT" => self.per_column(1, |c| c.iter().sum()),
            "MEAN" => self.per_column(1, mean),
            "VAR" => self.per_column(2, var),
            "SDEV" => self.per_column(2, |c| var(c).sqrt()),
            "MAXΣ" => self.per_column(1, |c| c.iter().copied().fold(f64::MIN, f64::max)),
            "MINΣ" => self.per_column(1, |c| c.iter().copied().fold(f64::MAX, f64::min)),
            "XCOL" | "YCOL" => self.need(1).and_then(|_| {
                let Obj::Real(n) = *self.peek(1) else {
                    return Err(Error::BadType);
                };
                if n < 1.0 || n.fract() != 0.0 {
                    return Err(Error::BadValue);
                }
                let mut p = self.par();
                if w == "XCOL" {
                    p.x = n as usize;
                } else {
                    p.y = n as usize;
                }
                self.set_par(&p)?;
                self.stack.pop();
                Ok(())
            }),
            "ΣX" | "ΣY" | "ΣX2" | "ΣY2" | "ΣXY" => self.xy().map(|(x, y)| {
                let s: f64 = match w {
                    "ΣX" => x.iter().sum(),
                    "ΣY" => y.iter().sum(),
                    "ΣX2" => x.iter().map(|a| a * a).sum(),
                    "ΣY2" => y.iter().map(|a| a * a).sum(),
                    _ => x.iter().zip(&y).map(|(a, b)| a * b).sum(),
                };
                self.stack.push(Obj::Real(r12(s)));
            }),
            "COV" | "CORR" => self.xy().and_then(|(x, y)| {
                if x.len() < 2 {
                    return Err(Error::BadValue);
                }
                let c = cov(&x, &y);
                let r = if w == "COV" {
                    c
                } else {
                    c / (var(&x) * var(&y)).sqrt()
                };
                if !r.is_finite() {
                    return Err(Error::UndefinedResult);
                }
                self.stack.push(Obj::Real(r12(r)));
                Ok(())
            }),
            // Least squares line through the x and y columns.
            "LR" => self.xy().and_then(|(x, y)| {
                if x.len() < 2 || var(&x) == 0.0 {
                    return Err(Error::BadValue);
                }
                let slope = cov(&x, &y) / var(&x);
                let intercept = mean(&y) - slope * mean(&x);
                let mut p = self.par();
                (p.intercept, p.slope) = (r12(intercept), r12(slope));
                self.set_par(&p)?;
                let tag = |t: &str, x| Obj::Tag(t.into(), Box::new(Obj::Real(x)));
                self.stack.push(tag("Intercept", p.intercept));
                self.stack.push(tag("Slope", p.slope));
                Ok(())
            }),
            "PREDY" | "PREDX" => self.need(1).and_then(|_| {
                let Obj::Real(v) = *self.peek(1) else {
                    return Err(Error::BadType);
                };
                let p = self.par();
                let r = if w == "PREDY" {
                    p.intercept + p.slope * v
                } else if p.slope == 0.0 {
                    return Err(Error::Infinite);
                } else {
                    (v - p.intercept) / p.slope
                };
                self.stack.pop();
                self.stack.push(Obj::Real(r12(r)));
                Ok(())
            }),
            _ => return None,
        })
    }
}

fn mean(c: &[f64]) -> f64 {
    c.iter().sum::<f64>() / c.len() as f64
}

/// Sample variance, dividing by n-1 as the calculator does.
fn var(c: &[f64]) -> f64 {
    cov(c, c)
}

fn cov(x: &[f64], y: &[f64]) -> f64 {
    let (mx, my) = (mean(x), mean(y));
    x.iter()
        .zip(y)
        .map(|(a, b)| (a - mx) * (b - my))
        .sum::<f64>()
        / (x.len() - 1) as f64
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
    fn statistics() {
        assert_eq!(
            run("1 Σ+ 2 Σ+ 3 Σ+ 4 Σ+ MEAN SDEV NΣ"),
            ["2.5", "1.29099444874", "4"]
        );
        assert_eq!(
            run("[1 3] Σ+ [2 5] Σ+ [3 7] Σ+ LR 10 PREDY"),
            [":Intercept:1", ":Slope:2", "21"]
        );
        assert_eq!(run("[1 3] Σ+ [2 5] Σ+ TOT CORR"), ["[ 3 8 ]", "1"]);
        assert_eq!(run("5 Σ+ 6 Σ+ Σ- NΣ"), ["6", "1"]);
    }
}

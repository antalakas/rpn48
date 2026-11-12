//! Statistics on the data matrix ΣDAT, one observation per row, with the
//! columns and fitted line kept in ΣPAR as on the calculator.

use crate::calc::{Calc, Error, OpFn, Res, one, op, real, real_of};
use crate::obj::{Fmt, Obj, r12};

const DAT: &str = "ΣDAT";
const PAR: &str = "ΣPAR";

/// ΣPAR: { x-column y-column intercept slope model }.
struct Par {
    x: usize,
    y: usize,
    intercept: f64,
    slope: f64,
    model: Model,
}

/// The curve LR fits, each a straight line through transformed data.
#[derive(Clone, Copy, PartialEq)]
enum Model {
    /// y = b + m·x
    Lin,
    /// y = b + m·ln x
    Log,
    /// y = b·e^(m·x)
    Exp,
    /// y = b·x^m
    Pwr,
}

impl Model {
    const ALL: [Model; 4] = [Model::Lin, Model::Log, Model::Exp, Model::Pwr];

    fn name(self) -> &'static str {
        match self {
            Model::Lin => "LIN",
            Model::Log => "LOG",
            Model::Exp => "EXP",
            Model::Pwr => "PWR",
        }
    }

    /// The data as the straight line sees it: ln x for LOG and PWR, ln y
    /// for EXP and PWR.
    fn transform(self, x: &[f64], y: &[f64]) -> Res<(Vec<f64>, Vec<f64>)> {
        let ln = |v: &[f64]| -> Res<Vec<f64>> {
            v.iter()
                .map(|&a| {
                    if a > 0.0 {
                        Ok(a.ln())
                    } else {
                        Err(Error::BadValue)
                    }
                })
                .collect()
        };
        Ok(match self {
            Model::Lin => (x.to_vec(), y.to_vec()),
            Model::Log => (ln(x)?, y.to_vec()),
            Model::Exp => (x.to_vec(), ln(y)?),
            Model::Pwr => (ln(x)?, ln(y)?),
        })
    }
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
            model: Model::Lin,
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
            if let Some(Obj::Name(n)) = l.get(4) {
                p.model = Model::ALL
                    .into_iter()
                    .find(|m| m.name() == n)
                    .unwrap_or(Model::Lin);
            }
        }
        p
    }

    fn set_par(&mut self, p: &Par) -> Res<()> {
        let l = vec![
            Obj::Real(p.x as f64),
            Obj::Real(p.y as f64),
            Obj::Real(p.intercept),
            Obj::Real(p.slope),
            Obj::Name(p.model.name().into()),
        ];
        self.store(PAR.into(), Obj::List(l))
    }

    fn column(&self, m: &[Vec<f64>], c: usize) -> Res<Vec<f64>> {
        if c == 0 || c > m[0].len() {
            return Err(Error::BadValue);
        }
        Ok(m.iter().map(|r| r[c - 1]).collect())
    }

    /// The x column alone, for bar plots and histograms.
    pub(crate) fn x_column(&self) -> Res<Vec<f64>> {
        let (m, p) = (self.data()?, self.par());
        self.column(&m, p.x)
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
            // Least squares fit of the current model to the x and y columns.
            "LR" => self.xy().and_then(|(x, y)| {
                let mut p = self.par();
                let (b, m) = fit(p.model, &x, &y)?;
                (p.intercept, p.slope) = (r12(b), r12(m));
                self.set_par(&p)?;
                let tag = |t: &str, x| Obj::Tag(t.into(), Box::new(Obj::Real(x)));
                self.stack.push(tag("Intercept", p.intercept));
                self.stack.push(tag("Slope", p.slope));
                Ok(())
            }),
            "LINFIT" | "LOGFIT" | "EXPFIT" | "PWRFIT" => {
                let mut p = self.par();
                p.model = Model::ALL
                    .into_iter()
                    .find(|m| w.starts_with(m.name()))
                    .unwrap();
                self.set_par(&p)
            }
            // The model whose straight line correlates best with the data.
            "BESTFIT" => self.xy().and_then(|(x, y)| {
                let mut best = None;
                for m in Model::ALL {
                    let Ok((tx, ty)) = m.transform(&x, &y) else {
                        continue;
                    };
                    let r = (cov(&tx, &ty) / (var(&tx) * var(&ty)).sqrt()).abs();
                    if r.is_finite() && best.is_none_or(|(_, b)| r > b + 1e-12) {
                        best = Some((m, r));
                    }
                }
                let mut p = self.par();
                p.model = best.ok_or(Error::BadValue)?.0;
                self.set_par(&p)?;
                self.eval_word("LR")
            }),
            // The fitted model as an expression in X.
            "ΣLINE" => {
                let p = self.par();
                let (b, m) = (Obj::Real(p.intercept), Obj::Real(p.slope));
                let (b, m) = (b.show(Fmt::Std), m.show(Fmt::Std));
                let src = match p.model {
                    Model::Lin => format!("{b} {m} 'X' * +"),
                    Model::Log => format!("{b} {m} 'X' LN * +"),
                    Model::Exp => format!("{b} {m} 'X' * EXP *"),
                    Model::Pwr => format!("{b} 'X' {m} ^ *"),
                };
                self.exec_line(&src)
            }
            "PVAR" => self.per_column(1, pvar),
            "PSDEV" => self.per_column(1, |c| pvar(c).sqrt()),
            "PCOV" => self.xy().and_then(|(x, y)| {
                let n = x.len() as f64;
                let r = cov(&x, &y) * (n - 1.0) / n;
                if !r.is_finite() {
                    return Err(Error::UndefinedResult);
                }
                self.stack.push(Obj::Real(r12(r)));
                Ok(())
            }),
            "STOΣ" => self.need(1).and_then(|_| {
                let m = match self.peek(1) {
                    Obj::Mat(m) => m.clone(),
                    Obj::Vect(v) => v.iter().map(|&x| vec![x]).collect(),
                    Obj::Name(_) => {
                        let o = self.peek(1).clone();
                        self.store(DAT.into(), o)?;
                        self.stack.pop();
                        return Ok(());
                    }
                    _ => return Err(Error::BadType),
                };
                self.store(DAT.into(), Obj::Mat(m))?;
                self.stack.pop();
                Ok(())
            }),
            "RCLΣ" => {
                let o = self.global(DAT).cloned().ok_or(Error::Undefined);
                o.map(|o| self.stack.push(o))
            }
            // x-column y-column COLΣ sets both at once.
            "COLΣ" => self.need(2).and_then(|_| {
                let (Obj::Real(x), Obj::Real(y)) = (self.peek(2), self.peek(1)) else {
                    return Err(Error::BadType);
                };
                let (x, y) = (*x, *y);
                if [x, y].iter().any(|&n| n < 1.0 || n.fract() != 0.0) {
                    return Err(Error::BadValue);
                }
                let mut p = self.par();
                (p.x, p.y) = (x as usize, y as usize);
                self.set_par(&p)?;
                self.stack.truncate(self.stack.len() - 2);
                Ok(())
            }),
            // xmin width n BINS: how many of the x column fall in each of n
            // bins, and how many fell below and above them all.
            "BINS" => self.need(3).and_then(|_| {
                let n = self.stack.len();
                let (Obj::Real(lo), Obj::Real(wd), Obj::Real(k)) =
                    (&self.stack[n - 3], &self.stack[n - 2], &self.stack[n - 1])
                else {
                    return Err(Error::BadType);
                };
                let (lo, wd, k) = (*lo, *wd, *k);
                if wd <= 0.0 || k < 1.0 || k.fract() != 0.0 {
                    return Err(Error::BadValue);
                }
                let xs = self.x_column()?;
                let mut counts = vec![vec![0.0]; k as usize];
                let (mut below, mut above) = (0.0, 0.0);
                for v in xs {
                    let b = ((v - lo) / wd).floor();
                    if b < 0.0 {
                        below += 1.0;
                    } else if b as usize >= counts.len() {
                        above += 1.0;
                    } else {
                        counts[b as usize][0] += 1.0;
                    }
                }
                self.stack.truncate(n - 3);
                self.stack.push(Obj::Mat(counts));
                self.stack.push(Obj::Vect(vec![below, above]));
                Ok(())
            }),
            "PREDY" | "PREDX" => self.need(1).and_then(|_| {
                let Obj::Real(v) = *self.peek(1) else {
                    return Err(Error::BadType);
                };
                let p = self.par();
                let (b, m) = (p.intercept, p.slope);
                let r = if w == "PREDY" {
                    match p.model {
                        Model::Lin => b + m * v,
                        Model::Log => b + m * v.ln(),
                        Model::Exp => b * (m * v).exp(),
                        Model::Pwr => b * v.powf(m),
                    }
                } else if m == 0.0 {
                    return Err(Error::Infinite);
                } else {
                    match p.model {
                        Model::Lin => (v - b) / m,
                        Model::Log => ((v - b) / m).exp(),
                        Model::Exp => (v / b).ln() / m,
                        Model::Pwr => (v / b).powf(1.0 / m),
                    }
                };
                if !r.is_finite() {
                    return Err(Error::BadValue);
                }
                self.stack.pop();
                self.stack.push(Obj::Real(r12(r)));
                Ok(())
            }),
            _ => return None,
        })
    }
}

/// Intercept and slope of `model` fitted to x and y; the intercept is the
/// multiplier for EXP and PWR.
fn fit(model: Model, x: &[f64], y: &[f64]) -> Res<(f64, f64)> {
    let (tx, ty) = model.transform(x, y)?;
    if tx.len() < 2 || var(&tx) == 0.0 {
        return Err(Error::BadValue);
    }
    let slope = cov(&tx, &ty) / var(&tx);
    let b = mean(&ty) - slope * mean(&tx);
    Ok(match model {
        Model::Lin | Model::Log => (b, slope),
        Model::Exp | Model::Pwr => (b.exp(), slope),
    })
}

/// Population variance, dividing by n.
fn pvar(c: &[f64]) -> f64 {
    let m = mean(c);
    c.iter().map(|x| (x - m) * (x - m)).sum::<f64>() / c.len() as f64
}

fn ln_gamma(x: f64) -> f64 {
    // Lanczos, g = 7.
    const C: [f64; 9] = [
        0.999_999_999_999_809_9,
        676.520_368_121_885_1,
        -1_259.139_216_722_402_8,
        771.323_428_777_653_1,
        -176.615_029_162_140_6,
        12.507_343_278_686_905,
        -0.138_571_095_265_720_12,
        9.984_369_578_019_572e-6,
        1.505_632_735_149_311_6e-7,
    ];
    if x < 0.5 {
        return (std::f64::consts::PI / (std::f64::consts::PI * x).sin()).ln() - ln_gamma(1.0 - x);
    }
    let x = x - 1.0;
    let t = x + 7.5;
    let s = C[1..]
        .iter()
        .enumerate()
        .fold(C[0], |s, (i, c)| s + c / (x + i as f64 + 1.0));
    0.5 * (2.0 * std::f64::consts::PI).ln() + (x + 0.5) * t.ln() - t + s.ln()
}

/// The regularized upper incomplete gamma function Q(a, x).
fn gamma_q(a: f64, x: f64) -> f64 {
    if x <= 0.0 {
        return 1.0;
    }
    let front = (-x + a * x.ln() - ln_gamma(a)).exp();
    if x < a + 1.0 {
        // The series for P.
        let (mut sum, mut term, mut ap) = (1.0 / a, 1.0 / a, a);
        for _ in 0..1000 {
            ap += 1.0;
            term *= x / ap;
            sum += term;
            if term.abs() < sum.abs() * 1e-16 {
                break;
            }
        }
        1.0 - sum * front
    } else {
        // Lentz's continued fraction for Q.
        let tiny = 1e-300;
        let mut b = x + 1.0 - a;
        let (mut c, mut d) = (1.0 / tiny, 1.0 / b);
        let mut h = d;
        for i in 1..1000 {
            let an = -(i as f64) * (i as f64 - a);
            b += 2.0;
            d = an * d + b;
            d = if d.abs() < tiny { tiny } else { d };
            c = b + an / c;
            c = if c.abs() < tiny { tiny } else { c };
            d = 1.0 / d;
            let del = d * c;
            h *= del;
            if (del - 1.0).abs() < 1e-16 {
                break;
            }
        }
        front * h
    }
}

/// The regularized incomplete beta function I_x(a, b).
fn beta_i(a: f64, b: f64, x: f64) -> f64 {
    if x <= 0.0 {
        return 0.0;
    }
    if x >= 1.0 {
        return 1.0;
    }
    let front =
        (ln_gamma(a + b) - ln_gamma(a) - ln_gamma(b) + a * x.ln() + b * (1.0 - x).ln()).exp();
    // The continued fraction converges fastest on this side.
    if x > (a + 1.0) / (a + b + 2.0) {
        return 1.0 - beta_i(b, a, 1.0 - x);
    }
    let tiny = 1e-300;
    let (mut c, mut d) = (1.0, 1.0 - (a + b) * x / (a + 1.0));
    d = if d.abs() < tiny { tiny } else { d };
    d = 1.0 / d;
    let mut h = d;
    for m in 1..1000 {
        let m = m as f64;
        for an in [
            m * (b - m) * x / ((a + 2.0 * m - 1.0) * (a + 2.0 * m)),
            -(a + m) * (a + b + m) * x / ((a + 2.0 * m) * (a + 2.0 * m + 1.0)),
        ] {
            d = 1.0 + an * d;
            d = if d.abs() < tiny { tiny } else { d };
            c = 1.0 + an / c;
            c = if c.abs() < tiny { tiny } else { c };
            d = 1.0 / d;
            h *= d * c;
        }
        if (d * c - 1.0).abs() < 1e-16 {
            break;
        }
    }
    front * h / a
}

fn dof(n: f64) -> Res<f64> {
    if n > 0.0 && n.fract() == 0.0 {
        Ok(n)
    } else {
        Err(Error::BadValue)
    }
}

/// The probability distributions: upper-tail probabilities and the normal
/// density.
pub fn pure(w: &str) -> Option<(usize, OpFn)> {
    match w {
        // mean variance x UTPN: the chance a normal variable exceeds x.
        "UTPN" => op(3, |_, a| {
            let (m, v, x) = (real_of(&a[0])?, real_of(&a[1])?, real_of(&a[2])?);
            if v <= 0.0 {
                return Err(Error::BadValue);
            }
            let z = (x - m) / (2.0 * v).sqrt();
            let erfc = |z: f64| gamma_q(0.5, z * z);
            one(real(if z >= 0.0 {
                erfc(z) / 2.0
            } else {
                1.0 - erfc(-z) / 2.0
            }))
        }),
        "NDIST" => op(3, |_, a| {
            let (m, v, x) = (real_of(&a[0])?, real_of(&a[1])?, real_of(&a[2])?);
            if v <= 0.0 {
                return Err(Error::BadValue);
            }
            let d = x - m;
            one(real(
                (-d * d / (2.0 * v)).exp() / (2.0 * std::f64::consts::PI * v).sqrt(),
            ))
        }),
        // n x UTPC: chi-square with n degrees of freedom.
        "UTPC" => op(2, |_, a| {
            let (n, x) = (dof(real_of(&a[0])?)?, real_of(&a[1])?);
            one(real(gamma_q(n / 2.0, x.max(0.0) / 2.0)))
        }),
        // n x UTPT: Student's t with n degrees of freedom.
        "UTPT" => op(2, |_, a| {
            let (n, x) = (dof(real_of(&a[0])?)?, real_of(&a[1])?);
            let tail = beta_i(n / 2.0, 0.5, n / (n + x * x)) / 2.0;
            one(real(if x >= 0.0 { tail } else { 1.0 - tail }))
        }),
        // n1 n2 x UTPF: Snedecor's F with n1 and n2 degrees of freedom.
        "UTPF" => op(3, |_, a| {
            let (n1, n2, x) = (
                dof(real_of(&a[0])?)?,
                dof(real_of(&a[1])?)?,
                real_of(&a[2])?,
            );
            if x <= 0.0 {
                return one(real(1.0));
            }
            one(real(beta_i(n2 / 2.0, n1 / 2.0, n2 / (n2 + n1 * x))))
        }),
        _ => None,
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
        assert_eq!(run("1 Σ+ 2 Σ+ 3 Σ+ 4 Σ+ PVAR"), ["1.25"]);
        assert_eq!(
            run("[[1 0][2 0][3 0][9 0]] STOΣ 0 3 3 BINS"),
            ["[[ 2 ] [ 1 ] [ 0 ]]", "[ 0 1 ]"]
        );
    }

    #[test]
    fn models() {
        // y = 2·3^x is exponential.
        let data = "[[0 2][1 6][2 18][3 54]] STOΣ";
        assert_eq!(
            run(&format!("{data} BESTFIT ΣPAR 5 GET")),
            [":Intercept:2", ":Slope:1.09861228867", "'EXP'"]
        );
        assert_eq!(
            run(&format!("{data} EXPFIT LR DROP2 4 PREDY 6 RND")),
            ["162"]
        );
        assert_eq!(
            run(&format!("{data} EXPFIT LR DROP2 ΣLINE")),
            ["'2*EXP(1.09861228867*X)'"]
        );
        assert_eq!(
            run("[[1 3][2 12][3 27]] STOΣ PWRFIT LR DROP2 48 PREDX"),
            ["4"]
        );
    }

    #[test]
    fn distributions() {
        assert_eq!(run("0 1 1.96 UTPN 4 RND"), [".025"]);
        assert_eq!(run("0 1 0 NDIST"), [".398942280401"]);
        assert_eq!(run("2 2 UTPC"), [".367879441171"]);
        assert_eq!(run("10 2.228 UTPT 4 RND"), [".025"]);
        assert_eq!(run("1 1 1 UTPF"), [".5"]);
    }
}

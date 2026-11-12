//! Differential equations (RKF, RRK and their single steps) and the time
//! value of money (TVMROOT, AMORT).

use crate::calc::{Calc, Error, Res};
use crate::obj::{Obj, r12};

/// The Runge-Kutta-Fehlberg 4(5) tableau.
const A: [[f64; 5]; 6] = [
    [0.0, 0.0, 0.0, 0.0, 0.0],
    [1.0 / 4.0, 0.0, 0.0, 0.0, 0.0],
    [3.0 / 32.0, 9.0 / 32.0, 0.0, 0.0, 0.0],
    [1932.0 / 2197.0, -7200.0 / 2197.0, 7296.0 / 2197.0, 0.0, 0.0],
    [439.0 / 216.0, -8.0, 3680.0 / 513.0, -845.0 / 4104.0, 0.0],
    [
        -8.0 / 27.0,
        2.0,
        -3544.0 / 2565.0,
        1859.0 / 4104.0,
        -11.0 / 40.0,
    ],
];
const C: [f64; 6] = [0.0, 1.0 / 4.0, 3.0 / 8.0, 12.0 / 13.0, 1.0, 1.0 / 2.0];
/// Fifth order weights, and the difference from the fourth order ones.
const B5: [f64; 6] = [
    16.0 / 135.0,
    0.0,
    6656.0 / 12825.0,
    28561.0 / 56430.0,
    -9.0 / 50.0,
    2.0 / 55.0,
];
const E: [f64; 6] = [
    1.0 / 360.0,
    0.0,
    -128.0 / 4275.0,
    -2197.0 / 75240.0,
    1.0 / 50.0,
    2.0 / 55.0,
];

/// { 'x' 'y' 'f(x,y)' … }: the names of the independent and solution
/// variables, and y' as an expression or program.
struct Ode {
    x: String,
    y: String,
    f: Obj,
}

fn values(o: &Obj) -> Res<Vec<f64>> {
    match o {
        Obj::Real(x) => Ok(vec![*x]),
        Obj::Vect(v) => Ok(v.clone()),
        _ => Err(Error::BadType),
    }
}

fn value(v: &[f64]) -> Obj {
    match v {
        [x] => Obj::Real(r12(*x)),
        v => Obj::Vect(v.iter().map(|x| r12(*x)).collect()),
    }
}

impl Calc {
    fn ode(&self, o: &Obj, len: usize) -> Res<Ode> {
        let Obj::List(l) = o else {
            return Err(Error::BadType);
        };
        match &l[..] {
            [Obj::Name(x), Obj::Name(y), f, rest @ ..] if rest.len() + 3 == len => Ok(Ode {
                x: x.clone(),
                y: y.clone(),
                f: f.clone(),
            }),
            _ => Err(Error::BadValue),
        }
    }

    fn global_values(&self, name: &str) -> Res<Vec<f64>> {
        values(self.global(name).ok_or(Error::Undefined)?)
    }

    /// y' at (x, y), with the variables set to them.
    fn ode_slope(&mut self, ode: &Ode, x: f64, y: &[f64]) -> Res<Vec<f64>> {
        self.store(ode.x.clone(), Obj::Real(x))?;
        self.store(ode.y.clone(), value(y))?;
        self.stack.push(ode.f.clone());
        self.eval_word("→NUM")?;
        let d = values(&self.pop()?)?;
        if d.len() != y.len() {
            return Err(Error::Dim);
        }
        Ok(d)
    }

    /// One step of size h from (x, y): the new y, and the error estimate.
    fn rkf_step(&mut self, ode: &Ode, x: f64, y: &[f64], h: f64) -> Res<(Vec<f64>, f64)> {
        let mut k: Vec<Vec<f64>> = vec![];
        for s in 0..6 {
            let ys: Vec<f64> = (0..y.len())
                .map(|i| y[i] + h * (0..s).map(|j| A[s][j] * k[j][i]).sum::<f64>())
                .collect();
            k.push(self.ode_slope(ode, x + C[s] * h, &ys)?);
        }
        let y5 = (0..y.len())
            .map(|i| y[i] + h * (0..6).map(|s| B5[s] * k[s][i]).sum::<f64>())
            .collect();
        let err = (0..y.len())
            .map(|i| (h * (0..6).map(|s| E[s] * k[s][i]).sum::<f64>()).abs())
            .fold(0.0, f64::max);
        Ok((y5, err))
    }

    /// An adaptive step from the variables' values, trying h first: the step
    /// taken and the size suggested for the next one. The variables are left
    /// at the new point.
    fn adaptive_step(&mut self, ode: &Ode, tol: f64, mut h: f64) -> Res<(f64, f64)> {
        let x = real_value(self.global(&ode.x).ok_or(Error::Undefined)?)?;
        let y = self.global_values(&ode.y)?;
        for _ in 0..60 {
            let (y1, err) = self.rkf_step(ode, x, &y, h)?;
            let scale = if err == 0.0 {
                4.0
            } else {
                (0.84 * (tol / err).powf(0.25)).clamp(0.1, 4.0)
            };
            if err <= tol.max(1e-300) {
                self.store(ode.x.clone(), Obj::Real(r12(x + h)))?;
                self.store(ode.y.clone(), value(&y1))?;
                return Ok((h, h * scale));
            }
            h *= scale;
        }
        Err(Error::UndefinedResult)
    }

    /// Integrates from the variables' values to x = `to`.
    fn integrate_ode(&mut self, ode: &Ode, tol: f64, to: f64) -> Res<()> {
        let x0 = real_value(self.global(&ode.x).ok_or(Error::Undefined)?)?;
        let mut h = (to - x0) / 16.0;
        let mut x = x0;
        let mut steps = 0;
        while (to - x).abs() > 1e-12 * to.abs().max(1.0) {
            if (x + h - to) * (to - x0).signum() > 0.0 {
                h = to - x;
            }
            let (taken, next) = self.adaptive_step(ode, tol, h)?;
            x += taken;
            h = next;
            steps += 1;
            if steps > 100_000 {
                return Err(Error::UndefinedResult);
            }
        }
        self.store(ode.x.clone(), Obj::Real(to))
    }

    /// The error estimate of a step of h from the variables' values, and
    /// the change in y it makes, leaving the variables as they were.
    fn step_error(&mut self, ode: &Ode, h: f64) -> Res<(Vec<f64>, f64)> {
        let x = real_value(self.global(&ode.x).ok_or(Error::Undefined)?)?;
        let y = self.global_values(&ode.y)?;
        let (y1, err) = self.rkf_step(ode, x, &y, h)?;
        self.store(ode.x.clone(), Obj::Real(x))?;
        self.store(ode.y.clone(), value(&y))?;
        Ok((y1.iter().zip(&y).map(|(a, b)| a - b).collect(), err))
    }

    pub(crate) fn numeric_cmd(&mut self, w: &str) -> Option<Res<()>> {
        // The stiff solver's list also holds ∂f/∂x and ∂f/∂y; the steps are
        // taken with the same embedded Runge-Kutta pair.
        let len = |w: &str| {
            if matches!(w, "RRK" | "RRKSTEP" | "RSBERR") {
                5
            } else {
                3
            }
        };
        Some(match w {
            // { x y f } tol xfinal RKF: x and y advanced to xfinal.
            "RKF" | "RRK" => self.args(3, |c, a| {
                let ode = c.ode(&a[0], len(w))?;
                let (tol, to) = (real_value(&a[1])?, real_value(&a[2])?);
                c.integrate_ode(&ode, tol.abs(), to)?;
                Ok(vec![a[0].clone(), a[1].clone()])
            }),
            // { x y f } tol h RKFSTEP: one step, with the next step's size.
            "RKFSTEP" => self.args(3, |c, a| {
                let ode = c.ode(&a[0], 3)?;
                let (tol, h) = (real_value(&a[1])?, real_value(&a[2])?);
                let (_, next) = c.adaptive_step(&ode, tol.abs(), h)?;
                Ok(vec![a[0].clone(), a[1].clone(), Obj::Real(r12(next))])
            }),
            // { x y f fx fy } tol h last RRKSTEP: also which method was used.
            "RRKSTEP" => self.args(4, |c, a| {
                let ode = c.ode(&a[0], 5)?;
                let (tol, h) = (real_value(&a[1])?, real_value(&a[2])?);
                let (_, next) = c.adaptive_step(&ode, tol.abs(), h)?;
                Ok(vec![
                    a[0].clone(),
                    a[1].clone(),
                    Obj::Real(r12(next)),
                    Obj::Real(1.0),
                ])
            }),
            // { x y f } h RKFERR: the step, the change in y and its error.
            "RKFERR" | "RSBERR" => self.args(2, |c, a| {
                let ode = c.ode(&a[0], len(w))?;
                let h = real_value(&a[1])?;
                let (dy, err) = c.step_error(&ode, h)?;
                Ok(vec![
                    a[0].clone(),
                    a[1].clone(),
                    value(&dy),
                    Obj::Real(r12(err)),
                ])
            }),
            "TVMROOT" => self.need(1).and_then(|_| {
                let Obj::Name(n) = self.peek(1).clone() else {
                    return Err(Error::BadType);
                };
                let x = Tvm::read(self, &n)?.solve(&n)?;
                self.store(n, Obj::Real(x))?;
                self.stack.pop();
                self.stack.push(Obj::Real(x));
                Ok(())
            }),
            // n AMORT: the principal and interest of the next n payments, and
            // the balance after them.
            "AMORT" => self.need(1).and_then(|_| {
                let Obj::Real(n) = *self.peek(1) else {
                    return Err(Error::BadType);
                };
                if n < 0.0 || n.fract() != 0.0 {
                    return Err(Error::BadValue);
                }
                let t = Tvm::read(self, "")?;
                let i = t.rate();
                let (mut bal, mut int_total) = (t.pv, 0.0);
                for k in 0..n as usize {
                    let int = if t.begin && k == 0 {
                        0.0
                    } else {
                        -r12(bal * i)
                    };
                    bal = bal - int + t.pmt;
                    int_total += int;
                }
                let principal = t.pmt * n - int_total;
                self.stack.pop();
                let tag = |s: &str, x: f64| Obj::Tag(s.into(), Box::new(Obj::Real(r12(x))));
                self.stack.push(tag("Principal", principal));
                self.stack.push(tag("Interest", int_total));
                self.stack.push(tag("Balance", bal));
                Ok(())
            }),
            _ => return None,
        })
    }
}

fn real_value(o: &Obj) -> Res<f64> {
    match o {
        Obj::Real(x) => Ok(*x),
        Obj::Tag(_, o) => real_value(o),
        _ => Err(Error::BadType),
    }
}

/// The variables of the time value of money: N I%YR PV PMT FV, payments a
/// year in PYR (12 if there is none), and payments at the beginning of each
/// period when flag -14 is set.
struct Tvm {
    n: f64,
    i_yr: f64,
    pv: f64,
    pmt: f64,
    fv: f64,
    pyr: f64,
    begin: bool,
}

const TVM_NAMES: [&str; 5] = ["N", "I%YR", "PV", "PMT", "FV"];

impl Tvm {
    /// The variables, all but `solving` needing a value.
    fn read(c: &Calc, solving: &str) -> Res<Tvm> {
        if !solving.is_empty() && !TVM_NAMES.contains(&solving) {
            return Err(Error::BadValue);
        }
        let get = |n: &str| -> Res<f64> {
            match c.global(n) {
                Some(o) => real_value(o),
                None if n == solving => Ok(0.0),
                None => Err(Error::Undefined),
            }
        };
        Ok(Tvm {
            n: get("N")?,
            i_yr: get("I%YR")?,
            pv: get("PV")?,
            pmt: get("PMT")?,
            fv: get("FV")?,
            pyr: match c.global("PYR") {
                Some(o) => real_value(o)?,
                None => 12.0,
            },
            begin: c.flags[1] & (1 << 13) != 0,
        })
    }

    fn rate(&self) -> f64 {
        self.i_yr / 100.0 / self.pyr
    }

    /// PV + (1 + i·S)·PMT·(1 - (1+i)^-N)/i + FV·(1+i)^-N, which is 0 when
    /// the cash flows balance.
    fn balance(&self) -> f64 {
        let i = self.rate();
        let s = if self.begin { 1.0 } else { 0.0 };
        let (annuity, disc) = if i == 0.0 {
            (self.n, 1.0)
        } else {
            let d = (1.0 + i).powf(-self.n);
            ((1.0 + i * s) * (1.0 - d) / i, d)
        };
        self.pv + self.pmt * annuity + self.fv * disc
    }

    fn solve(&mut self, n: &str) -> Res<f64> {
        let i = self.rate();
        let s = if self.begin { 1.0 } else { 0.0 };
        let fin = |x: f64| {
            if x.is_finite() {
                Ok(r12(x))
            } else {
                Err(Error::BadValue)
            }
        };
        let (annuity, disc) = if i == 0.0 {
            (self.n, 1.0)
        } else {
            let d = (1.0 + i).powf(-self.n);
            ((1.0 + i * s) * (1.0 - d) / i, d)
        };
        match n {
            "PV" => fin(-(self.pmt * annuity + self.fv * disc)),
            "PMT" => fin(-(self.pv + self.fv * disc) / annuity),
            "FV" => fin(-(self.pv + self.pmt * annuity) / disc),
            "N" => {
                if i == 0.0 {
                    return fin(-(self.pv + self.fv) / self.pmt);
                }
                let a = self.pmt * (1.0 + i * s) / i;
                // (1+i)^-N = (PV + a) / (a - FV)
                fin(-((self.pv + a) / (a - self.fv)).ln() / (1.0 + i).ln())
            }
            _ => {
                // The rate by the secant method from two guesses.
                let f = |t: &mut Tvm, r: f64| {
                    t.i_yr = r;
                    t.balance()
                };
                let (mut r0, mut r1) = (1.0, 10.0);
                let (mut f0, mut f1) = (f(self, r0), f(self, r1));
                for _ in 0..200 {
                    if f1 == f0 {
                        break;
                    }
                    let r2 = r1 - f1 * (r1 - r0) / (f1 - f0);
                    (r0, f0) = (r1, f1);
                    r1 = r2;
                    f1 = f(self, r1);
                    if (r1 - r0).abs() < 1e-12 * r1.abs().max(1.0) {
                        break;
                    }
                }
                if !f1.is_finite()
                    || f1.abs() > 1e-6 * (self.pv.abs() + self.pmt.abs() + self.fv.abs())
                {
                    return Err(Error::BadValue);
                }
                fin(r1)
            }
        }
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
    fn differential_equations() {
        // y' = y from y(0) = 1, so y(1) is e.
        assert_eq!(
            run("0 'X' STO 1 'Y' STO { X Y 'Y' } .00001 1 RKF Y 4 RND X"),
            ["{ X Y Y }", "1.E-5", "2.7183", "1"]
        );
        // A system: y = [ cos sin ] from y' = [ -y2 y1 ].
        assert_eq!(
            run(
                "RAD 0 'T' STO [ 1 0 ] 'Y' STO { T Y « Y 2 GET NEG Y 1 GET 2 →ARRY » } .000001 π →NUM RKF DROP2 Y 1 GET 4 RND"
            ),
            ["-1"]
        );
        assert_eq!(
            run("0 'X' STO 1 'Y' STO { X Y 'Y' } .1 RKFERR X"),
            ["{ X Y Y }", ".1", ".105170917147", "1.23397548971E-8", "0"]
        );
        assert_eq!(
            run("0 'X' STO 1 'Y' STO { X Y 'Y' } .001 .1 RKFSTEP DROP2 DROP X"),
            [".1"]
        );
    }

    #[test]
    fn money() {
        // 30 years of monthly payments on 200000 at 6%.
        assert_eq!(
            run("360 'N' STO 6 'I%YR' STO 200000 'PV' STO 0 'FV' STO 'PMT' TVMROOT"),
            ["-1199.10105031"]
        );
        assert_eq!(
            run(
                "360 'N' STO 200000 'PV' STO -1199.10105031 'PMT' STO 0 'FV' STO 'I%YR' TVMROOT 6 RND"
            ),
            ["6"]
        );
        assert_eq!(
            run("360 'N' STO 6 'I%YR' STO 200000 'PV' STO -1199.10 'PMT' STO 0 'FV' STO 1 AMORT"),
            [":Principal:-199.1", ":Interest:-1000", ":Balance:199800.9"]
        );
    }
}

//! The constants library (CONST), the largest and smallest reals, and the
//! built-in engineering functions ZFACTOR, DARCY, F0λ and SIDENS.

use crate::calc::{Calc, Error, OpFn, Res, one, op, real, real_of};
use crate::obj::{Node, Obj};
use crate::parse::parse;

/// The HP 48 GX's constants library: name, value and units.
pub const CONSTANTS: &[(&str, &str, &str)] = &[
    ("NA", "6.0221367E23", "mol^-1"),
    ("k", "1.380658E-23", "J/K"),
    ("Vm", "22.4141", "l/mol"),
    ("R", "8.31451", "J/(mol*K)"),
    ("StdT", "273.15", "K"),
    ("StdP", "101.325", "kPa"),
    ("σ", "5.67051E-8", "W/(m^2*K^4)"),
    ("c", "299792458", "m/s"),
    ("ε0", "8.85418781761E-12", "F/m"),
    ("μ0", "1.25663706144E-6", "H/m"),
    ("g", "9.80665", "m/s^2"),
    ("G", "6.67259E-11", "m^3/(s^2*kg)"),
    ("h", "6.6260755E-34", "J*s"),
    ("ħ", "1.05457266E-34", "J*s"),
    ("q", "1.60217733E-19", "C"),
    ("me", "9.1093897E-31", "kg"),
    ("qme", "175881962000", "C/kg"),
    ("mp", "1.6726231E-27", "kg"),
    ("mpme", "1836.152701", ""),
    ("α", "7.29735308E-3", ""),
    ("ø", "2.06783461E-15", "Wb"),
    ("F", "96485.309", "C/mol"),
    ("R∞", "10973731.534", "m^-1"),
    ("a0", ".0529177249", "nm"),
    ("μB", "9.2740154E-24", "J/T"),
    ("μN", "5.0507866E-27", "J/T"),
    ("λ0", "1239.8425", "nm"),
    ("f0", "2.4179883E14", "Hz"),
    ("λc", ".00242631058", "nm"),
    ("c3", ".002897756", "m*K"),
    ("kq", ".00008617386", "J/(K*C)"),
    ("ε0q", "55263469.6", "F/(m*C)"),
    ("qε0", "1.4185978E-30", "F*C/m"),
    ("εsi", "11.9", ""),
    ("εox", "3.9", ""),
    ("I0", ".000000000001", "W/m^2"),
];

/// The second radiation constant hc/k, in m·K.
const C2: f64 = 0.014_387_69;

/// Z for reduced temperature tr and pressure pr, from the Hall-Yarborough
/// equation of state solved for the reduced density by Newton's method.
fn zfactor(tr: f64, pr: f64) -> Res<f64> {
    if !(1.05..=3.0).contains(&tr) || !(0.0..=30.0).contains(&pr) {
        return Err(Error::BadValue);
    }
    if pr == 0.0 {
        return Ok(1.0);
    }
    let t = 1.0 / tr;
    let a = 0.06125 * t * (-1.2 * (1.0 - t).powi(2)).exp();
    let b = t * (14.76 - 9.76 * t + 4.58 * t * t);
    let c = t * (90.7 - 242.2 * t + 42.4 * t * t);
    let d = 2.18 + 2.82 * t;
    let f = |y: f64| {
        -a * pr + (y + y * y + y.powi(3) - y.powi(4)) / (1.0 - y).powi(3) - b * y * y
            + c * y.powf(d)
    };
    let df = |y: f64| {
        (1.0 + 4.0 * y + 4.0 * y * y - 4.0 * y.powi(3) + y.powi(4)) / (1.0 - y).powi(4)
            - 2.0 * b * y
            + c * d * y.powf(d - 1.0)
    };
    let mut y: f64 = 0.001;
    for _ in 0..100 {
        let step = f(y) / df(y);
        y = (y - step).clamp(1e-12, 0.99);
        if step.abs() < 1e-14 {
            return Ok(a * pr / y);
        }
    }
    Err(Error::UndefinedResult)
}

/// The Darcy friction factor for relative roughness ε/D and Reynolds number
/// re: 64/Re in laminar flow, Colebrook's equation in turbulent flow.
fn darcy(rough: f64, re: f64) -> Res<f64> {
    if rough < 0.0 || re <= 0.0 {
        return Err(Error::BadValue);
    }
    if re <= 2100.0 {
        return Ok(64.0 / re);
    }
    // 1/√f = -2·log(ε/3.7D + 2.51/(Re·√f)), iterated in x = 1/√f.
    let mut x: f64 = 8.0;
    for _ in 0..100 {
        let next = -2.0 * (rough / 3.7 + 2.51 * x / re).log10();
        if (next - x).abs() < 1e-14 * x {
            return Ok(1.0 / (next * next));
        }
        x = next;
    }
    Err(Error::UndefinedResult)
}

/// The fraction of a black body's emissive power at temperature t (K) that
/// is at wavelengths up to λ (m).
fn f0_lambda(t: f64, lambda: f64) -> Res<f64> {
    if t <= 0.0 || lambda <= 0.0 {
        return Err(Error::BadValue);
    }
    let x = C2 / (lambda * t);
    if x > 700.0 {
        return Ok(0.0);
    }
    let sum: f64 = (1..=200)
        .map(|n| {
            let n = n as f64;
            (-n * x).exp() / n * (x.powi(3) + 3.0 * x * x / n + 6.0 * x / (n * n) + 6.0 / n.powi(3))
        })
        .sum();
    Ok((15.0 / std::f64::consts::PI.powi(4) * sum).clamp(0.0, 1.0))
}

/// The intrinsic carrier density of silicon, per cm³, at t kelvin.
fn sidens(t: f64) -> Res<f64> {
    if !(0.0..=1685.0).contains(&t) {
        return Err(Error::BadValue);
    }
    Ok(5.29e19 * (t / 300.0).powf(2.54) * (-6726.0 / t).exp())
}

/// The temperature or length in a unit object as kelvin or metres, or a
/// real as it is.
fn si(c: &Calc, o: &Obj, unit: &str) -> Res<f64> {
    match o {
        Obj::Real(x) => Ok(*x),
        Obj::Unit(..) => {
            let (_, f) = crate::calc::pure_any("CONVERT").unwrap();
            let to = parse(&format!("1_{unit}"))?;
            let [Node::Push(to)] = &to[..] else {
                return Err(Error::InvalidUnit);
            };
            match &c.compute("CONVERT", f, &[o.clone(), to.clone()])?[..] {
                [Obj::Unit(x, _)] => Ok(*x),
                [Obj::Real(x)] => Ok(*x),
                _ => Err(Error::BadType),
            }
        }
        _ => Err(Error::BadType),
    }
}

pub fn pure(w: &str) -> Option<(usize, OpFn)> {
    match w {
        "ZFACTOR" => op(2, |_, a| {
            one(real(zfactor(real_of(&a[0])?, real_of(&a[1])?)?))
        }),
        "DARCY" => op(2, |_, a| {
            one(real(darcy(real_of(&a[0])?, real_of(&a[1])?)?))
        }),
        "F0λ" => op(2, |c, a| {
            one(real(f0_lambda(si(c, &a[0], "K")?, si(c, &a[1], "m")?)?))
        }),
        "SIDENS" => op(1, |c, a| {
            let t = si(c, &a[0], "K")?;
            let n = sidens(t)?;
            Ok(match &a[0] {
                Obj::Unit(..) => {
                    let u = crate::units::parse_units("1/cm^3").unwrap();
                    vec![Obj::Unit(crate::obj::r12(n), u)]
                }
                _ => vec![real(n)?],
            })
        }),
        _ => None,
    }
}

impl Calc {
    pub(crate) fn consts_cmd(&mut self, w: &str) -> Option<Res<()>> {
        Some(match w {
            // 'name' CONST: the constant with its units, or without them
            // when flag -61 is set.
            "CONST" => self.need(1).and_then(|_| {
                let Obj::Name(n) = self.peek(1).clone() else {
                    return Err(Error::BadType);
                };
                let &(_, v, u) = CONSTANTS
                    .iter()
                    .find(|(c, ..)| *c == n)
                    .ok_or(Error::BadValue)?;
                let bare = u.is_empty() || self.flags[1] & (1 << 60) != 0;
                let src = if bare {
                    v.to_string()
                } else {
                    format!("{v}_{u}")
                };
                let [Node::Push(o)] = &parse(&src)?[..] else {
                    unreachable!("the table's constants parse")
                };
                self.stack.pop();
                self.stack.push(o.clone());
                Ok(())
            }),
            "CONLIB" => {
                (self.menu, self.page) = (crate::menu::Menu::Conlib, 0);
                Ok(())
            }
            // The extremes of the reals rpn48 holds, 12 digits apiece.
            "MAXR" => {
                self.stack.push(Obj::Real(1.79769313486E308));
                Ok(())
            }
            "MINR" => {
                self.stack.push(Obj::Real(2.22507385851E-308));
                Ok(())
            }
            _ => return None,
        })
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
    fn constants() {
        assert_eq!(
            run("'c' CONST 'NA' CONST"),
            ["299792458_m/s", "6.0221367E23_1/mol"]
        );
        assert_eq!(
            run("-61 SF 'g' CONST 'α' CONST"),
            ["9.80665", ".00729735308"]
        );
        assert_eq!(
            run("'me' CONST 'c' CONST SQ * 1_MeV CONVERT UVAL 4 RND"),
            [".511"]
        );
    }

    #[test]
    fn engineering() {
        assert_eq!(run("2 0 ZFACTOR 1.5 2 ZFACTOR 3 RND"), ["1", ".821"]);
        assert_eq!(
            run(".0001 1000 DARCY .0002 100000 DARCY 4 RND"),
            [".064", ".019"]
        );
        assert_eq!(
            run("1000 .000005 F0λ 4 RND 1000_K 5_μm F0λ 4 RND"),
            [".6337", ".6337"]
        );
        assert_eq!(run("300 SIDENS -2 RND"), ["9700000000"]);
    }
}

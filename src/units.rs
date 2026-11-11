//! Unit objects such as 9.8_m/s^2: a real and the units it is measured in,
//! with the calculator's unit table, prefixes and conversions.

use crate::calc::{Calc, Error, OpFn, Res, div0, op, real, real_of};
use crate::obj::{Obj, r12};

/// Units as written: each name, prefix included, with its exponent.
pub type Units = Vec<(String, f64)>;

/// Powers of the SI base units m kg s A K mol cd.
type Dims = [f64; 7];

const BASE: [&str; 7] = ["m", "kg", "s", "A", "K", "mol", "cd"];

const L: [i8; 7] = [1, 0, 0, 0, 0, 0, 0];
const AREA: [i8; 7] = [2, 0, 0, 0, 0, 0, 0];
const VOL: [i8; 7] = [3, 0, 0, 0, 0, 0, 0];
const T: [i8; 7] = [0, 0, 1, 0, 0, 0, 0];
const FREQ: [i8; 7] = [0, 0, -1, 0, 0, 0, 0];
const SPEED: [i8; 7] = [1, 0, -1, 0, 0, 0, 0];
const ACCEL: [i8; 7] = [1, 0, -2, 0, 0, 0, 0];
const M: [i8; 7] = [0, 1, 0, 0, 0, 0, 0];
const MOL: [i8; 7] = [0, 0, 0, 0, 0, 1, 0];
const FORCE: [i8; 7] = [1, 1, -2, 0, 0, 0, 0];
const ENERGY: [i8; 7] = [2, 1, -2, 0, 0, 0, 0];
const POWER: [i8; 7] = [2, 1, -3, 0, 0, 0, 0];
const PRESS: [i8; 7] = [-1, 1, -2, 0, 0, 0, 0];
const TEMP: [i8; 7] = [0, 0, 0, 0, 1, 0, 0];
const AMP: [i8; 7] = [0, 0, 0, 1, 0, 0, 0];
const VOLT: [i8; 7] = [2, 1, -3, -1, 0, 0, 0];
const CHARGE: [i8; 7] = [0, 0, 1, 1, 0, 0, 0];
const OHM: [i8; 7] = [2, 1, -3, -2, 0, 0, 0];
const FARAD: [i8; 7] = [-2, -1, 4, 2, 0, 0, 0];
const HENRY: [i8; 7] = [2, 1, -2, -2, 0, 0, 0];
const SIEMENS: [i8; 7] = [-2, -1, 3, 2, 0, 0, 0];
const TESLA: [i8; 7] = [0, 1, -2, -1, 0, 0, 0];
const WEBER: [i8; 7] = [2, 1, -2, -1, 0, 0, 0];
const NONE: [i8; 7] = [0; 7];
const CD: [i8; 7] = [0, 0, 0, 0, 0, 0, 1];
const LUX: [i8; 7] = [-2, 0, 0, 0, 0, 0, 1];
const DOSE: [i8; 7] = [2, 0, -2, 0, 0, 0, 0];
const EXPOSURE: [i8; 7] = [0, -1, 1, 1, 0, 0, 0];
const DYNVISC: [i8; 7] = [-1, 1, -1, 0, 0, 0, 0];
const KINVISC: [i8; 7] = [2, 0, -1, 0, 0, 0, 0];

const PI: f64 = std::f64::consts::PI;

/// The unit table: name, value in SI base units, and dimensions.
const TABLE: &[(&str, f64, [i8; 7])] = &[
    ("m", 1.0, L),
    ("ft", 0.3048, L),
    ("ftUS", 1200.0 / 3937.0, L),
    ("in", 0.0254, L),
    ("yd", 0.9144, L),
    ("mi", 1609.344, L),
    ("miUS", 1609.34721869, L),
    ("nmi", 1852.0, L),
    ("au", 1.495979e11, L),
    ("lyr", 9.46052840488e15, L),
    ("pc", 3.08567818585e16, L),
    ("Å", 1e-10, L),
    ("μ", 1e-6, L),
    ("mil", 2.54e-5, L),
    ("fath", 1.8288, L),
    ("chain", 20.1168, L),
    ("rd", 5.0292, L),
    ("fermi", 1e-15, L),
    ("acre", 4046.87260987, AREA),
    ("a", 100.0, AREA),
    ("ha", 10000.0, AREA),
    ("b", 1e-28, AREA),
    ("l", 0.001, VOL),
    ("st", 1.0, VOL),
    ("gal", 0.003785411784, VOL),
    ("galUK", 0.004546092, VOL),
    ("galC", 0.00454609, VOL),
    ("qt", 9.46352946e-4, VOL),
    ("pt", 4.73176473e-4, VOL),
    ("cu", 2.365882365e-4, VOL),
    ("ozfl", 2.95735295625e-5, VOL),
    ("ozUK", 2.8413075e-5, VOL),
    ("tbsp", 1.47867647813e-5, VOL),
    ("tsp", 4.92892159375e-6, VOL),
    ("bbl", 0.158987294928, VOL),
    ("bu", 0.03523907, VOL),
    ("pk", 0.0088097675, VOL),
    ("fbm", 0.002359737216, VOL),
    ("s", 1.0, T),
    ("min", 60.0, T),
    ("h", 3600.0, T),
    ("d", 86400.0, T),
    ("yr", 31556925.9747, T),
    ("Hz", 1.0, FREQ),
    ("kph", 1.0 / 3.6, SPEED),
    ("mph", 0.44704, SPEED),
    ("knot", 1852.0 / 3600.0, SPEED),
    ("c", 299792458.0, SPEED),
    ("ga", 9.80665, ACCEL),
    ("g", 0.001, M),
    ("kg", 1.0, M),
    ("lb", 0.45359237, M),
    ("oz", 0.028349523125, M),
    ("slug", 14.5939029372, M),
    ("lbt", 0.3732417216, M),
    ("ton", 907.18474, M),
    ("tonUK", 1016.0469088, M),
    ("t", 1000.0, M),
    ("ozt", 0.0311034768, M),
    ("ct", 0.0002, M),
    ("grain", 6.479891e-5, M),
    ("u", 1.6605402e-27, M),
    ("mol", 1.0, MOL),
    ("N", 1.0, FORCE),
    ("dyn", 1e-5, FORCE),
    ("gf", 0.00980665, FORCE),
    ("kip", 4448.22161526, FORCE),
    ("lbf", 4.44822161526, FORCE),
    ("pdl", 0.138254954376, FORCE),
    ("J", 1.0, ENERGY),
    ("erg", 1e-7, ENERGY),
    ("Kcal", 4186.8, ENERGY),
    ("cal", 4.1868, ENERGY),
    ("Btu", 1055.05585262, ENERGY),
    ("therm", 105506000.0, ENERGY),
    ("eV", 1.60217733e-19, ENERGY),
    ("W", 1.0, POWER),
    ("hp", 745.699871582, POWER),
    ("Pa", 1.0, PRESS),
    ("atm", 101325.0, PRESS),
    ("bar", 100000.0, PRESS),
    ("psi", 6894.75729317, PRESS),
    ("torr", 133.322368421, PRESS),
    ("mmHg", 133.322368421, PRESS),
    ("inHg", 3386.38815789, PRESS),
    ("inH2O", 248.84, PRESS),
    ("K", 1.0, TEMP),
    ("°C", 1.0, TEMP),
    ("°F", 5.0 / 9.0, TEMP),
    ("°R", 5.0 / 9.0, TEMP),
    ("A", 1.0, AMP),
    ("V", 1.0, VOLT),
    ("C", 1.0, CHARGE),
    ("Ω", 1.0, OHM),
    ("F", 1.0, FARAD),
    ("Fdy", 96487.0, CHARGE),
    ("H", 1.0, HENRY),
    ("mho", 1.0, SIEMENS),
    ("S", 1.0, SIEMENS),
    ("T", 1.0, TESLA),
    ("Wb", 1.0, WEBER),
    ("°", PI / 180.0, NONE),
    ("r", 1.0, NONE),
    ("grad", PI / 200.0, NONE),
    ("arcmin", PI / 10800.0, NONE),
    ("arcs", PI / 648000.0, NONE),
    ("sr", 1.0, NONE),
    ("cd", 1.0, CD),
    ("lm", 1.0, CD),
    ("lx", 1.0, LUX),
    ("ph", 10000.0, LUX),
    ("fc", 10.7639104167, LUX),
    ("flam", 3.42625909964, LUX),
    ("nit", 1.0, LUX),
    ("sb", 10000.0, LUX),
    ("lam", 3183.09886184, LUX),
    ("Gy", 1.0, DOSE),
    ("rad", 0.01, DOSE),
    ("rem", 0.01, DOSE),
    ("Sv", 1.0, DOSE),
    ("Bq", 1.0, FREQ),
    ("Ci", 3.7e10, FREQ),
    ("R", 2.58e-4, EXPOSURE),
    ("P", 0.1, DYNVISC),
    ("St", 1e-4, KINVISC),
];

const PREFIXES: &[(&str, f64)] = &[
    ("Y", 1e24),
    ("Z", 1e21),
    ("E", 1e18),
    ("P", 1e15),
    ("T", 1e12),
    ("G", 1e9),
    ("M", 1e6),
    ("k", 1e3),
    ("h", 1e2),
    ("D", 1e1),
    ("d", 1e-1),
    ("c", 1e-2),
    ("m", 1e-3),
    ("μ", 1e-6),
    ("µ", 1e-6),
    ("n", 1e-9),
    ("p", 1e-12),
    ("f", 1e-15),
    ("a", 1e-18),
    ("z", 1e-21),
    ("y", 1e-24),
];

/// The UNITS menu: each category and the units on its keys.
pub const CATEGORIES: &[(&str, &[&str])] = &[
    (
        "LENG",
        &[
            "m", "cm", "mm", "km", "ft", "in", "yd", "mi", "nmi", "au", "lyr", "pc", "Å", "μ",
            "mil", "fath", "chain", "rd", "fermi", "ftUS", "miUS",
        ],
    ),
    (
        "AREA",
        &[
            "m^2", "cm^2", "km^2", "ft^2", "in^2", "yd^2", "mi^2", "acre", "a", "ha", "b",
        ],
    ),
    (
        "VOL",
        &[
            "m^3", "st", "cm^3", "ft^3", "in^3", "l", "ml", "gal", "galUK", "galC", "qt", "pt",
            "cu", "ozfl", "ozUK", "tbsp", "tsp", "bbl", "bu", "pk", "fbm",
        ],
    ),
    ("TIME", &["yr", "d", "h", "min", "s", "Hz"]),
    (
        "SPEED",
        &["m/s", "cm/s", "ft/s", "kph", "mph", "knot", "c", "ga"],
    ),
    (
        "MASS",
        &[
            "kg", "g", "lb", "oz", "slug", "lbt", "ton", "tonUK", "t", "ozt", "ct", "grain", "u",
            "mol",
        ],
    ),
    ("FORCE", &["N", "dyn", "gf", "kip", "lbf", "pdl"]),
    (
        "ENRG",
        &[
            "J", "erg", "Kcal", "cal", "Btu", "ft*lbf", "therm", "MeV", "eV",
        ],
    ),
    ("POWR", &["W", "hp"]),
    (
        "PRESS",
        &["Pa", "atm", "bar", "psi", "torr", "mmHg", "inHg", "inH2O"],
    ),
    ("TEMP", &["°C", "°F", "K", "°R"]),
    (
        "ELEC",
        &[
            "V", "A", "C", "Ω", "F", "W", "Fdy", "H", "mho", "S", "T", "Wb",
        ],
    ),
    ("ANGL", &["°", "r", "grad", "arcmin", "arcs", "sr"]),
    (
        "LIGHT",
        &["fc", "flam", "lx", "ph", "sb", "lm", "cd", "lam"],
    ),
    ("RAD", &["Gy", "rad", "rem", "Sv", "Bq", "Ci", "R"]),
    ("VISC", &["P", "St"]),
];

/// A unit name's value in SI base units and its dimensions, prefixes
/// included: km, MeV, μs.
fn lookup(name: &str) -> Option<(f64, Dims)> {
    let dims = |d: [i8; 7]| d.map(f64::from);
    if let Some(&(_, f, d)) = TABLE.iter().find(|(n, ..)| *n == name) {
        return Some((f, dims(d)));
    }
    PREFIXES.iter().find_map(|(p, pf)| {
        let rest = name.strip_prefix(p)?;
        let &(_, f, d) = TABLE.iter().find(|(n, ..)| *n == rest)?;
        (temp_offset(rest).is_none() || rest == "K").then(|| (pf * f, dims(d)))
    })
}

/// Where zero lies on a temperature scale, in its own degrees above absolute zero.
fn temp_offset(name: &str) -> Option<f64> {
    match name {
        "K" | "°R" => Some(0.0),
        "°C" => Some(273.15),
        "°F" => Some(459.67),
        _ => None,
    }
}

/// The value in SI base units of one of these units, and its dimensions.
fn factor(u: &[(String, f64)]) -> Res<(f64, Dims)> {
    let mut f = 1.0;
    let mut d = [0.0; 7];
    for (n, e) in u {
        let (uf, ud) = lookup(n).ok_or(Error::InvalidUnit)?;
        f *= uf.powf(*e);
        for (d, ud) in d.iter_mut().zip(ud) {
            *d += ud * e;
        }
    }
    Ok((f, d))
}

fn same_dims(a: &Dims, b: &Dims) -> bool {
    a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-9)
}

/// The base units for these dimensions, in the order UBASE gives them:
/// 1_V UBASE is 1_kg*m^2/(A*s^3).
fn base_units(d: &Dims) -> Units {
    [1, 0, 3, 2, 4, 5, 6]
        .into_iter()
        .filter(|&i| d[i].abs() > 1e-9)
        .map(|i| (BASE[i].to_string(), r12(d[i])))
        .collect()
}

/// Units multiplied by `b` raised to `sign`, like names combined.
fn merge(a: &[(String, f64)], b: &[(String, f64)], sign: f64) -> Units {
    let mut out = a.to_vec();
    for (n, e) in b {
        match out.iter_mut().find(|(m, _)| m == n) {
            Some((_, f)) => *f = r12(*f + sign * e),
            None => out.push((n.clone(), sign * e)),
        }
    }
    out.retain(|(_, e)| *e != 0.0);
    out
}

fn scale(u: &[(String, f64)], k: f64) -> Units {
    u.iter().map(|(n, e)| (n.clone(), r12(e * k))).collect()
}

/// A computed unit object, or a plain real once the units cancel.
fn unit(x: f64, u: Units) -> Res<Obj> {
    let r = real(x)?;
    Ok(match r {
        Obj::Real(x) if !u.is_empty() => Obj::Unit(x, u),
        r => r,
    })
}

/// A single temperature unit, the only kind CONVERT treats as a scale with
/// its own zero rather than a size.
fn temperature(u: &[(String, f64)]) -> Option<f64> {
    match u {
        [(n, e)] if *e == 1.0 => temp_offset(n),
        _ => None,
    }
}

fn convert(x: f64, from: &[(String, f64)], to: &[(String, f64)]) -> Res<f64> {
    let ((ff, fd), (tf, td)) = (factor(from)?, factor(to)?);
    if !same_dims(&fd, &td) {
        return Err(Error::InconsistentUnits);
    }
    Ok(match (temperature(from), temperature(to)) {
        (Some(fo), Some(to)) => (x + fo) * ff / tf - to,
        _ => x * ff / tf,
    })
}

/// Reads units such as m/s^2 or kg*m^2/(s^2*K).
pub fn parse_units(s: &str) -> Option<Units> {
    let cs: Vec<char> = s.chars().collect();
    let mut i = 0;
    let u = product(&cs, &mut i)?;
    (i == cs.len() && factor(&u).is_ok()).then_some(u)
}

fn product(cs: &[char], i: &mut usize) -> Option<Units> {
    let mut u = power(cs, i)?;
    while let Some(&c @ ('*' | '/')) = cs.get(*i) {
        *i += 1;
        let v = power(cs, i)?;
        u = merge(&u, &v, if c == '*' { 1.0 } else { -1.0 });
    }
    Some(u)
}

fn power(cs: &[char], i: &mut usize) -> Option<Units> {
    let u = if cs.get(*i) == Some(&'(') {
        *i += 1;
        let u = product(cs, i)?;
        (cs.get(*i) == Some(&')')).then_some(())?;
        *i += 1;
        u
    } else {
        let start = *i;
        while cs.get(*i).is_some_and(|&c| {
            c.is_alphabetic() || "°Åμµ".contains(c) || (*i > start && c.is_ascii_digit())
        }) {
            *i += 1;
        }
        if *i == start {
            return None;
        }
        vec![(cs[start..*i].iter().collect(), 1.0)]
    };
    if cs.get(*i) != Some(&'^') {
        return Some(u);
    }
    *i += 1;
    let start = *i;
    while cs
        .get(*i)
        .is_some_and(|&c| c.is_ascii_digit() || c == '.' || (c == '-' && *i == start))
    {
        *i += 1;
    }
    let e: f64 = cs[start..*i].iter().collect::<String>().parse().ok()?;
    Some(scale(&u, e))
}

/// Units written as the calculator does: m/s^2, kg*m^2/(s^2*K), 1/s.
pub fn write_units(out: &mut String, u: &[(String, f64)]) {
    let part = |pos: bool| -> Vec<String> {
        u.iter()
            .filter(|(_, e)| (*e > 0.0) == pos)
            .map(|(n, e)| match e.abs() {
                1.0 => n.clone(),
                e => format!("{n}^{}", crate::obj::fmt_real(e, crate::obj::Fmt::Std)),
            })
            .collect()
    };
    let (num, den) = (part(true), part(false));
    out.push_str(&if num.is_empty() {
        "1".into()
    } else {
        num.join("*")
    });
    match den.len() {
        0 => {}
        1 => out.push_str(&format!("/{}", den[0])),
        _ => out.push_str(&format!("/({})", den.join("*"))),
    }
}

/// The commands that take unit objects as numbers with units.
const OVER: &[&str] = &[
    "NEG", "ABS", "IP", "FP", "FLOOR", "CEIL", "SIGN", "INV", "SQ", "√", "SIN", "COS", "TAN", "*",
    "/", "^", "+", "-", "==", "≠", "<", ">", "≤", "≥", "MIN", "MAX",
];

/// What arithmetic, comparisons and the functions that keep units give for
/// unit objects; None when no argument has units.
pub(crate) fn over(c: &Calc, w: &str, f: OpFn, args: &[Obj]) -> Option<Res<Vec<Obj>>> {
    if !OVER.contains(&w)
        || !args.iter().any(|a| matches!(a, Obj::Unit(..)))
        || args.iter().any(|a| matches!(a, Obj::Str(_) | Obj::List(_)))
    {
        return None;
    }
    let parts = |o: &Obj| match o {
        Obj::Unit(x, u) => Ok((*x, u.clone())),
        Obj::Real(x) => Ok((*x, vec![])),
        _ => Err(Error::BadType),
    };
    Some((|| {
        let one = |o: Res<Obj>| o.map(|o| vec![o]);
        if let [a] = args {
            let (x, u) = parts(a)?;
            return match w {
                "NEG" => one(unit(-x, u)),
                "ABS" => one(unit(x.abs(), u)),
                "IP" => one(unit(x.trunc(), u)),
                "FP" => one(unit(x.fract(), u)),
                "FLOOR" => one(unit(x.floor(), u)),
                "CEIL" => one(unit(x.ceil(), u)),
                "SIGN" => one(real(x.signum())),
                "INV" if x == 0.0 => Err(Error::Infinite),
                "INV" => one(unit(1.0 / x, scale(&u, -1.0))),
                "SQ" => one(unit(x * x, scale(&u, 2.0))),
                "√" if x < 0.0 => Err(Error::BadValue),
                "√" => one(unit(x.sqrt(), scale(&u, 0.5))),
                // An angle in units is taken as such, whatever the angle mode.
                "SIN" | "COS" | "TAN" => {
                    let (fx, d) = factor(&u)?;
                    if !same_dims(&d, &[0.0; 7]) {
                        return Err(Error::InconsistentUnits);
                    }
                    f(c, &[Obj::Real(c.rad_to_angle(x * fx))])
                }
                _ => Err(Error::BadType),
            };
        }
        let [a, b] = args else {
            return Err(Error::BadType);
        };
        let ((x, u), (y, v)) = (parts(a)?, parts(b)?);
        match w {
            "*" => one(unit(x * y, merge(&u, &v, 1.0))),
            "/" if y == 0.0 => Err(div0(x)),
            "/" => one(unit(x / y, merge(&u, &v, -1.0))),
            "^" if v.is_empty() => one(unit(x.powf(y), scale(&u, y))),
            "+" | "-" | "==" | "≠" | "<" | ">" | "≤" | "≥" | "MIN" | "MAX" => {
                let y = convert(y, &v, &u)?;
                let t = |b: bool| Ok(vec![Obj::Real(if b { 1.0 } else { 0.0 })]);
                match w {
                    "+" => one(unit(x + y, u)),
                    "-" => one(unit(x - y, u)),
                    "MIN" => one(unit(x.min(y), u)),
                    "MAX" => one(unit(x.max(y), u)),
                    "==" => t(r12(x) == r12(y)),
                    "≠" => t(r12(x) != r12(y)),
                    "<" => t(x < y),
                    ">" => t(x > y),
                    "≤" => t(x <= y),
                    _ => t(x >= y),
                }
            }
            _ => Err(Error::BadType),
        }
    })())
}

fn unit_parts(o: &Obj) -> Res<(f64, Units)> {
    match o {
        Obj::Unit(x, u) => Ok((*x, u.clone())),
        _ => Err(Error::BadType),
    }
}

/// The unit commands that only compute.
pub(crate) fn pure(w: &str) -> Option<(usize, OpFn)> {
    match w {
        // Converts level 2 to the units of level 1.
        "CONVERT" => op(2, |_, a| {
            let ((x, u), (_, v)) = (unit_parts(&a[0])?, unit_parts(&a[1])?);
            Ok(vec![unit(convert(x, &u, &v)?, v)?])
        }),
        "UBASE" => op(1, |_, a| {
            let (x, u) = match &a[0] {
                Obj::Real(x) => return Ok(vec![Obj::Real(*x)]),
                o => unit_parts(o)?,
            };
            let (f, d) = factor(&u)?;
            let x = match temperature(&u) {
                Some(off) => (x + off) * f,
                None => x * f,
            };
            Ok(vec![unit(x, base_units(&d))?])
        }),
        "UVAL" => op(1, |_, a| Ok(vec![Obj::Real(unit_parts(&a[0])?.0)])),
        // Level 2 rewritten with level 1's units as a factor: 1_W 1_J UFACT
        // gives 1_J/s.
        "UFACT" => op(2, |_, a| {
            let ((x, u), (_, v)) = (unit_parts(&a[0])?, unit_parts(&a[1])?);
            let ((uf, ud), (vf, vd)) = (factor(&u)?, factor(&v)?);
            let rest: Dims = std::array::from_fn(|i| ud[i] - vd[i]);
            Ok(vec![unit(x * uf / vf, merge(&v, &base_units(&rest), 1.0))?])
        }),
        "→UNIT" => op(2, |_, a| {
            let x = real_of(&a[0])?;
            let (y, v) = unit_parts(&a[1])?;
            Ok(vec![unit(x * y, v)?])
        }),
        _ => None,
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
    fn units() {
        assert_eq!(run("9.8_m/s^2"), ["9.8_m/s^2"]);
        assert_eq!(run("1_kg*m^2/(s^2*K)"), ["1_kg*m^2/(s^2*K)"]);
        assert_eq!(run("1_ft 1_m CONVERT"), [".3048_m"]);
        assert_eq!(run("100_°C 1_°F CONVERT"), ["212_°F"]);
        assert_eq!(run("1_km 1_mi CONVERT"), [".621371192237_mi"]);
        assert_eq!(run("1_m 1_ft +"), ["1.3048_m"]);
        assert_eq!(run("3_m 2_s /"), ["1.5_m/s"]);
        assert_eq!(run("2_m 3_m *"), ["6_m^2"]);
        assert_eq!(run("6_m 3_m /"), ["2"]);
        assert_eq!(run("4_m^2 √ 5 *"), ["10_m"]);
        assert_eq!(run("1_N UBASE"), ["1_kg*m/s^2"]);
        assert_eq!(run("1_V UBASE"), ["1_kg*m^2/(A*s^3)"]);
        assert_eq!(run("1_W 1_J UFACT"), ["1_J/s"]);
        assert_eq!(run("1_kph 1_mph >"), ["0"]);
        assert_eq!(run("5 1_m →UNIT UVAL"), ["5"]);
        assert_eq!(run("RAD 30_° SIN"), [".5"]);
        assert_eq!(run("2_s INV"), [".5_1/s"]);
        assert_eq!(run("1_MeV 1_J CONVERT"), ["1.60217733E-13_J"]);
        let mut c = Calc::new();
        assert!(c.exec_line("1_m 1_s +").is_err());
        assert_eq!(c.stack.len(), 2);
        assert!(c.exec_line("1_xyz").is_err());
    }
}

//! Calculator objects, and how they are written on the display.

#[derive(Clone, Debug, PartialEq)]
pub enum Obj {
    Real(f64),
    Complex(f64, f64),
    Str(String),
    Name(String),
    List(Vec<Obj>),
    Prog(Vec<Node>),
}

/// One step of a program. Control structures are parsed into a tree once, when
/// the program is entered, instead of being scanned for their END each run.
#[derive(Clone, Debug, PartialEq)]
pub enum Node {
    Push(Obj),
    Word(String),
    If {
        cond: Vec<Node>,
        then: Vec<Node>,
        els: Option<Vec<Node>>,
    },
    Start {
        body: Vec<Node>,
        step: bool,
    },
    For {
        var: String,
        body: Vec<Node>,
        step: bool,
    },
    While {
        cond: Vec<Node>,
        body: Vec<Node>,
    },
    Do {
        body: Vec<Node>,
        cond: Vec<Node>,
    },
    Local {
        names: Vec<String>,
        body: Vec<Node>,
    },
}

/// Number display mode: STD, or FIX/SCI/ENG with a digit count.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Fmt {
    Std,
    Fix(u8),
    Sci(u8),
    Eng(u8),
}

/// Rounds to 12 significant digits, the precision of the HP 48's reals, so
/// that 0.1+0.2 is 0.3 and == behaves as it does on the calculator.
pub fn r12(x: f64) -> f64 {
    if !x.is_finite() || x == 0.0 {
        return x;
    }
    format!("{x:.11e}").parse().unwrap_or(x)
}

impl Obj {
    /// The object as it appears on the stack. With `Fmt::Std` this is also
    /// valid input that reads back as the same object.
    pub fn show(&self, fmt: Fmt) -> String {
        let mut out = String::new();
        write_obj(&mut out, self, fmt, true);
        out
    }
}

// Names are quoted ('X') on the stack and in programs, but written bare inside
// lists, as the calculator does: { A B }.
fn write_obj(out: &mut String, o: &Obj, fmt: Fmt, top: bool) {
    match o {
        Obj::Real(x) => out.push_str(&fmt_real(*x, fmt)),
        Obj::Complex(re, im) => {
            out.push('(');
            out.push_str(&fmt_real(*re, fmt));
            out.push(',');
            out.push_str(&fmt_real(*im, fmt));
            out.push(')');
        }
        Obj::Str(s) => {
            out.push('"');
            for c in s.chars() {
                match c {
                    '"' => out.push_str("\\\""),
                    '\\' => out.push_str("\\\\"),
                    '\n' => out.push_str("\\n"),
                    c => out.push(c),
                }
            }
            out.push('"');
        }
        Obj::Name(n) if top => {
            out.push('\'');
            out.push_str(n);
            out.push('\'');
        }
        Obj::Name(n) => out.push_str(n),
        Obj::List(items) => {
            out.push('{');
            for it in items {
                out.push(' ');
                write_obj(out, it, fmt, false);
            }
            out.push_str(" }");
        }
        Obj::Prog(nodes) => {
            out.push('«');
            write_nodes(out, nodes, fmt);
            out.push_str(" »");
        }
    }
}

fn write_nodes(out: &mut String, nodes: &[Node], fmt: Fmt) {
    for n in nodes {
        out.push(' ');
        write_node(out, n, fmt);
    }
}

fn write_node(out: &mut String, n: &Node, fmt: Fmt) {
    let next = |step: &bool| if *step { " STEP" } else { " NEXT" };
    match n {
        Node::Push(o) => write_obj(out, o, fmt, true),
        Node::Word(w) => out.push_str(w),
        Node::If { cond, then, els } => {
            out.push_str("IF");
            write_nodes(out, cond, fmt);
            out.push_str(" THEN");
            write_nodes(out, then, fmt);
            if let Some(els) = els {
                out.push_str(" ELSE");
                write_nodes(out, els, fmt);
            }
            out.push_str(" END");
        }
        Node::Start { body, step } => {
            out.push_str("START");
            write_nodes(out, body, fmt);
            out.push_str(next(step));
        }
        Node::For { var, body, step } => {
            out.push_str("FOR ");
            out.push_str(var);
            write_nodes(out, body, fmt);
            out.push_str(next(step));
        }
        Node::While { cond, body } => {
            out.push_str("WHILE");
            write_nodes(out, cond, fmt);
            out.push_str(" REPEAT");
            write_nodes(out, body, fmt);
            out.push_str(" END");
        }
        Node::Do { body, cond } => {
            out.push_str("DO");
            write_nodes(out, body, fmt);
            out.push_str(" UNTIL");
            write_nodes(out, cond, fmt);
            out.push_str(" END");
        }
        Node::Local { names, body } => {
            out.push('→');
            for n in names {
                out.push(' ');
                out.push_str(n);
            }
            out.push_str(" «");
            write_nodes(out, body, fmt);
            out.push_str(" »");
        }
    }
}

pub fn fmt_real(x: f64, fmt: Fmt) -> String {
    match fmt {
        Fmt::Std => std_real(x),
        Fmt::Fix(n) => fix_real(x, n as usize),
        Fmt::Sci(n) => sci_real(x, n as usize, false),
        Fmt::Eng(n) => sci_real(x, n as usize, true),
    }
}

/// Sign, decimal digits and exponent of `x` rounded to `sig` significant
/// digits. Rounding starts from the 12 decimal digits the real holds and takes
/// ties upward, as the calculator does (12345 in SCI 3 is 1.235E4), where
/// formatting the binary value would round 1.2345 to even.
fn digits(x: f64, sig: usize) -> (bool, Vec<u8>, i32) {
    let s = format!("{:.11e}", x.abs());
    let (m, e) = s.split_once('e').unwrap();
    let mut d: Vec<u8> = m
        .bytes()
        .filter(u8::is_ascii_digit)
        .map(|b| b - b'0')
        .collect();
    let mut e: i32 = e.parse().unwrap();
    if sig < d.len() {
        let up = d[sig] >= 5;
        d.truncate(sig);
        if up {
            match d.iter().rposition(|&x| x < 9) {
                Some(i) => {
                    d[i] += 1;
                    d[i + 1..].fill(0);
                }
                None => {
                    d.fill(0);
                    d[0] = 1;
                    e += 1;
                }
            }
        }
    }
    (x < 0.0, d, e)
}

fn ds(d: &[u8]) -> String {
    d.iter().map(|b| (b'0' + b) as char).collect()
}

fn sign(neg: bool) -> &'static str {
    if neg { "-" } else { "" }
}

// STD shows up to 12 significant digits, positionally from 1E-4 up to 12
// integer digits and with an exponent beyond, dropping the leading zero:
// .5, .0001, 1.E-5, 1.5E20.
fn std_real(x: f64) -> String {
    if x == 0.0 {
        return "0".into();
    }
    let (neg, mut d, e) = digits(x, 12);
    while d.len() > 1 && d.last() == Some(&0) {
        d.pop();
    }
    let body = if (0..12).contains(&e) {
        let e = e as usize;
        if d.len() <= e + 1 {
            ds(&d) + &"0".repeat(e + 1 - d.len())
        } else {
            format!("{}.{}", ds(&d[..=e]), ds(&d[e + 1..]))
        }
    } else if (-4..0).contains(&e) {
        format!(".{}{}", "0".repeat((-e - 1) as usize), ds(&d))
    } else {
        format!("{}.{}E{}", d[0], ds(&d[1..]), e)
    };
    format!("{}{body}", sign(neg))
}

// FIX falls back to SCI for numbers it can't show: too large, or nonzero but
// rounding to zero at this many places.
fn fix_real(x: f64, n: usize) -> String {
    if x != 0.0 && (x.abs() >= 1e12 || x.abs() < 0.5 * 10f64.powi(-(n as i32))) {
        return sci_real(x, n, false);
    }
    if x == 0.0 {
        return format!("0.{}", "0".repeat(n));
    }
    // Keep the digits down to the nth place. Below 10^-n that is none at all,
    // and the number (at least half of 10^-n, from the check above) rounds up.
    let places = digits(x, 12).2 + 1 + n as i32;
    if places <= 0 {
        return fix_real(10f64.powi(-(n as i32)).copysign(x), n);
    }
    let (neg, mut d, e) = digits(x, places as usize);
    let int = if e >= 0 {
        let k = e as usize + 1;
        d.resize(d.len().max(k), 0);
        d.drain(..k).collect()
    } else {
        d.splice(0..0, std::iter::repeat_n(0, (-e - 1) as usize));
        vec![]
    };
    d.resize(n, 0);
    format!("{}{}.{}", sign(neg), ds(&int), ds(&d))
}

// SCI and ENG show n+1 significant digits; ENG keeps the exponent a multiple of 3.
fn sci_real(x: f64, n: usize, eng: bool) -> String {
    let (neg, mut d, e) = if x == 0.0 {
        (false, vec![0; n + 1], 0)
    } else {
        digits(x, n + 1)
    };
    let shift = if eng { e.rem_euclid(3) as usize } else { 0 };
    while d.len() < shift + 1 {
        d.push(0);
    }
    format!(
        "{}{}.{}E{}",
        sign(neg),
        ds(&d[..=shift]),
        ds(&d[shift + 1..]),
        e - shift as i32
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn std_format() {
        let s = |x| fmt_real(r12(x), Fmt::Std);
        assert_eq!(s(10.0), "10");
        assert_eq!(s(0.5), ".5");
        assert_eq!(s(-0.5), "-.5");
        assert_eq!(s(1.0 / 3.0), ".333333333333");
        assert_eq!(s(2f64.sqrt()), "1.41421356237");
        assert_eq!(s(1e15), "1.E15");
        assert_eq!(s(1.5e20), "1.5E20");
        assert_eq!(s(1e-15), "1.E-15");
        assert_eq!(s(1e-5), "1.E-5");
        assert_eq!(s(0.0348994967025), ".0348994967025");
        assert_eq!(s(0.0001), ".0001");
        assert_eq!(s(123456789012.0), "123456789012");
        assert_eq!(s(0.1 + 0.2), ".3");
    }

    #[test]
    fn fix_sci_eng() {
        assert_eq!(fmt_real(1.255, Fmt::Fix(2)), "1.26");
        assert_eq!(fmt_real(0.5, Fmt::Fix(2)), ".50");
        assert_eq!(fmt_real(3.0, Fmt::Fix(0)), "3.");
        assert_eq!(fmt_real(9.996, Fmt::Fix(2)), "10.00");
        assert_eq!(fmt_real(-0.006, Fmt::Fix(2)), "-.01");
        assert_eq!(fmt_real(0.05, Fmt::Fix(3)), ".050");
        assert_eq!(fmt_real(0.0, Fmt::Fix(2)), "0.00");
        assert_eq!(fmt_real(0.0001, Fmt::Fix(2)), "1.00E-4");
        assert_eq!(fmt_real(12345.0, Fmt::Sci(3)), "1.235E4");
        assert_eq!(fmt_real(9.9996, Fmt::Sci(3)), "1.000E1");
        assert_eq!(fmt_real(12345.0, Fmt::Eng(2)), "12.3E3");
        assert_eq!(fmt_real(0.00012, Fmt::Eng(2)), "120.E-6");
    }
}

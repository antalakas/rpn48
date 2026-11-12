//! The rest of the commands: rounding and hours-minutes-seconds, probability,
//! tags, flags, errors, directories, variable arithmetic and the clock.

use crate::calc::{Calc, Error, OpFn, Res, one, op, pure_any, real, real_of, real2, truth};
use crate::obj::{Obj, r12};

/// x rounded (or with `trunc`, truncated) to n decimal places, or for a
/// negative n to -n significant digits, as RND and TRNC take it.
fn round(x: f64, n: f64, trunc: bool) -> Res<f64> {
    if n.fract() != 0.0 || !(-11.0..=11.0).contains(&n) {
        return Err(Error::BadValue);
    }
    if x == 0.0 {
        return Ok(0.0);
    }
    let places = if n >= 0.0 {
        n as i32
    } else {
        -n as i32 - 1 - x.abs().log10().floor() as i32
    };
    let k = 10f64.powi(places);
    let y = r12(x * k);
    Ok(r12(if trunc { y.trunc() } else { y.round() } / k))
}

fn exponent(x: f64) -> f64 {
    if x == 0.0 {
        0.0
    } else {
        format!("{:e}", r12(x))
            .split_once('e')
            .unwrap()
            .1
            .parse()
            .unwrap()
    }
}

/// H.MMSS to decimal hours.
fn from_hms(x: f64) -> f64 {
    let (s, x) = (x.signum(), x.abs());
    let h = x.trunc();
    let ms = r12((x - h) * 100.0);
    let m = ms.trunc();
    let sec = r12((ms - m) * 100.0);
    s * (h + m / 60.0 + sec / 3600.0)
}

/// Decimal hours to H.MMSS.
fn to_hms(x: f64) -> f64 {
    let (s, x) = (x.signum(), x.abs());
    let total = r12(x * 3600.0);
    let h = (total / 3600.0).trunc();
    let m = ((total - h * 3600.0) / 60.0).trunc();
    let sec = r12(total - h * 3600.0 - m * 60.0);
    s * (h + m / 100.0 + sec / 10000.0)
}

fn whole(x: f64) -> Res<u64> {
    if x < 0.0 || x.fract() != 0.0 {
        Err(Error::BadValue)
    } else {
        Ok(x as u64)
    }
}

fn comb(n: f64, r: f64, perm: bool) -> Res<f64> {
    let (n, r) = (whole(n)?, whole(r)?);
    if r > n {
        return Ok(0.0);
    }
    let mut x = 1.0;
    for i in 0..r {
        x *= (n - i) as f64;
        if !perm {
            x /= (i + 1) as f64;
        }
    }
    Ok(x.round())
}

pub fn pure(w: &str) -> Option<(usize, OpFn)> {
    use Obj::*;
    match w {
        "RND" => op(2, |_, a| {
            one(real(round(real_of(&a[0])?, real_of(&a[1])?, false)?))
        }),
        "TRNC" => op(2, |_, a| {
            one(real(round(real_of(&a[0])?, real_of(&a[1])?, true)?))
        }),
        "MANT" => op(1, |_, a| {
            let x = real_of(&a[0])?;
            one(real(x.abs() / 10f64.powf(exponent(x))))
        }),
        "XPON" => op(1, |_, a| one(real(exponent(real_of(&a[0])?)))),
        "→HMS" => op(1, |_, a| one(real(to_hms(real_of(&a[0])?)))),
        "HMS→" => op(1, |_, a| one(real(from_hms(real_of(&a[0])?)))),
        "HMS+" => op(2, |_, a| real2(a, |x, y| to_hms(from_hms(x) + from_hms(y)))),
        "HMS-" => op(2, |_, a| real2(a, |x, y| to_hms(from_hms(x) - from_hms(y)))),
        "COMB" => op(2, |_, a| {
            one(real(comb(real_of(&a[0])?, real_of(&a[1])?, false)?))
        }),
        "PERM" => op(2, |_, a| {
            one(real(comb(real_of(&a[0])?, real_of(&a[1])?, true)?))
        }),
        "→TAG" => op(2, |c, a| {
            let t = match &a[1] {
                Str(s) => s.clone(),
                o => c.str_of(o),
            };
            if t.is_empty() || t.contains(':') {
                return Err(Error::BadValue);
            }
            Ok(vec![Tag(t, Box::new(a[0].clone()))])
        }),
        "DTAG" => op(1, |_, a| {
            Ok(vec![match &a[0] {
                Tag(_, o) => (**o).clone(),
                o => o.clone(),
            }])
        }),
        _ => None,
    }
}

/// Seconds since 1970 and the local time's broken-down fields.
fn now() -> (f64, libc::tm) {
    let t = crate::alarms::now();
    let secs = t as libc::time_t;
    // SAFETY: localtime_r only writes the tm it is given.
    let tm = unsafe {
        let mut tm: libc::tm = std::mem::zeroed();
        libc::localtime_r(&secs, &mut tm);
        tm
    };
    (t, tm)
}

impl Calc {
    fn flag_bit(&self, n: f64) -> Res<(usize, u64)> {
        if n.fract() != 0.0 || n == 0.0 || n.abs() > 64.0 {
            return Err(Error::BadValue);
        }
        Ok((usize::from(n < 0.0), 1 << (n.abs() as u32 - 1)))
    }

    /// x 'name' STO+ and the like: the variable becomes var + x, or for
    /// 'name' x, x + var; RCL'd and stored back.
    fn sto_op(&mut self, cmd: &str) -> Res<()> {
        self.need(2)?;
        let (name, x, name_first) = match (self.peek(2), self.peek(1)) {
            (Obj::Name(n), x) => (n.clone(), x.clone(), true),
            (x, Obj::Name(n)) => (n.clone(), x.clone(), false),
            _ => return Err(Error::BadType),
        };
        let v = self.global(&name).cloned().ok_or(Error::Undefined)?;
        let (_, f) = pure_any(cmd).unwrap();
        let args = if name_first { [v, x] } else { [x, v] };
        let r = self.compute(cmd, f, &args)?;
        let [r] = &r[..] else {
            return Err(Error::BadType);
        };
        self.store(name, r.clone())?;
        self.stack.truncate(self.stack.len() - 2);
        Ok(())
    }

    /// The next random number in [0, 1), to 12 digits.
    pub(crate) fn random(&mut self) -> f64 {
        if self.seed == 0 {
            self.seed = now().0.to_bits() | 1;
        }
        // xorshift64*, taking 12 digits of the result.
        let mut x = self.seed;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.seed = x;
        let r = (x.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 11) as f64 / (1u64 << 53) as f64;
        (r * 1e12).floor() / 1e12
    }

    pub(crate) fn more_cmd(&mut self, w: &str) -> Option<Res<()>> {
        if let Some(r) = self
            .binary_cmd(w)
            .or_else(|| self.array_cmd(w))
            .or_else(|| self.matrix_cmd(w))
        {
            return Some(r);
        }
        Some(match w {
            "LASTARG" => {
                let a = self.last_args.clone();
                self.stack.extend(a);
                Ok(())
            }
            "ERRN" => {
                let n = self.last_err.as_ref().map_or(0, Error::number);
                self.stack.push(Obj::Bin(n as u64));
                Ok(())
            }
            "ERRM" => {
                let m = self
                    .last_err
                    .as_ref()
                    .map_or(String::new(), |e| e.bare().to_string());
                self.stack.push(Obj::Str(m));
                Ok(())
            }
            "ERR0" => {
                self.last_err = None;
                Ok(())
            }
            "DOERR" => self.need(1).and_then(|_| {
                let e = match self.peek(1) {
                    Obj::Str(s) => Error::User(s.clone()),
                    Obj::Real(n) => Error::from_number(*n as u32),
                    Obj::Bin(n) => Error::from_number(*n as u32),
                    _ => return Err(Error::BadType),
                };
                self.stack.pop();
                Err(e)
            }),
            "SF" | "CF" | "FS?" | "FC?" | "FS?C" | "FC?C" => self.need(1).and_then(|_| {
                let Obj::Real(n) = *self.peek(1) else {
                    return Err(Error::BadType);
                };
                let (i, bit) = self.flag_bit(n)?;
                self.stack.pop();
                let set = self.flags[i] & bit != 0;
                match w {
                    "SF" => self.flags[i] |= bit,
                    "CF" | "FS?C" | "FC?C" => self.flags[i] &= !bit,
                    _ => {}
                }
                match w {
                    "FS?" | "FS?C" => self.stack.push(truth(set)),
                    "FC?" | "FC?C" => self.stack.push(truth(!set)),
                    _ => {}
                }
                Ok(())
            }),
            "CRDIR" => self.need(1).and_then(|_| {
                let Obj::Name(n) = self.peek(1).clone() else {
                    return Err(Error::BadType);
                };
                if self.cur_vars().iter().any(|(v, _)| *v == n) {
                    return Err(Error::BadValue);
                }
                self.stack.pop();
                self.cur_vars_mut().insert(0, (n, Obj::Dir(vec![])));
                Ok(())
            }),
            "PGDIR" => self.need(1).and_then(|_| {
                let Obj::Name(n) = self.peek(1).clone() else {
                    return Err(Error::BadType);
                };
                let vars = self.cur_vars_mut();
                match vars
                    .iter()
                    .position(|(v, o)| *v == n && matches!(o, Obj::Dir(_)))
                {
                    Some(i) => {
                        vars.remove(i);
                        self.stack.pop();
                        Ok(())
                    }
                    None => Err(Error::BadValue),
                }
            }),
            "UPDIR" => {
                self.path.pop();
                Ok(())
            }
            "HOME" => {
                self.path.clear();
                Ok(())
            }
            "PATH" => {
                let mut p = vec![Obj::Name("HOME".into())];
                p.extend(self.path.iter().map(|n| Obj::Name(n.clone())));
                self.stack.push(Obj::List(p));
                Ok(())
            }
            "STO+" => self.sto_op("+"),
            "STO-" => self.sto_op("-"),
            "STO*" => self.sto_op("*"),
            "STO/" => self.sto_op("/"),
            "INCR" | "DECR" => self.need(1).and_then(|_| {
                let Obj::Name(n) = self.peek(1).clone() else {
                    return Err(Error::BadType);
                };
                let v = self.global(&n).cloned().ok_or(Error::Undefined)?;
                let d = Obj::Real(if w == "INCR" { 1.0 } else { -1.0 });
                let r = self.num2('+', &v, &d)?;
                self.store(n, r.clone())?;
                self.stack.pop();
                self.stack.push(r);
                Ok(())
            }),
            "RAND" => {
                let r = self.random();
                self.stack.push(Obj::Real(r));
                Ok(())
            }
            "RDZ" => self.need(1).and_then(|_| {
                let Obj::Real(x) = *self.peek(1) else {
                    return Err(Error::BadType);
                };
                self.stack.pop();
                self.seed = if x == 0.0 {
                    now().0.to_bits() | 1
                } else {
                    x.to_bits() | 1
                };
                Ok(())
            }),
            // TIME is HH.MMSS, DATE is MM.DDYYYY, TICKS counts 1/8192 seconds.
            "TIME" => {
                let tm = now().1;
                let t = tm.tm_hour as f64 + tm.tm_min as f64 / 100.0 + tm.tm_sec as f64 / 10000.0;
                self.stack.push(Obj::Real(r12(t)));
                Ok(())
            }
            "DATE" => {
                let tm = now().1;
                let d = (tm.tm_mon + 1) as f64
                    + tm.tm_mday as f64 / 100.0
                    + (tm.tm_year + 1900) as f64 / 1e6;
                self.stack.push(Obj::Real(r12(d)));
                Ok(())
            }
            "TICKS" => {
                self.stack.push(Obj::Bin((now().0 * 8192.0) as u64));
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
    fn reals() {
        assert_eq!(
            run("3.14159 2 RND  3.14159 -3 RND  3.789 1 TRNC"),
            ["3.14", "3.14", "3.7"]
        );
        assert_eq!(run("12345 MANT 12345 XPON"), ["1.2345", "4"]);
        assert_eq!(
            run("1.5 →HMS  1.3 HMS→  1.3 2.45 HMS+"),
            ["1.3", "1.5", "4.15"]
        );
        assert_eq!(run("5 2 COMB 5 2 PERM"), ["10", "20"]);
    }

    #[test]
    fn programs() {
        // As on the calculator, the failed command's arguments are put back.
        assert_eq!(
            run("« IFERR 1 0 / THEN ERRM ERRN END » EVAL"),
            ["1", "0", "\"Infinite Result\"", "# 773d"]
        );
        assert_eq!(
            run("« IFERR \"oops\" DOERR THEN ERRM END » EVAL"),
            ["\"oops\""]
        );
        assert_eq!(
            run(
                "« → x « CASE x 0 < THEN \"neg\" END x 0 == THEN \"zero\" END \"pos\" END » » 'S' STO -1 S 0 S 5 S"
            ),
            ["\"neg\"", "\"zero\"", "\"pos\""]
        );
        assert_eq!(run("5 \"A\" →TAG  DUP 1 +"), [":A:5", "6"]);
        assert_eq!(run("3 SF 3 FS? 3 FC?C 3 FS?"), ["1", "0", "0"]);
        assert_eq!(run("1 2 + LASTARG"), ["3", "1", "2"]);
        assert_eq!(run("5 'X' STO 2 'X' STO+ 'X' INCR X"), ["8", "8"]);
    }

    #[test]
    fn directories() {
        assert_eq!(
            run("1 'A' STO 'D' CRDIR D 2 'B' STO PATH A B VARS UPDIR VARS"),
            ["{ HOME D }", "1", "2", "{ B }", "{ D A }"]
        );
        assert_eq!(run("'D' CRDIR D 7 'X' STO HOME X"), ["'X'"]);
    }
}

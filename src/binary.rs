//! Binary integers: # FFh and the commands of the MTH BASE menu.

use crate::calc::{Calc, Error, OpFn, Res, op, truth};
use crate::obj::{Base, Obj};

impl Calc {
    fn mask(&self) -> u64 {
        if self.wordsize >= 64 {
            u64::MAX
        } else {
            (1 << self.wordsize) - 1
        }
    }

    fn bin(&self, n: u64) -> Obj {
        Obj::Bin(n & self.mask())
    }

    fn top_bit(&self) -> u64 {
        1 << (self.wordsize - 1)
    }

    pub(crate) fn binary_cmd(&mut self, w: &str) -> Option<Res<()>> {
        let b = match w {
            "HEX" => Base::Hex,
            "DEC" => Base::Dec,
            "OCT" => Base::Oct,
            "BIN" => Base::Bin,
            "STWS" => {
                return Some(self.need(1).and_then(|_| {
                    let n = match *self.peek(1) {
                        Obj::Real(x) => x,
                        Obj::Bin(n) => n as f64,
                        _ => return Err(Error::BadType),
                    };
                    self.stack.pop();
                    self.wordsize = n.clamp(1.0, 64.0) as u32;
                    Ok(())
                }));
            }
            "RCWS" => {
                self.stack.push(Obj::Real(self.wordsize as f64));
                return Some(Ok(()));
            }
            _ => return None,
        };
        self.set_base(b);
        Some(Ok(()))
    }
}

/// A real as a binary integer: its integer part, saturating at the ends.
fn from_real(x: f64) -> u64 {
    if x <= 0.0 { 0 } else { x as u64 }
}

fn int_of(o: &Obj) -> Res<u64> {
    match o {
        Obj::Bin(n) => Ok(*n),
        Obj::Real(x) => Ok(from_real(*x)),
        _ => Err(Error::BadType),
    }
}

/// Arithmetic with a binary integer on at least one side; a real on the
/// other side is taken as a binary integer.
pub fn arith(c: &Calc, o: char, a: &Obj, b: &Obj) -> Res<Obj> {
    let (x, y) = (int_of(a)?, int_of(b)?);
    Ok(c.bin(match o {
        '+' => x.wrapping_add(y),
        '-' => x.wrapping_sub(y),
        '*' => x.wrapping_mul(y),
        _ => {
            if y == 0 {
                return Err(if x == 0 {
                    Error::UndefinedResult
                } else {
                    Error::Infinite
                });
            }
            x / y
        }
    }))
}

/// Commands that act bitwise, or compare, when given binary integers.
pub fn over(c: &Calc, w: &str, a: &[Obj]) -> Option<Res<Vec<Obj>>> {
    const OVER: &[&str] = &[
        "NEG", "NOT", "AND", "OR", "XOR", "<", ">", "≤", "≥", "==", "≠",
    ];
    if !OVER.contains(&w) || !a.iter().any(|o| matches!(o, Obj::Bin(_))) {
        return None;
    }
    let v = match a.iter().map(int_of).collect::<Res<Vec<u64>>>() {
        Ok(v) => v,
        Err(e) => return Some(Err(e)),
    };
    let (x, y) = (v[0] & c.mask(), v.get(1).map_or(0, |y| y & c.mask()));
    Some(Ok(vec![match w {
        "NEG" => c.bin(x.wrapping_neg()),
        "NOT" => c.bin(!x),
        "AND" => c.bin(x & y),
        "OR" => c.bin(x | y),
        "XOR" => c.bin(x ^ y),
        "<" => truth(x < y),
        ">" => truth(x > y),
        "≤" => truth(x <= y),
        "≥" => truth(x >= y),
        "==" => truth(x == y),
        _ => truth(x != y),
    }]))
}

macro_rules! bitop {
    ($f:expr) => {
        op(1, |c, a| match a[0] {
            Obj::Bin(n) => Ok(vec![c.bin($f(c, n & c.mask()))]),
            _ => Err(Error::BadType),
        })
    };
}

pub fn pure(w: &str) -> Option<(usize, OpFn)> {
    match w {
        "R→B" => op(1, |c, a| match a[0] {
            Obj::Real(x) => Ok(vec![c.bin(from_real(x))]),
            _ => Err(Error::BadType),
        }),
        "B→R" => op(1, |_, a| match a[0] {
            Obj::Bin(n) => Ok(vec![Obj::Real(n as f64)]),
            _ => Err(Error::BadType),
        }),
        "SL" => bitop!(|_, n: u64| n << 1),
        "SR" => bitop!(|_, n: u64| n >> 1),
        "SLB" => bitop!(|_, n: u64| n << 8),
        "SRB" => bitop!(|_, n: u64| n >> 8),
        "ASR" => bitop!(|c: &Calc, n: u64| (n >> 1) | (n & c.top_bit())),
        "RL" => bitop!(|c: &Calc, n| rot(n, 1, c.wordsize)),
        "RR" => bitop!(|c: &Calc, n| rot(n, c.wordsize - 1, c.wordsize)),
        "RLB" => bitop!(|c: &Calc, n| rot(n, 8 % c.wordsize, c.wordsize)),
        "RRB" => bitop!(|c: &Calc, n| {
            let ws = c.wordsize;
            rot(n, (ws - 8 % ws) % ws, ws)
        }),
        _ => None,
    }
}

/// Rotates the low `ws` bits of `n` left by `k`.
fn rot(n: u64, k: u32, ws: u32) -> u64 {
    if k == 0 || ws == 0 {
        return n;
    }
    if ws == 64 {
        return n.rotate_left(k);
    }
    let m = (1u64 << ws) - 1;
    ((n << k) | (n >> (ws - k))) & m
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
    fn binary_integers() {
        assert_eq!(run("HEX #FFh #1h +"), ["# 100h"]);
        assert_eq!(run("#12 #5 -"), ["# 7d"]);
        assert_eq!(run("BIN #1010b SL"), ["# 10100b"]);
        assert_eq!(run("HEX #F0h #3Ch AND"), ["# 30h"]);
        assert_eq!(run("8 STWS HEX #FFh 1 +"), ["# 0h"]);
        assert_eq!(run("8 STWS HEX #81h RL"), ["# 3h"]);
        assert_eq!(run("8 STWS HEX #81h RR"), ["# C0h"]);
        assert_eq!(run("8 STWS HEX #80h ASR"), ["# C0h"]);
        assert_eq!(run("16 STWS HEX #1234h RLB"), ["# 3412h"]);
        assert_eq!(run("255 R→B HEX"), ["# FFh"]);
        assert_eq!(run("#10d B→R"), ["10"]);
        assert_eq!(run("8 STWS #1d NEG HEX"), ["# FFh"]);
        assert_eq!(run("#3d #5d <"), ["1"]);
    }
}

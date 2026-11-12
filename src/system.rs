//! Memory and system commands: clearing and ordering variables, memory and
//! object sizes, the flags as binary integers, menus by number, and storing
//! the negation, inverse or conjugate of a variable.

use crate::calc::{Calc, Error, Res, pure_any};
use crate::menu::Menu;
use crate::obj::{Fmt, Obj};

/// The memory rpn48 reports as a 128K HP 48 GX's.
const MEMORY: f64 = 131_072.0;

/// An object's size in bytes, following the calculator's object sizes
/// (a real is 10.5 bytes, a string 5 plus its characters, and so on).
pub fn bytes(o: &Obj) -> f64 {
    match o {
        Obj::Real(_) => 10.5,
        Obj::Complex(..) => 18.5,
        Obj::Bin(_) => 10.0,
        Obj::Str(s) => 5.0 + s.len() as f64,
        Obj::Name(n) => 3.5 + n.len() as f64,
        Obj::List(l) => 5.0 + l.iter().map(bytes).sum::<f64>(),
        Obj::Vect(v) => 12.5 + 8.0 * v.len() as f64,
        Obj::Mat(m) => 15.0 + 8.0 * (m.len() * m[0].len()) as f64,
        Obj::CVect(v) => 12.5 + 16.0 * v.len() as f64,
        Obj::CMat(m) => 15.0 + 16.0 * (m.len() * m[0].len()) as f64,
        Obj::Tag(t, o) => 3.5 + t.len() as f64 + bytes(o),
        Obj::Dir(vars) => {
            6.5 + vars
                .iter()
                .map(|(n, o)| 6.0 + n.len() as f64 + bytes(o))
                .sum::<f64>()
        }
        // Programs, algebraics and units as written out.
        o => 5.0 + 2.5 * o.show(Fmt::Std).chars().count() as f64,
    }
}

/// CRC-16/CCITT of the object written out, standing in for the
/// calculator's checksum of its memory image.
fn checksum(o: &Obj) -> u64 {
    let mut crc: u16 = 0;
    for b in o.show(Fmt::Std).bytes() {
        crc ^= u16::from(b) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 {
                (crc << 1) ^ 0x1021
            } else {
                crc << 1
            };
        }
    }
    u64::from(crc)
}

/// The menu numbers of MENU and RCLMENU: CST is 1, VAR 2 and MTH 3, as on
/// the calculator, and the rest follow in rpn48's own order.
fn menu_number(m: Menu) -> f64 {
    match m {
        Menu::Cst | Menu::Tmenu => 1.0,
        Menu::Var => 2.0,
        m => {
            let rest = Menu::ALL
                .iter()
                .filter(|m| !matches!(m, Menu::Cst | Menu::Var));
            match rest.clone().position(|x| *x == m) {
                Some(i) => 3.0 + i as f64,
                // The unit categories come after the fixed menus.
                None => match m {
                    Menu::UnitCat(i) => 3.0 + rest.count() as f64 + i as f64,
                    _ => 0.0,
                },
            }
        }
    }
}

fn menu_of(n: usize) -> Option<Menu> {
    (0..)
        .take(200)
        .map(|i| {
            if i < Menu::ALL.len() {
                Menu::ALL[i]
            } else {
                Menu::UnitCat((i - Menu::ALL.len()) as u8)
            }
        })
        .filter(|m| !matches!(m, Menu::UnitCat(i) if *i as usize >= crate::units::CATEGORIES.len()))
        .find(|m| menu_number(*m) as usize == n)
}

impl Calc {
    /// 'name' SNEG and the like: the variable is replaced by `cmd` of it.
    fn sto_fn(&mut self, cmd: &str) -> Res<()> {
        self.need(1)?;
        let Obj::Name(n) = self.peek(1).clone() else {
            return Err(Error::BadType);
        };
        let v = self.global(&n).cloned().ok_or(Error::Undefined)?;
        let (_, f) = pure_any(cmd).unwrap();
        let r = self.compute(cmd, f, &[v])?;
        let [r] = &r[..] else {
            return Err(Error::BadType);
        };
        self.store(n, r.clone())?;
        self.stack.pop();
        Ok(())
    }

    pub(crate) fn system_cmd(&mut self, w: &str) -> Option<Res<()>> {
        Some(match w {
            // Every variable in the current directory, and its empty
            // subdirectories.
            "CLVAR" | "CLUSR" => {
                self.cur_vars_mut()
                    .retain(|(_, o)| matches!(o, Obj::Dir(v) if !v.is_empty()));
                Ok(())
            }
            "MEM" => {
                let used: f64 = self
                    .vars
                    .iter()
                    .map(|(n, o)| 6.0 + n.len() as f64 + bytes(o))
                    .sum::<f64>()
                    + self.stack.iter().map(bytes).sum::<f64>();
                self.stack.push(Obj::Real((MEMORY - used).max(0.0).floor()));
                Ok(())
            }
            "BYTES" => self.pop().map(|o| {
                let (sum, size) = (checksum(&o), bytes(&o));
                self.last_args = vec![o];
                self.stack.push(Obj::Bin(sum));
                self.stack.push(Obj::Real(size));
            }),
            "NEWOB" => self.need(1),
            "VERSION" => {
                self.stack
                    .push(Obj::Str(format!("rpn48 {}", env!("CARGO_PKG_VERSION"))));
                self.stack.push(Obj::Str("MIT License".into()));
                Ok(())
            }
            // { #system #user } RCLF, flags -1 to -64 and 1 to 64 as bits.
            "RCLF" => {
                let [user, system] = self.flags;
                self.stack
                    .push(Obj::List(vec![Obj::Bin(system), Obj::Bin(user)]));
                Ok(())
            }
            "STOF" => self.need(1).and_then(|_| {
                match self.peek(1) {
                    Obj::Bin(s) => self.flags[1] = *s,
                    Obj::List(l) => match &l[..] {
                        [Obj::Bin(s)] => self.flags[1] = *s,
                        [Obj::Bin(s), Obj::Bin(u)] => self.flags = [*u, *s],
                        _ => return Err(Error::BadValue),
                    },
                    _ => return Err(Error::BadType),
                }
                self.stack.pop();
                Ok(())
            }),
            // obj n NDUPN: n copies of obj, and n.
            "NDUPN" => self.need(2).and_then(|_| {
                let Obj::Real(n) = *self.peek(1) else {
                    return Err(Error::BadType);
                };
                if n < 0.0 || n.fract() != 0.0 {
                    return Err(Error::BadValue);
                }
                self.stack.pop();
                let o = self.pop()?;
                self.stack.extend(std::iter::repeat_n(o, n as usize));
                self.stack.push(Obj::Real(n));
                Ok(())
            }),
            // { names } ORDER puts those variables first in the directory.
            "ORDER" => self.need(1).and_then(|_| {
                let names: Vec<String> = match self.peek(1) {
                    Obj::List(l) => l
                        .iter()
                        .map(|o| match o {
                            Obj::Name(n) => Ok(n.clone()),
                            _ => Err(Error::BadType),
                        })
                        .collect::<Res<_>>()?,
                    _ => return Err(Error::BadType),
                };
                let vars = self.cur_vars_mut();
                let mut first = vec![];
                for n in &names {
                    if let Some(i) = vars.iter().position(|(v, _)| v == n) {
                        first.push(vars.remove(i));
                    }
                }
                first.append(vars);
                *vars = first;
                self.stack.pop();
                Ok(())
            }),
            // 'name' VTYPE: the type of what the variable holds, -1 if nothing.
            "VTYPE" => self.need(1).and_then(|_| {
                let Obj::Name(n) = self.peek(1).clone() else {
                    return Err(Error::BadType);
                };
                self.stack.pop();
                match self.global(&n).cloned() {
                    Some(o) => {
                        self.stack.push(o);
                        self.eval_word("TYPE")
                    }
                    None => {
                        self.stack.push(Obj::Real(-1.0));
                        Ok(())
                    }
                }
            }),
            "SNEG" => self.sto_fn("NEG"),
            "SINV" => self.sto_fn("INV"),
            "SCONJ" => self.sto_fn("CONJ"),
            // Kermit is not there, so it never has an error to report.
            "KERRM" => {
                self.stack.push(Obj::Str(String::new()));
                Ok(())
            }
            // n MENU shows menu n, its fractional part the page; a list (or a
            // name holding one) becomes CST and is shown.
            "MENU" | "TMENU" => self.need(1).and_then(|_| {
                match self.peek(1).clone() {
                    Obj::Real(n) if w == "MENU" => {
                        let page = ((n.fract() * 100.0).round() as usize).saturating_sub(1);
                        match n.trunc() as usize {
                            0 => {}
                            k => self.menu = menu_of(k).ok_or(Error::BadValue)?,
                        }
                        self.page = page;
                    }
                    o @ (Obj::List(_) | Obj::Name(_)) => {
                        let items = match &o {
                            Obj::Name(n) => self.global(n).cloned().ok_or(Error::Undefined)?,
                            o => o.clone(),
                        };
                        let Obj::List(items) = items else {
                            return Err(Error::BadType);
                        };
                        if w == "MENU" {
                            self.store("CST".into(), o)?;
                            self.menu = Menu::Cst;
                        } else {
                            self.tmenu = items;
                            self.menu = Menu::Tmenu;
                        }
                        self.page = 0;
                    }
                    _ => return Err(Error::BadType),
                }
                self.stack.pop();
                Ok(())
            }),
            "RCLMENU" => {
                let n = menu_number(self.menu) + (self.page + 1) as f64 / 100.0;
                self.stack
                    .push(Obj::Real(if self.page == 0 { n.trunc() } else { n }));
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
    fn variables() {
        assert_eq!(run("1 'A' STO 2 'B' STO 'D' CRDIR CLVAR VARS"), ["{ }"]);
        assert_eq!(
            run("1 'A' STO 2 'B' STO 3 'C' STO { C A } ORDER VARS"),
            ["{ C A B }"]
        );
        assert_eq!(run("5 'A' STO 'A' SNEG 'A' SINV A"), ["-.2"]);
        assert_eq!(run("(1,2) 'Z' STO 'Z' SCONJ Z"), ["(1,-2)"]);
        assert_eq!(run("\"x\" 'S' STO 'S' VTYPE 'Q' VTYPE"), ["2", "-1"]);
    }

    #[test]
    fn system() {
        assert_eq!(run("7 3 NDUPN"), ["7", "7", "7", "3"]);
        assert_eq!(
            run("1.5 BYTES SWAP DROP \"AB\" BYTES SWAP DROP"),
            ["10.5", "7"]
        );
        assert_eq!(run("3 SF -2 SF RCLF"), ["{ # 2d # 4d }"]);
        assert_eq!(
            run("{ # 1d # 1d } STOF 1 FS? -1 FS? 2 FS?"),
            ["1", "1", "0"]
        );
        assert_eq!(run("MEM 100000 >"), ["1"]);
        assert_eq!(run("1.02 MENU RCLMENU 3 MENU RCLMENU"), ["1.02", "3"]);
        assert_eq!(run("{ A B } TMENU RCLMENU 'CST' VTYPE"), ["1", "-1"]);
        assert_eq!(run("{ A B } MENU 'CST' RCL"), ["{ A B }"]);
    }
}

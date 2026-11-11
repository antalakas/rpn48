//! The USER keyboard: objects assigned to keys with ASN or STOKEYS, which
//! the keys run instead of their own functions while USER mode (system
//! flag -62) is on. Keys are named by code, rc.p: row, column and plane (1
//! unshifted, 2 left-shifted, 3 right-shifted).

use crate::calc::{Calc, Error, Res};
use crate::obj::{Obj, r12};

/// Characters the keyboard lacks, for the CHARS menu to type.
pub const CHARS: &[&str] = &[
    "«", "»", "→", "∠", "√", "π", "Σ", "∂", "∫", "≤", "≥", "≠", "°", "μ", "Å", "Ω", "Δ", "Π", "∞",
    "_", "#", "&", "@", "~", "\\", "|", "?", "$", "%", ";",
];

/// The bit of system flag -62, USER mode.
const USER: u64 = 1 << 61;

/// A key code with its plane, rc.0 and rc.1 both meaning unshifted.
fn normal(code: f64) -> Res<f64> {
    let (rc, p) = (code.trunc(), r12(code.fract() * 10.0).round());
    if !(11.0..=99.0).contains(&rc) || !(0.0..=3.0).contains(&p) {
        return Err(Error::BadValue);
    }
    Ok(rc + p.max(1.0) / 10.0)
}

impl Calc {
    pub fn user_mode(&self) -> bool {
        self.flags[1] & USER != 0
    }

    pub fn toggle_user(&mut self) {
        self.flags[1] ^= USER;
    }

    /// What is assigned to the key with code rc.p.
    pub fn user_key(&self, code: f64) -> Option<Obj> {
        let code = normal(code).ok()?;
        self.user_keys
            .iter()
            .find(|(c, _)| *c == code)
            .map(|(_, o)| o.clone())
    }

    fn assign(&mut self, o: Obj, code: f64) -> Res<()> {
        let code = normal(code)?;
        self.user_keys.retain(|(c, _)| *c != code);
        self.user_keys.push((code, o));
        Ok(())
    }

    pub(crate) fn keys_cmd(&mut self, w: &str) -> Option<Res<()>> {
        Some(match w {
            // obj code ASN assigns obj to the key.
            "ASN" => self.need(2).and_then(|_| {
                let Obj::Real(code) = *self.peek(1) else {
                    return Err(Error::BadType);
                };
                let o = self.peek(2).clone();
                self.assign(o, code)?;
                self.stack.truncate(self.stack.len() - 2);
                Ok(())
            }),
            // { obj code obj code … } STOKEYS.
            "STOKEYS" => self.need(1).and_then(|_| {
                let Obj::List(l) = self.peek(1).clone() else {
                    return Err(Error::BadType);
                };
                if l.len() % 2 != 0 {
                    return Err(Error::BadValue);
                }
                for pair in l.chunks(2) {
                    let Obj::Real(code) = pair[1] else {
                        return Err(Error::BadType);
                    };
                    normal(code)?;
                }
                for pair in l.chunks(2) {
                    let Obj::Real(code) = pair[1] else {
                        unreachable!()
                    };
                    self.assign(pair[0].clone(), code)?;
                }
                self.stack.pop();
                Ok(())
            }),
            "RCLKEYS" => {
                let l = self
                    .user_keys
                    .iter()
                    .flat_map(|(c, o)| [o.clone(), Obj::Real(*c)])
                    .collect();
                self.stack.push(Obj::List(l));
                Ok(())
            }
            // code DELKEYS, or 0 DELKEYS for them all.
            "DELKEYS" => self.need(1).and_then(|_| {
                let Obj::Real(code) = *self.peek(1) else {
                    return Err(Error::BadType);
                };
                if code == 0.0 {
                    self.user_keys.clear();
                } else {
                    let code = normal(code)?;
                    self.user_keys.retain(|(c, _)| *c != code);
                }
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

    #[test]
    fn user_keys() {
        let mut c = Calc::new();
        c.exec_line(
            "« 2 * » 11 ASN { SIN 12.2 COS 13.3 } STOKEYS 'TAN' 11.1 ASN 13.3 DELKEYS RCLKEYS",
        )
        .unwrap();
        assert_eq!(c.stack[0].show(c.fmt), "{ SIN 12.2 TAN 11.1 }");
        assert_eq!(c.user_key(11.0).unwrap().show(c.fmt), "'TAN'");
        assert!(c.user_key(13.3).is_none());
        assert!(!c.user_mode());
        c.exec_line("-62 SF").unwrap();
        assert!(c.user_mode());
        assert!(c.exec_line("1 100 ASN").is_err());
    }
}

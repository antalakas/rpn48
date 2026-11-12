//! Ports and their backup objects, as on a 48 GX with a RAM card in each
//! slot: port 0 is part of main memory, ports 1 and 2 are 128K cards, and
//! the card in port 1 can be merged into main memory. A backup object is
//! named with its port as a tag: obj :1:NAME STO puts it there, :1:NAME RCL
//! and EVAL use it, :&:NAME looks in every port. Also OFF, and WSLOG.

use crate::calc::{Calc, Error, Res};
use crate::obj::Obj;
use crate::system::bytes;

pub const PORTS: usize = 3;

/// The size of the card in ports 1 and 2.
pub const CARD: f64 = 131_072.0;

/// Where a tagged name points: a port, or every port for &.
enum Port {
    One(usize),
    Any,
}

/// The port and name of :n:NAME, if `o` is one.
fn port_name(o: &Obj) -> Option<(Port, String)> {
    let Obj::Tag(t, inner) = o else {
        return None;
    };
    let Obj::Name(n) = &**inner else {
        return None;
    };
    let p = match t.as_str() {
        "&" => Port::Any,
        t => Port::One(t.parse().ok().filter(|p| *p < PORTS)?),
    };
    Some((p, n.clone()))
}

fn not_available() -> Error {
    Error::User("Port Not Available".into())
}

/// The memory a port's backup objects take.
pub fn used(objs: &[(String, Obj)]) -> f64 {
    objs.iter()
        .map(|(n, o)| 6.0 + n.len() as f64 + bytes(o))
        .sum()
}

impl Calc {
    fn port(&self, p: usize) -> Res<&Vec<(String, Obj)>> {
        if p == 1 && self.merged {
            return Err(not_available());
        }
        Ok(&self.ports[p])
    }

    /// The backup object :p:name, looking in each port in turn for &.
    fn backup(&self, p: &Port, name: &str) -> Res<(usize, Obj)> {
        let ports: Vec<usize> = match p {
            Port::One(p) => vec![*p],
            Port::Any => (0..PORTS).filter(|&p| self.port(p).is_ok()).collect(),
        };
        for p in ports {
            if let Some((_, o)) = self.port(p)?.iter().find(|(n, _)| n == name) {
                return Ok((p, o.clone()));
            }
        }
        Err(Error::Undefined)
    }

    /// Puts a backup object in port p, if there is room on its card.
    fn store_backup(&mut self, p: usize, name: String, o: Obj) -> Res<()> {
        self.port(p)?;
        let objs = &mut self.ports[p];
        objs.retain(|(n, _)| *n != name);
        if p > 0 && used(objs) + 6.0 + name.len() as f64 + bytes(&o) > CARD {
            return Err(Error::Memory);
        }
        objs.push((name, o));
        Ok(())
    }

    /// The port number on level 1.
    fn port_arg(&self) -> Res<usize> {
        self.need(1)?;
        match *self.peek(1) {
            Obj::Real(p) if p >= 0.0 && p.fract() == 0.0 && (p as usize) < PORTS => Ok(p as usize),
            Obj::Real(_) => Err(Error::BadValue),
            _ => Err(Error::BadType),
        }
    }

    /// STO, RCL, PURGE and EVAL with :n:NAME, ARCHIVE and RESTORE with a
    /// port, and the port commands.
    pub(crate) fn port_cmd(&mut self, w: &str) -> Option<Res<()>> {
        let at = self.stack.last().and_then(port_name);
        Some(match (w, at) {
            ("STO", Some((Port::One(p), name))) => self.need(2).and_then(|_| {
                let o = self.peek(2).clone();
                self.store_backup(p, name, o)?;
                self.stack.truncate(self.stack.len() - 2);
                Ok(())
            }),
            ("RCL" | "EVAL", Some((p, name))) => self.backup(&p, &name).and_then(|(_, o)| {
                let top = self.pop()?;
                self.last_args = vec![top];
                if w == "RCL" {
                    self.stack.push(o);
                    Ok(())
                } else {
                    self.eval_obj(o)
                }
            }),
            ("PURGE", Some((p, name))) => self.backup(&p, &name).map(|(p, _)| {
                self.ports[p].retain(|(n, _)| *n != name);
                self.stack.pop();
            }),
            // :n:NAME ARCHIVE backs HOME up to a port; RESTORE brings it back.
            ("ARCHIVE", Some((Port::One(p), name))) => {
                let home = Obj::Dir(self.vars.clone());
                self.store_backup(p, name, home).map(|_| {
                    self.stack.pop();
                })
            }
            ("RESTORE", Some((p, name))) => self.backup(&p, &name).and_then(|(_, o)| {
                let Obj::Dir(vars) = o else {
                    return Err(Error::User("Invalid Backup".into()));
                };
                self.vars = vars;
                self.path.clear();
                self.stack.pop();
                Ok(())
            }),
            // n PVARS: the port's backup objects as tagged names, and its
            // free memory; "SYSRAM" for port 0, which is main memory.
            ("PVARS", _) => self.port_arg().and_then(|p| {
                let objs = self.port(p)?;
                let names = objs
                    .iter()
                    .map(|(n, _)| Obj::Tag(p.to_string(), Box::new(Obj::Name(n.clone()))))
                    .collect();
                let free = match p {
                    0 => Obj::Str("SYSRAM".into()),
                    _ => Obj::Real(CARD - used(objs)),
                };
                self.stack.pop();
                self.stack.push(Obj::List(names));
                self.stack.push(free);
                Ok(())
            }),
            // 1 MERGE: port 1's card becomes main memory, and its backup
            // objects move to port 0. Only port 1 merges.
            ("MERGE", _) => self.port_arg().and_then(|p| {
                if p != 1 {
                    return Err(Error::BadValue);
                }
                if !self.merged {
                    let moved = std::mem::take(&mut self.ports[1]);
                    for (n, o) in moved {
                        self.ports[0].retain(|(m, _)| *m != n);
                        self.ports[0].push((n, o));
                    }
                    self.merged = true;
                }
                self.stack.pop();
                Ok(())
            }),
            // { names } 1 FREE: port 1's card on its own again, taking those
            // backup objects from port 0 with it.
            ("FREE", _) => self.port_arg().and_then(|p| {
                self.need(2)?;
                if p != 1 {
                    return Err(Error::BadValue);
                }
                if !self.merged {
                    return Err(not_available());
                }
                let Obj::List(l) = self.peek(2) else {
                    return Err(Error::BadType);
                };
                let names = l
                    .iter()
                    .map(|o| match (o, port_name(o)) {
                        (Obj::Name(n), _) => Ok(n.clone()),
                        (_, Some((Port::One(0), n))) => Ok(n),
                        _ => Err(Error::BadValue),
                    })
                    .collect::<Res<Vec<_>>>()?;
                let (go, stay): (Vec<_>, Vec<_>) = std::mem::take(&mut self.ports[0])
                    .into_iter()
                    .partition(|(n, _)| names.contains(n));
                if used(&go) > CARD {
                    self.ports[0] = go.into_iter().chain(stay).collect();
                    return Err(Error::Memory);
                }
                (self.ports[0], self.ports[1]) = (stay, go);
                self.merged = false;
                self.stack.truncate(self.stack.len() - 2);
                Ok(())
            }),
            // The cards are always there, so there is nothing to look for.
            ("PINIT", _) => Ok(()),
            // Turns the calculator off, ending a program that is running:
            // memory is saved and rpn48 quits.
            ("OFF", _) => {
                self.off = true;
                Err(Error::Killed)
            }
            // The last four times rpn48 was started, newest first.
            ("WSLOG", _) => self.starts.clone().iter().take(4).try_for_each(|&t| {
                let (d, time) = crate::alarms::date_time(t);
                self.stack.extend([Obj::Real(d), Obj::Real(time)]);
                self.eval_word("TSTR")
            }),
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::calc::{Calc, Error};

    fn run(src: &str) -> Vec<String> {
        let mut c = Calc::new();
        c.exec_line(src).unwrap();
        c.stack.iter().map(|o| o.show(c.fmt)).collect()
    }

    #[test]
    fn backups() {
        assert_eq!(
            run("5 :1:A STO « 2 * » :2:DBL STO :1:A RCL :&:DBL EVAL"),
            ["10"]
        );
        assert_eq!(run("1 :0:B STO 2 'B' STO :0:B RCL B"), ["1", "2"]);
        assert_eq!(run("1 :2:X STO 2 PVARS"), ["{ :2:X }", "131054.5"]);
        assert_eq!(run("1 :0:X STO 0 PVARS"), ["{ :0:X }", "\"SYSRAM\""]);
        assert_eq!(run("1 :1:X STO :1:X PURGE 1 PVARS DROP"), ["{ }"]);
        let mut c = Calc::new();
        assert!(c.exec_line(":1:Q RCL").is_err());
        assert_eq!(c.stack.len(), 1);
        assert!(c.exec_line("CLEAR 1 :3:Q STO").is_err());
        // Tagged objects that are not port names are left alone.
        assert_eq!(run(":T:5 1 +"), ["6"]);
    }

    #[test]
    fn merging() {
        assert_eq!(
            run("7 :1:A STO 1 MERGE 0 PVARS DROP :&:A RCL"),
            ["{ :0:A }", "7"]
        );
        let mut c = Calc::new();
        c.exec_line("1 MERGE").unwrap();
        assert!(c.exec_line("1 :1:A STO").is_err());
        assert!(c.exec_line("CLEAR 1 PVARS").is_err());
        c.exec_line("CLEAR 3 :0:A STO 4 :0:B STO { A :0:B } 1 FREE 1 PVARS DROP 0 PVARS DROP")
            .unwrap();
        let s: Vec<String> = c.stack.iter().map(|o| o.show(c.fmt)).collect();
        assert_eq!(s, ["{ :1:A :1:B }", "{ }"]);
        assert!(c.exec_line("CLEAR 2 MERGE").is_err());
    }

    #[test]
    fn archive_and_off() {
        assert_eq!(
            run("1 'A' STO :2:BAK ARCHIVE 2 'A' STO 'Q' CRDIR :2:BAK RESTORE VARS A"),
            ["{ A }", "1"]
        );
        let mut c = Calc::new();
        assert_eq!(c.exec_line("« 1 OFF 2 » EVAL"), Err(Error::Killed));
        assert!(c.off);
        assert_eq!(c.stack.len(), 1);
        c.exec_line("CLEAR 1 :2:Q STO").unwrap();
        let ports = crate::menu::Menu::Library;
        assert_eq!(ports.items(&c), ["PORT0", "PORT1", "PORT2"]);
        assert_eq!(
            ports.sub("PORT2").map(|m| m.items(&c)),
            Some(vec!["Q".to_string()])
        );
        c.starts = vec![0.0, 86400.0];
        c.exec_line("CLEAR WSLOG").unwrap();
        assert_eq!(c.stack.len(), 2);
    }
}

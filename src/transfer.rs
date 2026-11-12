//! I/O: the calculator's serial port and printer, as files. SEND writes a
//! variable to the transfer directory, a file named for it holding the
//! object's source; KGET and RECV read them back into variables. The
//! printing commands append to a print file instead of paper.

use std::fs;
use std::io::Write;
use std::path::PathBuf;

use crate::calc::{Calc, Error, Res};
use crate::obj::{Coord, Fmt, Node, Obj, coord, set_coord};
use crate::parse::parse;

/// $XDG_DATA_HOME/rpn48, beside the memory file.
fn data_dir() -> Res<PathBuf> {
    crate::persist::path()
        .and_then(|p| p.parent().map(PathBuf::from))
        .ok_or_else(|| Error::User("No data directory".into()))
}

fn io_err(e: std::io::Error) -> Error {
    Error::User(e.to_string())
}

/// The source of an object, in rectangular form so that it reads back
/// whatever the modes.
fn source(o: &Obj) -> String {
    let shown = coord();
    set_coord(Coord::Rect, 1.0);
    let s = o.show(Fmt::Std);
    set_coord(shown.0, shown.1);
    s
}

/// A variable name that is safe as a file name.
fn file_name(n: &str) -> Res<&str> {
    if n.is_empty() || n.contains(['/', '\0']) || n.starts_with('.') {
        return Err(Error::BadValue);
    }
    Ok(n)
}

impl Calc {
    pub fn transfer_dir(&self) -> Res<PathBuf> {
        match &self.io_dir {
            Some(d) => Ok(d.join("transfer")),
            None => data_dir().map(|d| d.join("transfer")),
        }
    }

    pub fn print_file(&self) -> Res<PathBuf> {
        match &self.io_dir {
            Some(d) => Ok(d.join("printer")),
            None => data_dir().map(|d| d.join("printer")),
        }
    }

    /// The names in level 1: a name, or a list of them.
    fn names_arg(&self) -> Res<Vec<String>> {
        match self.peek(1) {
            Obj::Name(n) => Ok(vec![n.clone()]),
            Obj::List(l) => l
                .iter()
                .map(|o| match o {
                    Obj::Name(n) => Ok(n.clone()),
                    _ => Err(Error::BadType),
                })
                .collect(),
            _ => Err(Error::BadType),
        }
    }

    fn receive(&mut self, name: &str) -> Res<()> {
        let path = self.transfer_dir()?.join(file_name(name)?);
        let text = fs::read_to_string(path).map_err(io_err)?;
        let o = match parse(&text)?.as_slice() {
            [Node::Push(o)] => o.clone(),
            _ => return Err(Error::User("Invalid Transfer".into())),
        };
        self.store(name.into(), o)
    }

    /// The file that stands for the serial line, which XMIT writes and
    /// SRECV reads, like a cable looped back.
    fn serial_file(&self) -> Res<PathBuf> {
        self.transfer_dir().map(|d| d.join(".serial"))
    }

    /// :IO:name or 'name': the name of an archive in the transfer directory.
    fn archive_name(&self) -> Res<String> {
        match self.peek(1) {
            Obj::Name(n) => Ok(file_name(n)?.into()),
            Obj::Tag(_, o) => match &**o {
                Obj::Name(n) => Ok(file_name(n)?.into()),
                _ => Err(Error::BadType),
            },
            _ => Err(Error::BadType),
        }
    }

    /// IOPAR: { baud parity receive-pacing transmit-pacing checksum
    /// translate }, which BAUD, PARITY, CKSM and TRANSIO set.
    fn set_iopar(&mut self, i: usize, lo: f64, hi: f64) -> Res<()> {
        self.need(1)?;
        let Obj::Real(x) = *self.peek(1) else {
            return Err(Error::BadType);
        };
        if x < lo || x > hi || x.fract() != 0.0 {
            return Err(Error::BadValue);
        }
        let mut par = match self.global("IOPAR") {
            Some(Obj::List(l)) if l.len() == 6 => l.clone(),
            _ => [9600.0, 0.0, 0.0, 0.0, 3.0, 1.0].map(Obj::Real).to_vec(),
        };
        par[i] = Obj::Real(x);
        self.store("IOPAR".into(), Obj::List(par))?;
        self.stack.pop();
        Ok(())
    }

    fn print(&self, text: &str) -> Res<()> {
        let path = self.print_file()?;
        if let Some(d) = path.parent() {
            fs::create_dir_all(d).map_err(io_err)?;
        }
        let mut f = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(io_err)?;
        writeln!(f, "{text}").map_err(io_err)
    }

    pub(crate) fn io_cmd(&mut self, w: &str) -> Option<Res<()>> {
        Some(match w {
            "SEND" | "XSEND" | "XPUT" => self.need(1).and_then(|_| {
                let dir = self.transfer_dir()?;
                fs::create_dir_all(&dir).map_err(io_err)?;
                for n in self.names_arg()? {
                    let v = self.global(&n).ok_or(Error::Undefined)?;
                    fs::write(dir.join(file_name(&n)?), source(v) + "\n").map_err(io_err)?;
                }
                self.stack.pop();
                Ok(())
            }),
            "KGET" | "XRECV" | "XGET" => self.need(1).and_then(|_| {
                for n in self.names_arg()? {
                    self.receive(&n)?;
                }
                self.stack.pop();
                Ok(())
            }),
            // Everything waiting in the transfer directory.
            "RECV" => self.transfer_dir().and_then(|dir| {
                let mut names: Vec<String> = fs::read_dir(dir)
                    .map_err(io_err)?
                    .filter_map(|e| e.ok()?.file_name().into_string().ok())
                    .filter(|n| file_name(n).is_ok())
                    .collect();
                names.sort();
                for n in names {
                    self.receive(&n)?;
                }
                Ok(())
            }),
            "PR1" => self
                .need(1)
                .and_then(|_| self.print(&self.peek(1).show(self.fmt))),
            "PRST" => (1..=self.stack.len())
                .rev()
                .try_for_each(|i| self.print(&format!("{i}: {}", self.peek(i).show(self.fmt)))),
            "PRVAR" => self.need(1).and_then(|_| {
                for n in self.names_arg()? {
                    let v = self.global(&n).ok_or(Error::Undefined)?;
                    self.print(&format!("{n}: {}", v.show(self.fmt)))?;
                }
                self.stack.pop();
                Ok(())
            }),
            "CR" => self.print(""),
            // The stack one line to a level, levels past the first line cut short.
            "PRSTC" | "PRLCD" => (1..=self.stack.len()).rev().try_for_each(|i| {
                let line: String = format!("{i}: {}", self.peek(i).show(self.fmt))
                    .chars()
                    .take(if w == "PRSTC" { 22 } else { 1000 })
                    .collect();
                self.print(&line)
            }),
            // The printer's line delay, and the old printer's character set,
            // mean nothing to a file.
            "DELAY" => self.need(1).map(|_| {
                self.stack.pop();
            }),
            "OLDPRT" | "OPENIO" | "CLOSEIO" | "SBRK" => Ok(()),
            "BAUD" => self.set_iopar(0, 1200.0, 9600.0),
            "PARITY" => self.set_iopar(1, -4.0, 4.0),
            "CKSM" => self.set_iopar(4, 1.0, 3.0),
            "TRANSIO" => self.set_iopar(5, 0.0, 3.0),
            // "text" XMIT: sent down the line, and 1.
            "XMIT" => self.need(1).and_then(|_| {
                let text = self.str_of(self.peek(1));
                let path = self.serial_file()?;
                fs::create_dir_all(path.parent().unwrap()).map_err(io_err)?;
                let mut f = fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(path)
                    .map_err(io_err)?;
                f.write_all(text.as_bytes()).map_err(io_err)?;
                self.stack.pop();
                self.stack.push(Obj::Real(1.0));
                Ok(())
            }),
            // n SRECV: up to n characters from the line, and 1.
            "SRECV" => self.need(1).and_then(|_| {
                let Obj::Real(n) = *self.peek(1) else {
                    return Err(Error::BadType);
                };
                let path = self.serial_file()?;
                let text = fs::read_to_string(&path).unwrap_or_default();
                let got: String = text.chars().take(n.max(0.0) as usize).collect();
                let rest = &text[got.len()..];
                if !text.is_empty() {
                    fs::write(&path, rest).map_err(io_err)?;
                }
                self.stack.pop();
                self.stack.push(Obj::Str(got));
                self.stack.push(Obj::Real(1.0));
                Ok(())
            }),
            "BUFLEN" => self.serial_file().map(|path| {
                let n = fs::read_to_string(path).map_or(0, |t| t.chars().count());
                self.stack.push(Obj::Real(n as f64));
                self.stack.push(Obj::Real(1.0));
            }),
            // 'name' ARCHIVE backs up HOME to the transfer directory, and
            // RESTORE puts such a backup back in its place.
            "ARCHIVE" => self.need(1).and_then(|_| {
                let name = self.archive_name()?;
                let dir = self.transfer_dir()?;
                fs::create_dir_all(&dir).map_err(io_err)?;
                let home = Obj::Dir(self.vars.clone());
                fs::write(dir.join(name), source(&home) + "\n").map_err(io_err)?;
                self.stack.pop();
                Ok(())
            }),
            "RESTORE" => self.need(1).and_then(|_| {
                let path = self.transfer_dir()?.join(self.archive_name()?);
                let text = fs::read_to_string(path).map_err(io_err)?;
                let nodes = parse(&text)?;
                let [Node::Push(Obj::Dir(vars))] = nodes.as_slice() else {
                    return Err(Error::User("Invalid Backup".into()));
                };
                self.vars = vars.clone();
                self.path.clear();
                self.stack.pop();
                Ok(())
            }),
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use crate::calc::Calc;

    #[test]
    fn send_and_print() {
        let dir = std::env::temp_dir().join(format!("rpn48-io-{}", std::process::id()));
        let mut c = Calc::new();
        c.io_dir = Some(dir.clone());
        c.exec_line("« 1 + » 'P' STO (1,2) 'Z' STO CYLIN { P Z } SEND 'P' PURGE 'Z' PURGE")
            .unwrap();
        c.exec_line("'P' KGET RECT 41 P").unwrap();
        assert_eq!(c.stack[0].show(c.fmt), "42");
        c.exec_line("RECV Z 'Z' PRVAR PRST CR").unwrap();
        let printed = fs::read_to_string(dir.join("printer")).unwrap();
        fs::remove_dir_all(dir).unwrap();
        assert_eq!(printed, "Z: (1,2)\n2: 42\n1: (1,2)\n\n");
        assert!(c.exec_line("'Q' KGET").is_err());
    }

    #[test]
    fn archive_and_serial() {
        let dir = std::env::temp_dir().join(format!("rpn48-arc-{}", std::process::id()));
        let mut c = Calc::new();
        c.io_dir = Some(dir.clone());
        c.exec_line("1 'A' STO 'D' CRDIR :IO:BAK ARCHIVE 'A' PURGE 2 'B' STO 'BAK' RESTORE VARS")
            .unwrap();
        c.exec_line("\"hello\" XMIT DROP BUFLEN DROP 3 SRECV DROP 9 SRECV DROP 2400 BAUD IOPAR")
            .unwrap();
        fs::remove_dir_all(dir).unwrap();
        let s: Vec<String> = c.stack.iter().map(|o| o.show(c.fmt)).collect();
        assert_eq!(
            s,
            ["{ D A }", "5", "\"hel\"", "\"lo\"", "{ 2400 0 0 0 3 1 }"]
        );
    }
}

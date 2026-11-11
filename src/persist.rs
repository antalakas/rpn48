//! Continuous memory: the stack, variables and modes, kept between runs.
//!
//! The file is calculator input: three lists, for the stack (level 1 last),
//! the variables as name/value pairs, and the modes. STD formatting writes all
//! 12 digits a real holds, so everything reads back exactly.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::calc::{Angle, Calc};
use crate::obj::{Fmt, Node, Obj};
use crate::parse::parse;

/// $XDG_DATA_HOME/rpn48/memory, or ~/.local/share/rpn48/memory.
pub fn path() -> Option<PathBuf> {
    let data = std::env::var_os("XDG_DATA_HOME")
        .filter(|d| !d.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))?;
    Some(data.join("rpn48/memory"))
}

pub fn save(calc: &Calc, path: &Path) -> io::Result<()> {
    let vars = calc
        .vars
        .iter()
        .flat_map(|(n, v)| [Obj::Name(n.clone()), v.clone()])
        .collect();
    let (kind, digits) = match calc.fmt {
        Fmt::Std => ("STD", 0),
        Fmt::Fix(n) => ("FIX", n),
        Fmt::Sci(n) => ("SCI", n),
        Fmt::Eng(n) => ("ENG", n),
    };
    let modes = vec![
        Obj::Str(calc.angle.name().into()),
        Obj::Str(kind.into()),
        Obj::Real(digits as f64),
    ];
    let text = [
        Obj::List(calc.stack.clone()),
        Obj::List(vars),
        Obj::List(modes),
    ]
    .map(|o| o.show(Fmt::Std) + "\n")
    .concat();
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, text)?;
    fs::rename(tmp, path)
}

/// Loads memory saved by `save`; a missing file leaves `calc` as it is.
pub fn load(calc: &mut Calc, path: &Path) -> Result<(), String> {
    let text = match fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e.to_string()),
    };
    let nodes = parse(&text).map_err(|e| e.to_string())?;
    let [
        Node::Push(Obj::List(stack)),
        Node::Push(Obj::List(vars)),
        Node::Push(Obj::List(modes)),
    ] = &nodes[..]
    else {
        return Err("unexpected contents".into());
    };
    let mut pairs = vec![];
    for pair in vars.chunks(2) {
        let [Obj::Name(n), v] = pair else {
            return Err("bad variable".into());
        };
        pairs.push((n.clone(), v.clone()));
    }
    let [Obj::Str(angle), Obj::Str(kind), Obj::Real(n)] = &modes[..] else {
        return Err("bad modes".into());
    };
    let n = *n as u8;
    calc.angle = match angle.as_str() {
        "RAD" => Angle::Rad,
        "GRAD" => Angle::Grad,
        _ => Angle::Deg,
    };
    calc.fmt = match kind.as_str() {
        "FIX" => Fmt::Fix(n),
        "SCI" => Fmt::Sci(n),
        "ENG" => Fmt::Eng(n),
        _ => Fmt::Std,
    };
    calc.stack = stack.clone();
    calc.vars = pairs;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let dir = std::env::temp_dir().join(format!("rpn48-test-{}", std::process::id()));
        let path = dir.join("memory");
        let mut c = Calc::new();
        c.exec_line("RAD 4 FIX π 1 3 / (1,-2) \"say \\\"hi\\\"\" { A 1 { } } 'X' « → a « a 'Y' STO » » 'P' STO 7 'N' STO")
            .unwrap();
        save(&c, &path).unwrap();
        let mut d = Calc::new();
        load(&mut d, &path).unwrap();
        fs::remove_dir_all(dir).unwrap();
        assert_eq!(d.stack, c.stack);
        assert_eq!(d.vars, c.vars);
        assert_eq!((d.angle, d.fmt), (Angle::Rad, Fmt::Fix(4)));
    }
}

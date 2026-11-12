//! Continuous memory: the stack, variables and modes, kept between runs.
//!
//! The file is calculator input: three lists, for the stack (level 1 last),
//! the variables as name/value pairs (subdirectories as DIR … END), and the
//! modes. STD formatting writes all 12 digits a real holds, and binary
//! integers carry their base, so everything reads back exactly.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::calc::{Angle, Calc};
use crate::obj::{Base, Coord, Fmt, Node, Obj, coord, set_coord};
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
        Obj::Str(calc.base.name().into()),
        Obj::Real(calc.wordsize as f64),
        Obj::Bin(calc.flags[0]),
        Obj::Bin(calc.flags[1]),
        Obj::List(calc.path.iter().map(|d| Obj::Name(d.clone())).collect()),
        Obj::Str(calc.coord.name().into()),
        Obj::List(
            calc.user_keys
                .iter()
                .flat_map(|(c, o)| [o.clone(), Obj::Real(*c)])
                .collect(),
        ),
        Obj::List(
            calc.alarms
                .iter()
                .flat_map(|a| [Obj::Real(a.at), a.action.clone(), Obj::Real(a.repeat)])
                .collect(),
        ),
        Obj::Real(crate::alarms::clock_offset()),
    ];
    // Written in rectangular form, which reads back whatever the modes.
    let shown = coord();
    set_coord(Coord::Rect, 1.0);
    let text = [
        Obj::List(calc.stack.clone()),
        Obj::List(vars),
        Obj::List(modes),
    ]
    .map(|o| o.show(Fmt::Std) + "\n")
    .concat();
    set_coord(shown.0, shown.1);
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
    // Memory from before binary integers and directories has only the first three.
    let [Obj::Str(angle), Obj::Str(kind), Obj::Real(n), more @ ..] = &modes[..] else {
        return Err("bad modes".into());
    };
    let n = *n as u8;
    calc.set_angle(match angle.as_str() {
        "RAD" => Angle::Rad,
        "GRAD" => Angle::Grad,
        _ => Angle::Deg,
    });
    calc.fmt = match kind.as_str() {
        "FIX" => Fmt::Fix(n),
        "SCI" => Fmt::Sci(n),
        "ENG" => Fmt::Eng(n),
        _ => Fmt::Std,
    };
    calc.stack = stack.clone();
    calc.vars = pairs;
    if let [
        Obj::Str(base),
        Obj::Real(ws),
        Obj::Bin(user),
        Obj::Bin(sys),
        Obj::List(path),
        rest @ ..,
    ] = more
    {
        // The coordinate system came later still.
        calc.set_coord(match rest {
            [Obj::Str(c), ..] if c == "CYLIN" => Coord::Cylin,
            [Obj::Str(c), ..] if c == "SPHERE" => Coord::Sphere,
            _ => Coord::Rect,
        });
        // And then the USER keyboard's assignments.
        if let [_, Obj::List(keys), ..] = rest {
            calc.user_keys = keys
                .chunks(2)
                .filter_map(|p| match p {
                    [o, Obj::Real(c)] => Some((*c, o.clone())),
                    _ => None,
                })
                .collect();
        }
        if let [_, _, Obj::List(alarms), ..] = rest {
            calc.alarms = alarms
                .chunks(3)
                .filter_map(|a| match a {
                    [Obj::Real(at), action, Obj::Real(repeat)] => Some(crate::alarms::Alarm {
                        at: *at,
                        action: action.clone(),
                        repeat: *repeat,
                        due: false,
                    }),
                    _ => None,
                })
                .collect();
        }
        if let [_, _, _, Obj::Real(clock), ..] = rest {
            crate::alarms::set_clock_offset(*clock);
        }
        calc.set_base(match base.as_str() {
            "HEX" => Base::Hex,
            "OCT" => Base::Oct,
            "BIN" => Base::Bin,
            _ => Base::Dec,
        });
        calc.wordsize = (*ws as u32).clamp(1, 64);
        calc.flags = [*user, *sys];
        // Follow the path as far as it still names directories.
        for d in path {
            let Obj::Name(d) = d else { break };
            if !calc
                .cur_vars()
                .iter()
                .any(|(n, o)| n == d && matches!(o, Obj::Dir(_)))
            {
                break;
            }
            calc.path.push(d.clone());
        }
    }
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
        c.exec_line("« 1 » 11.2 ASN { 12.312099 23 \"x\" 8192 } STOALARM DROP RAD CYLIN 4 FIX π 1 3 / (1,-2) \"say \\\"hi\\\"\" { A 1 { } } 'X' « → a « a 'Y' STO » » 'P' STO 7 'N' STO")
            .unwrap();
        c.exec_line("'X^2+1' [[1 2][3 4]] :T:5 [[ (1,2) 3 ][ 4 (0,-1) ]] 1_J/(kg*K) -40_°F HEX 16 STWS #FFh 5 SF -3 SF 'D' CRDIR D 'E' CRDIR E 9 'Z' STO")
            .unwrap();
        save(&c, &path).unwrap();
        let mut d = Calc::new();
        load(&mut d, &path).unwrap();
        fs::remove_dir_all(dir).unwrap();
        assert_eq!(d.stack, c.stack);
        assert_eq!(d.vars, c.vars);
        assert_eq!((d.angle, d.fmt), (Angle::Rad, Fmt::Fix(4)));
        assert_eq!((d.base, d.wordsize, d.flags), (Base::Hex, 16, [16, 4]));
        assert_eq!(d.path, ["D", "E"]);
        assert_eq!(d.coord, crate::obj::Coord::Cylin);
        assert_eq!(d.user_keys, c.user_keys);
        assert_eq!(d.alarms, c.alarms);
    }

    #[test]
    fn round_trip_of_this_rounds_state() {
        let dir = std::env::temp_dir().join(format!("rpn48-test2-{}", std::process::id()));
        let path = dir.join("memory");
        let mut c = Calc::new();
        c.exec_line("{ 'A=W*H' 'P=2*(W+H)' } STEQ MINIT { W } MUSER 0 2 XVOL (1,1) AXES 2 ATICK 'T' DEPND EXPFIT")
            .unwrap();
        c.exec_line("2_mol^-1 'U' STO 'NA' CONST [[1 2][3 4]] EGV 2400 BAUD 3 SF 1 2 3 3 ROW→")
            .unwrap();
        save(&c, &path).unwrap();
        let mut d = Calc::new();
        load(&mut d, &path).unwrap();
        fs::remove_dir_all(dir).unwrap();
        assert_eq!(d.stack, c.stack);
        assert_eq!(d.vars, c.vars);
    }
}

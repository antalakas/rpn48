//! Commands that stop a program to show something or wait for the user:
//! HALT and CONT, PROMPT, INPUT, MSGBOX, DISP, WAIT. How the stopping is
//! done is up to whoever runs the calculator, through `Calc::pause`.

use std::time::{Duration, Instant};

use crate::calc::{Calc, Error, Res};
use crate::obj::{Fmt, Node, Obj};
use crate::parse::parse;

/// What a program stops for.
pub enum Pause {
    /// Until CONT, with the calculator usable meanwhile.
    Halt,
    /// HALT with a message.
    Prompt(String),
    /// A line of input with a prompt and the text it starts with.
    Input(String, String),
    /// A message, until a key is pressed.
    Message(String),
    /// Draw the display now: DISP shows `line` while the program runs on.
    Show(String),
    /// A key's code, rr.p (row, column, shift plane), when one is pressed,
    /// waiting for it or not.
    Key(bool),
}

/// Stops the program for `Pause`; gives the text typed, for INPUT.
pub type PauseFn = Box<dyn FnMut(&mut Calc, Pause) -> Res<Option<String>>>;

/// The display has seven lines for DISP.
pub const LINES: usize = 7;

impl Calc {
    fn pause_for(&mut self, p: Pause) -> Res<Option<String>> {
        let Some(mut hook) = self.pause.take() else {
            return match p {
                Pause::Show(_) | Pause::Key(false) => Ok(None),
                _ => Err(Error::User("Can't stop here".into())),
            };
        };
        let halt = matches!(p, Pause::Halt | Pause::Prompt(_));
        self.halted += usize::from(halt);
        let r = hook(self, p);
        self.halted -= usize::from(halt);
        self.pause = Some(hook);
        r
    }

    /// An object typed in answer to a prompt; nothing typed is None.
    fn ask_obj(&mut self, prompt: String, default: String) -> Res<Option<Obj>> {
        let text = self
            .pause_for(Pause::Input(prompt, default))?
            .unwrap_or_default();
        if text.trim().is_empty() {
            return Ok(None);
        }
        match &parse(&text)?[..] {
            [Node::Push(o)] => Ok(Some(o.clone())),
            [Node::Word(w)] => Ok(Some(Obj::Name(w.clone()))),
            nodes => Ok(Some(Obj::Prog(nodes.to_vec()))),
        }
    }

    fn text_arg(&self) -> Res<String> {
        self.need(1)?;
        Ok(self.str_of(self.peek(1)))
    }

    pub(crate) fn pause_cmd(&mut self, w: &str) -> Option<Res<()>> {
        Some(match w {
            "HALT" => self.pause_for(Pause::Halt).map(drop),
            "CONT" => {
                self.cont = self.halted > 0;
                Ok(())
            }
            "KILL" => Err(Error::Killed),
            "PROMPT" | "MSGBOX" => self.text_arg().and_then(|s| {
                self.stack.pop();
                let p = if w == "PROMPT" {
                    Pause::Prompt(s)
                } else {
                    Pause::Message(s)
                };
                self.pause_for(p).map(drop)
            }),
            // "prompt" "default" INPUT gives the line typed, as a string. The
            // default may be a list starting with it, as on the calculator.
            "INPUT" => self.need(2).and_then(|_| {
                let default = match self.peek(1) {
                    Obj::Str(s) => s.clone(),
                    Obj::List(l) => match l.first() {
                        Some(Obj::Str(s)) => s.clone(),
                        None => String::new(),
                        _ => return Err(Error::BadType),
                    },
                    _ => return Err(Error::BadType),
                };
                let prompt = self.str_of(self.peek(2));
                let text = self.pause_for(Pause::Input(prompt, default))?;
                self.stack.truncate(self.stack.len() - 2);
                self.stack.push(Obj::Str(text.unwrap_or_default()));
                Ok(())
            }),
            "DISP" => self.need(2).and_then(|_| {
                let Obj::Real(n) = *self.peek(1) else {
                    return Err(Error::BadType);
                };
                let line = (n.clamp(1.0, LINES as f64) as usize) - 1;
                let text = self.str_of(self.peek(2));
                self.stack.truncate(self.stack.len() - 2);
                self.disp.resize(LINES, String::new());
                self.disp[line] = text.clone();
                self.pause_for(Pause::Show(text)).map(drop)
            }),
            "CLLCD" => {
                self.disp = vec![String::new(); LINES];
                self.pause_for(Pause::Show(String::new())).map(drop)
            }
            // What DISP showed stays after the program ends, until a key.
            "FREEZE" => self.need(1).map(|_| {
                self.stack.pop();
                self.frozen = true;
            }),
            // KEY: the code of a key pressed since, and 1, or just 0.
            "KEY" => {
                self.pause_for(Pause::Key(false))
                    .map(|k| match k.and_then(|k| k.parse().ok()) {
                        Some(code) => self
                            .stack
                            .extend([Obj::Real(f64::trunc(code)), Obj::Real(1.0)]),
                        None => self.stack.push(Obj::Real(0.0)),
                    })
            }
            "WAIT" => self.need(1).and_then(|_| {
                let Obj::Real(s) = *self.peek(1) else {
                    return Err(Error::BadType);
                };
                // 0 WAIT (or -1) waits for a key and gives its code, rc.p.
                if s <= 0.0 {
                    let k = self.pause_for(Pause::Key(true))?;
                    let code = k.and_then(|k| k.parse().ok()).ok_or(Error::BadValue)?;
                    self.stack.pop();
                    self.stack.push(Obj::Real(code));
                    return Ok(());
                }
                if s.is_nan() || s > 1e6 {
                    return Err(Error::BadValue);
                }
                self.stack.pop();
                let end = Instant::now() + Duration::from_secs_f64(s);
                while Instant::now() < end {
                    if self.interrupt.as_mut().is_some_and(|f| f()) {
                        return Err(Error::Interrupted);
                    }
                    std::thread::sleep(Duration::from_millis(20).min(end - Instant::now()));
                }
                Ok(())
            }),
            // "title" { items } pos CHOOSE: the item picked, by number, and 1;
            // just 0 if nothing is. An item { label obj } gives its object.
            "CHOOSE" => self.need(3).and_then(|_| {
                let n = self.stack.len();
                let (Obj::List(items), Obj::Real(pos)) = (&self.stack[n - 2], &self.stack[n - 1])
                else {
                    return Err(Error::BadType);
                };
                let items = items.clone();
                let title = self.str_of(&self.stack[n - 3]);
                let label = |c: &Calc, o: &Obj| match o {
                    Obj::List(l) if l.len() == 2 => c.str_of(&l[0]),
                    o => c.str_of(o),
                };
                let menu: Vec<String> = items
                    .iter()
                    .enumerate()
                    .map(|(i, o)| format!("{} {}", i + 1, label(self, o)))
                    .collect();
                let prompt = format!("{title}: {}", menu.join("  "));
                let default = if *pos >= 1.0 {
                    format!("{}", *pos as usize)
                } else {
                    String::new()
                };
                let text = self
                    .pause_for(Pause::Input(prompt, default))?
                    .unwrap_or_default();
                self.stack.truncate(n - 3);
                match text
                    .trim()
                    .parse::<usize>()
                    .ok()
                    .and_then(|k| items.get(k.wrapping_sub(1)))
                {
                    Some(o) => {
                        self.stack.push(match o {
                            Obj::List(l) if l.len() == 2 => l[1].clone(),
                            o => o.clone(),
                        });
                        self.stack.push(Obj::Real(1.0));
                    }
                    None => self.stack.push(Obj::Real(0.0)),
                }
                Ok(())
            }),
            // "title" { fields } format { reset } { initial } INFORM: a value
            // asked for each field, NOVAL where none is typed, and 1.
            "INFORM" => self.need(5).and_then(|_| {
                let n = self.stack.len();
                let Obj::List(fields) = self.stack[n - 4].clone() else {
                    return Err(Error::BadType);
                };
                let initial = match &self.stack[n - 1] {
                    Obj::List(l) => l.clone(),
                    _ => return Err(Error::BadType),
                };
                let title = self.str_of(&self.stack[n - 5]);
                let mut values = vec![];
                for (i, f) in fields.iter().enumerate() {
                    let label = match f {
                        Obj::List(l) => l.first().map(|o| self.str_of(o)).unwrap_or_default(),
                        o => self.str_of(o),
                    };
                    let init = initial.get(i).filter(|o| !is_noval(o));
                    let default = init.map(|o| o.show(Fmt::Std)).unwrap_or_default();
                    let v = self.ask_obj(format!("{title} {label}"), default)?;
                    values.push(v.unwrap_or(noval()));
                }
                self.stack.truncate(n - 5);
                self.stack.push(Obj::List(values));
                self.stack.push(Obj::Real(1.0));
                Ok(())
            }),
            "NOVAL" => {
                self.stack.push(noval());
                Ok(())
            }
            // 'name' VISIT: the variable's contents edited and stored back.
            "VISIT" | "VISITB" => self.need(1).and_then(|_| {
                let Obj::Name(name) = self.peek(1).clone() else {
                    return Err(Error::BadType);
                };
                let v = self.global(&name).cloned().ok_or(Error::Undefined)?;
                if let Some(o) = self.ask_obj(name.clone(), v.show(Fmt::Std))? {
                    self.store(name, o)?;
                }
                self.stack.pop();
                Ok(())
            }),
            "BEEP" => self.need(2).map(|_| {
                self.stack.truncate(self.stack.len() - 2);
            }),
            _ => return None,
        })
    }
}

/// The empty value of an INFORM field.
fn noval() -> Obj {
    Obj::Name("NOVAL".into())
}

fn is_noval(o: &Obj) -> bool {
    *o == noval()
}

#[cfg(test)]
mod tests {
    use super::Pause;
    use crate::calc::{Calc, Error};

    #[test]
    fn pauses() {
        let mut c = Calc::new();
        c.pause = Some(Box::new(|c, p| {
            Ok(match p {
                Pause::Input(prompt, default) => Some(format!("{prompt}:{default}")),
                Pause::Halt => {
                    c.stack.push(crate::obj::Obj::Real(c.halted as f64));
                    None
                }
                _ => None,
            })
        }));
        c.exec_line("\"N\" \"5\" INPUT 1 HALT \"a\" 3 DISP")
            .unwrap();
        let s: Vec<String> = c.stack.iter().map(|o| o.show(c.fmt)).collect();
        assert_eq!(s, ["\"N:5\"", "1", "1"]);
        assert_eq!(c.disp[2], "a");
        assert_eq!(c.halted, 0);
        assert!(matches!(
            c.exec_line("IFERR KILL THEN 1 END"),
            Err(Error::Killed)
        ));
        c.pause = None;
        assert!(c.exec_line("HALT").is_err());
    }

    /// A calculator whose prompts are answered from `answers` in turn.
    fn answering(answers: &[&str]) -> Calc {
        let mut c = Calc::new();
        let mut answers: Vec<String> = answers.iter().rev().map(|s| s.to_string()).collect();
        c.pause = Some(Box::new(move |_, p| {
            Ok(match p {
                Pause::Input(..) => answers.pop(),
                _ => None,
            })
        }));
        c
    }

    fn shown(c: &Calc) -> Vec<String> {
        c.stack.iter().map(|o| o.show(c.fmt)).collect()
    }

    #[test]
    fn dialogs() {
        let mut c = answering(&["2", ""]);
        c.exec_line("\"Pick\" { A { \"Two\" 2 } } 1 CHOOSE \"Pick\" { A } 1 CHOOSE")
            .unwrap();
        assert_eq!(shown(&c), ["2", "1", "0"]);
        let mut c = answering(&["5", "", "'X+1'"]);
        c.exec_line("\"Form\" { \"A\" \"B\" \"C\" } { } { } { 1 2 3 } INFORM")
            .unwrap();
        assert_eq!(shown(&c), ["{ 5 NOVAL 'X+1' }", "1"]);
        let mut c = answering(&["{ 1 2 3 }"]);
        c.exec_line("{ 1 2 } 'L' STO 'L' VISIT L").unwrap();
        assert_eq!(shown(&c), ["{ 1 2 3 }"]);
    }
}

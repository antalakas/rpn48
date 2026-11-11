//! Commands that stop a program to show something or wait for the user:
//! HALT and CONT, PROMPT, INPUT, MSGBOX, DISP, WAIT. How the stopping is
//! done is up to whoever runs the calculator, through `Calc::pause`.

use std::time::{Duration, Instant};

use crate::calc::{Calc, Error, Res};
use crate::obj::Obj;

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
            "BEEP" => self.need(2).map(|_| {
                self.stack.truncate(self.stack.len() - 2);
            }),
            _ => return None,
        })
    }
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
}

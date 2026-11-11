//! rpn48: an RPN calculator for the terminal in the style of the HP 48.

mod alg;
mod array;
mod binary;
mod calc;
mod keypad;
mod lists;
mod menu;
mod more;
mod obj;
mod parse;
mod pause;
mod persist;
mod stat;
mod symbolic;
mod ui;
mod units;

use std::cell::{Cell, RefCell};
use std::io;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind,
    KeyModifiers, MouseButton, MouseEventKind,
};
use ratatui::crossterm::execute;
use ratatui::layout::Rect;

use calc::{Angle, Calc, Error, Snapshot};
use keypad::{Act, Key, Shift};
use menu::Menu;
use obj::{Fmt, Obj};
use parse::{is_open, parse, parse_num};
use pause::{Pause, PauseFn};

const USAGE: &str = "\
usage: rpn48            start the calculator
       rpn48 <input>    run <input> on an empty stack and print the stack

Memory (stack, variables, modes) is kept in $XDG_DATA_HOME/rpn48/memory.";

const UNDO_LEVELS: usize = 100;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "-h" || a == "--help") {
        println!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    if !args.is_empty() {
        return batch(&args.join(" "));
    }
    // Deeply recursive user programs recurse in the interpreter too, so give
    // it more stack than the main thread has.
    let run = std::thread::Builder::new()
        .stack_size(256 << 20)
        .spawn(interactive);
    match run.map(|t| t.join()) {
        Ok(Ok(Ok(()))) => ExitCode::SUCCESS,
        Ok(Ok(Err(e))) | Err(e) => {
            eprintln!("rpn48: {e}");
            ExitCode::FAILURE
        }
        Ok(Err(_)) => ExitCode::FAILURE,
    }
}

fn batch(src: &str) -> ExitCode {
    let mut calc = Calc::new();
    calc.pause = Some(batch_pause());
    let r = parse(src).and_then(|nodes| calc.exec_nodes(&nodes));
    for o in &calc.stack {
        println!("{}", o.show(calc.fmt));
    }
    match r {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{}", e.report());
            ExitCode::FAILURE
        }
    }
}

// Without a display, prompts go to stderr and input comes from stdin; there
// is nothing to do while halted, so HALT is an error.
fn batch_pause() -> PauseFn {
    Box::new(|_, p| {
        let read = || {
            let mut s = String::new();
            io::stdin().read_line(&mut s).map_err(|_| Error::Killed)?;
            Ok(Some(s.trim_end_matches(['\r', '\n']).to_string()))
        };
        match p {
            Pause::Halt => Err(Error::User("HALT needs the display".into())),
            Pause::Prompt(s) | Pause::Message(s) => {
                eprintln!("{s}");
                read().map(|_| None)
            }
            Pause::Input(s, _) => {
                eprint!("{s} ");
                read()
            }
            Pause::Show(s) => {
                eprintln!("{s}");
                Ok(None)
            }
        }
    })
}

thread_local! {
    /// The terminal, shared by the App that runs a program and the one a
    /// halted program hands the calculator to.
    static TERMINAL: RefCell<Option<DefaultTerminal>> = const { RefCell::new(None) };
    /// Set when the calculator was turned off while a program was halted.
    static OFF: Cell<bool> = const { Cell::new(false) };
}

/// A halted program hands the calculator to a fresh App, which runs the
/// usual loop until CONT or KILL (or ENTER, for INPUT), then hands it back.
fn interactive_pause() -> PauseFn {
    Box::new(|calc, p| {
        let mut app = App::with(std::mem::replace(calc, Calc::blank()));
        app.calc.pause = Some(interactive_pause());
        let show = matches!(p, Pause::Show(_));
        match p {
            Pause::Halt | Pause::Show(_) => {}
            Pause::Prompt(s) => app.prompt = Some(s),
            Pause::Message(s) => (app.prompt, app.msgbox) = (Some(s), true),
            Pause::Input(s, d) => {
                app.prompt = Some(s);
                app.input = true;
                app.line = d.chars().collect();
                app.cur = app.line.len();
            }
        }
        let r = if show { draw(&app) } else { run(&mut app) };
        app.calc.pause = None;
        std::mem::swap(calc, &mut app.calc);
        r.map_err(|e| Error::User(e.to_string()))?;
        match app.resume {
            _ if show => Ok(None),
            Some(Resume::Input(s)) => Ok(Some(s)),
            Some(Resume::Cont) => Ok(None),
            Some(Resume::Kill) | None => {
                OFF.set(app.quit);
                Err(Error::Killed)
            }
        }
    })
}

/// How a halted program goes on.
enum Resume {
    Cont,
    Kill,
    Input(String),
}

fn interactive() -> io::Result<()> {
    let mut app = App::new();
    TERMINAL.set(Some(ratatui::init()));
    // Clicks press the keys on the drawn keyboard.
    let r = execute!(io::stdout(), EnableMouseCapture).and_then(|_| run(&mut app));
    let _ = execute!(io::stdout(), DisableMouseCapture);
    ratatui::restore();
    r
}

fn draw(app: &App) -> io::Result<()> {
    TERMINAL.with_borrow_mut(|t| match t {
        Some(t) => t.draw(|f| ui::draw(f, app)).map(drop),
        None => Ok(()),
    })
}

fn run(app: &mut App) -> io::Result<()> {
    while !app.quit && app.resume.is_none() {
        draw(app)?;
        let size = TERMINAL.with_borrow(|t| t.as_ref().map(|t| t.size()).transpose())?;
        let size = size.unwrap_or_default();
        match event::read()? {
            Event::Key(k) if k.kind == KeyEventKind::Press => app.key(k),
            Event::Mouse(m) if m.kind == MouseEventKind::Down(MouseButton::Left) => {
                let keys = keypad::split(Rect::new(0, 0, size.width, size.height)).1;
                if let Some(key) = keys.and_then(|area| keypad::hit(area, m.column, m.row)) {
                    app.press(key);
                }
            }
            _ => {}
        }
    }
    Ok(())
}

/// Esc or Ctrl-C stops a running program, as ON does on the calculator.
fn interrupted() -> bool {
    while event::poll(Duration::ZERO).unwrap_or(false) {
        if let Ok(Event::Key(k)) = event::read()
            && k.kind == KeyEventKind::Press
            && (k.code == KeyCode::Esc
                || (k.code == KeyCode::Char('c') && k.modifiers.contains(KeyModifiers::CONTROL)))
        {
            return true;
        }
    }
    false
}

pub struct App {
    pub calc: Calc,
    pub line: Vec<char>,
    /// Cursor position in `line`, in characters.
    pub cur: usize,
    pub message: Option<String>,
    history: Vec<String>,
    hist: Option<usize>,
    undo: Vec<Snapshot>,
    /// The object Ctrl-E took off the stack, while its text is being edited.
    editing: Option<Obj>,
    /// A shift key clicked on the keyboard, applying to the next key clicked.
    pub shift: Option<Shift>,
    memory: Option<PathBuf>,
    quit: bool,
    /// While a program is halted: its prompt, whether it is waiting for
    /// INPUT or for a key after MSGBOX, and how it is to go on.
    pub prompt: Option<String>,
    input: bool,
    msgbox: bool,
    resume: Option<Resume>,
}

impl App {
    fn new() -> Self {
        let mut calc = Calc::new();
        let mut message = None;
        let memory = persist::path();
        if let Some(path) = &memory
            && let Err(e) = persist::load(&mut calc, path)
        {
            // Keep the unreadable file rather than overwrite it on the first save.
            let bad = path.with_extension("bad");
            let _ = std::fs::rename(path, &bad);
            message = Some(format!(
                "Memory not loaded ({e}); kept as {}",
                bad.display()
            ));
        }
        calc.interrupt = Some(Box::new(interrupted));
        calc.pause = Some(interactive_pause());
        App {
            message,
            ..App::with(calc)
        }
    }

    fn with(calc: Calc) -> Self {
        App {
            calc,
            line: vec![],
            cur: 0,
            message: None,
            history: vec![],
            hist: None,
            undo: vec![],
            editing: None,
            shift: None,
            memory: persist::path(),
            quit: false,
            prompt: None,
            input: false,
            msgbox: false,
            resume: None,
        }
    }

    fn key(&mut self, k: KeyEvent) {
        if self.msgbox {
            return self.resume = Some(Resume::Cont);
        }
        self.unfreeze();
        let had_message = self.message.take().is_some();
        self.shift = None;
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        let act = match k.code {
            KeyCode::Char('q' | 'c') if ctrl => Act::Off,
            KeyCode::Char('z') if ctrl => Act::Undo,
            KeyCode::Char('e') if ctrl => Act::Edit,
            KeyCode::Char('u') if ctrl => Act::Cancel,
            KeyCode::Char(_) if ctrl => Act::Nothing,
            KeyCode::Char(c) => return self.type_char(c),
            KeyCode::Enter => Act::Enter,
            KeyCode::Backspace => Act::Backspace,
            KeyCode::Delete => Act::Delete,
            KeyCode::Right => Act::Right,
            KeyCode::Left => Act::Left,
            KeyCode::Home => return self.cur = 0,
            KeyCode::End => return self.cur = self.line.len(),
            KeyCode::Up => Act::Up,
            KeyCode::Down => Act::Down,
            KeyCode::Tab => Act::NextPage,
            KeyCode::BackTab => Act::PrevPage,
            KeyCode::F(n @ 1..=6) => Act::Soft(n as usize - 1),
            KeyCode::F(n @ 7..=11) => Act::Menu(Menu::FKEYS[n as usize - 7]),
            KeyCode::Esc if !had_message => Act::Cancel,
            _ => Act::Nothing,
        };
        self.act(act, "");
    }

    /// A click on the drawn keyboard: the key's own function, or after a
    /// shift key, the shifted one printed above it.
    fn press(&mut self, key: &Key) {
        if self.msgbox {
            return self.resume = Some(Resume::Cont);
        }
        self.unfreeze();
        self.message = None;
        let (label, act) = match self.shift.take() {
            _ if matches!(key.main, Act::Shift(_)) => (key.face, key.main),
            None => (key.face, key.main),
            Some(Shift::Left) => key.left,
            Some(Shift::Right) => key.right,
        };
        self.act(act, label);
    }

    fn act(&mut self, act: Act, label: &str) {
        let empty = self.line.is_empty();
        match act {
            Act::Nothing => {}
            Act::Unsupported => self.message = Some(format!("{label} is not supported")),
            Act::Soft(i) => self.soft_key(i),
            // In the VAR menu, left- and right-shifted soft keys store into
            // and recall the variable; in a units menu they convert to the unit
            // and divide by it; elsewhere they act unshifted.
            Act::SoftSto(i) | Act::SoftRcl(i) => {
                let name = self.calc.menu.page(&self.calc, self.calc.page)[i].clone();
                if let (Menu::UnitCat(_), false) = (self.calc.menu, name.is_empty()) {
                    let left = matches!(act, Act::SoftSto(_));
                    self.unit_key(&name, if left { "CONVERT" } else { "/" });
                } else if self.calc.menu == Menu::Var && !name.is_empty() {
                    let cmd = if matches!(act, Act::SoftSto(_)) {
                        "STO"
                    } else {
                        "RCL"
                    };
                    self.exec(&format!("'{name}' {cmd}"));
                } else {
                    self.soft_key(i);
                }
            }
            Act::Menu(m) => (self.calc.menu, self.calc.page) = (m, 0),
            Act::Cmd(c) => self.command(c),
            // The keyboard has +/- for signs, so its - key subtracts even on an
            // empty line.
            Act::Op(c) if empty => self.exec(&c.to_string()),
            Act::Op(c) => self.type_char(c),
            Act::Text(s, back) => {
                self.line.splice(self.cur..self.cur, s.chars());
                self.cur += s.chars().count() - back;
            }
            Act::Enter if self.input => {
                self.resume = Some(Resume::Input(self.line.iter().collect()))
            }
            Act::Enter => self.exec(if empty { "DUP" } else { "" }),
            Act::Backspace if empty => self.exec("DROP"),
            Act::Backspace => {
                if self.cur > 0 {
                    self.cur -= 1;
                    self.line.remove(self.cur);
                }
            }
            Act::Delete => {
                if self.cur < self.line.len() {
                    self.line.remove(self.cur);
                }
            }
            Act::Right if empty => self.exec("SWAP"),
            Act::Right => self.cur = (self.cur + 1).min(self.line.len()),
            Act::Left => self.cur = self.cur.saturating_sub(1),
            Act::Up => self.recall(true),
            Act::Down => self.recall(false),
            Act::Neg => self.negate(),
            Act::Edit => self.edit(),
            Act::Undo => self.undo(),
            Act::NextPage => {
                self.calc.page = (self.calc.page + 1) % self.calc.menu.pages(&self.calc)
            }
            Act::PrevPage => {
                let pages = self.calc.menu.pages(&self.calc);
                self.calc.page = (self.calc.page % pages + pages - 1) % pages;
            }
            Act::AngleToggle => self.exec(if self.calc.angle == Angle::Rad {
                "DEG"
            } else {
                "RAD"
            }),
            Act::Shift(s) => self.shift = if self.shift == Some(s) { None } else { Some(s) },
            Act::Cancel if self.input => self.resume = Some(Resume::Kill),
            Act::Cancel => self.cancel(),
            Act::Off => self.quit = true,
        }
    }

    // +/- flips the sign of the number being typed, or of its exponent after
    // EEX; with no number at the cursor it runs NEG.
    fn negate(&mut self) {
        let start = self
            .line
            .iter()
            .rposition(|c| c.is_whitespace())
            .map_or(0, |i| i + 1);
        let tok: String = self.line[start..].iter().collect();
        let number = tok.chars().any(|c| c.is_ascii_digit())
            && tok
                .chars()
                .all(|c| c.is_ascii_digit() || ".Ee-+".contains(c));
        if self.cur != self.line.len() || !number {
            return self.exec("NEG");
        }
        let at = match tok.find(['E', 'e']) {
            Some(i) => start + tok[..i].chars().count() + 1,
            None => start,
        };
        if self.line.get(at) == Some(&'-') {
            self.line.remove(at);
            self.cur -= 1;
        } else {
            self.line.insert(at, '-');
            self.cur += 1;
        }
    }

    // Typing an operator after a number runs it at once, as the calculator's
    // keys do: 2 ENTER 3 + gives 5. A '-' on an empty line starts a negative
    // number instead, and nothing runs inside an unfinished program or string.
    fn type_char(&mut self, c: char) {
        if "+-*/^".contains(c) && self.cur == self.line.len() {
            let line: String = self.line.iter().collect();
            let last = line.split_whitespace().last().unwrap_or("");
            let now = if line.trim().is_empty() {
                c != '-'
            } else {
                !self.input
                    && !is_open(&line)
                    && (parse_num(last).is_some() || last.ends_with(['"', ')', '}', '»']))
            };
            if now {
                self.exec(&c.to_string());
                return;
            }
        }
        self.line.insert(self.cur, c);
        self.cur += 1;
    }

    fn soft_key(&mut self, i: usize) {
        let label = self.calc.menu.page(&self.calc, self.calc.page)[i].clone();
        if label.is_empty() {
            return;
        }
        match self.calc.menu.sub(&label) {
            Some(m) => (self.calc.menu, self.calc.page) = (m, 0),
            None if matches!(self.calc.menu, Menu::UnitCat(_)) => self.unit_key(&label, "*"),
            None => self.command(&label),
        }
    }

    // A unit's key gives the number on the line or stack those units; shifted,
    // it converts to them (left) or divides by them (right).
    fn unit_key(&mut self, unit: &str, cmd: &str) {
        if cmd == "*" && self.line.is_empty() && self.calc.stack.is_empty() {
            self.command(&format!("1_{unit}"));
        } else {
            self.command(&format!("1_{unit} {cmd}"));
        }
    }

    // A command key runs its command after the command line, except while a
    // program is being typed, where the command's name goes into the text.
    fn command(&mut self, cmd: &str) {
        if self.input || is_open(&self.line.iter().collect::<String>()) {
            let text = format!(" {cmd} ");
            self.line.splice(self.cur..self.cur, text.chars());
            self.cur += text.chars().count();
        } else {
            self.exec(cmd);
        }
    }

    /// Runs the command line followed by `cmd`. On a syntax error nothing runs
    /// and the line stays for correcting.
    fn exec(&mut self, cmd: &str) {
        let line: String = self.line.iter().collect();
        let nodes = match parse(&format!("{line} {cmd}")) {
            Ok(n) => n,
            Err(e) => {
                self.message = Some(e.report());
                return;
            }
        };
        if !line.trim().is_empty() {
            self.history.push(line);
        }
        self.clear_line();
        // Editing and entering an object is one step to undo.
        if let Some(o) = self.editing.take() {
            self.calc.stack.push(o);
            self.push_undo();
            self.calc.stack.pop();
        } else {
            self.push_undo();
        }
        match self.calc.exec_nodes(&nodes) {
            Err(Error::Killed) => {
                self.quit |= OFF.take();
                if self.calc.halted > 0 {
                    self.resume = Some(Resume::Kill);
                }
            }
            Err(e) => {
                self.message = Some(e.report());
                self.calc.last_err = Some(e);
            }
            Ok(()) => {}
        }
        if std::mem::take(&mut self.calc.cont) {
            self.resume = Some(Resume::Cont);
        }
        if !self.calc.frozen {
            self.calc.disp.clear();
        }
        self.save();
    }

    /// A key after FREEZE takes the DISP lines down.
    fn unfreeze(&mut self) {
        if self.calc.frozen {
            self.calc.frozen = false;
            self.calc.disp.clear();
        }
    }

    fn push_undo(&mut self) {
        self.undo.push(self.calc.snapshot());
        if self.undo.len() > UNDO_LEVELS {
            self.undo.remove(0);
        }
    }

    fn undo(&mut self) {
        if self.editing.is_some() {
            return self.cancel();
        }
        match self.undo.pop() {
            Some(s) => {
                self.calc.restore(s);
                self.save();
            }
            None => self.message = Some("Nothing to undo".into()),
        }
    }

    /// Moves level 1 into the command line for editing. Memory isn't saved
    /// until the edit is entered, so quitting mid-edit loses nothing.
    fn edit(&mut self) {
        if !self.line.is_empty() || self.calc.stack.is_empty() {
            return;
        }
        let o = self.calc.stack.pop().unwrap();
        self.line = o.show(Fmt::Std).chars().collect();
        self.cur = self.line.len();
        self.editing = Some(o);
    }

    /// Clears the command line, putting back an object that was being edited.
    fn cancel(&mut self) {
        if let Some(o) = self.editing.take() {
            self.calc.stack.push(o);
        }
        self.clear_line();
    }

    fn recall(&mut self, older: bool) {
        let n = self.history.len();
        self.hist = match (self.hist, older) {
            _ if n == 0 => return,
            (None, true) => Some(n - 1),
            (None, false) => return,
            (Some(i), true) => Some(i.saturating_sub(1)),
            (Some(i), false) => (i + 1 < n).then_some(i + 1),
        };
        self.line = self
            .hist
            .map(|i| self.history[i].chars().collect())
            .unwrap_or_default();
        self.cur = self.line.len();
    }

    fn clear_line(&mut self) {
        self.line.clear();
        self.cur = 0;
        self.hist = None;
    }

    fn save(&mut self) {
        if let Some(path) = &self.memory
            && let Err(e) = persist::save(&self.calc, path)
        {
            self.message = Some(format!("Memory not saved: {e}"));
        }
    }
}

//! rpn48: an RPN calculator for the terminal in the style of the HP 48.

mod alarms;
mod alg;
mod array;
mod binary;
mod calc;
mod carray;
mod consts;
mod keypad;
mod keys;
mod lists;
mod matrix;
mod menu;
mod more;
mod numeric;
mod obj;
mod parse;
mod pattern;
mod pause;
mod persist;
mod plot;
mod plot3d;
mod pretty;
mod rearrange;
mod solve;
mod stat;
mod symbolic;
mod system;
mod transfer;
mod ui;
mod units;
mod writer;

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
use obj::{Coord, Fmt, Obj};
use parse::{is_open, parse, parse_num};
use pause::{Pause, PauseFn};
use pretty::Pic;
use writer::MatrixWriter;

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
            Pause::Key(_) => Ok(None),
        }
    })
}

thread_local! {
    /// The terminal, shared by the App that runs a program and the one a
    /// halted program hands the calculator to.
    static TERMINAL: RefCell<Option<DefaultTerminal>> = const { RefCell::new(None) };
    /// Set when the calculator was turned off while a program was halted.
    static OFF: Cell<bool> = const { Cell::new(false) };
    /// Events read while checking a running program for Esc.
    static PENDING: RefCell<std::collections::VecDeque<Event>> = RefCell::default();
}

/// A halted program hands the calculator to a fresh App, which runs the
/// usual loop until CONT or KILL (or ENTER, for INPUT), then hands it back.
fn interactive_pause() -> PauseFn {
    Box::new(|calc, p| {
        if let Pause::Key(wait) = p {
            return read_key(calc, wait);
        }
        let mut app = App::with(std::mem::replace(calc, Calc::blank()));
        app.calc.pause = Some(interactive_pause());
        let show = matches!(p, Pause::Show(_));
        match p {
            Pause::Halt | Pause::Show(_) | Pause::Key(_) => {}
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

/// KEY and 0 WAIT: the code of the next key pressed, typed or clicked, as
/// rc.p; a click on a shift key first chooses the plane.
fn read_key(calc: &mut Calc, wait: bool) -> Result<Option<String>, Error> {
    let app = App::with(std::mem::replace(calc, Calc::blank()));
    let drawn = draw(&app);
    *calc = app.calc;
    drawn.map_err(|e| Error::User(e.to_string()))?;
    let mut plane = 1;
    loop {
        let wait = (!wait).then_some(Duration::ZERO);
        let Some(ev) = next_event(wait).map_err(|e| Error::User(e.to_string()))? else {
            return Ok(None);
        };
        let key = match ev {
            Event::Key(k) if k.kind == KeyEventKind::Press => {
                if k.code == KeyCode::Char('c') && k.modifiers.contains(KeyModifiers::CONTROL) {
                    return Err(Error::Interrupted);
                }
                typed_key(&k).map(|(key, _)| key)
            }
            Event::Mouse(m) if m.kind == MouseEventKind::Down(MouseButton::Left) => {
                let size = TERMINAL.with_borrow(|t| t.as_ref().map(|t| t.size()).transpose());
                let size = size.ok().flatten().unwrap_or_default();
                keypad::split(Rect::new(0, 0, size.width, size.height))
                    .1
                    .and_then(|area| keypad::hit(area, m.column, m.row))
            }
            _ => None,
        };
        match key.map(|k| k.main) {
            Some(Act::Shift(Shift::Left)) => plane = 2,
            Some(Act::Shift(Shift::Right)) => plane = 3,
            Some(_) => return Ok(Some(format!("{}.{plane}", keypad::code(key.unwrap())))),
            None => {}
        }
    }
}

/// The drawn keyboard's key a typed key stands for, and its shift plane:
/// digits and operators, ENTER, the arrows, and F1–F6 for the soft keys,
/// Shift and Alt giving their shifted planes.
fn typed_key(k: &KeyEvent) -> Option<(&'static Key, u32)> {
    let act = match k.code {
        KeyCode::Char(ch) => {
            return keypad::find(|key| match key.main {
                Act::Text(s, 0) => s.chars().eq([ch]),
                Act::Op(o) => o == ch,
                _ => false,
            })
            .map(|key| (key, 1));
        }
        KeyCode::F(n @ 1..=6) => {
            let plane = if k.modifiers.contains(KeyModifiers::SHIFT) {
                2
            } else if k.modifiers.contains(KeyModifiers::ALT) {
                3
            } else {
                1
            };
            return keypad::find(|key| key.main == Act::Soft(n as usize - 1))
                .map(|key| (key, plane));
        }
        KeyCode::Enter => Act::Enter,
        KeyCode::Backspace => Act::Backspace,
        KeyCode::Delete => Act::Delete,
        KeyCode::Left => Act::Left,
        KeyCode::Right => Act::Right,
        KeyCode::Up => Act::Up,
        KeyCode::Down => Act::Down,
        KeyCode::Esc => Act::Cancel,
        _ => return None,
    };
    keypad::find(|key| key.main == act).map(|key| (key, 1))
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
        app.alarms();
        draw(app)?;
        let size = TERMINAL.with_borrow(|t| t.as_ref().map(|t| t.size()).transpose())?;
        let size = size.unwrap_or_default();
        // Waking each second to see to the alarms.
        let Some(ev) = next_event(Some(Duration::from_secs(1)))? else {
            continue;
        };
        match ev {
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
/// Other events that arrive meanwhile are kept for KEY and for afterwards.
fn interrupted() -> bool {
    while event::poll(Duration::ZERO).unwrap_or(false) {
        let Ok(ev) = event::read() else { break };
        if let Event::Key(k) = &ev
            && k.kind == KeyEventKind::Press
            && (k.code == KeyCode::Esc
                || (k.code == KeyCode::Char('c') && k.modifiers.contains(KeyModifiers::CONTROL)))
        {
            return true;
        }
        PENDING.with_borrow_mut(|p| p.push_back(ev));
    }
    false
}

/// The next event: one kept while a program ran, or else from the terminal,
/// waiting for it at most `wait`, or for as long as it takes.
fn next_event(wait: Option<Duration>) -> io::Result<Option<Event>> {
    if let Some(ev) = PENDING.with_borrow_mut(|p| p.pop_front()) {
        return Ok(Some(ev));
    }
    if let Some(w) = wait
        && !event::poll(w)?
    {
        return Ok(None);
    }
    event::read().map(Some)
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
    /// ENTRY: commands are typed into the line rather than run.
    pub entry: bool,
    /// The level the interactive stack has picked.
    pub stack_sel: Option<usize>,
    pub matrix: Option<MatrixWriter>,
    /// What VIEW is showing, and how far it is scrolled (rows, columns).
    pub view: Option<(Pic, usize, usize)>,
    /// The equation writer: the line is an algebraic, drawn out as it is typed.
    pub eqw: bool,
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
            entry: false,
            stack_sel: None,
            matrix: None,
            view: None,
            eqw: false,
        }
    }

    fn key(&mut self, k: KeyEvent) {
        if self.msgbox {
            return self.resume = Some(Resume::Cont);
        }
        self.unfreeze();
        if self.calc.plot.is_some() {
            return self.plot_key(k.code);
        }
        if self.stack_sel.is_some() {
            return self.stack_key(k.code);
        }
        if self.matrix.is_some() || self.view.is_some() {
            // Of the control keys only quitting works here.
            if k.modifiers.contains(KeyModifiers::CONTROL) {
                self.quit |= matches!(k.code, KeyCode::Char('q' | 'c'));
            } else if self.matrix.is_some() {
                self.matrix_key(k.code);
            } else {
                self.view_key(k.code);
            }
            return;
        }
        if self.eqw && self.equation_key(&k) {
            return;
        }
        if self.calc.user_mode()
            && let Some((key, plane)) = typed_key(&k)
            && self.user_key(key, plane)
        {
            return;
        }
        let had_message = self.message.take().is_some();
        self.shift = None;
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        let act = match k.code {
            KeyCode::Char('q' | 'c') if ctrl => Act::Off,
            KeyCode::Char('z') if ctrl => Act::Undo,
            KeyCode::Char('e') if ctrl => Act::Edit,
            KeyCode::Char('s') if ctrl => Act::Stack,
            KeyCode::Char('v') if ctrl => Act::View,
            KeyCode::Char('w') if ctrl => Act::Equation,
            KeyCode::Char('t') if ctrl => Act::Matrix,
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
            // Shift and Alt stand in for the calculator's ◀ and ▶ shifts.
            KeyCode::F(n @ 1..=6) if k.modifiers.contains(KeyModifiers::SHIFT) => {
                Act::SoftSto(n as usize - 1)
            }
            KeyCode::F(n @ 1..=6) if k.modifiers.contains(KeyModifiers::ALT) => {
                Act::SoftRcl(n as usize - 1)
            }
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
        if self.calc.plot.take().is_some() || self.view.take().is_some() || self.matrix.is_some() {
            return;
        }
        let plane = match self.shift {
            None => 1,
            Some(Shift::Left) => 2,
            Some(Shift::Right) => 3,
        };
        if self.calc.user_mode() && !matches!(key.main, Act::Shift(_)) && self.user_key(key, plane)
        {
            self.shift = None;
            return;
        }
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
                let solvr = (self.calc.menu == Menu::Solvr && name != "EXPR="
                    || self.calc.menu == Menu::Msolvr && name != "ALL")
                    && !name.is_empty();
                if solvr {
                    self.solvr_key(&name, Some(matches!(act, Act::SoftSto(_))));
                } else if let (Menu::UnitCat(_), false) = (self.calc.menu, name.is_empty()) {
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
            Act::Op(c) if empty && !self.eqw => self.exec(&c.to_string()),
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
            Act::UserToggle => self.calc.toggle_user(),
            Act::EntryToggle => self.entry = !self.entry,
            Act::Stack if !self.calc.stack.is_empty() => self.stack_sel = Some(1),
            Act::Stack => {}
            Act::View if self.eqw => {}
            Act::View => self.view_level(),
            Act::Matrix => self.matrix = Some(MatrixWriter::new(vec![], false)),
            Act::Equation => {
                self.clear_line();
                self.eqw = true;
            }
            Act::PolarToggle => self.exec(if self.calc.coord == Coord::Rect {
                "CYLIN"
            } else {
                "RECT"
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
        if "+-*/^".contains(c) && self.cur == self.line.len() && !self.eqw {
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
            None if self.calc.menu == Menu::Chars => {
                self.line.splice(self.cur..self.cur, label.chars());
                self.cur += label.chars().count();
            }
            None if self.calc.menu == Menu::Solvr && label != "EXPR=" => {
                self.solvr_key(&label, None)
            }
            None if self.calc.menu == Menu::Msolvr && label == "ALL" => self.command("'ALL' MROOT"),
            None if self.calc.menu == Menu::Msolvr => self.solvr_key(&label, None),
            None => self.command(&label),
        }
    }

    // A variable's key in SOLVR stores level 1 (or the number typed) in it;
    // left-shifted it solves EQ for the variable, right-shifted recalls it.
    fn solvr_key(&mut self, name: &str, shift: Option<bool>) {
        match shift {
            None => {
                self.exec(&format!("'{name}' STO"));
                if self.message.is_none()
                    && let Some(v) = self.calc.global(name)
                {
                    self.message = Some(format!("{name}: {}", v.show(self.calc.fmt)));
                }
            }
            Some(true) => {
                if !self.line.is_empty() {
                    self.exec("");
                    if self.message.is_some() {
                        return;
                    }
                }
                self.push_undo();
                self.message = Some(match self.calc.solve_for(name) {
                    Ok(m) => m.into(),
                    Err(e) => e.report(),
                });
                self.save();
            }
            Some(false) => self.exec(&format!("'{name}' RCL")),
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
        // In the equation writer a function key types the function, ready
        // for its argument.
        if self.eqw {
            let func = cmd
                .chars()
                .all(|c| c.is_alphanumeric() || "√∂∫Σ".contains(c));
            let (text, back) = if func {
                (format!("{cmd}()"), 1)
            } else {
                (cmd.to_string(), 0)
            };
            self.line.splice(self.cur..self.cur, text.chars());
            self.cur += text.chars().count() - back;
            return;
        }
        if self.input || self.entry || is_open(&self.line.iter().collect::<String>()) {
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

    // While the plot is up the arrows move its cursor and ENTER puts the
    // cursor's coordinates on the stack, as (x,y); any other key leaves.
    fn plot_key(&mut self, code: KeyCode) {
        let Some(p) = &mut self.calc.plot else { return };
        let (dx, dy) = ((p.x.1 - p.x.0) / 40.0, (p.y.1 - p.y.0) / 20.0);
        let (x, y) = &mut p.cursor;
        match code {
            KeyCode::Left => *x -= dx,
            KeyCode::Right => *x += dx,
            KeyCode::Up => *y += dy,
            KeyCode::Down => *y -= dy,
            KeyCode::Enter => {
                let c = Obj::Complex(obj::r12(*x), obj::r12(*y));
                self.push_undo();
                self.calc.stack.push(c);
                self.save();
            }
            _ => self.calc.plot = None,
        }
    }

    /// In USER mode a key with an object assigned runs it; false when the
    /// key has none and does its own function.
    /// Announces the appointment alarms that have come due, with a beep,
    /// and runs the control alarms' objects.
    fn alarms(&mut self) {
        let (say, run) = self.calc.due_alarms();
        if let Some(s) = say.last() {
            self.message = Some(format!("ALARM: {s}"));
            print!("\x07");
            let _ = io::Write::flush(&mut io::stdout());
        }
        for o in run {
            self.run_obj(o);
        }
        if !say.is_empty() {
            self.save();
        }
    }

    /// Runs an object as a key assigned it would.
    fn run_obj(&mut self, o: Obj) {
        let src = o.show(Fmt::Std);
        self.command(&match o {
            Obj::Prog(_) => format!("{src} EVAL"),
            Obj::Name(n) => n,
            _ => src,
        });
    }

    fn user_key(&mut self, key: &Key, plane: u32) -> bool {
        let code = keypad::code(key) as f64 + plane as f64 / 10.0;
        let Some(o) = self.calc.user_key(code) else {
            return false;
        };
        self.run_obj(o);
        true
    }

    /// VIEW: an array goes to the matrix writer to edit; anything else is
    /// drawn out to look at.
    fn view_level(&mut self) {
        if !self.line.is_empty() {
            self.exec("");
        }
        let Some(o) = self.calc.stack.last() else {
            return self.message = Some(Error::TooFewArgs.report());
        };
        match pretty::cells(o, Fmt::Std) {
            Some(cells) => self.matrix = Some(MatrixWriter::new(cells, true)),
            None => self.view = Some((pretty::view(o, self.calc.fmt), 0, 0)),
        }
    }

    fn view_key(&mut self, code: KeyCode) {
        let Some((_, r, c)) = &mut self.view else {
            return;
        };
        match code {
            KeyCode::Up => *r = r.saturating_sub(1),
            KeyCode::Down => *r += 1,
            KeyCode::Left => *c = c.saturating_sub(4),
            KeyCode::Right => *c += 4,
            _ => self.view = None,
        }
    }

    fn matrix_key(&mut self, code: KeyCode) {
        let Some(m) = &mut self.matrix else { return };
        match m.key(code) {
            writer::Done::Not => {}
            writer::Done::Cancel => self.matrix = None,
            writer::Done::Enter(src) => {
                let replace = m.replace;
                self.matrix = None;
                self.exec(&format!("{} {src}", if replace { "DROP" } else { "" }));
            }
        }
    }

    /// Keys the equation writer takes itself: text goes into the line as it
    /// is, ENTER enters it as an algebraic and Esc leaves.
    fn equation_key(&mut self, k: &KeyEvent) -> bool {
        let empty = self.line.is_empty();
        match k.code {
            KeyCode::Char(c) if !k.modifiers.contains(KeyModifiers::CONTROL) => {
                self.line.insert(self.cur, c);
                self.cur += 1;
            }
            KeyCode::Enter if empty => self.eqw = false,
            // Parentheses left open are closed, as the preview shows them.
            KeyCode::Enter => {
                let src: String = self.line.iter().collect();
                let open = src
                    .matches('(')
                    .count()
                    .saturating_sub(src.matches(')').count());
                self.clear_line();
                self.exec(&format!("'{src}{}'", ")".repeat(open)));
                if self.message.is_some() {
                    self.line = src.chars().collect();
                    self.cur = self.line.len();
                } else {
                    self.eqw = false;
                }
            }
            KeyCode::Esc => {
                self.clear_line();
                self.eqw = false;
            }
            // Nothing on the stack is touched from here.
            KeyCode::Backspace | KeyCode::Right | KeyCode::Up | KeyCode::Down if empty => {}
            KeyCode::Up | KeyCode::Down => {}
            _ => return false,
        }
        true
    }

    // The interactive stack: the arrows pick a level, which ENTER (or e)
    // copies to the command line, p copies to level 1, r and d roll up to
    // level 1 or down from it, Delete drops, and Esc leaves.
    fn stack_key(&mut self, code: KeyCode) {
        let Some(n) = self.stack_sel else { return };
        let depth = self.calc.stack.len();
        let mut step = |cmd: String| {
            self.push_undo();
            if let Err(e) = self.calc.exec_nodes(&parse(&cmd).unwrap_or_default()) {
                self.message = Some(e.report());
            }
            self.save();
        };
        match code {
            KeyCode::Up if n < depth => self.stack_sel = Some(n + 1),
            KeyCode::Down if n > 1 => self.stack_sel = Some(n - 1),
            KeyCode::Char('p') => step(format!("{n} PICK")),
            KeyCode::Char('r') => step(format!("{n} ROLL")),
            KeyCode::Char('d') => step(format!("{n} ROLLD")),
            KeyCode::Delete | KeyCode::Backspace => {
                step(format!("{n} ROLL DROP"));
                self.stack_sel = Some(n.min(self.calc.stack.len()).max(1));
            }
            KeyCode::Enter | KeyCode::Char('e') => {
                let text = self.calc.stack[depth - n].show(Fmt::Std);
                self.line.splice(self.cur..self.cur, text.chars());
                self.cur += text.chars().count();
                self.stack_sel = None;
            }
            KeyCode::Up | KeyCode::Down => {}
            _ => self.stack_sel = None,
        }
        if self.calc.stack.is_empty() {
            self.stack_sel = None;
        }
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

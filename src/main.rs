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
mod persist;
mod stat;
mod symbolic;
mod ui;

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

use calc::{Angle, Calc, Snapshot};
use keypad::{Act, Key, Shift};
use menu::Menu;
use obj::{Fmt, Obj};
use parse::{is_open, parse, parse_num};

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

fn interactive() -> io::Result<()> {
    let mut app = App::new();
    let mut terminal = ratatui::init();
    // Clicks press the keys on the drawn keyboard.
    let r = execute!(io::stdout(), EnableMouseCapture).and_then(|_| run(&mut terminal, &mut app));
    let _ = execute!(io::stdout(), DisableMouseCapture);
    ratatui::restore();
    r
}

fn run(terminal: &mut DefaultTerminal, app: &mut App) -> io::Result<()> {
    while !app.quit {
        terminal.draw(|f| ui::draw(f, app))?;
        let size = terminal.size()?;
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
        App {
            calc,
            line: vec![],
            cur: 0,
            message,
            history: vec![],
            hist: None,
            undo: vec![],
            editing: None,
            shift: None,
            memory,
            quit: false,
        }
    }

    fn key(&mut self, k: KeyEvent) {
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
            // and recall the variable; elsewhere they act unshifted.
            Act::SoftSto(i) | Act::SoftRcl(i) => {
                let name = self.calc.menu.page(&self.calc, self.calc.page)[i].clone();
                if self.calc.menu == Menu::Var && !name.is_empty() {
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
                !is_open(&line)
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
            None => self.command(&label),
        }
    }

    // A command key runs its command after the command line, except while a
    // program is being typed, where the command's name goes into the text.
    fn command(&mut self, cmd: &str) {
        if is_open(&self.line.iter().collect::<String>()) {
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
        if let Err(e) = self.calc.exec_nodes(&nodes) {
            self.message = Some(e.report());
            self.calc.last_err = Some(e);
        }
        self.save();
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

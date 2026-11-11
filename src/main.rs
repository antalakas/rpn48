//! rpn48: an RPN calculator for the terminal in the style of the HP 48.

mod calc;
mod menu;
mod obj;
mod parse;
mod persist;
mod ui;

use std::io;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use calc::{Calc, Snapshot};
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
    let r = run(&mut terminal, &mut app);
    ratatui::restore();
    r
}

fn run(terminal: &mut DefaultTerminal, app: &mut App) -> io::Result<()> {
    while !app.quit {
        terminal.draw(|f| ui::draw(f, app))?;
        if let Event::Key(k) = event::read()?
            && k.kind == KeyEventKind::Press
        {
            app.key(k);
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
            memory,
            quit: false,
        }
    }

    fn key(&mut self, k: KeyEvent) {
        let had_message = self.message.take().is_some();
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        let empty = self.line.is_empty();
        match k.code {
            KeyCode::Char('q' | 'c') if ctrl => self.quit = true,
            KeyCode::Char('z') if ctrl => self.undo(),
            KeyCode::Char('e') if ctrl => self.edit(),
            KeyCode::Char('u') if ctrl => self.cancel(),
            KeyCode::Char(_) if ctrl => {}
            KeyCode::Char(c) => self.type_char(c),
            KeyCode::Enter => self.exec(if empty { "DUP" } else { "" }),
            KeyCode::Backspace if empty => self.exec("DROP"),
            KeyCode::Backspace if self.cur > 0 => {
                self.cur -= 1;
                self.line.remove(self.cur);
            }
            KeyCode::Delete if self.cur < self.line.len() => {
                self.line.remove(self.cur);
            }
            KeyCode::Right if empty => self.exec("SWAP"),
            KeyCode::Right => self.cur = (self.cur + 1).min(self.line.len()),
            KeyCode::Left => self.cur = self.cur.saturating_sub(1),
            KeyCode::Home => self.cur = 0,
            KeyCode::End => self.cur = self.line.len(),
            KeyCode::Up => self.recall(true),
            KeyCode::Down => self.recall(false),
            KeyCode::Tab => {
                self.calc.page = (self.calc.page + 1) % self.calc.menu.pages(&self.calc)
            }
            KeyCode::BackTab => {
                let pages = self.calc.menu.pages(&self.calc);
                self.calc.page = (self.calc.page % pages + pages - 1) % pages;
            }
            KeyCode::F(n @ 1..=6) => self.soft_key(n as usize - 1),
            KeyCode::F(n @ 7..=11) => {
                (self.calc.menu, self.calc.page) = (Menu::ALL[n as usize - 7], 0)
            }
            KeyCode::Esc if !had_message => self.cancel(),
            _ => {}
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

    // A soft key runs its command after the command line, except while a
    // program is being typed, where the command's name goes into the text.
    fn soft_key(&mut self, i: usize) {
        let label = self.calc.menu.page(&self.calc, self.calc.page)[i].clone();
        if label.is_empty() {
            return;
        }
        if is_open(&self.line.iter().collect::<String>()) {
            let text = format!(" {label} ");
            self.line.splice(self.cur..self.cur, text.chars());
            self.cur += text.chars().count();
        } else {
            self.exec(&label);
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

//! The stack, variables and modes, and the commands that act on them.

use std::cmp::Ordering;
use std::f64::consts::{E, PI};
use std::fmt;

use crate::alg::{self, SYMBOLIC, is_symbolic};
use crate::menu::Menu;
use crate::obj::{Base, Coord, Fmt, Node, Obj, r12, set_base, set_coord};
use crate::parse::parse;
use crate::pause::PauseFn;
use crate::plot::{Plot, Shape};
use crate::{array, binary, carray, lists, matrix, more, stat, units};

#[derive(Clone, Debug, PartialEq)]
pub enum Error {
    TooFewArgs,
    BadType,
    BadValue,
    Undefined,
    UndefinedResult,
    Infinite,
    Syntax,
    Dim,
    InvalidUnit,
    InconsistentUnits,
    DirNotAllowed,
    NonEmptyDir,
    Memory,
    Interrupted,
    /// KILL, or ON at a prompt: the program stops, and nothing is reported.
    Killed,
    /// DOERR with a message of the program's own.
    User(String),
    /// DOERR with an error number that has no message here.
    Code(u32),
    /// An error raised by a named command, reported as "SIN Error: ...".
    At(String, Box<Error>),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(match self {
            Error::TooFewArgs => "Too Few Arguments",
            Error::BadType => "Bad Argument Type",
            Error::BadValue => "Bad Argument Value",
            Error::Undefined => "Undefined Name",
            Error::UndefinedResult => "Undefined Result",
            Error::Infinite => "Infinite Result",
            Error::Syntax => "Invalid Syntax",
            Error::Dim => "Invalid Dimension",
            Error::InvalidUnit => "Invalid Unit",
            Error::InconsistentUnits => "Inconsistent Units",
            Error::DirNotAllowed => "Directory Not Allowed",
            Error::NonEmptyDir => "Non-Empty Directory",
            Error::Memory => "Insufficient Memory",
            Error::Interrupted => "Interrupted",
            Error::Killed => "Killed",
            Error::User(m) => m,
            Error::Code(n) => return write!(f, "Error #{n:X}h"),
            Error::At(cmd, e) => return write!(f, "{cmd} Error: {e}"),
        })
    }
}

/// The calculator's error numbers, which ERRN returns and DOERR takes.
const CODES: &[(u32, Error)] = &[
    (0x001, Error::Memory),
    (0x106, Error::Syntax),
    (0x12A, Error::DirNotAllowed),
    (0x12B, Error::NonEmptyDir),
    (0x201, Error::TooFewArgs),
    (0x202, Error::BadType),
    (0x203, Error::BadValue),
    (0x204, Error::Undefined),
    (0x304, Error::UndefinedResult),
    (0x305, Error::Infinite),
    (0x501, Error::Dim),
    (0xB01, Error::InvalidUnit),
    (0xB02, Error::InconsistentUnits),
];

impl Error {
    /// The message as the calculator shows it.
    pub fn report(&self) -> String {
        match self {
            Error::At(..) => self.to_string(),
            e => format!("Error: {e}"),
        }
    }

    pub fn number(&self) -> u32 {
        match self {
            Error::At(_, e) => e.number(),
            Error::User(_) => 0x70000,
            Error::Code(n) => *n,
            e => CODES.iter().find(|(_, c)| c == e).map_or(0, |(n, _)| *n),
        }
    }

    pub fn from_number(n: u32) -> Error {
        CODES
            .iter()
            .find(|(c, _)| *c == n)
            .map_or(Error::Code(n), |(_, e)| e.clone())
    }

    /// The error without the command that raised it, as ERRM reports it.
    pub(crate) fn bare(&self) -> &Error {
        match self {
            Error::At(_, e) => e.bare(),
            e => e,
        }
    }
}

pub type Res<T> = Result<T, Error>;

pub fn at(cmd: &str, e: Error) -> Error {
    match e {
        Error::At(..) | Error::Interrupted | Error::Killed | Error::Memory => e,
        e => Error::At(cmd.into(), Box::new(e)),
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Angle {
    Deg,
    Rad,
    Grad,
}

impl Angle {
    pub fn name(self) -> &'static str {
        match self {
            Angle::Deg => "DEG",
            Angle::Rad => "RAD",
            Angle::Grad => "GRAD",
        }
    }
}

/// Everything UNDO restores.
#[derive(Clone)]
pub struct Snapshot {
    stack: Vec<Obj>,
    vars: Vec<(String, Obj)>,
    path: Vec<String>,
    angle: Angle,
    coord: Coord,
    fmt: Fmt,
    base: Base,
    wordsize: u32,
    flags: [u64; 2],
}

/// Nesting limit for programs and names that evaluate one another.
const MAX_DEPTH: usize = 400;

pub struct Calc {
    /// Level 1 is the last element.
    pub stack: Vec<Obj>,
    /// The HOME directory's variables, newest first as the VAR menu lists
    /// them. Subdirectories are `Obj::Dir` values among them.
    pub vars: Vec<(String, Obj)>,
    /// The current directory, as the names of the directories below HOME.
    pub path: Vec<String>,
    pub angle: Angle,
    pub coord: Coord,
    pub fmt: Fmt,
    pub base: Base,
    /// Binary integers are kept to this many bits (STWS), 1 to 64.
    pub wordsize: u32,
    /// User flags 1–64, then system flags -1 to -64.
    pub flags: [u64; 2],
    pub menu: Menu,
    pub page: usize,
    /// The labels of the menu TMENU shows.
    pub tmenu: Vec<Obj>,
    /// Polled now and then while a program runs; true stops it.
    pub interrupt: Option<Box<dyn FnMut() -> bool>>,
    /// Stops a program for HALT, PROMPT, INPUT and the like.
    pub pause: Option<PauseFn>,
    /// How many programs are stopped by HALT or PROMPT.
    pub halted: usize,
    /// Set by CONT, for whatever is waiting on a halted program.
    pub cont: bool,
    /// What DISP has put on the display's lines, empty when nothing has.
    pub disp: Vec<String>,
    /// FREEZE: the DISP lines stay up after the program ends.
    pub frozen: bool,
    /// PICT, shown while `plot` is set.
    pub plot: Option<Plot>,
    pub pict: Vec<Shape>,
    /// Objects assigned to keys, by code rc.p.
    pub user_keys: Vec<(f64, Obj)>,
    /// Alarms, soonest first.
    pub alarms: Vec<crate::alarms::Alarm>,
    /// Where I/O's transfer directory and print file go, when not beside
    /// the memory file.
    pub io_dir: Option<std::path::PathBuf>,
    /// The arguments of the last command, for LASTARG.
    pub last_args: Vec<Obj>,
    /// The last error, for ERRN and ERRM.
    pub last_err: Option<Error>,
    pub(crate) locals: Vec<Vec<(String, Obj)>>,
    /// Set while →NUM runs, so that undefined names are an error.
    pub(crate) numeric: bool,
    /// The lists DOSUBS is working through, for NSUB and ENDSUB.
    pub(crate) subs: Vec<(usize, usize)>,
    pub(crate) seed: u64,
    depth: usize,
    steps: u64,
}

impl Calc {
    pub fn new() -> Self {
        set_base(Base::Dec);
        let mut c = Calc::blank();
        c.set_angle(Angle::Deg);
        c
    }

    /// A calculator in its starting state, leaving the display's base alone.
    pub fn blank() -> Self {
        Calc {
            stack: vec![],
            vars: vec![],
            path: vec![],
            angle: Angle::Deg,
            coord: Coord::Rect,
            fmt: Fmt::Std,
            base: Base::Dec,
            wordsize: 64,
            flags: [0; 2],
            menu: Menu::Mth,
            page: 0,
            tmenu: vec![],
            interrupt: None,
            pause: None,
            halted: 0,
            cont: false,
            disp: vec![],
            frozen: false,
            plot: None,
            pict: vec![],
            user_keys: vec![],
            alarms: vec![],
            io_dir: None,
            last_args: vec![],
            last_err: None,
            locals: vec![],
            numeric: false,
            subs: vec![],
            seed: 0,
            depth: 0,
            steps: 0,
        }
    }

    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            stack: self.stack.clone(),
            vars: self.vars.clone(),
            path: self.path.clone(),
            angle: self.angle,
            coord: self.coord,
            fmt: self.fmt,
            base: self.base,
            wordsize: self.wordsize,
            flags: self.flags,
        }
    }

    pub fn restore(&mut self, s: Snapshot) {
        (self.stack, self.vars, self.path) = (s.stack, s.vars, s.path);
        (self.fmt, self.wordsize, self.flags) = (s.fmt, s.wordsize, s.flags);
        self.set_base(s.base);
        self.set_angle(s.angle);
        self.set_coord(s.coord);
    }

    // The display shows polar angles in the angle mode, so it is told both.
    pub fn set_angle(&mut self, a: Angle) {
        self.angle = a;
        set_coord(self.coord, self.angle_to_rad(1.0));
    }

    pub fn set_coord(&mut self, c: Coord) {
        self.coord = c;
        set_coord(c, self.angle_to_rad(1.0));
    }

    pub fn set_base(&mut self, b: Base) {
        self.base = b;
        set_base(b);
    }

    pub fn exec_line(&mut self, src: &str) -> Res<()> {
        self.exec_nodes(&parse(src)?)
    }

    pub fn exec_nodes(&mut self, nodes: &[Node]) -> Res<()> {
        for n in nodes {
            self.exec_node(n)?;
        }
        Ok(())
    }

    fn exec_node(&mut self, n: &Node) -> Res<()> {
        self.steps += 1;
        if self.steps.is_multiple_of(4096) && self.interrupt.as_mut().is_some_and(|f| f()) {
            return Err(Error::Interrupted);
        }
        match n {
            Node::Push(o) => {
                self.stack.push(o.clone());
                Ok(())
            }
            Node::Word(w) => self.eval_word(w),
            Node::If { cond, then, els } => {
                self.exec_nodes(cond)?;
                if self.pop_truth("IF")? {
                    self.exec_nodes(then)
                } else {
                    els.as_ref().map_or(Ok(()), |e| self.exec_nodes(e))
                }
            }
            Node::Start { body, step } => {
                let (start, end) = self.pop_bounds("START")?;
                self.count(start, end, *step, None, body)
            }
            Node::For { var, body, step } => {
                let (start, end) = self.pop_bounds("FOR")?;
                self.locals.push(vec![(var.clone(), Obj::Real(start))]);
                let r = self.count(start, end, *step, Some(var), body);
                self.locals.pop();
                r
            }
            Node::While { cond, body } => loop {
                self.exec_nodes(cond)?;
                if !self.pop_truth("WHILE")? {
                    return Ok(());
                }
                self.exec_nodes(body)?;
            },
            Node::Do { body, cond } => loop {
                self.exec_nodes(body)?;
                self.exec_nodes(cond)?;
                if self.pop_truth("UNTIL")? {
                    return Ok(());
                }
            },
            Node::Local { names, body } => self.with_locals(names, |c| c.exec_nodes(body)),
            Node::LocalAlg { names, expr } => self.with_locals(names, |c| c.eval_obj(expr.clone())),
            Node::Case { clauses, default } => {
                for (cond, body) in clauses {
                    self.exec_nodes(cond)?;
                    if self.pop_truth("CASE")? {
                        return self.exec_nodes(body);
                    }
                }
                self.exec_nodes(default)
            }
            Node::IfErr { body, then, els } => match self.exec_nodes(body) {
                Err(e @ (Error::Interrupted | Error::Killed)) => Err(e),
                Err(e) => {
                    self.last_err = Some(e);
                    self.exec_nodes(then)
                }
                Ok(()) => els.as_ref().map_or(Ok(()), |e| self.exec_nodes(e)),
            },
        }
    }

    /// Runs `f` with `names` bound to objects taken from the stack.
    pub(crate) fn with_locals(
        &mut self,
        names: &[String],
        f: impl FnOnce(&mut Self) -> Res<()>,
    ) -> Res<()> {
        if self.stack.len() < names.len() {
            return Err(at("→", Error::TooFewArgs));
        }
        let vals = self.stack.split_off(self.stack.len() - names.len());
        self.locals.push(names.iter().cloned().zip(vals).collect());
        let r = f(self);
        self.locals.pop();
        r
    }

    // START/FOR loops run the body at least once, then count to `end`;
    // with STEP the increment is taken from the stack each time round.
    fn count(
        &mut self,
        start: f64,
        end: f64,
        step: bool,
        var: Option<&String>,
        body: &[Node],
    ) -> Res<()> {
        let mut i = start;
        loop {
            self.exec_nodes(body)?;
            let inc = if step { self.pop_real("STEP")? } else { 1.0 };
            if let Some(Obj::Real(x)) = var.and_then(|v| self.local(v)) {
                i = *x;
            }
            i = r12(i + inc);
            if let Some(v) = var {
                self.set_local(v, Obj::Real(i));
            }
            if (inc >= 0.0 && i > end) || (inc < 0.0 && i < end) {
                return Ok(());
            }
        }
    }

    fn local(&self, name: &str) -> Option<&Obj> {
        self.locals
            .iter()
            .rev()
            .flat_map(|f| f.iter())
            .find(|(n, _)| n == name)
            .map(|(_, v)| v)
    }

    fn set_local(&mut self, name: &str, v: Obj) -> bool {
        let slot = self
            .locals
            .iter_mut()
            .rev()
            .flat_map(|f| f.iter_mut())
            .find(|(n, _)| n == name);
        slot.map(|(_, old)| *old = v).is_some()
    }

    /// The variables of the directory `depth` levels below HOME on the path.
    pub fn dir_vars(&self, depth: usize) -> &Vec<(String, Obj)> {
        let mut vars = &self.vars;
        for name in &self.path[..depth] {
            match vars.iter().find(|(n, _)| n == name) {
                Some((_, Obj::Dir(v))) => vars = v,
                _ => unreachable!("the path names directories"),
            }
        }
        vars
    }

    pub fn cur_vars(&self) -> &Vec<(String, Obj)> {
        self.dir_vars(self.path.len())
    }

    pub fn cur_vars_mut(&mut self) -> &mut Vec<(String, Obj)> {
        let mut vars = &mut self.vars;
        for name in &self.path {
            match vars.iter_mut().find(|(n, _)| n == name) {
                Some((_, Obj::Dir(v))) => vars = v,
                _ => unreachable!("the path names directories"),
            }
        }
        vars
    }

    /// A global variable, looked for in the current directory and then the
    /// ones above it, with the depth of the directory it was found in.
    pub(crate) fn find_global(&self, name: &str) -> Option<(usize, &Obj)> {
        (0..=self.path.len()).rev().find_map(|d| {
            self.dir_vars(d)
                .iter()
                .find(|(n, _)| n == name)
                .map(|(_, v)| (d, v))
        })
    }

    pub(crate) fn global(&self, name: &str) -> Option<&Obj> {
        self.find_global(name).map(|(_, v)| v)
    }

    /// Stores into the current directory, as STO does.
    pub(crate) fn store(&mut self, name: String, v: Obj) -> Res<()> {
        let vars = self.cur_vars_mut();
        match vars.iter_mut().find(|(n, _)| *n == name) {
            Some((_, Obj::Dir(_))) => return Err(Error::DirNotAllowed),
            Some((_, old)) => *old = v,
            None => vars.insert(0, (name, v)),
        }
        Ok(())
    }

    // A global variable runs its value, except a directory, which becomes the
    // current one.
    fn run_global(&mut self, name: &str) -> Option<Res<()>> {
        let (depth, v) = self.find_global(name)?;
        if let Obj::Dir(_) = v {
            self.path.truncate(depth);
            self.path.push(name.into());
            return Some(Ok(()));
        }
        // Programs, names and algebraics run; anything else, lists included,
        // is put on the stack as it is.
        let v = v.clone();
        if matches!(v, Obj::Prog(_) | Obj::Name(_) | Obj::Alg(_)) {
            Some(self.nested(v))
        } else {
            self.stack.push(v);
            Some(Ok(()))
        }
    }

    // A word runs as a local variable, a built-in command or a global variable,
    // in that order. Commands also answer to lowercase ("sin") when no variable
    // has that name. A word that is none of these is pushed as a name.
    pub(crate) fn eval_word(&mut self, w: &str) -> Res<()> {
        if let Some(v) = self.local(w) {
            self.stack.push(v.clone());
            return Ok(());
        }
        if let Some(r) = self.builtin(w) {
            return r.map_err(|e| at(w, e));
        }
        if let Some(r) = self.run_global(w) {
            return r;
        }
        let up = w.to_uppercase();
        if up != w
            && let Some(r) = self.builtin(&up)
        {
            return r.map_err(|e| at(&up, e));
        }
        self.undefined(w)
    }

    /// An undefined name evaluates to itself, except under →NUM.
    fn undefined(&mut self, w: &str) -> Res<()> {
        if self.numeric {
            return Err(Error::Undefined);
        }
        self.stack.push(Obj::Name(w.into()));
        Ok(())
    }

    fn nested(&mut self, o: Obj) -> Res<()> {
        if self.depth >= MAX_DEPTH {
            return Err(Error::Memory);
        }
        self.depth += 1;
        let r = self.eval_obj(o);
        self.depth -= 1;
        r
    }

    pub(crate) fn eval_obj(&mut self, o: Obj) -> Res<()> {
        match o {
            Obj::Prog(nodes) => self.exec_nodes(&nodes),
            Obj::Name(n) => {
                if let Some(v) = self.local(&n) {
                    self.stack.push(v.clone());
                    Ok(())
                } else if let Some(r) = self.run_global(&n) {
                    r
                } else if let Some(r) = self.numeric.then(|| constant(&n)).flatten() {
                    self.stack.push(r);
                    Ok(())
                } else {
                    self.undefined(&n)
                }
            }
            Obj::Alg(e) => {
                let mut nodes = vec![];
                alg::to_nodes(&e, &mut nodes);
                self.exec_nodes(&nodes)
            }
            Obj::Tag(_, o) => self.eval_obj(*o),
            Obj::List(items) => {
                for it in items {
                    match it {
                        Obj::Name(n) => self.eval_word(&n)?,
                        o => self.stack.push(o),
                    }
                }
                Ok(())
            }
            o => {
                self.stack.push(o);
                Ok(())
            }
        }
    }

    pub(crate) fn need(&self, n: usize) -> Res<()> {
        if self.stack.len() < n {
            Err(Error::TooFewArgs)
        } else {
            Ok(())
        }
    }

    pub(crate) fn peek(&self, level: usize) -> &Obj {
        &self.stack[self.stack.len() - level]
    }

    pub(crate) fn pop(&mut self) -> Res<Obj> {
        self.stack.pop().ok_or(Error::TooFewArgs)
    }

    pub(crate) fn pop_real(&mut self, cmd: &str) -> Res<f64> {
        self.need(1).map_err(|e| at(cmd, e))?;
        match self.peek(1) {
            Obj::Real(x) => {
                let x = *x;
                self.stack.pop();
                Ok(x)
            }
            _ => Err(at(cmd, Error::BadType)),
        }
    }

    fn pop_truth(&mut self, cmd: &str) -> Res<bool> {
        Ok(self.pop_real(cmd)? != 0.0)
    }

    fn pop_bounds(&mut self, cmd: &str) -> Res<(f64, f64)> {
        self.need(2).map_err(|e| at(cmd, e))?;
        match (self.peek(2), self.peek(1)) {
            (Obj::Real(a), Obj::Real(b)) => {
                let r = (*a, *b);
                self.stack.truncate(self.stack.len() - 2);
                Ok(r)
            }
            _ => Err(at(cmd, Error::BadType)),
        }
    }

    /// Takes a count n from level 1 for a command that then needs n more
    /// objects, checking both before anything is removed.
    pub(crate) fn pop_count(&mut self, extra: usize) -> Res<usize> {
        self.need(1)?;
        let Obj::Real(x) = *self.peek(1) else {
            return Err(Error::BadType);
        };
        if x < 0.0 || x.fract() != 0.0 {
            return Err(Error::BadValue);
        }
        let n = x as usize;
        self.need(n + extra + 1)?;
        self.stack.pop();
        Ok(n)
    }

    /// Runs `w` if it is a built-in command.
    fn builtin(&mut self, w: &str) -> Option<Res<()>> {
        if let Some((n, f)) = pure_any(w) {
            return Some(self.apply(w, n, f));
        }
        if let Some(r) = self
            .symbolic_cmd(w)
            .or_else(|| self.lists_cmd(w))
            .or_else(|| self.stat_cmd(w))
            .or_else(|| self.more_cmd(w))
            .or_else(|| self.pause_cmd(w))
            .or_else(|| self.solve_cmd(w))
            .or_else(|| self.plot_cmd(w))
            .or_else(|| self.keys_cmd(w))
            .or_else(|| self.alarm_cmd(w))
            .or_else(|| self.io_cmd(w))
            .or_else(|| self.system_cmd(w))
            .or_else(|| self.pattern_cmd(w))
        {
            return Some(r);
        }
        Some(match w {
            "EVAL" => self.pop().and_then(|o| self.eval_obj(o)),
            "→NUM" => self.pop().and_then(|o| {
                let was = std::mem::replace(&mut self.numeric, true);
                let r = self.eval_obj(o);
                self.numeric = was;
                r
            }),
            "IFT" => self.need(2).and_then(|_| {
                let Obj::Real(c) = *self.peek(2) else {
                    return Err(Error::BadType);
                };
                let o = self.pop()?;
                self.stack.pop();
                if c != 0.0 { self.eval_obj(o) } else { Ok(()) }
            }),
            "IFTE" => self.need(3).and_then(|_| {
                let Obj::Real(c) = *self.peek(3) else {
                    return Err(Error::BadType);
                };
                let (f, t) = (self.pop()?, self.pop()?);
                self.stack.pop();
                self.eval_obj(if c != 0.0 { t } else { f })
            }),
            "STR→" => self.str_eval(),
            "OBJ→" => self.need(1).and_then(|_| match self.peek(1).clone() {
                Obj::List(items) => {
                    self.stack.pop();
                    let n = items.len();
                    self.stack.extend(items);
                    self.stack.push(Obj::Real(n as f64));
                    Ok(())
                }
                Obj::Complex(re, im) => {
                    self.stack.pop();
                    self.stack.extend([Obj::Real(re), Obj::Real(im)]);
                    Ok(())
                }
                Obj::Str(_) => self.str_eval(),
                Obj::Tag(t, o) => {
                    self.stack.pop();
                    self.stack.extend([*o, Obj::Str(t)]);
                    Ok(())
                }
                Obj::Vect(_) | Obj::Mat(_) => self.eval_word("ARRY→"),
                Obj::Unit(x, u) => {
                    self.stack.pop();
                    self.stack.extend([Obj::Real(x), Obj::Unit(1.0, u)]);
                    Ok(())
                }
                Obj::Alg(alg::Expr::Op('=', a, b)) => {
                    self.stack.pop();
                    self.stack.extend([alg::to_obj(*a), alg::to_obj(*b)]);
                    Ok(())
                }
                _ => Err(Error::BadType),
            }),
            "LIST→" => self.need(1).and_then(|_| {
                let Obj::List(items) = self.peek(1).clone() else {
                    return Err(Error::BadType);
                };
                self.stack.pop();
                let n = items.len();
                self.stack.extend(items);
                self.stack.push(Obj::Real(n as f64));
                Ok(())
            }),
            "→LIST" => self.pop_count(0).map(|n| {
                let items = self.stack.split_off(self.stack.len() - n);
                self.stack.push(Obj::List(items));
            }),
            "DUPN" => self.pop_count(0).map(|n| {
                let top = self.stack[self.stack.len() - n..].to_vec();
                self.stack.extend(top);
            }),
            "DROPN" => self
                .pop_count(0)
                .map(|n| self.stack.truncate(self.stack.len() - n)),
            "ROLL" => self.pop_count(0).map(|n| {
                if n > 0 {
                    let o = self.stack.remove(self.stack.len() - n);
                    self.stack.push(o);
                }
            }),
            "ROLLD" => self.pop_count(0).map(|n| {
                if n > 0 {
                    let o = self.stack.pop().unwrap();
                    self.stack.insert(self.stack.len() + 1 - n, o);
                }
            }),
            "PICK" => self.need(1).and_then(|_| {
                if *self.peek(1) == Obj::Real(0.0) {
                    return Err(Error::BadValue);
                }
                let n = self.pop_count(0)?;
                self.stack.push(self.peek(n).clone());
                Ok(())
            }),
            "DEPTH" => {
                self.stack.push(Obj::Real(self.stack.len() as f64));
                Ok(())
            }
            "CLEAR" => {
                self.stack.clear();
                Ok(())
            }
            "STO" => self.need(2).and_then(|_| {
                let Obj::Name(name) = self.peek(1).clone() else {
                    return Err(Error::BadType);
                };
                let v = self.peek(2).clone();
                if !self.set_local(&name, v.clone()) {
                    self.store(name, v)?;
                }
                self.stack.truncate(self.stack.len() - 2);
                Ok(())
            }),
            "RCL" => self.need(1).and_then(|_| {
                let Obj::Name(n) = self.peek(1) else {
                    return Err(Error::BadType);
                };
                let v = self
                    .local(n)
                    .or_else(|| self.global(n))
                    .ok_or(Error::Undefined)?
                    .clone();
                self.stack.pop();
                self.stack.push(v);
                Ok(())
            }),
            "PURGE" => self.need(1).and_then(|_| {
                let names = match self.peek(1) {
                    Obj::Name(n) => vec![n.clone()],
                    Obj::List(l) if l.iter().all(|o| matches!(o, Obj::Name(_))) => l
                        .iter()
                        .map(|o| {
                            if let Obj::Name(n) = o {
                                n.clone()
                            } else {
                                unreachable!()
                            }
                        })
                        .collect(),
                    _ => return Err(Error::BadType),
                };
                let vars = self.cur_vars();
                if vars
                    .iter()
                    .any(|(n, v)| names.contains(n) && matches!(v, Obj::Dir(d) if !d.is_empty()))
                {
                    return Err(Error::NonEmptyDir);
                }
                self.stack.pop();
                self.cur_vars_mut().retain(|(n, _)| !names.contains(n));
                Ok(())
            }),
            "VARS" => {
                let names = self
                    .cur_vars()
                    .iter()
                    .map(|(n, _)| Obj::Name(n.clone()))
                    .collect();
                self.stack.push(Obj::List(names));
                Ok(())
            }
            "STD" => {
                self.fmt = Fmt::Std;
                Ok(())
            }
            "FIX" | "SCI" | "ENG" => self.need(1).and_then(|_| {
                let Obj::Real(x) = *self.peek(1) else {
                    return Err(Error::BadType);
                };
                let n = x.clamp(0.0, 11.0).trunc() as u8;
                self.stack.pop();
                self.fmt = match w {
                    "FIX" => Fmt::Fix(n),
                    "SCI" => Fmt::Sci(n),
                    _ => Fmt::Eng(n),
                };
                Ok(())
            }),
            "DEG" | "RAD" | "GRAD" => {
                self.set_angle(match w {
                    "DEG" => Angle::Deg,
                    "RAD" => Angle::Rad,
                    _ => Angle::Grad,
                });
                Ok(())
            }
            "RECT" | "CYLIN" | "SPHERE" => {
                self.set_coord(match w {
                    "RECT" => Coord::Rect,
                    "CYLIN" => Coord::Cylin,
                    _ => Coord::Sphere,
                });
                Ok(())
            }
            _ => {
                (self.menu, self.page) = (Menu::named(w)?, 0);
                Ok(())
            }
        })
    }

    fn str_eval(&mut self) -> Res<()> {
        self.need(1)?;
        let Obj::Str(s) = self.peek(1) else {
            return Err(Error::BadType);
        };
        let nodes = parse(s)?;
        self.stack.pop();
        self.exec_nodes(&nodes)
    }

    // A command with a fixed number of arguments: they are taken off the stack,
    // and put back if the command fails, as the calculator does.
    fn apply(&mut self, w: &str, n: usize, f: OpFn) -> Res<()> {
        self.need(n)?;
        let args = self.stack.split_off(self.stack.len() - n);
        match self.compute(w, f, &args) {
            Ok(out) => {
                self.stack.extend(out);
                if n > 0 {
                    self.last_args = args;
                }
                Ok(())
            }
            Err(e) => {
                self.stack.extend(args);
                Err(e)
            }
        }
    }

    /// What command `w` gives for `args`. Tags are dropped from arguments,
    /// names and algebraics give an expression, and math applied to lists
    /// applies to each element, as on the HP 48G.
    pub(crate) fn compute(&self, w: &str, f: OpFn, args: &[Obj]) -> Res<Vec<Obj>> {
        if !KEEP_TAGS.contains(&w) && args.iter().any(|a| matches!(a, Obj::Tag(..))) {
            let bare: Vec<Obj> = args.iter().map(untag).collect();
            return self.compute(w, f, &bare);
        }
        if SYMBOLIC.contains(&w)
            && args.iter().any(is_symbolic)
            && args
                .iter()
                .all(|a| matches!(a, Obj::Real(_)) || is_symbolic(a))
        {
            return Ok(vec![Obj::Alg(alg::build(w, args)?)]);
        }
        // Lists are compared whole, and + joins them.
        if !["+", "==", "≠"].contains(&w)
            && (SYMBOLIC.contains(&w) || lists::MAPPED.contains(&w))
            && args.iter().any(|a| matches!(a, Obj::List(_)))
        {
            return lists::map(self, w, f, args);
        }
        if let Some(r) = units::over(self, w, f, args)
            .or_else(|| binary::over(self, w, args))
            .or_else(|| carray::over(w, args))
            .or_else(|| array::over(w, args))
        {
            return r;
        }
        f(self, args)
    }

    pub(crate) fn str_of(&self, o: &Obj) -> String {
        match o {
            Obj::Str(s) => s.clone(),
            o => o.show(self.fmt),
        }
    }

    pub(crate) fn angle_to_rad(&self, x: f64) -> f64 {
        match self.angle {
            Angle::Deg => x.to_radians(),
            Angle::Rad => x,
            Angle::Grad => x * PI / 200.0,
        }
    }

    pub(crate) fn rad_to_angle(&self, x: f64) -> f64 {
        match self.angle {
            Angle::Deg => x.to_degrees(),
            Angle::Rad => x,
            Angle::Grad => x * 200.0 / PI,
        }
    }

    // In degrees and grads, whole quarter turns give exact results, so that
    // 180 SIN is 0 and 90 TAN is an infinite result rather than 1.6E16.
    fn quarter(&self, x: f64) -> Option<i64> {
        let turn = match self.angle {
            Angle::Deg => 360.0,
            Angle::Grad => 400.0,
            Angle::Rad => return None,
        };
        let q = x.rem_euclid(turn) / (turn / 4.0);
        (q.fract() == 0.0).then_some(q as i64)
    }

    fn sin(&self, x: f64) -> f64 {
        match self.quarter(x) {
            Some(q) => [0.0, 1.0, 0.0, -1.0][q as usize % 4],
            None => self.angle_to_rad(x).sin(),
        }
    }

    fn cos(&self, x: f64) -> f64 {
        match self.quarter(x) {
            Some(q) => [1.0, 0.0, -1.0, 0.0][q as usize % 4],
            None => self.angle_to_rad(x).cos(),
        }
    }

    fn tan(&self, x: f64) -> Res<Obj> {
        match self.quarter(x) {
            Some(q) if q % 2 == 1 => Err(Error::Infinite),
            Some(_) => real(0.0),
            None => real(self.angle_to_rad(x).tan()),
        }
    }

    fn inv_trig(&self, x: f64, f: fn(f64) -> f64, bounded: bool) -> Res<Obj> {
        if bounded && x.abs() > 1.0 {
            return Err(Error::BadValue);
        }
        real(self.rad_to_angle(f(x)))
    }
}

pub type OpFn = fn(&Calc, &[Obj]) -> Res<Vec<Obj>>;

pub fn op(n: usize, f: OpFn) -> Option<(usize, OpFn)> {
    Some((n, f))
}

/// Commands that see tagged objects as they are, rather than their contents.
const KEEP_TAGS: &[&str] = &[
    "DUP", "DROP", "SWAP", "OVER", "ROT", "DUP2", "DROP2", "TYPE", "SAME", "==", "≠", "→STR",
    "DTAG", "→TAG",
];

fn untag(o: &Obj) -> Obj {
    match o {
        Obj::Tag(_, o) => untag(o),
        o => o.clone(),
    }
}

/// The constants →NUM gives a value for.
fn constant(n: &str) -> Option<Obj> {
    match n {
        "π" => Some(Obj::Real(PI)),
        "e" => Some(Obj::Real(E)),
        "i" => Some(Obj::Complex(0.0, 1.0)),
        _ => None,
    }
}

/// Every command that takes a fixed number of arguments and only computes.
pub(crate) fn pure_any(w: &str) -> Option<(usize, OpFn)> {
    pure(w)
        .or_else(|| array::pure(w))
        .or_else(|| binary::pure(w))
        .or_else(|| lists::pure(w))
        .or_else(|| matrix::pure(w))
        .or_else(|| more::pure(w))
        .or_else(|| stat::pure(w))
        .or_else(|| units::pure(w))
}

/// Commands that take a fixed number of arguments and only compute.
fn pure(w: &str) -> Option<(usize, OpFn)> {
    use Obj::*;
    match w {
        "DUP" => op(1, |_, a| Ok(vec![a[0].clone(), a[0].clone()])),
        "DROP" => op(1, |_, _| Ok(vec![])),
        "SWAP" => op(2, |_, a| Ok(vec![a[1].clone(), a[0].clone()])),
        "OVER" => op(2, |_, a| Ok(vec![a[0].clone(), a[1].clone(), a[0].clone()])),
        "ROT" => op(3, |_, a| Ok(vec![a[1].clone(), a[2].clone(), a[0].clone()])),
        "DUP2" => op(2, |_, a| Ok([a, a].concat())),
        "DROP2" => op(2, |_, _| Ok(vec![])),

        "+" => op(2, |c, a| one(c.add(&a[0], &a[1]))),
        "-" => op(2, |c, a| one(c.num2('-', &a[0], &a[1]))),
        "*" => op(2, |c, a| one(c.num2('*', &a[0], &a[1]))),
        "/" => op(2, |c, a| one(c.num2('/', &a[0], &a[1]))),
        "=" => op(2, |_, a| Ok(vec![Obj::Alg(alg::build("=", a)?)])),
        "^" => op(2, |_, a| {
            one(arith(&a[0], &a[1], real_pow, |x, y| cplx(x.pow(y)?)))
        }),
        // y x XROOT is the real xth root of y, negative for an odd root of a
        // negative number where ^ would give a complex result.
        "XROOT" => op(2, |_, a| {
            let (y, x) = (real_of(&a[0])?, real_of(&a[1])?);
            one(if x == 0.0 {
                Err(Error::BadValue)
            } else if y < 0.0 && x.fract() == 0.0 && x % 2.0 != 0.0 {
                real(-(-y).powf(1.0 / x))
            } else {
                real_pow(y, 1.0 / x)
            })
        }),
        "NEG" => op(1, |_, a| num1(&a[0], |x| real(-x), |z| cplx(C(-z.0, -z.1)))),
        "INV" => op(1, |_, a| {
            num1(
                &a[0],
                |x| {
                    if x == 0.0 {
                        Err(Error::Infinite)
                    } else {
                        real(1.0 / x)
                    }
                },
                |z| cplx(C(1.0, 0.0).div(z)?),
            )
        }),
        "SQ" => op(1, |_, a| num1(&a[0], |x| real(x * x), |z| cplx(z.mul(z)))),
        "√" => op(1, |_, a| {
            num1(
                &a[0],
                |x| {
                    if x < 0.0 {
                        cplx(C(0.0, (-x).sqrt()))
                    } else {
                        real(x.sqrt())
                    }
                },
                |z| cplx(z.sqrt()),
            )
        }),
        "ABS" => op(1, |_, a| num1(&a[0], |x| real(x.abs()), |z| real(z.abs()))),
        "SIGN" => op(1, |_, a| {
            num1(
                &a[0],
                |x| real(if x == 0.0 { 0.0 } else { x.signum() }),
                |z| {
                    let m = z.abs();
                    cplx(if m == 0.0 { z } else { C(z.0 / m, z.1 / m) })
                },
            )
        }),
        "IP" => op(1, |_, a| real1(&a[0], f64::trunc)),
        "FP" => op(1, |_, a| real1(&a[0], f64::fract)),
        "FLOOR" => op(1, |_, a| real1(&a[0], f64::floor)),
        "CEIL" => op(1, |_, a| real1(&a[0], f64::ceil)),
        "MOD" => op(2, |_, a| {
            real2(a, |x, y| if y == 0.0 { x } else { x - y * (x / y).floor() })
        }),
        "MIN" => op(2, |_, a| real2(a, f64::min)),
        "MAX" => op(2, |_, a| real2(a, f64::max)),
        "%" => op(2, |_, a| real2(a, |x, y| x * y / 100.0)),
        "%CH" => op(2, |_, a| real2(a, |x, y| 100.0 * (y - x) / x)),
        "%T" => op(2, |_, a| real2(a, |x, y| 100.0 * y / x)),
        "!" => op(1, |_, a| one(fact(real_of(&a[0])?))),

        "SIN" => op(1, |c, a| one(real(c.sin(real_of(&a[0])?)))),
        "COS" => op(1, |c, a| one(real(c.cos(real_of(&a[0])?)))),
        "TAN" => op(1, |c, a| one(c.tan(real_of(&a[0])?))),
        "ASIN" => op(1, |c, a| one(c.inv_trig(real_of(&a[0])?, f64::asin, true))),
        "ACOS" => op(1, |c, a| one(c.inv_trig(real_of(&a[0])?, f64::acos, true))),
        "ATAN" => op(1, |c, a| one(c.inv_trig(real_of(&a[0])?, f64::atan, false))),
        "SINH" => op(1, |_, a| real1(&a[0], f64::sinh)),
        "COSH" => op(1, |_, a| real1(&a[0], f64::cosh)),
        "TANH" => op(1, |_, a| real1(&a[0], f64::tanh)),
        "ASINH" => op(1, |_, a| real1(&a[0], f64::asinh)),
        "ACOSH" => op(1, |_, a| {
            one(domain(real_of(&a[0])?, |x| x >= 1.0, f64::acosh))
        }),
        "ATANH" => op(1, |_, a| {
            let x = real_of(&a[0])?;
            one(if x.abs() == 1.0 {
                Err(Error::Infinite)
            } else {
                domain(x, |x| x.abs() < 1.0, f64::atanh)
            })
        }),

        "LN" => op(1, |_, a| num1(&a[0], |x| log(x, 1.0), |z| cplx(z.ln()?))),
        "LOG" => op(1, |_, a| {
            num1(
                &a[0],
                |x| log(x, 10f64.ln()),
                |z| cplx(z.ln()?.scale(1.0 / 10f64.ln())),
            )
        }),
        "EXP" => op(1, |_, a| num1(&a[0], |x| real(x.exp()), |z| cplx(z.exp()))),
        "ALOG" => op(1, |_, a| real1(&a[0], |x| 10f64.powf(x))),
        "LNP1" => op(1, |_, a| {
            let x = real_of(&a[0])?;
            one(if x == -1.0 {
                Err(Error::Infinite)
            } else {
                domain(x, |x| x > -1.0, f64::ln_1p)
            })
        }),
        "EXPM" => op(1, |_, a| real1(&a[0], f64::exp_m1)),
        "π" => op(0, |_, _| one(real(PI))),
        "e" => op(0, |_, _| one(real(E))),
        "i" => op(0, |_, _| Ok(vec![Complex(0.0, 1.0)])),

        "R→C" => op(2, |_, a| match (&a[0], &a[1]) {
            (Real(re), Real(im)) => Ok(vec![Complex(*re, *im)]),
            _ => Err(Error::BadType),
        }),
        "C→R" => op(1, |_, a| match a[0] {
            Complex(re, im) => Ok(vec![Real(re), Real(im)]),
            _ => Err(Error::BadType),
        }),
        "RE" => op(1, |_, a| num1(&a[0], real, |z| real(z.0))),
        "IM" => op(1, |_, a| num1(&a[0], |_| real(0.0), |z| real(z.1))),
        "CONJ" => op(1, |_, a| num1(&a[0], real, |z| cplx(C(z.0, -z.1)))),
        "ARG" => op(1, |c, a| {
            one(real(
                c.rad_to_angle(as_num(&a[0]).ok_or(Error::BadType)?.arg()),
            ))
        }),
        "D→R" => op(1, |_, a| real1(&a[0], f64::to_radians)),
        "R→D" => op(1, |_, a| real1(&a[0], f64::to_degrees)),

        "==" => op(2, |_, a| Ok(vec![truth(equal(&a[0], &a[1]))])),
        "≠" => op(2, |_, a| Ok(vec![truth(!equal(&a[0], &a[1]))])),
        "SAME" => op(2, |_, a| Ok(vec![truth(a[0] == a[1])])),
        "<" => op(2, |_, a| Ok(vec![truth(compare(a)? == Ordering::Less)])),
        ">" => op(2, |_, a| Ok(vec![truth(compare(a)? == Ordering::Greater)])),
        "≤" => op(2, |_, a| Ok(vec![truth(compare(a)? != Ordering::Greater)])),
        "≥" => op(2, |_, a| Ok(vec![truth(compare(a)? != Ordering::Less)])),
        "AND" => op(2, |_, a| logic(a, |x, y| x && y)),
        "OR" => op(2, |_, a| logic(a, |x, y| x || y)),
        "XOR" => op(2, |_, a| logic(a, |x, y| x != y)),
        "NOT" => op(1, |_, a| Ok(vec![truth(real_of(&a[0])? == 0.0)])),

        "TYPE" => op(1, |_, a| {
            let t = match a[0] {
                Real(_) => 0.0,
                Complex(..) => 1.0,
                Str(_) => 2.0,
                List(_) => 5.0,
                Name(_) => 6.0,
                Prog(_) => 8.0,
                Vect(_) | Mat(_) => 3.0,
                CVect(_) | CMat(_) => 4.0,
                Alg(_) => 9.0,
                Bin(_) => 10.0,
                Tag(..) => 12.0,
                Unit(..) => 13.0,
                Dir(_) => 15.0,
            };
            Ok(vec![Real(t)])
        }),
        "→STR" => op(1, |c, a| Ok(vec![Str(c.str_of(&a[0]))])),
        "SIZE" => op(1, |_, a| match &a[0] {
            Str(s) => Ok(vec![Real(s.chars().count() as f64)]),
            List(l) => Ok(vec![Real(l.len() as f64)]),
            Vect(v) => Ok(vec![List(vec![Real(v.len() as f64)])]),
            Mat(m) => Ok(vec![List(vec![
                Real(m.len() as f64),
                Real(m[0].len() as f64),
            ])]),
            Alg(_) | Name(_) => Ok(vec![Real(1.0)]),
            _ => Err(Error::BadType),
        }),
        "HEAD" => op(1, |_, a| match &a[0] {
            List(l) => l.first().cloned().map(|o| vec![o]).ok_or(Error::BadValue),
            Str(s) => s
                .chars()
                .next()
                .map(|c| vec![Str(c.into())])
                .ok_or(Error::BadValue),
            _ => Err(Error::BadType),
        }),
        "TAIL" => op(1, |_, a| match &a[0] {
            List(l) if !l.is_empty() => Ok(vec![List(l[1..].to_vec())]),
            Str(s) if !s.is_empty() => Ok(vec![Str(s.chars().skip(1).collect())]),
            List(_) | Str(_) => Err(Error::BadValue),
            _ => Err(Error::BadType),
        }),
        "GET" => op(2, |_, a| match (&a[0], &a[1]) {
            (List(l), Real(i)) => Ok(vec![l[index(*i, l.len())?].clone()]),
            (List(l), List(i)) if i.len() == 1 => {
                Ok(vec![l[index(real_of(&i[0])?, l.len())?].clone()])
            }
            (Vect(_) | Mat(_), _) => one(array::get(&a[0], &a[1])),
            _ => Err(Error::BadType),
        }),
        "PUT" => op(3, |_, a| match (&a[0], &a[1]) {
            (List(l), Real(i)) => {
                let mut l = l.clone();
                let i = index(*i, l.len())?;
                l[i] = a[2].clone();
                Ok(vec![List(l)])
            }
            (Vect(_) | Mat(_), _) => one(array::put(&a[0], &a[1], &a[2])),
            _ => Err(Error::BadType),
        }),
        "SUB" => op(3, |_, a| {
            let (Real(s), Real(e)) = (&a[1], &a[2]) else {
                return Err(Error::BadType);
            };
            let span = |len: usize| {
                let s = (s.max(1.0) as usize).saturating_sub(1);
                let e = (e.max(0.0) as usize).min(len);
                s..e.max(s)
            };
            match &a[0] {
                List(l) => Ok(vec![List(l[span(l.len())].to_vec())]),
                Str(st) => {
                    let cs: Vec<char> = st.chars().collect();
                    Ok(vec![Str(cs[span(cs.len())].iter().collect())])
                }
                _ => Err(Error::BadType),
            }
        }),
        _ => None,
    }
}

impl Calc {
    // + also joins strings and lists: "A" 1 + is "A1", { 1 } 2 + is { 1 2 }.
    fn add(&self, a: &Obj, b: &Obj) -> Res<Obj> {
        use Obj::*;
        Ok(match (a, b) {
            (List(x), List(y)) => List([x.as_slice(), y.as_slice()].concat()),
            (List(x), o) => List([x.as_slice(), std::slice::from_ref(o)].concat()),
            (o, List(y)) => List([std::slice::from_ref(o), y.as_slice()].concat()),
            (Str(s), o) => Str(s.clone() + &self.str_of(o)),
            (o, Str(s)) => Str(self.str_of(o) + s),
            _ => return self.num2('+', a, b),
        })
    }

    /// + - * / on numbers, binary integers and arrays.
    pub(crate) fn num2(&self, o: char, a: &Obj, b: &Obj) -> Res<Obj> {
        use Obj::*;
        match (a, b) {
            (Bin(_), _) | (_, Bin(_)) => binary::arith(self, o, a, b),
            (Vect(_) | Mat(_), _) | (_, Vect(_) | Mat(_)) => array::arith(o, a, b),
            _ => match o {
                '+' => arith(a, b, |x, y| real(x + y), |x, y| cplx(x.add(y))),
                '-' => arith(a, b, |x, y| real(x - y), |x, y| cplx(x.sub(y))),
                '*' => arith(a, b, |x, y| real(x * y), |x, y| cplx(x.mul(y))),
                _ => arith(
                    a,
                    b,
                    |x, y| if y == 0.0 { Err(div0(x)) } else { real(x / y) },
                    |x, y| cplx(x.div(y)?),
                ),
            },
        }
    }
}

/// A computed real, checked and rounded to the calculator's 12 digits.
pub(crate) fn real(x: f64) -> Res<Obj> {
    if x.is_nan() {
        Err(Error::UndefinedResult)
    } else if x.is_infinite() {
        Err(Error::Infinite)
    } else {
        Ok(Obj::Real(if x == 0.0 { 0.0 } else { r12(x) }))
    }
}

fn cplx(z: C) -> Res<Obj> {
    let (Obj::Real(re), Obj::Real(im)) = (real(z.0)?, real(z.1)?) else {
        unreachable!()
    };
    Ok(Obj::Complex(re, im))
}

pub(crate) fn one(o: Res<Obj>) -> Res<Vec<Obj>> {
    o.map(|o| vec![o])
}

pub(crate) fn truth(b: bool) -> Obj {
    Obj::Real(if b { 1.0 } else { 0.0 })
}

pub(crate) fn div0(x: f64) -> Error {
    if x == 0.0 {
        Error::UndefinedResult
    } else {
        Error::Infinite
    }
}

pub(crate) fn real_of(o: &Obj) -> Res<f64> {
    match o {
        Obj::Real(x) => Ok(*x),
        _ => Err(Error::BadType),
    }
}

fn as_num(o: &Obj) -> Option<C> {
    match *o {
        Obj::Real(x) => Some(C(x, 0.0)),
        Obj::Complex(re, im) => Some(C(re, im)),
        _ => None,
    }
}

pub(crate) fn real1(a: &Obj, f: fn(f64) -> f64) -> Res<Vec<Obj>> {
    one(real(f(real_of(a)?)))
}

pub(crate) fn real2(a: &[Obj], f: fn(f64, f64) -> f64) -> Res<Vec<Obj>> {
    one(real(f(real_of(&a[0])?, real_of(&a[1])?)))
}

fn num1(a: &Obj, rf: fn(f64) -> Res<Obj>, cf: fn(C) -> Res<Obj>) -> Res<Vec<Obj>> {
    one(match *a {
        Obj::Real(x) => rf(x),
        Obj::Complex(re, im) => cf(C(re, im)),
        _ => Err(Error::BadType),
    })
}

fn arith(a: &Obj, b: &Obj, rf: fn(f64, f64) -> Res<Obj>, cf: fn(C, C) -> Res<Obj>) -> Res<Obj> {
    match (a, b) {
        (Obj::Real(x), Obj::Real(y)) => rf(*x, *y),
        _ => match (as_num(a), as_num(b)) {
            (Some(x), Some(y)) => cf(x, y),
            _ => Err(Error::BadType),
        },
    }
}

fn domain(x: f64, ok: fn(f64) -> bool, f: fn(f64) -> f64) -> Res<Obj> {
    if ok(x) {
        real(f(x))
    } else {
        Err(Error::BadValue)
    }
}

// A negative number has a complex logarithm and fractional powers, which the
// calculator returns rather than an error.
fn log(x: f64, base_ln: f64) -> Res<Obj> {
    if x == 0.0 {
        Err(Error::Infinite)
    } else if x < 0.0 {
        cplx(C(x, 0.0).ln()?.scale(1.0 / base_ln))
    } else {
        real(x.ln() / base_ln)
    }
}

fn real_pow(x: f64, y: f64) -> Res<Obj> {
    if x < 0.0 && y.fract() != 0.0 {
        cplx(C(x, 0.0).pow(C(y, 0.0))?)
    } else if x == 0.0 && y < 0.0 {
        Err(Error::Infinite)
    } else {
        real(x.powf(y))
    }
}

pub(crate) fn fact(x: f64) -> Res<Obj> {
    if x < 0.0 && x.fract() == 0.0 {
        Err(Error::Infinite)
    } else if x.fract() == 0.0 && x <= 170.0 {
        real((1..=x as u64).map(|i| i as f64).product())
    } else {
        real(gamma(x + 1.0))
    }
}

// Lanczos approximation (g = 7), for factorials of non-integers.
pub(crate) fn gamma(x: f64) -> f64 {
    if x < 0.5 {
        return PI / ((PI * x).sin() * gamma(1.0 - x));
    }
    const C: [f64; 9] = [
        0.999_999_999_999_809_9,
        676.520_368_121_885_1,
        -1_259.139_216_722_402_8,
        771.323_428_777_653_1,
        -176.615_029_162_140_6,
        12.507_343_278_686_905,
        -0.138_571_095_265_720_12,
        9.984_369_578_019_572e-6,
        1.505_632_735_149_311_6e-7,
    ];
    let x = x - 1.0;
    let t = x + 7.5;
    let a = C[0] + (1..9).map(|i| C[i] / (x + i as f64)).sum::<f64>();
    (2.0 * PI).sqrt() * t.powf(x + 0.5) * (-t).exp() * a
}

fn equal(a: &Obj, b: &Obj) -> bool {
    match (as_num(a), as_num(b)) {
        (Some(x), Some(y)) => x == y,
        _ => a == b,
    }
}

pub(crate) fn compare(a: &[Obj]) -> Res<Ordering> {
    match (&a[0], &a[1]) {
        (Obj::Real(x), Obj::Real(y)) => x.partial_cmp(y).ok_or(Error::BadValue),
        (Obj::Str(x), Obj::Str(y)) => Ok(x.cmp(y)),
        _ => Err(Error::BadType),
    }
}

fn logic(a: &[Obj], f: fn(bool, bool) -> bool) -> Res<Vec<Obj>> {
    Ok(vec![truth(f(
        real_of(&a[0])? != 0.0,
        real_of(&a[1])? != 0.0,
    ))])
}

/// A 1-based list position.
pub(crate) fn index(i: f64, len: usize) -> Res<usize> {
    if i.fract() == 0.0 && i >= 1.0 && i <= len as f64 {
        Ok(i as usize - 1)
    } else {
        Err(Error::BadValue)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct C(f64, f64);

impl C {
    fn add(self, o: C) -> C {
        C(self.0 + o.0, self.1 + o.1)
    }

    fn sub(self, o: C) -> C {
        C(self.0 - o.0, self.1 - o.1)
    }

    fn mul(self, o: C) -> C {
        C(self.0 * o.0 - self.1 * o.1, self.0 * o.1 + self.1 * o.0)
    }

    fn scale(self, k: f64) -> C {
        C(self.0 * k, self.1 * k)
    }

    fn div(self, o: C) -> Res<C> {
        let d = o.0 * o.0 + o.1 * o.1;
        if d == 0.0 {
            return Err(div0(self.abs()));
        }
        Ok(C(
            (self.0 * o.0 + self.1 * o.1) / d,
            (self.1 * o.0 - self.0 * o.1) / d,
        ))
    }

    fn abs(self) -> f64 {
        self.0.hypot(self.1)
    }

    fn arg(self) -> f64 {
        self.1.atan2(self.0)
    }

    fn ln(self) -> Res<C> {
        if self.abs() == 0.0 {
            Err(Error::Infinite)
        } else {
            Ok(C(self.abs().ln(), self.arg()))
        }
    }

    fn exp(self) -> C {
        let m = self.0.exp();
        C(m * self.1.cos(), m * self.1.sin())
    }

    fn sqrt(self) -> C {
        let r = self.abs();
        C(
            ((r + self.0) / 2.0).sqrt(),
            ((r - self.0) / 2.0).sqrt().copysign(self.1),
        )
    }

    fn pow(self, o: C) -> Res<C> {
        if self.abs() == 0.0 {
            return if o.abs() == 0.0 {
                Ok(C(1.0, 0.0))
            } else if o.0 > 0.0 {
                Ok(C(0.0, 0.0))
            } else {
                Err(Error::Infinite)
            };
        }
        Ok(self.ln()?.mul(o).exp())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs `src` on a fresh calculator and returns the stack, level 1 last.
    fn run(src: &str) -> Vec<String> {
        let mut c = Calc::new();
        c.exec_line(src).unwrap();
        c.stack.iter().map(|o| o.show(c.fmt)).collect()
    }

    fn err(src: &str) -> (String, Vec<String>) {
        let mut c = Calc::new();
        let e = c.exec_line(src).unwrap_err();
        (e.report(), c.stack.iter().map(|o| o.show(c.fmt)).collect())
    }

    #[test]
    fn arithmetic() {
        assert_eq!(run("1 2 +"), ["3"]);
        assert_eq!(run("10 4 -  3 *  2 /"), ["9"]);
        assert_eq!(run(".1 .2 + .3 =="), ["1"]);
        assert_eq!(run("2 10 ^"), ["1024"]);
        assert_eq!(run("-4 √"), ["(0,2)"]);
        assert_eq!(run("(1,2) (3,4) *"), ["(-5,10)"]);
        assert_eq!(run("5 !  .5 !"), ["120", ".886226925453"]);
        assert_eq!(run("-7 3 MOD"), ["2"]);
        assert_eq!(run("27 3 XROOT  -8 3 XROOT"), ["3", "-2"]);
        assert_eq!(run("π"), ["3.14159265359"]);
    }

    #[test]
    fn trig_modes() {
        assert_eq!(run("30 SIN  180 SIN  60 COS"), [".5", "0", ".5"]);
        assert_eq!(run("RAD π 2 / SIN  GRAD 200 COS"), ["1", "-1"]);
        assert_eq!(err("90 TAN").0, "TAN Error: Infinite Result");
        assert_eq!(run("1 ATAN 4 *"), ["180"]);
    }

    #[test]
    fn errors_put_arguments_back() {
        assert_eq!(
            err("1 0 /"),
            (
                "/ Error: Infinite Result".into(),
                vec!["1".into(), "0".into()]
            )
        );
        assert_eq!(
            err("1 +"),
            ("+ Error: Too Few Arguments".into(), vec!["1".into()])
        );
        assert_eq!(err("\"a\" SIN").0, "SIN Error: Bad Argument Type");
        assert_eq!(err("'X' RCL").0, "RCL Error: Undefined Name");
    }

    #[test]
    fn stack_commands() {
        assert_eq!(run("1 2 3 ROT"), ["2", "3", "1"]);
        assert_eq!(run("1 2 3 3 ROLL"), ["2", "3", "1"]);
        assert_eq!(run("1 2 3 3 ROLLD"), ["3", "1", "2"]);
        assert_eq!(run("1 2 3 3 PICK"), ["1", "2", "3", "1"]);
        assert_eq!(run("1 2 2 DUPN DEPTH"), ["1", "2", "1", "2", "4"]);
        assert_eq!(run("1 2 3 3 →LIST LIST→"), ["1", "2", "3", "3"]);
        assert_eq!(err("1 5 DROPN").1, ["1", "5"]);
    }

    #[test]
    fn variables_and_programs() {
        assert_eq!(run("« → a b « a b * » » 'MUL' STO 6 7 MUL"), ["42"]);
        assert_eq!(run("5 'X' STO X X *"), ["25"]);
        assert_eq!(run("'Y' Y"), ["'Y'", "'Y'"]);
        assert_eq!(
            run("{ 1 A } 'L' STO L { 1 A } EVAL"),
            ["{ 1 A }", "1", "'A'"]
        );
        assert_eq!(run("0 1 10 FOR i i + NEXT"), ["55"]);
        assert_eq!(run("1 1 10 START 2 * 3 STEP"), ["16"]);
        assert_eq!(run("1 WHILE DUP 100 < REPEAT 2 * END"), ["128"]);
        assert_eq!(run("1 DO 3 * UNTIL DUP 50 > END"), ["81"]);
        assert_eq!(run("IF 1 2 < THEN \"yes\" ELSE \"no\" END"), ["\"yes\""]);
        assert_eq!(
            run("« DUP 1 > « DUP 1 - FAC * » IFT » 'FAC' STO 10 FAC"),
            ["3628800"]
        );
        assert_eq!(run("\"3 4 +\" STR→"), ["7"]);
        assert_eq!(run("1 2 sin"), ["1", ".0348994967025"]);
    }

    #[test]
    fn runaway_recursion_is_an_error() {
        // The interpreter recurses too; give it the stack main gives it.
        let t = std::thread::Builder::new()
            .stack_size(256 << 20)
            .spawn(|| err("« R » 'R' STO R").0)
            .unwrap();
        assert_eq!(t.join().unwrap(), "Error: Insufficient Memory");
    }

    #[test]
    fn strings_and_lists() {
        assert_eq!(run("\"A\" 1 +"), ["\"A1\""]);
        assert_eq!(run("{ 1 2 } 3 +"), ["{ 1 2 3 }"]);
        assert_eq!(run("{ 10 20 30 } 2 GET"), ["20"]);
        assert_eq!(run("{ 10 20 30 } 2 99 PUT"), ["{ 10 99 30 }"]);
        assert_eq!(run("\"HELLO\" 2 4 SUB SIZE"), ["3"]);
        assert_eq!(run("2 FIX π →STR"), ["\"3.14\""]);
    }
}

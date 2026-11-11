//! Command-line and program parsing.

use crate::alg::{Expr, parse_alg};
use crate::calc::{Error, Res};
use crate::obj::{Base, Node, Obj, base, from_polar, r12};
use crate::units::{Units, parse_units};

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Num(f64),
    Cplx(f64, f64),
    Str(String),
    Quoted(String),
    Alg(Expr),
    Bin(u64),
    Unit(f64, Units),
    /// A component of a vector written in polar form, [ 1 ∠90 ].
    Angle(f64),
    Tag(String),
    LBrack,
    RBrack,
    LBrace,
    RBrace,
    LProg,
    RProg,
    Word(String),
}

/// Words that open or close a control structure.
const KEYWORDS: &[&str] = &[
    "IF", "THEN", "ELSE", "END", "START", "FOR", "NEXT", "STEP", "WHILE", "REPEAT", "DO", "UNTIL",
    "CASE", "IFERR", "DIR",
];
const TERMINATORS: &[&str] = &["THEN", "ELSE", "END", "NEXT", "STEP", "REPEAT", "UNTIL"];

fn is_delim(c: char) -> bool {
    c.is_whitespace() || "\"'{}«»([".contains(c)
}

/// Parses a real the way the calculator reads one, rounded to 12 digits.
pub fn parse_num(s: &str) -> Option<f64> {
    let mut cs = s.chars();
    let first = cs.next()?;
    let numeric = |c: char| c.is_ascii_digit() || c == '.';
    if !(numeric(first) || ((first == '-' || first == '+') && cs.next().is_some_and(numeric))) {
        return None;
    }
    let v: f64 = s.parse().ok()?;
    v.is_finite().then(|| r12(v))
}

pub fn valid_name(s: &str) -> bool {
    let Some(first) = s.chars().next() else {
        return false;
    };
    !first.is_ascii_digit()
        && first != '.'
        && !s.chars().any(is_delim)
        && !s.contains([
            '}', '»', ')', ',', '+', '-', '*', '/', '^', '=', '<', '>', '[', ']', '√',
        ])
        && parse_num(s).is_none()
        && !KEYWORDS.contains(&s)
        && s != "→"
}

/// ASCII spellings of the calculator's special characters, so they can be
/// typed: -> for →, SQRT for √, <= for ≤ and so on.
pub fn normalize(w: &str) -> String {
    let w = w.replace("->", "→");
    let alias = match w.as_str() {
        "SQRT" => "√",
        "PI" => "π",
        "<=" => "≤",
        ">=" => "≥",
        "!=" | "<>" => "≠",
        "FACT" => "!",
        _ => "",
    };
    if !alias.is_empty() {
        return alias.into();
    }
    let up = w.to_uppercase();
    if KEYWORDS.contains(&up.as_str()) {
        up
    } else {
        w
    }
}

fn tokenize(src: &str) -> Res<Vec<Tok>> {
    let cs: Vec<char> = src.chars().collect();
    let mut out = vec![];
    let mut i = 0;
    let pair = |i: usize, s: &str| cs[i..].iter().take(2).copied().eq(s.chars());
    while i < cs.len() {
        let c = cs[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        if pair(i, "<<") || pair(i, ">>") {
            out.push(if c == '<' { Tok::LProg } else { Tok::RProg });
            i += 2;
            continue;
        }
        match c {
            '"' => {
                i += 1;
                let mut s = String::new();
                loop {
                    match cs.get(i) {
                        None => return Err(Error::Syntax),
                        Some('"') => break,
                        Some('\\') => {
                            match cs.get(i + 1) {
                                Some('n') => s.push('\n'),
                                Some(&e) => s.push(e),
                                None => return Err(Error::Syntax),
                            }
                            i += 1;
                        }
                        Some(&ch) => s.push(ch),
                    }
                    i += 1;
                }
                i += 1;
                out.push(Tok::Str(s));
            }
            '\'' => {
                let len = cs[i + 1..]
                    .iter()
                    .position(|&c| c == '\'')
                    .ok_or(Error::Syntax)?;
                let text: String = cs[i + 1..i + 1 + len].iter().collect();
                let name = normalize(text.trim());
                out.push(if valid_name(&name) {
                    Tok::Quoted(name)
                } else {
                    match parse_alg(&text)? {
                        Expr::Name(n) => Tok::Quoted(n),
                        e => Tok::Alg(e),
                    }
                });
                i += len + 2;
            }
            '(' => {
                let len = cs[i + 1..]
                    .iter()
                    .position(|&c| c == ')')
                    .ok_or(Error::Syntax)?;
                let inner: String = cs[i + 1..i + 1 + len].iter().collect();
                let parts: Vec<&str> = inner
                    .split(|c: char| c == ',' || c.is_whitespace())
                    .filter(|p| !p.is_empty())
                    .collect();
                // (x,y), or in polar form (r,∠θ).
                let (re, im, polar) = match parts[..] {
                    [re, "∠", im] => (re, im, true),
                    [re, im] => match im.strip_prefix('∠') {
                        Some(im) => (re, im, true),
                        None => (re, im, false),
                    },
                    _ => return Err(Error::Syntax),
                };
                let (re, im) = (
                    parse_num(re).ok_or(Error::Syntax)?,
                    parse_num(im).ok_or(Error::Syntax)?,
                );
                out.push(if polar {
                    let p = from_polar(re, im, None);
                    Tok::Cplx(r12(p[0]), r12(p[1]))
                } else {
                    Tok::Cplx(re, im)
                });
                i += len + 2;
            }
            '{' | '}' | '«' | '»' | '[' | ']' => {
                out.push(match c {
                    '{' => Tok::LBrace,
                    '}' => Tok::RBrace,
                    '«' => Tok::LProg,
                    '[' => Tok::LBrack,
                    ']' => Tok::RBrack,
                    _ => Tok::RProg,
                });
                i += 1;
            }
            // # FFh, #255d, #1010b; without a suffix, in the current base.
            '#' => {
                i += 1;
                while cs.get(i) == Some(&' ') {
                    i += 1;
                }
                let start = i;
                while i < cs.len() && cs[i].is_ascii_alphanumeric() {
                    i += 1;
                }
                let w: String = cs[start..i].iter().collect();
                out.push(Tok::Bin(parse_bin(&w).ok_or(Error::Syntax)?));
            }
            ':' => {
                let len = cs[i + 1..]
                    .iter()
                    .position(|&c| c == ':')
                    .ok_or(Error::Syntax)?;
                let tag: String = cs[i + 1..i + 1 + len].iter().collect();
                if tag.is_empty() || tag.chars().any(char::is_whitespace) {
                    return Err(Error::Syntax);
                }
                out.push(Tok::Tag(tag));
                i += len + 2;
            }
            _ => {
                let start = i;
                // Units after a number may have parentheses: 1_J/(kg*K).
                let mut depth = 0;
                while i < cs.len()
                    && (depth > 0
                        || !is_delim(cs[i])
                        || (cs[i] == '(' && cs[start..i].contains(&'_')))
                    && !"}»]".contains(cs[i])
                    && !pair(i, "<<")
                    && !pair(i, ">>")
                {
                    match cs[i] {
                        '(' => depth += 1,
                        ')' => depth -= 1,
                        _ => {}
                    }
                    i += 1;
                }
                let w: String = cs[start..i].iter().collect();
                if let Some(x) = parse_num(&w) {
                    out.push(Tok::Num(x));
                } else if let Some(x) = w.strip_prefix('∠').and_then(parse_num) {
                    out.push(Tok::Angle(x));
                } else if let Some((x, u)) = w.split_once('_')
                    && let Some(x) = parse_num(x)
                {
                    out.push(Tok::Unit(x, parse_units(u).ok_or(Error::InvalidUnit)?));
                } else if w.starts_with(|c: char| c.is_ascii_digit()) {
                    return Err(Error::Syntax);
                } else {
                    out.push(Tok::Word(normalize(&w)));
                }
            }
        }
    }
    Ok(out)
}

/// The digits of a binary integer and its optional base suffix.
fn parse_bin(w: &str) -> Option<u64> {
    let (digits, b) = match w.chars().last().and_then(Base::from_suffix) {
        Some(b) if w.len() > 1 => (&w[..w.len() - 1], b),
        _ => (w, base()),
    };
    u64::from_str_radix(digits, b.radix()).ok()
}

/// A row of an array as it was written.
enum Row {
    Real(Vec<f64>),
    Complex(Vec<(f64, f64)>),
}

impl Row {
    fn len(&self) -> usize {
        match self {
            Row::Real(r) => r.len(),
            Row::Complex(r) => r.len(),
        }
    }

    fn complex(self) -> Vec<(f64, f64)> {
        match self {
            Row::Real(r) => r.into_iter().map(|x| (x, 0.0)).collect(),
            Row::Complex(r) => r,
        }
    }
}

/// A vector's components, the angles (∠) among them converted from polar
/// or spherical form.
fn polar_row(r: &[(f64, bool)]) -> Res<Vec<f64>> {
    let rect = |v: Vec<f64>| v.into_iter().map(r12).collect();
    match *r {
        _ if r.iter().all(|c| !c.1) => Ok(r.iter().map(|c| c.0).collect()),
        [(rho, false), (t, true)] => Ok(rect(from_polar(rho, t, None))),
        [(rho, false), (t, true), third] => Ok(rect(from_polar(rho, t, Some(third)))),
        _ => Err(Error::Syntax),
    }
}

/// Parses a command line (or the source of a program) into the steps to run.
pub fn parse(src: &str) -> Res<Vec<Node>> {
    let mut p = Parser {
        toks: tokenize(src)?,
        i: 0,
    };
    Ok(p.until(&[""])?.0)
}

/// True while the line is inside a string, name, list, program or complex
/// number, where typed commands belong in the text rather than running.
pub fn is_open(s: &str) -> bool {
    let cs: Vec<char> = s.chars().collect();
    let (mut depth, mut in_str, mut in_quote) = (0i32, false, false);
    let mut i = 0;
    while i < cs.len() {
        let c = cs[i];
        let next = cs.get(i + 1).copied();
        if in_str {
            match c {
                '\\' => i += 1,
                '"' => in_str = false,
                _ => {}
            }
        } else if in_quote {
            in_quote = c != '\'';
        } else {
            match c {
                '"' => in_str = true,
                '\'' => in_quote = true,
                '{' | '«' | '(' | '[' => depth += 1,
                '}' | '»' | ')' | ']' => depth -= 1,
                '<' | '>' if next == Some(c) => {
                    depth += if c == '<' { 1 } else { -1 };
                    i += 1;
                }
                _ => {}
            }
        }
        i += 1;
    }
    in_str || in_quote || depth > 0
}

struct Parser {
    toks: Vec<Tok>,
    i: usize,
}

impl Parser {
    /// Parses steps up to one of `terms` and returns which one ended them.
    /// "" stands for the end of input and "»" for the end of a program.
    fn until(&mut self, terms: &[&str]) -> Res<(Vec<Node>, String)> {
        let mut nodes = vec![];
        loop {
            match self.toks.get(self.i) {
                None if terms.contains(&"") => return Ok((nodes, String::new())),
                None => return Err(Error::Syntax),
                Some(Tok::RProg) if terms.contains(&"»") => {
                    self.i += 1;
                    return Ok((nodes, "»".into()));
                }
                Some(Tok::Word(w)) if terms.contains(&w.as_str()) => {
                    let w = w.clone();
                    self.i += 1;
                    return Ok((nodes, w));
                }
                Some(Tok::Word(w)) if TERMINATORS.contains(&w.as_str()) => {
                    return Err(Error::Syntax);
                }
                Some(_) => nodes.push(self.node()?),
            }
        }
    }

    fn node(&mut self) -> Res<Node> {
        let tok = self.toks[self.i].clone();
        self.i += 1;
        Ok(match tok {
            Tok::Quoted(n) => Node::Push(Obj::Name(n)),
            Tok::Word(w) => match w.as_str() {
                "IF" => {
                    let cond = self.until(&["THEN"])?.0;
                    let (then, end) = self.until(&["ELSE", "END"])?;
                    let els = if end == "ELSE" {
                        Some(self.until(&["END"])?.0)
                    } else {
                        None
                    };
                    Node::If { cond, then, els }
                }
                "START" => {
                    let (body, end) = self.until(&["NEXT", "STEP"])?;
                    Node::Start {
                        body,
                        step: end == "STEP",
                    }
                }
                "FOR" => {
                    let var = match self.toks.get(self.i) {
                        Some(Tok::Word(v)) if valid_name(v) => v.clone(),
                        _ => return Err(Error::Syntax),
                    };
                    self.i += 1;
                    let (body, end) = self.until(&["NEXT", "STEP"])?;
                    Node::For {
                        var,
                        body,
                        step: end == "STEP",
                    }
                }
                "WHILE" => {
                    let cond = self.until(&["REPEAT"])?.0;
                    let body = self.until(&["END"])?.0;
                    Node::While { cond, body }
                }
                "DO" => {
                    let body = self.until(&["UNTIL"])?.0;
                    let cond = self.until(&["END"])?.0;
                    Node::Do { body, cond }
                }
                "CASE" => {
                    let mut clauses = vec![];
                    loop {
                        let (cond, end) = self.until(&["THEN", "END"])?;
                        if end == "END" {
                            break Node::Case {
                                clauses,
                                default: cond,
                            };
                        }
                        clauses.push((cond, self.until(&["END"])?.0));
                    }
                }
                "IFERR" => {
                    let body = self.until(&["THEN"])?.0;
                    let (then, end) = self.until(&["ELSE", "END"])?;
                    let els = if end == "ELSE" {
                        Some(self.until(&["END"])?.0)
                    } else {
                        None
                    };
                    Node::IfErr { body, then, els }
                }
                "DIR" => Node::Push(self.dir()?),
                "→" => {
                    let mut names = vec![];
                    loop {
                        match self.toks.get(self.i) {
                            Some(Tok::Word(v)) if valid_name(v) => names.push(v.clone()),
                            Some(Tok::LProg) if !names.is_empty() => break,
                            Some(Tok::Quoted(n)) if !names.is_empty() => {
                                self.i += 1;
                                let expr = Obj::Name(n.clone());
                                return Ok(Node::LocalAlg { names, expr });
                            }
                            Some(Tok::Alg(e)) if !names.is_empty() => {
                                self.i += 1;
                                let expr = Obj::Alg(e.clone());
                                return Ok(Node::LocalAlg { names, expr });
                            }
                            _ => return Err(Error::Syntax),
                        }
                        self.i += 1;
                    }
                    self.i += 1;
                    let body = self.until(&["»"])?.0;
                    Node::Local { names, body }
                }
                _ => Node::Word(w),
            },
            tok => Node::Push(self.object(tok)?),
        })
    }

    /// An object written literally, the one `tok` starts.
    fn object(&mut self, tok: Tok) -> Res<Obj> {
        Ok(match tok {
            Tok::Num(x) => Obj::Real(x),
            Tok::Cplx(re, im) => Obj::Complex(re, im),
            Tok::Str(s) => Obj::Str(s),
            Tok::Quoted(n) => Obj::Name(n),
            Tok::Alg(e) => Obj::Alg(e),
            Tok::Bin(n) => Obj::Bin(n),
            Tok::Unit(x, u) => Obj::Unit(x, u),
            Tok::LBrace => self.list()?,
            Tok::LBrack => self.array()?,
            Tok::LProg => Obj::Prog(self.until(&["»"])?.0),
            Tok::Tag(t) => {
                let tok = self.toks.get(self.i).cloned().ok_or(Error::Syntax)?;
                self.i += 1;
                let o = match tok {
                    Tok::Word(w) if w == "DIR" => self.dir()?,
                    Tok::Word(w) => Obj::Name(w),
                    tok => self.object(tok)?,
                };
                Obj::Tag(t, Box::new(o))
            }
            Tok::RBrace | Tok::RProg | Tok::RBrack | Tok::Angle(_) | Tok::Word(_) => {
                return Err(Error::Syntax);
            }
        })
    }

    // [ 1 2 3 ] is a vector, [[ 1 2 ] [ 3 4 ]] a matrix.
    fn array(&mut self) -> Res<Obj> {
        // A row's elements, complex if any is.
        let row = |p: &mut Parser| -> Res<Row> {
            let (mut r, mut z) = (vec![], vec![]);
            loop {
                let tok = p.toks.get(p.i).cloned().ok_or(Error::Syntax)?;
                p.i += 1;
                match tok {
                    Tok::Num(x) => r.push((x, false)),
                    Tok::Angle(x) => r.push((x, true)),
                    Tok::Cplx(re, im) => z.push((r.len(), (re, im))),
                    Tok::RBrack if z.is_empty() => return polar_row(&r).map(Row::Real),
                    Tok::RBrack => {
                        if r.iter().any(|c| c.1) {
                            return Err(Error::Syntax);
                        }
                        let mut out: Vec<(f64, f64)> = r.iter().map(|c| (c.0, 0.0)).collect();
                        for (k, (i, v)) in z.into_iter().enumerate() {
                            out.insert(i + k, v);
                        }
                        return Ok(Row::Complex(out));
                    }
                    _ => return Err(Error::Syntax),
                }
            }
        };
        if self.toks.get(self.i) != Some(&Tok::LBrack) {
            return Ok(match row(self)? {
                Row::Real(r) => Obj::Vect(r),
                Row::Complex(r) => Obj::CVect(r),
            });
        }
        let mut rows: Vec<Row> = vec![];
        loop {
            let tok = self.toks.get(self.i).cloned().ok_or(Error::Syntax)?;
            self.i += 1;
            match tok {
                Tok::LBrack => rows.push(row(self)?),
                Tok::RBrack => break,
                _ => return Err(Error::Syntax),
            }
        }
        let cols = rows[0].len();
        if cols == 0 || rows.iter().any(|r| r.len() != cols) {
            return Err(Error::Syntax);
        }
        if rows.iter().all(|r| matches!(r, Row::Real(_))) {
            return Ok(Obj::Mat(
                rows.into_iter()
                    .map(|r| match r {
                        Row::Real(r) => r,
                        Row::Complex(_) => unreachable!(),
                    })
                    .collect(),
            ));
        }
        Ok(Obj::CMat(rows.into_iter().map(Row::complex).collect()))
    }

    // DIR name object name object ... END
    fn dir(&mut self) -> Res<Obj> {
        let mut vars = vec![];
        loop {
            let tok = self.toks.get(self.i).cloned().ok_or(Error::Syntax)?;
            self.i += 1;
            let name = match tok {
                Tok::Word(w) if w == "END" => return Ok(Obj::Dir(vars)),
                Tok::Word(n) | Tok::Quoted(n) if valid_name(&n) => n,
                _ => return Err(Error::Syntax),
            };
            let tok = self.toks.get(self.i).cloned().ok_or(Error::Syntax)?;
            self.i += 1;
            let v = match tok {
                Tok::Word(w) if w == "DIR" => self.dir()?,
                tok => self.object(tok)?,
            };
            vars.push((name, v));
        }
    }

    // Inside a list nothing runs: words become names, programs stay programs.
    fn list(&mut self) -> Res<Obj> {
        let mut items = vec![];
        loop {
            let tok = self.toks.get(self.i).cloned().ok_or(Error::Syntax)?;
            self.i += 1;
            items.push(match tok {
                Tok::RBrace => return Ok(Obj::List(items)),
                Tok::Word(w) if w == "DIR" => self.dir()?,
                Tok::Quoted(n) | Tok::Word(n) => Obj::Name(n),
                tok => self.object(tok)?,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::obj::Fmt;

    fn round_trip(src: &str) -> String {
        let nodes = parse(src).unwrap();
        let [Node::Push(o)] = &nodes[..] else {
            panic!("{nodes:?}")
        };
        o.show(Fmt::Std)
    }

    #[test]
    fn programs_read_back() {
        assert_eq!(
            round_trip("<< -> a b << a b * >> >>"),
            "« → a b « a b * » »"
        );
        assert_eq!(
            round_trip("« IF DUP 0 < THEN NEG END »"),
            "« IF DUP 0 < THEN NEG END »"
        );
        assert_eq!(
            round_trip("« 1 10 FOR i i NEXT 'X' STO »"),
            "« 1 10 FOR i i NEXT 'X' STO »"
        );
        assert_eq!(
            round_trip("{ 1 A \"s\" { } (1,2) }"),
            "{ 1 A \"s\" { } (1,2) }"
        );
        assert_eq!(round_trip("« x SQRT R->C »"), "« x √ R→C »");
    }

    #[test]
    fn syntax_errors() {
        assert_eq!(parse("« 1"), Err(Error::Syntax));
        assert_eq!(parse("1 THEN"), Err(Error::Syntax));
        assert_eq!(parse("\"abc"), Err(Error::Syntax));
        assert_eq!(parse("12abc"), Err(Error::Syntax));
        assert_eq!(parse("'X+'"), Err(Error::Syntax));
        assert_eq!(parse("[ 1 [ 2 ] ]"), Err(Error::Syntax));
        assert_eq!(parse("[[ 1 2 ] [ 3 ]]"), Err(Error::Syntax));
    }

    #[test]
    fn new_objects_read_back() {
        assert_eq!(round_trip("'X^2+1'"), "'X^2+1'");
        assert_eq!(round_trip("[1 2 3]"), "[ 1 2 3 ]");
        assert_eq!(round_trip("[[1 2][3 4]]"), "[[ 1 2 ] [ 3 4 ]]");
        assert_eq!(round_trip("#FFh"), "# 255d");
        assert_eq!(round_trip("# 1010b"), "# 10d");
        assert_eq!(round_trip(":A: 5"), ":A:5");
        assert_eq!(
            round_trip("DIR A 1 B DIR C 'X' END END"),
            "DIR A 1 B DIR C 'X' END END"
        );
        assert_eq!(
            round_trip("« CASE DUP 0 < THEN NEG END DUP 0 == THEN 1 + END END »"),
            "« CASE DUP 0 < THEN NEG END DUP 0 == THEN 1 + END END »"
        );
        assert_eq!(
            round_trip("« IFERR 1 0 / THEN ERRM END »"),
            "« IFERR 1 0 / THEN ERRM END »"
        );
        assert_eq!(round_trip("« → x 'x^2' »"), "« → x 'x^2' »");
    }

    #[test]
    fn open_lines() {
        assert!(is_open("« 1"));
        assert!(is_open("<< 1"));
        assert!(is_open("\"ab"));
        assert!(is_open("'X"));
        assert!(!is_open("« 1 »"));
        assert!(!is_open("12"));
    }
}

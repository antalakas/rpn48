//! Command-line and program parsing.

use crate::calc::{Error, Res};
use crate::obj::{Node, Obj, r12};

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Num(f64),
    Cplx(f64, f64),
    Str(String),
    Quoted(String),
    LBrace,
    RBrace,
    LProg,
    RProg,
    Word(String),
}

/// Words that open or close a control structure.
const KEYWORDS: &[&str] = &[
    "IF", "THEN", "ELSE", "END", "START", "FOR", "NEXT", "STEP", "WHILE", "REPEAT", "DO", "UNTIL",
];
const TERMINATORS: &[&str] = &["THEN", "ELSE", "END", "NEXT", "STEP", "REPEAT", "UNTIL"];

fn is_delim(c: char) -> bool {
    c.is_whitespace() || "\"'{}«»(".contains(c)
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
            '}', '»', ')', ',', '+', '-', '*', '/', '^', '=', '<', '>', '[', ']',
        ])
        && parse_num(s).is_none()
        && !KEYWORDS.contains(&s)
        && s != "→"
}

/// ASCII spellings of the calculator's special characters, so they can be
/// typed: -> for →, SQRT for √, <= for ≤ and so on.
fn normalize(w: &str) -> String {
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
                let name: String = cs[i + 1..i + 1 + len].iter().collect();
                let name = normalize(name.trim());
                if !valid_name(&name) {
                    return Err(Error::Algebraic);
                }
                out.push(Tok::Quoted(name));
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
                let [re, im] = parts[..] else {
                    return Err(Error::Syntax);
                };
                let (re, im) = (
                    parse_num(re).ok_or(Error::Syntax)?,
                    parse_num(im).ok_or(Error::Syntax)?,
                );
                out.push(Tok::Cplx(re, im));
                i += len + 2;
            }
            '{' | '}' | '«' | '»' => {
                out.push(match c {
                    '{' => Tok::LBrace,
                    '}' => Tok::RBrace,
                    '«' => Tok::LProg,
                    _ => Tok::RProg,
                });
                i += 1;
            }
            _ => {
                let start = i;
                while i < cs.len()
                    && !is_delim(cs[i])
                    && !"}»".contains(cs[i])
                    && !pair(i, "<<")
                    && !pair(i, ">>")
                {
                    i += 1;
                }
                let w: String = cs[start..i].iter().collect();
                if let Some(x) = parse_num(&w) {
                    out.push(Tok::Num(x));
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
                '{' | '«' | '(' => depth += 1,
                '}' | '»' | ')' => depth -= 1,
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
            Tok::Num(x) => Node::Push(Obj::Real(x)),
            Tok::Cplx(re, im) => Node::Push(Obj::Complex(re, im)),
            Tok::Str(s) => Node::Push(Obj::Str(s)),
            Tok::Quoted(n) => Node::Push(Obj::Name(n)),
            Tok::LBrace => Node::Push(self.list()?),
            Tok::LProg => Node::Push(Obj::Prog(self.until(&["»"])?.0)),
            Tok::RBrace | Tok::RProg => return Err(Error::Syntax),
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
                "→" => {
                    let mut names = vec![];
                    loop {
                        match self.toks.get(self.i) {
                            Some(Tok::Word(v)) if valid_name(v) => names.push(v.clone()),
                            Some(Tok::LProg) if !names.is_empty() => break,
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
        })
    }

    // Inside a list nothing runs: words become names, programs stay programs.
    fn list(&mut self) -> Res<Obj> {
        let mut items = vec![];
        loop {
            let tok = self.toks.get(self.i).cloned().ok_or(Error::Syntax)?;
            self.i += 1;
            items.push(match tok {
                Tok::RBrace => return Ok(Obj::List(items)),
                Tok::Num(x) => Obj::Real(x),
                Tok::Cplx(re, im) => Obj::Complex(re, im),
                Tok::Str(s) => Obj::Str(s),
                Tok::Quoted(n) | Tok::Word(n) => Obj::Name(n),
                Tok::LBrace => self.list()?,
                Tok::LProg => Obj::Prog(self.until(&["»"])?.0),
                Tok::RProg => return Err(Error::Syntax),
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
        assert_eq!(parse("'X+1'"), Err(Error::Algebraic));
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

//! Algebraic objects: expressions such as 'X^2+1', how they are read and
//! written, and the symbolic operations on them (derivatives, substitution).

use crate::calc::{Error, Res};
use crate::obj::{Fmt, Node, Obj, fmt_real};
use crate::parse::{normalize, parse_num};

#[derive(Clone, Debug, PartialEq)]
pub enum Expr {
    Num(f64),
    Name(String),
    Neg(Box<Expr>),
    /// A binary operator: + - * / ^ or = for equations.
    Op(char, Box<Expr>, Box<Expr>),
    /// A function applied to its arguments, SIN(X) or a user function F(X,Y).
    Fn(String, Vec<Expr>),
}

use Expr::*;

pub fn op(c: char, a: Expr, b: Expr) -> Expr {
    Op(c, Box::new(a), Box::new(b))
}

pub fn neg(a: Expr) -> Expr {
    Neg(Box::new(a))
}

pub fn func(name: &str, args: Vec<Expr>) -> Expr {
    Fn(name.into(), args)
}

/// Commands that, given names or algebraics, build an expression instead of
/// computing: 'X' SIN is 'SIN(X)'.
pub const SYMBOLIC: &[&str] = &[
    "+", "-", "*", "/", "^", "NEG", "INV", "SQ", "√", "ABS", "SIGN", "IP", "FP", "FLOOR", "CEIL",
    "MOD", "MIN", "MAX", "%", "XROOT", "!", "SIN", "COS", "TAN", "ASIN", "ACOS", "ATAN", "SINH",
    "COSH", "TANH", "ASINH", "ACOSH", "ATANH", "LN", "LOG", "EXP", "ALOG", "LNP1", "EXPM", "RE",
    "IM", "CONJ", "ARG",
];

/// The expression for an object that can be part of one.
pub fn from_obj(o: &Obj) -> Res<Expr> {
    match o {
        Obj::Real(x) => Ok(Num(*x)),
        Obj::Name(n) => Ok(Name(n.clone())),
        Obj::Alg(e) => Ok(e.clone()),
        _ => Err(Error::BadType),
    }
}

/// An expression as an object: numbers and lone names become those objects.
pub fn to_obj(e: Expr) -> Obj {
    match e {
        Num(x) => Obj::Real(x),
        Name(n) => Obj::Name(n),
        e => Obj::Alg(e),
    }
}

pub fn is_symbolic(o: &Obj) -> bool {
    matches!(o, Obj::Name(_) | Obj::Alg(_))
}

/// The expression applying command `w` to `args`.
pub fn build(w: &str, args: &[Obj]) -> Res<Expr> {
    let mut es = args.iter().map(from_obj).collect::<Res<Vec<_>>>()?;
    Ok(match (w, &mut es[..]) {
        ("NEG", [a]) => neg(a.clone()),
        (c, [a, b]) if c.len() == 1 && "+-*/^=".contains(c) => {
            op(c.chars().next().unwrap(), a.clone(), b.clone())
        }
        _ => Fn(w.into(), es),
    })
}

/// The steps that evaluate `e` on the stack: 'X+2' runs X 2 +.
pub fn to_nodes(e: &Expr, out: &mut Vec<Node>) {
    match e {
        Num(x) => out.push(Node::Push(Obj::Real(*x))),
        Name(n) => out.push(Node::Word(n.clone())),
        Neg(a) => {
            to_nodes(a, out);
            out.push(Node::Word("NEG".into()));
        }
        Op(c, a, b) => {
            to_nodes(a, out);
            to_nodes(b, out);
            out.push(Node::Word(c.to_string()));
        }
        Fn(f, args) => {
            for a in args {
                to_nodes(a, out);
            }
            out.push(Node::Word(f.clone()));
        }
    }
}

// ---------------------------------------------------------------- writing

fn prec(e: &Expr) -> u8 {
    match e {
        Op('=', ..) => 1,
        Op('+' | '-', ..) => 2,
        Op('*' | '/', ..) => 3,
        Neg(_) => 4,
        Num(x) if *x < 0.0 => 4,
        Op(..) => 5,
        _ => 6,
    }
}

pub fn write_expr(out: &mut String, e: &Expr, fmt: Fmt) {
    let wrap = |out: &mut String, e: &Expr, paren: bool| {
        if paren {
            out.push('(');
        }
        write_expr(out, e, fmt);
        if paren {
            out.push(')');
        }
    };
    match e {
        Num(x) => out.push_str(&fmt_real(*x, fmt)),
        Name(n) => out.push_str(n),
        Neg(a) => {
            out.push('-');
            wrap(out, a, prec(a) <= 4);
        }
        Op(c, a, b) => {
            let p = prec(e);
            // ^ groups to the right, the others to the left.
            let (lp, rp) = if *c == '^' {
                (prec(a) <= p, prec(b) < p)
            } else {
                (
                    prec(a) < p,
                    prec(b) < p || (prec(b) == p && "-/".contains(*c)),
                )
            };
            wrap(out, a, lp);
            out.push(*c);
            wrap(out, b, rp);
        }
        Fn(f, args) if f == "√" && args.len() == 1 => {
            out.push('√');
            wrap(out, &args[0], prec(&args[0]) < 6);
        }
        Fn(f, args) if f == "!" && args.len() == 1 => {
            wrap(out, &args[0], prec(&args[0]) < 6);
            out.push('!');
        }
        Fn(f, args) => {
            out.push_str(f);
            out.push('(');
            for (i, a) in args.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_expr(out, a, fmt);
            }
            out.push(')');
        }
    }
}

// ---------------------------------------------------------------- reading

#[derive(Clone, Debug, PartialEq)]
enum T {
    Num(f64),
    Name(String),
    Sym(char),
}

fn lex(s: &str) -> Res<Vec<T>> {
    let cs: Vec<char> = s.chars().collect();
    let mut out = vec![];
    let mut i = 0;
    while i < cs.len() {
        let c = cs[i];
        if c.is_whitespace() {
            i += 1;
        } else if "+-*/^=(),!√".contains(c) {
            out.push(T::Sym(c));
            i += 1;
        } else if c.is_ascii_digit() || c == '.' {
            let start = i;
            while i < cs.len() && (cs[i].is_ascii_digit() || cs[i] == '.') {
                i += 1;
            }
            if i < cs.len() && "Ee".contains(cs[i]) {
                let mut j = i + 1;
                if j < cs.len() && "+-".contains(cs[j]) {
                    j += 1;
                }
                if j < cs.len() && cs[j].is_ascii_digit() {
                    i = j;
                    while i < cs.len() && cs[i].is_ascii_digit() {
                        i += 1;
                    }
                }
            }
            let w: String = cs[start..i].iter().collect();
            out.push(T::Num(parse_num(&w).ok_or(Error::Syntax)?));
        } else {
            let start = i;
            while i < cs.len() && !cs[i].is_whitespace() && !"+-*/^=(),!√'".contains(cs[i]) {
                i += 1;
            }
            let w: String = cs[start..i].iter().collect();
            out.push(T::Name(normalize(&w)));
        }
    }
    Ok(out)
}

/// Reads the text between the quotes of an algebraic.
pub fn parse_alg(s: &str) -> Res<Expr> {
    let mut p = P { t: lex(s)?, i: 0 };
    let e = p.equation()?;
    if p.i != p.t.len() {
        return Err(Error::Syntax);
    }
    Ok(e)
}

struct P {
    t: Vec<T>,
    i: usize,
}

impl P {
    fn eat(&mut self, c: char) -> bool {
        if self.t.get(self.i) == Some(&T::Sym(c)) {
            self.i += 1;
            true
        } else {
            false
        }
    }

    fn equation(&mut self) -> Res<Expr> {
        let a = self.sum()?;
        if self.eat('=') {
            return Ok(op('=', a, self.sum()?));
        }
        Ok(a)
    }

    fn sum(&mut self) -> Res<Expr> {
        let mut a = self.term()?;
        loop {
            if self.eat('+') {
                a = op('+', a, self.term()?);
            } else if self.eat('-') {
                a = op('-', a, self.term()?);
            } else {
                return Ok(a);
            }
        }
    }

    fn term(&mut self) -> Res<Expr> {
        let mut a = self.unary()?;
        loop {
            if self.eat('*') {
                a = op('*', a, self.unary()?);
            } else if self.eat('/') {
                a = op('/', a, self.unary()?);
            } else {
                return Ok(a);
            }
        }
    }

    fn unary(&mut self) -> Res<Expr> {
        if self.eat('-') {
            return Ok(match self.unary()? {
                Num(x) => Num(-x),
                a => neg(a),
            });
        }
        if self.eat('+') {
            return self.unary();
        }
        let a = self.postfix()?;
        if self.eat('^') {
            return Ok(op('^', a, self.unary()?));
        }
        Ok(a)
    }

    fn postfix(&mut self) -> Res<Expr> {
        let mut a = self.atom()?;
        while self.eat('!') {
            a = func("!", vec![a]);
        }
        Ok(a)
    }

    fn atom(&mut self) -> Res<Expr> {
        let t = self.t.get(self.i).cloned().ok_or(Error::Syntax)?;
        self.i += 1;
        match t {
            T::Num(x) => Ok(Num(x)),
            T::Sym('(') => {
                let e = self.equation()?;
                if !self.eat(')') {
                    return Err(Error::Syntax);
                }
                Ok(e)
            }
            T::Sym('√') => Ok(func("√", vec![self.postfix()?])),
            T::Name(n) => {
                if !self.eat('(') {
                    return Ok(Name(n));
                }
                let mut args = vec![self.equation()?];
                while self.eat(',') {
                    args.push(self.equation()?);
                }
                if !self.eat(')') {
                    return Err(Error::Syntax);
                }
                Ok(Fn(n, args))
            }
            T::Sym(_) => Err(Error::Syntax),
        }
    }
}

// ---------------------------------------------------------------- symbolic

/// Replaces the names in `subs` with their expressions.
pub fn subst(e: &Expr, subs: &[(String, Expr)]) -> Expr {
    match e {
        Name(n) => subs
            .iter()
            .find(|(s, _)| s == n)
            .map_or_else(|| e.clone(), |(_, v)| v.clone()),
        Num(_) => e.clone(),
        Neg(a) => neg(subst(a, subs)),
        Op(c, a, b) => op(*c, subst(a, subs), subst(b, subs)),
        Fn(f, args) => Fn(f.clone(), args.iter().map(|a| subst(a, subs)).collect()),
    }
}

pub fn contains(e: &Expr, x: &str) -> bool {
    match e {
        Name(n) => n == x,
        Num(_) => false,
        Neg(a) => contains(a, x),
        Op(_, a, b) => contains(a, x) || contains(b, x),
        Fn(_, args) => args.iter().any(|a| contains(a, x)),
    }
}

/// The derivative of `e` with respect to `x`. `angle` is the factor that
/// converts the angle mode to radians (π/180 in degrees), or None in radians.
pub fn deriv(e: &Expr, x: &str, angle: &Option<Expr>) -> Res<Expr> {
    if !contains(e, x) {
        return Ok(Num(0.0));
    }
    let d = |e: &Expr| deriv(e, x, angle);
    let n = Num;
    Ok(match e {
        Num(_) => n(0.0),
        Name(_) => n(1.0),
        Neg(a) => neg(d(a)?),
        Op('+', a, b) => op('+', d(a)?, d(b)?),
        Op('-', a, b) => op('-', d(a)?, d(b)?),
        Op('*', a, b) => op(
            '+',
            op('*', d(a)?, (**b).clone()),
            op('*', (**a).clone(), d(b)?),
        ),
        Op('/', a, b) => op(
            '/',
            op(
                '-',
                op('*', d(a)?, (**b).clone()),
                op('*', (**a).clone(), d(b)?),
            ),
            op('^', (**b).clone(), n(2.0)),
        ),
        Op('^', a, b) if !contains(b, x) => op(
            '*',
            op(
                '*',
                (**b).clone(),
                op('^', (**a).clone(), op('-', (**b).clone(), n(1.0))),
            ),
            d(a)?,
        ),
        // a^b = EXP(b·LN(a)), so its derivative is a^b·(b'·LN(a) + b·a'/a).
        Op('^', a, b) => op(
            '*',
            e.clone(),
            op(
                '+',
                op('*', d(b)?, func("LN", vec![(**a).clone()])),
                op('/', op('*', (**b).clone(), d(a)?), (**a).clone()),
            ),
        ),
        Op('=', a, b) => op('=', d(a)?, d(b)?),
        Fn(f, args) if args.len() == 1 => {
            let u = &args[0];
            let du = d(u)?;
            let f1 = |name: &str| func(name, vec![u.clone()]);
            let trig = |g: Expr| match angle {
                Some(k) => op('*', g, k.clone()),
                None => g,
            };
            // Inverse trig functions return angles, so their derivatives are
            // divided by the factor instead.
            let atrig = |g: Expr| match angle {
                Some(k) => op('/', g, k.clone()),
                None => g,
            };
            let one_minus_sq = op('-', n(1.0), op('^', u.clone(), n(2.0)));
            let outer = match f.as_str() {
                "SIN" => trig(f1("COS")),
                "COS" => trig(neg(f1("SIN"))),
                "TAN" => trig(op('+', n(1.0), op('^', f1("TAN"), n(2.0)))),
                "ASIN" => atrig(op('/', n(1.0), func("√", vec![one_minus_sq]))),
                "ACOS" => atrig(neg(op('/', n(1.0), func("√", vec![one_minus_sq])))),
                "ATAN" => atrig(op('/', n(1.0), op('+', n(1.0), op('^', u.clone(), n(2.0))))),
                "SINH" => f1("COSH"),
                "COSH" => f1("SINH"),
                "TANH" => op('/', n(1.0), op('^', f1("COSH"), n(2.0))),
                "ASINH" => op(
                    '/',
                    n(1.0),
                    func("√", vec![op('+', op('^', u.clone(), n(2.0)), n(1.0))]),
                ),
                "ACOSH" => op(
                    '/',
                    n(1.0),
                    func("√", vec![op('-', op('^', u.clone(), n(2.0)), n(1.0))]),
                ),
                "ATANH" => op('/', n(1.0), one_minus_sq),
                "LN" => op('/', n(1.0), u.clone()),
                "LOG" => op('/', n(1.0), op('*', u.clone(), func("LN", vec![n(10.0)]))),
                "EXP" => f1("EXP"),
                "ALOG" => op('*', f1("ALOG"), func("LN", vec![n(10.0)])),
                "LNP1" => op('/', n(1.0), op('+', u.clone(), n(1.0))),
                "EXPM" => f1("EXP"),
                "√" => op('/', n(1.0), op('*', n(2.0), f1("√"))),
                "SQ" => op('*', n(2.0), u.clone()),
                "INV" => neg(op('/', n(1.0), op('^', u.clone(), n(2.0)))),
                "NEG" => n(-1.0),
                "ABS" => f1("SIGN"),
                _ => return Err(Error::BadValue),
            };
            op('*', outer, du)
        }
        _ => return Err(Error::BadValue),
    })
}

/// Light algebraic tidying: drops zeros and ones, and folds negations.
/// Numeric subexpressions are folded by the calculator, which knows the
/// functions (`Calc::collect`).
pub fn tidy(e: Expr) -> Expr {
    let z = |e: &Expr| *e == Num(0.0);
    let one = |e: &Expr| *e == Num(1.0);
    match e {
        Neg(a) => match tidy(*a) {
            Num(x) => Num(-x),
            Neg(b) => *b,
            a => neg(a),
        },
        Op(c, a, b) => {
            let (a, b) = (tidy(*a), tidy(*b));
            match c {
                '+' if z(&a) => b,
                '+' | '-' if z(&b) => a,
                '-' if z(&a) => tidy(neg(b)),
                '+' => match b {
                    Neg(b) => op('-', a, *b),
                    Num(y) if y < 0.0 => op('-', a, Num(-y)),
                    b => op('+', a, b),
                },
                '-' => match b {
                    Neg(b) => op('+', a, *b),
                    b => op('-', a, b),
                },
                '*' if z(&a) || z(&b) => Num(0.0),
                '*' if one(&a) => b,
                '*' if one(&b) => a,
                '*' if a == Num(-1.0) => tidy(neg(b)),
                '*' if b == Num(-1.0) => tidy(neg(a)),
                '/' if z(&a) => Num(0.0),
                '/' if one(&b) => a,
                '^' if z(&b) => Num(1.0),
                '^' if one(&b) => a,
                c => op(c, a, b),
            }
        }
        Fn(f, args) => Fn(f, args.into_iter().map(tidy).collect()),
        e => e,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rt(s: &str) -> String {
        let mut out = String::new();
        write_expr(&mut out, &parse_alg(s).unwrap(), Fmt::Std);
        out
    }

    #[test]
    fn read_and_write() {
        assert_eq!(rt("X^2+1"), "X^2+1");
        assert_eq!(rt("(X+1)*(X-1)"), "(X+1)*(X-1)");
        assert_eq!(rt("A-(B-C)"), "A-(B-C)");
        assert_eq!(rt("A-B-C"), "A-B-C");
        assert_eq!(rt("2^3^2"), "2^3^2");
        assert_eq!(rt("(2^3)^2"), "(2^3)^2");
        assert_eq!(rt("-X^2"), "-X^2");
        assert_eq!(rt("(-X)^2"), "(-X)^2");
        assert_eq!(rt("X^-2"), "X^(-2)");
        assert_eq!(rt("-(-X)"), "-(-X)");
        assert_eq!(rt("SIN(X)*√(X+1)"), "SIN(X)*√(X+1)");
        assert_eq!(rt("F(X, Y)=0"), "F(X,Y)=0");
        assert_eq!(rt("SQRT(X)"), "√X");
        assert_eq!(rt("N!/2"), "N!/2");
        assert_eq!(rt("1.5E-3*X"), ".0015*X");
        assert!(parse_alg("X+").is_err());
        assert!(parse_alg("(X").is_err());
    }
}

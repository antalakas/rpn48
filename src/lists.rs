//! The HP 48G list commands (DOLIST, DOSUBS, SEQ, STREAM and the rest),
//! math applied across lists, and the string commands.

use std::cmp::Ordering;

use crate::calc::{Calc, Error, OpFn, Res, index, op, real_of};
use crate::obj::Obj;

/// Commands besides the symbolic ones that apply to each element of a list.
pub const MAPPED: &[&str] = &[
    "RND", "TRNC", "MANT", "XPON", "→HMS", "HMS→", "HMS+", "HMS-", "D→R", "R→D", "%CH", "%T",
    "COMB", "PERM", "R→B", "B→R", "R→C", "C→R", "NOT", "AND", "OR", "XOR", "CHR", "NUM",
];

/// `w` applied to lists element by element: { 1 2 } { 3 4 } * is { 3 8 }, and
/// a single object pairs with every element, { 1 2 } 10 * is { 10 20 }.
pub fn map(c: &Calc, w: &str, f: OpFn, a: &[Obj]) -> Res<Vec<Obj>> {
    let len = a
        .iter()
        .filter_map(|o| match o {
            Obj::List(l) => Some(l.len()),
            _ => None,
        })
        .try_fold(None, |n, l| match n {
            Some(n) if n != l => Err(Error::Dim),
            _ => Ok(Some(l)),
        })?
        .unwrap_or(0);
    let mut out = vec![];
    for i in 0..len {
        let args: Vec<Obj> = a
            .iter()
            .map(|o| match o {
                Obj::List(l) => l[i].clone(),
                o => o.clone(),
            })
            .collect();
        let r = c.compute(w, f, &args)?;
        out.push(match <[Obj; 1]>::try_from(r) {
            Ok([o]) => o,
            Err(r) => Obj::List(r),
        });
    }
    Ok(vec![Obj::List(out)])
}

fn list_of(o: &Obj) -> Res<&Vec<Obj>> {
    match o {
        Obj::List(l) => Ok(l),
        _ => Err(Error::BadType),
    }
}

fn str_of(o: &Obj) -> Res<&String> {
    match o {
        Obj::Str(s) => Ok(s),
        _ => Err(Error::BadType),
    }
}

fn order(a: &Obj, b: &Obj) -> Res<Ordering> {
    match (a, b) {
        (Obj::Real(x), Obj::Real(y)) => Ok(x.total_cmp(y)),
        (Obj::Str(x), Obj::Str(y)) => Ok(x.cmp(y)),
        _ => Err(Error::BadType),
    }
}

// ΣLIST, ΠLIST: the list folded with + or *.
fn fold(c: &Calc, l: &[Obj], o: char) -> Res<Vec<Obj>> {
    let (first, rest) = l.split_first().ok_or(Error::BadValue)?;
    let mut acc = first.clone();
    for x in rest {
        acc = c.num2(o, &acc, x)?;
    }
    Ok(vec![acc])
}

pub fn pure(w: &str) -> Option<(usize, OpFn)> {
    use Obj::*;
    match w {
        "REVLIST" => op(1, |_, a| {
            let mut l = list_of(&a[0])?.clone();
            l.reverse();
            Ok(vec![List(l)])
        }),
        "SORT" => op(1, |_, a| {
            let mut l = list_of(&a[0])?.clone();
            for w in l.windows(2) {
                order(&w[0], &w[1])?;
            }
            l.sort_by(|x, y| order(x, y).unwrap_or(Ordering::Equal));
            Ok(vec![List(l)])
        }),
        "ΣLIST" => op(1, |c, a| fold(c, list_of(&a[0])?, '+')),
        "ΠLIST" => op(1, |c, a| fold(c, list_of(&a[0])?, '*')),
        "ΔLIST" => op(1, |c, a| {
            let l = list_of(&a[0])?;
            if l.len() < 2 {
                return Err(Error::BadValue);
            }
            let d = l
                .windows(2)
                .map(|w| c.num2('-', &w[1], &w[0]))
                .collect::<Res<_>>()?;
            Ok(vec![List(d)])
        }),
        // ADD adds lists element by element, where + would join them.
        "ADD" => op(2, |c, a| match (&a[0], &a[1]) {
            (List(_), _) | (_, List(_)) => {
                map(c, "ADD", |c, a| Ok(vec![c.num2('+', &a[0], &a[1])?]), a)
            }
            _ => Ok(vec![c.num2('+', &a[0], &a[1])?]),
        }),
        "POS" => op(2, |_, a| {
            let p = match (&a[0], &a[1]) {
                (Str(s), Str(t)) => s.find(t.as_str()).map(|i| s[..i].chars().count()),
                (List(l), o) => l.iter().position(|x| x == o),
                _ => return Err(Error::BadType),
            };
            Ok(vec![Real(p.map_or(0.0, |i| i as f64 + 1.0))])
        }),
        // "HELLO" 2 "IPP" REPL is "HIPPO".
        "REPL" => op(3, |_, a| match (&a[0], &a[1], &a[2]) {
            (Str(s), Real(i), Str(t)) => {
                let mut cs: Vec<char> = s.chars().collect();
                let i = index(*i, cs.len() + 1)?;
                let ts: Vec<char> = t.chars().collect();
                let end = (i + ts.len()).min(cs.len());
                cs.splice(i..end, ts);
                Ok(vec![Str(cs.into_iter().collect())])
            }
            (List(l), Real(i), List(t)) => {
                let mut l = l.clone();
                let i = index(*i, l.len() + 1)?;
                let end = (i + t.len()).min(l.len());
                l.splice(i..end, t.iter().cloned());
                Ok(vec![List(l)])
            }
            _ => Err(Error::BadType),
        }),
        "NUM" => op(1, |_, a| {
            let c = str_of(&a[0])?.chars().next().ok_or(Error::BadValue)?;
            Ok(vec![Real(c as u32 as f64)])
        }),
        "CHR" => op(1, |_, a| {
            let n = real_of(&a[0])?;
            let c = char::from_u32(n as u32).ok_or(Error::BadValue)?;
            Ok(vec![Str(c.into())])
        }),
        _ => None,
    }
}

impl Calc {
    /// Takes `n` arguments off the stack, runs `f` on them, and puts them
    /// back if it fails.
    fn with_args(
        &mut self,
        n: usize,
        f: impl FnOnce(&mut Self, &[Obj]) -> Res<Vec<Obj>>,
    ) -> Res<()> {
        self.need(n)?;
        let args = self.stack.split_off(self.stack.len() - n);
        let base = self.stack.len();
        match f(self, &args) {
            Ok(out) => {
                self.stack.extend(out);
                Ok(())
            }
            Err(e) => {
                self.stack.truncate(base);
                self.stack.extend(args);
                Err(e)
            }
        }
    }

    /// Pushes `args`, evaluates `prog` and returns what it left in their place.
    fn results(&mut self, args: impl IntoIterator<Item = Obj>, prog: &Obj) -> Res<Vec<Obj>> {
        let base = self.stack.len();
        self.stack.extend(args);
        self.eval_obj(prog.clone())?;
        if self.stack.len() < base {
            return Err(Error::TooFewArgs);
        }
        Ok(self.stack.split_off(base))
    }

    pub(crate) fn lists_cmd(&mut self, w: &str) -> Option<Res<()>> {
        Some(match w {
            // list1 … listn n prog DOLIST; n may be left out for one list.
            "DOLIST" => self.need(2).and_then(|_| {
                let (n, counted) = match self.peek(2) {
                    Obj::List(_) => (1, 0),
                    Obj::Real(n) if *n >= 1.0 && n.fract() == 0.0 => (*n as usize, 1),
                    Obj::Real(_) => return Err(Error::BadValue),
                    _ => return Err(Error::BadType),
                };
                self.with_args(n + counted + 1, |c, a| {
                    let (lists, prog) = (&a[..n], &a[n + counted..]);
                    let lists = lists.iter().map(list_of).collect::<Res<Vec<_>>>()?;
                    let len = lists[0].len();
                    if lists.iter().any(|l| l.len() != len) {
                        return Err(Error::Dim);
                    }
                    let mut out = vec![];
                    for i in 0..len {
                        out.extend(c.results(lists.iter().map(|l| l[i].clone()), &prog[0])?);
                    }
                    Ok(vec![Obj::List(out)])
                })
            }),
            // list n prog DOSUBS runs prog on each run of n neighbours.
            "DOSUBS" => self.with_args(3, |c, a| {
                let l = list_of(&a[0])?;
                let n = real_of(&a[1])?;
                if n < 1.0 || n as usize > l.len() {
                    return Err(Error::BadValue);
                }
                let n = n as usize;
                let count = l.len() + 1 - n;
                let mut out = vec![];
                for i in 0..count {
                    c.subs.push((i + 1, count));
                    let r = c.results(l[i..i + n].iter().cloned(), &a[2]);
                    c.subs.pop();
                    out.extend(r?);
                }
                Ok(vec![Obj::List(out)])
            }),
            "NSUB" | "ENDSUB" => match self.subs.last() {
                Some(&(i, n)) => {
                    let v = if w == "NSUB" { i } else { n };
                    self.stack.push(Obj::Real(v as f64));
                    Ok(())
                }
                None => Err(Error::Undefined),
            },
            // expr 'var' start end step SEQ lists expr for each value of var.
            "SEQ" => self.with_args(5, |c, a| {
                let Obj::Name(var) = &a[1] else {
                    return Err(Error::BadType);
                };
                let (start, end, step) = (real_of(&a[2])?, real_of(&a[3])?, real_of(&a[4])?);
                if step == 0.0 || (end - start) / step > 1e6 {
                    return Err(Error::BadValue);
                }
                let mut out = vec![];
                let mut x = start;
                while (step > 0.0 && x <= end) || (step < 0.0 && x >= end) {
                    c.stack.push(Obj::Real(x));
                    let mut got = vec![];
                    c.with_locals(std::slice::from_ref(var), |c| {
                        got = c.results([], &a[0])?;
                        Ok(())
                    })?;
                    out.extend(got);
                    x = crate::obj::r12(x + step);
                }
                Ok(vec![Obj::List(out)])
            }),
            // list prog STREAM folds prog over the list: { 1 2 3 } « + » is 6.
            "STREAM" => self.with_args(2, |c, a| {
                let l = list_of(&a[0])?;
                let (first, rest) = l.split_first().ok_or(Error::BadValue)?;
                c.stack.push(first.clone());
                let base = c.stack.len() - 1;
                for x in rest {
                    c.stack.push(x.clone());
                    c.eval_obj(a[1].clone())?;
                }
                Ok(c.stack.split_off(base))
            }),
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::calc::Calc;

    fn run(src: &str) -> Vec<String> {
        let mut c = Calc::new();
        c.exec_line(src).unwrap();
        c.stack.iter().map(|o| o.show(c.fmt)).collect()
    }

    #[test]
    fn list_math() {
        assert_eq!(run("{ 1 2 3 } 10 *"), ["{ 10 20 30 }"]);
        assert_eq!(run("{ 1 2 } { 3 4 } -"), ["{ -2 -2 }"]);
        assert_eq!(run("{ 1 2 } { 3 4 } ADD"), ["{ 4 6 }"]);
        assert_eq!(run("{ 0 90 } SIN"), ["{ 0 1 }"]);
        assert_eq!(run("{ 1 2 3 4 } ΣLIST { 1 2 3 4 } ΠLIST"), ["10", "24"]);
        assert_eq!(run("{ 1 4 9 } ΔLIST"), ["{ 3 5 }"]);
    }

    #[test]
    fn list_programs() {
        assert_eq!(run("{ 1 2 3 } « SQ » DOLIST"), ["{ 1 4 9 }"]);
        assert_eq!(run("{ 1 2 } { 3 4 } 2 « * » DOLIST"), ["{ 3 8 }"]);
        assert_eq!(run("{ 1 2 3 4 } 2 « + » DOSUBS"), ["{ 3 5 7 }"]);
        assert_eq!(run("{ 5 6 7 } 1 « DROP NSUB » DOSUBS"), ["{ 1 2 3 }"]);
        assert_eq!(run("'X^2' 'X' 1 4 1 SEQ"), ["{ 1 4 9 16 }"]);
        assert_eq!(run("{ 1 2 3 4 } « * » STREAM"), ["24"]);
        assert_eq!(run("{ 3 1 2 } SORT REVLIST"), ["{ 3 2 1 }"]);
    }

    #[test]
    fn strings() {
        assert_eq!(run("\"HELLO\" \"LL\" POS"), ["3"]);
        assert_eq!(run("\"HELLO\" 2 \"IPP\" REPL"), ["\"HIPPO\""]);
        assert_eq!(run("\"A\" NUM 66 CHR"), ["65", "\"B\""]);
    }
}

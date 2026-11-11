//! Two-dimensional pictures of objects, as the equation writer and VIEW show
//! them: fractions stacked over a bar, powers raised, roots under a bar,
//! integrals with their limits, and arrays as grids.

use crate::alg::{Expr, op_word};
use crate::obj::{Fmt, Obj, fmt_real};

/// Rows of text, all the same width, and the row the baseline is on.
#[derive(Clone, Debug, PartialEq)]
pub struct Pic {
    pub rows: Vec<Vec<char>>,
    pub base: usize,
}

impl Pic {
    pub fn text(s: &str) -> Pic {
        Pic {
            rows: vec![s.chars().collect()],
            base: 0,
        }
    }

    pub fn width(&self) -> usize {
        self.rows.first().map_or(0, Vec::len)
    }

    fn height(&self) -> usize {
        self.rows.len()
    }

    pub fn lines(&self) -> Vec<String> {
        self.rows.iter().map(|r| r.iter().collect()).collect()
    }

    /// Side by side, baselines lined up.
    fn beside(parts: &[Pic]) -> Pic {
        let above = parts.iter().map(|p| p.base).max().unwrap_or(0);
        let below = parts.iter().map(|p| p.height() - p.base).max().unwrap_or(1);
        let mut rows = vec![vec![]; above + below];
        for p in parts {
            let top = above - p.base;
            for (y, row) in rows.iter_mut().enumerate() {
                match y.checked_sub(top).and_then(|i| p.rows.get(i)) {
                    Some(r) => row.extend(r),
                    None => row.extend(std::iter::repeat_n(' ', p.width())),
                }
            }
        }
        Pic { rows, base: above }
    }

    /// Centred in `w` columns.
    fn centred(&self, w: usize) -> Pic {
        let left = (w - self.width()) / 2;
        let rows = self
            .rows
            .iter()
            .map(|r| {
                let mut row = vec![' '; left];
                row.extend(r);
                row.resize(w, ' ');
                row
            })
            .collect();
        Pic {
            rows,
            base: self.base,
        }
    }

    /// One above the other, centred; `base` is the row of the result that
    /// is its baseline.
    fn stacked(parts: &[Pic], base: usize) -> Pic {
        let w = parts.iter().map(Pic::width).max().unwrap_or(0);
        let rows = parts.iter().flat_map(|p| p.centred(w).rows).collect();
        Pic { rows, base }
    }

    fn fraction(num: Pic, den: Pic) -> Pic {
        let w = num.width().max(den.width()) + 2;
        let base = num.height();
        let bar = Pic::text(&"─".repeat(w));
        Pic::stacked(&[num, bar, den], base)
    }

    /// `b` raised to the right of `a`.
    fn power(a: Pic, b: Pic) -> Pic {
        let (wa, wb, hb) = (a.width(), b.width(), b.height());
        let mut rows: Vec<Vec<char>> = b
            .rows
            .into_iter()
            .map(|r| std::iter::repeat_n(' ', wa).chain(r).collect())
            .collect();
        rows.extend(
            a.rows
                .into_iter()
                .map(|r| r.into_iter().chain(std::iter::repeat_n(' ', wb)).collect()),
        );
        Pic {
            rows,
            base: hb + a.base,
        }
    }

    fn root(a: Pic) -> Pic {
        let h = a.height();
        let mut rows = vec![
            std::iter::once(' ')
                .chain("─".repeat(a.width()).chars())
                .collect(),
        ];
        for (i, r) in a.rows.into_iter().enumerate() {
            let side = if i + 1 == h { '√' } else { '│' };
            rows.push(std::iter::once(side).chain(r).collect());
        }
        Pic {
            rows,
            base: a.base + 1,
        }
    }

    fn parens(self) -> Pic {
        let h = self.height();
        let side = |top: char, mid: char, bot: char, one: char| {
            let rows = (0..h)
                .map(|i| match i {
                    _ if h == 1 => vec![one],
                    0 => vec![top],
                    _ if i + 1 == h => vec![bot],
                    _ => vec![mid],
                })
                .collect();
            Pic {
                rows,
                base: self.base,
            }
        };
        let (l, r) = (side('⎛', '⎜', '⎝', '('), side('⎞', '⎟', '⎠', ')'));
        Pic::beside(&[l, self, r])
    }

    fn wrapped(self, paren: bool) -> Pic {
        if paren { self.parens() } else { self }
    }
}

/// How tightly an expression binds, as the writer in alg.rs counts it, but
/// with a fraction as a single block.
fn prec(e: &Expr) -> u8 {
    match e {
        Expr::Op('=', ..) => 0,
        Expr::Op('<' | '>' | '≤' | '≥' | '≠' | '≡', ..) => 1,
        Expr::Op('+' | '-', ..) => 2,
        Expr::Op('*', ..) => 3,
        Expr::Neg(_) => 4,
        Expr::Num(x) if *x < 0.0 => 4,
        Expr::Op('^', ..) => 5,
        _ => 6,
    }
}

pub fn expr(e: &Expr, fmt: Fmt) -> Pic {
    let sub = |e: &Expr| expr(e, fmt);
    match e {
        Expr::Num(x) => Pic::text(&fmt_real(*x, fmt)),
        Expr::Name(n) => Pic::text(n),
        Expr::Neg(a) => Pic::beside(&[Pic::text("-"), sub(a).wrapped(prec(a) <= 4)]),
        Expr::Op('/', a, b) => Pic::fraction(sub(a), sub(b)),
        Expr::Op('^', a, b) => Pic::power(sub(a).wrapped(prec(a) <= 5), sub(b)),
        Expr::Op(c, a, b) => {
            let p = prec(e);
            let right = prec(b) < p || (prec(b) == p && *c == '-');
            let sym = match c {
                '*' => "·".into(),
                '+' | '-' | '=' | '<' | '>' | '≤' | '≥' | '≠' | '≡' => {
                    format!(" {} ", op_word(*c))
                }
                c => c.to_string(),
            };
            Pic::beside(&[
                sub(a).wrapped(prec(a) < p),
                Pic::text(&sym),
                sub(b).wrapped(right),
            ])
        }
        Expr::Fn(f, args) if f == "√" && args.len() == 1 => Pic::root(sub(&args[0])),
        Expr::Fn(f, args) if f == "!" && args.len() == 1 => {
            Pic::beside(&[sub(&args[0]).wrapped(prec(&args[0]) < 6), Pic::text("!")])
        }
        Expr::Fn(f, args) if f == "ABS" && args.len() == 1 => {
            let a = sub(&args[0]);
            let bar = Pic {
                rows: vec![vec!['│']; a.height()],
                base: a.base,
            };
            Pic::beside(&[bar.clone(), a, bar])
        }
        // ∫(lo,hi,f,x), with the limits above and below the sign.
        Expr::Fn(f, args) if f == "∫" && args.len() == 4 => {
            let (lo, hi) = (sub(&args[0]), sub(&args[1]));
            let base = hi.height();
            let sign = Pic::stacked(&[hi, Pic::text("∫"), lo], base);
            let body = sub(&args[2]).wrapped(prec(&args[2]) <= 2);
            Pic::beside(&[sign, Pic::text(" "), body, Pic::text(" d"), sub(&args[3])])
        }
        Expr::Fn(f, args) => {
            let mut parts = vec![];
            for (i, a) in args.iter().enumerate() {
                if i > 0 {
                    parts.push(Pic::text(","));
                }
                parts.push(sub(a));
            }
            let inner = if parts.is_empty() {
                Pic::text("")
            } else {
                Pic::beside(&parts)
            };
            Pic::beside(&[Pic::text(f), inner.parens()])
        }
    }
}

/// An array as a grid, its columns right-aligned within brackets.
pub fn grid(cells: &[Vec<String>]) -> Pic {
    let cols = cells.iter().map(Vec::len).max().unwrap_or(0);
    let widths: Vec<usize> = (0..cols)
        .map(|j| {
            cells
                .iter()
                .filter_map(|r| r.get(j))
                .map(|c| c.chars().count())
                .max()
                .unwrap_or(0)
        })
        .collect();
    let rows: Vec<Pic> = cells
        .iter()
        .map(|r| {
            let s = widths
                .iter()
                .enumerate()
                .map(|(j, w)| format!("{:>w$}", r.get(j).map_or("", |c| c.as_str())))
                .collect::<Vec<_>>()
                .join("  ");
            Pic::text(&format!(" {s} "))
        })
        .collect();
    let body = Pic::stacked(&rows, (rows.len().max(1) - 1) / 2);
    let (h, base) = (body.height(), body.base);
    let side = |top: char, mid: char, bot: char, one: char| Pic {
        rows: (0..h)
            .map(|i| match i {
                _ if h == 1 => vec![one],
                0 => vec![top],
                _ if i + 1 == h => vec![bot],
                _ => vec![mid],
            })
            .collect(),
        base,
    };
    Pic::beside(&[side('⎡', '⎢', '⎣', '['), body, side('⎤', '⎥', '⎦', ']')])
}

/// The cells of an array object, written in the display format.
pub fn cells(o: &Obj, fmt: Fmt) -> Option<Vec<Vec<String>>> {
    let real = |x: &f64| fmt_real(*x, fmt);
    let cplx = |c: &(f64, f64)| Obj::Complex(c.0, c.1).show(fmt);
    Some(match o {
        Obj::Vect(v) => vec![v.iter().map(real).collect()],
        Obj::Mat(m) => m.iter().map(|r| r.iter().map(real).collect()).collect(),
        Obj::CVect(v) => vec![v.iter().map(cplx).collect()],
        Obj::CMat(m) => m.iter().map(|r| r.iter().map(cplx).collect()).collect(),
        _ => return None,
    })
}

/// The picture VIEW shows of an object.
pub fn view(o: &Obj, fmt: Fmt) -> Pic {
    match o {
        Obj::Alg(e) => expr(e, fmt),
        Obj::Tag(t, v) => Pic::beside(&[Pic::text(&format!("{t}: ")), view(v, fmt)]),
        o => match cells(o, fmt) {
            Some(c) => grid(&c),
            None => {
                let rows: Vec<Pic> = o.show(fmt).lines().map(Pic::text).collect();
                let w = rows.iter().map(Pic::width).max().unwrap_or(0);
                Pic {
                    rows: rows
                        .into_iter()
                        .map(|p| {
                            let mut r = p.rows[0].clone();
                            r.resize(w, ' ');
                            r
                        })
                        .collect(),
                    base: 0,
                }
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::view;
    use crate::calc::Calc;

    fn pic(src: &str) -> Vec<String> {
        let mut c = Calc::new();
        c.exec_line(src).unwrap();
        view(&c.stack[0], c.fmt).lines()
    }

    #[test]
    fn pictures() {
        assert_eq!(
            pic("'(X+1)/2^N'"),
            [" X + 1 ", "───────", "   N   ", "  2    "]
        );
        assert_eq!(pic("'√(A*B)+1'"), [" ───    ", "√A·B + 1"]);
        assert_eq!(pic("'X^2=1'"), [" 2    ", "X  = 1"]);
        assert_eq!(pic("[[ 1 2 ] [ 30 4 ]]"), ["⎡  1  2 ⎤", "⎣ 30  4 ⎦"]);
        assert_eq!(pic("[ 1 2 ]"), ["[ 1  2 ]"]);
    }
}

//! Drawing the display (status line, stack, command line and soft keys) and,
//! when the terminal is wide enough, the keyboard beside it.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::symbols::Marker;
use ratatui::text::{Line, Span};
use ratatui::widgets::canvas::{Canvas, Line as CLine, Points};
use ratatui::widgets::{Block, BorderType, Paragraph};

use crate::App;
use crate::grob::Grob;
use crate::keypad::{self, Act, Shift};
use crate::obj::{Coord, Fmt, fmt_real, r12};
use crate::plot::{Plot, Shape};
use crate::pretty::{self, Pic};
use crate::writer::MatrixWriter;

const LEFT_SHIFT: Color = Color::Magenta;
const RIGHT_SHIFT: Color = Color::Green;

pub fn draw(f: &mut Frame, app: &App) {
    let (display, keys) = keypad::split(f.area());
    if let Some(p) = &app.calc.plot {
        draw_plot(f, p, &app.calc.pict, app.calc.fmt, display);
    } else if let Some(m) = &app.matrix {
        draw_matrix(f, m, display);
    } else if let Some((pic, r, c)) = &app.view {
        draw_view(f, pic, *r, *c, display);
    } else {
        draw_display(f, app, display);
    }
    if let Some(area) = keys {
        draw_keys(f, app, area);
    }
}

fn draw_display(f: &mut Frame, app: &App, area: Rect) {
    let calc = &app.calc;
    // Key hints go in the bottom border, so every row inside is the calculator's.
    let hints = if app.stack_sel.is_some() {
        " STACK · ↑↓ level · ENTER echo · p pick · r roll · d rolld · Del drop · Esc back ".into()
    } else if app.eqw {
        " EQUATION · ENTER enter · Esc cancel ".into()
    } else {
        format!(
            " {} {}/{} · Tab more · ^E edit · ^Z undo · ^Q quit ",
            calc.menu.name(),
            calc.page % calc.menu.pages(calc) + 1,
            calc.menu.pages(calc)
        )
    };
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .title(" rpn48 ")
        .title_bottom(Line::from(hints).dim());
    let inner = block.inner(area);
    f.render_widget(block, area);
    let w = inner.width as usize;
    if inner.height < 4 || w < 24 {
        f.render_widget(Paragraph::new("Terminal too small"), inner);
        return;
    }
    let row = |y: u16| Rect {
        y: inner.y + y,
        height: 1,
        ..inner
    };
    let mut y = 0;

    // Shift annunciators, then the directory, then the modes.
    let shift = match app.shift {
        Some(Shift::Left) => Span::styled("◀ ", Style::new().fg(LEFT_SHIFT).bold()),
        Some(Shift::Right) => Span::styled("▶ ", Style::new().fg(RIGHT_SHIFT).bold()),
        None => Span::raw(""),
    };
    let fmt = match calc.fmt {
        Fmt::Std => "STD".into(),
        Fmt::Fix(n) => format!("FIX {n}"),
        Fmt::Sci(n) => format!("SCI {n}"),
        Fmt::Eng(n) => format!("ENG {n}"),
    };
    let halt = [
        (calc.halted > 0, "HALT  "),
        (calc.user_mode(), "USER  "),
        (app.alpha != crate::Alpha::Off, "α  "),
        (app.entry, "PRG  "),
        (calc.alarms.iter().any(|a| a.due), "ALRM  "),
    ]
    .iter()
    .filter(|(on, _)| *on)
    .map(|(_, s)| *s)
    .collect::<String>();
    // The calculator's annunciators for polar modes.
    let coord = match calc.coord {
        Coord::Rect => "",
        Coord::Cylin => "R∠Z  ",
        Coord::Sphere => "R∠∠  ",
    };
    let modes = format!(
        "{halt}{}  {coord}{}  {fmt}",
        calc.angle.name(),
        calc.base.name()
    );
    let path = format!(
        "{{ HOME {}}}",
        calc.path
            .iter()
            .map(|d| d.clone() + " ")
            .collect::<String>()
    );
    let header = spread(&path, &modes, w.saturating_sub(shift.width()));
    f.render_widget(
        Paragraph::new(Line::from(vec![shift, Span::raw(header).dim()])),
        row(y),
    );
    y += 1;

    if let Some(msg) = &app.message {
        f.render_widget(Paragraph::new(fit(msg, w)).red().bold(), row(y));
        y += 1;
    }
    // A halted program's prompt, a line or more of it.
    if let Some(p) = &app.prompt {
        for l in p.lines().take(3) {
            f.render_widget(Paragraph::new(fit(l, w)).yellow().bold(), row(y));
            y += 1;
        }
    }

    // Stack levels fill whatever the other rows leave, highest level at the top.
    let cmd_rows = u16::from(!app.line.is_empty() || app.eqw);
    let levels = inner.height.saturating_sub(y + cmd_rows + 1);
    // The equation writer's picture of the line, or what DISP put up, covers
    // the stack, one line each from the top.
    let cover = if app.eqw {
        let pic = equation_preview(app, w);
        let pad = (levels as usize).saturating_sub(pic.len()) / 2;
        let mut lines = vec![String::new(); pad];
        lines.extend(pic);
        lines
    } else {
        calc.disp.clone()
    };
    for r in 0..levels {
        if !cover.is_empty() {
            let text = cover.get(r as usize).map_or("", |s| s.as_str());
            f.render_widget(Paragraph::new(fit(text, w)), row(y));
            y += 1;
            continue;
        }
        let level = (levels - r) as usize;
        let label = format!("{level}:");
        let text = if level <= calc.stack.len() {
            calc.stack[calc.stack.len() - level].display(calc.fmt)
        } else {
            String::new()
        };
        let text = fit(&text, w.saturating_sub(label.len() + 1));
        let line = Paragraph::new(spread(&label, &text, w));
        // The level the interactive stack has picked is shown inverted.
        let line = if app.stack_sel == Some(level) {
            line.reversed()
        } else {
            line
        };
        f.render_widget(line, row(y));
        y += 1;
    }

    // The command line scrolls sideways to keep the cursor in view.
    if cmd_rows > 0 {
        let start = (app.cur + 1).saturating_sub(w);
        let shown: String = app.line[start..].iter().take(w).collect();
        f.render_widget(Paragraph::new(shown), row(y));
        f.set_cursor_position((inner.x + (app.cur - start) as u16, inner.y + y));
        y += 1;
    }

    let cell = (w - 5) / 6;
    let mut labels = vec![];
    for (i, label) in calc.menu.page(calc, calc.page).iter().enumerate() {
        if i > 0 {
            labels.push(Span::raw(" "));
        }
        let style = if label.is_empty() {
            Style::new()
        } else {
            Style::new().add_modifier(Modifier::REVERSED)
        };
        labels.push(Span::styled(center(&fit(label, cell), cell), style));
    }
    f.render_widget(Paragraph::new(Line::from(labels)), row(y));
}

/// The algebraic typed so far, drawn out and centred; while it does not
/// read (half typed), as much of it as does, with the open parentheses
/// closed.
fn equation_preview(app: &App, w: usize) -> Vec<String> {
    let line: String = app.line.iter().collect();
    let fmt = app.calc.fmt;
    let open = line
        .matches('(')
        .count()
        .saturating_sub(line.matches(')').count());
    let closed = format!("{line}{}", ")".repeat(open));
    let pic = match crate::alg::parse_alg(&closed) {
        Ok(e) => pretty::expr(&e, fmt),
        Err(_) => return vec![center(&line, w)],
    };
    let pad = w.saturating_sub(pic.width()) / 2;
    pic.lines()
        .iter()
        .map(|l| format!("{}{l}", " ".repeat(pad)))
        .collect()
}

/// VIEW: the picture, scrolled, in a box of its own.
fn draw_view(f: &mut Frame, pic: &Pic, r: usize, c: usize, area: Rect) {
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .title(" VIEW ")
        .title_bottom(Line::from(" ←→↑↓ scroll · any other key back ").dim());
    let inner = block.inner(area);
    f.render_widget(block, area);
    let lines = pic.lines();
    let (h, w) = (inner.height as usize, inner.width as usize);
    // Centred when it fits, scrolled from the corner when it does not.
    let top = if lines.len() <= h {
        (h - lines.len()) / 2
    } else {
        0
    };
    let left = w.saturating_sub(pic.width()) / 2;
    let r = r.min(lines.len().saturating_sub(h));
    let c = c.min(pic.width().saturating_sub(w));
    let text: Vec<Line> = std::iter::repeat_n(Line::raw(""), top)
        .chain(lines.iter().skip(r).map(|l| {
            let shown: String = l.chars().skip(c).take(w).collect();
            Line::raw(format!("{}{shown}", " ".repeat(left)))
        }))
        .collect();
    f.render_widget(Paragraph::new(text), inner);
}

/// The matrix writer: column and row numbers, the cells, the one under the
/// cursor inverted, and what is being typed into it at the bottom.
fn draw_matrix(f: &mut Frame, m: &MatrixWriter, area: Rect) {
    const CELL: usize = 10;
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .title(" MATRIX ")
        .title_bottom(
            Line::from(" type · ENTER cell/done · ←→↑↓ move · Del row · Esc cancel ").dim(),
        );
    let inner = block.inner(area);
    f.render_widget(block, area);
    let (h, w) = (inner.height as usize, inner.width as usize);
    if h < 4 || w < CELL + 4 {
        return;
    }
    let (cols, rows) = ((w - 4) / (CELL + 1), h - 3);
    // Scrolled to keep the cursor in view.
    let c0 = (m.col + 1).saturating_sub(cols);
    let r0 = (m.row + 1).saturating_sub(rows);
    let mut lines = vec![];
    let mut head = vec![Span::raw("    ")];
    for j in c0..c0 + cols {
        head.push(Span::raw(format!("{:>CELL$} ", j + 1)).dim());
    }
    lines.push(Line::from(head));
    for i in r0..r0 + rows {
        let mut spans = vec![Span::raw(format!("{:>3} ", i + 1)).dim()];
        for j in c0..c0 + cols {
            let here = (i, j) == (m.row, m.col);
            let text = match (here, &m.edit) {
                (true, Some(e)) => format!("{e}_"),
                _ => m
                    .cells
                    .get(i)
                    .and_then(|r| r.get(j))
                    .cloned()
                    .unwrap_or_default(),
            };
            let style = if here {
                Style::new().add_modifier(Modifier::REVERSED)
            } else {
                Style::new()
            };
            spans.push(Span::styled(format!("{:>CELL$}", fit(&text, CELL)), style));
            spans.push(Span::raw(" "));
        }
        lines.push(Line::from(spans));
    }
    let size = format!("{}×{}", m.cells.len(), m.width());
    let edit = m.edit.as_deref().map_or(String::new(), |e| format!("{e}_"));
    lines.push(Line::raw(""));
    lines.push(Line::raw(spread(&edit, &size, w)));
    f.render_widget(Paragraph::new(lines), inner);
}

// The plot fills the display, axes dimmed, with the cursor's coordinates in
// the bottom border.
fn draw_plot(f: &mut Frame, p: &Plot, pict: &[Shape], fmt: Fmt, area: Rect) {
    let num = |x: f64| fmt_real(r12(x), fmt);
    let (cx, cy) = p.cursor;
    let hints = format!(
        " ({},{}) · arrows move · ENTER coordinates · Esc back ",
        num(cx),
        num(cy)
    );
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .title(" PLOT ")
        .title_bottom(Line::from(hints).dim());
    let ((x0, x1), (y0, y1)) = (p.x, p.y);
    let cols = area.width.saturating_sub(2).max(1) as f64;
    let rows = area.height.saturating_sub(2).max(1) as f64;
    let canvas = Canvas::default()
        .block(block)
        .marker(Marker::Braille)
        .x_bounds([x0, x1])
        .y_bounds([y0, y1])
        .paint(|ctx| {
            let axis = Color::DarkGray;
            if (y0..=y1).contains(&0.0) {
                ctx.draw(&CLine::new(x0, 0.0, x1, 0.0, axis));
            }
            if (x0..=x1).contains(&0.0) {
                ctx.draw(&CLine::new(0.0, y0, 0.0, y1, axis));
            }
            ctx.print(x0, y0, Span::raw(format!("{},{}", num(x0), num(y0))).dim());
            let top = format!("{},{}", num(x1), num(y1));
            // Right-aligned: as many columns back from the edge as it is long.
            let w = (x1 - x0) * top.chars().count() as f64 / cols;
            ctx.print(x1 - w, y1, Span::raw(top).dim());
            ctx.layer();
            for s in pict {
                match s {
                    Shape::Curve(c) => {
                        for s in c.windows(2) {
                            ctx.draw(&CLine::new(s[0].0, s[0].1, s[1].0, s[1].1, Color::Cyan));
                        }
                    }
                    Shape::Points(pts) => ctx.draw(&Points {
                        coords: pts,
                        color: Color::Yellow,
                    }),
                    Shape::Bars(bars) => {
                        for &(a, b, h) in bars {
                            for (x0, y0, x1, y1) in [(a, 0.0, a, h), (a, h, b, h), (b, h, b, 0.0)] {
                                ctx.draw(&CLine::new(x0, y0, x1, y1, Color::Cyan));
                            }
                        }
                    }
                    Shape::Text(x, y, t) => ctx.print(*x, *y, Span::raw(t.clone())),
                    Shape::Bitmap(g) => ctx.draw(&Points {
                        coords: &bitmap_points(g, p, cols, rows),
                        color: Color::White,
                    }),
                }
            }
            ctx.layer();
            let (dx, dy) = ((x1 - x0) / 60.0, (y1 - y0) / 30.0);
            ctx.draw(&CLine::new(cx - dx, cy, cx + dx, cy, LEFT_SHIFT));
            ctx.draw(&CLine::new(cx, cy - dy, cx, cy + dy, LEFT_SHIFT));
        });
    f.render_widget(canvas, area);
}

/// A bitmap's pixels as braille dots in user coordinates, each pixel as
/// many dots as it covers, so that it fills its place at any size.
fn bitmap_points(g: &Grob, p: &Plot, cols: f64, rows: f64) -> Vec<(f64, f64)> {
    let ((x0, x1), (y0, y1)) = (p.x, p.y);
    let (w, h) = (g.w.max(2) as f64, g.h.max(2) as f64);
    let (pw, ph) = ((x1 - x0) / (w - 1.0), (y1 - y0) / (h - 1.0));
    let nx = (cols * 2.0 / w).ceil().max(1.0) as usize;
    let ny = (rows * 4.0 / h).ceil().max(1.0) as usize;
    let mut out = vec![];
    for j in 0..g.h {
        for i in 0..g.w {
            if !g.get(i as i64, j as i64) {
                continue;
            }
            let (cx, cy) = (x0 + i as f64 * pw, y1 - j as f64 * ph);
            for a in 0..nx {
                for b in 0..ny {
                    out.push((
                        cx + ((a as f64 + 0.5) / nx as f64 - 0.5) * pw,
                        cy + ((b as f64 + 0.5) / ny as f64 - 0.5) * ph,
                    ));
                }
            }
        }
    }
    out
}

// Each key is two lines: its shifted functions, left shift's at the left in
// purple and right shift's at the right in green, then the key itself. What
// rpn48 can't do is dimmed. The top row's keys carry the soft-key labels.
fn draw_keys(f: &mut Frame, app: &App, area: Rect) {
    f.render_widget(
        Block::bordered()
            .border_type(BorderType::Rounded)
            .title(" keys ")
            .dim(),
        area,
    );
    let soft = app.calc.menu.page(&app.calc, app.calc.page);
    let dim = |s: Style, act: Act| {
        if act == Act::Unsupported {
            s.add_modifier(Modifier::DIM)
        } else {
            s
        }
    };
    for (cell, key) in keypad::cells(area) {
        let w = cell.width as usize - 1;
        let (l, r) = (key.left.0, key.right.0);
        let shifted = Line::from(vec![
            Span::styled(l, dim(Style::new().fg(LEFT_SHIFT), key.left.1)),
            Span::raw(" ".repeat(w.saturating_sub(width(l) + width(r)))),
            Span::styled(r, dim(Style::new().fg(RIGHT_SHIFT), key.right.1)),
        ]);
        let (face, style) = match key.main {
            Act::Soft(i) => (
                soft[i].as_str(),
                Style::new().add_modifier(Modifier::REVERSED),
            ),
            Act::Shift(s) => {
                let color = if s == Shift::Left {
                    LEFT_SHIFT
                } else {
                    RIGHT_SHIFT
                };
                let style = Style::new().fg(Color::Black).bg(color);
                (
                    key.face,
                    if app.shift == Some(s) {
                        style.bold().underlined()
                    } else {
                        style
                    },
                )
            }
            act => (
                key.face,
                dim(Style::new().add_modifier(Modifier::REVERSED), act),
            ),
        };
        // The letter α types, at the right; bright while α is on.
        let face = match keypad::letter(key) {
            Some(c) => {
                let lit = if app.alpha == crate::Alpha::Off {
                    Style::new().dim()
                } else {
                    Style::new().fg(Color::Yellow).bold()
                };
                let w = w.saturating_sub(1);
                Line::from(vec![
                    Span::styled(center(&fit(face, w), w), style),
                    Span::styled(c.to_string(), lit),
                ])
            }
            None => Line::from(Span::styled(center(&fit(face, w), w), style)),
        };
        f.render_widget(Paragraph::new(vec![shifted, face]), cell);
    }
}

fn width(s: &str) -> usize {
    s.chars().count()
}

/// Cuts `s` to `w` columns, marking the cut with an ellipsis.
fn fit(s: &str, w: usize) -> String {
    if width(s) <= w {
        s.into()
    } else {
        s.chars().take(w.saturating_sub(1)).chain(['…']).collect()
    }
}

/// `left` and `right` at either end of a `w`-column row.
fn spread(left: &str, right: &str, w: usize) -> String {
    let gap = w.saturating_sub(width(left) + width(right)).max(1);
    format!("{left}{}{right}", " ".repeat(gap))
}

fn center(s: &str, w: usize) -> String {
    let pad = w.saturating_sub(width(s));
    format!("{}{s}{}", " ".repeat(pad / 2), " ".repeat(pad - pad / 2))
}

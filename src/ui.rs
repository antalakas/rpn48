//! Drawing the display: status line, stack, command line and soft keys.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Paragraph};

use crate::App;
use crate::obj::Fmt;

const HELP: &str =
    "F1–F6 keys · Tab more · F7 MTH F8 STK F9 PRG F10 VAR F11 MODES · ^Z undo · ^E edit · ^Q quit";

pub fn draw(f: &mut Frame, app: &App) {
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .title(" rpn48 ");
    let inner = block.inner(f.area());
    f.render_widget(block, f.area());
    let w = inner.width as usize;
    if inner.height < 5 || w < 24 {
        f.render_widget(Paragraph::new("Terminal too small"), inner);
        return;
    }
    let calc = &app.calc;
    let row = |y: u16| Rect {
        y: inner.y + y,
        height: 1,
        ..inner
    };
    let mut y = 0;

    let modes = match calc.fmt {
        Fmt::Std => format!("{}  STD", calc.angle.name()),
        Fmt::Fix(n) => format!("{}  FIX {n}", calc.angle.name()),
        Fmt::Sci(n) => format!("{}  SCI {n}", calc.angle.name()),
        Fmt::Eng(n) => format!("{}  ENG {n}", calc.angle.name()),
    };
    f.render_widget(Paragraph::new(spread("{ HOME }", &modes, w)).dim(), row(y));
    y += 1;

    if let Some(msg) = &app.message {
        f.render_widget(Paragraph::new(fit(msg, w)).red().bold(), row(y));
        y += 1;
    }

    // Stack levels fill whatever the other rows leave, highest level at the top.
    let cmd_rows = u16::from(!app.line.is_empty());
    let levels = inner.height.saturating_sub(y + cmd_rows + 2);
    for r in 0..levels {
        let level = (levels - r) as usize;
        let label = format!("{level}:");
        let text = if level <= calc.stack.len() {
            calc.stack[calc.stack.len() - level].show(calc.fmt)
        } else {
            String::new()
        };
        let text = fit(&text, w.saturating_sub(label.len() + 1));
        f.render_widget(Paragraph::new(spread(&label, &text, w)), row(y));
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
    let mut keys = vec![];
    for (i, label) in calc.menu.page(calc, calc.page).iter().enumerate() {
        if i > 0 {
            keys.push(Span::raw(" "));
        }
        let style = if label.is_empty() {
            Style::new()
        } else {
            Style::new().add_modifier(Modifier::REVERSED)
        };
        keys.push(Span::styled(center(&fit(label, cell), cell), style));
    }
    f.render_widget(Paragraph::new(Line::from(keys)), row(y));
    y += 1;

    let menu = format!(
        "{} {}/{}  ",
        calc.menu.name(),
        calc.page % calc.menu.pages(calc) + 1,
        calc.menu.pages(calc)
    );
    f.render_widget(Paragraph::new(fit(&(menu + HELP), w)).dim(), row(y));
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

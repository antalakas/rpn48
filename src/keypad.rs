//! The HP 48 GX keyboard, drawn beside the display and clickable.
//!
//! Each key has its own function and the two shifted ones printed above it:
//! left shift (◀, purple on the calculator) on the left, right shift (▶,
//! green) on the right. Functions rpn48 lacks are listed as `Unsupported` so
//! the keyboard still shows the whole layout.

use ratatui::layout::Rect;

use crate::menu::Menu;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Shift {
    Left,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Act {
    /// No function printed in this position.
    Nothing,
    Unsupported,
    /// Soft key n; shifted, STO into or RCL from the variable it names.
    Soft(usize),
    SoftSto(usize),
    SoftRcl(usize),
    Menu(Menu),
    /// A command, run after the command line, or typed while inside « ».
    Cmd(&'static str),
    /// An operator, with the immediate-execution rules of typing it.
    Op(char),
    /// Text to type, with the cursor left `back` characters from its end.
    Text(&'static str, usize),
    Enter,
    Backspace,
    Delete,
    Left,
    Right,
    Up,
    Down,
    Neg,
    Edit,
    Undo,
    NextPage,
    PrevPage,
    AngleToggle,
    /// Between rectangular and polar display, as POLAR does.
    PolarToggle,
    Shift(Shift),
    Cancel,
    Off,
}

pub struct Key {
    /// Printed on the key; empty for the soft keys, which show the menu.
    pub face: &'static str,
    pub main: Act,
    pub left: (&'static str, Act),
    pub right: (&'static str, Act),
    /// Width in key units: ENTER is two, everything else one.
    pub units: u16,
}

use Act::*;

const NONE: (&str, Act) = ("", Nothing);

const fn key(
    face: &'static str,
    main: Act,
    left: (&'static str, Act),
    right: (&'static str, Act),
) -> Key {
    Key {
        face,
        main,
        left,
        right,
        units: 1,
    }
}

const fn digit(face: &'static str, left: (&'static str, Act)) -> Key {
    key(face, Text(face, 0), left, NONE)
}

const fn no(label: &'static str) -> (&'static str, Act) {
    (label, Unsupported)
}

const fn soft(i: usize) -> Key {
    key("", Soft(i), ("", SoftSto(i)), ("", SoftRcl(i)))
}

pub static ROWS: [&[Key]; 9] = [
    &[soft(0), soft(1), soft(2), soft(3), soft(4), soft(5)],
    &[
        key(
            "MTH",
            Menu(Menu::Mth),
            ("RAD", AngleToggle),
            ("POLAR", PolarToggle),
        ),
        key("PRG", Menu(Menu::Prg), ("CHARS", Unsupported), NONE),
        key("CST", Menu(Menu::Cst), ("MODES", Menu(Menu::Modes)), NONE),
        key("VAR", Menu(Menu::Var), ("MEMORY", Menu(Menu::Memory)), NONE),
        key("▲", Up, ("STACK", Unsupported), NONE),
        key("NXT", NextPage, ("PREV", PrevPage), ("MENU", Unsupported)),
    ],
    &[
        key(
            "'",
            Text("''", 1),
            ("UP", Cmd("UPDIR")),
            ("HOME", Cmd("HOME")),
        ),
        key(
            "STO",
            Cmd("STO"),
            ("DEF", Cmd("DEFINE")),
            ("RCL", Cmd("RCL")),
        ),
        key("EVAL", Cmd("EVAL"), ("→NUM", Cmd("→NUM")), ("UNDO", Undo)),
        key("◀", Left, ("PICTURE", Unsupported), NONE),
        key("▼", Down, ("VIEW", Unsupported), NONE),
        key("▶", Right, ("SWAP", Cmd("SWAP")), NONE),
    ],
    &[
        key("SIN", Cmd("SIN"), ("ASIN", Cmd("ASIN")), ("∂", Cmd("∂"))),
        key("COS", Cmd("COS"), ("ACOS", Cmd("ACOS")), ("∫", Cmd("∫"))),
        key("TAN", Cmd("TAN"), ("ATAN", Cmd("ATAN")), ("Σ", Cmd("Σ"))),
        key("√x", Cmd("√"), ("x²", Cmd("SQ")), ("ˣ√y", Cmd("XROOT"))),
        key("yˣ", Op('^'), ("10ˣ", Cmd("ALOG")), ("LOG", Cmd("LOG"))),
        key("1/x", Cmd("INV"), ("eˣ", Cmd("EXP")), ("LN", Cmd("LN"))),
    ],
    &[
        Key {
            units: 2,
            ..key(
                "ENTER",
                Enter,
                ("EQUATION", Unsupported),
                ("MATRIX", Unsupported),
            )
        },
        key("+/-", Neg, ("EDIT", Edit), ("CMD", Up)),
        key(
            "EEX",
            Text("E", 0),
            ("PURG", Cmd("PURGE")),
            ("ARG", Cmd("LASTARG")),
        ),
        key("DEL", Delete, ("CLEAR", Cmd("CLEAR")), NONE),
        key("←", Backspace, ("DROP", Cmd("DROP")), NONE),
    ],
    &[
        key(
            "α",
            Unsupported,
            ("USER", Unsupported),
            ("ENTRY", Unsupported),
        ),
        digit("7", ("SOLVE", Menu(Menu::Solvr))),
        digit("8", ("PLOT", Menu(Menu::Plot))),
        digit("9", ("SYMBOLIC", Menu(Menu::Symbolic))),
        key("÷", Op('/'), ("( )", Text("()", 1)), ("#", Text("#", 0))),
    ],
    &[
        key("◀", Shift(Shift::Left), NONE, NONE),
        digit("4", ("TIME", Menu(Menu::Time))),
        digit("5", ("STAT", Menu(Menu::Stat))),
        digit("6", ("UNITS", Menu(Menu::Units))),
        key("×", Op('*'), ("[ ]", Text("[]", 1)), ("_", Text("_", 0))),
    ],
    &[
        key("▶", Shift(Shift::Right), NONE, NONE),
        digit("1", no("I/O")),
        digit("2", no("LIBRARY")),
        digit("3", no("EQ LIB")),
        key(
            "-",
            Op('-'),
            ("« »", Text("«  »", 2)),
            ("\"\"", Text("\"\"", 1)),
        ),
    ],
    &[
        key("ON", Cancel, ("CONT", Cmd("CONT")), ("OFF", Off)),
        key("0", Text("0", 0), ("=", Text("=", 0)), ("→", Text("→ ", 0))),
        key(".", Text(".", 0), (",", Text(",", 0)), ("↵", Unsupported)),
        key("SPC", Text(" ", 0), ("π", Cmd("π")), ("∠", Text("∠", 0))),
        key(
            "+",
            Op('+'),
            ("{ }", Text("{  }", 2)),
            ("::", Text("::", 1)),
        ),
    ],
];

/// Size of the keyboard panel, border included: two lines per row of keys
/// (shifted functions, then the key), and cells wide enough for "RAD POLAR".
pub const WIDTH: u16 = 62;
pub const HEIGHT: u16 = ROWS.len() as u16 * 2 + 2;

/// Narrowest display worth keeping beside the keyboard.
const MIN_DISPLAY: u16 = 30;

/// Splits the terminal into the display and, when there is room, the keyboard
/// at its top right.
pub fn split(area: Rect) -> (Rect, Option<Rect>) {
    if area.width < WIDTH + MIN_DISPLAY || area.height < HEIGHT {
        return (area, None);
    }
    let display = Rect {
        width: area.width - WIDTH,
        ..area
    };
    let keys = Rect {
        x: area.x + display.width,
        width: WIDTH,
        height: HEIGHT,
        ..area
    };
    (display, Some(keys))
}

/// Every key with the two-line cell it occupies inside the panel `area`.
pub fn cells(area: Rect) -> Vec<(Rect, &'static Key)> {
    let inner = Rect {
        x: area.x + 1,
        y: area.y + 1,
        width: area.width - 2,
        height: area.height - 2,
    };
    let w = inner.width as u32;
    let mut out = vec![];
    for (r, row) in ROWS.iter().enumerate() {
        let total: u32 = row.iter().map(|k| k.units as u32).sum();
        let mut before = 0;
        for k in row.iter() {
            let x0 = before * w / total;
            before += k.units as u32;
            let x1 = before * w / total;
            let cell = Rect {
                x: inner.x + x0 as u16,
                y: inner.y + 2 * r as u16,
                width: (x1 - x0) as u16,
                height: 2,
            };
            out.push((cell, k));
        }
    }
    out
}

/// The key under a mouse click at (`col`, `row`), if any.
pub fn hit(area: Rect, col: u16, row: u16) -> Option<&'static Key> {
    cells(area)
        .into_iter()
        .find(|(c, _)| c.contains((col, row).into()))
        .map(|(_, k)| k)
}

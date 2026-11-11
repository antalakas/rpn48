//! Soft-key menus, six labels to a page.

use crate::calc::Calc;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Menu {
    Mth,
    Stk,
    Prg,
    Var,
    Modes,
}

const MTH: &[&str] = &[
    "ABS", "SIGN", "IP", "FP", "FLOOR", "CEIL", //
    "MOD", "MIN", "MAX", "%", "%CH", "%T", //
    "!", "SQ", "√", "INV", "NEG", "^", //
    "SIN", "COS", "TAN", "ASIN", "ACOS", "ATAN", //
    "SINH", "COSH", "TANH", "ASINH", "ACOSH", "ATANH", //
    "LN", "LOG", "EXP", "ALOG", "π", "e", //
    "R→C", "C→R", "RE", "IM", "CONJ", "ARG", //
];
const STK: &[&str] = &[
    "DUP", "SWAP", "DROP", "OVER", "ROT", "PICK", //
    "ROLL", "ROLLD", "DUP2", "DROP2", "DUPN", "DROPN", //
    "DEPTH", "CLEAR", //
];
const PRG: &[&str] = &[
    "EVAL", "IFT", "IFTE", "→LIST", "LIST→", "OBJ→", //
    "SIZE", "GET", "PUT", "HEAD", "TAIL", "SUB", //
    "→STR", "STR→", "TYPE", "SAME", "==", "≠", //
    "<", ">", "≤", "≥", "AND", "OR", //
    "XOR", "NOT", "STO", "RCL", "PURGE", "VARS", //
];
const MODES: &[&str] = &[
    "STD", "FIX", "SCI", "ENG", "DEG", "RAD", "GRAD", "D→R", "R→D",
];

impl Menu {
    pub const ALL: [Menu; 5] = [Menu::Mth, Menu::Stk, Menu::Prg, Menu::Var, Menu::Modes];

    pub fn name(self) -> &'static str {
        match self {
            Menu::Mth => "MTH",
            Menu::Stk => "STK",
            Menu::Prg => "PRG",
            Menu::Var => "VAR",
            Menu::Modes => "MODES",
        }
    }

    /// The menu a command of that name switches to.
    pub fn named(w: &str) -> Option<Menu> {
        Menu::ALL.into_iter().find(|m| m.name() == w)
    }

    pub fn items(self, calc: &Calc) -> Vec<String> {
        let fixed = match self {
            Menu::Mth => MTH,
            Menu::Stk => STK,
            Menu::Prg => PRG,
            Menu::Modes => MODES,
            Menu::Var => return calc.vars.iter().map(|(n, _)| n.clone()).collect(),
        };
        fixed.iter().map(|s| s.to_string()).collect()
    }

    pub fn pages(self, calc: &Calc) -> usize {
        self.items(calc).len().div_ceil(6).max(1)
    }

    /// The six labels on `page`; empty strings where a page runs short.
    pub fn page(self, calc: &Calc, page: usize) -> Vec<String> {
        let items = self.items(calc);
        let start = (page % self.pages(calc)) * 6;
        (start..start + 6)
            .map(|i| items.get(i).cloned().unwrap_or_default())
            .collect()
    }
}

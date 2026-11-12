//! Soft-key menus, six labels to a page. A label that names another menu
//! switches to it, as MTH's VECTR or PRG's BRCH do on the calculator.

use crate::calc::Calc;
use crate::obj::Obj;
use crate::units::CATEGORIES;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Menu {
    Mth,
    Vectr,
    Matr,
    List,
    Hyp,
    Real,
    Base,
    Prob,
    Cmpl,
    Trig,
    Logs,
    Stk,
    Prg,
    Brch,
    Test,
    Type,
    Error,
    Var,
    Cst,
    Modes,
    Flag,
    Memory,
    Stat,
    Symbolic,
    Time,
    Units,
    Solvr,
    Plot,
    Chars,
    Io,
    /// A category of the UNITS menu, by its place in units::CATEGORIES.
    UnitCat(u8),
    /// A temporary custom menu, from TMENU.
    Tmenu,
}

const MTH: &[&str] = &[
    "VECTR", "MATR", "LIST", "HYP", "REAL", "BASE", //
    "PROB", "CMPL", "TRIG", "LOGS", "π", "e", //
    "MINR", "MAXR", "CONST", "TINC", "TDELTA", "ZFACTOR", //
    "DARCY", "F0λ", "SIDENS", //
];
const VECTR: &[&str] = &[
    "ABS", "DOT", "CROSS", "V→", "→V2", "→V3", //
    "NEG", "SIZE", "→ARRY", "ARRY→", "GET", "PUT", //
];
const MATR: &[&str] = &[
    "→ARRY", "ARRY→", "IDN", "CON", "TRN", "DET", //
    "INV", "RDM", "RNRM", "CNRM", "TRACE", "SIZE", //
    "ABS", "SQ", "GET", "PUT", "GETI", "PUTI", //
    "RANM", "→DIAG", "DIAG→", "RANK", "RREF", "COND", //
    "SNRM", "SRAD", "LU", "QR", "LQ", "SVD", //
    "SVL", "SCHUR", "EGV", "EGVL", "LSQ", "RSD", //
    "→COL", "COL→", "COL+", "COL-", "CSWP", "RCI", //
    "→ROW", "ROW→", "ROW+", "ROW-", "RSWP", "RCIJ", //
    "PCOEF", "PROOT", "PEVAL", "FFT", "IFFT", //
];
const LIST: &[&str] = &[
    "DOLIST", "DOSUBS", "NSUB", "ENDSUB", "STREAM", "SEQ", //
    "REVLIST", "SORT", "ΣLIST", "ΠLIST", "ΔLIST", "ADD", //
    "→LIST", "LIST→", "HEAD", "TAIL", "SUB", "REPL", //
    "SIZE", "POS", "GET", "PUT", //
];
const HYP: &[&str] = &[
    "SINH", "COSH", "TANH", "ASINH", "ACOSH", "ATANH", //
    "EXPM", "LNP1", //
];
const REAL: &[&str] = &[
    "ABS", "SIGN", "IP", "FP", "FLOOR", "CEIL", //
    "MOD", "MIN", "MAX", "%", "%CH", "%T", //
    "RND", "TRNC", "MANT", "XPON", "D→R", "R→D", //
    "!", "SQ", "√", "INV", "NEG", "XROOT", //
];
const BASE: &[&str] = &[
    "HEX", "DEC", "OCT", "BIN", "STWS", "RCWS", //
    "R→B", "B→R", "AND", "OR", "XOR", "NOT", //
    "SL", "SR", "SLB", "SRB", "ASR", "NEG", //
    "RL", "RR", "RLB", "RRB", //
];
const PROB: &[&str] = &[
    "COMB", "PERM", "!", "RAND", "RDZ", "UTPN", //
    "UTPC", "UTPT", "UTPF", "NDIST", //
];
const CMPL: &[&str] = &[
    "R→C", "C→R", "RE", "IM", "CONJ", "ARG", //
    "ABS", "SIGN", "NEG", //
];
const TRIG: &[&str] = &[
    "SIN", "COS", "TAN", "ASIN", "ACOS", "ATAN", //
    "π", "D→R", "R→D", "→HMS", "HMS→", //
];
const LOGS: &[&str] = &["LN", "LOG", "EXP", "ALOG", "LNP1", "EXPM"];
const STK: &[&str] = &[
    "DUP", "SWAP", "DROP", "OVER", "ROT", "PICK", //
    "ROLL", "ROLLD", "DUP2", "DROP2", "DUPN", "DROPN", //
    "DEPTH", "CLEAR", "LASTARG", //
];
const PRG: &[&str] = &[
    "BRCH", "TEST", "TYPE", "LIST", "ERROR", "STK", //
    "EVAL", "STO", "RCL", "PURGE", "VARS", "MEMORY", //
];
const BRCH: &[&str] = &[
    "IF", "THEN", "ELSE", "END", "IFT", "IFTE", //
    "CASE", "START", "FOR", "NEXT", "STEP", "→", //
    "DO", "UNTIL", "WHILE", "REPEAT", //
];
const TEST: &[&str] = &[
    "==", "≠", "<", ">", "≤", "≥", //
    "AND", "OR", "XOR", "NOT", "SAME", "TYPE", //
    "SF", "CF", "FS?", "FC?", "FS?C", "FC?C", //
];
const TYPE: &[&str] = &[
    "OBJ→", "→ARRY", "→LIST", "→STR", "→TAG", "DTAG", //
    "STR→", "LIST→", "ARRY→", "EQ→", "TYPE", "SIZE", //
    "NUM", "CHR", "POS", "SUB", "REPL", "HEAD", //
];
const ERROR: &[&str] = &["DOERR", "ERRN", "ERRM", "ERR0", "IFERR", "LASTARG"];
const MODES: &[&str] = &[
    "STD", "FIX", "SCI", "ENG", "FLAG", "BASE", //
    "DEG", "RAD", "GRAD", "HEX", "DEC", "BIN", //
    "RECT", "CYLIN", "SPHERE", //
];
const FLAG: &[&str] = &["SF", "CF", "FS?", "FC?", "FS?C", "FC?C"];
const MEMORY: &[&str] = &[
    "ASN", "STOKEYS", "RCLKEYS", "DELKEYS", //
    "PATH", "CRDIR", "PGDIR", "HOME", "UPDIR", "VARS", //
    "PURGE", "STO+", "STO-", "STO*", "STO/", "INCR", //
    "DECR", "CLVAR", "ORDER", "MEM", "BYTES", "NEWOB", //
    "VTYPE", "SNEG", "SINV", "SCONJ", "RCLF", "STOF", //
    "MENU", "TMENU", "RCLMENU", "NDUPN", "VERSION", //
];
const STAT: &[&str] = &[
    "Σ+",
    "Σ-",
    "CLΣ",
    "NΣ",
    "TOT",
    "MEAN", //
    "SDEV",
    "VAR",
    "MAXΣ",
    "MINΣ",
    "XCOL",
    "YCOL", //
    "LR",
    "PREDY",
    "PREDX",
    "CORR",
    "COV",
    "ΣX", //
    "ΣY",
    "ΣX2",
    "ΣY2",
    "ΣXY",
    "COLΣ",
    "STOΣ", //
    "RCLΣ",
    "LINFIT",
    "LOGFIT",
    "EXPFIT",
    "PWRFIT",
    "BESTFIT", //
    "ΣLINE",
    "PVAR",
    "PSDEV",
    "PCOV",
    "BINS",
    "SCLΣ", //
    "SCATRPLOT",
    "BARPLOT",
    "HISTPLOT",
    "UTPN",
    "UTPC",
    "UTPT", //
    "UTPF",
    "NDIST", //
];
const SYMBOLIC: &[&str] = &[
    "∂", "∫", "Σ", "ROOT", "TAYLR", "|", //
    "COLCT", "→Q", "EQ→", "EVAL", "→NUM", "=", //
    "ISOL", "QUAD", "EXPAN", "STEQ", "RCEQ", "SOLVR", //
    "↑MATCH", "↓MATCH", "SHOW", "APPLY", "QUOTE", "→Qπ", //
    "RKF", "RRK", "RKFSTEP", "RRKSTEP", "RKFERR", "RSBERR", //
    "TVMROOT", "AMORT", //
];
const PLOT: &[&str] = &[
    "DRAW",
    "AUTO",
    "XRNG",
    "YRNG",
    "INDEP",
    "RES", //
    "ERASE",
    "PVIEW",
    "GRAPH",
    "STEQ",
    "RCEQ",
    "SOLVR", //
    "FUNCTION",
    "POLAR",
    "PARAMETRIC",
    "SCATTER",
    "BAR",
    "HISTOGRAM", //
    "TRUTH",
    "CONIC",
    "DIFFEQ",
    "SLOPEFIELD",
    "WIREFRAME",
    "YSLICE", //
    "PCONTOUR",
    "GRIDMAP",
    "PARSURFACE",
    "DEPND",
    "AXES",
    "ATICK", //
    "CENTR",
    "SCALE",
    "SCALEW",
    "SCALEH",
    "PDIM",
    "PMIN", //
    "PMAX",
    "DRAX",
    "LABEL",
    "TEXT",
    "PICTURE",
    "XVOL", //
    "YVOL",
    "ZVOL",
    "XXRNG",
    "YYRNG",
    "EYEPT",
    "NUMX", //
    "NUMY",
    "LINE",
    "TLINE",
    "BOX",
    "ARC",
    "PIXON", //
    "PIXOFF",
    "PIX?",
    "PX→C",
    "C→PX", //
];
const IO: &[&str] = &[
    "SEND", "KGET", "RECV", "PR1", "PRST", "PRVAR", //
    "CR",    //
];
const TIME: &[&str] = &[
    "TIME",
    "DATE",
    "TICKS",
    "→HMS",
    "HMS→",
    "HMS+", //
    "HMS-",
    "STOALARM",
    "RCLALARM",
    "DELALARM",
    "FINDALARM",
    "ACK", //
    "ACKALL",
    "→DATE",
    "→TIME",
    "CLKADJ",
    "DATE+",
    "DDAYS", //
    "TSTR",  //
];

/// The labels of a custom menu's keys.
fn labels(calc: &Calc, l: &[Obj]) -> Vec<String> {
    l.iter()
        .map(|o| match o {
            Obj::Name(n) | Obj::Str(n) => n.clone(),
            o => o.show(calc.fmt),
        })
        .collect()
}

impl Menu {
    pub const ALL: [Menu; 30] = [
        Menu::Mth,
        Menu::Vectr,
        Menu::Matr,
        Menu::List,
        Menu::Hyp,
        Menu::Real,
        Menu::Base,
        Menu::Prob,
        Menu::Cmpl,
        Menu::Trig,
        Menu::Logs,
        Menu::Stk,
        Menu::Prg,
        Menu::Brch,
        Menu::Test,
        Menu::Type,
        Menu::Error,
        Menu::Var,
        Menu::Cst,
        Menu::Modes,
        Menu::Flag,
        Menu::Memory,
        Menu::Stat,
        Menu::Symbolic,
        Menu::Time,
        Menu::Units,
        Menu::Solvr,
        Menu::Plot,
        Menu::Chars,
        Menu::Io,
    ];

    /// The menus on F7–F11.
    pub const FKEYS: [Menu; 5] = [Menu::Mth, Menu::Stk, Menu::Prg, Menu::Var, Menu::Modes];

    pub fn name(self) -> &'static str {
        match self {
            Menu::Mth => "MTH",
            Menu::Vectr => "VECTR",
            Menu::Matr => "MATR",
            Menu::List => "LIST",
            Menu::Hyp => "HYP",
            Menu::Real => "REAL",
            Menu::Base => "BASE",
            Menu::Prob => "PROB",
            Menu::Cmpl => "CMPL",
            Menu::Trig => "TRIG",
            Menu::Logs => "LOGS",
            Menu::Stk => "STK",
            Menu::Prg => "PRG",
            Menu::Brch => "BRCH",
            Menu::Test => "TEST",
            Menu::Type => "TYPE",
            Menu::Error => "ERROR",
            Menu::Var => "VAR",
            Menu::Cst => "CST",
            Menu::Modes => "MODES",
            Menu::Flag => "FLAG",
            Menu::Memory => "MEMORY",
            Menu::Stat => "STAT",
            Menu::Symbolic => "SYMBOLIC",
            Menu::Time => "TIME",
            Menu::Units => "UNITS",
            Menu::Solvr => "SOLVR",
            Menu::Plot => "PLOT",
            Menu::Chars => "CHARS",
            Menu::Io => "I/O",
            Menu::UnitCat(i) => CATEGORIES[i as usize].0,
            Menu::Tmenu => "CST",
        }
    }

    /// The menu a command of that name switches to. TYPE and TIME are
    /// commands as well, so their menus are only reached from the keys.
    pub fn named(w: &str) -> Option<Menu> {
        Menu::ALL
            .into_iter()
            .find(|m| m.name() == w && !matches!(m, Menu::Type | Menu::Time))
    }

    /// The menu a soft key labelled `label` opens, if it is a submenu here.
    /// TYPE is the PRG TYPE menu only in PRG; elsewhere it is the command.
    pub fn sub(self, label: &str) -> Option<Menu> {
        if self == Menu::Units {
            let i = CATEGORIES.iter().position(|(n, _)| *n == label)?;
            return Some(Menu::UnitCat(i as u8));
        }
        if label == "TYPE" && self != Menu::Prg {
            return None;
        }
        Menu::ALL.into_iter().find(|m| m.name() == label)
    }

    pub fn items(self, calc: &Calc) -> Vec<String> {
        let fixed = match self {
            Menu::Mth => MTH,
            Menu::Vectr => VECTR,
            Menu::Matr => MATR,
            Menu::List => LIST,
            Menu::Hyp => HYP,
            Menu::Real => REAL,
            Menu::Base => BASE,
            Menu::Prob => PROB,
            Menu::Cmpl => CMPL,
            Menu::Trig => TRIG,
            Menu::Logs => LOGS,
            Menu::Stk => STK,
            Menu::Prg => PRG,
            Menu::Brch => BRCH,
            Menu::Test => TEST,
            Menu::Type => TYPE,
            Menu::Error => ERROR,
            Menu::Modes => MODES,
            Menu::Flag => FLAG,
            Menu::Memory => MEMORY,
            Menu::Stat => STAT,
            Menu::Symbolic => SYMBOLIC,
            Menu::Time => TIME,
            Menu::Plot => PLOT,
            Menu::Chars => crate::keys::CHARS,
            Menu::Io => IO,
            Menu::Units => &CATEGORIES.iter().map(|(n, _)| *n).collect::<Vec<_>>(),
            Menu::UnitCat(i) => CATEGORIES[i as usize].1,
            Menu::Solvr => return calc.solvr_items(),
            Menu::Var => return calc.cur_vars().iter().map(|(n, _)| n.clone()).collect(),
            // The custom menu lists what the variable CST holds.
            Menu::Cst => {
                return match calc.global("CST") {
                    Some(Obj::List(l)) => labels(calc, l),
                    _ => vec![],
                };
            }
            Menu::Tmenu => return labels(calc, &calc.tmenu),
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

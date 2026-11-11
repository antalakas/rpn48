# rpn48

An RPN calculator for the terminal that works like the HP 48 GX: a stack of
objects, soft-key menus, and RPL programs. It is written from scratch rather
than emulating the calculator, so it needs no ROM.

```
╭ rpn48 ───────────────────────────────────────────╮
│{ HOME }                                  DEG  STD│
│6:                                                │
│5:                                                │
│4:                                                │
│3:                                              .5│
│2:                                           (0,2)│
│1:                             « → a b « a b * » »│
│  ABS    SIGN     IP      FP     FLOOR   CEIL     │
╰ MTH 1/7 · Tab more · ^E edit · ^Z undo · ^Q quit ╯
```

## Running

```sh
nix run github:antalakas/rpn48      # or: cargo run --release
rpn48 '2 3 + 4 *'                   # one-shot: prints the stack, here 20
```

The stack, variables and modes are saved after every command to
`$XDG_DATA_HOME/rpn48/memory` (default `~/.local/share/rpn48/memory`), so they
are still there next time, like the calculator's continuous memory.

## Keys

| Key | Empty command line | While typing |
| --- | --- | --- |
| Enter | DUP | enter the line |
| Backspace | DROP | delete a character |
| → | SWAP | move the cursor |
| `+ * / ^` | run at once | run at once after a number, otherwise type it |
| `-` | type it (starts a negative number) | run at once after a number |
| F1–F6 | run the soft key | run it, or type its name inside `« »` |
| Tab / Shift-Tab | next / previous menu page | |
| F7–F11 | MTH, STK, PRG, VAR, MODES menus | |
| Ctrl-E | edit level 1 | |
| Ctrl-Z | undo (100 levels) | cancel the edit |
| Esc | dismiss an error, stop a running program | clear the line |
| ↑ / ↓ | command history | |
| Ctrl-Q | quit | |

## The keyboard

In a terminal at least 92 columns wide and 20 rows tall, the HP 48 GX
keyboard is drawn beside the display, and clicking a key presses it. Each key
shows its left-shifted function at the top left, in purple, and its
right-shifted one at the top right, in green. Click ◀ or ▶ (the shift keys
in the bottom-left column), then a key, for the shifted function: ◀ SIN is
ASIN, ▶ EVAL is UNDO, ◀ - types `« »`. The top row of keys is the soft keys,
labelled with the current menu; in the VAR menu, ◀ and ▶ on a soft key
store into and recall that variable. Functions rpn48 doesn't have (PLOT,
SOLVE, UNITS and the rest) are dimmed.

The keyboard takes the mouse, so to select text on the screen, hold Shift
while dragging (in most terminals).

Characters the keyboard lacks have ASCII spellings: `<< >>` for `« »`, `->`
for `→` (so `R->C`, `->LIST`), `SQRT` for `√`, `PI` for `π`, and `<=`, `>=`,
`!=` for `≤`, `≥`, `≠`. Commands also answer to lowercase (`sin`) unless a
variable has that name.

## What's there

- **Objects:** reals (12 significant digits, as on the calculator, so
  `.1 .2 + .3 ==` is true), complex numbers `(1,2)`, strings, names `'X'`,
  lists `{ 1 2 }` and programs `« … »`.
- **Math:** `+ - * / ^ NEG INV SQ √ ABS SIGN IP FP FLOOR CEIL MOD MIN MAX % %CH %T !`,
  trigonometry and its inverses in DEG/RAD/GRAD, hyperbolics,
  `LN LOG EXP ALOG LNP1 EXPM`, `π e`, and complex `R→C C→R RE IM CONJ ARG`.
  Square roots, logs and fractional powers of negative numbers give complex
  results.
- **Stack:** `DUP SWAP DROP OVER ROT PICK ROLL ROLLD DUP2 DROP2 DUPN DROPN DEPTH CLEAR`.
- **Programs:** `IF THEN ELSE END`, `START NEXT/STEP`, `FOR i NEXT/STEP`,
  `WHILE REPEAT END`, `DO UNTIL END`, local variables `→ a b « … »`,
  `EVAL IFT IFTE`, and comparisons and logic `== ≠ < > ≤ ≥ SAME AND OR XOR NOT`.
- **Variables:** `STO RCL PURGE VARS`; the VAR menu runs or recalls them.
- **Strings and lists:** `+` joins, `SIZE GET PUT HEAD TAIL SUB →LIST LIST→ OBJ→ →STR STR→ TYPE`.
- **Display:** `STD`, `n FIX`, `n SCI`, `n ENG`.

As on the calculator, a failed command puts its arguments back and reports
`SIN Error: Bad Argument Type` and the like.

For example, a recursive factorial:

```
« IF DUP 1 > THEN DUP 1 - FAC * END » 'FAC' STO
20 FAC          → 2.43290200818E18
```

## Not there (yet)

Algebraic objects (`'X^2+1'`) and symbolic math, units, matrices and arrays,
binary integers, directories, plotting, the equation solver, and the
calculator's I/O.

## License

MIT

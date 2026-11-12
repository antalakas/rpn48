# rpn48

An RPN calculator for the terminal that works like the HP 48 GX: a stack of
objects, soft-key menus, and RPL programs. It is written from scratch rather
than emulating the calculator, so it needs no ROM.

```
╭ rpn48 ───────────────────────────────────────────╮
│{ HOME }                             DEG  DEC  STD│
│6:                                                │
│5:                                                │
│4:                                                │
│3:                                              .5│
│2:                                           (0,2)│
│1:                             « → a b « a b * » »│
│  ABS    SIGN     IP      FP     FLOOR   CEIL     │
╰ REAL 1/4 · Tab more · ^E edit · ^Z undo · ^Q quit╯
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
| Shift- / Alt-F1–F6 | the soft key left- / right-shifted (◀ / ▶) | |
| Tab / Shift-Tab | next / previous menu page | |
| F7–F11 | MTH, STK, PRG, VAR, MODES menus | |
| Ctrl-E | edit level 1 | |
| Ctrl-S | the interactive stack (↑↓ pick a level) | |
| Ctrl-V | VIEW level 1 (an array opens in the matrix writer) | |
| Ctrl-W | the equation writer | |
| Ctrl-T | the matrix writer | |
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
store into and recall that variable. The few functions rpn48 doesn't have
(libraries, the α keyboard) are dimmed.

The keyboard takes the mouse, so to select text on the screen, hold Shift
while dragging (in most terminals).

Characters the keyboard lacks have ASCII spellings: `<< >>` for `« »`, `->`
for `→` (so `R->C`, `->LIST`), `SQRT` for `√`, `PI` for `π`, and `<=`, `>=`,
`!=` for `≤`, `≥`, `≠`. Commands also answer to lowercase (`sin`) unless a
variable has that name.

## What's there

- **Objects:** reals (12 significant digits, as on the calculator, so
  `.1 .2 + .3 ==` is true), complex numbers `(1,2)`, strings, names `'X'`,
  algebraics `'X^2+1'`, binary integers `# FFh`, vectors `[ 1 2 3 ]` and
  matrices `[[ 1 2 ] [ 3 4 ]]`, lists `{ 1 2 }`, programs `« … »`, tagged
  objects `:A:5`, unit objects `9.8_m/s^2`, directories and graphics
  objects (see below). Complex numbers and vectors can be typed in polar
  form, `(1,∠90)` and `[ 2 ∠45 ]`.
- **Math:** `+ - * / ^ NEG INV SQ √ XROOT ABS SIGN IP FP FLOOR CEIL MOD MIN MAX % %CH %T !`,
  `RND TRNC MANT XPON`, trigonometry and its inverses in DEG/RAD/GRAD,
  hyperbolics, `LN LOG EXP ALOG LNP1 EXPM`, `π e`, complex
  `R→C C→R RE IM CONJ ARG`, hours-minutes-seconds `→HMS HMS→ HMS+ HMS-`, and
  `COMB PERM RAND RDZ`. Square roots, logs and fractional powers of negative
  numbers give complex results.
- **Algebraics:** math on names and algebraics builds expressions (`'X' 2 +`
  is `'X+2'`), `EVAL` substitutes variables, `→NUM` insists on a number.
  `∂` differentiates, `∫` integrates numerically, `Σ` sums, `ROOT` finds
  roots, `|` substitutes, `TAYLR` gives Taylor polynomials, `COLCT` folds
  numbers, `→Q` turns a real into a fraction, `EQ→` splits an equation, and
  `DEFINE` makes user-defined functions: `'F(X)=X^2+1' DEFINE 'F(3)' EVAL` is 10.
  With a symbolic limit `∫` integrates symbolically: `0 'Y' 'X^2' 'X' ∫`
  is `'Y^3/3'`.
  `ISOL` solves for a name that appears once (`'2*X+3=7' 'X' ISOL` is
  `'X=2'`), `QUAD` solves quadratics with `s1` for the sign
  (`'X^2-5*X+6' 'X' QUAD` is `'X=(5+s1)/2'`), `EXPAN` multiplies out, and
  `COLCT` collects like terms (`'X*X+X+X+1'` becomes `'X^2+2*X+1'`).
- **Arrays:** `+ - *` and `/` (which solves `B A /` for A·X = B),
  `DET INV TRN DOT CROSS ABS IDN CON RDM RNRM CNRM TRACE →ARRY ARRY→ V→ →V2 →V3`,
  and `GET PUT GETI PUTI SIZE`, on real and complex arrays (`[ (1,2) 3 ]`; `R→C` and
  `C→R` build and split them, `CONJ RE IM` take them apart). On real
  matrices, rows and columns: `→COL COL→ COL+ COL- CSWP →ROW ROW→ ROW+ ROW-
  RSWP RCI RCIJ →DIAG DIAG→ RANM`; the factorizations `LU QR LQ SVD SVL
  SCHUR`, eigenvalues and eigenvectors `EGVL EGV` (complex where they are),
  `RANK RREF COND SNRM SRAD`, least squares `LSQ` and residuals `RSD`.
  Polynomials as coefficient vectors, highest power first: `PROOT PCOEF
  PEVAL` (`[ 1 -5 6 ] PROOT` is `[ 2 3 ]`). `FFT IFFT` on vectors and
  matrices whose sides are powers of 2.
- **Solver:** `STEQ` stores an equation in `EQ`, and the SOLVR menu (◀ 7)
  has a key for each of its variables: a value then the key stores it, ◀
  (or Shift-F*n*) solves for it, ▶ (or Alt-F*n*) recalls it, and `EXPR=`
  shows both sides. With a list of equations in `EQ`, `MINIT` sets up the
  multiple-equation solver and `MSOLVR` its menu, which works the same way;
  `MROOT` solves for a variable (or `'ALL'`) through whichever equations
  have one unknown left, `MUSER` and `MCALC` mark variables as given or to
  be found, and `MITM` sets the menu's title and keys.
- **Differential equations:** `{ 'X' 'Y' 'f' } tol xfinal RKF` takes the
  variables X and Y from their values to x = xfinal along y' = f(x,y) (y
  may be a vector), by Runge-Kutta-Fehlberg; `RKFSTEP` takes one step and
  `RKFERR` estimates a step's error. `RRK RRKSTEP RSBERR` take the
  stiff solver's arguments, with ∂f/∂x and ∂f/∂y, but step the same way.
- **Money:** `TVMROOT` solves for one of `N I%YR PV PMT FV` from the
  others (`PYR` payments a year, 12 if unset; flag -14 for payments at the
  start of each period), and `n AMORT` gives the principal, interest and
  balance after n payments.
- **Plotting:** `DRAW` plots `EQ` (or a list of expressions) as a
  `FUNCTION`, in `POLAR` or `PARAMETRIC` form, or as a `TRUTH` plot of
  where a comparison holds, or `ΣDAT` as a `SCATTER`, `BAR` or `HISTOGRAM`
  plot, over the ranges in `PPAR` set by `XRNG YRNG INDEP RES AUTO`. What
  is drawn stays in PICT, where `LINE TLINE BOX ARC PIXON PIXOFF PIX?` add
  to it, `DRAX` and `LABEL` draw axes and their labels, `ERASE` clears it,
  `PVIEW GRAPH PICTURE` show it, `TEXT` goes back to the stack, and `PX→C C→PX`
  convert between pixels and coordinates. The arrows move a cursor and
  ENTER puts its coordinates on the stack. The PLOT menu is ◀ 8, and ◀ with the ◀ arrow
  shows PICT. `AXES ATICK DEPND CENTR SCALE SCALEW SCALEH PDIM PMIN PMAX`
  set the rest of PPAR. The other plot types: `CONIC` (where an equation
  in X and Y holds), `DIFFEQ` (y' = EQ from the dependent variable's
  value), `SLOPEFIELD`, `PCONTOUR`, `GRIDMAP` (a complex function's image
  of a grid), and in perspective `WIREFRAME YSLICE PARSURFACE`, over the
  view volume in VPAR that `XVOL YVOL ZVOL XXRNG YYRNG EYEPT NUMX NUMY` set.
- **Graphics objects:** grobs, written `GROB w h data` as on the
  calculator (rows of pixels in hex, the leftmost pixel in each digit's
  lowest bit) and shown as `Graphic w × h`. `obj n →GROB` draws an object
  in the small, medium or large font (`0` draws algebraics as the equation
  writer does), `#w #h BLANK` makes an empty one, `GOR GXOR REPL` put one
  grob on another at `{ #x #y }`, `SUB` cuts out part of one, `SIZE` gives
  its size, `+` ORs two and `NEG` inverts one. `PICT` names the plot's
  picture in those commands and in `RCL STO PURGE`, so `PICT RCL` is what
  is drawn as a grob and `grob PICT STO` replaces it; `#w #h PDIM` sets its
  size. `LCD→` is the display as a grob, `→LCD` shows one, and `ANIMATE`
  shows several in turn. The PRG menu has GROB and PICT menus. In VIEW,
  `→LCD` and the plot view a grob is drawn in braille.
- **Units:** the calculator's unit table (length, area, volume, time,
  speed, mass, force, energy, power, pressure, temperature, electricity,
  angles, light, radiation, viscosity) with SI prefixes (`km`, `MeV`, `μs`).
  `+ - * / ^ INV SQ √` and comparisons work on units, converting where
  needed (`1_m 1_ft +` is `1.3048_m`), and `CONVERT UBASE UVAL UFACT →UNIT`.
  Temperatures convert as scales: `100_°C 1_°F CONVERT` is `212_°F`. An
  angle with units is taken as such by `SIN COS TAN` in any angle mode. In
  the UNITS menu a unit's key gives a number those units; shifted, it
  converts to them or divides by them.
- **Binary integers:** `HEX DEC OCT BIN`, word size `STWS RCWS`,
  `R→B B→R`, bitwise `AND OR XOR NOT`, shifts and rotations
  `SL SR SLB SRB ASR RL RR RLB RRB`.
- **Lists:** math applies to each element as on the 48G (`{ 1 2 } 10 *` is
  `{ 10 20 }`, `{ 0 90 } SIN` is `{ 0 1 }`), and
  `DOLIST DOSUBS NSUB ENDSUB SEQ STREAM REVLIST SORT ΣLIST ΠLIST ΔLIST ADD POS`.
- **Strings and lists:** `+` joins, `SIZE GET PUT HEAD TAIL SUB REPL POS NUM CHR →LIST LIST→ OBJ→ →STR STR→ TYPE`.
- **Stack:** `DUP SWAP DROP OVER ROT PICK ROLL ROLLD DUP2 DROP2 DUPN DROPN DEPTH CLEAR LASTARG`.
- **Programs:** `IF THEN ELSE END`, `CASE … THEN … END … END`,
  `IFERR THEN ELSE END` with `ERRN ERRM ERR0 DOERR`, `START NEXT/STEP`,
  `FOR i NEXT/STEP`, `WHILE REPEAT END`, `DO UNTIL END`, local variables
  `→ a b « … »` and `→ x 'x^2'`, `EVAL IFT IFTE`, comparisons and logic
  `== ≠ < > ≤ ≥ SAME AND OR XOR NOT`, tags `→TAG DTAG`, and flags
  `SF CF FS? FC? FS?C FC?C`. `HALT` stops a program with the calculator
  usable until `CONT` (◀ ON) or `KILL`. The debugger, in PRG RUN:
  `prog DBUG` starts a program halted before its first step, `SST` runs
  one step (a subprogram as a whole) and halts again, and `SST↓` steps
  into subprograms; the step to come is shown above the stack, as RUN's
  `NEXT` would show it. These also step on from a `HALT`. `PROMPT` halts
  with a message, `INPUT` asks for a line, `MSGBOX` waits for a key, and
  `DISP CLLCD FREEZE WAIT` show things while it runs. `KEY` and `0 WAIT` read a key as its
  code, row-column-plane like the calculator's (`5` is 73.1). Without the display (`rpn48 '…'`),
  prompts go to stderr and input comes from stdin.
- **Variables and directories:** `STO RCL PURGE VARS`, `STO+ STO- STO* STO/ INCR DECR`,
  `CRDIR PGDIR UPDIR HOME PATH`; names are looked up in the current
  directory and then those above it. The VAR menu runs or recalls variables
  and enters directories, and CST shows the list in the variable `CST`.
- **Statistics:** on the matrix `ΣDAT`, `Σ+ Σ- CLΣ STOΣ RCLΣ NΣ TOT MEAN SDEV VAR PSDEV PVAR MAXΣ MINΣ BINS`,
  and on its `XCOL`/`YCOL` columns (`COLΣ` sets both) `LR PREDY PREDX CORR COV PCOV ΣX ΣY ΣX2 ΣY2 ΣXY`.
  `LINFIT LOGFIT EXPFIT PWRFIT` choose the curve `LR` fits, `BESTFIT` the
  one that fits best, and `ΣLINE` gives it as an expression. `SCATRPLOT
  BARPLOT HISTPLOT` draw the data, `SCLΣ` scales the plot to it. The
  distributions: upper-tail probabilities `UTPN UTPC UTPT UTPF` and the
  normal density `NDIST`.
- **Time and alarms:** `TIME DATE TICKS`, `→TIME →DATE CLKADJ` to set
  rpn48's clock (it runs ahead of or behind the system's, which is left
  alone), `DATE+ DDAYS TSTR`, and alarms with `STOALARM
  RCLALARM DELALARM FINDALARM ACK ACKALL`: `{ 11.122026 9.3 "Call" }
  STOALARM` shows "Call" at 9:30 on 12 November 2026 and lights ALRM until
  acknowledged, and an alarm with a program runs it. A repeat interval, in
  ticks, makes it come round again. They need rpn48 to be running.
- **Keyboards:** `ASN STOKEYS RCLKEYS DELKEYS` assign objects to keys by
  code, which run in USER mode (◀ α, or `-62 SF`); ENTRY (▶ α) types
  commands rather than running them; CHARS (◀ PRG) types the characters the
  keyboard lacks.
- **Equation and matrix writers:** EQUATION (◀ ENTER) types an algebraic
  with a live picture of it, fractions stacked and powers raised; MATRIX (▶
  ENTER) is a grid of cells to enter an array into (the first row sets the
  width, ▼ starts a new row); VIEW (◀ ▼) draws level 1 out, or edits an
  array in the matrix writer.
- **I/O:** with files for cable and paper, in `$XDG_DATA_HOME/rpn48`:
  `SEND` (and `XSEND XPUT`) writes variables to `transfer/`, `KGET` (and
  `XRECV XGET`) reads them back by name and `RECV` reads everything there;
  `ARCHIVE` backs up HOME there and `RESTORE` puts it back; `XMIT SRECV
  BUFLEN` use a file there as a looped-back serial line, and `BAUD PARITY
  CKSM TRANSIO` set IOPAR. `PR1 PRST PRSTC PRLCD PRVAR CR` append to `printer`.
- **Constants and EQ LIB:** `'c' CONST` gives a constant of the GX's
  constants library with its units (without, with flag -61 set), and
  `CONLIB` is a menu of them. EQ LIB (▶ 3) has it with the
  multiple-equation solver and the utilities `ZFACTOR DARCY F0λ SIDENS
  TDELTA TINC`. `MINR MAXR` are the smallest and largest reals rpn48 holds
  (an f64's, not the calculator's 1E-499 and 9.99999999999E499).
- **Patterns and the rest of the symbolics:** `↑MATCH ↓MATCH` rewrite an
  expression by a pattern with `&`-wildcards and an optional condition
  (`'SIN(A)^2+COS(A)^2' { 'SIN(&X)^2+COS(&X)^2' 1 } ↓MATCH` is `1`),
  `APPLY QUOTE SHOW →Qπ`.
- **Dialogs and editing:** `CHOOSE` and `INFORM` (with `NOVAL` for empty
  fields) ask through the prompt line, a choice by number and a field at a
  time; `VISIT` edits a variable.
- **Memory and menus:** `CLVAR CLUSR ORDER VTYPE MEM BYTES NEWOB NDUPN
  SNEG SINV SCONJ`, the flags as binary integers `RCLF STOF`, `MENU TMENU
  RCLMENU` (CST is menu 1, VAR 2 and MTH 3; the rest are numbered in
  rpn48's own order), `VERSION`, and `LAST` for `LASTARG`.
- **Display:** `STD`, `n FIX`, `n SCI`, `n ENG`, and `RECT CYLIN SPHERE`
  (▶ MTH toggles polar), which show complex numbers as `(r,∠θ)` and 2- and
  3-element vectors as `[ r ∠θ ]`, `[ r ∠θ z ]` or `[ r ∠θ ∠φ ]`.

The menus follow the calculator's: MTH leads to VECTR, MATR, LIST, HYP,
REAL, BASE, PROB and CMPL, PRG to BRCH, TEST, TYPE, LIST and ERROR, and the
keyboard's shifted keys open SOLVE, PLOT, SYMBOLIC, TIME, STAT, UNITS and
MEMORY.

As on the calculator, a failed command puts its arguments back and reports
`SIN Error: Bad Argument Type` and the like.

For example, a recursive factorial:

```
« IF DUP 1 > THEN DUP 1 - FAC * END » 'FAC' STO
20 FAC          → 2.43290200818E18
```

## Not there

Libraries and the equation library's 300 equations: the calculator's
libraries are ROM and machine-code extensions, which have no equivalent
here (programs in directories do the same job), so `ATTACH DETACH LIBS
LIBEVAL SYSEVAL` are not there either. Also the α keyboard (the terminal's
own keyboard types letters), plug-in cards and their ports (`FREE MERGE
PVARS PINIT`), and Kermit's server and packets (`SERVER XSERV PKT
FINISH`), for which files stand in.

## License

MIT

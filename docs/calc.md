# Live calculations with units

> **Status:** stages 1–5 done (worksheet with one `=`, symbolic linear/quadratic/`abs` solving and dimensional numeric fallback, Σ/∏, vectors, calculated tables, lab statistics and linear regression, number format, working units, checks, `°C`/`°F`, user functions, Variables pane with unit converter); systems next · **Last updated:** 2026-11-14 · **Change:** Phase 2 math engine & Variables pane (`M4`–`M6`): full SI prefix table (`P`, `T`, `da`, `d`, `p`, `f`) and electromagnetism/photometry/chemistry/topography units; STEM functions (`cbrt`, `root`, `log2`, 2-arg `log`, `atan2`, hyperbolic, `sign`, `mod`), combinatorics (`gcd`/`mcd`, `lcm`/`mcm`, `fact`/`n!`, `binom`) and lab statistics (`median`, `var`, `varp`, `stdev`/`std`, `stdevp`, `slope`, `intercept`, `r2`) plus full Unicode superscripts; symbolic degree 1–2 polynomial solver with dimensional Δ check, domain constraints (`t > 0`) and `|u(x)| = c` branching, algebraic dimension inference in numeric fallback, and the **Variables** pane (`view.variablesPane`, Math › Calculation › Variables)

Strix documents hold live engineering calculations, in the spirit of SMath Studio and Mathcad:
you write parameters with units, Strix remembers them, and later equations reuse them, solve
for what is unknown and show results with units, recomputed whenever the inputs change. This
is a Strix feature beyond Word parity; the document stays a normal OOXML package (`.xmath`), so
Word shows the last computed values as ordinary equations and tables.

## Model

A **calc region** is an equation (`InlineObject::Equation`) marked as calculated: `Math::calc`,
saved in the `.xmath` package as `strix:calc="1"` on its `m:oMath` (a Strix namespace listed in
`mc:Ignorable`). The **Calculate** button (Equation tab, Math tab, or `=` beside the equation's
frame) or `equation.calc` turns it on and off. New equations are calculated by default; the
**Always Calculate** preference (`calc.newEquations`) changes that.

A **calculated table** is an ordinary Word table with `TableProps::calc` on (Table Layout ›
Data › Calculated Table, Math › Tables, `table.calc`), saved as `strix:calc="1"` on `w:tblPr`.
Nothing else about the table changes: style, borders and text stay the user's.

Regions and calculated tables are evaluated in reading order (body top to bottom, a paragraph
left to right, tables cell by cell; a calculated table as a whole), like a worksheet. Only
calculated items see each other.

**Recalculation is automatic**: after every command that changes what the calculations read (a
region's text, a calculated table's cells), the document is recalculated, from the engine, so
the CLI and MCP get it too. While an equation is being edited, or the caret is in a calculated
table, it waits until the caret leaves, like a spreadsheet: what is being typed is never
rewritten under the caret. F9 and Update Fields recalculate on demand.

Strix saves calculations live in `.xmath`/`.xmathm` and in Word documents (`.docx`…), which
carry them through Word ([word-round-trip.md](word-round-trip.md)); plus static PDF, PNG, LaTeX,
HTML and Markdown exports. ODT, RTF, plain text and Word XML would lose the regions, so a
document with them isn't saved to those.

## One `=` for everything

Every statement is written with `=`; Strix works out what it means from what is known so far.
A name is **known** when an earlier item defined it; constants (`π`, `e`, `g`) and units (`m`,
`kN`) are known too, except that a name **alone on one side** is unknown whenever it is not a
parameter (`m = 3 kg` defines `m` although `m` is also the metre).

| Written | Unknowns | Meaning | Shown |
|---|---|---|---|
| `L = 4.5 m` | `L` | define | as typed |
| `M = q·L²/8` | `M` | define from earlier parameters | as typed |
| `M = q·L²/8 =` | `M` | define and show | `M = q·L²/8 = 30.375 kN·m` |
| `A = 3 m · 5 m =` | `A` | define and show, from data | `A = 3 m · 5 m = 15 m²` |
| `M =`, `M = kN·m` | none | show, in the unit asked for | `M = 30.375 kN·m` |
| `F = m·a` | `a` | **solve** for the one unknown and define it | `F = m·a ⇒ a = 2.5 m/s²` (or under it: Result Layout) |
| `x² + x = 6` | `x` | **solve** symbolically (degree 1–2 polynomial) | `x² + x = 6 ⇒ x = 2` (or both roots / domain-filtered root) |
| `x³ + x = 10` | `x` | solve numerically (higher degree / transcendental) | `x³ + x = 10 ⇒ x = 2 (numerical solution)` |
| `a + b = c` | none | **check**: both sides must agree | as typed, or an error |
| `σ ≤ f_yd` | none | check a comparison (`≤ ≥ < > ≠`) | `σ ≤ f_yd ⇒ 5.4 MPa ≤ 355 MPa ✓` |
| `E = mc²` | `E`, `mc` | more than one unknown: an ordinary equation | as typed (reported) |

- A number (or a vector) after `=` with every name before it known is a result to refresh:
  `M = 99 kN·m` becomes `M = 30.375 kN·m`. Units alone after `=` ask for a display unit.
- `:=` still defines (and redefines) explicitly: `x := 2 m`.
- **A definition worked out at once**: `A = 3 m · 5 m =` (or `A := …`) defines `A` and writes
  its value after the last `=`; read back, a stale result is refreshed and a unit asked for is
  kept (`A = 3 m·5 m = cm²` → `150000 cm²`). Only one bare `=` (or a unit, or a stale result)
  may follow: `A = 3 m = 4 m =` and `A = 3 m = =` are errors left as typed; `A = =` and
  `= 3 m =` are not calculations. A product of quantities is never a working unit: only a unit
  as written (`kN·m`, `m/s²`, `1/min`) after `=` or a number sets one, not the `m·5 m` of
  `3 m·5 m`.
- Solving: the unknown is **isolated** symbolically when it appears once (inverting `+ − · /`,
  powers, `√`, `|u(x)| = c` via `±` branching, `exp ln log`, `sin cos tan` and their inverses,
  principal values), or solved **symbolically as a degree 1–2 polynomial** (`poly_coeffs`) when it
  appears multiple times (`2·x + 3 = x + 7`, `s = v_0·t + 1/2·a·t²`), checking dimensional
  homogeneity of `B² − 4·A·C`, reporting `Δ < 0` explicitly, generating quadratic-formula steps,
  computing both roots `x₁, x₂` and filtering by physical domain constraints (`t > 0`, standalone
  or after `;`). Otherwise it is found **numerically**: its dimension is inferred algebraically
  from powers `x^k` and inverse ratios across the equation (plus known quantities), every sign
  change on a logarithmic grid from 10⁻¹² to 10¹² (both signs) is refined by bisection (a sign
  change across a pole is dropped), and secant steps catch roots that touch zero. The **smallest
  non-negative root** wins, else the negative root closest to zero. No root: "no value of x solves
  the equation".
- After `⇒` Strix writes the solved value (with Show Steps, the algebra first); on the next
  calculation that tail is replaced, and the unit of its last value, if changed, is kept
  (`⇒ a = mm/s²`).
- An error never stops later items: the region shows `= ⚠` (or `⇒ x = ⚠`) and the message is
  listed by `calc.recalculate`. Names depending on a failed definition are undefined.
- Identifiers: letters (Latin, Greek), digits, `_` subscripts (`f_ck`, `σ_max`). A defined
  parameter wins over a unit of the same spelling, **except right after a number**: there unit
  names are units (ISO 80000: a quantity is a number times a unit), so with a mass `m = 400 kg`,
  `L = 4.5 m` is still metres and `q = 12 kN/m` still per metre. The units after a number run on
  through `·`, `/`, spaces and powers (`9.81 m/s²`, `30 kN·m`) and stop at the first name that
  is not a unit. To multiply by a parameter spelled like a unit, put it first or use brackets:
  `L·2 kN`, `(2 kN)·L`, `2·m`. The same holds after numbers alone — a vector of numbers or a
  power of numbers (`[4; 5; 6] m`, `10^3 m`) — and after a written quantity and `/`
  (`(12 kN)/m`, the linear text of a stacked `12 kN` over `m`, is per metre).
- After `=`, units alone ask for a display unit, except a **defined parameter alone of the
  other side's kind**: with `A = 0.5 m²`, `b·h = A` checks against `A` (not "in amperes"), and
  with `N = 12 kN`, `F_1 + F_2 = N` is a check. A parameter of another kind leaves the request
  alone (`m := 3`, `1 km = m` shows metres).

## Sums, products, ranges and vectors

- `∑_(i=1)^n▒i²`, `∏_(k=1)^5▒k` (Word's linear format; `Σ` works too). The operand spans `·` and
  `/` but not `+ −`: `∑_(i=1)^3▒F[i]·x[i]` is a sum of products. Limits are whole numbers; an
  empty range sums to 0 (product 1).
- A range `i = 1..5` (or `1…5`) is a vector `[1; 2; 3; 4; 5]`; `∑_i▒…` runs over its values.
- Vectors: `F = [1; 2; 3] kN`, `F[2]` (1-based), arithmetic element by element (a vector with a
  number, or two of the same length), functions element by element; `sum mean len min max` take
  any mix of values and vectors; `∑▒F` sums a vector. A vector holds values of one kind.
- Shown as `[1; 2; 3] kN`; a long vector shows its first 8 values, `…` and the last.
- Limits: 10 000 values per vector, range or sum; 1 000 000 steps per statement, per table and
per numeric solve; 20 000 000 steps per document; a calculated table evaluates at most 64
columns and 100 000 cells.

## Calculated tables

The first row is the **header**: the calculations the table does are read from it. Turning a
table into a calculated one (Calculated Table, `table.calc`, Data Table, Cells as Equations,
`calc.insertTable`) makes that row its header row (Table Design › Header Row, repeated at the top
of each page); a table whose first row holds values (a table of numbers) gets an empty header
row above them, to write the columns' names and formulas in. A header cell is a column name with an optional unit in square
brackets or parentheses and an optional formula: `L [m]`, `q (kN/m)`, `M [kN·m] = γ·q·L²/8`.
Any other header (`Beam`, `Load case`, empty) is a label column, never read; so is one whose
round brackets hold words, not a unit (`Load (case A)`). A unit in square brackets must be one
(`[kNN]` is reported). A formula needs no name, and its unit can close it: `M = γ·q·L²/8 [kN·m]` and
`= γ·q·L²/8 [kN·m]` (spreadsheet style) both compute the column. Spreadsheet style, the `=` is
hidden text: the header shows `γ·q·L²/8 [kN·m]`, and the `=` appears only while the caret is in
that cell (`LayoutOptions::reveal_hidden`); typing after it, the next calculation shows what was
typed; a table that stops calculating shows the `=` again. A column without a name is not defined after the table and no formula can read it, but
its totals work. A trailing `[…]` is a unit when it holds one and is not written against a
name (`x[i+1]`, `x[m]` index a column; `x [m]` is a unit). A column has one unit: before the `=`
or at the end, not both. In a `[°C]`/`[°F]` column a formula that is a difference of
temperatures (`ΔT [°C] = T_2 - T_1`) shows the difference (60, not −213.15), as statements do.

| Beam | L [m] | q [kN/m] | M [kN·m] = γ·q·L²/8 |
|---|---|---|---|
| B1 | 4.5 | 12 | *41.006* |
| B2 | 6000 mm | 10 | *60.75* |
| Total | | | *Σ = 101.756* |

- **Every cell works out what it is.** A cell holding `=` (or a comparison) is a statement of
  its own, read like a region with the row's values as parameters: `q·L²/8 =` becomes
  `q·L²/8 = 30.375 kN·m`; `= q·L²/8` (spreadsheet style) reads as `q·L²/8 =`; `x = 5 m` defines
  `x` for the rest of the document; `F = m·a` solves. Its value is the column's value in that row
  (for totals and for formulas to its right) when it is a result or defines the column's own
  name. In a formula column such a cell replaces the column formula. Cells are read row by row,
  left to right.
- Data cells are expressions, usually numbers in the column's unit; a cell with its own unit of
  the same kind is converted (`6000 mm` in a metre column). A bare-name column whose cells are
  not all values (`B1`) is a label column.
- Formula cells are overwritten with the formula's value for the row, in the column's unit (or
  with the automatic unit when the header gives none). The formula sees the row's values by
  column name, then the worksheet's parameters. A row whose input cells are all empty is skipped
  (its formula cells are cleared).
- A cell `Σ =` (also `sum`, `total`), `mean =`, `min =` or `max =` totals its column over the
  data rows; its row is not a data row.
- **Rows by index.** In a column formula `i` is the row's number among the data rows (1, 2,
  3…) and `n` how many there are (unless a column has that name), and `x[k]` is column `x` in
  data row `k`: `x[i+1]` the next row, `x[i-1]` the one before. Indices wrap round — after the
  last row comes the first, before the first the last — so a polygon closes, as Gauss's method
  wants. Area and centroid of a polygon from its vertices:

  | x [m] | y [m] | a [m^2] = (x·y[i+1] − x[i+1]·y)/2 | s [m^3] = (x + x[i+1])·(x·y[i+1] − x[i+1]·y)/6 |
  |---|---|---|---|
  | 0 | 0 | | |
  | 4 | 0 | | |
  | 4 | 3 | | |
  | 0 | 3 | | |
  | | | Σ = 12 | Σ = 24 |

  then `x_G = sum(s)/sum(a) =` gives 2 m. A column may read its own earlier rows (`c = c[i-1] +
  x`); a row not worked out yet is an error in that cell. A column only read by index does not
  hold a row back when its own cell is empty.
- **Rows by subscript**, as an equation writes them: `y_(i+1)` is column `y` in the next data
  row, `y_(i-1)` the one before, `y_1` the first, `y_n` the last, and `y_i` the row's own value.
  A column may be named for its rows, `y_i`: `y_(i+1)` reads it too. A subscript never wraps
  round: where the row isn't there (`y_(i+1)` in the last row, `y_(i-1)` in the first) the cell
  shows `–` — no data, not an error — and what reads that cell shows `–` too. The totals sum the
  values there are; a column with a `–` is not a vector after the table. A parameter or column
  of that very name (`F_1` defined before the table) stays itself, and `x[i+1]` keeps wrapping
  round (Gauss).

  | i | x_i | y_i | x_i·y_(i+1) |
  |---|---|---|---|
  | 1 | 3 | 3 | *12* |
  | 2 | 4 | 4 | *–* |
  | Σ | *7* | *7* | *12* |

- A dash in an input cell (`-`, `–`, `—`) is no value, like an empty cell.
- **Total Row** (Table Design › Table Style Options › Total Row, also Table Layout › Data and
  Math › Tables; `table.look {"totalRow"}`): the last row is the totals row. Its first column
  shows the sign `Σ` (the ordinal column's cell, or a label column's; when the first column is
  summed itself, `Σ = 7`), and each value or formula column's cell shows its sum alone (`7`, in
  the column's unit). A cell written there keeps its own text (`mean =`, `max =`, a label);
  what an earlier calculation left (a number, `Σ = …`, `–`, `⚠`) is written over. It is not a
  data row. Turning it on or off works the table out at once; off, what the calculations wrote
  in that row goes with it (it would otherwise be read as a row of data).
- **Column Total** (Table Layout › Data and Math › Tables; `table.calcTotal {"value"?}`): the
  column at the caret is summed in the total row or not; not summed, its cell there stays empty.
  Summing a column turns the total row on. Saved on the column's header cell
  (`strix:noTotal="1"` on its `w:tcPr`).
- **Ordinal column** (Table Layout › Data › Ordinal Column, Math › Tables; `table.calcIndex`,
  saved as `strix:calcIndex="1"` on `w:tblPr`): a first column, column "0", headed `i` (as `i` in a column formula), numbers
  the data rows with something in them 1, 2, 3… (not the header, a totals row or an empty row;
  `strix_calc::ordinals`). The calculations never read it: formulas, totals and a data table's
  row labels (the first column after it) see the table without it. Off, the column goes.
- After the table every complete column is a vector in the worksheet (`sum(M)`, `∑_i▒L[i]`); a
  column with empty cells is not defined.
- Errors stay in their cell (`⚠`) and are listed by `calc.recalculate` with row and column.

### Cells as equations

Table Layout › Cells as Equations (also in Math › Tables; `table.mathCells`) shows every cell of
a calculated table as an equation: names in italics with true subscripts (`F_k`), units in the
Unit style, the values the table works out written into them. A label (`Support`, `Beam 2`:
words with no digit, `=`, brackets or subscripts) stays plain text; single letters (`A`) are
symbols; units are styled only where a quantity or a column's unit is (`12 kN`, `F [kN]`). Turned
off, the cells are text again. Saved as `strix:calcMath` on `w:tblPr`.

The whole of a cell is its equation's place: a click anywhere in a cell that is an equation
edits that equation, and typing in a cell — empty, or with the caret beside its equation (after
Tab or the arrow keys) — types in its equation (an empty cell gets one first, one undo step with
what is typed). The table's sign `Σ` and a `–` stay words.

### Style

A table that becomes calculated with the default look (no style, or Table Grid) takes the
**Calculated Table** style, added to the document's styles the first time: a grid of thin lines,
the header row bold with a heavier bottom edge (1.5 pt) and the total row with a heavier top
edge (1.5 pt), so names and formulas stand apart from the values and the totals from the rows
they sum. A style the table already has stays; the style can be changed like any other (Table
Design › Table Styles).

### Data tables

A calculated table can name its cells instead of (as well as) its columns: Table Layout › Data
Table, or Math › Tables › Data Table (`table.calcNames`). Every value is then a parameter named
by its column and its row's label — the first column's text, its letters and digits — joined by
a subscript, one way or the other:

| Support | F [kN] | a [m] |
|---|---|---|
| A | 10 | 2 |
| B | 12 | 0.5 |

- **Column, row below** (`columnRow`): `F_A = 10 kN`, `a_B = 0.5 m`.
- **Row, column below** (`rowColumn`): `A_F = 10 kN`, `B_a = 0.5 m`.

Formula columns, units and totals work as in any calculated table, and the columns stay vectors
(`sum(F)`). A name must start with a letter: a row labelled `1` names `F_1`, never `1_F`. Saved as
`strix:calcNames` on `w:tblPr`.

## Quantities and units

`strix-calc` (L1, pure Rust, no dependencies) holds a quantity as an `f64` SI magnitude plus a
dimension vector over the 7 SI base dimensions. Units: SI base and derived (`m s kg A K mol cd N
Pa J W C V Ω Hz F H T Wb S lm lx`), full SI prefixes (`f p n µ m c d da k M G T P`, with exact
unit match before prefix splitting so `T` is tesla while `THz`/`TW` are tera-, `d` is day while
`dm`/`dL`/`dB` are deci-, and `pF` is picofarad), chemistry and physics units (`atm torr mmHg eV
cal Da u`), topography and mechanics units (`are ha gon/grad arcmin arcsec d/day yr rpm kn`),
common engineering units (`mm cm km`, `t`, `kN MN`, `kPa MPa GPa`, `min h`, `L`, `°` for angles,
`bar`, `kWh`), dimensionless units `%`, `‰`, `ppm`, `°`, `rad`, `gon`, `sr`, and temperatures in
`K`, `°C` and `°F` (see Temperatures). A unit typed in the wrong case gets a hint: "unknown name
KN (did you mean kN?)".

### Temperatures

`°C` and `°F` (also `℃` `℉`, and the ASCII spellings `degC` `degF`) sit beside the kelvin.
Temperatures are held in kelvin; the rules:

- **A number written in °C or °F is an absolute temperature**, converted with the scale's
  offset: `20 °C` is 293.15 K, `68 °F` is 293.15 K (= 20 °C), `-40 °F` = `-40 °C`. Also a
  vector (`[0; 100] °C`) and a number in a `[°C]` table column.
- **Inside a compound unit, or with no number, a degree is its size only**: `4.2 kJ/(kg·°C)` is
  4200 J/(kg·K), `3 °C/min` is 0.05 K/s.
- **Absolute − absolute is a difference**, shown in K: with `T_1 = 20 °C` and `T_2 = 80 °C`,
  `T_2 - T_1 = 60 K`. A difference times or over a number stays a difference (`2·ΔT = 120 K`).
- **Absolute ± a difference (or a value in K) stays absolute**: `T_1 + ΔT = 80 °C`,
  `T_1 + 5 K = 25 °C`. `min max mean abs round` keep the kind of their arguments
  (`mean(T_1; T_2) = 50 °C`). Anything else (`T_1 + T_2`, a value written in K) is a plain kelvin
  value.
- **Display**: an absolute temperature shows in the working unit (the last temperature unit
  written: `°C` after `T = 20 °C`), else in the unit it was written in; asked for after `=`
  (`T = °F`) it converts with the offset. Differences and kelvin values never take a °C/°F
  offset by themselves: they show in K (or a K working unit). A kelvin value asked for in °C is
  read as absolute (`300 K = °C` gives 26.85 °C); a difference asked for in °C is a difference
  (`ΔT = °C` gives 60 °C).
- How a parameter was written is remembered with it, like the dimensionless units, so the kind
  follows names through later statements. Checks compare in the left side's unit
  (`T_1 ≤ 30 °C ⇒ 20 °C ≤ 30 °C ✓`); worked steps never convert temperatures to one unit.

**Units are set apart.** In calculated equations every unit (`m`, `kN·m`, `°`, `%`, with its
power: `s²`) is upright (ISO 80000) and coloured with the **Unit** character style (`MathUnit`,
"Unità di misura", teal by default: change its colour in the Styles pane and the next
recalculation follows), after a thin space from its number (`300 mm`). Only names that are units
there count: a parameter `m` is not coloured as the metre. Generated exercise data use the same
style.

**Dimensionless units are kept.** A plain number carries no unit, so Strix remembers the one
written: `α = 30°` shows `α = 30°`, `p = 15 %` shows `15 %`. Results inherit it from their parts
(`90° − α` → `60°`, `2·p` → `30 %`, `1 + p` → `115 %`, `acos(0.5)` → `60°`); `sin(α)` and a ratio
`α/β` are plain numbers. A table column `[%]` reads `15` as 15 % and `15 %` as itself. Asking for
a unit converts as usual (`α = rad`).

**Display unit**, in order: the unit asked for after `=`; the **working unit** of the result's
kind, which is the last unit written for it (in a definition, a requested display unit or a
table header: after `b = 300 mm`, lengths show in mm); else a coherent SI unit from a small
preferred table (`N`, `Pa`, `J`, `W`, `N/m`, `m/s²` …) with an engineering prefix for force,
pressure, energy, power and electrical units (`kN`, `MPa`, `kJ`), else base units. Torque has
the dimension of energy and shows in `J` unless `N·m` is asked for.

Precedence, loosest first: comparisons; `+ -`; explicit `* · × /` (left to right); unary minus;
implicit multiplication; postfix `!` and `^` / Unicode superscripts `⁺⁻⁰¹²³⁴⁵⁶⁷⁸⁹` (right to left).
So `12 kN/m` is (12·kN)/m and `2 kN/4 m^2` is 500 Pa, while `1/2 m` is 1/(2 m). Typing `*` in an
equation writes the multiplication dot `·`. Limits: 4096 characters, nesting depth 64, 512 terms.

Functions: algebraic and trigonometric (`sqrt cbrt root(x; n) abs sign mod(a; b) exp ln log2
log(x; base) sin cos tan asin acos atan atan2(y; x) sinh cosh tanh asinh acosh atanh min max
round floor ceil trunc`), combinatorics and integer arithmetic (`gcd/mcd lcm/mcm fact/n!
binom(n; k)`), and laboratory statistics and linear regression (`sum mean median var varp
stdev/std stdevp len slope(X; Y) intercept(X; Y) r2(X; Y)`); `π`/`pi`, `e`, `g` (standard gravity)
are predefined. Numbers: `1.5`, `1,5` when the document language uses decimal commas, `2e-3`.

### User functions

- **`f(x) = 2·x + 1` defines a function** (`:=` works too); later `f(3) =` shows `f(3) = 7`.
  Several parameters: `g(a, b) = a·b` (`;` separates them too). Units flow through:
  `A(b, h) = b·h`, then `A(2 m, 3 m) = 6 m²`. Calls go anywhere a value goes: `f(3)^2`,
  `2·f(x) + 1`, `f(f(2))`, `∑_(i=1)^3▒f(i)`, element by element on vectors (`f([1; 2; 3])`).
- **What is a definition**: a name, then brackets holding plain names only (distinct), then `=`
  and a body. It is never an equation to solve, even with the parameters unknown. Anything else
  is an ordinary statement: `a(b + 1) =` and `a(3) =` are products, `sin(α) = 0.5` (a built-in
  function) is an equation, `f(3) = 7` a check. To solve an equation in a function, put the
  call on the right: `7 = f(y) ⇒ y = 3`.
- **Names in the body**: the parameters shadow the worksheet's parameters (and units: in
  `A(b, h) = b·h`, `h` is the parameter, not the hour; after a number a unit stays a unit).
  Every other name is read from the worksheet **when the function is called**, so
  `f(x) = k·x`, `k = 3`, `f(2) =` gives 6, and redefining `k` later changes later calls. A `∑`
  variable or a table column of the caller is not seen inside the body.
- After `f(x) = …` a name `f` followed by `(` is a call (even when `f` is also a unit, like `m`);
  `f` alone is still a parameter or unit. A later definition of `f` replaces it from there on;
  one that fails to parse undefines it. The built-in functions cannot be redefined.
- Limits: calls nest at most 32 deep (`MAX_CALL_DEPTH`; `f(x) = f(x)` stops with "functions call
  each other too deeply"), evaluation as a whole nests at most 640 deep, so recursion ends with
  an error, never a stack overflow; the step budgets bound the work of calls fanning out. A call
  with the wrong number of arguments is an error ("g takes 2 arguments, not 1").

**Composed units for structures.** A result that is a force times a length to a power keeps
the units it is written in, force first: `F·a` with `F` in kN and `a` in m is `24 kN·m` (a moment,
not `24 kJ`), `q·L²/8` is in kN·m, `γ·t` in kN/m², a line load in kN/m. Other compounds take the
derived unit (`2 kg·a` is `6 N`, `F/m` is `2.5 m/s²`), a unit asked for after `=` (`= Pa`) or a
working unit written for the kind wins.

## Number format

Set on the Math tab (Number Display: More/Fewer Digits, Rounding, Scope) or with `calc.options`;
resolved through the hierarchy **Equation > Exercise > Document > Preferences**. New documents
inherit their document options from Preferences (`calc_digits`, `calc_count`, `calc_zeros`,
`calc_steps`, `calc_layout`); opened `.xmath` and `.docx` documents use their own saved options
(`customXml/strixObjects.xml` in Word packages). Pass `"scope": "equation" | "exercise" |
"document" | "prefs"` (and `"reset": true` to clear an override) to `calc.options`, `calc.steps`
or `calc.layout`.

- **Digits** (default 3) with **Automatic** count: decimal places for |x| ≥ 1, significant
  digits below 1 (`30.375`, `0.0035`, `0.123`). **Decimal Places** always counts decimals
  (`0.0001` with 2 → `0`); **Significant Digits** counts significant digits (`1234.5` with 2 →
  `1200`).
- Rounding is half away from zero on the shortest decimal form, capped at 15 significant digits
  (`2.675` → `2.68` with 2 decimals). `round(x; n)`, `floor(x; n)`, `ceil(x; n)` and `trunc(x; n)`
  are **unit-aware**: with 1 or 2 arguments they round in the value's display or working unit
  (including `°`, `%`, engineering/written multiples such as `cm` or `km`, and affine temperatures
  `°C`/`°F`), then convert back to coherent SI while keeping the unit hint (`round(30°; 0) = 30°`,
  `round(20.6 °C; 0) = 21 °C`); a 3rd unit argument `round(x; n; unit)` rounds in that explicit
  unit (`round(alpha; 1; °)`).
- The automatic prefix is chosen on the number as shown: `999999.9996 Pa` shows `1 MPa`, never
  `1000 kPa`.
- In an equation a unit's quotient reads inline, as ISO 80000 writes units (`(12 kN/m)·…`, a
  linear fraction), while the formula's own fractions stay stacked.
- A value's number stays out of its unit's quotient: `v = 90 km/h` reads `90 km/h`, the number
  times the unit, never `90 km` stacked over `h`. The number keeps the formatting it was typed
  with, and only a well-formed one is taken out (`90`, `−9.81`, `2,5`; not `3-`); `1/s`, with no
  unit over the bar, stays a fraction.
- Trailing zeros are dropped unless **Show Trailing Zeros** (`2.500`).
- Scientific notation `1.235·10^9` at or beyond 10⁷, and below 10⁻³ except with Decimal Places.

## Solution steps and layout

**Show Steps** (`calc.steps`, Math › Number Display, on by default) shows how each result was found:

| Statement | Steps |
|---|---|
| a formula: `M = q·L²/8 =`, `M = q·L²/8`, `q·L²/8 =`, `A := …  =` | the values substituted (with their units, one unit per kind), the result |
| solved symbolically: `F = m·a` | each step of the algebra isolating the unknown (one inversion each: `π·d² = 4·A`, `d² = 4·A/π`, `d = √(4·A/π)`; powers, roots, `exp ln log`, `sin cos tan` and their inverses likewise), then the substitution and the result |
| solved numerically: `x³ + x = k` | the equation with its values (`x³ + x = 10`), the result, and the note in the block's language: `(numerical solution)`, `(soluzione numerica)`, `(numerische Lösung)` or `(solution numérique)` |
| a check: `σ ≤ f_yd` | the two sides' values and ✓ or ✗ |

A formula with nothing to substitute (`L = 4.5 m`, `A = 3 m·5 m =`) and a parameter shown alone
(`M =`) have no steps: the result says it all. A unit asked for after `=` (`= cm^2`) is the
result's unit, after the working (and, being a working unit, the substitution's).

**Result Layout** (`calc.layout`, Math › Number Display; a preference like the number format) says where
the result and the steps go:

- **Inline (⇒)** (`inline`, the default): on the statement's line, the algebra after `⇒`, the
  substitution and the result after `=`:
  `F = m·a ⇒ a = F/m = (1 kN)/(400 kg) = 2.5 m/s²`,
  `M = q·L²/8 = (12 kN/m)·(4.5 m)²/8 = 30.375 kN·m`, `σ ≤ f_yd ⇒ 30.375 MPa ≤ 355 MPa ✓`.
  Without steps: `F = m·a ⇒ a = 2.5 m/s²`.
- **Line by Line** (`lines`): one line each under the statement, aligned at `=` (a check at its
  comparison); the unknown is named on each line of a solved equation, a formula's lines go on
  with `=`:

  ```
  F = m·a            M = q·L²/8
  a = F/m              = (12 kN/m)·(4.5 m)²/8
  a = (1 kN)/(400 kg)  = 30.375 kN·m
  a = 2.5 m/s²
  ```

  Without steps a solved unknown and a check's values still go under the statement; a formula's
  result (`M = q·L²/8 = 30.375 kN·m`) stays on its line.

The steps are results, rebuilt with every recalculation and when the layout changes. Inline,
everything after the statement is Strix's (read back, only the statement and its result count:
a result's unit, if changed, is kept). On lines, only the statement (the first row) is edited: a
click or an arrow key into the steps puts the caret at the end of the statement; read back, the
statement goes with its last line (the result, for its unit). Such an equation is marked
`strix:steps="1"`, as is one carrying the numerical note. With steps, `M = q·L²/8` reads back as
`M = q·L²/8 =` (it showed its value). Calculated table cells show results only, never steps.

**One unit per kind of quantity.** Quantities of one kind written in different units are worked
in one, that of the first: with Show Steps `5 m·30 cm =` shows `5 m·30 cm = 5 m·0.3 m = 1.5 m²`
(off, the result alone), and a formula's substitution writes its values of one kind in one unit
(`A = (200 mm)·(2000 mm) + (5 mm)·(2000 mm)`). Read back, the working is recognised as Strix's
own (between the statement and its result, values with units and no parameter; older documents'
`5 m·30 cm = 5 m·0.3 m` too), so the statement is worked again from its data, never checked
against the old working. A definition worked out at once does the same:
`A = 3 m·50 cm =` shows `A = 3 m·50 cm = 3 m·0.5 m = 1.5 m²`. A chain the user wrote that ends
without a result (`A = 3 m = 4 m =`) is still an error.

**Units in results are units.** In every part of a line (between `=`, `⇒` and comparisons) the
units are upright, in the Unit style, after a thin space (`30.375 MPa ≤ 355 MPa ✓`); a parameter
spelled like a unit stays a parameter in its own part (`m` the mass in `F = m·a`, the metre in
`2.5 m/s²`).

## Commands (engine, `cmd/calc.rs`)

| Id | Does |
|---|---|
| `calc.insert` | insert a calc region (`{"text": "L = 4.5 m"}`), then recalculate |
| `equation.calc` | the Calculate button: the equation at the caret is a region or not (`{"value"?: bool}`) |
| `calc.newEquations` | preference: new equations are regions (default on) |
| `table.calc` | the table at the caret is calculated or not (`{"value"?: bool}`) |
| `calc.insertTable` | insert a calculated table (`{"rows"?, "cols"?}`) |
| `calc.tableInfo` | whether the table at the caret is calculated, and its options (`columnTotal`: the caret's column is summed) |
| `table.calcTotal` | Column Total: the column at the caret is summed in the total row or not (`{"value"?: bool}`) |
| `calc.recalculate` | evaluate every region and table in order, update results; returns errors |
| `calc.variables` | the parameters of the document and of each exercise sandbox, with jump positions and optional unit conversions (`{"units"?: {…}}`) |
| `view.variablesPane` | open or close the **Variables** task pane (Math › Calculation › Variables) |
| `calc.options` | number format: digits, count, trailing zeros (and `layout`) |
| `calc.steps` | Show Steps: formulas, solved equations and checks show their working (a preference) |
| `calc.layout` | Result Layout: `{"value"?: "inline" \| "lines"}` (default: toggle), a preference; recalculates |
| `calc.new` | a new empty calculation at the caret, being edited: **Alt+Shift+C** (⌃⌥C on a Mac; Ctrl+Space stays Word's Reset Character Formatting) |
| `equation.quickKeys` | preference (default on): `^`, `_` and `/` make the superscript, subscript or fraction at once |

**Quick keys** (Mathcad-style, `equation.quickKeys`): typing `^` makes a superscript of the
operand before the caret, `_` a subscript, `/` a fraction with it as the numerator; the caret
goes into the new box and Right leaves it. The operand is the name or number just typed, a
bracket typed as text (`(a+b)/`), or the structure before the caret; `M_Ed^2` gives the same `M`
both scripts. With nothing before it `/` makes an empty fraction (the caret in the numerator).
Off, the three characters build up on Space as in Word.

**Selecting in a formula**: Shift with the arrows, Home or End selects from where it started
(`equation.move {"extend": true}`); a selection reaching into a box takes the whole structure
(the fraction, the script). Typing, a symbol or a structure replaces it, Backspace and Delete
remove it, Copy gives its linear text (`(L^2)/8`); a plain move drops it.

The **Math** tab gathers the tools: Calculation (New Calculation, Calculate, Always Calculate,
Recalculate), Symbols, Exercise (docs/exercise.md), Tables (Calculated Table, Calculate Table),
Number Display (More/Fewer Digits, Show Steps, Rounding, Result Layout) and Drawing (CAD Drawing,
Edit Drawing, see `docs/cad.md`).

## Stages

1. **Scalar worksheet**: parser, units, evaluation, errors; commands; tests. *Done.*
2. **One `=`**: unknown detection, symbolic isolation, degree 1–2 polynomial and `abs` solving,
   numeric roots with dimensional inference, checks and comparisons, working units, number format,
   unit hints, `°C`/`°F`, Variables pane and unit converter. *Done.*
3. **Ranges, vectors, `Σ`/`∏`, user functions `f(x) = …`, lab statistics and regression.** *Done.* Still to do: matrices.
4. **Calculated tables.** *Done.*
5. Solution steps (each isolation step, substitution, result; numeric solves marked) and the
   result layout (inline or line by line). *Done.* Still to do: systems of equations across
   regions (pending equations solved together), plots.

## Revision history

- 2026-11-14: calculated tables: rows by subscript (`y_(i+1)`, `y_(i-1)`, `y_1`, `y_n`, `y_i`; columns named `y_i`), never wrapping round, `–` where there is no such row; the total row shows `Σ` in its first column and the sums alone; Column Total (`table.calcTotal`) per column; in Cells as Equations the whole cell is its equation's place; the Calculated Table style (heavier header bottom and total top edges).

- 2026-11-13: Phase 2 (`M4`–`M6`): full SI prefixes (`P`, `T`, `da`, `d`, `p`, `f`) and electromagnetism/photometry/chemistry/topography units (`F H T Wb S lm lx atm torr mmHg eV cal Da u are ha gon/grad arcmin arcsec d/day yr rpm kn`); STEM functions (`cbrt`, `root`, `log2`, `log(x; b)`, `atan2`, hyperbolic, `sign`, `mod`), combinatorics (`gcd`/`mcd`, `lcm`/`mcm`, `fact`/`n!`, `binom`) and lab statistics/regression (`median`, `var`, `varp`, `stdev`/`std`, `stdevp`, `slope`, `intercept`, `r2`) plus full Unicode superscripts (`⁺⁻⁰¹²³⁴⁵⁶⁷⁸⁹`); symbolic degree 1–2 polynomial solver with dimensional Δ check, physical domain constraints (`t > 0`) and `|u(x)| = c` branching, algebraic dimension inference in numeric fallback, and the **Variables** pane (`view.variablesPane`, Math › Calculation › Variables) with unit conversion selector and click-to-jump across document and exercise sandboxes.

- 2026-11-13: calculation options (`digits`, `count`, `zeros`, `steps`, `layout`) resolve through `equation > exercise > document > prefs` and save in `.xmath` and `.docx` (`customXml/strixObjects.xml`); `round`, `floor`, `ceil` and `trunc` are unit-aware (display/working/affine units plus optional 3rd unit argument); `(numerical solution)` note in EN, IT, DE and FR.
- 2026-11-12: calculated tables: a column formula needs no name (`= q·L²/8`, its `=` hidden except while the cell is edited) and its unit may close it (`M = q·L²/8 [kN·m]`); the ordinal column is headed `i`.
- 2026-11-09: Math tab: Calculation (Always Calculate, was Calculate New Equations), Calculate Table (was Calculate This Table), Number Display (was Results; Rounding, was Number Format), smaller and in two columns.
- 2026-11-09: units after numbers alone (`[4; 5; 6] m`, `10^3 m`) and after a written quantity
  and `/` (`(12 kN)/m`) win over parameters; a parameter alone after `=` of the other side's
  kind is checked, not read as a unit; `round` keeps units and rounds as results show; the
  automatic prefix follows the shown number; `[°C]` difference columns; round-bracket words in a
  header are a label; units' quotients inline in equations; recalculating a document with no
  calculation adds no Unit style.
- 2026-10-29: Result Layout (`calc.layout`): results and steps inline after `⇒` or line by line
  under the statement; steps for every formula (also `M = q·L²/8 =`, `q·L²/8 =`, `A := … =`, which
  had none), each algebra step of a solved equation, numeric solves marked; check lines' units
  styled.

- 2026-10-28: a definition worked out at once (`A = 3 m · 5 m =` defines `A` and shows
  `15 m²`), with its working under Show Steps; a product of quantities is no working unit.
- 2026-10-28: temperatures in `°C` and `°F` (`degC`, `degF`): absolute with the offset,
  differences in K; user functions `f(x) = …` with parameters, units, call-time names and a
  recursion cap.
- 2026-10-28: calculated tables: rows by index (`i`, `n`, `x[i+1]`, cyclic) for Gauss's area and centroid.
- 2026-10-28: calculated tables: the header row is marked (a table of numbers gets one), Total Row sums the columns, ordinal column 0.
- 2026-10-28: calculated tables can show their cells as equations.
- 2026-10-27: composed units for moments and loads (`kN·m`, `kN/m²`), force first.
- 2026-10-27: data tables: cells named by column and row with a subscript (`F_A` or `A_F`).
- 2026-10-27: quantities of one kind worked in one unit, with the working shown (`5 m·30 cm = 5 m·0.3 m = 1.5 m²`).
- 2026-10-27: quick keys `^ _ /` in formulas; New Calculation on Alt+Shift+C.
- 2026-10-27: the solution steps are not edited; only the statement takes the caret.
- 2026-10-27: a unit after a number is always the unit; solution steps (formula, substitution, result) under formulas and solved equations.
- 2026-10-27: units styled (Unit character style, upright, thin space).
- 2026-10-27: automatic recalculation after edits (waiting while the caret is in the item);
  every table cell is data or a statement; dimensionless units (`% ‰ ppm ° rad gon sr`) kept
  through results; step budgets per document, table and solve.
- 2026-10-27: one `=` for definitions, results, solving and checks; Σ/∏, ranges and vectors;
  calculated tables; number format (digits, auto/decimals/significant, zeros, half-away
  rounding); working units; comparisons with ✓/✗; unit hints; Math tab; `*` types `·`.
- 2026-10-27: new (stage 1).

## Laboratory measurements and uncertainty (Phase 5)

A scalar measurement carries an absolute SI uncertainty: `L = 12.4 ± 0.1 cm`
(`+-` also works), or `L = 12.4 ± 2% cm` for a relative uncertainty. A unit
written after the uncertainty applies to both numbers. `val(L)` extracts the
central value, `unc(L)` the absolute uncertainty with the same dimension, and
`relunc(L)` its dimensionless relative uncertainty (`unc(L)/abs(val(L))`). The
last is undefined at zero. Celsius/Fahrenheit uncertainties are differences,
so no temperature offset is added to the uncertainty.

Math › Results › Rounding offers Maximum Uncertainty and Uncertainty in
Quadrature (GUM). The same setting is `calc.options {"uncertainty":"max"|"rss"}`,
with the existing equation > exercise > document > preferences inheritance and
Word round trip. Maximum propagation adds absolute derivative contributions;
RSS combines them using `hypot`. This is first-order propagation with independent
inputs; covariance and correlations between repeated uses of one measurement
are not modelled. Arithmetic, powers, square/cube roots, absolute value,
exponential/logarithmic and trigonometric/hyperbolic unary functions, sums and
means retain uncertainty, including vector elements. Functions without a
propagation rule return an error for uncertain inputs: use `val()` deliberately
when only the central value is wanted.

Results round uncertainty to two significant figures when its first digit is
1 or 2, otherwise one; the central value uses the same decimal position.
Unit conversion scales both the central value and uncertainty, including
trailing zeros (`12.44 ± 0.12 cm` → `124.4 ± 1.2 mm`).

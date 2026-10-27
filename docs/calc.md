# Live calculations with units

> **Status:** stages 1–4 done (worksheet with one `=`, solving for the unknown, Σ/∏, vectors, calculated tables, number format, working units, checks); Variables pane, systems and solution steps next · **Last updated:** 2026-10-27 · **Change:** automatic recalculation; cell statements; dimensionless units

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
**Calculate New Equations** preference (`calc.newEquations`) changes that.

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

Strix saves only `.xmath`/`.xmathm` (plus static PDF, PNG, LaTeX, HTML and Markdown exports): a
`.docx` or any other editable format would lose the regions, so they are import-only.

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
| `M =`, `M = kN·m` | none | show, in the unit asked for | `M = 30.375 kN·m` |
| `F = m·a` | `a` | **solve** for the one unknown and define it | `F = m·a ⇒ a = 2.5 m/s²` |
| `x² + x = 6` | `x` | solve (numerically: it appears twice) | `x² + x = 6 ⇒ x = 2` |
| `a + b = c` | none | **check**: both sides must agree | as typed, or an error |
| `σ ≤ f_yd` | none | check a comparison (`≤ ≥ < > ≠`) | `σ ≤ f_yd ⇒ 5.4 MPa ≤ 355 MPa ✓` |
| `E = mc²` | `E`, `mc` | more than one unknown: an ordinary equation | as typed (reported) |

- A number (or a vector) after `=` with every name before it known is a result to refresh:
  `M = 99 kN·m` becomes `M = 30.375 kN·m`. Units alone after `=` ask for a display unit.
- `:=` still defines (and redefines) explicitly: `x := 2 m`.
- Solving: the unknown is **isolated** symbolically when it appears once (inverting `+ − · /`,
  powers, `√`, `exp ln log`, `sin cos tan` and their inverses, principal values); the result is
  kept only if it satisfies the equation. Otherwise it is found **numerically**: its unit is
  tried from the equation's known values, every sign change on a logarithmic grid from 10⁻¹² to
  10¹² (both signs) is refined by bisection (a sign change across a pole is dropped), and secant
  steps catch roots that touch zero. The **smallest non-negative root** wins, else the negative
  root closest to zero. No root: "no value of x solves the equation".
- After `⇒` Strix writes the solved value; on the next calculation that tail is replaced, and
  its unit, if changed, is kept (`⇒ a = mm/s²`).
- An error never stops later items: the region shows `= ⚠` (or `⇒ x = ⚠`) and the message is
  listed by `calc.recalculate`. Names depending on a failed definition are undefined.
- Identifiers: letters (Latin, Greek), digits, `_` subscripts (`f_ck`, `σ_max`). A defined
  parameter wins over a unit of the same spelling, **except right after a number**: there unit
  names are units (ISO 80000: a quantity is a number times a unit), so with a mass `m = 400 kg`,
  `L = 4.5 m` is still metres and `q = 12 kN/m` still per metre. The units after a number run on
  through `·`, `/`, spaces and powers (`9.81 m/s²`, `30 kN·m`) and stop at the first name that
  is not a unit. To multiply by a parameter spelled like a unit, put it first or use brackets:
  `L·2 kN`, `(2 kN)·L`, `2·m`.

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

The first row is the header. A header cell is a column name with an optional unit in square
brackets or parentheses and an optional formula: `L [m]`, `q (kN/m)`, `M [kN·m] = γ·q·L²/8`.
Any other header (`Beam`, `Load case`, empty) is a label column, never read.

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
Pa J W C V Ω Hz`), prefixes (`n µ m c d k M G`) on the units that take them, common engineering
units (`mm cm km`, `t`, `kN MN`, `kPa MPa GPa`, `min h`, `L`, `°` for angles, `bar`, `kWh`), and
dimensionless units `%`, `‰`, `ppm`, `°`, `rad`, `gon`, `sr`. Temperature is absolute (`K`); affine `°C` is a later stage. A unit typed in
the wrong case gets a hint: "unknown name KN (did you mean kN?)".

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
implicit multiplication; `^ ² ³` (right to left). So `12 kN/m` is (12·kN)/m and `2 kN/4 m^2` is
500 Pa, while `1/2 m` is 1/(2 m). Typing `*` in an equation writes the multiplication dot `·`.
Limits: 4096 characters, nesting depth 64, 512 terms.

Functions: `sqrt abs exp ln log sin cos tan asin acos atan min max round sum mean len`; `π`/`pi`,
`e`, `g` (standard gravity) are predefined. Numbers: `1.5`, `1,5` when the document language uses
decimal commas, `2e-3`.

**Composed units for structures.** A result that is a force times a length to a power keeps
the units it is written in, force first: `F·a` with `F` in kN and `a` in m is `24 kN·m` (a moment,
not `24 kJ`), `q·L²/8` is in kN·m, `γ·t` in kN/m², a line load in kN/m. Other compounds take the
derived unit (`2 kg·a` is `6 N`, `F/m` is `2.5 m/s²`), a unit asked for after `=` (`= Pa`) or a
working unit written for the kind wins.

## Number format

Set on the Math tab (Results: More/Fewer Digits, Number Format) or with `calc.options`; kept as
a preference (`calc_digits`, `calc_count`, `calc_zeros`).

- **Digits** (default 3) with **Automatic** count: decimal places for |x| ≥ 1, significant
  digits below 1 (`30.375`, `0.0035`, `0.123`). **Decimal Places** always counts decimals
  (`0.0001` with 2 → `0`); **Significant Digits** counts significant digits (`1234.5` with 2 →
  `1200`).
- Rounding is half away from zero on the shortest decimal form, capped at 15 significant digits
  (`2.675` → `2.68` with 2 decimals).
- Trailing zeros are dropped unless **Show Trailing Zeros** (`2.500`).
- Scientific notation `1.235·10^9` at or beyond 10⁷, and below 10⁻³ except with Decimal Places.

## Solution steps

With Show Steps (`calc.steps`) a formula or a solved equation shows, one under the other and
aligned at `=`: the formula isolated for the unknown, the values substituted, the result. The
steps are results, rebuilt with every recalculation: only the statement (the first row) is
edited. A click or an arrow key into the steps puts the caret at the end of the statement.


**One unit per kind of quantity.** Quantities of one kind written in different units are worked
in one, that of the first: `5 m·30 cm =` shows `5 m·30 cm = 5 m·0.3 m` with `= 1.5 m²` under it
(with Show Steps on; off, the result as before), and a formula's substitution writes its values
of one kind in one unit (`A = (200 mm)·(2000 mm) + (5 mm)·(2000 mm)`). Read back, the working
after `=` is recognised as Strix's own (the left side has units to bring together and the right
side two quantities or more, where a stale result has one number), so the statement is worked
again from its data, never checked against the old working. A unit asked for after `=`
(`= cm^2`) is kept as asked, without a working.
## Commands (engine, `cmd/calc.rs`)

| Id | Does |
|---|---|
| `calc.insert` | insert a calc region (`{"text": "L = 4.5 m"}`), then recalculate |
| `equation.calc` | the Calculate button: the equation at the caret is a region or not (`{"value"?: bool}`) |
| `calc.newEquations` | preference: new equations are regions (default on) |
| `table.calc` | the table at the caret is calculated or not (`{"value"?: bool}`) |
| `calc.insertTable` | insert a calculated table (`{"rows"?, "cols"?}`) |
| `calc.tableInfo` | whether the table at the caret is calculated |
| `calc.recalculate` | evaluate every region and table in order, update results; returns errors |
| `calc.variables` | the parameters at the end of the document: name, value (or values), unit, where |
| `calc.options` | number format: digits, count, trailing zeros |
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

The **Math** tab gathers the tools: Calculations (New Calculation, Calculate, Calculate New
Equations, Recalculate), Tables (Calculated Table, Calculate This Table), Results (More/Fewer
Digits, Number Format) and Drawing (CAD Drawing, see `docs/cad.md`).

## Stages

1. **Scalar worksheet**: parser, units, evaluation, errors; commands; tests. *Done.*
2. **One `=`**: unknown detection, symbolic isolation, numeric roots, checks and comparisons,
   working units, number format, unit hints. *Done.* Still to do: Variables pane, unit picker,
   `°C`/`°F`.
3. **Ranges, vectors, `Σ`/`∏`.** *Done.* Still to do: user functions `f(x) = …`, matrices.
4. **Calculated tables.** *Done.*
5. Systems of equations across regions (pending equations solved together), solution steps
   (substitution, isolation lines), plots.

## Revision history

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

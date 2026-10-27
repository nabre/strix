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

- 2026-10-27: a unit after a number is always the unit; solution steps (formula, substitution, result) under formulas and solved equations.
- 2026-10-27: units styled (Unit character style, upright, thin space).
- 2026-10-27: automatic recalculation after edits (waiting while the caret is in the item);
  every table cell is data or a statement; dimensionless units (`% ‰ ppm ° rad gon sr`) kept
  through results; step budgets per document, table and solve.
- 2026-10-27: one `=` for definitions, results, solving and checks; Σ/∏, ranges and vectors;
  calculated tables; number format (digits, auto/decimals/significant, zeros, half-away
  rounding); working units; comparisons with ✓/✗; unit hints; Math tab; `*` types `·`.
- 2026-10-27: new (stage 1).

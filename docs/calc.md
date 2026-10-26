# Live calculations with units

> **Status:** stage 1: `strix-calc` engine done; commands and UI in progress · **Last updated:** 2026-10-26 · **Change:** new

Strix documents can hold live engineering calculations, in the spirit of SMath Studio and
Mathcad: you define parameters with units, Strix remembers them, and later expressions reuse them
and show their results with units, recomputed whenever the inputs change. This is a Strix
feature beyond Word parity; the document stays a normal OOXML package (`.xmath`), so Word shows
the last computed values as ordinary equations.

## Model

A **calc region** is an ordinary equation (`InlineObject::Equation`, OMML on disk) whose linear
text is a calc statement. Regions are evaluated in reading order (document order: body top to
bottom, a paragraph left to right), like a Mathcad worksheet. Only regions see each other; normal
equations are never touched.

| Statement | Meaning | Shown |
|---|---|---|
| `L := 4.5 m` | define `L` | as typed |
| `q := 12 kN/m` | define with a derived unit | as typed |
| `M := q*L^2/8` | define from earlier parameters | as typed |
| `M =` | evaluate, unit chosen automatically (torque has the dimension of energy) | `M = 30.38 kJ` |
| `M = kN*m` / `M = 30.4 kN*m` | evaluate in a requested unit (the number after `=` is replaced, the unit kept) | `M = 30.38 kN·m` |
| `σ := M/W =` | define and show | `σ := M/W = 125 MPa` |

- A name used before its definition, an unknown name, or a dimension mismatch (`2 m + 3 s`) is an
  error: the region shows `= ⚠` and the message is listed by `calc.variables`/`calc.recalculate`.
  An error never stops later regions; names depending on a failed definition are undefined.
- Redefining a name later is allowed (worksheet semantics): later regions see the new value.
- Identifiers: letters (Latin, Greek), digits, `_` subscripts (`f_ck`, `σ_max`). A name that is a
  defined parameter wins over a unit of the same spelling (`m := 3` makes later `m` the number).

## Quantities and units

`strix-calc` (L1, pure Rust, no dependencies) holds a quantity as an `f64` SI magnitude plus a
dimension vector over the 7 SI base dimensions. Units: SI base and derived (`m s kg A K mol cd N
Pa J W C V Ω Hz`), prefixes (`n µ m c d k M G`) on the units that take them, common engineering
units (`mm cm km`, `t`, `kN MN`, `kPa MPa GPa`, `kN/m`, `min h`, `L`, `°` for angles, `bar`,
`kWh`), and dimensionless `%`. Temperature is absolute (`K`); affine `°C` arithmetic is a later
stage. Display picks the requested unit, else a coherent SI unit from a small preferred table
(`N`, `Pa`, `J`, `W`, `N/m`, `m/s²` …) with an engineering prefix for force, pressure, energy,
power and electrical units (`kN`, `MPa`, `kJ`), else base units (`kg·m^-1·s^-2`). Once shown, the
unit after `=` is kept on later recalculations, as a requested one. On an error the region shows
`= ⚠` followed by the requested unit, if any.

Precedence, loosest first: `+ -`; explicit `* · × /` (left to right); unary minus; implicit
multiplication; `^ ² ³` (right to left). So `12 kN/m` is (12·kN)/m and `2 kN/4 m^2` is 500 Pa,
while `1/2 m` is 1/(2 m). A text after `=` that is not a unit (`E = mc^2`, `y = 2 x`) makes the
equation an ordinary one, never evaluated. Limits: 4096 characters, nesting depth 64, 512 terms.

Functions: `sqrt abs exp ln log sin cos tan asin acos atan min max round`; `π`/`pi`, `e`, `g`
(standard gravity) are predefined. Numbers: `1.5`, `1,5` when the document language uses decimal
commas, `2e-3`. Results show 4 significant digits by default (`calc.options`).

## Commands (engine, `cmd/calc.rs`)

| Id | Does |
|---|---|
| `calc.insert` | insert a calc region (`{"text": "L := 4.5 m"}`), then recalculate |
| `calc.recalculate` | evaluate every region in order, update results; returns errors |
| `calc.variables` | the parameters defined at the end of the document (or at the caret): name, value, unit, where |
| `calc.options` | significant digits, auto-recalculate on/off |

Recalculation is automatic after any edit that changes a calc region (and on open) unless turned
off. Results are written into the equation's linear text and math nodes, so undo, saving and
export see them like any edit. The UI adds a **Calculate** ribbon tab (Insert region, Recalculate,
Variables pane).

## Stages

1. **Scalar worksheet** (this stage): `strix-calc` parser, units, evaluation, errors; `calc.*`
   commands; Calculate tab; tests.
2. Variables pane with values and units at the caret; unit conversion picker; `°C`/`°F`.
3. User functions `f(x) := …`, ranges `i := 1..10`, vectors and matrices, `Σ`/`Π` over ranges.
4. Tabular calculation: table columns as vectors, column formulas with units, sums like `SUM`.
5. Solving: `solve(f(x) = 0, x, guess)`, systems, plots.

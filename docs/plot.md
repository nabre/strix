# Scientific and experimental plots (`strix-plot`)

> **Status:** done · **Last updated:** 2026-11-15 · **Crates:** `crates/plot` (`L1`), `crates/engine/src/cmd/plot.rs` (`L4`), `crates/ui-egui/src/plot_pane.rs` (`L6`)

`strix-plot` draws live Cartesian plots driven by the active calculation sandbox (`strix-calc::Worksheet`) — either the main document worksheet or the enclosing exercise's active variant sandbox. Every plot is a `GraphicKind::Plot` inline graphic holding both its JSON specification (`word/strix/plotN.json`) and a vector SVG rendering (`word/media/plotN.svg`), rendered natively into `GraphicItem` primitives for `vello_cpu`, `krilla` PDF export and `.docx` round-trip.

## Mathematical & experimental series

A `Plot` holds one or more `Series` drawn over a shared Cartesian viewport (`[xMin, xMax] × [yMin, yMax]`, with major/minor grid lines, axis arrowheads, tick labels formatted with `strix-calc` options, title, axis labels, units and optional legend):

- **Cartesian functions (`type: "function"`):** `y = f(x)` (or custom variable) sampled adaptively with recursive midpoint subdivision and automatic pole/vertical-asymptote splitting (`1/x`, `tan(x)` never draw spurious vertical segments across discontinuities). Optional notable features:
  - `roots: true` — sign-change + bisection zeros `(x, 0)`;
  - `extrema: true` — golden-section local minima and maxima;
  - `tangentAt: x0` — central-difference slope and clipped tangent line at `x0`;
  - `integral: [a, b]` — adaptive Simpson definite integral $\int_a^b f(x)\,dx$ with shaded area between the curve and the horizontal axis.
- **Parametric curves (`type: "parametric"`):** `(x(t), y(t))` over `t ∈ [tMin, tMax]` evaluated in the active sandbox.
- **Polar curves (`type: "polar"`):** `ρ = f(θ)` in the plot's angle convention and unit (Reference system).
- **Experimental data (`type: "data"`):** reads vector columns defined by preceding calculated tables (`calc.table`, `xCol`, `yCol`, optional `dxCol`/`dyCol` or inline `points`), automatically extracting measurement uncertainties (`±`) as horizontal and vertical error bars with end caps, plus optional least-squares **linear** (`c0 + c1·x`) or **quadratic** (`c0 + c1·x + c2·x²`) regression curves with coefficient of determination $R^2$.

## Reference system

- **Axis names at the arrows.** The abscissa's and ordinate's names (`xLabel`, `yLabel`, with their units) are written at the tips of the axes' arrows, as Cartesian axes are labelled: the abscissa's above its axis at the right end, the ordinate's beside its axis at the top. An axis out of view (`0` outside its range) has its name in the margin instead.
- **Names are variables.** When an axis name is a name (`x`, `E`, `N`, `t`), the formulas use it: the pane shows `N = …` for a function of `E`. Renaming the abscissa (`plot.set {"xLabel": "E"}`, or the pane) renames the variable of the functions written in the old name, whole names only (`2x + exp(x)` → `2E + exp(E)`; `Plot::set_x_label`). A label that is not a name (`Tempo (s)`) keeps `x` and `y`.
- **Oriented angles** (`angles`): `math` (default) starts from the positive abscissa and turns counter-clockwise (`x = ρ·cos θ`, `y = ρ·sin θ`); `survey` starts from the positive ordinate and turns clockwise, like a bearing from north (`x = ρ·sin θ`, `y = ρ·cos θ`). **Angle unit** (`angleUnit`): `rad` (default), `deg`, `gon`; the pane offers gon when surveying is chosen. **Show angle direction** (`angleMark`) draws a short ray along the reference axis and an arc with an arrow the way angles grow, `θ` inside it, at the origin (or near the lower-left corner when the origin is out of view).
- **Polar curves (`type: "polar"`):** `ρ = f(θ)` over `theta: [a, b]` in the angle unit (a full turn when absent), placed by the convention. `θ` holds the angle itself, as calc's angles do, so `sin(θ)` is right in every unit.

## Automatic ranges

**Automatic** (`autoRange: true`) fits the ranges to what the plot has to show instead of `xMin`…`yMax` (kept for when it is turned off; turning it off in the pane keeps the fitted view). The abscissa spans the analysis (roots and extrema found in the ranges set, tangent points, integral bounds), the data with their error bars and the parametric and polar curves; with none of these it stays as set. The ordinate spans the functions over that abscissa (their central 96 %, so a pole does not flatten the rest), the analysis points, the data and the curves. Both get an 8 % margin, take in the origin when it is near, and are rounded outward to a tidy step. `plot.get` returns the ranges drawn as `range`.

## Formulas in the pane

Formulas are written in the Equation's linear format (`x^2`, `√x`, `(a)/(b)`, `sin⁡x`), which is what calc reads from the document's equations, and each field shows the formula built up by the math engine that sets the document's equations (`y = x²`), so the pane and the document look alike. The series cards fit the pane's width: text boxes take the room left in their row. Each card's **Line** row shows the four line styles drawn in the series' colour (`style`: `solid`, `dashed`, `dotted`, `dashDot`; dots and dashes grow with the line) and the thickness in millimetres (`width`); older plots' `dashed: true` reads as `dashed`.

## Legend

**Legend** lists every series drawn: a short line in its colour and style, then its formula, after its name when it has one or the ordinate's (`f(x) = 2·sin(x)`, `y = x²`, `ρ = 1 + cos(θ)`), written as text reads it (single-character powers as superscripts, `·` for `*`, `√` for `sqrt`); data by their name or columns. It is off by default.

## Analysis in the document

**Insert Analysis in Document** (Plot Properties, `plot.analysis`) writes after the plot, one
paragraph each, live calculations of every function series (calc regions, recomputed like any
other):

- the definition, `f(x) = x^2 - 4` (the series' label names the function when it is one, like
  `f(x)` or `v(t)`; otherwise `f`, `g`, `h`…);
- **zeros**: `0 = f(x_1); x_1 > -3; x_1 < -1 ⇒ x_1 = -2`, one per zero the plot found; the plot
  only gives the bracket (halfway to the next point), calc solves;
- **extrema**: `0 = f′(x_3); … ⇒ x_3 = 0` and `f(x_3) = -4`;
- **tangent** at `x₀`: `m = f′(1) = 2` and the line `t(x) = m·(x - 1) + f(1)`;
- **integral**: `A = ∫_(-2)^(2)▒〖f(x) dx〗 = -10.667`.

Points are numbered across the plot (`x_1`, `x_2`…) because each solution defines its name;
second tangents and areas are `m_2`, `t_2`, `A_2`. The words before each line (Zeros, Minimum,
Maximum, Tangent, Area) come in the interface's language (`labels`). Derivatives and integrals
are calc's own ([calc.md](calc.md), Derivatives and integrals), so changing the definition in
the document updates every result.

## Dual student / teacher mode (`answer`)

Both the whole `Plot` (`plot.answer = true`, *Student blank grid*) and individual `Series` (`series[i].answer = true`, *Solution only*) participate in the exercise's `Show` state (`exercise.show {"solution": false}`):

- When the enclosing exercise hides its solution (`Show = false`), `Plot::render_for(ws, false)` omits `answer` curves, points, shaded integrals, regressions and legend entries while keeping the **exact same** frame `(width, height)`, viewport margins, grid lines and axis ticks.
- Students receive a calibrated, labelled Cartesian plane ready to sketch on by hand, and pagination never shifts between the student handout and the teacher key.

## Multi-variant validation & pre-flight checks

Plot expressions participate in continuous multi-variant validation (`exercise.data` `variantErrors`) and `exercise.check`: if an inactive variant causes a syntax error, unit mismatch or total domain collapse (for instance `sqrt(a)*x` when variant `B` sets `a = -4`), the error is flagged immediately on that variant's badge and in the Assessment Check pane.

## Commands & UI

- **Ribbon:** `Math › Drawing › Plot` (`plot.insert {"openPane": true}`) and `Plot Properties` (`view.plotPane`).
- **Commands:**
  - `plot.insert` — inserts a live plot at the caret (`plot` JSON or shorthand `expr`, `xMin`, `xMax`, `yMin`, `yMax`, `title`, `answer`, `openPane`);
  - `plot.get` — returns `{"plot", "viewport", "notables", "integrals", "regressions", "errors"}` for the selected plot;
  - `plot.set` — updates the selected plot's specification or fields in one undo step and recalculates.
  - `plot.analysis` — writes the analysis after the selected plot as live calculations (`labels` for the words before each line).
- **Word `.docx` round-trip:** wrapped in `strix:obj:N` with its JSON source and SVG fingerprint in `customXml/strixObjects.xml` (`docs/word-round-trip.md`), staying live across saves in Microsoft Word.

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-11-15 | minor | Analysis in the document: `plot.analysis` and Insert Analysis in Document write each function's definition, zeros, extrema, tangent and integral after the plot as live calc regions, using calc's new derivatives and integrals |
| 2026-11-15 | minor | Reference system: axis names at the arrows and used as the formulas' variables, mathematical/surveying angle convention and unit, angle direction mark, polar curves; automatic ranges; formulas shown built up by the math engine in the pane; series cards fit the pane; line styles (solid, dashed, dotted, dash-dot) and thickness; legend fixed (the pane showed it on while it was off) and lists each formula |
| 2026-11-13 | major | Initial `strix-plot` crate (`L1`), `GraphicKind::Plot`, adaptive Cartesian/parametric/experimental plotting with error bars, calculus notables/integrals, regression, student blank-grid mode, OOXML + Word round-trip, `plot.*` commands and Plot Properties pane |

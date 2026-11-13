# Scientific and experimental plots (`strix-plot`)

> **Status:** done · **Last updated:** 2026-11-13 · **Crates:** `crates/plot` (`L1`), `crates/engine/src/cmd/plot.rs` (`L4`), `crates/ui-egui/src/plot_pane.rs` (`L6`)

`strix-plot` draws live Cartesian plots driven by the active calculation sandbox (`strix-calc::Worksheet`) — either the main document worksheet or the enclosing exercise's active variant sandbox. Every plot is a `GraphicKind::Plot` inline graphic holding both its JSON specification (`word/strix/plotN.json`) and a vector SVG rendering (`word/media/plotN.svg`), rendered natively into `GraphicItem` primitives for `vello_cpu`, `krilla` PDF export and `.docx` round-trip.

## Mathematical & experimental series

A `Plot` holds one or more `Series` drawn over a shared Cartesian viewport (`[xMin, xMax] × [yMin, yMax]`, with major/minor grid lines, axis arrowheads, tick labels formatted with `strix-calc` options, title, axis labels, units and optional legend):

- **Cartesian functions (`type: "function"`):** `y = f(x)` (or custom variable) sampled adaptively with recursive midpoint subdivision and automatic pole/vertical-asymptote splitting (`1/x`, `tan(x)` never draw spurious vertical segments across discontinuities). Optional notable features:
  - `roots: true` — sign-change + bisection zeros `(x, 0)`;
  - `extrema: true` — golden-section local minima and maxima;
  - `tangentAt: x0` — central-difference slope and clipped tangent line at `x0`;
  - `integral: [a, b]` — adaptive Simpson definite integral $\int_a^b f(x)\,dx$ with shaded area between the curve and the horizontal axis.
- **Parametric curves (`type: "parametric"`):** `(x(t), y(t))` over `t ∈ [tMin, tMax]` evaluated in the active sandbox.
- **Experimental data (`type: "data"`):** reads vector columns defined by preceding calculated tables (`calc.table`, `xCol`, `yCol`, optional `dxCol`/`dyCol` or inline `points`), automatically extracting measurement uncertainties (`±`) as horizontal and vertical error bars with end caps, plus optional least-squares **linear** (`c0 + c1·x`) or **quadratic** (`c0 + c1·x + c2·x²`) regression curves with coefficient of determination $R^2$.

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
- **Word `.docx` round-trip:** wrapped in `strix:obj:N` with its JSON source and SVG fingerprint in `customXml/strixObjects.xml` (`docs/word-round-trip.md`), staying live across saves in Microsoft Word.

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-11-13 | major | Initial `strix-plot` crate (`L1`), `GraphicKind::Plot`, adaptive Cartesian/parametric/experimental plotting with error bars, calculus notables/integrals, regression, student blank-grid mode, OOXML + Word round-trip, `plot.*` commands and Plot Properties pane |

# CAD drawings

> **Status:** stage 1 (model, parametric coordinates, rendering, point entry) in `strix-cad`; document integration and editor next · **Last updated:** 2026-10-27 · **Change:** new

Strix documents hold technical drawings whose sizes follow the document's calculations: a beam
drawn `L` long redraws when `L` changes, and its dimension shows `L = 4.5 m`. This is a Strix
feature beyond Word parity. The behaviour comes from the owner's earlier prototype (a
Python/Typst editor), re-designed for Strix: one Rust geometry module feeds the page, the PDF
and the editor, where the prototype kept three copies (TypeScript, Python, Typst) in step.

## Model (`crates/cad`, L1, depends on `strix-calc` only)

- A `Drawing` has a **unit** (`mm`, `cm`, `m`; real-world sizes, y up, angles in degrees
  counter-clockwise), a **scale** `1:N` (one unit is `unit_mm / N` paper mm), **layers** and
  **entities** (at most 5000, drawn in order).
- **Layers**: name, colour, stroke width (paper mm), dash (solid, dashed, dotted, dash-dotted),
  visible. Layer `0` always exists. An entity's own stroke overrides its layer's ("ByLayer").
- **Entities** have a stable `id` (`e1`, `e2`…; edits and dimensions refer to ids, never
  indices) and a layer: `line {a, b}`, `circle {c, r}`, `arc {c, r, start, end}`,
  `polyline {pts, bulges, closed}` (DXF bulges), `point {p}`, `text {p, value, height, angle}`,
  `dim {a, b, offset, orient, text}` (aligned, horizontal or vertical).
- **Paper sizes do not scale**: stroke widths, text heights, dimension offsets, ticks and gaps
  are paper mm; positions are drawing units (the prototype moved the dimension offset to paper
  mm because it drifted with the scale).
- **JSON** is canonical (fixed key order, numbers rounded to 0.001), so the same drawing always
  gives the same bytes. Reading is defensive: limits enforced, ids made unique, non-finite
  numbers dropped, sizes clamped.

## Parametric drawings: bound to the calculations

Every coordinate, radius and angle is a number **or an expression of calculation parameters**:
`{"type": "line", "a": [0, 0], "b": ["L", 0]}`. `Drawing::resolve` evaluates them with the
worksheet as it stands where the drawing sits in the document (the parameters defined before
it): a length converts to the drawing's unit (`L = 4.5 m` is 4500 in a mm drawing), a plain
number is drawing units, an angle is degrees (`30°`). A failing expression keeps its last value
and is reported.

Dimension text: `<>` is the measured size, `{{L}}` the value of parameter `L` as calculations
show it (`L = {{L}}` → `L = 4.5 m`); none shows the size.

This gives "drawings driven by results" without a constraint solver. Driving dimensions and
geometric constraints (the prototype's Gauss-Newton solver) are a later stage.

## Rendering

`Drawing::render` gives primitives in paper mm (y down) and the paper size: polylines with
colour, width and dash; dots; texts with height, angle and anchor. Circles and arcs are
tessellated; dimensions draw extension lines, the dimension line, 45° ticks and the label above
the line, kept readable. The page, the PDF and the editor all draw these primitives.

## Point entry (`parse_point`)

AutoCAD-style: `x,y`; `@dx,dy` relative; `@d<a` polar from the last point; `d<a` polar from the
origin; a lone number is a **direct distance** towards the pointer. `10<30<5` and other junk are
errors.

## In the document (next)

Insert › Illustrations › **CAD Drawing** and Math › Drawing insert a drawing. It is stored as a
Word **drawing canvas** whose shapes are generated from the model (so Word and every export show
the drawing) with the CAD source kept beside it in the `.xmath` package. Recalculation re-resolves
the drawing in reading order and regenerates its shapes.

## Stages

1. **Model, parametric coordinates, labels from parameters, rendering, point entry.** *Done
   (crate).*
2. Document integration: `insert.cad`, `cad.*` commands (add entity, set, delete, layers),
   regeneration on recalculation, `.xmath` round trip, PDF.
3. Editor (egui): canvas with zoom/pan, grid, crosshair, command line with direct distance and
   `<angle`, Ortho (F8), snaps (endpoint, midpoint, centre, intersection, perpendicular,
   nearest), click-add selection with window/crossing, grips, undo.
4. Edit commands: move, copy, mirror, offset, trim, extend, fillet, join, explode, match
   properties; hatch (by selection and by point); angular dimensions, rows and chains.
5. Driving dimensions and geometric constraints bound to parameters (solver).

## Revision history

- 2026-10-27: new: design and stage 1.

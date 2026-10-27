# CAD drawings

> **Status:** stages 1–2 done (model, parametric coordinates, rendering, point entry; drawings in the document, saved, redrawn with the calculations); editor next · **Last updated:** 2026-10-27 · **Change:** kernel, snaps, grips, SVG

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

Annotations recall parameters (as the prototype's dimension texts did): in a dimension's text
and in a free text, `{{L}}` or `{{L:value}}` is the value of parameter `L` as calculations show
it (`4.5 m`), `{{L:symbol}}` its symbol (`L`), `{{L:both}}` both (`L = 4.5 m`); `<>` is a
dimension's measured size, and a dimension without text shows it. A parameter not defined where
the drawing sits shows `[L?]` in red.

This gives "drawings driven by results" without a constraint solver. Driving dimensions and
geometric constraints (the prototype's Gauss-Newton solver) are a later stage.

## Precision: the geometry kernel (`crates/cad/src/kernel.rs`)

Snaps, grips and measures work on the exact curves (segments, circles, arcs, polyline bulges),
never on the drawn outlines:

- **Intersections** in closed form: line–line by cross products; line–circle from the foot of
  the perpendicular from the centre and the half chord `√((r − h)(r + h))`; circle–circle along
  the centre line `a = (d² + r₁² − r₂²)/2d`, across it `√((r₁ − a)(r₁ + a))`. These forms keep
  every digit where the textbook ones lose half of them (near tangency, near-equal radii).
- **Perpendiculars**: the foot on a line is the projection; on a circle, the radius through the
  point. **Tangents** from a point: the tangent points at `±atan2(√((d − r)(d + r)), r)` off the
  direction to the point, so a tangent is square to its radius to 1e-9°.
- **Angles** with `atan2(|u × v|, u · v)`, exact to rounding at every angle (`acos` of the dot
  product loses half the digits near 0° and 180°).
- **Tolerance** for "same point", "tangent" and "parallel": 1e-9 of the size of the geometry
  (at least 1e-9 units). Within it a line touching a circle meets it once; parallel lines are
  those within a sine of 1e-12.
- **Limit of approximation**: coordinates are saved to **1e-9 drawing units** (nine decimals: a
  nanometre in a millimetre drawing; above 10⁶ units `f64` keeps 15–16 significant digits). An
  angle measured from saved points of a side `L` long is right to about `2·10⁻⁹/L` rad (1.2e-9°
  for a 100-unit side); between exact directions to 1e-15 rad, and near 180° to π's own spacing
  in `f64` (4.4e-16 rad). Tests pin each bound (`crates/cad/src/tests.rs`).

## Rendering

`Drawing::render` gives primitives in paper mm (y down) and the paper size: polylines with
colour, width and dash; dots; texts with height, angle and anchor. Circles and arcs are
tessellated; dimensions draw extension lines, the dimension line, 45° ticks and the label above
the line, kept readable. The page, the PDF and the editor all draw these primitives.

## Point entry (`parse_point`)

AutoCAD-style: `x,y`; `@dx,dy` relative; `@d<a` polar from the last point; `d<a` polar from the
origin; a lone number is a **direct distance** towards the pointer. `10<30<5` and other junk are
errors.

## In the document

Insert › Illustrations › **CAD Drawing** and Math › Drawing › CAD Drawing (`insert.cad`) insert a
drawing in line at the caret (a starter rectangle with its two sizes, or the `drawing` given).
It is a graphic (`GraphicKind::Cad`) holding the drawing's source; its items (paths and texts in
points) are drawn from it, so the page, PDF and image exports show it like a chart.

- **Reading order**: a drawing is evaluated with the calculations, in the worksheet as it stands
  where the drawing sits (inside an exercise, that exercise's sandbox). Changing `L` above it
  redraws it with the automatic recalculation; so does opening the file.
- **Coordinates in the drawing unit**: length parameters count in the drawing's unit, so
  expressions read like a CAD's (`L - 0.15`, `L/2 + 0.3` in a metre drawing); a value written with
  a length unit converts (`300 mm`); a force or a vector is an error, reported by
  `calc.recalculate` and `cad.get`.
- **Units in the Unit style**: a parameter shown in a dimension (`L = {{L}}`) has its unit in the
  Unit character style's colour, like the calculations' units.
- **Texts are level**: a steep dimension's text is set level beside its line, on the outer side
  (rotated text is a later stage).
- **JSON and SVG**: the drawing is managed by its JSON; its SVG picture is made automatically
  from it, only when what it shows changes (or when a file brings none). Saving writes a picture
  (`pic:pic`) of the SVG (`word/media/cadN.svg`, with Office's SVG picture extension, so other
  programs show the drawing) whose properties carry the source part (`word/strix/cadN.json`,
  extension `urn:io.github.nabre.strix:cad`): Strix reads and edits the JSON, others show the
  picture. Files from the first version (the JSON inline in `a:graphicData`) still open.
- **Padding**: the picture's space around the drawing, paper mm (`"padding": 2` or
  `[top, right, bottom, left]`, 0–100), set in the editor (Picture › Padding, `ui.cadPadding`).
- **Commands**: `insert.cad`, `cad.get` (source and failing expressions of the selected
  drawing), `cad.set` (replace its source), `cad.add` (add an entity; returns its id).

## Stages

1. **Model, parametric coordinates, labels from parameters, rendering, point entry.** *Done
   (crate).*
2. **Document integration**: `insert.cad`, `cad.get/set/add`, regeneration in reading order with
   the calculations, `.xmath` round trip, units in the Unit style. *Done.* Still to do: rotated
   texts and dashed lines in the drawn items, a picture for Word.
3. **Editor** (`crates/ui-egui/src/cad_editor.rs`). *First cut done*: double-click a drawing, or
   Math › Drawing › Edit Drawing (`ui.cadEdit`): the page gives way to the drawing's sheet and the
   **CAD Drawing** tab comes up (Draw: Line, Circle, Polyline, Rectangle, Arc, Point, Text;
   Dimensions: Aligned, Horizontal, Vertical; Modify: Select, Move, Copy, Delete, Undo, Redo;
   Precision: Snap, Ortho, Grid, Zoom Fit; Close Drawing). Wheel zooms about the pointer, the
   middle button or Shift+drag pans; a grid; a crosshair with snap markers (ends, midpoints,
   centres, grid). The command line takes `x,y`, `@dx,dy`, `@d<a`, `d<a`, a distance towards the
   pointer, a tool alias (`l pl rec c a t m co`), `c` to close a polyline, a radius; Enter ends,
   Esc cancels the step then the command, F8 toggles Ortho, Del deletes. Selection: click, Shift
   to add or remove, a window left to right, a crossing right to left. Every finished operation
   is one `cad.set` (an undo step); the editor reads the drawing back after Undo or a
   recalculation. Moving keeps a coordinate's expression (`L` → `L + 0.5`).
   **Object snaps** (exact, from the kernel; AutoCAD's markers and names): Endpoint, Midpoint,
   Centre, Node, Quadrant, Intersection, Perpendicular and Tangent from the last point, Nearest
   (only when nothing else); the nearest wins, ties to that order; each toggles in Object Snaps
   (`ui.cadToggle {"what": "snap.<kind>"}`); Ortho holds a point only when no object snap does.
   The status line shows distance, angle, Δx and Δy from the last point.
   **Grips** (AutoCAD and Allplan): the selected objects (up to 50) show square grips; a click
   takes one (hot, red), the next point places it (a click with snaps and Ortho, or a typed point:
   `@dx,dy` is from where it was), Esc puts it back; dragging a grip places it where released.
   Line ends and middle (moves the line), circle centre and quadrants (radius), arc centre, ends
   (angles) and middle (radius), polyline vertices and segment middles (a straight segment moves,
   an arc bends through the point), point and text position, dimension ends and line (distance).
   A dragged coordinate keeps its expression; a degenerate result is refused.
   **Frame** (Picture › Frame): the picture's edge with the area outside it shaded light grey.
   `ui.cadSelect` (`{"all": true}` or `{"ids": […]}`, Ctrl+A) selects objects.
   Still to do: offset, trim, extend, fillet, mirror, stretch, layers and stroke controls,
   angular dimensions, hatches, constraints.
4. Edit commands: move, copy, mirror, offset, trim, extend, fillet, join, explode, match
   properties; hatch (by selection and by point); angular dimensions, rows and chains.
5. Driving dimensions and geometric constraints bound to parameters (solver).

## Revision history

- 2026-10-27: geometry kernel (exact intersections, perpendiculars, tangents, angles; limit of approximation); exact object snaps; grips; frame; padding; JSON source with an automatic SVG picture.
- 2026-10-27: placeholders with a mode (value, symbol, both) in dimensions and texts; missing data in red.
- 2026-10-27: stage 3 first cut: the drawing editor.
- 2026-10-27: stage 2: drawings in the document, redrawn with the calculations.
- 2026-10-27: new: design and stage 1.

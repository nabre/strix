# CAD drawings

> **Status:** stages 1–2 done (model, parametric coordinates, rendering, point entry; drawings in the document, saved, redrawn with the calculations); stage 3 editor in progress · **Last updated:** 2026-11-12 · **Change:** the rectangular array from a base point to an end point (fit, exact spacing, spread), arrays one object (a group, Explode ungroups); previously: texts picked in their box with corner grips, arrowheads picked inside, a hatch's boundary grips; previously: drawing names (slugs, `cad.name`) shown in the underlay list; Arc ways (3 points; centre, start, end; start, centre, end; start, end, radius) on an Arc split button; Polyline on the Line split button, Text only in its own group; drawings are drawn in millimetres (one in cm or m converted in the editor and saved so with the first edit, `Drawing::to_mm`, its expressions kept by `exprUnit`), the Units group only has the input unit; the status line's Snap arrow (object snaps menu, all on or off) in place of the nine object snap icons; scale and name editable in the Properties pane; a new Offset icon; previously: underlays (another drawing of the document under this one, to trace over) and inherited layers and styles; style windows (Dimension, Level, Text Style); units shown in dimension and level texts; dimension and level texts moved with a grip (`textShift`); slope ticks (barbette) and Flip Side for strips; a drawing knows the space where it sits; scales typed by hand; object snaps, Snap, Grid, Ortho and Zoom Fit in the status line (no CAD View tab); Match large beside the Layer Manager, Explode/Join one split button, no Undo/Redo in the ribbon; the document's panes hidden while editing; Grid off by default; previously: named text and dimension styles

Strix documents hold technical drawings whose sizes follow the document's calculations: a beam
drawn `L` long redraws when `L` changes, and its dimension shows `L = 4.5 m`. This is a Strix
feature beyond Word parity. The behaviour comes from the owner's earlier prototype (a
Python/Typst editor), re-designed for Strix: one Rust geometry module feeds the page, the PDF
and the editor, where the prototype kept three copies (TypeScript, Python, Typst) in step.

## Model (`crates/cad`, L1, depends on `strix-calc` only)

- A `Drawing` has a **unit** (`mm`, `cm`, `m`; real-world sizes, y up, angles in degrees
  counter-clockwise), a **scale** `1:N` (one unit is `unit_mm / N` paper mm), **layers** and
  **entities** (at most 5000, drawn in order).
- **Layers**: name, colour, stroke width (paper mm), dash (solid, dashed, dotted, dash-dotted,
  dash-double-dotted — a long dash and two dots; patterns in paper mm, so they read the same at any
  scale),
  visible, locked (shown and snapped to, not picked), printed (off: only in the editor, never on
  the page or in the PDF). Layer `0` always exists. An entity's own stroke overrides its layer's
  ("ByLayer").
- **Entities** have a stable `id` (`e1`, `e2`…; edits and dimensions refer to ids, never
  indices) and a layer: `line {a, b}`, `circle {c, r}`, `arc {c, r, start, end}`,
  `polyline {pts, bulges, closed}` (DXF bulges), `point {p}`, `text {p, value, height, angle}`,
  `dim {a, b, offset, orient, text}` (aligned, horizontal or vertical), `angle {c, a, b, r}`,
  `level {p, datum, text, flip, left}` (an elevation mark).
- **Arrowheads** (`"arrows": "start" | "end" | "both"`, Properties › Arrows): an open line,
  arc or polyline carries filled arrowheads at its ends — a force, a distributed load, a
  direction — the dimension style's mark size long and 30° across the tip (ISO 129-1), pointing
  along the path's last stretch at least an arrowhead long (an arc's follows the curve). Closed
  shapes ignore it. Trimmed, exploded or mirrored, each piece keeps only the arrowheads of the
  ends it still has.
- **Paper sizes do not scale**: stroke widths, text heights, dimension offsets, ticks and gaps
  are paper mm; positions are drawing units (the prototype moved the dimension offset to paper
  mm because it drifted with the scale).
- **Input unit** (`input`: `mm`, `cm` or `m`; Properties with nothing selected › Input unit,
  `ui.cadInputUnit`): the unit numbers typed in the editor are read in — the command line's
  points (`x,y`, `@dx,dy`, `@d<a`, direct distances), radii and diameters, and the Offset,
  Fillet and Chamfer distances — converted to the drawing's unit (`unit`); left out, the
  drawing's own. A value written with its unit is read in that (`2.5m`, `@300mm<45`,
  `120cm;2m`; `strix_cad::parse_point_in`, `parse_length`). Angles, factors and counts are not
  lengths. The status bar shows it (`Unit: mm · input m`).
- **Dimension style defaults** (ISO 129 / ISO 128 practice): text 2.5 mm, thin lines 0.18 mm
  (`lineWidth`; a dimension's own stroke width wins), ticks 3 mm drawn heavier (about 0.5 mm),
  extension lines from 1 mm off the object to 2 mm past the dimension line, and with ticks or
  dots the dimension line runs 1.5 mm past the extension lines (`lineOver`; not with arrows).
- **Styles** (CAD Annotate › Dimension Style and Text Style; `strix_cad::StyleKind`, `crates/cad/src/styles.rs`):
  every drawing has the **Standard** dimension style (`dimStyle`) and as many named ones as it
  needs (`dimStyles: [{"name", "style": {…as dimStyle}}]`, at most 64), and named **text
  styles** (`textStyles: [{"name", "font"?, "height", "bold"?, "italic"?}]`; Standard is the
  document's font at the dimension text height). A text, dimension, angle or level mark names its
  style (`"style": "Arch"`; none, or a name the drawing doesn't have: Standard). New objects take
  the current style (`currentDimStyle`, `currentTextStyle`; a new text also the style's height).
  A dimension style has a **start mark** (`mark`) and an **end mark** (`markEnd`, absent: as the
  start), a **mark width** (`markWidth` mm, 0 or absent: automatic — a tick about 2.8× the line,
  an arrowhead as thin), and may name the **text style** of its texts (`textStyle`: their font,
  bold and italic; the height stays the style's `text`). A dimension's own `mark` (Properties ›
  Marks) puts that mark at both ends. Renaming or deleting a style carries along the objects,
  current styles and dimension styles that name it (deleted: they take Standard). The ribbon:
  the style list (choosing one with objects selected gives it to them, as a CAD's list does),
  its name to rename it, New (a copy of it, made current), Delete; then the current style's
  fields. Commands: `ui.cadStyles {"kind": "dim"|"text", "new"|"rename"+"to"|"delete"|"current"}`,
  `ui.cadDimStyle {"style"?, …, "markEnd"?, "markWidth"?, "textStyle"?}` and
  `ui.cadTextStyle {"style"?, "font"?, "height"?, "bold"?, "italic"?}`. On screen a text style's
  bold is the interface's semibold and its font family is shown on the page and in print.
- **Drawing unit** (CAD Annotate › Units; `ui.cadUnit {"unit": "mm"|"cm"|"m", "convert"?}`):
  what one coordinate measures. Changed, the drawing's lengths are converted (1000 mm becomes
  1 m: the drawing keeps its size, on paper too), or with **Keep the numbers** they stay and mean
  the new unit. Expressions are left as written and worked out in the new unit. The input unit
  is beside it.
- **Drawn in millimetres**: the editor works on a drawing in centimetres or metres converted to
  millimetres (`Drawing::to_mm`: lengths and the underlay's offset converted, dimension styles
  that showed the drawing's unit show the old one, expressions keep meaning the old unit through
  `exprUnit`). Opening and closing such a drawing changes nothing: the document takes the
  converted drawing with the first edit, and Undo brings the old one back. The page, the PDF,
  `cad.get` and scripts see the drawing in the unit it is saved in. **Compatibility:** a
  converted drawing with expressions needs a Strix that knows `exprUnit` (2026-11-11 or later);
  an older one would read its expressions in millimetres (1000 times too small for metres).
- **Dimension units** (`dimStyle.units`, CAD Annotate › Dimension Style › Dimension units): how
  linear dimensions show their size (`<>`) — in the drawing's unit (`drawing`, the default), all
  in metres, centimetres or millimetres (`m`, `cm`, `mm`), or as SIA 400 (`sia`): metres with
  two decimals from 1 m up (`2.45`, `3.00`), centimetres below (`30`, `30.5`), rounded to the
  centimetre before choosing (99.996 cm reads `1.00`). Numbers follow the document's number
  style (decimal comma). Angles and level marks keep their own units. A word the program does not
  know in a drawing's style or end mark reads as the default instead of failing the drawing.
- **Two-part distances**: a linear dimension's line is `offset` paper mm (annotative: the same
  in print at any scale) plus `offsetUnits` drawing units (scaled: it follows Scale and the
  drawing's scale) from its points; an angle's arc is `r` drawing units (scaled) plus `rPaper`
  paper mm (annotative) from its vertex. Either part may be zero (the JSON leaves a zero
  `offsetUnits` / `rPaper` out). Scale multiplies the scaled parts only. The style's **Scaled
  distances** (`dimStyle.scaled`, off by default) says which part a click or a grip places: on
  paper, or in the drawing; the other part is kept. Properties shows both, as **Scaled
  distance** (drawing units) and **Annotative distance** (paper mm) — an angle's **Scaled
  radius** and **Annotative radius** — each marked with its overlay colour and explained on
  hover.
- **The parts on screen** (editor only: never printed, exported or in the SVG picture): for the
  selected dimensions (up to 50), the one being placed and the one a grip is dragging, the editor
  draws a **blue reference** at the scaled distance, in dash and two dots (ISO 128's phantom
  line, the CAD linetype Dash-dot-dot: the boundary between the two parts) — a line parallel to
  the dimension line between its extension lines, or an arc (a square for a right angle) at the
  scaled radius
  — and two stretches along the normal (an angle's bisector) in the dimension's middle: **blue**
  from the farther measured point's level (the vertex) to the reference, the scaled part, and
  **magenta** from the reference on to the dimension line or arc, the annotative part, each
  labelled with its size (`scaled 300 mm`, `annotative 6 mm`). A zero part is a dot: all
  annotative, the reference passes through the farther measured point (a ring on the vertex);
  all scaled, it lies on the dimension line. The geometry is `Drawing::dim_parts`.
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
the drawing sits shows `[L?]` in red. A parameter is **set as the equations set it** (in
dimensions, angles, level marks and texts; on the page, in the PDF, the SVG picture and the
editor): its symbol in italics with its subscript smaller and lowered (`F_k` as 𝐹ₖ), the value
upright with its unit in the Unit style, in the equations' font; the text around it keeps the
drawing's font. `strix_cad::spans` splits a text into these pieces (markers `MATH_START/END`,
`SYM_START/END`), and the page measures each piece in its font, so they sit side by side as
in an equation.

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
- **Tolerance** for "same point", "tangent" and "parallel": 1e-9 of the size of the *feature*
  (a segment's length, a radius), at least 1e-9 units, never below the rounding noise of the
  coordinates (64 ulps of the largest). It does not grow with the distance from the origin: a
  2 mm circle 5 km away is as exact as one at it. Within it a line touching a circle meets it
  once; parallel lines are those within a sine of 1e-12.
- **Constructions**: Tan Tan Tan in closed form (the problem of Apollonius: for each choice of
  side the conditions are two linear equations and one quadratic; every real circle is found,
  including those around the given circles, and the one touching nearest the picks wins). Tan
  Tan Radius and Extend meet with the curves' own tolerance (offsets are true lines, not long
  segments). Tangent lines must touch the arcs, not only their circles. A circle through
  nearly aligned points that would lie beyond the drawing's limits is refused.
- **Near-straight bulges**: a polyline arc whose sagitta is within its chord's tolerance is the
  chord (its centre would be far off, and its tolerance with it); a tessellated arc ends exactly
  on its vertex. A closed polyline of two vertices (DXF's circle) has both its segments.
- **Selection** works on the exact curves: a click's distance, and windows and lassos (a
  crossing takes what passes through it with no vertex inside; a window holds a circle only if
  no edge cuts it). Orientation signs use Shewchuk's adaptive predicates (`robust`), and a point
  on a window's or lasso's border is inside for both.
- **Drawing**: curves are tessellated to a chord tolerance of 0.01 mm on paper
  (`φ = 4·asin(√(tol/2r))` per segment, at most 4096), not a fixed count.
- **Limit of approximation**: coordinates are saved to **1e-9 drawing units** (nine decimals: a
  nanometre in a millimetre drawing; above 10⁶ units `f64` keeps 15–16 significant digits). An
  angle measured from saved points of a side `L` long is right to about `2·10⁻⁹/L` rad (1.2e-9°
  for a 100-unit side); between exact directions to 1e-15 rad, and near 180° to π's own spacing
  in `f64` (4.4e-16 rad). Tests pin each bound (`crates/cad/src/tests.rs`).

## Rendering

`Drawing::render` gives primitives in paper mm (y down) and the paper size: polylines with
colour, width and dash; dots; texts with height, angle and anchor (the anchor is in the text's
own frame: a turned text turns about it). Circles and arcs are tessellated; dimensions draw
extension lines, the dimension line, 45° ticks and the label above the line, kept readable. The
page, the PDF and the editor all draw these primitives.

- **Texts at their angle**: a text entity's `angle` and a dimension's slope reach the primitive;
  the SVG writes `rotate(−angle)` (y down), the page turns each text item about its rectangle's
  centre (`GraphicItem::Text::angle`, drawn as a `Draw::Rotated`), so the PDF and the raster
  agree. `render::readable` turns a direction to read as drafting reads it: within (−90°, 90°],
  half a turn round when it pointed left or down, so a vertical text reads bottom to top.
- **Dashes on every output**: the SVG writes the pattern as `stroke-dasharray`; the page, which
  strokes solid paths, gets each dashed line, arc, circle and polyline cut into its dashes
  (`Prim::pieces`, `render::dash_runs`: from the first point, carried round corners and a closed
  shape's closing side; at most 20 000 dashes a path, the rest solid; a degenerate pattern draws
  solid). Hatches and dimensions stay solid.
- **One budget a drawing** (`render::MAX_POINTS`, 2 000 000 points; `MAX_HATCH_TOTAL`, 200 000
  hatch and strip segments): every entity's own limits hold (4096 chords an arc, 40 000 segments
  a hatch), and the drawing as a whole stays within these too, so a file of thousands of huge
  arcs or fine hatches draws quickly in bounded memory. Past the budget a curve keeps a few
  chords (at least 16 points), a polyline's remaining arcs are drawn as their chords (its own
  vertices are always kept), and hatches are left out; every entity is still drawn.
- **Light saved outputs** (`render::MAX_OUTPUT_POINTS`, 500 000 points): the SVG picture and the
  page items saved with the drawing are made from `Rendered::lightened`, which past that thins
  every path of more than a few points by the same share; texts, dots and single strokes stay.
  A drawing near the render budget saves a few megabytes, not tens (the editor still draws it
  whole).
- **Compact outputs**: in the SVG picture a run of single strokes in one style (a hatch's
  lines) is one `<path>` of as many pieces; on the page it is one item, so fine hatches, which
  are drawn first, never use up the page's 20 000 items before the dimensions over them.
- **Saved drawings reopen**: `to_json` writes the drawing made safe (`sanitize`), so a number an
  edit left non-finite is never written as `null`; coordinates an edit pushes past ±`MAX_COORD`
  are clamped as reading them back would. A value too small to write (a datum of 1e-12, written
  `0`) is left out as a zero is, so a drawing read back saves to the same bytes.
- **Copies at once**: Copy and the rectangular and polar arrays add all their copies in one
  go (`Drawing::copies`: fresh ids from one set, the drawing made safe once), so an array filling
  the drawing's 5000 entities takes milliseconds, not seconds; a hatch or strip copied with its
  boundary follows the copy.
- **The editor's drawing is kept**: the ribbon asks for the selected drawing resolved where it
  sits every frame; `calc::resolved_cad` evaluates the document only up to the drawing and keeps
  the answer until what it reads changes (the calculations before it, the exercises, the number
  format).
- **Transforms keep what was drawn**: Scale by a negative factor is a half turn (arcs keep
  their sweep), not a mirror; a mirrored linear dimension stays on its side of what it measures
  and a strip along a mirrored path changes side with it; a horizontal or vertical dimension
  turned by a quarter turn becomes the other, by any other angle aligned.
- **Picked where drawn**: a dimension's text is picked on the side it is drawn (above the line as
  read, whichever way it runs); dragging a dimension line between its two points leaves it at the
  farther one. An entity on an unknown layer is drawn and picked as layer `0`'s. A **text** is
  picked anywhere in its box (`Drawing::text_box`, turned with the text; a window takes it only
  whole): its width as the editor measured it in the font it draws it in, its placeholders
  filled (`Rendered::texts` gives each text as shown, `Drawing::text_widths` keeps the measures,
  never saved), else (headless: CLI, MCP) estimated from the characters, a placeholder counted as
  what it shows; a
  line's, arc's or polyline's **arrowheads** anywhere inside them, a hatch anywhere inside its
  outer boundary.
- **One set of limits**: text heights 0.5–50 mm on reading, in Properties and in the style; ids
  and the references to them (`along`, `boundary`) are cut at 64 characters alike. A steep
  dimension with its style's aligned texts off keeps its second text under the first. A level
  mark's line runs the length of its text.
- **Random edits** (`tests_fuzz.rs`, `CAD_FUZZ=n` for more): every edit on random drawings with
  huge, tiny and degenerate numbers; none panics, and each result saves, reopens and saves to the
  same bytes.

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
- **Dimension texts**: aligned with the dimension line at any slope (ISO 129), turned to read
  from the bottom or the right, never upside down; an angle's runs along its arc. This is the
  style's **aligned texts** (`dimStyle.aligned`, on by default, written only when off); off, a
  dimension's text runs along its line up to 45° and a steeper one's is set level beside the
  line, on the outer side, and an angle's reads level outside its arc.
- **JSON and SVG**: the drawing is managed by its JSON; its SVG picture is made automatically
  from it, only when what it shows changes (or when a file brings none). Saving writes a picture
  (`pic:pic`) of the SVG (`word/media/cadN.svg`, with Office's SVG picture extension, so other
  programs show the drawing) whose properties carry the source part (`word/strix/cadN.json`,
  extension `urn:io.github.nabre.strix:cad`): Strix reads and edits the JSON, others show the
  picture. Files from the first version (the JSON inline in `a:graphicData`) still open.
- **Padding**: the picture's space around the drawing, paper mm (`"padding": 2` or
  `[top, right, bottom, left]`, 0–100), set in the editor (Picture › Padding, `ui.cadPadding`).
- **CAD Picture Format** (the contextual tab of a selected drawing): Edit Drawing; **Scale**, a
  fixed standard one (`1:50`, `2:1`…) or **Fit to Page** — the largest standard scale (1, 2,
  2.5, 5 × 10ⁿ) at which the drawing, its dimensions and padding included, fits a space (by
  default the page's text width and two thirds of its height; width and height can be set, 0
  leaves a side free). The fit is kept in the drawing (`fit`) and worked out at every
  recalculation, so a drawing that grows with `L` takes the next scale; the scale it is drawn at
  is written back to its source. Padding (`cad.padding`), alt text and Arrange (position, wrap,
  rotate, align — written back with its frame on save); its size is its scale's.
- **Maximum size and the scales that fit** (Picture Format › Scale and CAD Output › Scale, the
  same fields; `cad.scale` / `ui.cadScale` `{"max": true | false | {"width", "height"}, "auto":
  true, "scale": n}`): a maximum size on paper (from the page's text area, or typed; 0 leaves a
  side free) limits the drawing. The Scale list then offers **Fitted to Frame** (the largest
  standard scale that fits, following the calculations) and only the standard scales at which
  the drawing fits; a scale picked among them is kept (`fit.fixed`) while it still fits, one that
  does not is refused. A smaller maximum size the picked scale no longer meets goes back to the
  fitted one. Without a maximum size every standard scale is offered. Old drawings with a `fit`
  read as fitted.
- **Completion drawings (`answer` on layers and entities, `F6`)**: any layer (`layer.answer: true`,
  the **Answer** column in the Layer Manager) or individual entity (`entity.answer: true`,
  **Solution only** in Properties) can be marked as belonging to the solution. Bounds and `Fit`
  scales are always computed over the **entire** drawing (`Drawing::render_for(ws, screen,
  show_answers)`), while hiding an exercise's solution (`exercise.show {"solution": false}`) omits
  `answer` primitives and keeps the exact same frame `(width, height)`, origin and scale — leaving
  students the calibrated base geometry to complete by hand without shifting pagination. In the CAD
  editor, **Student Preview** (`Layers › Student Preview`, `ui.cadToggle {"what": "student"}`)
  previews the student sheet live.
- **Commands**: `cad.scale` (`{"scale": n}` or `{"fit": true | false | {"width", "height"}}`),
  `cad.padding`, `insert.cad`, `cad.get` (source and failing expressions of the selected
  drawing), `cad.set` (replace its source), `cad.add` (add an entity; returns its id).

## Stages

1. **Model, parametric coordinates, labels from parameters, rendering, point entry.** *Done
   (crate).*
2. **Document integration**: `insert.cad`, `cad.get/set/add`, regeneration in reading order with
   the calculations, `.xmath` round trip, units in the Unit style. *Done.* Rotated texts and dashed
   lines in the drawn items done (2026-10-28). Still to do: a picture for Word.
3. **Editor** (`crates/ui-egui/src/cad_editor.rs`). *First cut done*: double-click a drawing, or
   Math › Drawing › Edit Drawing (`ui.cadEdit`): the page gives way to the drawing's sheet and the
   **CAD Drawing** tab comes up (Draw: Line, Circle, Polyline, Rectangle, Arc, Point, Text;
   Dimensions: Aligned, Horizontal, Vertical; Modify: Select, Move, Copy, Delete, Undo, Redo;
   Precision: Snap, Ortho, Grid, Zoom Fit; Close Drawing). Wheel zooms about the pointer, the
   middle button or Alt+drag pans; a grid; a crosshair with snap markers (ends, midpoints,
   centres, grid). The command line takes `x,y`, `@dx,dy`, `@d<a`, `d<a`, a distance towards the
   pointer, a tool alias (`l pl rec c a t m co`), `c` to close a polyline, a radius; Enter ends,
   Esc cancels the step then the command, F8 toggles Ortho, Del deletes; Enter or Space at the
   idle prompt repeats the last command. Every finished operation
   is one `cad.set` (an undo step); the editor reads the drawing back after Undo or a
   recalculation. Moving keeps a coordinate's expression (`L` → `L + 0.5`).
   **Object snaps** (exact, from the kernel; AutoCAD's markers and names): Endpoint, Midpoint,
   Centre, Node, Quadrant, Intersection, Perpendicular and Tangent from the last point, Nearest
   (only when nothing else); the nearest wins, ties to that order; each toggles with its icon in
   the status line (`ui.cadToggle {"what": "snap.<kind>"}`); Ortho holds a point only when no object snap does.
   The status line shows distance, angle, Δx and Δy from the last point.
   **Grips** (AutoCAD and Allplan): the selected objects (up to 50) show square grips; a click
   takes one (hot, red), the next point places it (a click with snaps and Ortho, or a typed point:
   `@dx,dy` is from where it was), Esc puts it back; dragging a grip places it where released.
   Line ends and middle (moves the line), circle centre and quadrants (radius), arc centre, ends
   (angles) and middle (radius), polyline vertices and segment middles (a straight segment moves,
   an arc bends through the point), point and text position (and a text's four box corners, which
   move it), dimension ends and line (distance), and the blue reference between a dimension's
   (or angle's) scaled and annotative parts, in the middle of the stretches (`Grip::Reference`):
   dragging it moves the boundary, the dimension line or arc staying put (the annotative part
   within ±500 mm, never negative for an angle). A selected hatch (a solid fill too) or strip
   shows its boundaries' or path's grips with its own (`Drawing::grip_targets`): dragging them
   reshapes what follows them. A hatch moved, turned, scaled, mirrored or copied on its own takes
   its boundaries along.
   A dragged coordinate keeps its expression; a degenerate result is refused.
   **Frame** (Picture › Frame, on by default; turned off it stays off for the next drawing
   while the program runs): the picture's edge with the area outside it shaded light grey.
   **Selection** (AutoCAD's defaults, from the owner's prototype): a click **adds** the object
   under the pointer, Shift+click removes it (the halo under the pointer turns red); a click on
   empty space starts a **two-click window** (to the right: whole objects, blue; to the left:
   touched too, green dashed); a **drag** from empty space is a **lasso**, its first move decides
   window or crossing; Esc clears the selection last (after a polygon, a window, Remove mode, a
   grip, the points and the command). Typed options (English and Italian): `all`/`t`, `f`/`i`
   fence, `wp`/`fp` window polygon, `cp`/`ip` crossing polygon, `r` remove mode, `a` add mode,
   `p` the previous selection, `u` undo the last selection step (at the idle prompt `u` undoes the
   last command). A fence or polygon closes with Enter or a right-click. Overlapping objects:
   Shift+Space steps to the next one under the pointer (shown haloed), and a second click in the
   same place swaps the one just added for the next one. The command line reports
   "3 found · 5 selected". Hidden layers are not selectable. Move and Copy take the selection
   (noun-verb) or, without one, ask for objects first (Enter confirms). The cursor tells the state:
   idle, crosshair with the pickbox; a point asked for, the crosshair; objects asked for, the
   pickbox. `ui.cadSelect` takes `{"all": true}`, `{"ids": […]}`, `{"similar": true}`,
   `{"none": true}` or `{"option": "previous"}`.
   **Context menu** (right button, AutoCAD-style; items are the editor's own operations): with a
   grip taken, put it back and a snap for the next point; during a command, Enter (or Confirm the
   objects), Cancel the step or Exit, the polyline's Close and Undo the last vertex, the
   selection options or a snap for the next point, Ortho; with objects selected, Delete, Move,
   Copy, Draw order (to front, to back, forward, backward), Move to layer, Select Similar (same
   type, same layer), Deselect All, Properties; on the empty sheet, Repeat the last command,
   Undo, Redo, Select All, the selection options, Zoom Fit, the object snaps, Snap, Ortho, Grid,
   Properties, Close Drawing. With nothing selected, a right-click on an object selects it first.
   Shift+right-click while a point is asked for gives the **snap for the next point** menu
   (`ui.cadSnapOnce`: one snap kind, or none, for that point only).
   **Properties pane** (beside the sheet; Layers › Properties, `ui.cadToggle {"what": "props"}`):
   with several objects, **Considering** all of them or one family ("Line (3)"), ‹ › step through
   them one by one (back to all at the end), the counter goes back to the family, **Only these**
   reduces the selection to them (the old one becomes Previous); the sheet shows them in orange.
   General: layer, colour, lineweight, linetype, each ByLayer or the objects' own, "Varies" when
   they differ (`ui.cadLayer`, `ui.cadStroke`). Geometry of one object: coordinates, radii and
   angles take a number or an expression of the calculations (`L/2` stays linked); a line's length
   and angle and a circle's diameter recompute it (a diameter expression `D` gives `r = (D)/2`);
   a polyline shows one vertex at a time and Closed; a text its value, height and rotation; a
   dimension its text, distance and orientation (`ui.cadProps {"field", "value"}`, `strix_cad`'s
   `Drawing::fields`/`set_field`).
   **Layer Manager** (docked under the sheet; Layers › Layer Manager, `ui.cadToggle {"what":
   "layers"}`): one row per layer, `0` first then by name: current (new objects go on it,
   `ui.cadCurrentLayer`, also in the ribbon), on, locked, printed, colour, lineweight, linetype,
   object count, rename (objects follow), delete (objects go to layer 0); New Layer, Make Object's
   Layer Current. Colours: 23 pens (fifteen colours and a grey scale from black to white), any
   RGB (`r, g, b`) or `#rrggbb`.
   **Panes**: Properties and the Layer Manager each have a header with Detach (a window of its
   own, moved by its background, resizable; `ui.cadToggle {"what": "props.float"}` /
   `"layers.float"`), Dock and Close (×); docked or detached, they stay so for the next drawing.
   **Tabs**: the editor has its own tabs — CAD Drawing (Draw, Modify, Layers), CAD Annotate
   (Dimensions, Styles, Units, Hatch, Text), CAD Output (Scale, Picture) — each with Close
   Drawing; the document's tabs are disabled until the drawing is closed, and the document's
   task panes (Styles, Navigation, Format Shape…) are hidden while it is open (they come back as
   they were when it closes). In CAD Drawing, **Line** is a split button as Circle is (Line,
   Tangent Line: the top part starts the one chosen last), and Trim/Extend, Fillet/Chamfer,
   Array/Polar Array and **Explode/Join** are small split buttons, each pair one icon with its
   arrow; **Match** Properties is a large button in the Layers group, beside the Layer Manager;
   there are no Undo and Redo buttons (the title bar and Ctrl+Z / Ctrl+Y have them) and no
   Select button (the editor selects whenever no command runs; Esc ends one). There is no CAD
   View tab: the **status line** under the sheet holds, as icons without labels (names and state
   in tooltips, a pressed look when on), Snap, Grid and Ortho, the nine object snaps (Endpoint,
   Midpoint, Centre, Intersection, Perpendicular, Tangent, Quadrant, Node, Nearest) and Zoom Fit
   — the same `ui.cadToggle {"what": …}` and `ui.cadFit` as before; the context menu has them
   too. The choices of the paired buttons, the snaps (all on by default, Quadrant too), Ortho,
   Grid (off by default), Frame, the panes and the last distances and counts are the editor's
   settings, saved with the app's (`ui.json`, `cad`): every drawing opens as the last was left,
   after a restart too (a saved Grid on stays on).
   **Style windows** (CAD Annotate › Styles): Dimension Style, Level Style and Text Style each
   open a modal window (`ui.cadStyleDialog {"which": "dim" | "level" | "text"}`; scripts set the
   same things with `ui.cadDimStyle`, `ui.cadTextStyle` and `ui.cadStyles`, no window) with the
   style list (current, rename, New, Delete) and that style's settings; the ribbon keeps the
   current dimension and text styles. **Show the unit** (`showUnit`, Dimension Style) ends
   linear sizes with their unit (`3.50 m`, the unit of the style's Dimension units; SIA 400: m or
   cm as shown); off by default, as before. Level marks take the dimension styles; the Level
   Style window sets their **text height** (`levelText`, 0: the dimension text's), **triangle
   height** (`levelMark`, 0: a text and a fifth) and **Show the unit** (`levelUnit`: `+3.00 m`),
   all written only when set. **Text grips**: a selected dimension or level mark has a grip on
   its text; dragged anywhere, the text moves (`textShift: [dx, dy]`, drawing units: in scale,
   it follows the drawing; converted with the unit), with a thin leader back to the dimension
   line's middle or the level mark's top when it is more than a text height away; the measured
   value never changes; one undo step. `ui.cadTextShift {"dx", "dy"}` sets it on the selected
   dimensions and level marks (`{"reset": true}` puts the texts back); shifts are kept within
   the coordinates' limits and only on dimensions and level marks.
   **Slope ticks** (barbette, the crest of a cut; tool `slope`, aliases `barbette`, `ciglio`;
   CAD Annotate › Hatch › Slope Ticks): picked as a hatched strip (the line or polyline of the
   crest, then the side the slope goes down), a strip with `pattern: "slope"` — ticks square to
   the path, long (`width`, paper mm) and short (`ratio` of it, 0.1–1, a half by default) in
   turn, `spacing` apart, at most 20 000 (a too fine spacing on a long path is widened). They
   follow their path, render on the page, in the PDF and the SVG like any strip, and are saved
   with the drawing. **Flip Side** (`ui.cadFlip {"strips": true}`) puts the selected strips and
   slope ticks on the other side of their line; plain `ui.cadFlip` flips the selected
   dimensions, level marks and strips together.
   **Underlay** (CAD Drawing › Underlay): another drawing of the document shown under the open
   one, faded and locked, to trace over — its objects are never picked or changed, but the object
   snaps find its geometry. The button's menu lists the document's other drawings (body
   paragraphs, reading order: their alternative text or "Drawing n", and page; `cad.drawings`);
   picking one takes it (`cad.underlay {"source": n | id}`; `ui.cadUnderlay` in the editor) and
   merges its layers (colour, lineweight, linetype) and named dimension, level and text styles
   into the drawing: a name the drawing has keeps its own definition, new ones are added
   (`Drawing::inherit`; there are no drawing-level hatch or strip settings to merge: those are
   each strip's own). **Import styles and layers only** (`cad.inherit {"source"}`) merges without
   an underlay, and `insert.cad {"underlay": n}` / `{"inherit": n}` starts a new drawing from
   another. A drawing can be given a **name**, a slug (`Pianta piano terra` → `pianta-piano-terra`;
   Properties pane › Name, `cad.name {"name"}`, `ui.cadName` in the editor): it becomes the id,
   must be unique in the document, is listed in the underlay menu in place of the alternative
   text, and the drawings using it as their underlay follow the new name (one undo step).
   The underlay is a reference by the source's `id` (`d1`, `d2`…, given when first
   needed), never a copy: later edits to the source show. It sits 1:1 in drawing units (converted
   to this drawing's unit) with an optional offset (`dx`, `dy`), a fade (`fade`, 0–0.95, 0.6 by
   default), Show (the button) and Remove; by default it shows only in the editor, and **Print
   with the drawing** (`print`) draws it on the page and in the PDF too, the picture growing to
   hold it. A drawing never takes itself, two drawings never take each other, and an underlay's
   own underlay is never drawn (one level deep); at most 5000 of its objects are drawn and snapped
   to. A source deleted or unreadable shows nothing and says so on the status bar.
   **Space here**: a drawing knows the space where it sits — its column's width and the height
   from its top down to the page's bottom margin (`cad.space`, from the layout) — a guide, not a
   limit: a new drawing takes it as its maximum size (the scale fitted to it), Maximum size and
   `cad.scale {"fit": true}` use it, the scale fields show it with **Fit Here**, and the editor
   draws it as a dashed frame from the picture's top left. The **Scale** field takes a scale typed
   by hand (`1:75`, `1/75`, `75`, `2:1`, a decimal comma; 1000:1 to 1:1 000 000) besides the
   list; `cad.scale` and `ui.cadScale` take `"scale": n` or the text.
   **Annotations** (CAD Annotate tab): the drawing's **dimension style** (`dimStyle`, paper mm:
   text height — also new texts' —, end mark tick/arrow/dot/none, mark size, extension gap and
   overshoot, text gap; `ui.cadDimStyle`, one undo step, sizes clamped); a dimension may set its
   own end mark (`mark`, "By style" otherwise) and a **text below** its line (`below`, with `<>`
   and `{{L}}` like the text above). **Angular dimension** (`angle {c, a, b, r}`; tool
   `dimAngular`: the vertex, a point on each side, where the arc goes — the side it is placed in
   is the angle measured): arc with end marks, extension lines when the arc is beyond the sides,
   the angle (`<>°`) just outside the arc's middle, along the arc. A **right angle** (90° to a millionth
   of a degree: exact and parametric drawings) without a text of its own is drawn as the
   right-angle symbol instead: a square of side `r` on the vertex with a dot in it, no arc, no
   marks, no "90°"; a text of its own (`<>°`) keeps the arc. **Continue** and **Baseline**
   (`dimContinue dco quotaContinua`, `dimBaseline dba quotaBase`; `Drawing::dim_continue`,
   `dim_baseline`): from the one linear dimension selected, else the newest, each click adds a
   dimension of the same orientation and look — Continue from the last one's second point, its
   line on the last one's line (a chain); Baseline from the base's first point, each a row
   (`Drawing::dim_row`: 1.5 texts and a gap, paper mm) further out, away from the points. The
   first point keeps its expression (a chain from `L` starts at `L`). **Level marks**
   (`level {p, datum, text, flip, left}`; tool `level liv quotaLivello`, one click per mark, each
   like the last one: datum, side, way up): a triangle standing on the level, half filled on the
   text side, a short line through its tip and a line along its top under the elevation —
   `p`'s height above `datum` (the ±0.00 level, drawing units, an expression too) in metres, two
   decimals, signed (`+3.00`, `−1.50`, `±0.00`), with the document's decimal comma when it uses
   one; `text` takes `<>` and `{{L}}` as a dimension's. `flip` hangs it under the level (a
   ceiling, a soffit), `left` puts the text on the left. Moving, scaling or stretching moves its
   point, so the elevation follows; the datum stays. **Flip** (`ui.cadFlip`, Dimensions › Flip
   and the right-click menu; `Drawing::flip_dims`): a linear dimension's line to the other side
   of its points, an angle's other side (360° − the angle), a level mark under or over its
   level; one undo step. Texts have an **alignment** (left,
   centre, right on their point). Arrowheads are filled paths (`Prim::Path { fill }`).
   **Scale, Rotate, Mirror, Offset** (CAD Drawing › Modify; `scale sc`, `rotate ro`, `mirror mi`,
   `offset o`): Scale and Rotate take the selection (or ask for objects), a base point, then a
   value typed (factor, degrees) or a point (that many units away, in that direction); Mirror makes
   a mirrored copy about two points. One affine transform (`Affine`, `Drawing::transform`):
   expressions stay linked (`L` scaled by 2 is `(L)*2`), radii scale, paper sizes (texts,
   dimension offsets, hatch spacings) do not; a mirror turns arcs, bulges and angles the other
   way. Offset: the distance typed (kept while the program runs), the object, the side; a line
   moves square to itself, circles and arcs stay concentric, a polyline's corners are mitred and
   its arcs stay concentric (bulges kept); it goes on for the next object.
   **Trim, Extend, Fillet, Chamfer** (CAD Drawing › Modify; `trim tr`, `extend ex`, `fillet f`,
   `chamfer cha`; `Drawing::trim`, `extend`, `fillet`, `chamfer`): AutoCAD's quick mode, every
   other object shown is a cutting or boundary edge, and the picks are taken as they are (no
   snaps). Trim takes away the piece picked between the nearest crossings on either side: a piece
   out of the middle leaves two objects (the second with a new id, after the first), a circle
   becomes an arc (it needs two crossings), a closed polyline an open one, arcs of polylines keep
   their bulges in proportion; an object nothing crosses goes. Extend runs the end nearer the pick
   to the first object ahead: a line along itself, an arc round its circle, a polyline's end
   segment (straight or arc). Both go on for the next pick. Fillet joins two lines with an arc of
   the radius typed (0, the default, makes the corner), Chamfer with a bevel the distance typed
   from the corner (equal on both); the parts picked are kept, each line trimmed or extended to
   the arc or bevel, which takes the first line's layer and stroke; the size is kept while the
   program runs. Ends an edit leaves keep their expressions; the ends it makes are numbers.
   **Explode, Join, Match Properties, Stretch, arrays** (CAD Drawing › Modify; `explode x`,
   `join j`, `matchProp ma`, `stretch s`, `arrayRect ar`, `arrayPolar arp`; `Drawing::explode`,
   `join`, `match_properties`, `stretch`, `array_rect`, `array_polar`): Explode and Join act on the
   selection at once (noun-verb, or the objects picked then Enter). Explode turns polylines and
   rectangles into their lines and arcs in place (a strip along one or a hatch bounded by it
   goes, having lost its shape). Join chains lines, arcs and open polylines meeting end to end, in
   any order and direction, into one polyline in place of the first (closed when the ends meet;
   arcs become bulges); objects off the chain stay. Match Properties takes a source object, then
   gives each object picked its layer, stroke and shared look (a text's height; a hatch's or
   strip's pattern, angle, spacing; a dimension's end mark) until Enter. Stretch takes a
   crossing window, a base point and a second point: points of lines, polylines and dimensions
   inside the window move (as Move moves them: `L` becomes `L + 5`), circles, points and texts
   move with their centre or point, an arc moves when it is inside whole. The rectangular array
   (Serie; `Drawing::array_series`, `Series::plan`, `crates/cad/src/series.rs`) copies the
   selection from a base point to an end point: a single row or column (`1x5`, `5x1`) runs along
   the line between them in any direction, more of each make a grid with the end point its far
   corner (columns along x, rows along y). It is spaced one of three ways (`Spacing`, typed `F`,
   `I`, `S` at the base point, or the array button's menu, `ui.cadArrayMode {"spacing": "fit" |
   "pitch" | "spread", "pitch"}`, kept in the settings as `arraySpacing`, `arrayPitch`): **fit**
   (the default) the counts typed (`4x3`, kept), the last item exactly on the end point; **exact
   spacing** (interasse rigoroso) the spacing typed (a length, `25` or `2.5cm`; typing one at
   the base point switches Fit to it) kept exactly, as many items as fit, the rest left over past
   the last; **spread** (spalmato) as few items as keep the spacing at most the one typed,
   spread evenly so the last is on the end point (100 within 30: four spaces of 25). The command
   line says what it came to (`5 items · spacing 25 mm`). The polar array copies it round a
   centre, the number of items typed (kept), each turned as it goes; at most `MAX_ARRAY` (1000)
   items and the drawing's entity limit.
   **Arrays are one object**: an array's originals and copies are one group (`Entity::group`,
   saved as `"group": "g1"`), a pseudo-block: a click, a window or any selection that takes one
   item takes them all (`Drawing::with_groups`), so they move, copy, rotate and delete together;
   a copy of an array is a group of its own; an array of part of a group makes a new group of
   what it copies. Groups nest: an array of arrays or of block insertions wraps them, and an
   entity's group is a path, outermost first (`"group": "g3/g1"`, at most 16 levels); picking
   goes by the outermost. Explode takes the outermost group apart, one level
   (`Drawing::ungroup`): the arrays and insertions inside stay whole, the items of a plain
   array are on their own again, unexploded (a polyline among them explodes with a second
   Explode); a block insertion taken apart loses its record.
   **Blocks** (`crates/cad/src/blocks.rs`, editor `cad_editor/blocks.rs`; CAD Drawing › Block ›
   Create Block / Insert Block, the selection's context menu, `B` and `I` at the idle prompt;
   `ui.cadBlock {"action": "create" | "insert" | "delete" | "list" | "dialog"}`): Create Block
   asks for a name, whether the block is **annotative**, the base point (picked in the drawing,
   or the selection's centre or lower left corner) and whether the selected objects become an
   insertion of it (on by default). The definition (`Drawing::blocks`: name, objects round the
   base point with their expressions worked out, unit, and `scale`, the drawing's `1:N₀` when it
   was made, the CAD drawing format's) is saved with the drawing. An insertion is stored expanded,
   its objects one group (picked whole, moved, copied, rotated, scaled, mirrored like an array;
   Explode makes them plain objects), with a record (`Drawing::inserts`: group, block, insertion
   point, factor, angle, `drawnAt`) that follows Move, Copy, Rotate, Scale and Mirror. A plain
   block keeps its size in the drawing (scale 1 proposed). An **annotative** block keeps its size
   on paper: Insert Block proposes the scale `N / N₀` (drawing at 1:100, block drawn at 1:50:
   2), shows the size it will print at, and lets it be changed; the insertion is scaled about its
   insertion point. When the drawing's scale changes (picked, typed or fitted to its maximum
   size) every annotative insertion is scaled again about its insertion point
   (`Drawing::sync_annotative`, run when the drawing is made safe and, for a scale only being
   tried, when it is drawn), so it prints the same size. Insert Block draws the block dashed at
   the pointer until the click. At most `MAX_BLOCKS` (256) blocks, holding the drawing's entity
   limit together; nested blocks are kept as their objects; an array of insertions is a plain
   group.
   **Lineweights on screen**: the sheet draws every line at its width on paper at the current
   zoom (`stroke_px`: a 0.5 mm line is twice a 0.25 mm one, as it prints), a 0.5 pt hairline at
   the thinnest.
   **Line samples** in the panes and menus (linetype, lineweight, a layer's preview) are drawn on
   the sheet's white, as on paper, so a black line reads in the dark theme too.
   **Hatch** (CAD Annotate › Hatch; `hatch {boundary, pattern, angle, spacing}`; tool `hatch`,
   aliases `h`, `bh`): a click inside a region fills it: the smallest closed object around the
   click (closed polyline, rectangle, circle) with the closed objects inside it as islands
   (even-odd). It refers to its boundaries by id, so it follows them; deleting one deletes it; it
   goes behind everything else. Conventional patterns, this project's own (`hatch.rs`): lines,
   cross, steel (pairs), dashed, solid (lines close and wide enough to cover, so islands and
   concave regions fill on every output); `spacing` paper mm (annotative). The hatched strip
   takes the same patterns (filling its band), or "along the path".
   **Hatched strip** (CAD Annotate › Hatch; `strip {along, width, side, pattern, angle,
   spacing}`): a band along a line, polyline, circle or arc (found by its id, so it follows it
   when it moves or the calculations redraw it), `width` paper mm wide on its left or right,
   filled solid or hatched with lines (or crossed) at `angle` to each segment, `spacing` paper mm
   apart square to them, running on from one segment to the next; corners mitred (at most four
   widths). Annotative: the same on paper at any scale. Copied with its path it follows the copy;
   deleting the path deletes it. Tool `strip`: a click on the path, a click on the side.
   **Circle ways** (Draw › Circle, a split button: its top part starts the way chosen last, its
   icon and tooltip showing which, centre and radius at first; its bottom part, Circle ▾, lists
   every way with its own icon, and the one picked becomes the top part's, kept while the app
   runs like the editor's other settings; `ui.cadTool` `circle`, `circleDiameter`, `circle2p`,
   `circle3p`, `circleTtr`, `circleTtt`, whose result's `circle` names the remembered way; typed
   `c`, then `2p`, `3p`, `ttr`, `ttt`, or `d` after the centre): centre and radius, centre and
   diameter, 2 points (a diameter), 3 points, Tan Tan
   Radius (two objects picked near where the circle should touch them, then the radius typed or
   given as a point from the second pick), Tan Tan Tan (three objects: Newton's method from the
   circle through the picks, each tangency on the side the pick shows). **Tangent Line**
   (`lineTan`): a line touching two circles or arcs, the outer or inner one nearest the picks.
   The geometry is the kernel's (`circle_2p`, `circle_3p`, `circle_ttr`, `circle_ttt`,
   `line_tan_tan`), with the next result previewed as the pointer moves.
   **Draw order** (`ui.cadOrder`, Modify › Bring to Front / Send to Back): the order of the objects
   in the JSON is the order they are drawn in; the selection follows the objects.
   Still to do: constraints, joining arcs and polylines in fillets.
4. Edit commands: move, copy, mirror, offset, trim, extend, fillet, join, explode, match
   properties; hatch (by selection and by point); angular dimensions, rows and chains.
5. Driving dimensions and geometric constraints bound to parameters (solver).

## Revision history

- 2026-11-14: a grip on the reference between a dimension's scaled and annotative parts (`Grip::Reference`, the line or arc staying put); the editor draws lines at their paper width at the zoom (it showed every lineweight about the same).
- 2026-11-13: completion drawings (`F6`): `answer` flag on layers and entities (`Layer Manager › Answer`, `Properties › Solution only`), pagination-invariant `Drawing::render_for` keeping exact bounds, origin and scale when an exercise hides its solution, and `Student Preview` (`ui.cadToggle {"what": "student"}`) on the CAD editor ribbon.
- 2026-11-12: blocks: Create Block (annotative or not, base point picked or the selection's centre or corner) and Insert Block (proposed scale `N / N₀` for annotative blocks, kept the same size on paper when the drawing's scale changes), from the ribbon's Block group, the context menu, `B`/`I` and `ui.cadBlock`.
- 2026-11-12: the rectangular array (Serie) from a base point to an end point, spaced to fit it, at an exact spacing (interasse rigoroso) or spread within one (spalmato), along a line in any direction or as a grid; arrays (rectangular, polar) are one object, a group picked whole, that Explode takes apart.

- 2026-11-12: texts picked anywhere in their box, with a grip at each corner (`Grip::Corner`), the box measured by the editor in the text's own font (`Rendered::texts`, `TextWidths`); arrowheads picked inside; a selected hatch shows its boundaries' grips (a strip its path's), and a hatch moved, turned or copied on its own takes its boundaries along.

- 2026-11-11: drawing names as slugs (`cad.name`, `ui.cadName`, Properties › Name; underlays follow a rename); Arc split button with its ways (`arc`, `arc3p`, `arcSce`, `arcSer`; a typed radius for Start, End, Radius; the remembered way in the settings as `arcMode`), with a preview of the arc; Polyline in the Line split button (`picks.polyline`), Text out of the Draw group; drawings in millimetres: a drawing in centimetres or metres reaches the editor converted, and the document takes it with the first edit (`Drawing::to_mm`: lengths and the underlay's offset converted, dimension styles that showed the drawing's unit show the old one, expressions keep meaning the old unit through `exprUnit`), CAD Annotate › Units keeps only the input unit, the Properties pane shows the unit without a choice; the status line's Snap arrow menu (each object snap, `ui.cadToggle {"what": "snaps.on" | "snaps.off"}`) replaces the nine object snap icons; Properties with nothing selected: the scale typed or picked (standard scales that fit, Fitted to Frame); a new Offset icon (a rounded corner and its parallel copy).

- 2026-11-10: underlays: another drawing of the document under this one, faded, locked and snapped to, by id (`cad.drawings`, `cad.underlay`, `underlay`), optionally printed; its layers and styles inherited (`cad.inherit`, `insert.cad {"underlay" | "inherit"}`); no self-reference, no pair, one level deep.

- 2026-11-10: style windows for the dimension, level and text styles; Show the unit in dimension and level texts; level text and triangle heights; text grips (`textShift`, `ui.cadTextShift`) with leaders; slope ticks (`pattern: "slope"`, tool `slope`/`barbette`) and Flip Side for strips; `cad.space` (the space where a drawing sits), used for new drawings, Maximum size and Fit Here, drawn as a guide; scales typed by hand; Snap, Grid, Ortho, the object snaps and Zoom Fit as icons in the status line, the CAD View tab removed; Match large beside the Layer Manager, Explode/Join one split button, Undo/Redo out of the ribbon; the document's panes hidden while a drawing is open; Grid off by default.

- 2026-11-09: named text styles (font, height, bold, italic) and dimension styles (start and end marks, mark width, text style), with the current ones new objects take; Properties › Style; the drawing's unit in CAD Annotate › Units (converted or kept); CAD Drawing: Line and Tangent Line one split button, Trim/Extend, Fillet/Chamfer and Array/Polar small split buttons, no Select button; the editor's settings saved between runs; Quadrant snap on by default.

- 2026-11-09: arrowheads on open lines, arcs and polylines (`arrows`, Properties › Arrows); arrowheads 30° across (ISO 129-1); a negative scale is a half turn; mirrored and rotated dimensions and strips stay where drawn; dimension texts picked where drawn; text heights to 50 mm; long ids keep their strips; unknown layers are layer 0; `cad.get` reports the errors where the drawing sits; an unreadable drawing is never replaced by a blank one; `cad.padding` refuses a malformed list; a drawing redraws when the calculation it reads is deleted; the editor shows units in the Unit style's colour; the template's beam has load arrows, hatched ground and dimensions bound to `L` and `q`.
- 2026-11-08: the "Calculation and exercise" template has a beam drawing sized by its exercise's data (docs/exercise.md).
- 2026-11-08: dimension texts aligned with their line by default, steep ones too (`dimStyle.aligned` on unless a drawing turns it off).
- 2026-11-08: past the render budget a polyline keeps its vertices; the SVG picture and page items are thinned past 500 000 points.
- 2026-11-08: saved drawings save to the same bytes when read back (a datum too small to write); Copy and arrays add their copies at once (an array filling the drawing: 5.7 s → under a millisecond); the editor's resolved drawing is kept between frames and worked out only up to the drawing; SVG numbers written without a string each.
- 2026-11-07: one render budget a drawing (hostile files no longer take gigabytes); saved drawings always reopen (a dragged dimension could save a `null` distance); hatches are one SVG path and one page item each run.
- 2026-10-29: Circle is a split button: the top part starts the way chosen last, the menu lists the ways with their icons.
- 2026-10-29: Frame on by default; the editor shows a dimension's scaled and annotative parts (reference line or arc, coloured stretches, sizes) for the selected dimension and while placing or gripping one; Properties labels and colours the two parts alike.
- 2026-10-28: input unit (typed values in mm, cm or m, or with their own unit); default dimension style after ISO 129 (thin lines, heavier ticks, line past the extension lines).
- 2026-10-28: dimension units: the drawing's, m, cm, mm or SIA 400.
- 2026-10-28: parameters in dimension, angle, level and text annotations set as equations (italic symbol, lowered subscript, upright value).
- 2026-10-28: dimension distances and angle radii in two parts, scaled and annotative; Scaled distances in the style.
- 2026-10-28: right-angle symbol; Continue and Baseline dimensions; level marks; Flip.
- 2026-10-28: maximum size; the Scale list offers Fitted to Frame and the scales that fit (Picture Format, CAD Output).
- 2026-10-28: Explode, Join, Match Properties, Stretch, rectangular and polar arrays; line samples on the sheet's white.
- 2026-10-28: Trim and Extend (quick mode), Fillet and Chamfer of two lines.
- 2026-10-28: texts drawn at their angle on the page; dimension texts aligned with their line (`dimStyle.aligned`, readable); dash-double-dotted; dashes cut into the page's items.
- 2026-10-27: AutoCAD selection (adds, Shift removes, two-click window, lasso, fence, polygons, previous, undo, cycling); context menu; Properties pane (families, stepping, geometry with expressions, stroke, layers); draw order; current layer.
- 2026-10-27: geometry kernel (exact intersections, perpendiculars, tangents, angles; limit of approximation); exact object snaps; grips; frame; padding; JSON source with an automatic SVG picture.
- 2026-10-27: placeholders with a mode (value, symbol, both) in dimensions and texts; missing data in red.
- 2026-10-27: stage 3 first cut: the drawing editor.
- 2026-10-27: stage 2: drawings in the document, redrawn with the calculations.
- 2026-10-27: new: design and stage 1.

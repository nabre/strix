# CAD drawings

> **Status:** stages 1–2 done (model, parametric coordinates, rendering, point entry; drawings in the document, saved, redrawn with the calculations); stage 3 editor in progress · **Last updated:** 2026-10-28 · **Change:** input unit; cleaner default dimension style; dimension units

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
  paper, or in the drawing; the other part is kept. Properties shows both.
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
- **Dimension texts**: by default a dimension's text runs along its line up to 45° and a steeper
  one's is set level beside the line, on the outer side; an angle's reads level outside its arc
  (drawings look as they did). The style's **aligned texts** (`dimStyle.aligned`, written only
  when on) turn every dimension's text with its line, never upside down, and an angle's along
  its arc.
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
   (Dimensions, Text), CAD View (Precision, Object Snaps), CAD Output (Picture) — each with Close
   Drawing; the document's tabs are disabled until the drawing is closed.
   **Annotations** (CAD Annotate tab): the drawing's **dimension style** (`dimStyle`, paper mm:
   text height — also new texts' —, end mark tick/arrow/dot/none, mark size, extension gap and
   overshoot, text gap; `ui.cadDimStyle`, one undo step, sizes clamped); a dimension may set its
   own end mark (`mark`, "By style" otherwise) and a **text below** its line (`below`, with `<>`
   and `{{L}}` like the text above). **Angular dimension** (`angle {c, a, b, r}`; tool
   `dimAngular`: the vertex, a point on each side, where the arc goes — the side it is placed in
   is the angle measured): arc with end marks, extension lines when the arc is beyond the sides,
   the angle (`<>°`) level just outside the arc's middle. A **right angle** (90° to a millionth
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
   copies the selection in columns × rows (typed `4x3`, kept) a base point and a spacing point
   apart; the polar array copies it round a centre, the number of items typed (kept), each turned
   as it goes; at most `MAX_ARRAY` (1000) items and the drawing's entity limit.
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
   **Circle ways** (Draw › Circle menu; typed `c`, then `2p`, `3p`, `ttr`, `ttt`, or `d` after the
   centre): centre and radius, centre and diameter, 2 points (a diameter), 3 points, Tan Tan
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

# CAD parity with the owner's prototype

> **Status:** plan · **Last updated:** 2026-10-28 · **Change:** new: every CAD feature of the
> prototype (`old_strix`, Python/Typst/TypeScript), with the owner's requests, ordered

The CAD drawing editor (`docs/cad.md`) takes over every feature of the owner's earlier prototype,
re-designed for Strix: one Rust model and kernel (expression-valued coordinates, exact
geometry), the editor's ribbon tabs, command ids and aliases following CADCraft's (AutoCAD's)
names, and algorithms ported from CADCraft where it is broader (MIT OR Apache-2.0; ported files
carry its notice, `NOTICE` lists them). Status: **done**, **partial**, **todo**.

## 1. Model

| Feature (prototype) | Strix | Status |
|---|---|---|
| Unit mm / cm / m, scale 1:N, y up, degrees | same | done |
| Fit: max width/height on paper, standard scale series | `fit`, Fit to Page | done |
| Layers: colour, width, dash, visible, locked, printable; layer 0 | same, Layer Manager | done |
| Dash types incl. dash-double-dotted | 4 of 5 | partial |
| Per-entity stroke, ByLayer | same | done |
| Entity names (`name`, used by calculations: `area(name)`, `length()`) | — | todo |
| Line, circle, arc, point, polyline with bulges | same | done |
| **Point style** (owner): how points show — dot, cross, x, circle, square and combinations; size on paper (annotative) or in drawing units | dot only | todo |
| Text: multiline, align, base point (9), reach, offset | single line, align | partial |
| Hatch: loops with islands, solid / lines / cross, angle, spacing (even-odd) | hatch by boundary ids; lines, cross, steel, dashed, solid | done |
| Level (elevation mark): value, auto, datum, flip, side | — | todo |
| Linear dimension: marks per end, text above/below, `{{row}}`, associative ends, driving value, chains/levels | marks, below, `{{L}}` | partial |
| Angular dimension: right-angle symbol, vertex mark | basic | partial |
| Dimension extension: to point or fixed length (`dim_ext`) | gap/overshoot | partial |
| Constraints (geometric, dimensional) and solver; driving dimensions | — | todo |
| Xrefs (other drawings, fade, layer rules) | — | todo |
| Measures in SI for calculations (length, area, angle, radius) | — | todo |

## 2. Editor commands

| Feature | Status |
|---|---|
| Line (incl. tangent to two curves), polyline (`a` arc, `l` line, `c` close, `u` undo vertex), rectangle | partial (polyline arc mode, undo vertex) |
| Circle: centre-radius, centre-diameter, 2P, 3P, TTR, TTT | done |
| Arc, point, text | done |
| Hatch: pick inside a region (islands), or selected closed outlines; sent to back | done (pick inside) |
| Hatched strip along a line/polyline, then a conventional hatch fill with the same settings | done |
| Dimensions: aligned, linear-auto, horizontal, vertical; angular; continue/chain; level; flip; group into levels; unlink; extension mode; mark style | partial |
| Copy, move, mirror, stretch, **scale** (owner), rotate | done but stretch |
| Offset (line, circle, arc, polyline; distance or through point) | done (distance) |
| Fillet, trim, extend, join, explode | todo |
| Match properties; draw order | partial (draw order) |
| Grips per type; dimension grips | done |
| Snaps: end, mid, centre, geometric centre, quadrant, node, intersection, perpendicular, tangent, nearest, grid | partial (geometric centre, grid snap) |
| Point aids: snap override, temporary track, From, M2P, .X/.Y filters | partial (override) |
| Command line: `x,y`, `@dx,dy`, `@d<a`, direct distance, `<angle` lock | partial (angle lock) |
| **Input units** (owner: as in the prototype): typed values with a unit (`2.5 m`, `300 mm`), the drawing's unit shown, unit options | todo |
| Ortho F8, grid step, zoom, pan, fit | done |
| Selection: click/Shift, window/crossing, lasso, T, I, FP, IP, R, A, P, U, Ctrl+A, Shift+Space | done |
| **Dimension selection** (owner: improve): pick on the drawn dimension line, extension lines and text | todo |
| Undo/redo, repeat (F4/Enter), Esc unwinding | done (F4 todo) |
| Properties: exact fields, SI measures, vertex navigator, all dimension/level/text/hatch fields, multi-select scope | partial |
| **Annotative vs scaled** (owner): show which sizes are paper (annotative) and which are drawing units (scaled) | todo |
| Ω symbols / variables inserter for texts | todo |
| **Right-click menu** (owner): richer, with icons | todo |
| Layer panel dockable or floating; ribbon layer group | partial (docked) |
| Dynamic tab: constraint tools, auto-constrain, show, panel, relations toggle | todo |

## 3. Output

| Feature | Status |
|---|---|
| Rendered in the document (page, PDF, SVG picture) | done |
| **CAD Output tab** (owner): padding, graphic scale, maximum drawing size from the page | partial (padding) |
| Non-printing layers left out of the page and PDF | done |
| Empty drawing placeholder | todo |
| Scale and final size from the picture's context menu | partial (Picture Format) |

## Order of work

1. Owner's open requests: hatch command with conventional patterns (and the strip filled with
   them), Scale, dimension selection, input units, CAD Output tab (scale, maximum size),
   annotative/scaled marking, right-click menu with icons.
2. Modify commands: mirror, rotate, stretch, offset, trim, extend, fillet, join, explode, match
   properties (ported from CADCraft where it applies).
3. Dimensions: linear-auto, continue/chain, levels, flip, extension modes, associative ends,
   right-angle symbol; level marks.
4. Text: multiline, base points; symbols inserter.
5. Snaps and point aids: geometric centre, grid snap, tracking, From, M2P, filters, angle lock.
6. Entity names and measures for the calculations.
7. Constraints and driving dimensions (solver); xrefs.

## Revision history

- 2026-10-28: new: the prototype's CAD features and the owner's requests, ordered.

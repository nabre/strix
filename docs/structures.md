# Plane structures (`strix-structure`)

> **Status:** stage 1 done (solver, structure object, diagrams, commands); stage 2 (Structures tab, editor and tables) and stage 3 (step-by-step working, exercise variants in the editor) next · **Last updated:** 2026-11-17 · **Crates:** `crates/structure` (`L1`), `crates/engine/src/cmd/structure.rs` (`L4`)

Strix documents hold plane structures — beams, frames and trusses — that are solved where they sit and drawn with their reactions and their N, V (T) and M diagrams one under the other, at the same horizontal scale, as textbooks draw them. Lengths and loads may be calculation parameters (`L`, `q`), so a structure follows the calculations before it and each exercise variant has its own numbers and diagrams. This is a Strix feature beyond Word parity; the model is the one of 2D frame analysis tools such as structural-analyser.com, drawn with the conventions of Italian structural mechanics texts.

## Model (`crates/structure`, L1, depends on `strix-calc` and `strix-plot`)

A `Structure` (JSON, camelCase) has:

- **Units:** `lengthUnit` (`m` default, `cm`, `mm`) and `forceUnit` (`kN` default, `N`, `daN`); moments are force × length, distributed loads force / length. Coordinates are `y` up, angles in degrees counter-clockwise.
- **Nodes** `{id, x, y, hinge?}`: `hinge` makes every member end meeting there free to turn (an internal hinge, Gerber beams).
- **Members** `{id, a, b, hingeA?, hingeB?, e?, i?, area?}`: the local axis `x` goes from `a` to `b`, `y` is turned 90° counter-clockwise; the **reference fibres** are on the `−y` side (below a member drawn left to right, on the right of a column drawn upwards). `hingeA`/`hingeB` release one end. Without `e`, `i`, `area`: `EI = 1` (constant along the structure) and the member does not stretch (`EA = 10⁶·EI/ℓ²`, `ℓ` the longest member), which is what textbook hyperstatic frames assume; isostatic results never depend on stiffness.
- **Supports** `{node, kind, angle?}`: `fixed` (incastro), `pin` (cerniera), `roller` (carrello), `link` (pendolo: restrains like a roller, drawn as a bar), `slider` (glifo / bipendolo: the translation across its plane and the rotation). `angle` turns the support about its node: at 0 it stands on the ground below the node, at 90 it is on a wall to its right, at −90 on a wall to its left (a cantilever's fixed end).
- **Loads** (`type`):
  - `force {node | member + at, value, angle}` — `angle` −90 (down) by default; `at` from the member's `a` end;
  - `moment {node | member + at, value}` — counter-clockwise positive;
  - `distributed {member, q1, q2?, from?, to?, angle?, axes?, per?}` — linear from `q1` to `q2` (uniform, triangular, trapezoidal) over `[from, to]` (the whole member by default); `axes: "global"` (default; `angle` from the horizontal) or `"local"` (`angle` from the member's axis, −90 towards the reference fibres: wind on a column); `per: "length"` (default, per metre of member) or `"projection"` (per metre of the member's projection across the load: snow on a rafter).
- **Every number is a `Num`**: a JSON number, or an expression of the calculation parameters (`"L/2"`, `"q"`). A parameter with a unit converts to the structure's units (`L = 450 cm` is 4.5 in a metre structure, `q = 12 kN/m` is 12, `30°` is 30 degrees); a plain number counts in the structure's units; `L/2 - 1` mixes a length and a number, so lengths count as plain numbers in the structure's unit there, as coordinates do in a CAD drawing. A failed expression keeps its last value and is reported.
- **Diagrams** `{reactions, n, v, m, values, hatch, signs}`: each of `n`, `v`, `m` has `show`, `positive` (`up`: the member's `+y` side; `down`: the reference fibres' side) and `amplitude` (the largest value's height, paper mm, 12 by default). **Italian convention** (default, `structure.set {"convention": "italian"}`): N and V positive up, M positive down, so the moment is drawn on the stretched fibres. `titles {n, v, m}` name the panels (`N`, `T`, `M` by default; `V` for English texts).
- `title`, `width` (paper mm, 160), `answer` (true: reactions and diagrams are the answer, hidden in the student copy).

Sign conventions: N positive in tension; V positive when it turns the piece clockwise; M positive when it stretches the reference fibres.

## Analysis

- **Classification** by the rank of the equilibrium matrix (equations: two translations per node and the rotations; unknowns: three internal forces per member — N and the two end moments — and one per constraint): **isostatic**, **hyperstatic of degree h** (unknowns − rank), or **labile** (rank < equations, the number of independent mechanisms). Three hinges in a row, two parallel rollers, a couple on a node where every member is hinged: labile, whatever the count of constraints says.
- **Solution** by the stiffness method: rotations are one per node for the member ends rigidly joined there, one of its own for each hinged end; supports are constraints solved with Lagrange multipliers, which are the reactions. Member loads enter as consistent nodal loads (exact for a clamped beam), and the internal forces along each member follow by equilibrium of the piece from its `a` end, so a stretch's law is exact (polynomial), not sampled from the solver.
- **Truss:** when every member is hinged at both ends and carries no load along it, the structure is a truss: one panel shows each member's axial force (tension positive, coral; compression blue; zero members dashed grey) instead of the N, V, M diagrams.
- **Notable values** (`notables`): at the ends of each stretch (members' ends, loads' positions, with the jumps under concentrated loads), the extremes of M where V is zero, and the zeros of each diagram.

## Drawing

`Structure::render_for(ws, show_answers)` returns a `strix-plot` `PlotRendered`, so the engine draws it, writes its SVG and prints it as it does a plot. Panels, one under the other at one horizontal scale (the structure fitted to `width`, at most 70 mm tall):

1. the **structure** with its supports (Italian textbook symbols: hatched ground, rollers on wheels, links as bars with hinges, fixed ends hatched), hinges as white circles, node names away from the members, and its **loads** in red: arrows with their values, curved arrows for couples, distributed loads as rows of arrows proportional to the intensity with their envelope and values;
2. the **reactions** on the structure drawn in grey: components along the axes (along the support's direction for a roller, link or slider), arrows pointing the way the force acts, their magnitudes, curved arrows for moments;
3. **N**, **V (T)** and **M** (or the truss panel), each in its colour (blue, teal, plum), hatched across the axis, with ⊕/⊖ in each area and the notable values written at the curve.

A structure that is not solved (labile, invalid) shows the structure and a red line saying why. The student copy (`exercise.show` off, `answer` on) keeps every panel and the structure, with the reactions and diagrams left out, so both copies have the same size.

## In the document

A `GraphicKind::Structure` graphic holds the JSON source (`Graphic::structure`), its items and its SVG. It is recalculated in reading order with the calculations (as `Item::Plot { structure: true }` in `cmd/calc.rs`), against the worksheet it sees: the document's, or its exercise's sandbox with the active variant's data. DOCX: a picture of the SVG (`word/media/structureN.svg`) whose properties carry the source part (`word/strix/structureN.json`, relationship `urn:io.github.nabre.strix:2026/relationships/structure`, inline form `urn:io.github.nabre.strix:structure`); the Word envelope (`docs/word-round-trip.md`) keeps it through a save in Word. It counts among the Strix objects a foreign format loses.

## Commands

| Command | Does |
|---|---|
| `structure.insert {structure?, template?}` | inserts a structure at the caret: `beam` (default: simply supported, 6 m, 10 kN/m), `cantilever`, `continuous`, `frame` (portal), `truss` |
| `structure.get` | the source, `status` (`isostatic`, `hyperstatic`, `labile`, `invalid`, `empty`), `degree`, `reactions`, each member's notable N, V, M values, `errors` |
| `structure.set {structure?, title?, width?, lengthUnit?, forceUnit?, answer?, convention?, diagrams?, titles?}` | settings (`diagrams` and `titles` merged key by key) |
| `structure.node {id?, x?, y?, hinge?, rename?, remove?}` | adds (next free letter), changes, renames (everywhere) or removes a node with its members, supports and loads |
| `structure.member {id?, a?, b?, hingeA?, hingeB?, e?, i?, area?, remove?}` | adds, changes or removes a member (with its loads); `null` clears `e`, `i`, `area` |
| `structure.support {node, kind?, angle?, remove?}` | one support per node: puts, changes or removes it |
| `structure.load {index?, load?, remove?}` | adds, replaces or removes a load |

## Next

- **Stage 2:** the Structures tab (between Math and Teaching): insert from templates, the structure editor (drawing nodes and members with the CAD editor's snaps, grid and typed coordinates; supports, hinges and loads from the ribbon) beside tables of nodes, members, supports and loads that accept parameters; the diagrams' settings (positive side per diagram, convention, values, hatch, signs).
- **Stage 3:** the step-by-step working written after a structure as live calculations (equilibrium equations, reactions, the laws N(x), V(x), M(x) per stretch); exercise variants from the editor.

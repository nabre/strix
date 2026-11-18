# Plane structures (`strix-structure`)

> **Status:** done — stage 1 (solver, structure object, diagrams, commands), stage 2 (Structures tab, Structure Editor pane with its tables), stage 3 (the written working) · **Last updated:** 2026-11-17 · **Crates:** `crates/structure` (`L1`), `crates/structure/src/working.rs`, `crates/engine/src/cmd/structure.rs` and `cmd/structure/working.rs` (`L4`), `crates/ui-egui/src/structure_pane.rs`, `crates/ui-egui/src/ribbon/structures.rs` (`L6`)

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
- **Diagrams** `{reactions, n, v, m, values, hatch, signs, dimensions, stations}`: each of `n`, `v`, `m` has `show`, `positive` (`up`: the member's `+y` side; `down`: the reference fibres' side) and `amplitude` (the largest value's height, paper mm, 12 by default). **Italian convention** (default, `structure.set {"convention": "italian"}`): N and V positive up, M positive down, so the moment is drawn on the stretched fibres. `titles {n, v, m}` name the panels (`N`, `T`, `M` by default; `V` for English texts).
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

What the drawing measures (`crates/structure/src/render/annotate.rs`):

- **Dimensions** (`diagrams.dimensions`, on by default; Structures › Show › Dimensions), on the structure's panel: chains of distances in grey below the structure (x) — first with the points where loads act, start and end on level members, when they add any, then between the nodes, then overall when there are more than two stations — and at its right (y, with the loads on plumb members), texts along their lines; loads on an inclined member get a chain parallel to it on its reference fibres' side, distances measured along the member from its first node (as `at`, `from`, `to` are). Each **inclined member's slope** is marked once per value: an arc from a dashed horizontal at its lower end, with the angle. Each **inclined force** (or inclined distributed load) gets the angle it makes with its member: an arc from the member's side nearest the arrow's tail to the arrow, with the angle, in the loads' red; a force along or across its member, or at a node where it runs along or across a member, is evident and gets none.
- **Stations** (`diagrams.stations`, on by default; Structures › Show › Stations), on each diagram: a tick on each member's axis where a notable value is inside the member (a load's position, a jump, M's extreme, a zero), a dotted ordinate up to the curve, and the distance from the member's first node (`2.333 m`) written across the axis on the side away from the diagram; one too close to the last on paper keeps only its tick.
- **Values' direction:** a value that belongs to a point (a member's end, a jump, an extreme) is written across the member, standing on the curve there (vertical on a beam); a stretch of constant value keeps one level value at its middle. Rotated texts are `PlotPrim::TextAt` (centred, turned), drawn the same in the document, its SVG and the editor.

A structure that is not solved (labile, invalid) shows the structure and a red line saying why. The student copy (`exercise.show` off, `answer` on) keeps every panel and the structure, with the reactions and diagrams left out, so both copies have the same size.

## In the document

A `GraphicKind::Structure` graphic holds the JSON source (`Graphic::structure`), its items and its SVG. It is recalculated in reading order with the calculations (as `Item::Plot { structure: true }` in `cmd/calc.rs`), against the worksheet it sees: the document's, or its exercise's sandbox with the active variant's data. DOCX: a picture of the SVG (`word/media/structureN.svg`) whose properties carry the source part (`word/strix/structureN.json`, relationship `urn:io.github.nabre.strix:2026/relationships/structure`, inline form `urn:io.github.nabre.strix:structure`); the Word envelope (`docs/word-round-trip.md`) keeps it through a save in Word. It counts among the Strix objects a foreign format loses.

## Commands

| Command | Does |
|---|---|
| `structure.insert {structure?, template?, openPane?}` | inserts a structure at the caret: `beam` (default: simply supported, 6 m, 10 kN/m), `cantilever`, `continuous`, `frame` (portal), `truss`; `openPane` opens the Structure Editor |
| `structure.get` | the source and `resolved` (its numbers' values), `status` (`isostatic`, `hyperstatic`, `labile`, `invalid`, `empty`), `degree`, `message`, `truss`, `reactions`, each member's length and notable N, V, M values, `errors` |
| `structure.set {structure?, title?, width?, lengthUnit?, forceUnit?, answer?, convention?, diagrams?, titles?, layout?}` | settings (`diagrams` and `titles` merged key by key; `layout` `auto`, `column` or `grid`) |
| `structure.node {id?, x?, y?, hinge?, rename?, remove?}` | adds (next free letter), changes, renames (everywhere) or removes a node with its members, supports and loads |
| `structure.member {id?, a?, b?, hingeA?, hingeB?, e?, i?, area?, remove?}` | adds, changes or removes a member (with its loads); `null` clears `e`, `i`, `area` |
| `structure.support {node, kind?, angle?, remove?}` | one support per node: puts, changes or removes it |
| `structure.load {index?, load?, remove?}` | adds, replaces or removes a load |
| `structure.working {labels?}` | writes the solution after the structure as live calculations ([Working](#working)) |

## The Structures tab (Strutture)

Between Math and Teaching (`ribbon::TABS`; a saved ribbon layout gets it after Math). Its groups:

- **Structure:** Structure (a menu of the templates: Simply Supported Beam, Cantilever, Continuous Beam, Portal Frame, Truss; it opens the editor), **Edit Structure** (`ui.structureEdit`; Close Editor while it is open, `ui.structureClose`) and **Structure Editor** (`view.structurePane`).
- **Diagrams:** Reactions, Axial Force N, Shear T (the shear's title as the structure names it), Moment M, each shown or hidden; **Positive Side** (up or down for each of N, T, M); **Italian Convention**.
- **Show:** Values, Hatching, Signs, Dimensions, Stations; **Panel Layout** (Automatic — frames in two columns —, One Column, Two Columns); **Units** (m, cm, mm; kN, daN, N).
- **Teaching:** **Solution Only** (`answer`: reactions and diagrams hidden in the student copy).

Everything but Structure and the editor needs a structure selected (`structure.set` is enabled only then).

## The Structure Editor pane

A task pane (docked right, 440 pt) for the selected structure, every change made through the `structure.*` commands (so undoable and scriptable):

- **Status:** isostatic, hyperstatic of degree h, labile (with its degrees of freedom), a truss; the expressions that failed and the analysis' warnings.
- **Sheet:** the structure as the document draws it (`Structure::schema_prims`: the same symbols, loads and names) on a white sheet with a grid; the wheel zooms about the pointer, the middle or right button pans, **Fit** shows it whole. The pointer snaps to the nodes, and to the grid with **Snap** (its step in **Grid**); its coordinates are shown in the corner.
- **Full page:** a **double-click** on a structure in the document (or Edit Structure, or the pane's Edit in Full Page) opens the editor **in place of the page**, like the CAD editor: the sheet as large as the window with the tools above it and the typed point below, the selection's properties and the tables at the right, the Structures tab brought up. Close Editor, Esc with nothing selected, or a structure no longer selected (undone, deleted) goes back to the page and the tab that was open. The document's panes are hidden meanwhile.
- **Selection** (Select tool): a click picks a node, else a load (its arrow, its couple's circle, its band of arrows), else a member, and shows what the pointer is over; **Shift** or **Ctrl**+click adds or takes out; a drag from an empty place draws a **window** that takes the nodes in it, the members with both ends in it and the loads in it (Shift keeps what was selected); **Ctrl+A** selects every node and member. **Moving:** dragging a node or a member moves the whole selection (the selected nodes and the ends of the selected members) by a step of the grid when snapping, the Δx, Δy shown in the corner, as one undo step (`structure.set`); a coordinate written as an expression stays one (`L` → `L + 0.5`). Dragging a force or couple on a member slides it along the member (its `at`, on the grid). The **arrow keys** move the selection by the grid (Shift: ten steps). **Delete** (or Backspace) removes it, nodes with their members, supports and loads, in one step. **Esc** cancels the drag or the member being drawn, then goes back to Select, then clears the selection. Several things selected: the properties say how many, with Remove Selected; one load: its row of the Loads table.
- **Tools:** Select (see above), Node, Member (two clicks, then it goes on from its end; Esc or a right click stops; missing nodes are made), the five supports (a click on a node), Internal hinge (a click on a node turns it on or off), Force and Couple (on a node, or on a member at the point clicked, rounded to the grid), Distributed (a uniform load on the member clicked), Erase (a node with what uses it, or a member with its loads). New loads start at 10 in the structure's units.
- **Point:** coordinates typed as in the CAD editor — `x, y`, or `x; y` with decimal commas, or `@dx, dy` from the member's last point or the selected node — place a node (or the member's next point with Member) on Enter.
- **Properties:** a node's name (renamed everywhere), x, y, internal hinge, support and its angle; a member's hinges at either end and E, I, A (empty: none), its length.
- **Tables:** Nodes (x, y, hinge), Members (ends, hinges, E, I, A), Supports (kind, angle), Loads (where, position, values, direction, from the member's axis, per projection), each row editable and removable, with Add buttons. Every number may be a calculation parameter or an expression; a plain number is read with a decimal point or comma.

## Working

`structure.working` (Structures › Teaching › Write Working, or the editor's button) writes after the selected structure its solution as live calculations, in the interface's language (`labels`):

- **Reactions.** One unknown per support component: `H_A`, `V_A` (along x and y), `R_B` along an inclined roller's or link's direction (`H_B` or `V_B` when it is horizontal or vertical), `M_A` for a fixed end or slider. The equations come from equilibrium — rotation about each support (`ΣM_A = 0`), translation along x and y (`ΣF_x = 0`, `ΣF_y = 0`), and the rotation about an internal hinge of the part on one side of it (`ΣM_B = 0 (part about the hinge B, Member 2)`) — and are written in an order where each has one unknown left, so calc solves it and shows its steps one under the other (`{"layout": "lines"}`): `0 kN·m = V_B·(6 m) − (10 kN/m)·(6 m)·(3 m) ⇒ … ⇒ V_B = 30 kN`. Each reaction solved is a calculation name the later equations use. Loads enter as resultants: a trapezoid as a rectangle and a triangle (`(6 kN/m)·(2 m)/2·(4 m)/3`), a load per projection over the projected length. A reaction no equation isolates — a hyperstatic structure's redundants, or reactions only found together — is given its value from the stiffness solution, under a line saying so. A note recalls the reactions' signs.
- **Internal forces.** For each member and each stretch between loads: `N_1(x)`, `T_1(x)`, `M_1(x)` (`N_2a`, `N_2b`… when a member has several stretches; the shear's letter is the structure's `titles.v`) as calc functions of `x` from the member's first node, then their values at the stretch's ends and at M's extremes (`M_1(3 m) = 45 kN·m`). A truss gets each member's axial force (`N_3 = (-25 kN)/2`).
- **Numbers.** Quantities are written exactly when they are simple fractions (`(70 kN)/3`, `(15 m)/8`), so a law evaluated at a stretch's end gives the value itself (`M_2(6 m) = 0 kN·m`), else with eight significant digits; in the document's decimal separator. Equations start with their zero, `0 kN·m = V_B·(6 m) − …` (calc solves an equation whose zero has its unit, on either side).
- **Long equations wrap.** A display equation array wider than the line (a calculation's steps one under the other, `{"layout": "lines"}`) wraps row by row at its operators like any long display equation, keeping the rows aligned at `=` as far as the line allows (`crates/layout/src/math/eq_wrap.rs`).
- The working is written with the structure's current values: after changing the structure (or for another variant of an exercise), write it again. A labile or invalid structure has none (the command says why).

## Next

- The working written with the structure's parameters (`L`, `q`) instead of their values, so it follows each variant without being written again.

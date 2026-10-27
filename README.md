<!-- Modified by the Strix contributors (fork of WordCraft); see NOTICE. -->
<p align="center">
  <img src="assets/app-icon/hicolor/256x256/apps/io.github.nabre.strix.png" width="128" height="128" alt="Strix icon: an owl, half feathers and half circuit, with a quill, on a deep green tile">
</p>

<h1 align="center">Strix</h1>

<p align="center">
  <b>WYSIWYG math for engineers: type an equation in the page, and it calculates.</b>
</p>

<p align="center">
  Strix is an open-source document editor, written in pure Rust, for working out math and
  engineering problems. You write the problem as you would on paper (text, tables, equations,
  drawings), and the equations are live: they carry units, solve for what is unknown, check
  limits, and update as soon as an input changes. Exercises keep a locked structure with a
  results summary, and CAD drawings follow the calculations. It runs natively on Windows, macOS,
  Linux and BSD, and in the browser via WebAssembly.
</p>

<p align="center">
  <img alt="Written in Rust" src="https://img.shields.io/badge/written%20in-Rust-047857?style=flat-square&logo=rust&logoColor=white">
  <img alt="Runs on Windows, macOS, Linux, BSD and the web" src="https://img.shields.io/badge/runs%20on-Windows%20%C2%B7%20macOS%20%C2%B7%20Linux%20%C2%B7%20BSD%20%C2%B7%20Web-0F3D2E?style=flat-square">
  <img alt="License: MIT OR Apache-2.0" src="https://img.shields.io/badge/license-MIT%20%2F%20Apache--2.0-047857?style=flat-square">
  <img alt="Agent-drivable over MCP" src="https://img.shields.io/badge/agents-MCP%20%C2%B7%20CLI-0F3D2E?style=flat-square">
</p>

<p align="center">
  <a href="#wysiwyg-math">WYSIWYG math</a> ·
  <a href="#exercises">Exercises</a> ·
  <a href="#cad-drawings-driven-by-the-calculations">CAD drawings</a> ·
  <a href="#a-full-document-editor-underneath">Document editor</a> ·
  <a href="#files">Files</a> ·
  <a href="#for-agents-commands-cli-mcp">For agents</a> ·
  <a href="#build-and-run">Build and run</a> ·
  <a href="#status-and-roadmap">Roadmap</a> ·
  <a href="#license-and-credits">License</a>
</p>

> **Status: alpha.** The document editor Strix is built on is mature; the calculation engine,
> exercises and CAD are new and moving fast. There is no Strix release yet: build from source.

## WYSIWYG math

There is no separate worksheet, no code cell and no formula bar. An equation in the document *is*
the calculation, typed in place with the equation editor and shown as typeset math. Strix reads
every calculated equation in reading order, like a worksheet, and keeps its results up to date:
after each edit (waiting until the caret leaves the equation, so nothing is rewritten while you
type), on F9 and when the file opens.

```text
L = 4.5 m                          defines L
q = 12 kN/m                        units multiply and divide like numbers
M = q·L²/8 =                       →  M = q·L²/8 = 30.375 kN·m
W = 1000 cm³
f_yd = 355 MPa
σ = M/W =                          →  σ = M/W = 30.375 MPa
σ ≤ f_yd                           →  σ ≤ f_yd ⇒ 30.375 MPa ≤ 355 MPa ✓
m = 400 kg
F = 1 kN
F = m·a                            →  F = m·a ⇒ a = 2.5 m/s²        (a is the one unknown: solved)
```

- **One `=` for everything.** Strix works out what a statement means from what is known so far:
  a definition (`L = 4.5 m`), a result to show (`M =`, or `M = kN·m` to choose the unit), a
  formula shown with its value (`M = q·L²/8 =`), an equation to **solve** for its one unknown
  (`F = m·a`, `x² + x = 6`), or a **check** (`a + b = c`, and `≤ ≥ < > ≠` with ✓/✗). `:=` still
  defines explicitly.
- **Solving.** The unknown is isolated symbolically when it appears once (through `+ − · /`,
  powers, roots, `exp ln log`, trigonometry and their inverses); otherwise it is found
  numerically and the smallest non-negative root wins.
- **Units everywhere.** SI base and derived units with prefixes, engineering units (`mm`, `kN`,
  `MPa`, `bar`, `kWh`, `t`, `h`…) and dimensionless ones kept through results (`%`, `‰`, `ppm`,
  `°`, `rad`, `gon`). Dimension errors (`2 m + 3 s`) are caught, never silently wrong. The
  **working unit** follows what you write: after `b = 300 mm`, lengths show in mm. Units are
  typeset upright and coloured with the **Unit** character style, so they stand apart from
  symbols. A unit typed in the wrong case gets a hint ("did you mean kN?").
- **Solution steps.** With Show Steps a formula or a solved equation shows the formula isolated
  for the unknown, the values substituted and the result, aligned at `=`, rebuilt on every
  recalculation.
- **Calculated tables.** Turn on Calculated Table and an ordinary table becomes a calculation:
  headers name columns with units and formulas (`L [m]`, `M [kN·m] = γ·q·L²/8`), cells may hold
  their own statements, `Σ =`, `mean =`, `min =`, `max =` total a column, and every complete
  column becomes a vector for the rest of the document.

  | Beam | L [m] | q [kN/m] | M [kN·m] = γ·q·L²/8 |
  |---|---|---|---|
  | B1 | 4.5 | 12 | *41.006* |
  | B2 | 6000 mm | 10 | *60.75* |
  | Total | | | *Σ = 101.756* |

- **Sums, ranges and vectors.** `∑_(i=1)^n▒i²`, `∏`, ranges `i = 1..5`, vectors
  `F = [1; 2; 3] kN` with indexing and element-wise arithmetic; `sum mean len min max`.
- **Number format.** Digits (default 3), automatic / decimal places / significant digits,
  trailing zeros, half-away-from-zero rounding, scientific notation for very large and very small
  values: on the Math tab.
- **Errors never stop the sheet.** A failing statement shows `⚠` and its message; later items
  still calculate.

Everything is on the **Math** tab: New Calculation, Calculate (also the `=` button beside an
equation), Recalculate, Calculated Table, number format, Exercise and CAD Drawing. Design and
details: [docs/calc.md](docs/calc.md).

## Exercises

**Math › Exercise** inserts a worked problem with a structure that editing can't break:

- a title, the **statement** (free content: text, figures, tables, equations, drawings), the
  **data** generated from the Exercise Properties pane, and the **questions**, lettered and
  nested up to three levels (a), a.1), a.1.1)), with notes between them;
- the **solution**, where each question is repeated as a read-only line with room to work it out
  under it; the working stays with its question when the questions change;
- one **results summary** at the end of the document for all exercises: an equation marked as
  the result of a question (Result Of) is listed with its calculated value (`a) M = 30.375 kN·m`).

Each exercise is a **calculation sandbox**: what it defines stays inside, so two exercises can
both define `L`. In the Exercise Properties pane the data (symbol, description, value, unit) can
have **variants** A, B, C…; choosing one recalculates the data, results and summary. Details:
[docs/exercise.md](docs/exercise.md).

## CAD drawings driven by the calculations

**Insert › CAD Drawing** (or Math › Drawing) places a technical drawing in the text. Its
coordinates, radii and angles can be numbers or **expressions of the calculation's parameters**
(`["L", 0]`, `L/2 + 0.3`), evaluated where the drawing sits in the document:

- change `L = 4.5 m` above it and the beam **redraws**;
- dimensions and texts recall parameters: `{{L}}` shows `4.5 m`, `{{L:both}}` shows
  `L = 4.5 m` (units in the Unit style), `<>` the measured size; a parameter that isn't defined
  there shows `[L?]` in red.

Double-click a drawing to edit it on its sheet, with a **CAD Drawing** tab and an AutoCAD-style
command line (`x,y`, `@dx,dy`, `@d<a`, direct distances, tool aliases): line, circle, arc,
polyline, rectangle, point, text, aligned / horizontal / vertical dimensions; select, move, copy,
delete; ortho, grid, zoom. **Object snaps** are exact, computed on the true curves by a geometry
kernel (endpoint, midpoint, centre, node, quadrant, intersection, perpendicular, tangent,
nearest); **grips** reshape selected objects and keep a coordinate's expression (`L` becomes
`L + 0.5`). Selection works as in AutoCAD: a click adds, Shift+click removes, a window, a
crossing or a lasso, fences and polygons, Previous, Undo, cycling through overlapping objects; the
**right button** opens a menu for what is under way (a command's options, the selected objects,
the drawing), with a snap for the next point on Shift+right-click. A **Properties pane** shows the
selection by family or object by object, edits layer, colour, lineweight and linetype, and the
geometry with numbers or calculation expressions; it manages the **layers** too. The drawing is
stored as JSON in the `.xmath` file together with an automatic SVG
picture, so other programs still show it. Details: [docs/cad.md](docs/cad.md).

*Planned for CAD:* offset, trim, extend, fillet, mirror and the other modify tools, match
properties, angular dimensions, hatches, rotated texts, and driving dimensions
with geometric constraints.

## A full document editor underneath

Strix grew out of a word processor that aims at Microsoft Word parity (it is a fork of WordCraft,
see [License and credits](#license-and-credits)), so the report around a calculation is a real
document: a familiar ribbon interface, styles, headings, lists, tables, equations, pictures and
shapes, headers and footers, sections and columns, footnotes, citations and tables of contents,
comments and track changes, spelling and hyphenation, mail merge and printing. The interface
follows the operating system's language (27 languages, Italian included; File › Options ›
Interface language picks one by hand). Measured parity: [ROADMAP.md](ROADMAP.md).

## Files

| | Formats |
|---|---|
| **Saves** | `.xmath` (Strix document) and `.xmathm` (Strix template): an OOXML package that keeps calculations, exercises and drawings live |
| **Exports** (static copies) | PDF, PNG, LaTeX (`.tex`), HTML, Markdown |
| **Opens** (import) | `.docx`, `.docm`, `.dotx`, `.dotm`, `.doc`, `.odt`, `.rtf`, `.md`, `.html`, `.tex`, `.txt`, Word XML |

Imported documents become Strix documents: Save asks for a `.xmath` name. Strix never writes back
to `.docx` or the other editable formats, because they would lose the live calculations. A
`.xmath` file stays a standard package, so other programs show the last computed values as
ordinary equations, tables and pictures.

## For agents: commands, CLI, MCP

Everything Strix does is a **command** in one registry, with an id, parameters and a ribbon
location. The ribbon, keyboard shortcuts, command search, the command line, the control channel
and the MCP server all dispatch the same commands, and programmatic calls never open dialogs.
The calculation, exercise and drawing commands include `calc.insert`, `calc.recalculate`,
`calc.variables`, `calc.options`, `calc.steps`, `table.calc`, `exercise.insert`,
`exercise.setData`, `equation.answer`, `insert.cad`, `cad.get`, `cad.set` and `cad.add`.

```sh
claude mcp add strix -- strix-cli mcp                      # headless documents
# confine the agent to folders: strix-cli mcp --automation-read-root DIR --automation-write-root DIR
strix --control 7981 &                                     # or drive the running app…
claude mcp add strix-app -- strix-cli mcp --connect 127.0.0.1:7981
```

MCP tools include `list_commands`, `execute`, `batch`, `type_text`, `select_text`,
`inspect_document`, `render_page`, `save_document` and, with a running app, `screenshot`, `click`
and `key`. See [docs/mcp.md](docs/mcp.md) and the [control protocol](docs/control-protocol.md).

## Platforms

Native desktop builds for **Windows**, **macOS**, **Linux** and **BSD** (FreeBSD), and a
**web** build that runs the same app in the browser through WebAssembly. One Rust code base, one
egui front end.

## Build and run

Requirements: a Rust toolchain (1.95 or later); for the web build,
[Trunk](https://trunkrs.dev/) and the `wasm32-unknown-unknown` target.

```sh
git clone https://github.com/nabre/strix
cd strix
cargo run --release -p strix -- --sample          # the desktop app with a sample document
cargo run --release -p strix -- problem.xmath     # open a Strix document
```

Command line (`cargo run --release -p strix-cli -- …`, or the built `strix-cli` binary):

```sh
strix-cli convert report.docx report.xmath        # import a Word document
strix-cli convert problem.xmath problem.pdf       # static export: pdf, png, tex, html, md
strix-cli run --template sample \
  --cmd 'calc.insert={"text":"L = 4.5 m"}' --save beam.xmath
strix-cli commands                                # every command id
```

Web: `cd apps/strix-web && trunk serve`, then open <http://127.0.0.1:8771/?sample>.

Before a commit, `cargo xtask ci` runs formatting, clippy, the tests, the asset-attribution
check, the layering check and the wasm build. Contributor and agent instructions:
[AGENTS.md](AGENTS.md).

## Architecture

| Layer | Crate | Job |
|---|---|---|
| L0 | `strix-geom` | units and measurements |
| L1 | `strix-doc`, `strix-fonts`, `strix-proof`, `strix-calc`, `strix-cad` | document model and editing; fonts and shaping; spelling and hyphenation; **quantities, units, solving and worksheet evaluation**; **parametric drawings and the geometry kernel** |
| L2 | `strix-layout`, `strix-docx`, `strix-formats` | line breaking, pagination, tables, hit testing; the `.xmath` package (OOXML); imports and static exports |
| L3 | `strix-render`, `strix-pdf` | rasteriser (vello_cpu); PDF ([craft-pdf](docs/shared-pdf.md), Krilla) |
| L4 | `strix-engine` | session, undo, the command registry, calculations, exercises, file dispatch |
| L5 | `strix-mcp` | MCP server |
| L6 | `strix-ui-egui` | the desktop and web front end, including the CAD editor |
| apps | `strix`, `strix-cli`, `strix-web` | desktop, command line, browser |

Strix never panics on purpose: no `unwrap`, `expect` or `unsafe` in production code, and every
command is tested with hostile parameters.

## Status and roadmap

Done:

1. **Worksheet**: parameters with units, live results, errors, the Calculate button.
2. **One `=`**: definitions, results, solving for one unknown, checks and comparisons, working
   units, number format, unit hints, solution steps.
3. **Ranges, vectors, Σ/∏.**
4. **Calculated tables.**
5. **Exercises**: locked structure, sandbox, repeated questions, results summary, data and
   variants.
6. **CAD**: parametric drawings in the document, redrawn with the calculations; the editor with
   exact snaps and grips.

Planned:

- Calculations: Variables pane, unit picker, `°C`/`°F`, user functions `f(x) = …`, matrices,
  systems of equations across regions, plots; number format per document.
- Exercises: a results table per exercise and checks against expected values; a summary per
  variant.
- CAD: the modify tools, layers, hatches, angular dimensions, constraints and driving dimensions.

Progress log: [ROADMAP.md](ROADMAP.md).

## License and credits

Strix is dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.
Copyright (c) 2026 nabre and the Strix contributors. Strix is a fork of
[WordCraft](https://github.com/storytold/wordcraft) and is not affiliated with or endorsed by its
authors. The original notice is kept as it was:

Copyright (c) 2026 ArtCraft Team and the WordCraft contributors. Required notices are in [NOTICE](NOTICE).

Bundled fonts, icons, images, dictionaries and translations keep their own open licenses; each
one is listed with its author, source and license in [ATTRIBUTION.md](ATTRIBUTION.md). The
interface translations come from the craft-i18n project and its translators
([locales/ATTRIBUTION-craft-i18n.md](locales/ATTRIBUTION-craft-i18n.md)). Colours follow the
[Atollo](https://github.com/nabre/atollo) palette. The project site is in [`site/`](site/).

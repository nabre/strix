<!-- Modified by the Strix contributors (fork of WordCraft); see NOTICE. -->
<p align="center">
  <img src="assets/app-icon/hicolor/256x256/apps/io.github.nabre.strix.png" width="128" height="128" alt="Strix icon: an owl, half feathers and half circuit, with a quill, on a deep green tile">
</p>

<h1 align="center">Strix</h1>

<p align="center">
  <b>Work out engineering and math problems in a live document: formulas with units, results that
  update, and the drawings that explain them.</b>
</p>

<p align="center">
  Strix is an open-source calculation notebook built in pure Rust. You write the problem as you
  would on paper (text, tables, equations), define parameters with their units, and Strix keeps
  every result up to date as you change the inputs, in the spirit of SMath Studio and Mathcad.
  A built-in CAD will draw the figures the problem needs. It runs natively on Windows, macOS,
  Linux and BSD, and in the browser via WebAssembly.
</p>

<p align="center">
  <img alt="Written in Rust" src="https://img.shields.io/badge/written%20in-Rust-047857?style=flat-square&logo=rust&logoColor=white">
  <img alt="Runs on Windows, macOS, Linux, BSD and the web" src="https://img.shields.io/badge/runs%20on-Windows%20%C2%B7%20macOS%20%C2%B7%20Linux%20%C2%B7%20BSD%20%C2%B7%20Web-0F3D2E?style=flat-square">
  <img alt="License: MIT OR Apache-2.0" src="https://img.shields.io/badge/license-MIT%20%2F%20Apache--2.0-047857?style=flat-square">
  <img alt="Agent-drivable over MCP" src="https://img.shields.io/badge/agents-MCP%20%C2%B7%20CLI-0F3D2E?style=flat-square">
</p>

<p align="center">
  <a href="#what-strix-does">What Strix does</a> ·
  <a href="#live-calculations">Live calculations</a> ·
  <a href="#files">Files</a> ·
  <a href="#quick-start">Quick start</a> ·
  <a href="#for-agents-cli-and-mcp">For agents</a> ·
  <a href="#architecture">Architecture</a> ·
  <a href="#roadmap">Roadmap</a> ·
  <a href="#license-and-credits">License and credits</a>
</p>

## What Strix does

- **Live calculations with units.** Any equation can be a calculation: `L := 4.5 m` defines a
  parameter, `M = kN*m` shows a result in the unit you ask for. Strix remembers every parameter
  and recomputes everything below it when an input changes. Dimension errors (`2 m + 3 s`) are
  caught and shown, never silently wrong.
- **A real document around the maths.** Headings, styles, tables, lists, references, figures,
  comments and track changes: Strix grew out of a full word processor, so the report around your
  calculation is as good as the calculation.
- **Drawings (in development).** A built-in CAD for the figures a problem needs (beams, sections,
  schemes, geometry), parametric so they can follow the calculation's values.
- **Files that stay alive.** A Strix document (`.xmath`) keeps the calculations and drawings live.
  Copies for others are static exports: PDF, PNG, LaTeX, HTML and Markdown.
- **Made to be driven by agents too.** Every action is a command, reachable from the ribbon, the
  command search, the command line and the MCP server.

## Live calculations

Turn on **Calculate** on an equation (or leave it on for all new equations with
`calc.newEquations`, the default) and write statements:

| You write | Strix shows |
|---|---|
| `L := 4.5 m` | the definition, remembered for everything after it |
| `q := 12 kN/m` | units multiply and divide like numbers |
| `M := q*L^2/8` | a parameter from earlier parameters |
| `M = kN*m` | `M = 30.38 kN·m`, kept up to date |
| `σ := M/W = MPa` | define and show in one region |

Regions are evaluated in reading order, like a worksheet. Units: SI base and derived units with
prefixes, common engineering units (`mm`, `kN`, `MPa`, `bar`, `kWh`, `t`, `h`, `°`…). Functions:
`sqrt`, `abs`, `exp`, `ln`, `log`, trigonometry, `min`, `max`, `round`; constants `π`, `e`, `g`.
F9 (or `calc.recalculate`) recalculates the whole document. Design and stages:
[docs/calc.md](docs/calc.md).

## Files

| | Formats |
|---|---|
| **Saves** | `.xmath` (Strix document) and `.xmathm` (Strix template): an OOXML package that keeps calculations live |
| **Exports** (static copies) | PDF, PNG, LaTeX (`.tex`), HTML, Markdown |
| **Opens** (import) | `.docx`, `.docm`, `.dotx`, `.dotm`, `.doc`, `.odt`, `.rtf`, `.md`, `.html`, `.tex`, `.txt`, Word XML |

Imported documents become Strix documents: Save asks for a `.xmath` name. Strix never writes back
to `.docx` or the other editable formats, because they would lose the live calculations.

## Quick start

```sh
git clone https://github.com/nabre/strix
cd strix
cargo run --release -p strix -- --sample          # the desktop app with a sample document
cargo run --release -p strix -- problem.xmath     # open a Strix document
```

Command line:

```sh
strix-cli convert report.docx report.xmath        # import a Word document
strix-cli convert problem.xmath problem.pdf       # static export: pdf, png, tex, html, md
strix-cli run --template sample \
  --cmd 'calc.insert={"text":"L := 4.5 m"}' --save beam.xmath
```

Web: `cd apps/strix-web && trunk serve`, then open <http://127.0.0.1:8771/?sample>.

The interface follows the operating system's language (27 languages, Italian included; File ›
Options › Interface language picks one by hand).

## For agents: CLI and MCP

```sh
claude mcp add strix -- strix-cli mcp                      # headless documents
# confine the agent to folders: strix-cli mcp --automation-read-root DIR --automation-write-root DIR
strix --control 7981 &                                     # or drive the running app…
claude mcp add strix-app -- strix-cli mcp --connect 127.0.0.1:7981
```

Tools include `list_commands`, `execute`, `batch`, `type_text`, `select_text`, `inspect_document`,
`render_page`, `save_document`, and, with a running app, `screenshot`, `click` and `key`. The
calculation commands are `calc.insert`, `calc.recalculate`, `calc.variables`, `equation.calc` and
`calc.newEquations`. See [docs/mcp.md](docs/mcp.md) and the
[control protocol](docs/control-protocol.md).

## Architecture

| Layer | Crate | Job |
|---|---|---|
| L0 | `strix-geom` | units and measurements |
| L1 | `strix-doc`, `strix-fonts`, `strix-proof`, `strix-calc` | document model and editing; fonts and shaping; spelling and hyphenation; **quantities, units and worksheet evaluation** |
| L2 | `strix-layout`, `strix-docx`, `strix-formats` | line breaking, pagination, tables, hit testing; the `.xmath` package (OOXML); imports and static exports |
| L3 | `strix-render`, `strix-pdf` | rasteriser (vello_cpu); PDF ([craft-pdf](docs/shared-pdf.md), Krilla) |
| L4 | `strix-engine` | session, undo, the command registry, calculations, file dispatch |
| L5 | `strix-mcp` | MCP server |
| L6 | `strix-ui-egui` | the desktop and web front end |
| apps | `strix`, `strix-cli`, `strix-web` | desktop, command line, browser |

`cargo xtask ci` runs formatting, clippy, the tests, the asset-attribution check, the layering
check and the wasm build. Contributor and agent instructions: [AGENTS.md](AGENTS.md).

## Roadmap

Strix is in **alpha**. The document editor it inherits is mature; the calculation engine is new.

1. **Scalar worksheet** (done): parameters with units, live results, errors, the Calculate button.
2. Variables pane, unit picker, `°C`/`°F`.
3. User functions `f(x) := …`, ranges, vectors and matrices, `Σ`/`Π` over ranges.
4. Tabular calculation: table columns as vectors, column formulas with units, sums.
5. Solving: `solve(…)`, systems of equations, plots.
6. Built-in CAD: parametric drawings bound to the calculation's parameters.

Progress log: [ROADMAP.md](ROADMAP.md). Calculation design: [docs/calc.md](docs/calc.md).

## License and credits

Strix is dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.
Copyright (c) 2026 nabre and the Strix contributors. Strix is a fork of WordCraft, Copyright (c)
2026 ArtCraft Team and the WordCraft contributors; Strix is not affiliated with or endorsed by
them. Required notices are in [NOTICE](NOTICE).

Bundled fonts, icons, images, dictionaries and translations keep their own open licenses; each
one is listed with its author, source and license in [ATTRIBUTION.md](ATTRIBUTION.md). The
interface translations come from the craft-i18n project and its translators
([locales/ATTRIBUTION-craft-i18n.md](locales/ATTRIBUTION-craft-i18n.md)). Colours follow the
[Atollo](https://github.com/nabre/atollo) palette.

<!-- Modified by the Strix contributors (fork of WordCraft); see NOTICE. -->
# Strix architecture

> **Last reviewed:** 2026-10-28 · **Last updated:** 2026-11-10 · **Change:** minor (WordCraft 0.17.0 merged: proofing is the external `craft-proof` (storytold/craft-libs) instead of `crates/proof`; previously minor: a push to `main` skips the checks a PR already passed on the same code; previously minor: each check runs once across the GitHub workflows; `cargo xtask ci` checks the workflows; previously minor: `dictionaries` (L2) added; previously minor: layer table matches `cargo xtask layers`: `calc`, `cad`, `metafile` added, `zotero` is L5, `control-key` standalone; previously minor: `proof` is per language) · **Target:** Microsoft Word (Microsoft 365) for Mac 16.113.4

How Strix is built today (origin/main `40c5d51`). The longer design notes in
`plan/architecture.md` are gitignored and local to the original author's machine; this file is the
committed, current summary. Rules for working in the code are in [`AGENTS.md`](../AGENTS.md).

## Layers

18 crates, 3 apps and `xtask`. Layering is enforced by `cargo xtask layers`:
a crate may only depend on crates in lower layers, and nothing below L6 depends on egui, eframe,
winit or rfd.

| Layer | Crate | Lines | Job |
|---|---|---|---|
| L0 | `geom` | 0.2k | Units (points, twips, EMU, inches, cm), rectangles, measurement parsing |
| L1 | `doc` | 10.0k | Document model: stories, paragraphs, runs, tables, sections, styles, numbering, fields, notes, comments, revisions, equations (OMML-shaped math tree, linear/LaTeX input); edit primitives; bidi |
| L1 | `fonts` | 2.3k | Font database (bundled OFL + system fonts), metrics, outlines, shaping with HarfRust, Word font substitution (`word.rs`: Carlito ↔ Calibri, Caladea ↔ Cambria, Liberation ↔ Arial/Times/Courier), OpenType MATH |
| L1 | `calc` | | Live calculations with units: calc regions, calculated tables (header formulas, ordinal column, total row, index row) |
| L1 | `cad` | | 2D drafting model (drawing units, scale, layers, entities, dimensions, levels), editing kernel, rendering to a neutral item list and SVG; builds on `calc` |
| L1 | `metafile` | | WMF/EMF reader into a neutral vector model |
| L1 (external) | [`craft-proof`](https://github.com/storytold/craft-libs/tree/main/crates/proof) (package alias `wordcraft-proof`) | — | Proofing moved to craft-libs (CP1, 2026-11): fifteen spelling dictionaries: English (US, GB, CA), Polish, Portuguese, Italian, Croatian, Turkish, Russian, Czech, Indonesian and Spanish (Spain, Mexico, Latin America, Uruguay); language modules provide spelling, suggestions and grammar, while Bulgarian and German have hyphenation only. Its word lists and patterns are craft-i18n's `language-data/`, embedded from `CRAFT_I18N_DIR` at the commit in `craft-i18n.rev` (CI and releases set `CRAFT_I18N_REQUIRED=1`); without them English spelling falls back to an installed Hunspell English or craft-proof's built-in Moby list, and pattern hyphenation is off (Bulgarian, Turkish and Russian hyphenate by rule) |
| L2 | `layout` | 9.6k | Shaping runs, line breaking, tabs, lists, tables (row splitting, floating tables), pagination, headers/footers, notes, floating objects and wrap, math layout, fields, hit testing; produces a display list |
| L2 | `docx` | 8.4k | OOXML WordprocessingML reader and writer (Transitional and Strict namespaces, `.docx/.docm/.dotx/.dotm`), OMML, custom properties, VBA parts preserved |
| L2 | `docbin` | 4.4k | Word 97-2003 `.doc` reader ([MS-DOC]: FIB, piece table, FKPs, SPRMs, stylesheet, lists, tables, notes, pictures) over a CFB container |
| L2 | `formats` | 10.6k | TXT, Markdown, HTML, RTF, ODT and LaTeX import/export through a shared intermediate model |
| L2 | `dictionaries` | 0.2k | Spelling dictionaries on demand: which public Hunspell dictionary each proofing language reads (LibreOffice's repository, fixed revision, licences) and a size-capped, all-or-nothing download into the user's dictionaries folder (desktop only) |
| L3 | `render` | 0.6k | vello_cpu rasteriser: pages and thumbnails from the display list; PNG/JPEG |
| L3 | `pdf` | 1.6k | krilla PDF export: font subsets, selectable text, links, outline, tagging |
| L4 | `engine` | 15.2k | `Session` (document, selection, undo, layout cache, view state), **command registry** (422 `CommandSpec`s in `cmd/*.rs`), Word feature catalog (`catalog.rs`), samples, file I/O dispatch (`io.rs`), Read Aloud sentence model |
| L5 | `zotero` | 1.7k | Zotero's word-processor integration protocol answered against a `Session` |
| standalone | `control-key` | 0.1k | Per-window control-port key files shared by the app and the MCP bridge |
| L5 | `mcp` | 1.0k | MCP server: every command as a tool; headless or bridged to the running app |
| L6 | `ui-egui` | 13.1k | Word-style front end: ribbon, keytips, mini-toolbar, Backstage, canvas, rulers, panes, dialogs, Equation tab, i18n catalogs, themes, control channel, headless `ui_shot` example |
| apps | `strix` | 1.6k | Desktop app (eframe + wgpu; logging, window geometry, file associations) |
| apps | `strix-cli` | 0.4k | `convert`, `text`, `inspect`, `run`, `commands`, `parity`, `mcp`, `zotero` |
| apps | `strix-web` | 0.3k | Browser build (eframe web runner, WebGPU with WebGL2 fallback) |

## Data flow

1. **Open:** `engine::io` picks a reader by extension (`docx`, `docbin`, `formats`, JSON) and
   produces a `doc::Document`.
2. **Edit:** every user-visible action is a command id run through `Session::run` (ribbon,
   shortcuts, command search, CLI, control channel and MCP all dispatch by id). `Session::run`
   snapshots for undo (blocks are `Arc`-shared, so snapshots are cheap) and catches an escaped
   panic, restoring the document (never-crash standard).
3. **Layout:** `layout` turns the document into pages of positioned glyphs, boxes and images
   (display list), incrementally: an edit relayouts only what changed (1.4 ms on a 188-page
   document, measured 2026-10-06).
4. **Draw:** the UI paints pages through `render` (CPU raster, cached per page, pixel-aligned) on
   an egui/wgpu canvas; PDF export walks the same display list through `pdf`.
5. **Save:** `engine::io` picks a writer by extension.

## Testing and gates

- 821 `#[test]` functions and 10 property-test blocks: model, layout, DOCX round-trip and hostile
  input, `.doc`, formats, typing parity (`engine/src/tests/typing.rs`), bidi, i18n catalogs, UI
  interaction (egui_kittest), MCP acceptance; `hostile_file_params_never_panic` runs `file.*`,
  `insert.picture` and `insert.textFromFile` with junk params and unusable paths, held to a
  temporary folder, and `hostile_params_never_panic` every other command with junk params.
- `cargo xtask ci`: fmt, clippy `-D warnings`, tests, asset attribution, the GitHub workflows
  (`cargo xtask workflows`: outside actions pinned to one commit each, one craft-fonts commit
  everywhere), layering, wasm build.
- `cargo xtask parity` regenerates [`parity-checklist.md`](parity-checklist.md); a test keeps the
  catalog percentage above a floor that only rises.
- GitHub Actions, each check run once: CI (`cargo xtask ci`) on pushes to `main` and on PRs,
  but a push to `main` whose exact code (git tree) a PR already passed skips it, as Printing and
  Packaging lint do (`tested-tree.yml`; a nightly run on `main` refreshes the dependency caches
  when every push was skipped);
  packaging lint, and FreeBSD / Windows ARM64 on PRs that change their files and nightly on `main`;
  one release workflow: a push to `main` whose version has no GitHub Release merges it into
  `release`, where every package is built (signed) and the release published, reading those runs instead of running them again (the
  released commit's CI must have passed; the nightly FreeBSD and Windows ARM64 runs are
  reported). See [`releasing.md`](releasing.md).

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-11-10 | minor | WordCraft 0.17.0 merged: `crates/proof` left the workspace for the external `craft-proof` (storytold/craft-libs, still named `wordcraft-proof`), its word lists and patterns from craft-i18n's `language-data/` (`CRAFT_I18N_DIR`); the file-command hostile-parameter test |
| 2026-11-10 | minor | A push to `main` doesn't run again CI, Printing and Packaging lint when a PR already passed them on the same tree (`tested-tree.yml`) |
| 2026-11-10 | minor | One release workflow: a version without a release on `main` is merged into `release`, built signed and published with every package |
| 2026-11-10 | minor | Each check runs once across the workflows (release reads CI and the nightlies instead of running them again); `cargo xtask workflows` in `cargo xtask ci` |
| 2026-10-28 | minor | Layer table matches `cargo xtask layers` (`calc`, `cad`, `metafile`; `zotero` L5; `control-key` standalone) |
| 2026-10-10 | minor | `proof`: proofing languages (English, Polish), affix engine, Polish patterns; layout and engine route each run by its language |
| 2026-10-10 | major | First committed architecture doc, from the crates' sources and `AGENTS.md` |
| 2026-11-08 | minor | `dictionaries` (L2): spelling dictionaries downloaded on demand |

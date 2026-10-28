# Resume notes

Last updated 2026-10-28. Branch `main` (pushed). Read **In progress** and **Next** first.

## Done
- WordCraft → Strix rename; upstream notices kept; NOTICE Modifications section; ArtCraft brand
  files removed (their licence requires forks to remove them).
- New icon, Atollo palette in `theme::Tokens` (contrast tests pass).
- Update check disabled until Strix has its own release feed.
- i18n catalogs in `locales/`, Italian complete; OS language followed (`auto`).
- Native files: Strix saves only `.xmath`/`.xmathm`; every other editable format is import-only;
  static exports PDF, PNG, LaTeX, HTML, Markdown (`io::EXPORT_EXTS`, `io::check_writable`).
- Live calculations (docs/calc.md): `strix-calc`; `calc.insert`, `calc.recalculate`,
  `calc.variables`, `equation.calc` (Calculate button `=` beside the equation frame),
  `calc.newEquations` (default on); recalculation on leaving a calculated equation and on F9;
  flag saved as `strix:calc="1"`.
- README and ROADMAP reoriented: Strix is a tool for solving math/engineering problems, with a
  built-in CAD planned.

- Calculations with one `=` (docs/calc.md): unknown detected and solved (symbolic isolation,
  else numeric with the smallest non-negative root), checks `a = b` and comparisons `≤ ≥ < > ≠`
  with ✓/✗; Σ/∏, ranges, vectors; calculated tables (`TableProps::calc`, `strix:calc` on
  `w:tblPr`, Table Layout › Data › Calculated Table); number format (`calc.options`: digits 3,
  auto/decimals/significant, zeros, half-away rounding, scientific ≥1e7 / <1e-3); working units
  (last unit written per kind); unit-case hints; Math tab (keytip C); `*` types `·` in equations.
- `strix-cad` stage 1 (docs/cad.md): drawing model, coordinates as expressions of calc
  parameters, `<>`/`{{L}}` dimension labels, rendering to paper-mm primitives, AutoCAD point entry.
- Exercises (docs/exercise.md): Math › Exercise; locked structure (title, statement, questions,
  solution with the questions repeated, one results summary at the end); each exercise is a
  calculation sandbox; equations marked as a question's result (`strix:answer`).
- Automatic recalculation after edits; every calculated-table cell is data or a statement;
  dimensionless units kept (`% ‰ ppm ° rad gon sr`). Discord link removed everywhere.
- `old_strix/` (the owner's earlier prototype) is local only and git-ignored: nothing may depend
  on it; it can be deleted at any time. Its calc and CAD behaviour is recorded in docs/calc.md and
  docs/cad.md.

## Known
- Windows-only test failure inherited from upstream: `strix-proof`
  `hunspell::tests::reads_are_bounded_and_regular_files_only`; `strix-engine`
  `tests::ime_body_epoch_marks_body_replacements_not_edits` is flaky under the parallel run
  (passes alone). Timing tests (`a_megabyte_long_word_lays_out_quickly`, docx
  `content_control_save_cost`) can fail on a loaded machine. Skip them with `--skip`.
- GitHub billing: some jobs (Nix, SonarCloud, Windows ARM64) were not started because "recent
  account payments have failed"; Nix checks nothing (no `flake.nix`); release needs signing
  secrets; shared workflows track `craft-common-actions@main` (consider pinning).
- The repository is public since 2026-10-28, so GitHub Actions run. The shared gate
  (`storytold/craft-common-actions` `gate.yml`) failed at clippy because a `craft-i18n.rev` at
  the repository root made it check out upstream craft-i18n (no `strix.tsv`) with
  `CRAFT_I18N_REQUIRED=1`; the pin moved to `locales/craft-i18n.rev` (provenance only; the
  catalogs are the vendored `locales/`). Check the next CI run. The other inherited workflows
  (Printing, Nix, FreeBSD, Windows ARM64, SonarCloud — needs `SONAR_TOKEN`, release signing —
  Apple secrets, only on the `release` branch) still need adapting or disabling.
- On Windows a running Strix (`target\debug\strix.exe`) blocks `cargo test --workspace`
  rebuilding the app binary ("Accesso negato"): close it, or test the `strix` package with
  another `CARGO_TARGET_DIR`.
- Parallel agents: one worktree and one `CARGO_TARGET_DIR` each; appends to
  `locales/it/strix.tsv`, `crates/cad/src/tests.rs` and the docs' revision histories conflict on
  merge (keep both sides).

- CAD stage 2: drawings in the document (`GraphicKind::Cad`, `insert.cad`, `cad.get/set/add`),
  redrawn with the calculations in reading order, `.xmath` round trip; units in the Unit style.
- Solution steps under formulas and solved equations (`calc.steps`); a unit after a number is
  always the unit; Exercise Properties pane (data, variants); nested questions and notes.

- CAD editor: AutoCAD selection (click adds, Shift removes, window, lasso, fence, polygons,
  previous, undo, Shift+Space cycling), right-click menu, Properties pane (families, stepping,
  geometry with expressions, stroke, layers), draw order, current layer (docs/cad.md).
- Exercise: an empty paragraph at the caret gives way to the exercise.
- README rewritten around the WYSIWYG math; static project site in `site/` (Atollo palette).

- 2026-10-28 (this session):
  - CAD modify commands (docs/cad.md, docs/cad-parity.md): Trim, Extend (quick mode), Fillet,
    Chamfer (`crates/cad/src/trim.rs`); Explode, Join, Match Properties, Stretch, rectangular
    and polar arrays (`crates/cad/src/rework.rs`, tests in `tests_rework.rs`); editor tools with
    aliases, icons, Italian strings.
  - CAD output: rotated texts, dimension texts aligned with their line (`dimStyle.aligned`),
    dash-double-dotted, dashes drawn on the page and the editor canvas.
  - CAD scale: maximum size on paper (`Fit.fixed`), the Scale list offers *Fitted to Frame* and
    only the standard scales that fit (Picture Format and CAD Output share `scale_fields`;
    `cad.scale` / `ui.cadScale` with `max`, `auto`, `scale`).
  - Line samples in the CAD panes drawn on the sheet's white (dark theme contrast).
  - Calculations: `°C`/`°F` absolute temperatures and differences; user functions `f(x) = …`.
  - Exercises: titles numbered automatically (*Esercizio 1.* …, a list in the Exercise Title
    style); a new exercise never goes inside another.
  - Dependabot #1 merged (harfrust 0.15, quick-xml 0.42, zip 9, vello_cpu 0.3, hayro 0.8,
    getrandom 0.4…) with the code adapted; macOS DMG background redrawn for Strix.
  - Calculations: a definition worked out at once, `A = 3 m · 5 m =` → `15 m²` (docs/calc.md);
    a product of quantities is no working unit.
  - Graph paper, "foglio a quadretti" (docs/graph-paper.md): spacing ("interasse"), line style /
    colour / weight, a heavier line every N squares; placements background, to the end of the
    page, until the next odd/even page, for N pages, for a height; `graph.insert` / `graph.set` /
    `graph.get` / `graph.remove`, `view.graphPaperPane`; Insert › Pages › Graph Paper and the
    Graph Paper pane (with a preview); `strix:graph` on `w:pPr` / `w:sectPr` in `.xmath`. Merged
    without the final `cargo xtask ci` and screenshots (owner's call): run them.
  - CI: Linux gate green (`a162494`): non-UTF-8 save tests use `.xmath`, `hit_and_caret_agree`
    fixed (it failed everywhere after the rename), Zotero accept race on Linux (a connection not
    accepted yet is judged by its listener), Spanish grammar timing test takes the best of three.

## In progress (stopped, unfinished, on their own branches)
- None.

## Next
- Check the GitHub CI after the `locales/craft-i18n.rev` move; adapt or disable the inherited
  workflows (SonarCloud token, Printing, Nix, FreeBSD, Windows ARM64).
- CAD: constraints and driving dimensions (stage 5); fillet/chamfer of arcs and polylines;
  dimension chains and levels; input units (`2.5 m` typed); entity names and measures for the
  calculations; point styles (docs/cad-parity.md, Order of work).
- Calc: Variables pane, unit picker, systems of equations across regions, plots; number format
  per document (now a preference).
- Screenshots in `docs/images/` show the old WordCraft UI; regenerate with `ui_shot`.

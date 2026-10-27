# Resume notes

Last updated 2026-10-27 (evening). Branch `main`.

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
- `old_strix/` (the owner's earlier prototype) is local only and git-ignored: nothing may depend
  on it; it can be deleted at any time. Its calc and CAD behaviour is recorded in docs/calc.md and
  docs/cad.md.

## Known
- Windows-only test failures inherited from upstream: `strix-proof`
  `hunspell::tests::reads_are_bounded_and_regular_files_only`, `strix-layout`
  `tests::hit_and_caret_agree`; `strix-engine` `tests::ime_body_epoch_marks_body_replacements_not_edits`
  is flaky under the parallel run (passes alone).
- GitHub Actions don't start: the account's billing/spending limit blocks them (private repo).
  The inherited workflows (Printing, Nix, FreeBSD, Windows ARM64, SonarCloud, release signing)
  still need adapting to Strix.

## Next
- CAD stage 2: `insert.cad` (Insert › Illustrations › CAD Drawing, Math › Drawing) as a drawing
  canvas generated from the model, `cad.*` commands, regeneration on recalculation, `.xmath`
  round trip; then the egui editor (stage 3).
- Calc: Variables pane, unit picker, `°C`/`°F`, user functions `f(x) = …`, systems of equations
  across regions, solution steps, plots; number format per document (now a preference).
- Redraw the macOS DMG background (`packaging/macos/dmg/background.svg` still says "WordCraft").
- Screenshots in `docs/images/` show the old WordCraft UI; regenerate with `ui_shot`.

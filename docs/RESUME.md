# Resume notes

Last updated 2026-10-27. Branch `main`.

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

## Known
- Windows-only test failures inherited from upstream: `strix-proof`
  `hunspell::tests::reads_are_bounded_and_regular_files_only`, `strix-layout`
  `tests::hit_and_caret_agree`; `strix-engine` `tests::ime_body_epoch_marks_body_replacements_not_edits`
  is flaky under the parallel run (passes alone).
- GitHub Actions don't start: the account's billing/spending limit blocks them (private repo).
  The inherited workflows (Printing, Nix, FreeBSD, Windows ARM64, SonarCloud, release signing)
  still need adapting to Strix.

## Next
- Calc stage 2+: Variables pane, unit picker, `°C`/`°F`, user functions, ranges/Σ, tabular
  calculation, solve, plots.
- Built-in CAD (parametric drawings bound to calculation parameters): design first.
- Redraw the macOS DMG background (`packaging/macos/dmg/background.svg` still says "WordCraft").
- Discord icon in `crates/ui-egui/src/icons.rs` ("discord") → a generic community icon.
- Screenshots in `docs/images/` show the old WordCraft UI; regenerate with `ui_shot`.

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
- GitHub workflows (2026-10-28): CI gate (`gate.yml`, Rust 1.99, craft-fonts) and Printing
  (Linux/Windows/macOS UI tests) run on every push to `main`; Windows ARM64 and FreeBSD run
  nightly (FreeBSD's last failure was the non-UTF-8 save test fixed in `a162494`); Packaging
  lint on packaging changes; Release only on the `release` branch and needs the signing
  secrets (Apple, Windows/Azure). Nix was removed (no `flake.nix`: it evaluated nothing).
  SonarCloud skips every step without a `SONAR_TOKEN` secret (green but empty); to turn it on
  the owner needs a SonarCloud organization and project and must set `sonar.organization`
  (still upstream's `storytold`) and `sonar.projectKey` (`nabre_strix`) in
  `sonar-project.properties`. Shared workflows track `craft-common-actions@main` (consider
  pinning).
- Without craft-fonts (`CRAFT_FONTS_DIR`), `strix-layout`
  `tests_complex::myanmar_prevowel_is_drawn_before_its_consonant` and
  `tests_pages::a_page_ends_when_a_multiple_spaced_lines_text_no_longer_fits` depend on the
  installed fonts and can fail (an Ubuntu container with Unifont/WenQuanYi); the GitHub gate
  builds with craft-fonts. The wgpu tests (`objects::tests::a_dragged_shape…`,
  `rotation_drag…`) and `ui_shot` need a Vulkan adapter: `apt-get install mesa-vulkan-drivers`.
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
    Graph Paper pane (with a preview); `strix:graph` on `w:pPr` / `w:sectPr` in `.xmath`. Final
    check done after the merge: `cargo xtask ci` on Rust 1.99 and a `ui_shot` screenshot.
  - CI after the graph-paper merge: the gate failed at clippy 1.99
    (`manual_is_multiple_of` in `graph_pane.rs`), fixed; the Printing workflow failed in
    `ribbon_groups_fit_above_their_labels` (kittest's missing-glyph panic on CJK labels on a
    runner with no CJK font and no craft-fonts): the test allows tofu when no CJK face is
    embedded. Both merged into `main` (`8f16ae3`).
  - CI: Linux gate green (`a162494`): non-UTF-8 save tests use `.xmath`, `hit_and_caret_agree`
    fixed (it failed everywhere after the rename), Zotero accept race on Linux (a connection not
    accepted yet is judged by its listener), Spanish grammar timing test takes the best of three.

  - CAD dimensions (docs/cad.md, docs/cad-parity.md): right-angle symbol, Continue and
    Baseline, level marks (`Kind::Level`), Flip (`ui.cadFlip`); `crates/cad/src/dims.rs`, tests
    in `tests_dims.rs`.

  - CAD dimensions, second round: distances and angle radii in a scaled and an annotative part
    (`offsetUnits`, `rPaper`, style Scaled distances); dimension units (`dimStyle.units`:
    drawing, m, cm, mm, SIA 400); parameters in annotations set as equations
    (`strix_cad::spans`, `GraphicItem::Text::italic`); unknown words in a style read as the
    default.
  - Calculated tables (docs/calc.md): header row marked (bold, repeated; a table of numbers gets
    one), Total Row sums the columns, ordinal column 0 (`table.calcIndex`), rows by index (`i`,
    `n`, `x[i+1]` cyclic: Gauss's area and centroid).

  - Upstream tracking (docs/upstream.md): `cargo xtask upstream status|sync`, branch
    `vendor/wordcraft` (rebranded upstream snapshots; push it with `main`), fork point recorded
    in `main` (true fork commit `d859c673`, NOTICE corrected). Stale catalog source texts fixed
    (`STRIX_DICTIONARIES`, `github.com/nabre/strix`).
  - SignPath Foundation readiness (docs/code-signing.md): code signing policy (attribution, roles,
    privacy, system changes, uninstall) linked from README and site; publisher was upstream's
    "Learning Machines LLC" in the exe, MSI, deb and metainfo, now "The Strix contributors";
    `strix-cli.exe` gets VERSIONINFO; Release workflow signs through SignPath when
    `SIGNPATH_API_TOKEN` is set (`.signpath/artifact-configuration.xml`).

  - Upstream merge: WordCraft `9724f15c` merged (4800 tests green); Strix ribbon code in
    `crates/ui-egui/src/ribbon/strix.rs`, equation-page drawing in `canvas/strix.rs`; CAD editor
    `show`/`feed` split; dependency bump reverted (see Next).

## In progress (stopped, unfinished, on their own branches)
- None.

## Next
- SignPath Foundation (owner): apply at signpath.org with github.com/nabre/strix; turn on MFA on
  GitHub and SignPath; create project `strix`, signing policy `release-signing`, artifact
  configuration `windows` (copy `.signpath/artifact-configuration.xml`, check it in SignPath's
  editor: paths inside the MSI and the metadata restrictions are untested); set secret
  `SIGNPATH_API_TOKEN` and variable `SIGNPATH_ORGANIZATION_ID` on the `release` environment;
  remove the WINDOWS_CERTIFICATE / AZURE_* secrets so binaries are signed once. Open risks: egui's
  `default_fonts` embed Ubuntu-Light (Ubuntu Font Licence, not OSI-approved; enabled in
  `crates/ui-egui/Cargo.toml` and by craft-ui); the repository is not a GitHub fork of WordCraft
  (SignPath's rule on signing modified upstream builds).
- Dependencies: the upstream merge (2026-10-28, WordCraft `9724f15c`) reverted Dependabot #1
  (harfrust 0.15, quick-xml 0.42, zip 9, vello_cpu 0.3, hayro 0.8, getrandom 0.4) because
  upstream's new code targets the older versions; redo the bump on top of the merged code
  (`git show 4cda28be` is the earlier adaptation), or wait for upstream to bump them.
- SonarCloud: owner's organization, project and `SONAR_TOKEN` (see Known).
- CAD: constraints and driving dimensions (stage 5); fillet/chamfer of arcs and polylines;
  linear-auto dimensions, extension modes, associative ends, marks per end; input units (`2.5 m` typed); entity names and measures for the
  calculations; point styles (docs/cad-parity.md, Order of work).
- Calc: Variables pane, unit picker, systems of equations across regions, plots; number format
  per document (now a preference).
- Screenshots in `docs/images/` show the old WordCraft UI; regenerate with `ui_shot`.

# Resume notes (paused 2026-10-27)

Work paused by the owner. Branch `strix-rebrand`. Commits: `655616f` (rebrand, i18n, strix-calc)
and the WIP commit that adds this file. Nothing has been verified by `cargo xtask ci` yet.

## Done
- WordCraft → Strix rename (crates, apps, ids `io.github.nabre.strix`, packaging); upstream
  notices kept; NOTICE has the Strix header + Modifications section (Apache-2.0 §4). Licence
  skill: `.claude/skills/license-compliance/` (local, `.claude/` is gitignored), run its `check.sh`.
- New app icon `assets/app-icon/strix.svg` + PNG/hicolor/.ico/.icns renders.
- Update check disabled (`crates/ui-egui/src/updates_ui.rs`, `UPDATE_CHANNEL = false`).
- ArtCraft Discord/getartcraft links removed; Community → github.com/nabre/strix/discussions.
- i18n catalogs vendored in `locales/` (`strix.tsv` + `common.tsv`), embedded by
  `crates/ui-egui/build.rs` in every build; Italian completed and reviewed. OS language: `auto`.
- `crates/calc` (strix-calc, L1): units, worksheet evaluation, 17 tests. Engine commands
  `calc.insert`, `calc.recalculate`, `calc.variables` in `crates/engine/src/cmd/calc.rs`
  (5 tests); F9 (`references.updateFields`) also recalculates. Design: `docs/calc.md`.

## In progress (resume here)
1. **Native `.xmath` / `.xmathm`** (OOXML package; other formats import/export). An agent was
   stopped mid-way: engine/io, docx Flavor, `crates/engine/src/tests_native.rs`, CLI
   (`apps/strix-cli/tests/it/convert_native.rs`), `crates/ui-egui/src/file_dialogs.rs`, mcp tools,
   web, `docs/file-format-parity.md` are partly done. Re-check the brief (Save on non-native →
   Save As with `.xmath`; Save as Template → `.xmathm`; OS associations with
   `application/vnd.strix.xmath(m)`), then build and test.
2. **Atollo theme**: `crates/ui-egui/src/theme.rs` tokens switched to the Atollo palette
   (Reef Emerald `#047857` light accent, Emerald `#10B981` dark accent, Deep Forest nav, VS Code
   Modern neutrals). Not yet tested: run `cargo test -p strix-ui-egui --lib theme::` (contrast
   tests) and look at a `ui_shot` screenshot.
3. Then: full `cargo xtask ci`, regenerate `docs/parity-checklist.md` (`cargo xtask parity`),
   fix fallout, commit.

## Next
- Calc stage 2+ (docs/calc.md): Calculate ribbon tab and Variables pane, Italian strings for
  them, user functions, ranges/Σ, tabular calculation, solve.
- Owner decisions pending: delete `docs/brand/artcraft-*` trademark files; redraw the macOS DMG
  background (`packaging/macos/dmg/background.svg` still says "WordCraft", old blue).
- Discord icon in `crates/ui-egui/src/icons.rs` ("discord") should become a generic community icon.

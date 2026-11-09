# Naming: WordCraft's internal names, Strix's visible ones

Status: inventory approved; phase 2 (the targeted rules in `xtask/src/upstream.rs`) and phase 3
(the reverse rename on `main`, `cargo xtask upstream unrename`) done — 2026-11-09. Next: re-create
`vendor/wordcraft` with the new rules (`docs/upstream.md`, Transition).

Strix keeps its own name only where changing it would break something for the user or make
Strix collide with a WordCraft installed on the same computer. Everything else goes back to
upstream's name, so `vendor/wordcraft` needs only a few exact, file-scoped rules instead of the
blind `wordcraft` → `strix` substitution in `xtask/src/upstream.rs` (`RENAMES`).

Measured on `vendor/wordcraft` = `e326304` (upstream `9724f15c`) and `main` = `558f169`. On the
vendor branch every `strix`/`Strix`/`STRIX` comes from the rename, apart from the
"Modified by the Strix contributors" line it adds. So the occurrences below are exactly what
`RENAMES` produces today: **9,578 occurrences in 704 files, 61 distinct identifiers**.

## Group a — back to upstream's name

| What | Forms (vendor count) | Where |
|---|---|---|
| Crate paths | `strix_doc` 4417, `strix_engine` 789, `strix_docx` 704, `strix_geom` 276, `strix_layout` 235, `strix_fonts` 230, `strix_proof` 70, `strix_render` 57, `strix_ui_egui` 44, `strix_formats` 36, `strix_metafile` 14, `strix_zotero` 12, `strix_control_key` 11, `strix_pdf` 9, `strix_mcp` 6, `strix_docbin` 4 | everywhere in `crates/`, `apps/`, `xtask/` |
| Package names and workspace deps | `strix-doc`, `strix-engine`, … `strix-ui-egui` (16 crates) | `Cargo.toml`, `crates/*/Cargo.toml`, `Cargo.lock` (81) |
| Types and identifiers | `StrixApp`, `StrixVersion`, `StrixBuildCommit`, `strix_canvas`, `strix_loading`, `strix_viewport_input`, `strix_print_settings`, `strix_ibeam`, test names (`settings_written_by_strix_keep_no_original`, …) | ui-egui, engine, docx |
| egui ids | `strix.colorsDialog`, `strix.colorsDialogOpen`, `strix.moreColorsRequest`, `strix.recentColors` | `crates/ui-egui/src/colors.rs` … (see U6) |
| Log targets | `strix_docx=trace` in docs; the library crates' targets follow their crate names | `apps/strix/src/logging.rs` (see U3) |
| Temp file / test prefixes | `.strix-save-…` (safe save), `strix-recovery-…`, `strix-prefs-test-…` and ~70 more test prefixes, `zxqstrixy` | engine, ui-egui, apps tests |
| Internal keys and URIs | `strix:settings.orig`, `strix:app.kept`, `strix:vbaProject.related`, `strix:glossary.read`, `strix:embedded.parts`, `strix:customXml.roots`, `strix:unmodelled.roots` (passthrough map keys), `strix://document`, `strix://commands` (MCP resources), `strix:open`/`strix:ready`/`strix:save`/`strix:dirty`/`strix:failed` (web host messages) | docx `package.rs`, mcp `server.rs`, strix-web `host.rs` (see U5, U7) |
| Paste mode value | `"strix"` in `paste.special` modes | `crates/engine/src/cmd/paste.rs:116` |
| User-Agent | `Strix/` | `crates/dictionaries/src/lib.rs:155` is a Strix file and stays; upstream's agents go back |
| Log file name | `strix.log`, `strix.N.log` (the folder stays Strix, b3) | `apps/strix/src/logging.rs:34,132` |
| Environment variables (all from upstream files) | see the table below | |
| Running text in docs and comments | `Strix` in upstream's `docs/*.md`, `AGENTS.md`, `CLAUDE.md`, code comments | (see U9 for the docs Strix rewrote) |

### Environment variables

`vendor` = number of files on the vendor branch that name the variable, i.e. it exists upstream.

| Variable | Born in | vendor / main files | Becomes |
|---|---|---|---|
| `STRIX_LANGUAGE` | upstream `crates/ui-egui/src/i18n.rs` | 5 / 5 | `WORDCRAFT_LANGUAGE` |
| `STRIX_REGION` | upstream `crates/geom/src/region.rs`, `apps/strix/src/region.rs` | 5 / 5 | `WORDCRAFT_REGION` |
| `STRIX_DICTIONARIES` | upstream `crates/proof/src/hunspell.rs` | 6 / 35 | `WORDCRAFT_DICTIONARIES` (also used by Strix's `crates/engine/src/prefs.rs`, `cmd/dictionaries.rs`; named in one UI string, see translations) |
| `STRIX_WAYLAND` | upstream `apps/strix/src/main.rs` | 3 / 1 | `WORDCRAFT_WAYLAND` |
| `STRIX_CONTROL_PORT` | upstream `apps/strix/src/main.rs` | 6 / 6 | `WORDCRAFT_CONTROL_PORT` |
| `STRIX_CONTROL_KEY` | upstream `crates/mcp/src/backend.rs` | 3 / 3 | `WORDCRAFT_CONTROL_KEY` |
| `STRIX_PROFILE_DIR` | upstream `crates/control-key/src/lib.rs` | 3 / 4 | `WORDCRAFT_PROFILE_DIR` |
| `STRIX_NO_PREFS` | upstream `apps/strix/src/main.rs`, `logging.rs`, `region.rs` | 11 / 9 | `WORDCRAFT_NO_PREFS` |
| `STRIX_FILE_DIALOG` | upstream `apps/strix/src/file_dialogs.rs` | 1 / 1 | `WORDCRAFT_FILE_DIALOG` |
| `STRIX_SPEECH` | **upstream** `crates/engine/src/speech.rs` | 2 / 2 | `WORDCRAFT_SPEECH` |
| `STRIX_PASSWORD` | **upstream** `apps/strix-cli/src/main.rs` | 1 / 1 | `WORDCRAFT_PASSWORD` |
| `STRIX_TEST_KAI_FONT` | upstream `crates/fonts/src/fontdb/tests.rs` | 1 / 1 | `WORDCRAFT_TEST_KAI_FONT` |
| `STRIX_ROTATION_SHOT` | upstream `crates/ui-egui/src/objects/tests.rs` | 1 / 1 | `WORDCRAFT_ROTATION_SHOT` |
| `STRIX_DOCBIN_FIXTURES` | upstream `crates/docbin/src/tests.rs` | 1 / 1 | `WORDCRAFT_DOCBIN_FIXTURES` |
| `STRIX_BENCH_DOCX` | upstream `crates/ui-egui/src/tests/perf.rs` | 2 / 1 | `WORDCRAFT_BENCH_DOCX` |
| `STRIX_CARET_GLIDE_MS` | upstream `crates/ui-egui/src/caret_glide.rs` | 1 / 1 | `WORDCRAFT_CARET_GLIDE_MS` |
| `STRIX_BUILD_SHA` | upstream `apps/strix/src/main.rs`, `dialogs/dispatch.rs` | 6 / 8 | `WORDCRAFT_BUILD_SHA` (also set by Strix's `release.yml`, `site.yml`, `packaging/env.sh`, `package.ps1`) |
| `STRIX_BUILD_DATE` | upstream packaging and build scripts | 3 / 4 | `WORDCRAFT_BUILD_DATE` (same) |
| `STRIX_VERSION` | upstream packaging | 2 / 4 | `WORDCRAFT_VERSION` (same) |
| `STRIX_MAINTAINER` | upstream `packaging/linux/` | 2 / 2 | `WORDCRAFT_MAINTAINER` |
| `STRIX_REQUIRE_WINRES` | upstream `apps/strix/build.rs` | 2 / 3 | `WORDCRAFT_REQUIRE_WINRES` (also Strix's `apps/strix-cli/build.rs`) |
| `STRIX_PRETEND_VERSION` | Strix `crates/ui-egui/src/strix_update.rs` | 0 / 2 | **stays** |

`STRIX_SPEECH` and `STRIX_PASSWORD` both exist upstream, so they go back too. Two other
`STRIX_*` names are Rust constants rather than environment variables, and both are in Strix's
own code: `STRIX_NS` (`crates/docx/src/xml.rs`) and `STRIX_CAD`. They stay.

The variables that Strix's own `packaging/` and `.github/` set (`*_BUILD_SHA`, `*_BUILD_DATE`,
`*_VERSION`, `*_MAINTAINER`, `*_REQUIRE_WINRES`) are read by upstream code. Those files change to
`WORDCRAFT_*` by hand, at the same time.

## Group b — stays Strix (rule per file)

Lines are in `main`; on the vendor branch each line holds upstream's text, and a rule turns it into this.

| # | What | File:line (main) |
|---|---|---|
| b1 | App id `io.github.nabre.strix` (exact rule from `ai.storyteller.wordcraft`) | `apps/strix/src/main.rs:418` (+ packaging, owned) |
| b2 | Repository URL `nabre/strix` (exact rules from `github.com/storytold/wordcraft`, `storytold/wordcraft`) | `Cargo.toml:12`, `crates/engine/src/sample.rs:116`, `crates/mcp/src/server.rs:184`, docs |
| b3 | User folders | `crates/control-key/src/lib.rs:23,25,30` (`Application Support/Strix`, `APPDATA\Strix`, `~/.config/strix`); `crates/proof/src/hunspell.rs:93` (`<base>/Strix/dictionaries`); `crates/proof/src/downloads.rs:30,32,34` is a Strix file and stays |
| b4 | App package and executable names | `apps/strix/Cargo.toml:3`, `apps/strix-cli/Cargo.toml:3,11`, `apps/strix-web/Cargo.toml:3,14`; the path rule `apps/wordcraft*` → `apps/strix*`; `apps/strix/build.rs:22-23` (`OriginalFilename`, `InternalName`) |
| b5 | `serverInfo.name` `"strix"` | `crates/mcp/src/server.rs:182` (test `crates/mcp/src/tests.rs:26`) |
| b6 | `PRODUCT_NAME` rows outside `packaging/` (23 rows; the 13 `packaging/` rows go, the folder becomes Strix's) | `apps/strix/build.rs`, `apps/strix/src/main.rs`, `apps/strix-web/index.html`, docx `write/settings.rs`, `engine/sample.rs`, formats html/latex/odt/rtf, `mcp/server.rs`, `pdf/metadata.rs`, ui-egui `chrome.rs`, `dialogs/chrome.rs`, `updates_ui.rs`, `windows.rs` |

## Group c — external names, never renamed

- `craft_update` (incl. `craft_update::suite::WORDCRAFT`: today the rename turns it into
  `suite::STRIX`, a name that crate doesn't have; Strix uses its own `strix_update::STRIX`).
- `craft-i18n` and its catalog name `locales/<locale>/wordcraft.tsv` (see U1), `craft-libs`.
- Every other crate outside this workspace (`eframe`, `egui`, …): none of them contains `wordcraft`.

## Strix-owned paths (left out of the sync, maintained by hand)

`docs/brand/` (as today), `packaging/`, `site/`, `README.md`, `docs/README.*.md`.

Upstream work these exclusions will cost, counted from the fork (2026-10-26) to upstream HEAD
`7b8c0ba5` with the GitHub API:

| Path | Upstream commits | Note |
|---|---|---|
| `packaging/` | 33 | the 9724f15c snapshot alone changed 11 files (+829/−67): Windows install policy, `package.ps1`, `.wxs`, macOS `package.sh`, flatpak |
| `README.md` | 11 | mostly marketing text; Strix's is rewritten anyway |
| `site/` | 0 | doesn't exist upstream |
| `docs/README.*.md` | 0 | doesn't exist upstream |

`packaging/` is the only real cost: each sync has to review upstream's packaging commits by hand
(`git log <old>..<new> -- packaging` on upstream). The Windows `UpgradeCode` in
`packaging/windows/strix.wxs` never changes.

Strix's own crates and files keep their names and only get their imports updated:
`strix-calc`, `strix-cad`, `strix-dictionaries`, and in ui-egui/engine `cmd/{calc,cad,exercise,graph,master,dictionaries}.rs`,
`strix_objects.rs`, `sample_calc.rs`, `ribbon/strix.rs`, `canvas/strix.rs`, `cad_editor.rs`, `cad_panel.rs`,
`cad_dim_parts.rs`, `graph_pane.rs`, `exercise_pane.rs`, `subdocument_pane.rs`, `pane_kit.rs`, `strix_save.rs`,
`strix_update.rs`, `brand.rs`, `dictionary_download.rs`, `backstage/tiles.rs`, `widgets/{big_split,value_field}.rs`,
`crates/control-key/src/portable.rs`, `crates/proof/src/downloads.rs`, `crates/doc/src/graph_paper.rs`,
`crates/layout/src/graph.rs`.

## Translations of group-b strings

Group-b strings that pass through `tl!()` (or are catalog keys) and their entries in
`locales/*/strix.tsv` (27 locales): all present, except one Strix-owned string, missing in `ka`:
"File › New lists the .xmathm, .dotx and .dotm templates here under Personal; Save as Strix
Template starts here." (`crates/ui-egui/src/options.rs:641`).

Changes the migration forces on the catalogs:
- `STRIX_DICTIONARIES` is named in one key ("No spelling dictionary for {language} is
  installed…", `crates/ui-egui/src/chrome.rs:20`): the key and its 27 translations change to
  `WORDCRAFT_DICTIONARIES`.
- Upstream strings whose product name goes back to "WordCraft" change their key (see U2).

## Cases the decisions don't cover (proposals)

- **U1 — catalog file name.** `main` vendored craft-i18n's `wordcraft.tsv` as
  `locales/*/strix.tsv` and added Strix's strings to it; upstream's `crates/ui-egui/build.rs` reads
  `APP_CATALOG = "wordcraft.tsv"`. *Proposal:* rename the 27 files back to `wordcraft.tsv`
  (group c), so `build.rs` matches upstream with no rule; Strix's strings stay in the same files.
- **U2 — product name in upstream's running UI text.** Fourteen upstream `tl!()` strings name the
  product in running text ("Make Strix the default for Word documents…", "Show the Start screen
  when Strix starts", "Browse in Strix…", "{language}: Strix doesn't check spelling…", …: in
  `ui-egui/src/options.rs` ×7, `app/commands.rs` ×3, `backstage.rs` ×2, `dialogs/dispatch.rs` ×1,
  plus the `PRODUCT_NAME` one in `updates_ui.rs`). Engine messages do too (`cmd/chart.rs:123`,
  `cmd/smart_art.rs:156`, `cmd/citations.rs:91`). Users would read "WordCraft" in Strix.
  *Proposal:* they stay Strix, with rules per file (≈17), and keep their translations. The
  alternative is one scoped rule ("`WordCraft` inside `tl!("…")` literals of `crates/ui-egui` and
  `crates/engine/src/cmd`"), which is fewer rules but is a substring rule.
- **U3 — app log filter.** Upstream's `DEFAULT_FILTER = "warn,wordcraft*=info"`
  (`apps/strix/src/logging.rs:42`) would no longer match the app crate `strix`, so the app's own
  info logs would vanish. *Proposal:* a rule for that line → `"warn,wordcraft*=info,strix*=info"`,
  plus the targets in `apps/strix/src/logging/tests.rs` (`level_for("strix")`).
- **U4 — more user folders and stores than the list names.** These are user data and must stay
  Strix (b3): eframe's app name `"Strix"` (`apps/strix/src/main.rs:462`, picks the window-state
  storage folder), the portable data folder `StrixData` (`crates/control-key/src/portable.rs:20`,
  a Strix file), the web font cache IndexedDB `DB_NAME = "strix"`
  (`apps/strix-web/src/web/font_store.rs:11`), and the Windows Default-apps URI
  `registeredAppMachine=Strix` (`apps/strix/src/default_apps.rs:92`, which has to match the
  installer's registry name).
- **U5 — names written inside saved files.** `urn:strix:canvas-xfrm` (`crates/docx/src/lib.rs:82`)
  is written into .docx. Going back to `urn:wordcraft:canvas-xfrm` makes Strix and WordCraft files
  agree, but documents Strix has already saved would lose their canvas rotation. *Proposal:* go
  back to upstream's URI and have the reader also accept `urn:strix:canvas-xfrm` (a small reader
  change, plus a roundtrip test). Before moving the passthrough keys (`strix:settings.orig`, …), check
  that they never reach the saved package. Strix's own `strix:` namespace (`STRIX_NS`, `strix:cad`) stays.
- **U6 — egui ids that are saved.** `strix.recentColors` is kept in egui memory, which eframe
  saves; renaming it empties each user's recent colours once. *Proposal:* go back to upstream
  (a one-off loss of a convenience, no rule needed).
- **U7 — external interfaces.** MCP resource URIs `strix://document` and `strix://commands`, and
  the web host messages `strix:open`/`strix:ready`/…: clients read the URIs from
  `resources/list`, and `site/` doesn't use the messages. *Proposal:* back to upstream.
- **U8 — WordCraft's site URL.** Upstream links to `getartcraft.com/apps/wordcraft` in
  `crates/formats/src/latex/export.rs:29`, `crates/mcp/src/server.rs:184,189` and
  `crates/ui-egui/src/dialogs/dispatch.rs:1092` (the last one through `tl!`). `main` points them to
  `github.com/nabre/strix`. *Proposal:* group b, with three rules per file.
- **U9 — the executable name in text.** `wordcraft-cli` appears in help, error messages and docs
  (106 times; in code: `apps/strix-cli` ×10, `crates/engine` ×3, `crates/mcp` ×2, `xtask` ×2), and so
  do `wordcraft.exe` and `CARGO_BIN_EXE_wordcraft*` (13, apps tests). The user types the
  executable's name. *Proposal:* exact token rules `wordcraft-cli` → `strix-cli`, `wordcraft.exe`
  → `strix.exe`, `CARGO_BIN_EXE_wordcraft` → `CARGO_BIN_EXE_strix`. Each is a whole identifier,
  not a substring rule.
- **U10 — the modification notice.** Today 642 `.rs` files carry it, because the sync adds it to
  every renamed file. After the migration it stays only on upstream files whose content `main`
  changes (about 286 today, plus whatever the group-b rules touch). Files that only move
  (`apps/wordcraft*` → `apps/strix*`) don't get it.

## Rule count

| Kind | Rules |
|---|---|
| Exact rules (app id, 2 URLs) | 3 |
| Path rule `apps/wordcraft*` → `apps/strix*` | 1 |
| Exact tokens for the executable (U9) | 3 |
| `PRODUCT_NAME`, outside `packaging/` | 23 |
| User folders and stores (b3, U4) | 8 |
| App package names (b4) | 5 |
| `serverInfo.name` (b5) | 1 |
| Site URLs (U8) | 4 |
| Running UI text (U2), per file | ≈17 |
| Log filter (U3) | 2 |
| **Total** | **≈67** |

That is over ~60, almost all because of U2 and `PRODUCT_NAME`. Ways to cut it:
1. **Declare more paths Strix-owned.** `.github/workflows/` (Strix already has its own
   `release`, `site`, `publish-release` and `upstream` workflows; fork surface 7 files, +812) and
   the progress docs Strix rewrites (`ROADMAP.md`, `docs/roadmap.md`, `docs/gaps.md`,
   `docs/*-parity.md`, `docs/target-app-parity.md`: fork surface on `docs/` is 36 files, +3069/−1470).
   This doesn't lower the rule count but removes the largest source of merge conflicts.
2. **Move the product-name strings into one Strix file.** `crates/ui-egui/src/brand.rs` (already
   Strix's) would hold `PRODUCT` and `PRODUCT_FULL`. Each upstream line gets one call instead of
   a rule. That's still a change to an upstream line, though (a conflict when upstream edits it).
3. **One scoped rule for U2** instead of ≈17 per-file ones (see U2): about 51 rules in total.

*Recommendation:* option 1 plus option 3, which gives about 51 rules and fewer conflicts.

## Phase 2: the rules as written

`xtask/src/upstream.rs` now has no substring renames. The tables:

| Table | What | Rules |
|---|---|---|
| `EXACT` | whole names in contents and paths: app id, 2 repository URLs, `apps/wordcraft*`, `app-icon/wordcraft*` (Strix's own icon files keep their names), `wordcraft-cli`, `wordcraft-web`, `wordcraft.exe`, `CARGO_BIN_EXE_wordcraft*` | 9 |
| `PRODUCT_WORD` | `WordCraft` as a whole word inside string literals of upstream `.rs` files | 1 |
| `PRODUCT_NAME` | "Strix Writer" where upstream shows its name as a title (the 13 `packaging/` rows went with the folder; the LaTeX and MCP rows also carry the U8 URLs; 2 are tests that check a title) | 24 |
| `STRIX_NAMES` | app packages, the desktop executable's own messages and their tests, user folders and stores, the log filter, site links, `serverInfo.name` and its test, the layers table | 28 |
| `STRIX_OWNED` | paths kept as the vendor branch has them | 9 paths |
| **Total** | | **62 rules** |

Decided while writing them (each is a case the inventory had missed or got wrong):
- **U2 is wider than proposed.** The proposal scoped the rule to `tl!()` in ui-egui and engine
  `cmd/`. The same running text is also in MCP errors (`crates/mcp/src/backend.rs`), Zotero
  (`crates/zotero/src/bridge.rs`), CLI and app messages, and engine I/O errors, and these aren't
  translated. So `PRODUCT_WORD` covers string literals in every upstream `.rs` file. It doesn't
  touch identifiers, comments or other formats. It also covers the folder names written with a
  capital (`Application Support/WordCraft`, `%APPDATA%\WordCraft`, `<base>/WordCraft/dictionaries`),
  eframe's app name (`"WordCraft"` in `apps/strix/src/main.rs`) and `registeredAppMachine=WordCraft`.
  None of those needs a row of its own.
- **The web app's preferences** are stored in `localStorage` under `KEY = "wordcraft.ui"`
  (`apps/strix-web/src/prefs.rs`). This is user data, so it is group b, not an egui id (group a, as the inventory had it).
- **The desktop executable names itself** in its usage text, `--version`, error lines and the
  tests that check them, and the MCP crate's `NEEDS_APP` hint tells users to start it. These are
  rows in `STRIX_NAMES`. So are `Exec=strix %F`, which the test reads from Strix's own `.desktop` file,
  the layers table (`("strix", Class::Exempt)`, `xtask/src/layers.rs`) and `CompanyName` in
  `apps/strix/build.rs` (upstream's publisher, "Learning Machines LLC").
- **Log filter (U3).** `main` already uses `"warn,wordcraft*=info,strix*=info"`. That works with
  today's names and with upstream's.
- **The layers check** strips `wordcraft-` from package names; the apps (`strix-cli`,
  `strix-web`) and Strix's crates (`strix-calc`, …) are named `strix-*`, so `short_name` strips
  both prefixes (one row; `main` already has the line).
- **Proved on a simulated snapshot.** I rebuilt upstream `9724f15c` by reversing the old rename on
  `vendor/wordcraft`, passed it through the new rules, and put back `packaging/` as the sync does.
  No rule is left unapplied. The whole workspace builds. Four test failures needed rules (two
  title checks, MCP `serverInfo.name`, the layers table), and those tests now pass. What's left
  fails for reasons outside the rules: the Zotero tests, because the sandbox runs them as another
  uid; the i18n tests, because the build has no craft-i18n catalogs; and
  `layout::tests::hit_and_caret_agree`, which fails the same way on today's vendor branch and
  which `main` already fixes in its own copy of the test.
- **62 rules is slightly over ~60.** The four added rows all exist so that tests and messages say
  what Strix is called. Declaring more paths Strix-owned wouldn't help: these rows live in core
  upstream files.

## Phase 3: the reverse rename on `main`

`cargo xtask upstream unrename UPSTREAM_DIR` (`xtask/src/unrename.rs`) did it once, from upstream
`9724f15c` (the commit `vendor/wordcraft` was made from):

1. It rebuilt the new vendor snapshot: upstream through the phase 2 rules, then rustfmt.
2. It put back upstream's whole names (`NAMES`, 85 entries: the 16 upstream crates' paths and
   packages, upstream identifiers and test names, the environment variables of the table above,
   the internal keys and URIs of U5–U7, `wordcraft.log`, `.wordcraft-save-`, `wordcraft.tsv`) in
   every file of `main` except this one and the rule tables. Strix's own names aren't in the table:
   `strix-calc`, `strix-cad`, `strix-dictionaries`, `strix_update::STRIX`, `STRIX_NS`,
   `STRIX_PRETEND_VERSION`, `strix:cad`, `strix:exercise`, …
3. For upstream code and configuration files, it merged three ways what the new rules do
   differently from the old rename (`git merge-file`; base: the old vendor file, theirs: the new
   one). Lines Strix never changed became exactly the new vendor's (comments back to
   "WordCraft", messages to Strix's). 15 files had lines both sides changed; Strix's lines were
   kept (test temp-file prefixes, the web loading screen, Strix's own update channel, the About
   box), which is right: they are Strix's changes.
4. Upstream prose (`.md`, `.txt`) only had its internal names put back. The new rules leave
   upstream's prose alone, so merging would have brought back upstream's sentences, and with
   them WordCraft's package and folder names (`cargo run -p wordcraft`, `~/.config/wordcraft` in
   `CLAUDE.md`), which are wrong in Strix.
5. `locales/*/strix.tsv` became `wordcraft.tsv` (U1). The one catalog key that named
   `STRIX_DICTIONARIES` and its 27 translations changed with the code.
6. After rustfmt, 680 of the 886 upstream files on the vendor branch are identical to the new
   vendor files and lost the modification notice (U10). The other 206 are the ones Strix really
   changes, and they keep it.

By hand, as feature code: the docx reader also accepts `urn:strix:canvas-xfrm` (U5), with a test
(`crates/docx/tests/it/strix_canvas_uri.rs`).

## Revision history

- 2026-11-09 — phase 3: the reverse rename on `main`, by `cargo xtask upstream unrename`.

- 2026-11-09 — phase 2: the rules as written (62), cases settled while writing them.

- 2026-11-09 — first inventory (phase 1).

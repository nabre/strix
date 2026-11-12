# Tracking WordCraft upstream

Status: in use since 2026-10-28; targeted rename rules and WordCraft's internal names on `main` since 2026-11-09 (`docs/naming.md`); WordCraft v0.17.0 merged by hand on 2026-11-10, upstream main up to `a42c9eaa` the same day; up to v0.18.0 (`a8db235e`) on 2026-11-11.

Strix is a fork of [WordCraft](https://github.com/storytold/wordcraft). It keeps WordCraft's
internal names (crates, Rust paths, types, environment variables) and is Strix only where users
see it or where it would collide with a WordCraft installed on the same computer: the app id, the
repository, the executables, the user's folders, the product name on screen
([`docs/naming.md`](naming.md)). Upstream reaches `main` through a **vendor branch** that applies
those few rules, so a merge only conflicts where both sides really changed a line.

> **Transition (2026-11-09).** The rules below replace the blind `wordcraft` → `strix` rename.
> `main`'s internal names went back to WordCraft's with `cargo xtask upstream unrename
> UPSTREAM_DIR` (phase 3 of `docs/naming.md`; `UPSTREAM_DIR` is a checkout of upstream
> `9724f15c`, the commit the old vendor branch was made from). Before the next `upstream sync`,
> `vendor/wordcraft` is re-created with the new rules: delete it, `sync` `d859c673` and merge it
> into `main` with `git merge -s ours`, `sync` `9724f15c` and merge that with `-s ours` too
> (`main` already holds that snapshot, under the new names). From then on `sync` and `git merge`
> work as below.
>
> **v0.17.0 merged by hand (2026-11-10).** Upstream `5574b326` (WordCraft v0.17.0) went into `main`
> file by file, without a vendor commit: both snapshots (`9724f15c` and `5574b326`) were passed
> through `cargo xtask upstream rebrand`, files Strix hadn't changed took upstream's version, the
> others were merged three-way and by hand. So `main` already holds v0.17.0: when
> `vendor/wordcraft` is re-created as above, also `sync` `5574b326c` and merge it with `git merge -s
> ours`; the next real merge starts from there.
>
> **Up to `a42c9eaa` (2026-11-10).** The 17 upstream commits after v0.17.0 (to `a42c9eaa`: strict
> PDF/PNG export, `--strict` on `convert` and `render`, ODT fixes for gap 19, RTF style-sheet
> names, pasted CSS colours with alpha, the macOS title bar behind a modal dialog) went in as
> upstream's diff `5574b326..a42c9eaa`, with `wordcraft-cli` → `strix-cli` applied to it; six
> files Strix had changed were merged three-way. Upstream's `Cargo.toml` version (back to
> 0.15.1) and Strix-owned files were left out. So `sync` `a42c9eaa` and merge it with `git merge
> -s ours` too when `vendor/wordcraft` is re-created.
>
> **Up to v0.18.0 (2026-11-11).** The 26 upstream commits after `a42c9eaa` (to `a8db235e`,
> WordCraft v0.18.0: the Date & Time dialog, page numbers that keep a header's content, Compare's
> chunked alignment, equations' revision marks, line numbers in columns, native Wayland without
> the X11 libraries, the fsync fallback on Apple, the ribbon's hover fades) went in the same way:
> both snapshots passed through `cargo xtask upstream rebrand`, the 25 files Strix hadn't changed
> took upstream's version, the 27 others were merged three-way (one conflict, `strix-cli`'s usage
> text). Upstream's version bump (0.18.0), Strix-owned files (workflows' Harden-Runner steps,
> parity docs) were left out; the Linux package recommendations were ported by hand. So `sync`
> `a8db235e` and merge it with `git merge -s ours` too when `vendor/wordcraft` is re-created.

## How it works

```
upstream (storytold/wordcraft)   d859c673 ─────────────── 9724f15c ─── …
                                     │ cargo xtask upstream sync │
vendor/wordcraft                 rebranded(d859c673) ─── rebranded(9724f15c) ─── …
                                     │ (merge -s ours, history only)     │ git merge
main                              … ─┴── Strix work ──────────────────────┴── …
```

- Every commit on `vendor/wordcraft` is one upstream snapshot passed through the rules in
  [`xtask/src/upstream.rs`](../xtask/src/upstream.rs); its `Upstream-Commit:` trailer names the
  upstream commit. There are no substring renames:
  - `EXACT`: whole names, in contents and paths: `ai.storyteller.wordcraft` →
    `io.github.nabre.strix`, `storytold/wordcraft` → `nabre/strix`, `apps/wordcraft*` →
    `apps/strix*`, `app-icon/wordcraft*` → `app-icon/strix*`, the executables `wordcraft-cli`,
    `wordcraft-web`, `wordcraft.exe` and `CARGO_BIN_EXE_wordcraft*`.
  - `PRODUCT_WORD`: in upstream Rust files, the word `WordCraft` inside a string literal (never an
    identifier or a comment) becomes `Strix`, so every message, label and capitalised folder name
    says Strix.
  - `PRODUCT_NAME` and `STRIX_NAMES`: `(file, upstream text, Strix's text)` rows, applied before
    the others. The product's full name, **Strix Writer**, where upstream shows its name as such
    (window and dialog titles, the generator recorded in exported files); the app packages'
    names; the lowercase user folders and stores; the desktop executable's own messages; the MCP
    `serverInfo.name`; links to WordCraft's site.
  - `STRIX_OWNED`: `packaging/`, `site/`, `.github/workflows/`, `README.md`, `docs/README.*.md`,
    `ROADMAP.md`, `docs/roadmap.md`, `docs/gaps.md`, `docs/*-parity.md` are Strix's own: the sync
    keeps them as the vendor branch already had them and lists the upstream commits that touched
    them, to review and port by hand. The Windows installer's `UpgradeCode` never changes.

  A new upstream string that names the product as a title gets a `PRODUCT_NAME` row, and the
  same text in `main`'s file (a test checks that every row's result is in `main`). When upstream
  changes a line a row matches, the sync warns that the rule didn't apply. The changed Rust files
  are then formatted with the workspace's `rustfmt.toml`; without `rustfmt` on the PATH the sync
  warns and skips it.
- Licence rules are built in: `LICENSE-*` and `NOTICE` stay byte-for-byte, lines with a
  copyright or attribution notice are never renamed, every file whose contents the rules change gets the "Modified by the
  Strix contributors" line (a file that only moves doesn't) (Apache-2.0 §4(b), (c)), and the ArtCraft brand files in `docs/brand/`
  never enter the branch.
- The branch's first commit is the true fork point, upstream `d859c673` (2026-10-26), and `main`
  records it with a history-only merge. From then on `git merge vendor/wordcraft` is an ordinary
  three-way merge: only lines both sides really changed conflict.
- `git diff vendor/wordcraft main` is exactly what Strix changes on top of upstream (the *fork
  surface*).

## Updating from upstream

**From GitHub:** Actions › **Upstream WordCraft** › Run workflow (`.github/workflows/upstream.yml`),
with a WordCraft release tag (`v0.16.0`) or commit, or empty for WordCraft's latest release. It
commits the rebranded snapshot on `vendor/wordcraft`, pushes it and opens a pull request into
`main`: from `upstream/wordcraft-<version>` with the merge done and CI started when it merges
cleanly, otherwise from `vendor/wordcraft` with the conflicting files listed, to resolve on a
branch as below. (Settings › Actions › General must let GitHub Actions create pull requests.)

**By hand:**

```sh
cargo xtask upstream status        # how far behind, fork surface
cargo xtask upstream sync          # commit rebranded upstream/main onto vendor/wordcraft
git switch -c upstream-merge
git merge vendor/wordcraft         # resolve conflicts (see below)
cargo xtask ci
git switch main && git merge upstream-merge
git push origin main vendor/wordcraft
```

`sync REV` takes any upstream commit (merge in steps when upstream moved a lot). The `upstream`
remote is added on first use with pushing disabled; offline, `sync` uses the refs fetched before.

Resolving:
- `Cargo.lock`: take either side, then let `cargo` rewrite it (`cargo xtask ci` does).
- A file upstream deleted or split that Strix modified (modify/delete): move the Strix change
  into upstream's new file, then `git rm` the old one.
- New upstream user-visible strings land in the catalogs through craft-i18n; Strix's own strings
  stay in `locales/*/wordcraft.tsv`.
- Branding that is not a plain rename (README, site, store text, About box) stays Strix's.
- When upstream splits a file Strix changed (it moved inline test modules and big files into
  submodules in 2026-10), git can't follow the moved code: take upstream's version of the file
  and re-apply Strix's own change to it (`git diff <vendor base> main -- FILE`) where the code
  now lives. Check afterwards that no command id (`CommandSpec::new("…"`), `ui.*` command or
  interface string Strix had is missing.
- Dependency bumps upstream hasn't made yet collide with upstream's new code: keep shared
  dependencies at upstream's versions and bump them after the merge. A bump whose new API would
  change upstream code (harfrust, skrifa, quick-xml, vello_cpu, hayro in 2026-10) waits for
  upstream; `.github/dependabot.yml` ignores those.

If the rename rules ever change, re-create the branch: delete `vendor/wordcraft`, run
`cargo xtask upstream sync d859c673c1e93bccaa6536fa3653b902ea877cea`, merge it into `main` with
`git merge -s ours`, then `sync` again.

## Keeping the fork easy to merge

Every line Strix changes in an upstream file is a potential conflict at the next merge. Measured
2026-10-28: Strix adds 111 files of its own and changes 195 upstream files; merging upstream's
186 commits since the fork (WordCraft `9724f15c`, merged that day) conflicted in 49 files, most
of them because upstream had moved the code Strix edited into new submodules. Strix's ribbon
groups moved into their own `crates/ui-egui/src/ribbon/strix.rs` in that merge.

- **New code goes in new files.** Strix features live in their own crates (`calc`, `cad`) and
  modules (`cmd/calc.rs`, `cmd/exercise.rs`, `cad_editor.rs`, `graph_pane.rs`); an upstream file
  only gets the few lines that register or call them (a `mod`, a `CommandSpec` entry, a ribbon
  button).
- **Don't restructure upstream files** — no reformatting, reordering, renaming or splitting of
  code Strix did not write, even when it would read better. Upstream does that kind of work too
  (its strict-Clippy refactor touched every crate) and both versions would collide. Refactor
  Strix's own files freely.
- **Upstream bugs**: fix them in Strix if needed, and send the fix upstream too, so the line
  disappears from the fork surface at the next merge.
- **Brand text only through the rules.** Don't hand-edit `WordCraft` → `Strix` in upstream
  files; add a row to `PRODUCT_NAME` or `STRIX_NAMES` instead (`docs/naming.md` says which names
  stay Strix's), so the vendor branch does the same and the edit never conflicts. Internal names
  stay upstream's.
- **Dead code**: `cargo xtask unused` reports unused dependencies and the public items in
  Strix's own files that nothing, or only tests, uses; remove those. Unused code in upstream
  files stays: deleting it is a change upstream doesn't have.
- **Merge often.** Small, frequent merges are cheap; months of drift are not.

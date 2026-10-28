# Tracking WordCraft upstream

Status: in use since 2026-10-28.

Strix is a fork of [WordCraft](https://github.com/storytold/wordcraft) with every product name,
crate name, app id and URL renamed. A direct `git merge` of upstream would conflict on thousands
of renamed identifiers, so upstream reaches `main` through a **vendor branch**.

## How it works

```
upstream (storytold/wordcraft)   d859c673 ─────────────── 9724f15c ─── …
                                     │ cargo xtask upstream sync │
vendor/wordcraft                 rebranded(d859c673) ─── rebranded(9724f15c) ─── …
                                     │ (merge -s ours, history only)     │ git merge
main                              … ─┴── Strix work ──────────────────────┴── …
```

- Every commit on `vendor/wordcraft` is one upstream snapshot passed through the mechanical
  rename in [`xtask/src/upstream.rs`](../xtask/src/upstream.rs) (`RENAMES`): `WordCraft` →
  `Strix`, `wordcraft_*` → `strix_*`, `apps/wordcraft*` → `apps/strix*`,
  `ai.storyteller.wordcraft` → `io.github.nabre.strix`, `storytold/wordcraft` → `nabre/strix`.
  The commit's `Upstream-Commit:` trailer names the upstream commit. The renamed Rust files are
  then formatted with the workspace's `rustfmt.toml` (shorter names let lines upstream wrapped
  fit on one, as in `main`); without `rustfmt` on the PATH the sync warns and skips it.
- Licence rules are built in: `LICENSE-*` and `NOTICE` stay byte-for-byte, lines with a
  copyright or attribution notice are never renamed, every renamed file gets the "Modified by the
  Strix contributors" line (Apache-2.0 §4(b), (c)), and the ArtCraft brand files in `docs/brand/`
  never enter the branch.
- The branch's first commit is the true fork point, upstream `d859c673` (2026-10-26), and `main`
  records it with a history-only merge. From then on `git merge vendor/wordcraft` is an ordinary
  three-way merge: only lines both sides really changed conflict.
- `git diff vendor/wordcraft main` is exactly what Strix changes on top of upstream (the *fork
  surface*).

## Updating from upstream

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
  stay in `locales/*/strix.tsv`.
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
- **Brand text only through the rename.** Don't hand-edit `WordCraft` → `Strix` in upstream
  files; extend `RENAMES` instead, so the vendor branch does the same and the edit never
  conflicts.
- **Dead code**: `cargo xtask unused` reports unused dependencies and the public items in
  Strix's own files that nothing, or only tests, uses; remove those. Unused code in upstream
  files stays: deleting it is a change upstream doesn't have.
- **Merge often.** Small, frequent merges are cheap; months of drift are not.

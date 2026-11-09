//! `cargo xtask upstream unrename UPSTREAM_DIR`: brings `main`'s internal names back to WordCraft's
//! (phase 3 of `docs/naming.md`), once, reproducibly.
//!
//! Until 2026-11, the upstream sync renamed every `wordcraft` to `strix`, so `main` holds Strix
//! names for WordCraft's crates, packages, environment variables and internal keys. The sync now
//! applies only the targeted rules of [`crate::upstream`]. This command moves `main` onto the
//! same names, so that the next `git merge vendor/wordcraft` only shows real changes:
//!
//! 1. It rebuilds the new vendor snapshot: `UPSTREAM_DIR` (a checkout of the upstream commit
//!    named by the vendor branch's `Upstream-Commit:` trailer) through the new rules and rustfmt.
//! 2. In every file of `main` it puts back upstream's whole names from [`NAMES`]. Strix's own names
//!    (`strix-calc`, `strix_update::STRIX`, `STRIX_NS`, `strix:cad`, …) aren't in the table.
//! 3. In every upstream file it then merges, three ways, what the new rules do differently from
//!    the old rename (`git merge-file`: ours = `main` after step 2, base = the old vendor file
//!    after step 2, theirs = the new vendor file). Lines Strix changed keep Strix's text; the
//!    others become exactly the new vendor's, comments and messages included. Prose files
//!    (`.md`, `.txt`) skip this step and keep Strix's text ([`is_prose`]).
//! 4. Catalogs `locales/*/strix.tsv` become `wordcraft.tsv`, upstream's catalog name (U1).
//! 5. rustfmt on the Rust files it changed; then each upstream file that ends up identical to
//!    the new vendor file takes it as is (without the modification notice, U10), and every other
//!    one carries the notice.
//!
//! It prints the files where Strix and the new rules changed the same lines (kept as Strix had
//! them) for review.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::upstream::{
    MARK, NOTICE_MARKERS, VENDOR_BRANCH, VERBATIM_FILES, format_rust, git, git_ok, rebrand_tree, strix_owned, walk, with_mark, word_char,
};

/// Whole names that go back to upstream's, `(Strix name, upstream name)`. A name matches only
/// where the characters around it can't continue it (`strix-doc` isn't `strix-docx`).
pub(crate) const NAMES: &[(&str, &str)] = &[
    // Crate paths of upstream's crates.
    ("strix_control_key", "wordcraft_control_key"),
    ("strix_docbin", "wordcraft_docbin"),
    ("strix_doc", "wordcraft_doc"),
    ("strix_docx", "wordcraft_docx"),
    ("strix_engine", "wordcraft_engine"),
    ("strix_fonts", "wordcraft_fonts"),
    ("strix_formats", "wordcraft_formats"),
    ("strix_geom", "wordcraft_geom"),
    ("strix_layout", "wordcraft_layout"),
    ("strix_mcp", "wordcraft_mcp"),
    ("strix_metafile", "wordcraft_metafile"),
    ("strix_pdf", "wordcraft_pdf"),
    ("strix_proof", "wordcraft_proof"),
    ("strix_render", "wordcraft_render"),
    ("strix_ui_egui", "wordcraft_ui_egui"),
    ("strix_zotero", "wordcraft_zotero"),
    // Their package names.
    ("strix-control-key", "wordcraft-control-key"),
    ("strix-docbin", "wordcraft-docbin"),
    ("strix-doc", "wordcraft-doc"),
    ("strix-docx", "wordcraft-docx"),
    ("strix-engine", "wordcraft-engine"),
    ("strix-fonts", "wordcraft-fonts"),
    ("strix-formats", "wordcraft-formats"),
    ("strix-geom", "wordcraft-geom"),
    ("strix-layout", "wordcraft-layout"),
    ("strix-mcp", "wordcraft-mcp"),
    ("strix-metafile", "wordcraft-metafile"),
    ("strix-pdf", "wordcraft-pdf"),
    ("strix-proof", "wordcraft-proof"),
    ("strix-render", "wordcraft-render"),
    ("strix-ui-egui", "wordcraft-ui-egui"),
    ("strix-zotero", "wordcraft-zotero"),
    // Upstream's identifiers and test names.
    ("strix_canvas", "wordcraft_canvas"),
    ("strix_ibeam", "wordcraft_ibeam"),
    ("strix_loading", "wordcraft_loading"),
    ("strix_print_settings", "wordcraft_print_settings"),
    ("strix_viewport_input", "wordcraft_viewport_input"),
    ("storytold_strix", "storytold_wordcraft"),
    ("zxqstrixy", "zxqwordcrafty"),
    ("settings_written_by_strix_keep_no_original", "settings_written_by_wordcraft_keep_no_original"),
    ("the_default_filter_shows_strix_info_and_other_crates_warnings", "the_default_filter_shows_wordcraft_info_and_other_crates_warnings"),
    ("the_editing_languages_are_the_ones_strix_can_proof", "the_editing_languages_are_the_ones_wordcraft_can_proof"),
    // Environment variables (`docs/naming.md`, all but Strix's `STRIX_PRETEND_VERSION`).
    ("STRIX_BENCH_DOCX", "WORDCRAFT_BENCH_DOCX"),
    ("STRIX_BUILD_DATE", "WORDCRAFT_BUILD_DATE"),
    ("STRIX_BUILD_SHA", "WORDCRAFT_BUILD_SHA"),
    ("STRIX_CARET_GLIDE_MS", "WORDCRAFT_CARET_GLIDE_MS"),
    ("STRIX_CONTROL_KEY", "WORDCRAFT_CONTROL_KEY"),
    ("STRIX_CONTROL_PORT", "WORDCRAFT_CONTROL_PORT"),
    ("STRIX_DICTIONARIES", "WORDCRAFT_DICTIONARIES"),
    ("STRIX_DOCBIN_FIXTURES", "WORDCRAFT_DOCBIN_FIXTURES"),
    ("STRIX_FILE_DIALOG", "WORDCRAFT_FILE_DIALOG"),
    ("STRIX_LANGUAGE", "WORDCRAFT_LANGUAGE"),
    ("STRIX_MAINTAINER", "WORDCRAFT_MAINTAINER"),
    ("STRIX_NO_PREFS", "WORDCRAFT_NO_PREFS"),
    ("STRIX_PASSWORD", "WORDCRAFT_PASSWORD"),
    ("STRIX_PROFILE_DIR", "WORDCRAFT_PROFILE_DIR"),
    ("STRIX_REGION", "WORDCRAFT_REGION"),
    ("STRIX_REQUIRE_WINRES", "WORDCRAFT_REQUIRE_WINRES"),
    ("STRIX_ROTATION_SHOT", "WORDCRAFT_ROTATION_SHOT"),
    ("STRIX_SPEECH", "WORDCRAFT_SPEECH"),
    ("STRIX_TEST_KAI_FONT", "WORDCRAFT_TEST_KAI_FONT"),
    ("STRIX_VERSION", "WORDCRAFT_VERSION"),
    ("STRIX_WAYLAND", "WORDCRAFT_WAYLAND"),
    // Internal keys and interfaces (U5, U6, U7), the log file, the safe-save prefix, the catalog.
    ("urn:strix:canvas-xfrm", "urn:wordcraft:canvas-xfrm"),
    ("strix://document", "wordcraft://document"),
    ("strix://commands", "wordcraft://commands"),
    ("strix:open", "wordcraft:open"),
    ("strix:ready", "wordcraft:ready"),
    ("strix:save", "wordcraft:save"),
    ("strix:dirty", "wordcraft:dirty"),
    ("strix:failed", "wordcraft:failed"),
    ("strix:settings.orig", "wordcraft:settings.orig"),
    ("strix:app.kept", "wordcraft:app.kept"),
    ("strix:vbaProject.related", "wordcraft:vbaProject.related"),
    ("strix:glossary.read", "wordcraft:glossary.read"),
    ("strix:embedded.parts", "wordcraft:embedded.parts"),
    ("strix:customXml.roots", "wordcraft:customXml.roots"),
    ("strix:unmodelled.roots", "wordcraft:unmodelled.roots"),
    ("strix.colorsDialogOpen", "wordcraft.colorsDialogOpen"),
    ("strix.colorsDialog", "wordcraft.colorsDialog"),
    ("strix.moreColorsRequest", "wordcraft.moreColorsRequest"),
    ("strix.recentColors", "wordcraft.recentColors"),
    ("strix.log", "wordcraft.log"),
    (".strix-save-", ".wordcraft-save-"),
    ("strix.tsv", "wordcraft.tsv"),
];

/// Files left as they are: the ones that describe the names themselves, and the sandbox's.
const UNTOUCHED: &[&str] = &["docs/naming.md", "xtask/src/upstream.rs", "xtask/src/unrename.rs", ".sandbox/", ".vscode/"];

/// `line` with every [`NAMES`] entry replaced where it is a whole name.
pub(crate) fn names_back(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut rest = line;
    let mut prev: Option<char> = None;
    'scan: while let Some(c) = rest.chars().next() {
        if !prev.is_some_and(word_char) {
            for (from, to) in NAMES {
                let Some(after) = rest.strip_prefix(from) else { continue };
                let open_end = from.chars().last().is_some_and(word_char);
                if open_end && after.chars().next().is_some_and(word_char) {
                    continue;
                }
                out.push_str(to);
                prev = from.chars().last();
                rest = after;
                continue 'scan;
            }
        }
        out.push(c);
        prev = Some(c);
        rest = &rest[c.len_utf8()..];
    }
    out
}

/// Prose (`.md`, `.txt`): the new rules leave upstream's running text alone there, so upstream's
/// lines would say WordCraft and name WordCraft's packages and folders. Strix's docs keep their
/// text; only [`NAMES`] go back.
fn is_prose(path: &str) -> bool {
    path.ends_with(".md") || path.ends_with(".txt")
}

/// `text` through [`names_back`], keeping copyright and attribution lines as they are.
pub(crate) fn text_back(text: &str) -> String {
    text.split_inclusive('\n').map(|line| if NOTICE_MARKERS.iter().any(|m| line.contains(m)) { line.to_string() } else { names_back(line) }).collect()
}

/// `path` with the catalog name back to upstream's (`locales/<locale>/strix.tsv`, U1).
pub(crate) fn path_back(path: &str) -> String {
    match path.strip_prefix("locales/").and_then(|r| r.strip_suffix("/strix.tsv")) {
        Some(locale) if !locale.contains('/') => format!("locales/{locale}/wordcraft.tsv"),
        _ => path.to_string(),
    }
}

/// `text` without the modification notice line near its top.
pub(crate) fn without_mark(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for (i, line) in text.split_inclusive('\n').enumerate() {
        if i < 15 && line.contains(MARK) {
            continue;
        }
        out.push_str(line);
    }
    out
}

pub fn run(root: &Path, upstream: &Path) -> Result<(), String> {
    if !upstream.join("apps/wordcraft/Cargo.toml").is_file() {
        return Err(format!("{}: not a WordCraft checkout (no apps/wordcraft/Cargo.toml)", upstream.display()));
    }
    let vendor = vendor_ref(root)?;
    let scratch = std::env::temp_dir().join(format!("strix-unrename-{}", std::process::id()));
    let result = unrename(root, upstream, &vendor, &scratch);
    let _ = std::fs::remove_dir_all(&scratch);
    result
}

/// The vendor branch, local or as fetched from `origin`.
fn vendor_ref(root: &Path) -> Result<String, String> {
    [VENDOR_BRANCH.to_string(), format!("origin/{VENDOR_BRANCH}")]
        .into_iter()
        .find(|r| git_ok(root, &["rev-parse", "--verify", "--quiet", r]))
        .ok_or_else(|| format!("no {VENDOR_BRANCH} branch"))
}

fn unrename(root: &Path, upstream: &Path, vendor: &str, scratch: &Path) -> Result<(), String> {
    let new_vendor = scratch.join("vendor");
    copy_tree(upstream, &new_vendor)?;
    rebrand_tree(&new_vendor)?;
    let mut rust = Vec::new();
    walk(&new_vendor, &new_vendor, &mut rust)?;
    rust.retain(|p| p.ends_with(".rs"));
    format_rust(&new_vendor, &rust.iter().map(String::as_str).collect::<Vec<_>>());
    println!("new vendor snapshot built from {}", upstream.display());

    let old_vendor: BTreeSet<String> = git(root, &["ls-tree", "-r", "--name-only", vendor])?.lines().map(str::to_string).collect();
    let files = git(root, &["ls-files"])?;
    let mut changed_rust = Vec::new();
    let mut upstream_files = Vec::new();
    let mut overlaps = Vec::new();
    let (mut moved, mut rewritten) = (0, 0);
    for path in files.lines() {
        if UNTOUCHED.iter().any(|u| if u.ends_with('/') { path.starts_with(u) } else { path == *u }) || VERBATIM_FILES.contains(&path) {
            continue;
        }
        let Ok(bytes) = std::fs::read(root.join(path)) else { continue };
        let Some(text) = std::str::from_utf8(&bytes).ok().filter(|t| !t.contains('\0')) else { continue };
        let ours = text_back(text);
        let theirs_path = new_vendor.join(path);
        let new = if old_vendor.contains(path) && !strix_owned(path) && theirs_path.is_file() {
            let theirs = std::fs::read_to_string(&theirs_path).map_err(|e| format!("{path}: {e}"))?;
            if is_prose(path) {
                upstream_files.push((path.to_string(), theirs));
                ours
            } else {
                let base = text_back(&show(root, vendor, path)?);
                let (merged, clean) = merge3(scratch, &without_mark(&ours), &without_mark(&base), &without_mark(&theirs))?;
                if !clean {
                    overlaps.push(path.to_string());
                }
                upstream_files.push((path.to_string(), theirs));
                merged
            }
        } else {
            ours
        };
        let new_path = path_back(path);
        if new_path != path {
            std::fs::rename(root.join(path), root.join(&new_path)).map_err(|e| format!("{path}: {e}"))?;
            moved += 1;
        }
        if new != text {
            std::fs::write(root.join(&new_path), &new).map_err(|e| format!("{new_path}: {e}"))?;
            rewritten += 1;
            if new_path.ends_with(".rs") {
                changed_rust.push(new_path);
            }
        }
    }
    format_rust(root, &changed_rust.iter().map(String::as_str).collect::<Vec<_>>());

    // The modification notice: only on upstream files that still differ from the new vendor's.
    let (mut same, mut marked) = (0, 0);
    for (path, theirs) in &upstream_files {
        let now = std::fs::read_to_string(root.join(path)).map_err(|e| format!("{path}: {e}"))?;
        let body = without_mark(&now);
        let new = if body == without_mark(theirs) {
            same += 1;
            theirs.clone()
        } else {
            marked += 1;
            with_mark(path, body)
        };
        if new != now {
            std::fs::write(root.join(path), new).map_err(|e| format!("{path}: {e}"))?;
        }
    }
    println!(
        "{rewritten} file(s) rewritten, {moved} catalog(s) renamed; of {} upstream files, {same} are now upstream's (no notice), {marked} keep Strix's changes",
        upstream_files.len()
    );
    if !overlaps.is_empty() {
        println!("Strix and the new rules changed the same lines here (Strix's kept; review):");
        for p in &overlaps {
            println!("  {p}");
        }
    }
    Ok(())
}

/// The contents of `path` on `rev`, as they are (no trimming).
fn show(root: &Path, rev: &str, path: &str) -> Result<String, String> {
    let out = Command::new("git").current_dir(root).args(["show", &format!("{rev}:{path}")]).output().map_err(|e| format!("git show: {e}"))?;
    if !out.status.success() {
        return Err(format!("git show {rev}:{path}: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    String::from_utf8(out.stdout).map_err(|_| format!("{rev}:{path}: not UTF-8"))
}

/// Three-way merge of `ours` and `theirs` from `base` (`git merge-file`); where both changed the
/// same lines, ours wins and the second value is false.
fn merge3(scratch: &Path, ours: &str, base: &str, theirs: &str) -> Result<(String, bool), String> {
    if ours == base || base == theirs {
        return Ok((if ours == base { theirs } else { ours }.to_string(), true));
    }
    let files: Vec<PathBuf> = ["ours", "base", "theirs"].iter().map(|n| scratch.join(n)).collect();
    for (f, t) in files.iter().zip([ours, base, theirs]) {
        std::fs::write(f, t).map_err(|e| format!("{}: {e}", f.display()))?;
    }
    let merge = |extra: &[&str]| {
        Command::new("git").arg("merge-file").arg("-p").args(extra).args(&files).output().map_err(|e| format!("git merge-file: {e}"))
    };
    let first = merge(&[])?;
    match first.status.code() {
        Some(0) => String::from_utf8(first.stdout).map(|s| (s, true)).map_err(|e| e.to_string()),
        Some(n) if n > 0 => {
            let ours_wins = merge(&["--ours"])?;
            String::from_utf8(ours_wins.stdout).map(|s| (s, false)).map_err(|e| e.to_string())
        }
        _ => Err(format!("git merge-file: {}", String::from_utf8_lossy(&first.stderr).trim())),
    }
}

/// Copies the files under `from` (not `.git`) to `to`.
fn copy_tree(from: &Path, to: &Path) -> Result<(), String> {
    let mut files = Vec::new();
    walk(from, from, &mut files)?;
    for rel in files {
        let dest = to.join(&rel);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        std::fs::copy(from.join(&rel), &dest).map_err(|e| format!("{rel}: {e}"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upstream_names_go_back_and_strix_names_stay() {
        for (strix, upstream) in [
            ("use strix_doc::Doc;", "use wordcraft_doc::Doc;"),
            ("strix-docx = { workspace = true }", "wordcraft-docx = { workspace = true }"),
            ("cargo test -p strix-ui-egui", "cargo test -p wordcraft-ui-egui"),
            ("var_os(\"STRIX_DICTIONARIES\")", "var_os(\"WORDCRAFT_DICTIONARIES\")"),
            ("\"strix://document\"", "\"wordcraft://document\""),
            ("locales/it/strix.tsv", "locales/it/wordcraft.tsv"),
            // Strix's own names, and the ones the vendor rules keep Strix's.
            ("use strix_calc::Quantity; strix-cad strix_update::STRIX", "use strix_calc::Quantity; strix-cad strix_update::STRIX"),
            ("STRIX_PRETEND_VERSION STRIX_NS strix:cad strix:exercise", "STRIX_PRETEND_VERSION STRIX_NS strix:cad strix:exercise"),
            ("apps/strix-cli strix-web CARGO_BIN_EXE_strix \"strix\" Strix", "apps/strix-cli strix-web CARGO_BIN_EXE_strix \"strix\" Strix"),
            ("strix-dictionaries my_strix_doc strix_docs", "strix-dictionaries my_strix_doc strix_docs"),
        ] {
            assert_eq!(names_back(strix), upstream);
        }
    }

    #[test]
    fn notice_lines_and_catalog_paths() {
        assert_eq!(text_back("// Copyright strix_doc\nstrix_doc\n"), "// Copyright strix_doc\nwordcraft_doc\n");
        assert_eq!(path_back("locales/pt-BR/strix.tsv"), "locales/pt-BR/wordcraft.tsv");
        assert_eq!(path_back("locales/strix.tsv"), "locales/strix.tsv");
        assert_eq!(path_back("crates/x/strix.tsv"), "crates/x/strix.tsv");
    }

    #[test]
    fn the_mark_is_stripped_near_the_top_only() {
        let text = format!("// {MARK}\nfn f() {{}}\n");
        assert_eq!(without_mark(&text), "fn f() {}\n");
    }
}

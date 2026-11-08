//! `cargo xtask upstream <status|sync [REV]|rebrand DIR>`: keep Strix mergeable with WordCraft.
//!
//! Strix is a fork of WordCraft with every product name, crate name, app id and URL rebranded.
//! Merging upstream directly would conflict on thousands of renamed identifiers, so upstream is
//! tracked through a *vendor branch* (`vendor/wordcraft`): each commit on it is an upstream
//! snapshot passed through [`rebrand_text`] and [`rebrand_path`], the same mechanical rename the
//! fork made. `main` merges that branch, so git's three-way merge only sees real changes on
//! both sides. The workflow is in `docs/upstream.md`.

use std::path::Path;
use std::process::Command;

/// Upstream repository and the local names that track it.
pub const UPSTREAM_URL: &str = "https://github.com/storytold/wordcraft.git";
const REMOTE: &str = "upstream";
const UPSTREAM_BRANCH: &str = "main";
pub const VENDOR_BRANCH: &str = "vendor/wordcraft";
/// Trailer naming the upstream commit a vendor commit was made from.
const TRAILER: &str = "Upstream-Commit:";

/// Literal replacements, longest and most specific first. Applied to file contents and paths.
const RENAMES: &[(&str, &str)] = &[
    ("ai.storyteller.wordcraft", "io.github.nabre.strix"),
    ("github.com/storytold/wordcraft", "github.com/nabre/strix"),
    ("storytold/wordcraft", "nabre/strix"),
    // Upstream's publisher; Strix is published by its contributors.
    ("Learning Machines LLC", "The Strix contributors"),
    ("<developer id=\"ai.storyteller\">", "<developer id=\"io.github.nabre\">"),
    ("WordCraft", "Strix"),
    ("Wordcraft", "Strix"),
    ("WORDCRAFT", "STRIX"),
    ("wordcraft", "strix"),
];

/// Upstream paths that never enter the vendor branch: the ArtCraft brand files, whose licence
/// (docs/brand/LICENSE-brand.txt upstream) requires forks to remove them.
const EXCLUDED_PREFIXES: &[&str] = &["docs/brand/"];

/// Files kept byte-for-byte: Apache-2.0 §4(a) and §4(c) forbid rewording licence texts and
/// upstream notices.
const VERBATIM_FILES: &[&str] = &["LICENSE-APACHE", "LICENSE-MIT", "NOTICE"];

/// Lines holding a copyright or attribution notice are never rebranded (§4(c)), e.g.
/// "Copyright (c) 2026 ArtCraft Team and the WordCraft contributors" or an ATTRIBUTION.md row
/// crediting the "WordCraft contributors".
const NOTICE_MARKERS: &[&str] = &["Copyright", "copyright", "©", "WordCraft contributors", "WordCraft authors"];

/// The modification notice every changed upstream file carries (§4(b)); same text as `main`.
const MARK: &str = "Modified by the Strix contributors (fork of WordCraft); see NOTICE.";

/// Where upstream shows its product name as such (window and dialog titles, installers, store
/// entries, the generator recorded in exported files), Strix shows its full name, "Strix Writer";
/// in running text the short "Strix" stays. Applied after [`RENAMES`], to the files named:
/// `(path, rebranded text, Strix's text)`. A pattern may span lines.
const PRODUCT_NAME: &[(&str, &str, &str)] = &[
    ("apps/strix/build.rs", "\"ProductName\", \"Strix\"", "\"ProductName\", \"Strix Writer\""),
    ("apps/strix/build.rs", "\"Strix word processor\"", "\"Strix Writer word processor\""),
    ("apps/strix/src/main.rs", "strix — the Strix desktop app", "strix — the Strix Writer desktop app"),
    ("apps/strix/src/main.rs", "log::info!(\"Strix {} (", "log::info!(\"Strix Writer {} ("),
    ("apps/strix/src/main.rs", ".with_title(\"Strix\")", ".with_title(\"Strix Writer\")"),
    ("apps/strix-web/index.html", "<title>Strix</title>", "<title>Strix Writer</title>"),
    ("crates/docx/src/write/settings.rs", "\"Application\", &[], \"Strix\"", "\"Application\", &[], \"Strix Writer\""),
    ("crates/engine/src/sample.rs", "creator = \"Strix\"", "creator = \"Strix Writer\""),
    ("crates/formats/src/html/export.rs", "content=\\\"Strix\\\"", "content=\\\"Strix Writer\\\""),
    ("crates/formats/src/latex/export.rs", "% Written by Strix (", "% Written by Strix Writer ("),
    ("crates/formats/src/odt/package.rs", "<meta:generator>Strix<", "<meta:generator>Strix Writer<"),
    ("crates/formats/src/rtf/export.rs", "\\\\generator Strix;", "\\\\generator Strix Writer;"),
    ("crates/mcp/src/server.rs", "\"title\": \"Strix\"", "\"title\": \"Strix Writer\""),
    ("crates/mcp/src/server.rs", "\"Strix is a word processor", "\"Strix Writer is a word processor"),
    ("crates/pdf/src/metadata.rs", "creator: \"Strix\".into(), producer: \"Strix\"", "creator: \"Strix Writer\".into(), producer: \"Strix Writer\""),
    ("crates/ui-egui/src/chrome.rs", "    \"Strix\".to_string()", "    \"Strix Writer\".to_string()"),
    ("crates/ui-egui/src/dialogs/chrome.rs", "\"About Strix\"", "\"About Strix Writer\""),
    ("crates/ui-egui/src/dialogs/chrome.rs", "SaveChanges { .. } => \"Strix\"", "SaveChanges { .. } => \"Strix Writer\""),
    ("crates/ui-egui/src/updates_ui.rs", "tl!(\"Strix {version} is", "tl!(\"Strix Writer {version} is"),
    ("crates/ui-egui/src/windows.rs", " - Strix\"", " - Strix Writer\""),
    ("crates/ui-egui/src/windows.rs", "return \"Strix\".to_string()", "return \"Strix Writer\".to_string()"),
    ("crates/ui-egui/src/windows.rs", ".with_title(\"Strix\")", ".with_title(\"Strix Writer\")"),
    ("packaging/linux/io.github.nabre.strix.desktop", "\nName=Strix\n", "\nName=Strix Writer\n"),
    ("packaging/linux/io.github.nabre.strix.metainfo.xml.in", "<name>Strix</name>", "<name>Strix Writer</name>"),
    ("packaging/linux/io.github.nabre.strix.metainfo.xml.in", "      Strix is a fast", "      Strix Writer is a fast"),
    ("packaging/macos/Info.plist.in", "<key>CFBundleDisplayName</key>\n  <string>Strix<", "<key>CFBundleDisplayName</key>\n  <string>Strix Writer<"),
    ("packaging/macos/Info.plist.in", "<key>CFBundleName</key>\n  <string>Strix<", "<key>CFBundleName</key>\n  <string>Strix Writer<"),
    ("packaging/windows/strix.wxs", "<Package Name=\"Strix\"", "<Package Name=\"Strix Writer\""),
    ("packaging/windows/strix.wxs", "Description=\"Strix word processor\"", "Description=\"Strix Writer word processor\""),
    ("packaging/windows/strix.wxs", "A newer version of Strix is", "A newer version of Strix Writer is"),
    ("packaging/windows/strix.wxs", "Value=\"Launch Strix\"", "Value=\"Launch Strix Writer\""),
    ("packaging/windows/strix.wxs", "Feature Id=\"Main\" Title=\"Strix\"", "Feature Id=\"Main\" Title=\"Strix Writer\""),
    ("packaging/windows/strix.wxs", "Name=\"Strix\" Description=", "Name=\"Strix Writer\" Description="),
    ("packaging/windows/strix.wxs", "Name=\"ApplicationName\" Value=\"Strix\"", "Name=\"ApplicationName\" Value=\"Strix Writer\""),
    ("packaging/windows/strix.wxs", "Name=\"FriendlyAppName\" Value=\"Strix\"", "Name=\"FriendlyAppName\" Value=\"Strix Writer\""),
];

/// `text` (upstream file `path`, already through [`RENAMES`]) with Strix's full product name.
fn product_name(path: &str, text: String) -> String {
    PRODUCT_NAME.iter().filter(|(p, ..)| *p == path).fold(text, |t, (_, from, to)| t.replace(from, to))
}

pub fn rebrand_text(text: &str) -> String {
    text.split_inclusive('\n')
        .map(|line| {
            if NOTICE_MARKERS.iter().any(|m| line.contains(m)) {
                line.to_string()
            } else {
                RENAMES.iter().fold(line.to_string(), |t, (from, to)| t.replace(from, to))
            }
        })
        .collect()
}

/// `text` (the contents of upstream file `path`) rebranded, with the §4(b) modification notice
/// added when the rename changed something and the format has comments. `None`: unchanged.
pub fn rebrand_file(path: &str, text: &str) -> Option<String> {
    if VERBATIM_FILES.contains(&path) {
        return None;
    }
    let new_path = rebrand_path(path);
    let new = product_name(new_path.as_deref().unwrap_or(path), rebrand_text(text));
    let renamed = new_path.is_some_and(|p| p != path);
    if new == text && !renamed {
        return None;
    }
    Some(with_mark(path, new))
}

/// `text` with [`MARK`] as a comment near the top, after a shebang, XML declaration or doctype, in the
/// comment syntax of the file's format; unchanged when the format has none or the mark is there.
fn with_mark(path: &str, text: String) -> String {
    let name = path.rsplit('/').next().unwrap_or(path);
    let ext = name.trim_end_matches(".in").rsplit_once('.').map_or("", |(_, e)| e);
    let comment = match ext {
        "rs" | "js" | "ts" => format!("// {MARK}"),
        "toml" | "sh" | "py" | "yml" | "yaml" | "ps1" | "properties" | "desktop" => format!("# {MARK}"),
        "md" | "html" | "svg" | "xml" | "wxs" | "plist" => format!("<!-- {MARK} -->"),
        _ => return text,
    };
    if text.lines().take(15).any(|l| l.contains(MARK)) {
        return text;
    }
    let eol = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let skip_first = ["#!", "<?xml", "<!DOCTYPE", "<!doctype"].iter().any(|p| text.starts_with(p));
    match text.split_once('\n') {
        Some((first, rest)) if skip_first => format!("{first}\n{comment}{eol}{rest}"),
        _ => format!("{comment}{eol}{text}"),
    }
}

/// The rebranded path, or `None` when the path is excluded from the vendor branch.
pub fn rebrand_path(path: &str) -> Option<String> {
    if EXCLUDED_PREFIXES.iter().any(|p| path.starts_with(p)) {
        return None;
    }
    Some(rebrand_text(path))
}

pub fn run(root: &Path, args: &[&str]) -> Result<(), String> {
    match args {
        ["status"] | [] => status(root),
        ["sync"] => sync(root, &format!("{REMOTE}/{UPSTREAM_BRANCH}")),
        ["sync", rev] => sync(root, rev),
        ["rebrand", dir] => rebrand_tree(Path::new(dir)).map(|n| println!("rebranded {n} file(s)")),
        _ => Err("usage: cargo xtask upstream [status | sync [REV] | rebrand DIR]".into()),
    }
}

fn status(root: &Path) -> Result<(), String> {
    ensure_remote(root)?;
    fetch(root);
    let Some(synced) = last_synced(root)? else {
        println!("No `{VENDOR_BRANCH}` branch yet: run `cargo xtask upstream sync <fork commit>` (docs/upstream.md).");
        return Ok(());
    };
    let head = git(root, &["rev-parse", &format!("{REMOTE}/{UPSTREAM_BRANCH}")])?;
    let behind = git(root, &["rev-list", "--count", &format!("{synced}..{head}")])?;
    let merged = git_ok(root, &["merge-base", "--is-ancestor", VENDOR_BRANCH, "HEAD"]);
    println!("upstream     {UPSTREAM_URL} ({REMOTE}/{UPSTREAM_BRANCH} = {})", short(&head));
    println!("vendor       {VENDOR_BRANCH} = upstream {}", short(&synced));
    println!("behind       {behind} upstream commit(s)");
    println!("merged       {}", if merged { "yes, HEAD contains the vendor branch" } else { "no: run `git merge vendor/wordcraft`" });
    let surface = git(root, &["diff", "--shortstat", VENDOR_BRANCH, "HEAD"])?;
    println!("fork surface {} (vendor branch vs HEAD)", surface.trim());
    Ok(())
}

/// Commits the rebranded tree of upstream `rev` on top of the vendor branch.
fn sync(root: &Path, rev: &str) -> Result<(), String> {
    ensure_remote(root)?;
    fetch(root);
    let sha = git(root, &["rev-parse", "--verify", &format!("{rev}^{{commit}}")])?;
    if last_synced(root)?.as_deref() == Some(sha.as_str()) {
        println!("{VENDOR_BRANCH} already holds upstream {}", short(&sha));
        return Ok(());
    }
    let wt = root.join("target").join("upstream-sync");
    if wt.exists() {
        let _ = git(root, &["worktree", "remove", "--force", &path_arg(&wt)?]);
    }
    let exists = git_ok(root, &["rev-parse", "--verify", "--quiet", &format!("refs/heads/{VENDOR_BRANCH}")]);
    if exists {
        git(root, &["worktree", "add", "--quiet", &path_arg(&wt)?, VENDOR_BRANCH])?;
    } else {
        git(root, &["worktree", "add", "--quiet", "--detach", &path_arg(&wt)?])?;
        git(&wt, &["checkout", "--quiet", "--orphan", VENDOR_BRANCH])?;
    }
    let result = commit_snapshot(&wt, &sha);
    let _ = git(root, &["worktree", "remove", "--force", &path_arg(&wt)?]);
    result?;
    println!("{VENDOR_BRANCH} now holds upstream {} rebranded.", short(&sha));
    println!("Next: `git merge {VENDOR_BRANCH}` on a branch, resolve, `cargo xtask ci` (docs/upstream.md).");
    Ok(())
}

fn commit_snapshot(wt: &Path, sha: &str) -> Result<(), String> {
    git(wt, &["read-tree", "-u", "--reset", sha])?;
    let n = rebrand_tree(wt)?;
    git(wt, &["add", "--all"])?;
    // Shorter names let rustfmt join lines upstream had to wrap; formatting the renamed files as
    // `main` is formatted keeps those lines out of the merge.
    let renamed = git(wt, &["diff", "--cached", "--name-only", "--diff-filter=AMR", sha, "--", "*.rs"])?;
    format_rust(wt, &renamed.lines().collect::<Vec<_>>());
    git(wt, &["add", "--all"])?;
    let subject = git(wt, &["log", "-1", "--format=%s", sha])?;
    let msg = format!("Upstream WordCraft {} rebranded\n\n{subject}\n\n{TRAILER} {sha}\n", short(sha));
    git(wt, &["commit", "--quiet", "--allow-empty", "--no-verify", "-m", &msg])?;
    eprintln!("rebranded {n} file(s)");
    Ok(())
}

/// Runs rustfmt (the workspace's `rustfmt.toml`) on `files`, relative to `dir`. Without rustfmt
/// it warns and leaves them as they are: the merge then shows the formatting too.
fn format_rust(dir: &Path, files: &[&str]) {
    let config = dir.join("rustfmt.toml");
    for chunk in files.chunks(100) {
        let mut cmd = Command::new("rustfmt");
        cmd.current_dir(dir).args(["--edition", "2024"]).arg("--config-path").arg(&config).args(chunk);
        match cmd.output() {
            Ok(o) if o.status.success() => {}
            Ok(o) => eprintln!("warning: rustfmt: {}", String::from_utf8_lossy(&o.stderr).trim()),
            Err(e) => {
                eprintln!("warning: rustfmt not run ({e}); renamed files keep upstream's line breaks");
                return;
            }
        }
    }
}

/// Rewrites every file under `dir` (except `.git`) in place; returns how many changed.
fn rebrand_tree(dir: &Path) -> Result<usize, String> {
    let mut files = Vec::new();
    walk(dir, dir, &mut files)?;
    let mut changed = 0;
    for rel in files {
        let from = dir.join(&rel);
        let Some(new_rel) = rebrand_path(&rel) else {
            std::fs::remove_file(&from).map_err(|e| format!("{rel}: {e}"))?;
            changed += 1;
            continue;
        };
        let bytes = std::fs::read(&from).map_err(|e| format!("{rel}: {e}"))?;
        let text = std::str::from_utf8(&bytes).ok().filter(|t| !t.contains('\0'));
        let new_text = text.and_then(|t| rebrand_file(&rel, t));
        let to = dir.join(&new_rel);
        if new_rel != rel {
            if let Some(parent) = to.parent() {
                std::fs::create_dir_all(parent).map_err(|e| format!("{new_rel}: {e}"))?;
            }
            std::fs::rename(&from, &to).map_err(|e| format!("{rel} -> {new_rel}: {e}"))?;
            changed += 1;
        }
        if let Some(new) = new_text {
            std::fs::write(&to, new).map_err(|e| format!("{new_rel}: {e}"))?;
            changed += usize::from(new_rel == rel);
        }
    }
    Ok(changed)
}

/// Relative `/`-separated paths of the files under `dir`, skipping `.git`.
fn walk(base: &Path, dir: &Path, out: &mut Vec<String>) -> Result<(), String> {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if entry.file_name() == ".git" {
            continue;
        }
        let ty = entry.file_type().map_err(|e| e.to_string())?;
        if ty.is_dir() {
            walk(base, &path, out)?;
        } else if let Ok(rel) = path.strip_prefix(base) {
            out.push(rel.to_string_lossy().replace('\\', "/"));
        }
    }
    Ok(())
}

/// The upstream commit recorded on the vendor branch tip, if the branch exists.
fn last_synced(root: &Path) -> Result<Option<String>, String> {
    if !git_ok(root, &["rev-parse", "--verify", "--quiet", &format!("refs/heads/{VENDOR_BRANCH}")]) {
        return Ok(None);
    }
    let body = git(root, &["log", "-1", "--format=%B", VENDOR_BRANCH])?;
    Ok(body.lines().find_map(|l| l.strip_prefix(TRAILER)).map(|s| s.trim().to_string()))
}

fn ensure_remote(root: &Path) -> Result<(), String> {
    if git_ok(root, &["remote", "get-url", REMOTE]) {
        return Ok(());
    }
    git(root, &["remote", "add", REMOTE, UPSTREAM_URL])?;
    // Nothing is ever pushed to upstream from here.
    git(root, &["remote", "set-url", "--push", REMOTE, "DISABLED"])?;
    Ok(())
}

/// Fetches upstream; offline, warns and works from the refs already fetched.
fn fetch(root: &Path) {
    if let Err(e) = git(root, &["fetch", "--quiet", "--no-tags", REMOTE, UPSTREAM_BRANCH]) {
        eprintln!(
            "warning: {e}
warning: using the upstream refs fetched earlier"
        );
    }
}

fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    let out = Command::new("git").current_dir(dir).args(args).output().map_err(|e| format!("git: {e}"))?;
    if !out.status.success() {
        return Err(format!("git {}: {}", args.join(" "), String::from_utf8_lossy(&out.stderr).trim()));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn git_ok(dir: &Path, args: &[&str]) -> bool {
    Command::new("git").current_dir(dir).args(args).output().is_ok_and(|o| o.status.success())
}

fn path_arg(p: &Path) -> Result<String, String> {
    p.to_str().map(str::to_string).ok_or_else(|| format!("non-UTF-8 path {}", p.display()))
}

fn short(sha: &str) -> &str {
    sha.get(..9).unwrap_or(sha)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifiers_urls_and_app_ids_are_rebranded() {
        assert_eq!(rebrand_text("use wordcraft_doc::Doc; // WordCraft"), "use strix_doc::Doc; // Strix");
        assert_eq!(rebrand_text("WORDCRAFT_DICTIONARIES"), "STRIX_DICTIONARIES");
        assert_eq!(rebrand_text("https://github.com/storytold/wordcraft/issues"), "https://github.com/nabre/strix/issues");
        assert_eq!(rebrand_text("ai.storyteller.wordcraft.desktop"), "io.github.nabre.strix.desktop");
        assert_eq!(rebrand_text("storytold/craft-libs"), "storytold/craft-libs");
    }

    #[test]
    fn notices_are_kept_verbatim() {
        let notice = "WordCraft\nCopyright (c) 2026 ArtCraft Team and the WordCraft contributors.\n";
        assert_eq!(rebrand_text(notice), "Strix\nCopyright (c) 2026 ArtCraft Team and the WordCraft contributors.\n");
        assert_eq!(rebrand_file("NOTICE", notice), None);
        assert_eq!(rebrand_file("LICENSE-MIT", notice), None);
    }

    #[test]
    fn changed_files_get_the_modification_notice() {
        assert_eq!(rebrand_file("a.rs", "use wordcraft_doc;\n").as_deref(), Some(format!("// {MARK}\nuse strix_doc;\n").as_str()));
        assert_eq!(rebrand_file("a.rs", "fn f() {}\n"), None);
        assert_eq!(rebrand_file("x.sh", "#!/bin/sh\nwordcraft\n").as_deref(), Some(format!("#!/bin/sh\n# {MARK}\nstrix\n").as_str()));
        let xml = "<?xml version=\"1.0\"?>\r\n<a>WordCraft</a>\r\n";
        assert_eq!(
            rebrand_file("m.xml.in", xml).as_deref(),
            Some(format!("<?xml version=\"1.0\"?>\r\n<!-- {MARK} -->\r\n<a>Strix</a>\r\n").as_str())
        );
        assert_eq!(rebrand_file("d.json", "\"wordcraft\"").as_deref(), Some("\"strix\""));
    }

    #[test]
    fn titles_and_installers_get_the_full_product_name() {
        let plist = "<key>CFBundleName</key>\n  <string>WordCraft</string>\n<key>CFBundleExecutable</key>\n  <string>WordCraft</string>\n";
        let out = rebrand_file("packaging/macos/Info.plist.in", plist).unwrap_or_default();
        assert!(out.contains("<key>CFBundleName</key>\n  <string>Strix Writer</string>"), "{out}");
        assert!(out.contains("<key>CFBundleExecutable</key>\n  <string>Strix</string>"), "{out}");
        // Running text keeps the short name, and other files are untouched by the table.
        let rs = "let t = \"About WordCraft\"; // WordCraft can't\n";
        assert!(rebrand_file("crates/ui-egui/src/dialogs/chrome.rs", rs).unwrap_or_default().contains("\"About Strix Writer\"; // Strix can't"));
        assert!(rebrand_file("crates/other.rs", rs).unwrap_or_default().contains("\"About Strix\";"));
    }

    /// `main` already shows what the next sync writes: every row's result is in its file.
    #[test]
    fn main_has_the_full_product_name_where_the_table_puts_it() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        for (path, from, to) in PRODUCT_NAME {
            let text = std::fs::read_to_string(root.join(path)).unwrap_or_else(|e| panic!("{path}: {e}"));
            assert!(text.contains(to), "{path} lacks {to:?}");
            assert!(!text.contains(from) || to.contains(from), "{path} still has {from:?}");
        }
    }

    #[test]
    fn paths_are_renamed_and_brand_files_dropped() {
        assert_eq!(rebrand_path("apps/wordcraft-cli/src/main.rs").as_deref(), Some("apps/strix-cli/src/main.rs"));
        assert_eq!(
            rebrand_path("packaging/linux/ai.storyteller.wordcraft.desktop").as_deref(),
            Some("packaging/linux/io.github.nabre.strix.desktop")
        );
        assert_eq!(rebrand_path("docs/brand/artcraft-logo.svg"), None);
    }
}

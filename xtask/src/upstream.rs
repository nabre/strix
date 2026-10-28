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
    let new = rebrand_text(text);
    let renamed = rebrand_path(path).is_some_and(|p| p != path);
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
    let subject = git(wt, &["log", "-1", "--format=%s", sha])?;
    let msg = format!("Upstream WordCraft {} rebranded\n\n{subject}\n\n{TRAILER} {sha}\n", short(sha));
    git(wt, &["commit", "--quiet", "--allow-empty", "--no-verify", "-m", &msg])?;
    eprintln!("rebranded {n} file(s)");
    Ok(())
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
    fn paths_are_renamed_and_brand_files_dropped() {
        assert_eq!(rebrand_path("apps/wordcraft-cli/src/main.rs").as_deref(), Some("apps/strix-cli/src/main.rs"));
        assert_eq!(
            rebrand_path("packaging/linux/ai.storyteller.wordcraft.desktop").as_deref(),
            Some("packaging/linux/io.github.nabre.strix.desktop")
        );
        assert_eq!(rebrand_path("docs/brand/artcraft-logo.svg"), None);
    }
}

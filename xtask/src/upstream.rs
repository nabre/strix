//! `cargo xtask upstream <status|sync [REV]|rebrand DIR>`: keep Strix mergeable with WordCraft.
//!
//! Strix is a fork of WordCraft that keeps WordCraft's internal names (crates, Rust paths, types,
//! environment variables) and is Strix only where users see it or where it would collide with a
//! WordCraft installed on the same computer (`docs/naming.md`). Upstream is tracked through a
//! *vendor branch* (`vendor/wordcraft`): each commit on it is an upstream snapshot passed through
//! [`rebrand_file`] and [`rebrand_path`], a short list of exact, file-scoped rules. `main` merges
//! that branch, so git's three-way merge only sees real changes on both sides. The workflow is in
//! `docs/upstream.md`.

use std::path::Path;
use std::process::Command;

/// Upstream repository and the local names that track it.
pub const UPSTREAM_URL: &str = "https://github.com/storytold/wordcraft.git";
const REMOTE: &str = "upstream";
const UPSTREAM_BRANCH: &str = "main";
pub const VENDOR_BRANCH: &str = "vendor/wordcraft";
/// Trailer naming the upstream commit a vendor commit was made from.
const TRAILER: &str = "Upstream-Commit:";

/// Names that are Strix's in every upstream file: the app id, the repository, the app packages'
/// and executables' names and the paths that hold them. Each is a whole name: it matches only
/// where the character before it can't be part of an identifier, so `CARGO_BIN_EXE_wordcraft-cli`
/// is the third rule's, not the seventh's. Applied to file contents and paths, longest first.
const EXACT: &[(&str, &str)] = &[
    ("ai.storyteller.wordcraft", "io.github.nabre.strix"),
    ("github.com/storytold/wordcraft", "github.com/nabre/strix"),
    ("CARGO_BIN_EXE_wordcraft", "CARGO_BIN_EXE_strix"),
    ("storytold/wordcraft", "nabre/strix"),
    ("app-icon/wordcraft", "app-icon/strix"),
    ("apps/wordcraft", "apps/strix"),
    ("wordcraft-cli", "strix-cli"),
    ("wordcraft-web", "strix-web"),
    ("wordcraft.exe", "strix.exe"),
];

/// Upstream's product name in running interface text. In upstream `.rs` files, the whole word
/// inside a string literal (not in identifiers or comments) becomes Strix's: messages, labels,
/// the folder names that are written with a capital (`Application Support/WordCraft`).
const PRODUCT_WORD: (&str, &str) = ("WordCraft", "Strix");

/// Upstream paths that never enter the vendor branch: the ArtCraft brand files, whose licence
/// (docs/brand/LICENSE-brand.txt upstream) requires forks to remove them.
const EXCLUDED_PREFIXES: &[&str] = &["docs/brand/"];

/// Paths that are Strix's own and maintained by hand: the sync leaves them as the vendor branch
/// already has them, so upstream's changes there never reach the merge (each sync lists the
/// upstream commits that touched them, to review). `dir/` is a folder, one `*` matches within a
/// file name, anything else is a file.
const STRIX_OWNED: &[&str] = &[
    "packaging/",
    "site/",
    ".github/workflows/",
    "README.md",
    "docs/README.*.md",
    "ROADMAP.md",
    "docs/roadmap.md",
    "docs/gaps.md",
    "docs/*-parity.md",
];

/// Files kept byte-for-byte: Apache-2.0 §4(a) and §4(c) forbid rewording licence texts and
/// upstream notices.
const VERBATIM_FILES: &[&str] = &["LICENSE-APACHE", "LICENSE-MIT", "NOTICE"];

/// Lines holding a copyright or attribution notice are never rebranded (§4(c)), e.g.
/// "Copyright (c) 2026 ArtCraft Team and the WordCraft contributors" or an ATTRIBUTION.md row
/// crediting the "WordCraft contributors".
const NOTICE_MARKERS: &[&str] = &["Copyright", "copyright", "©", "WordCraft contributors", "WordCraft authors"];

/// The modification notice every changed upstream file carries (§4(b)); same text as `main`.
const MARK: &str = "Modified by the Strix contributors (fork of WordCraft); see NOTICE.";

/// Where upstream shows its product name as such (window and dialog titles, installers, the
/// generator recorded in exported files), Strix shows its full name, "Strix Writer"; in running
/// text the short "Strix" stays. `(path, upstream text, Strix's text)`, applied to the file named
/// (its Strix path) before the line rules ([`EXACT`], [`PRODUCT_WORD`]). A pattern may span lines.
const PRODUCT_NAME: &[(&str, &str, &str)] = &[
    ("apps/strix/build.rs", "\"ProductName\", \"WordCraft\"", "\"ProductName\", \"Strix Writer\""),
    ("apps/strix/build.rs", "\"WordCraft word processor\"", "\"Strix Writer word processor\""),
    ("apps/strix/src/main.rs", "wordcraft — the WordCraft desktop app", "strix — the Strix Writer desktop app"),
    ("apps/strix/src/main.rs", "log::info!(\"WordCraft {} (", "log::info!(\"Strix Writer {} ("),
    ("apps/strix/src/main.rs", ".with_title(\"WordCraft\")", ".with_title(\"Strix Writer\")"),
    ("apps/strix-web/index.html", "<title>WordCraft</title>", "<title>Strix Writer</title>"),
    ("crates/docx/src/write/settings.rs", "\"Application\", &[], \"WordCraft\"", "\"Application\", &[], \"Strix Writer\""),
    ("crates/engine/src/sample.rs", "creator = \"WordCraft\"", "creator = \"Strix Writer\""),
    ("crates/formats/src/html/export.rs", "content=\\\"WordCraft\\\"", "content=\\\"Strix Writer\\\""),
    (
        "crates/formats/src/latex/export.rs",
        "% Written by WordCraft (https://getartcraft.com/apps/wordcraft).",
        "% Written by Strix Writer (https://github.com/nabre/strix).",
    ),
    ("crates/formats/src/odt/package.rs", "<meta:generator>WordCraft<", "<meta:generator>Strix Writer<"),
    ("crates/formats/src/rtf/export.rs", "\\\\generator WordCraft;", "\\\\generator Strix Writer;"),
    (
        "crates/mcp/src/server.rs",
        "\"serverInfo\": {\"name\": \"wordcraft\", \"title\": \"WordCraft\", \"version\": env!(\"CARGO_PKG_VERSION\"), \"websiteUrl\": \"https://getartcraft.com/apps/wordcraft\"}",
        "\"serverInfo\": {\"name\": \"strix\", \"title\": \"Strix Writer\", \"version\": env!(\"CARGO_PKG_VERSION\"), \"websiteUrl\": \"https://github.com/nabre/strix\"}",
    ),
    ("crates/mcp/src/server.rs", "\"WordCraft is a word processor", "\"Strix Writer is a word processor"),
    (
        "crates/pdf/src/metadata.rs",
        "creator: \"WordCraft\".into(), producer: \"WordCraft\"",
        "creator: \"Strix Writer\".into(), producer: \"Strix Writer\"",
    ),
    ("crates/ui-egui/src/chrome.rs", "    \"WordCraft\".to_string()", "    \"Strix Writer\".to_string()"),
    ("crates/ui-egui/src/dialogs/chrome.rs", "\"About WordCraft\"", "\"About Strix Writer\""),
    ("crates/ui-egui/src/dialogs/chrome.rs", "SaveChanges { .. } => \"WordCraft\"", "SaveChanges { .. } => \"Strix Writer\""),
    ("crates/ui-egui/src/updates_ui.rs", "tl!(\"WordCraft {version} is", "tl!(\"Strix Writer {version} is"),
    ("crates/ui-egui/src/windows.rs", " - WordCraft\"", " - Strix Writer\""),
    ("crates/ui-egui/src/windows.rs", "return \"WordCraft\".to_string()", "return \"Strix Writer\".to_string()"),
    ("crates/ui-egui/src/windows.rs", ".with_title(\"WordCraft\")", ".with_title(\"Strix Writer\")"),
];

/// The other names that stay Strix's in one upstream file (`docs/naming.md`, group b):
/// `(path, upstream text, Strix's text)`, applied with [`PRODUCT_NAME`].
const STRIX_NAMES: &[(&str, &str, &str)] = &[
    // App package and executable names (b4).
    ("apps/strix/Cargo.toml", "name = \"wordcraft\"", "name = \"strix\""),
    ("apps/strix/Cargo.toml", "default-run = \"wordcraft\"", "default-run = \"strix\""),
    ("apps/strix/build.rs", ".set(\"InternalName\", \"wordcraft\")", ".set(\"InternalName\", \"strix\")"),
    ("apps/strix/build.rs", ".set(\"CompanyName\", \"Learning Machines LLC\")", ".set(\"CompanyName\", \"The Strix contributors\")"),
    ("xtask/src/layers.rs", "(\"wordcraft\", Class::Exempt)", "(\"strix\", Class::Exempt)"),
    ("xtask/src/layers.rs", "for app in [\"wordcraft\", ", "for app in [\"strix\", "),
    // The desktop executable names itself in its usage, version and error lines (and their tests).
    ("apps/strix/src/main.rs", "//! Usage: `wordcraft [", "//! Usage: `strix ["),
    ("apps/strix/src/main.rs", "\n  wordcraft [--sample]", "\n  strix [--sample]"),
    ("apps/strix/src/main.rs", "println!(\"wordcraft {} (", "println!(\"strix {} ("),
    ("apps/strix/src/main.rs", "format_args!(\"wordcraft: ", "format_args!(\"strix: "),
    ("apps/strix/src/logging.rs", "format_args!(\"wordcraft: {report}", "format_args!(\"strix: {report}"),
    ("apps/strix/src/default_apps.rs", "l == \"Exec=wordcraft %F\"", "l == \"Exec=strix %F\""),
    ("apps/strix/tests/it/help.rs", "starts_with(\"wordcraft — \")", "starts_with(\"strix — \")"),
    ("apps/strix/tests/it/control_arg.rs", "format!(\"wordcraft: --control", "format!(\"strix: --control"),
    ("apps/strix/tests/it/control_arg.rs", "starts_with(\"wordcraft \")", "starts_with(\"strix \")"),
    ("apps/strix/tests/it/non_utf8_args.rs", "\"wordcraft: argument 3", "\"strix: argument 3"),
    ("crates/mcp/src/lib.rs", "start `wordcraft --control 7981`", "start `strix --control 7981`"),
    // The user's folders and stores (b3); the capitalised ones are string literals ([`PRODUCT_WORD`]).
    ("crates/control-key/src/lib.rs", ".map(|c| c.join(\"wordcraft\"))", ".map(|c| c.join(\"strix\"))"),
    ("crates/control-key/src/lib.rs", "Path::new(\"/settings/wordcraft\")", "Path::new(\"/settings/strix\")"),
    ("crates/control-key/src/lib.rs", "name == \"wordcraft\" || ", "name == \"strix\" || "),
    (
        "crates/control-key/src/lib.rs",
        "/// `~/Library/Application Support/WordCraft`, Windows `%APPDATA%\\WordCraft`, otherwise\n/// `$XDG_CONFIG_HOME/wordcraft` or `~/.config/wordcraft`.",
        "/// `~/Library/Application Support/Strix`, Windows `%APPDATA%\\Strix`, otherwise\n/// `$XDG_CONFIG_HOME/strix` or `~/.config/strix`.",
    ),
    ("apps/strix-web/src/prefs.rs", "const KEY: &str = \"wordcraft.ui\";", "const KEY: &str = \"strix.ui\";"),
    ("apps/strix-web/src/web/font_store.rs", "const DB_NAME: &str = \"wordcraft\";", "const DB_NAME: &str = \"strix\";"),
    // The app's own crate is `strix`: its info logs stay on by default (U3).
    ("apps/strix/src/logging.rs", "DEFAULT_FILTER: &str = \"warn,wordcraft*=info\";", "DEFAULT_FILTER: &str = \"warn,wordcraft*=info,strix*=info\";"),
    // Links to WordCraft's site point to Strix's repository (U8).
    (
        "crates/mcp/src/server.rs",
        "Community: https://discord.gg/artcraft · https://getartcraft.com/apps/wordcraft · https://github.com/storytold/wordcraft\"",
        "Community: https://github.com/nabre/strix/discussions · https://github.com/nabre/strix\"",
    ),
    (
        "crates/ui-egui/src/dialogs/dispatch.rs",
        "ui.hyperlink_to(tl!(\"getartcraft.com/apps/wordcraft\"), \"https://getartcraft.com/apps/wordcraft\");",
        "ui.hyperlink_to(tl!(\"Source code: github.com/nabre/strix\"), \"https://github.com/nabre/strix\");",
    ),
];

/// Every file rule: Strix path, upstream text, Strix's text.
fn file_rules() -> impl Iterator<Item = &'static (&'static str, &'static str, &'static str)> {
    PRODUCT_NAME.iter().chain(STRIX_NAMES)
}

/// `text` (upstream file `path`, before the line rules) with the rules for that file.
fn apply_file_rules(path: &str, text: String) -> String {
    file_rules().filter(|(p, ..)| *p == path).fold(text, |t, (_, from, to)| t.replace(from, to))
}

/// Whether `c` can be part of an identifier, so a name right after it is part of a longer one.
fn word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// `line` with every [`EXACT`] name replaced where it starts a name.
fn exact_names(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut rest = line;
    let mut prev: Option<char> = None;
    'scan: while let Some(c) = rest.chars().next() {
        if !prev.is_some_and(word_char) {
            for (from, to) in EXACT {
                if let Some(after) = rest.strip_prefix(from) {
                    out.push_str(to);
                    prev = from.chars().last();
                    rest = after;
                    continue 'scan;
                }
            }
        }
        out.push(c);
        prev = Some(c);
        rest = &rest[c.len_utf8()..];
    }
    out
}

/// `line` (Rust) with [`PRODUCT_WORD`] replaced as a whole word inside string literals only; a
/// `//` comment ends the scan. A literal that spans lines is seen only on its first line.
fn product_in_strings(line: &str) -> String {
    let (from, to) = PRODUCT_WORD;
    let mut out = String::with_capacity(line.len());
    let mut in_str = false;
    let mut prev: Option<char> = None;
    let mut rest = line;
    while let Some(c) = rest.chars().next() {
        if !in_str {
            if rest.starts_with("//") {
                out.push_str(rest);
                return out;
            }
            // Character literals holding a quote.
            for lit in ["'\"'", "'\\\"'"] {
                if let Some(after) = rest.strip_prefix(lit) {
                    out.push_str(lit);
                    rest = after;
                }
            }
            let Some(c) = rest.chars().next() else { break };
            if c == '"' {
                in_str = true;
            }
            out.push(c);
            prev = Some(c);
            rest = &rest[c.len_utf8()..];
            continue;
        }
        if c == '\\' {
            let escaped: String = rest.chars().take(2).collect();
            out.push_str(&escaped);
            rest = &rest[escaped.len()..];
            prev = escaped.chars().last();
            continue;
        }
        if c == '"' {
            in_str = false;
        } else if let Some(after) = rest.strip_prefix(from)
            && !prev.is_some_and(word_char)
            && !after.chars().next().is_some_and(word_char)
        {
            out.push_str(to);
            rest = after;
            prev = to.chars().last();
            continue;
        }
        out.push(c);
        prev = Some(c);
        rest = &rest[c.len_utf8()..];
    }
    out
}

/// `text` (the contents of upstream file `path`) through the line rules: [`EXACT`] names, and in
/// Rust [`PRODUCT_WORD`] inside string literals. Notice lines are kept as they are.
pub fn rebrand_text(path: &str, text: &str) -> String {
    let rust = path.ends_with(".rs");
    text.split_inclusive('\n')
        .map(|line| {
            if NOTICE_MARKERS.iter().any(|m| line.contains(m)) {
                line.to_string()
            } else if rust {
                product_in_strings(&exact_names(line))
            } else {
                exact_names(line)
            }
        })
        .collect()
}

/// `text` (the contents of upstream file `path`) rebranded, with the §4(b) modification notice
/// added when the rules changed its contents and the format has comments. `None`: unchanged
/// (a file that only moves, `apps/wordcraft*` → `apps/strix*`, keeps upstream's contents).
pub fn rebrand_file(path: &str, text: &str) -> Option<String> {
    if VERBATIM_FILES.contains(&path) {
        return None;
    }
    let new_path = rebrand_path(path).unwrap_or_else(|| path.to_string());
    let new = rebrand_text(path, &apply_file_rules(&new_path, text.to_string()));
    if new == text {
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

/// The rebranded path, or `None` when the path is excluded from the vendor branch or is Strix's
/// own ([`STRIX_OWNED`], which the sync then takes from the vendor branch as it was).
pub fn rebrand_path(path: &str) -> Option<String> {
    if EXCLUDED_PREFIXES.iter().any(|p| path.starts_with(p)) || strix_owned(path) {
        return None;
    }
    Some(exact_names(path))
}

/// Whether `path` is one of Strix's own paths ([`STRIX_OWNED`]).
fn strix_owned(path: &str) -> bool {
    STRIX_OWNED.iter().any(|p| match p.split_once('*') {
        _ if p.ends_with('/') => path.starts_with(p),
        Some((head, tail)) => path.strip_prefix(head).and_then(|r| r.strip_suffix(tail)).is_some_and(|mid| !mid.contains('/')),
        None => path == *p,
    })
}

/// [`STRIX_OWNED`] as git pathspecs.
fn strix_owned_pathspecs() -> Vec<String> {
    STRIX_OWNED.iter().map(|p| if p.ends_with('/') { format!(":(glob){p}**") } else { format!(":(glob){p}") }).collect()
}

pub fn run(root: &Path, args: &[&str]) -> Result<(), String> {
    match args {
        ["status"] | [] => status(root),
        ["sync"] => sync(root, &format!("{REMOTE}/{UPSTREAM_BRANCH}")),
        ["sync", rev] => sync(root, rev),
        ["rebrand", dir] => rebrand_tree(Path::new(dir)).map(|n| {
            warn_unapplied_rules(Path::new(dir));
            println!("rebranded {n} file(s)");
        }),
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
    let previous = last_synced(root)?;
    if previous.as_deref() == Some(sha.as_str()) {
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
    if let Some(prev) = previous {
        report_owned_changes(root, &prev, &sha)?;
    }
    println!("Next: `git merge {VENDOR_BRANCH}` on a branch, resolve, `cargo xtask ci` (docs/upstream.md).");
    Ok(())
}

fn commit_snapshot(wt: &Path, sha: &str) -> Result<(), String> {
    git(wt, &["read-tree", "-u", "--reset", sha])?;
    let n = rebrand_tree(wt)?;
    restore_owned(wt)?;
    warn_unapplied_rules(wt);
    git(wt, &["add", "--all"])?;
    // A rule can shorten a line enough for rustfmt to join what upstream wrapped; formatting the
    // changed files as `main` is formatted keeps those lines out of the merge.
    let renamed = git(wt, &["diff", "--cached", "--name-only", "--diff-filter=AMR", sha, "--", "*.rs"])?;
    format_rust(wt, &renamed.lines().collect::<Vec<_>>());
    git(wt, &["add", "--all"])?;
    let subject = git(wt, &["log", "-1", "--format=%s", sha])?;
    let msg = format!("Upstream WordCraft {} rebranded\n\n{subject}\n\n{TRAILER} {sha}\n", short(sha));
    git(wt, &["commit", "--quiet", "--allow-empty", "--no-verify", "-m", &msg])?;
    eprintln!("rebranded {n} file(s)");
    Ok(())
}

/// Puts back Strix's own paths ([`STRIX_OWNED`]) as the vendor branch had them before this
/// snapshot; on the branch's first commit there is nothing to put back.
fn restore_owned(wt: &Path) -> Result<(), String> {
    if !git_ok(wt, &["rev-parse", "--verify", "--quiet", "HEAD"]) {
        return Ok(());
    }
    let tree = git(wt, &["ls-tree", "-r", "--name-only", "HEAD"])?;
    let owned: Vec<&str> = tree.lines().filter(|p| strix_owned(p)).collect();
    for chunk in owned.chunks(100) {
        let mut args = vec!["checkout", "HEAD", "--"];
        args.extend_from_slice(chunk);
        git(wt, &args)?;
    }
    Ok(())
}

/// Warns about every file rule whose Strix text isn't in its file after the snapshot: upstream
/// changed or moved that line, and the rule needs updating (`docs/naming.md`).
fn warn_unapplied_rules(wt: &Path) {
    for (path, from, to) in file_rules() {
        let text = std::fs::read_to_string(wt.join(path)).unwrap_or_default();
        if !text.contains(to) {
            eprintln!("warning: rule for {path} didn't apply: upstream no longer has {from:?}");
        }
    }
}

/// Lists the upstream commits between `from` and `to` that touched Strix's own paths: the sync
/// left those paths alone, so their changes are for a person to review and port by hand.
fn report_owned_changes(root: &Path, from: &str, to: &str) -> Result<(), String> {
    let range = format!("{from}..{to}");
    let mut args = vec!["log", "--oneline", "--no-merges", &range, "--"];
    let specs = strix_owned_pathspecs();
    args.extend(specs.iter().map(String::as_str));
    let log = git(root, &args)?;
    if !log.is_empty() {
        println!("Upstream changed Strix's own paths (not synced; review and port by hand):\n{log}");
    }
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
    fn internal_names_stay_upstreams() {
        for line in [
            "use wordcraft_doc::Doc;\n",
            "wordcraft-doc = { workspace = true }\n",
            "std::env::var_os(\"WORDCRAFT_DICTIONARIES\")\n",
            "log::debug!(target: \"wordcraft_docx\", \"x\");\n",
            "use craft_update::suite::WORDCRAFT;\n",
            "let catalog = \"wordcraft.tsv\";\n",
            "// WordCraft lays it out\n",
        ] {
            assert_eq!(rebrand_file("crates/x/src/a.rs", line), None, "{line}");
        }
        assert_eq!(rebrand_file("docs/a.md", "WordCraft uses `wordcraft_doc`.\n"), None);
    }

    #[test]
    fn app_ids_urls_and_executables_are_strixs() {
        let t = |s: &str| rebrand_text("a.md", s);
        assert_eq!(t("https://github.com/storytold/wordcraft/issues"), "https://github.com/nabre/strix/issues");
        assert_eq!(t("ai.storyteller.wordcraft.desktop"), "io.github.nabre.strix.desktop");
        assert_eq!(t("storytold/craft-libs"), "storytold/craft-libs");
        assert_eq!(t("run `wordcraft-cli convert` or wordcraft.exe"), "run `strix-cli convert` or strix.exe");
        assert_eq!(t("env!(\"CARGO_BIN_EXE_wordcraft-cli\")"), "env!(\"CARGO_BIN_EXE_strix-cli\")");
        assert_eq!(t("members = [\"apps/wordcraft\", \"apps/wordcraft-web\"]"), "members = [\"apps/strix\", \"apps/strix-web\"]");
        assert_eq!(t("../../assets/app-icon/wordcraft.ico"), "../../assets/app-icon/strix.ico");
        // Only whole names: not the tail of a longer one.
        assert_eq!(t("my_wordcraft-cli xwordcraft.exe"), "my_wordcraft-cli xwordcraft.exe");
    }

    #[test]
    fn the_product_name_in_rust_strings_is_strixs() {
        let t = |s: &str| rebrand_text("crates/ui-egui/src/a.rs", s);
        assert_eq!(
            t("tl!(\"Show the Start screen when WordCraft starts\") // WordCraft\n"),
            "tl!(\"Show the Start screen when Strix starts\") // WordCraft\n"
        );
        assert_eq!(t("let WordCraftApp = \"WordCraft's\";"), "let WordCraftApp = \"Strix's\";");
        assert_eq!(t("\"WordCraftX \\\"WordCraft\\\"\""), "\"WordCraftX \\\"Strix\\\"\"");
        assert_eq!(t("if c == '\"' { \"WordCraft\" }"), "if c == '\"' { \"Strix\" }");
        assert_eq!(t("a.join(\"WordCraft\").join(\"wordcraft\")"), "a.join(\"Strix\").join(\"wordcraft\")");
        // Not outside Rust.
        assert_eq!(rebrand_text("a.toml", "description = \"WordCraft engine\""), "description = \"WordCraft engine\"");
    }

    #[test]
    fn notices_are_kept_verbatim() {
        let notice = "wordcraft-cli\nCopyright (c) 2026 ArtCraft Team and the WordCraft contributors.\n";
        assert_eq!(rebrand_text("a.md", notice), "strix-cli\nCopyright (c) 2026 ArtCraft Team and the WordCraft contributors.\n");
        assert_eq!(rebrand_file("NOTICE", notice), None);
        assert_eq!(rebrand_file("LICENSE-MIT", notice), None);
    }

    #[test]
    fn changed_files_get_the_modification_notice() {
        assert_eq!(rebrand_file("a.rs", "let s = \"WordCraft\";\n").as_deref(), Some(format!("// {MARK}\nlet s = \"Strix\";\n").as_str()));
        assert_eq!(rebrand_file("a.rs", "fn f() {}\n"), None);
        assert_eq!(rebrand_file("x.sh", "#!/bin/sh\nwordcraft-cli\n").as_deref(), Some(format!("#!/bin/sh\n# {MARK}\nstrix-cli\n").as_str()));
        let xml = "<?xml version=\"1.0\"?>\r\n<a>ai.storyteller.wordcraft</a>\r\n";
        assert_eq!(
            rebrand_file("m.xml.in", xml).as_deref(),
            Some(format!("<?xml version=\"1.0\"?>\r\n<!-- {MARK} -->\r\n<a>io.github.nabre.strix</a>\r\n").as_str())
        );
        assert_eq!(rebrand_file("d.json", "\"wordcraft-cli\"").as_deref(), Some("\"strix-cli\""));
        // A file that only moves keeps upstream's contents.
        assert_eq!(rebrand_path("apps/wordcraft/src/print.rs").as_deref(), Some("apps/strix/src/print.rs"));
        assert_eq!(rebrand_file("apps/wordcraft/src/print.rs", "use wordcraft_doc;\n"), None);
    }

    #[test]
    fn file_rules_apply_to_their_file_only() {
        let rs = "let t = \"About WordCraft\"; // WordCraft can't\n";
        assert!(rebrand_file("crates/ui-egui/src/dialogs/chrome.rs", rs).unwrap_or_default().contains("\"About Strix Writer\"; // WordCraft can't"));
        assert!(rebrand_file("crates/other.rs", rs).unwrap_or_default().contains("\"About Strix\";"));
        let toml = "[package]\nname = \"wordcraft\"\ndefault-run = \"wordcraft\"\n";
        assert!(rebrand_file("apps/wordcraft/Cargo.toml", toml).unwrap_or_default().contains("name = \"strix\"\ndefault-run = \"strix\""));
        assert_eq!(rebrand_file("crates/doc/Cargo.toml", "name = \"wordcraft\"\n"), None);
    }

    /// `main` already shows what the next sync writes: every file rule's result is in its file.
    #[test]
    fn main_has_strixs_text_where_the_file_rules_put_it() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        for (path, from, to) in file_rules() {
            let text = std::fs::read_to_string(root.join(path)).unwrap_or_else(|e| panic!("{path}: {e}"));
            assert!(text.contains(to), "{path} lacks {to:?}");
            assert!(!text.contains(from) || to.contains(from), "{path} still has {from:?}");
        }
    }

    #[test]
    fn strix_owned_and_brand_paths_stay_out() {
        for p in [
            "docs/brand/artcraft-logo.svg",
            "packaging/windows/wordcraft.wxs",
            "site/landing.html",
            ".github/workflows/ci.yml",
            "README.md",
            "docs/README.de.md",
            "ROADMAP.md",
            "docs/gaps.md",
            "docs/typing-parity.md",
        ] {
            assert_eq!(rebrand_path(p), None, "{p}");
        }
        for p in ["docs/README/x.md", "docs/x/typing-parity.md", "docs/parity-checklist.md", ".github/dependabot.yml", "sitemap.md"] {
            assert!(rebrand_path(p).is_some(), "{p}");
        }
        assert_eq!(strix_owned_pathspecs()[0], ":(glob)packaging/**");
    }
}

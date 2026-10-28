//! `cargo xtask unused [--strict]`: code and dependencies nothing uses.
//!
//! - **Dependencies** a crate declares but never names in its sources (every crate).
//! - **Public items** (`pub fn`, `struct`, `enum`, `const`, `static`, `type`) in Strix's own files
//!   that no other code names, or that only tests name. Upstream WordCraft's files are left out:
//!   removing code there would only conflict at the next upstream merge (docs/upstream.md).
//!
//! Names are matched as identifiers, so a hit is a strong hint, not a proof (a name can be
//! reached through a macro or another crate's re-export). Report only; `--strict` fails on hits.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::process::Command;

const VENDOR_BRANCH: &str = crate::upstream::VENDOR_BRANCH;
const KINDS: &[&str] = &["fn", "struct", "enum", "const", "static", "type"];

pub fn run(root: &Path, args: &[&str]) -> Result<(), String> {
    let strict = args.contains(&"--strict");
    let files: Vec<String> = git(root, &["ls-files", "*.rs", "*Cargo.toml"])?.lines().map(str::to_string).collect();
    let mut sources: HashMap<String, String> = HashMap::new();
    for f in files.iter().filter(|f| f.ends_with(".rs")) {
        if let Ok(text) = std::fs::read_to_string(root.join(f)) {
            sources.insert(f.clone(), text);
        }
    }
    let deps = unused_dependencies(root, &files, &sources);
    let upstream: HashSet<String> =
        git(root, &["ls-tree", "-r", "--name-only", VENDOR_BRANCH]).unwrap_or_default().lines().map(str::to_string).collect();
    if upstream.is_empty() {
        eprintln!("warning: no `{VENDOR_BRANCH}` branch: every file counts as Strix's own (docs/upstream.md)");
    }
    let items = unused_items(&sources, &upstream);

    println!("Dependencies declared but never named in the crate's sources: {}", deps.len());
    for d in &deps {
        println!("  {d}");
    }
    println!("\nPublic items in Strix's own files that nothing else uses: {}", items.iter().filter(|i| !i.tests).count());
    for i in items.iter().filter(|i| !i.tests) {
        println!("  {}: {} {}", i.file, i.kind, i.name);
    }
    println!("\nPublic items in Strix's own files that only tests use: {}", items.iter().filter(|i| i.tests).count());
    for i in items.iter().filter(|i| i.tests) {
        println!("  {}: {} {}", i.file, i.kind, i.name);
    }
    let hits = deps.len() + items.len();
    if strict && hits > 0 { Err(format!("{hits} unused dependency or item(s)")) } else { Ok(()) }
}

/// `crate/Cargo.toml [section] name` for each dependency its crate's sources never name.
fn unused_dependencies(root: &Path, files: &[String], sources: &HashMap<String, String>) -> Vec<String> {
    let mut out = Vec::new();
    for manifest in files.iter().filter(|f| f.ends_with("Cargo.toml")) {
        let Some(dir) = manifest.strip_suffix("Cargo.toml") else { continue };
        if dir.is_empty() {
            continue; // the workspace manifest declares, it doesn't use
        }
        let Ok(text) = std::fs::read_to_string(root.join(manifest)) else { continue };
        let words = identifiers(sources.iter().filter(|(f, _)| f.starts_with(dir)).map(|(_, t)| t.as_str()));
        let mut section = String::new();
        for line in text.lines() {
            let line = line.trim();
            if let Some(s) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
                section = s.to_string();
                continue;
            }
            if !section.ends_with("dependencies") || line.starts_with('#') {
                continue;
            }
            let Some((name, _)) = line.split_once('=') else { continue };
            let name = name.trim();
            if name.is_empty() || name.contains('.') {
                continue;
            }
            if !words.contains_key(&name.replace('-', "_")) {
                out.push(format!("{manifest} [{section}] {name}"));
            }
        }
    }
    out.sort();
    out
}

struct Item {
    file: String,
    kind: &'static str,
    name: String,
    /// Named by tests only.
    tests: bool,
}

fn unused_items(sources: &HashMap<String, String>, upstream: &HashSet<String>) -> Vec<Item> {
    let mut prod_texts = Vec::new();
    let mut all_texts = Vec::new();
    for (f, t) in sources {
        all_texts.push(t.as_str());
        if !is_test_file(f) {
            prod_texts.push(without_tests(t));
        }
    }
    let prod = identifiers(prod_texts.iter().copied());
    let all = identifiers(all_texts.iter().copied());
    let mut out = Vec::new();
    let mut own: Vec<&String> = sources.keys().filter(|f| !upstream.contains(*f) && !is_test_file(f)).collect();
    own.sort();
    for f in own {
        let Some(text) = sources.get(f) else { continue };
        for (kind, name) in public_items(without_tests(text)) {
            // The definition itself is one mention.
            if prod.get(&name).copied().unwrap_or(0) <= 1 {
                let tests = all.get(&name).copied().unwrap_or(0) > 1;
                out.push(Item { file: f.clone(), kind, name, tests });
            }
        }
    }
    out
}

/// `(kind, name)` of each `pub` / `pub(crate)` / `pub(super)` item declared in `text`.
fn public_items(text: &str) -> Vec<(&'static str, String)> {
    let mut out = Vec::new();
    for line in text.lines() {
        let mut rest = line.trim_start();
        let Some(after) = rest.strip_prefix("pub") else { continue };
        rest = after;
        if let Some(r) = rest.strip_prefix('(') {
            let Some((_, r)) = r.split_once(')') else { continue };
            rest = r;
        }
        let mut words = rest.split_whitespace();
        let mut kind = words.next().unwrap_or_default();
        if kind == "const" && words.clone().next() == Some("fn") {
            kind = words.next().unwrap_or_default();
        }
        let Some(kind) = KINDS.iter().find(|k| **k == kind) else { continue };
        let name: String = words.next().unwrap_or_default().chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
        if !name.is_empty() && name != "main" {
            out.push((*kind, name));
        }
    }
    out
}

/// How many times each identifier occurs in `texts`.
fn identifiers<'a>(texts: impl Iterator<Item = &'a str>) -> HashMap<String, usize> {
    let mut counts = HashMap::new();
    for t in texts {
        for word in t.split(|c: char| !(c.is_alphanumeric() || c == '_')) {
            if word.chars().next().is_some_and(|c| c.is_alphabetic() || c == '_') {
                *counts.entry(word.to_string()).or_insert(0) += 1;
            }
        }
    }
    counts
}

/// A test file: under `tests/`, or `tests.rs` / `tests_*.rs`.
fn is_test_file(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    path.contains("/tests/") || name == "tests.rs" || name.starts_with("tests_")
}

/// `text` without its trailing `#[cfg(test)] mod tests { … }` (the usual place for tests).
fn without_tests(text: &str) -> &str {
    text.find("#[cfg(test)]\nmod tests {").or_else(|| text.find("#[cfg(test)]\r\nmod tests {")).map_or(text, |i| text.get(..i).unwrap_or(text))
}

fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    let out = Command::new("git").current_dir(dir).args(args).output().map_err(|e| format!("git: {e}"))?;
    if !out.status.success() {
        return Err(format!("git {}: {}", args.join(" "), String::from_utf8_lossy(&out.stderr).trim()));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_items_are_found() {
        let items =
            public_items("pub fn a() {}\n    pub(crate) struct B;\npub const fn c() {}\nfn d() {}\npub(super) const E: u8 = 1;\npub use x::y;");
        let names: Vec<String> = items.into_iter().map(|(k, n)| format!("{k} {n}")).collect();
        assert_eq!(names, ["fn a", "struct B", "fn c", "const E"]);
    }

    #[test]
    fn identifiers_are_whole_words() {
        let ids = identifiers(["snap(x); snap_points(y); 2snap"].into_iter());
        assert_eq!(ids.get("snap"), Some(&1));
        assert_eq!(ids.get("snap_points"), Some(&1));
    }

    #[test]
    fn inline_test_modules_and_test_files_are_tests() {
        assert_eq!(without_tests("pub fn a() {}\n#[cfg(test)]\nmod tests {\n a();\n}"), "pub fn a() {}\n");
        assert!(is_test_file("crates/cad/src/tests_dims.rs") && is_test_file("crates/docx/tests/it/main.rs"));
        assert!(!is_test_file("crates/cad/src/model.rs"));
    }
}

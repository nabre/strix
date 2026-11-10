//! `cargo xtask workflows` (part of `cargo xtask ci`): the GitHub workflows keep the values they
//! share in step, so each is decided once and can't drift between files:
//!
//! - every action from outside the suite is pinned to a full commit SHA (a tag can be moved), and
//!   one action uses one commit in every workflow;
//! - the craft-fonts commit (`craft-fonts-ref:`, `CRAFT_FONTS_REF:`, the `ref:` of a
//!   `checkout-craft-fonts` step) is the same everywhere: CI, the FreeBSD build, the release and the
//!   site build the same fonts.
//!
//! The suite's own actions and reusable workflows (`storytold/…`) are left out: they follow
//! `@main` on purpose, so suite-wide CI fixes reach every app (see the comments in the workflows).

use std::collections::BTreeMap;
use std::path::Path;

/// Owner of the suite's own actions, which may follow a branch.
const SUITE: &str = "storytold";

/// Whether `s` is a full commit SHA.
fn is_sha(s: &str) -> bool {
    s.len() == 40 && s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// The value after `key:` on a YAML line, without quotes or a trailing comment.
fn value_of<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let t = line.trim_start().trim_start_matches("- ").trim_start();
    let rest = t.strip_prefix(key)?.strip_prefix(':')?;
    let v = rest.split(" #").next().unwrap_or_default().trim();
    Some(v.trim_matches(|c| c == '"' || c == '\''))
}

/// What one workflow file says: actions used (action, ref, line) and craft-fonts refs (ref, line).
#[derive(Default, Debug)]
struct Found {
    actions: Vec<(String, String, usize)>,
    fonts: Vec<(String, usize)>,
}

fn scan(text: &str) -> Found {
    let mut out = Found::default();
    let lines: Vec<&str> = text.lines().collect();
    for (i, line) in lines.iter().enumerate() {
        let n = i + 1;
        if let Some(u) = value_of(line, "uses") {
            if let Some((action, rev)) = u.split_once('@') {
                out.actions.push((action.to_string(), rev.to_string(), n));
            }
            // The fonts checkout's `ref:` sits in its `with:` block, a few lines below.
            if u.contains("checkout-craft-fonts") {
                for (k, next) in lines.iter().enumerate().skip(i + 1).take(4) {
                    if next.trim_start().starts_with("- ") {
                        break;
                    }
                    if let Some(r) = value_of(next, "ref") {
                        out.fonts.push((r.to_string(), k + 1));
                    }
                }
            }
        }
        for key in ["craft-fonts-ref", "CRAFT_FONTS_REF"] {
            if let Some(r) = value_of(line, key) {
                out.fonts.push((r.to_string(), n));
            }
        }
    }
    out
}

/// The problems across workflow files `files` (name, text).
fn check(files: &[(String, String)]) -> Vec<String> {
    let mut problems = Vec::new();
    let mut shas: BTreeMap<String, BTreeMap<String, Vec<String>>> = BTreeMap::new();
    let mut fonts: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (name, text) in files {
        let found = scan(text);
        for (action, rev, n) in found.actions {
            if action.starts_with("./") || action.starts_with("docker://") || action.split('/').next() == Some(SUITE) {
                continue;
            }
            if is_sha(&rev) {
                shas.entry(action).or_default().entry(rev).or_default().push(format!("{name}:{n}"));
            } else {
                problems.push(format!("{name}:{n}: {action}@{rev} is not pinned to a commit SHA (a tag or branch can move)"));
            }
        }
        for (rev, n) in found.fonts {
            // An expression names the value set elsewhere in the file (`${{ env.CRAFT_FONTS_REF }}`).
            if rev.contains("${{") {
                continue;
            }
            if is_sha(&rev) {
                fonts.entry(rev).or_default().push(format!("{name}:{n}"));
            } else {
                problems.push(format!("{name}:{n}: craft-fonts ref `{rev}` is not a commit SHA"));
            }
        }
    }
    for (action, revs) in &shas {
        if revs.len() > 1 {
            let list: Vec<String> = revs.iter().map(|(rev, at)| format!("{rev} ({})", at.join(", "))).collect();
            problems.push(format!("{action} is pinned to {} different commits: {}", revs.len(), list.join("; ")));
        }
    }
    if fonts.len() > 1 {
        let list: Vec<String> = fonts.iter().map(|(rev, at)| format!("{rev} ({})", at.join(", "))).collect();
        problems.push(format!("the workflows build with {} different craft-fonts commits: {}", fonts.len(), list.join("; ")));
    }
    problems
}

pub fn run(root: &Path) -> Result<(), String> {
    let dir = root.join(".github/workflows");
    let entries = std::fs::read_dir(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut files = Vec::new();
    for entry in entries {
        let path = entry.map_err(|e| format!("{}: {e}", dir.display()))?.path();
        if !matches!(path.extension().and_then(std::ffi::OsStr::to_str), Some("yml" | "yaml")) {
            continue;
        }
        let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let name = path.strip_prefix(root).unwrap_or(&path).display().to_string();
        files.push((name, text));
    }
    files.sort();
    if files.is_empty() {
        return Err(format!("no workflow in {}", dir.display()));
    }
    let problems = check(&files);
    if problems.is_empty() {
        println!("workflows: {} files, actions pinned and shared values in step", files.len());
        Ok(())
    } else {
        Err(format!("workflows:\n  {}", problems.join("\n  ")))
    }
}

#[cfg(test)]
mod tests {
    use super::check;

    const A: &str = "0123456789abcdef0123456789abcdef01234567";
    const B: &str = "fedcba9876543210fedcba9876543210fedcba98";

    fn files(list: &[(&str, &str)]) -> Vec<(String, String)> {
        list.iter().map(|(n, t)| (n.to_string(), t.to_string())).collect()
    }

    #[test]
    fn pinned_and_in_step_passes() {
        let one = format!(
            "steps:\n      - uses: actions/checkout@{A} # v7\n      - uses: storytold/craft-common-actions/actions/checkout-craft-fonts@main\n        with:\n          ref: ${{{{ env.CRAFT_FONTS_REF }}}}\nenv:\n  CRAFT_FONTS_REF: {B}\n"
        );
        let two = format!(
            "jobs:\n  gate:\n    uses: storytold/craft-common-actions/.github/workflows/gate.yml@main\n    with:\n      craft-fonts-ref: {B}\n      - uses: actions/checkout@{A}\n      - uses: ./local\n"
        );
        assert_eq!(check(&files(&[("a.yml", &one), ("b.yml", &two)])), Vec::<String>::new());
    }

    #[test]
    fn a_tag_or_two_commits_or_two_font_refs_fail() {
        let one = format!("      - uses: actions/checkout@v7\n      - uses: dtolnay/rust-toolchain@{A}\n      craft-fonts-ref: {A}\n");
        let two = format!(
            "      - uses: dtolnay/rust-toolchain@{B} # stable\n      - uses: storytold/craft-common-actions/actions/checkout-craft-fonts@main\n        with:\n          ref: {B} # same as release.yml\n"
        );
        let p = check(&files(&[("a.yml", &one), ("b.yml", &two)]));
        assert_eq!(p.len(), 3, "{p:#?}");
        assert!(p[0].starts_with("a.yml:1: actions/checkout@v7 is not pinned"), "{p:#?}");
        assert!(p[1].starts_with("dtolnay/rust-toolchain is pinned to 2 different commits"), "{p:#?}");
        assert!(p[2].contains("2 different craft-fonts commits") && p[2].contains("b.yml:4"), "{p:#?}");
    }

    #[test]
    fn a_branch_as_the_fonts_ref_fails() {
        let p = check(&files(&[("a.yml", "env:\n  CRAFT_FONTS_REF: main\n")]));
        assert_eq!(p, vec!["a.yml:2: craft-fonts ref `main` is not a commit SHA".to_string()]);
    }
}

//! `cargo xtask dev [--once] [-- <app args>]`: runs the app in a debug build and, when
//! `watchexec` is installed, rebuilds and restarts it on every change to the code, the manifests
//! or the translations (`crates/`, `apps/`, `locales/`, `Cargo.toml`).
//!
//! Tuned to spare RAM and disk rather than to win seconds: no debug info for Strix's own crates
//! (dependencies already have none, see `[profile.dev.package."*"]`) and half the cores for rustc
//! (each instance can take a gigabyte or more). These go through environment variables, so
//! nothing in `Cargo.toml` or `.cargo/config.toml` changes; a plain `cargo build` afterwards
//! recompiles Strix's crates once (not the dependencies). The linker is left alone on purpose:
//! cargo fingerprints it, so setting it here would rebuild every dependency each time one switches
//! between this command and a plain `cargo`; a faster linker belongs in the user's own
//! `~/.cargo/config.toml`, where every build sees it. With no args the app opens the sample
//! document (`--sample`).

use std::process::Command;

pub fn run(rest: &[&str]) -> Result<(), String> {
    let (opts, app_args) = match rest.iter().position(|a| *a == "--") {
        Some(i) => (rest.get(..i).unwrap_or(&[]), rest.get(i + 1..).unwrap_or(&[])),
        None => (rest, &[][..]),
    };
    let once = opts.contains(&"--once");
    if let Some(bad) = opts.iter().find(|a| **a != "--once") {
        return Err(format!("dev: unknown option `{bad}` (usage: cargo xtask dev [--once] [-- <app args>])"));
    }
    let app_args: Vec<&str> = if app_args.is_empty() { vec!["--sample"] } else { app_args.to_vec() };

    let watch = !once && has_watchexec();
    if !once && !watch {
        eprintln!("dev: watchexec not found, running once (install it with `cargo install watchexec-cli`)");
    }

    let mut cmd = if watch {
        let mut c = Command::new("watchexec");
        c.args(["--restart", "--debounce", "500ms", "--shell", "none"]);
        for dir in ["crates", "apps", "locales", "Cargo.toml"] {
            c.args(["--watch", dir]);
        }
        c.args(["--exts", "rs,toml,tsv", "--", "cargo", "run", "-p", "strix", "--"]);
        c.current_dir(crate::root());
        c
    } else {
        let mut c = crate::cargo();
        c.args(["run", "-p", "strix", "--"]);
        c
    };
    cmd.args(&app_args);
    light_env(&mut cmd);

    let what = if watch { "watchexec … cargo run -p strix" } else { "cargo run -p strix" };
    eprintln!("$ {what} -- {}", app_args.join(" "));
    let status = cmd.status().map_err(|e| format!("{what}: failed to spawn: {e}"))?;
    if status.success() { Ok(()) } else { Err(format!("{what}: exited with {status}")) }
}

/// Build settings that keep RAM and disk use down (see the module docs).
fn light_env(cmd: &mut Command) {
    cmd.env("CARGO_PROFILE_DEV_DEBUG", "false");
    let cores = std::thread::available_parallelism().map_or(2, std::num::NonZero::get);
    cmd.env("CARGO_BUILD_JOBS", (cores / 2).max(2).to_string());
}

fn has_watchexec() -> bool {
    Command::new("watchexec").arg("--version").output().is_ok_and(|o| o.status.success())
}

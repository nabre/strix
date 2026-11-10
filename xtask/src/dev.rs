//! `cargo xtask dev [--once] [--web | --desktop] [-- <app args>]`: runs the app in a debug build
//! and, when `watchexec` is installed, rebuilds and restarts it on every change to the code, the
//! manifests or the translations (`crates/`, `apps/`, `locales/`, `Cargo.toml`).
//!
//! On a machine with no screen (Linux or BSD with neither `DISPLAY` nor `WAYLAND_DISPLAY` nor
//! `WAYLAND_SOCKET`: a sandbox, an SSH session) the window can't open, so it serves the web build
//! instead (`--web` asks for it anywhere, `--desktop` refuses it): `trunk serve` in
//! `apps/strix-web`, which rebuilds on every change and reloads the page by itself (no watchexec
//! needed), on 127.0.0.1:8771 (`apps/strix-web/Trunk.toml`); open it in a browser, from another
//! computer through an SSH tunnel to that port. `--sample` becomes `?sample` in the address.
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

const USAGE: &str = "usage: cargo xtask dev [--once] [--web | --desktop] [-- <app args>]";

pub fn run(rest: &[&str]) -> Result<(), String> {
    let (opts, app_args) = match rest.iter().position(|a| *a == "--") {
        Some(i) => (rest.get(..i).unwrap_or(&[]), rest.get(i + 1..).unwrap_or(&[])),
        None => (rest, &[][..]),
    };
    if let Some(bad) = opts.iter().find(|a| !["--once", "--web", "--desktop"].contains(*a)) {
        return Err(format!("dev: unknown option `{bad}` ({USAGE})"));
    }
    let (once, web, desktop) = (opts.contains(&"--once"), opts.contains(&"--web"), opts.contains(&"--desktop"));
    if web && desktop {
        return Err(format!("dev: --web and --desktop together ({USAGE})"));
    }
    let app_args: Vec<&str> = if app_args.is_empty() { vec!["--sample"] } else { app_args.to_vec() };

    let headless = !has_screen();
    if web || (headless && !desktop) {
        if headless && !web {
            eprintln!(
                "dev: no screen here (DISPLAY and WAYLAND_DISPLAY are not set): serving the web build instead (--desktop to try the window anyway)"
            );
        }
        return run_web(once, &app_args);
    }

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

/// The web build served by trunk, which watches the workspace and reloads the page itself;
/// `--once` turns the reload off (a rebuild still happens, the page waits for a manual reload).
fn run_web(once: bool, app_args: &[&str]) -> Result<(), String> {
    if !Command::new("trunk").arg("--version").output().is_ok_and(|o| o.status.success()) {
        return Err(
            "dev: trunk not found: install it with `cargo install trunk --locked` and add the target with `rustup target add wasm32-unknown-unknown`"
                .into(),
        );
    }
    let ignored: Vec<&&str> = app_args.iter().filter(|a| **a != "--sample").collect();
    if !ignored.is_empty() {
        eprintln!("dev: the web build takes no command-line arguments, ignoring {ignored:?}");
    }
    let query = if app_args.contains(&"--sample") { "?sample" } else { "" };
    let mut cmd = Command::new("trunk");
    cmd.arg("serve").current_dir(crate::root().join("apps/strix-web"));
    if once {
        cmd.arg("--no-autoreload");
    }
    light_env(&mut cmd);
    eprintln!("$ (apps/strix-web) trunk serve{}", if once { " --no-autoreload" } else { "" });
    eprintln!(
        "dev: when it says \"server listening\", open http://127.0.0.1:8771/{query} (from another computer: through an SSH tunnel to port 8771)"
    );
    let status = cmd.status().map_err(|e| format!("trunk serve: failed to spawn: {e}"))?;
    if status.success() { Ok(()) } else { Err(format!("trunk serve: exited with {status}")) }
}

/// Whether a window can open: always on Windows and macOS; elsewhere (winit on X11 or Wayland)
/// only with a display to talk to.
fn has_screen() -> bool {
    if cfg!(any(windows, target_os = "macos")) {
        return true;
    }
    ["DISPLAY", "WAYLAND_DISPLAY", "WAYLAND_SOCKET"].iter().any(|v| std::env::var_os(v).is_some_and(|x| !x.is_empty()))
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

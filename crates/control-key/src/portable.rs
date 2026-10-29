//! Portable mode: the settings folder next to the program instead of in the user's profile.
//!
//! Strix runs portable when, beside the running executable, there is
//! - a folder named `StrixData`: the portable ZIP ships one, and any copy of
//!   the program finds it; or
//! - no such folder yet, but the executable's file name contains `portable` (the single-file
//!   `strix-<version>-windows-<arch>-portable.exe`): the folder is made on the first run, so a
//!   copy renamed afterwards still finds it.
//!
//! Then every per-user file (preferences, logs, recovery copies, the custom dictionary, personal
//! templates, added fonts, control-key files, extra dictionaries) lives in that folder, and
//! nothing is written to `%APPDATA%`, the home folder or the registry. When the folder can't be
//! made or written to (a read-only drive, a protected folder) the normal location is used and
//! [`Portable::Unavailable`] says why, for the app to log. See `docs/releasing.md`.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// The portable settings folder's name, next to the executable.
pub(crate) const PORTABLE_DIR: &str = "StrixData";

/// What the running program decided about portable mode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Portable {
    /// Not portable: settings live in the user's profile.
    Off,
    /// Portable: settings live in this folder next to the executable.
    On(PathBuf),
    /// Portable was asked for (the folder or the executable's name) but the folder can't be
    /// used; settings live in the user's profile. `reason` says why.
    Unavailable { dir: PathBuf, reason: String },
}

/// The decision for the running executable, made once per process.
pub fn portable() -> &'static Portable {
    static DECISION: OnceLock<Portable> = OnceLock::new();
    DECISION.get_or_init(|| match std::env::current_exe() {
        Ok(exe) => detect(&exe),
        Err(_) => Portable::Off,
    })
}

/// The portable settings folder, when this run is portable.
pub fn portable_dir() -> Option<&'static Path> {
    match portable() {
        Portable::On(dir) => Some(dir.as_path()),
        _ => None,
    }
}

/// Portable mode for the program at `exe`: see the module docs. Never panics; a folder that
/// can't be made or written is [`Portable::Unavailable`].
fn detect(exe: &Path) -> Portable {
    let Some(parent) = exe.parent() else { return Portable::Off };
    let dir = parent.join(PORTABLE_DIR);
    if !dir.is_dir() && !named_portable(exe) {
        return Portable::Off;
    }
    if let Err(e) = std::fs::create_dir_all(&dir) {
        return Portable::Unavailable { reason: format!("cannot create {}: {e}", dir.display()), dir };
    }
    match check_writable(&dir) {
        Ok(()) => Portable::On(dir),
        Err(e) => Portable::Unavailable { reason: format!("cannot write to {}: {e}", dir.display()), dir },
    }
}

/// Whether the executable's file name asks for portable mode (`…portable…`, any case).
fn named_portable(exe: &Path) -> bool {
    exe.file_stem().is_some_and(|s| s.to_string_lossy().to_ascii_lowercase().contains("portable"))
}

/// Writes and removes a small probe file in `dir`.
fn check_writable(dir: &Path) -> std::io::Result<()> {
    use std::io::Write;
    let probe = dir.join(format!(".strix-write-test-{}", std::process::id()));
    let written = std::fs::OpenOptions::new().write(true).create_new(true).open(&probe).and_then(|mut f| f.write_all(b"ok"));
    let removed = std::fs::remove_file(&probe);
    written.and(removed)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fresh, empty folder for one test.
    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("strix-portable-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn no_marker_is_not_portable() {
        let dir = temp("off");
        assert_eq!(detect(&dir.join("strix.exe")), Portable::Off);
        assert!(!dir.join(PORTABLE_DIR).exists(), "nothing is made");
        // A file named like the folder isn't the marker.
        std::fs::write(dir.join(PORTABLE_DIR), b"").unwrap();
        assert_eq!(detect(&dir.join("strix.exe")), Portable::Off);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_folder_beside_the_program_makes_it_portable() {
        let dir = temp("folder");
        std::fs::create_dir(dir.join(PORTABLE_DIR)).unwrap();
        assert_eq!(detect(&dir.join("strix.exe")), Portable::On(dir.join(PORTABLE_DIR)));
        assert_eq!(detect(&dir.join("strix-cli.exe")), Portable::On(dir.join(PORTABLE_DIR)), "the CLI beside it shares it");
        assert_eq!(std::fs::read_dir(dir.join(PORTABLE_DIR)).unwrap().count(), 0, "the write probe is removed");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_portable_named_program_makes_its_folder() {
        let dir = temp("named");
        let exe = dir.join("strix-0.14.0-windows-x64-Portable.exe");
        assert_eq!(detect(&exe), Portable::On(dir.join(PORTABLE_DIR)));
        assert!(dir.join(PORTABLE_DIR).is_dir(), "made on the first run");
        // Renamed later, the program still finds its folder.
        assert_eq!(detect(&dir.join("strix.exe")), Portable::On(dir.join(PORTABLE_DIR)));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_folder_that_cant_be_made_falls_back_with_a_reason() {
        let dir = temp("blocked");
        // A file where the folder would go: it can't be made.
        std::fs::write(dir.join(PORTABLE_DIR), b"").unwrap();
        match detect(&dir.join("strix-portable.exe")) {
            Portable::Unavailable { dir: d, reason } => {
                assert_eq!(d, dir.join(PORTABLE_DIR));
                assert!(reason.contains("cannot create"), "{reason}");
            }
            other => panic!("expected Unavailable, got {other:?}"),
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    #[cfg(unix)]
    #[test]
    fn a_read_only_folder_falls_back_with_a_reason() {
        use std::os::unix::fs::PermissionsExt;
        let dir = temp("readonly");
        let data = dir.join(PORTABLE_DIR);
        std::fs::create_dir(&data).unwrap();
        std::fs::set_permissions(&data, std::fs::Permissions::from_mode(0o555)).unwrap();
        // Root writes anyway: then there's nothing to check.
        if check_writable(&data).is_err() {
            match detect(&dir.join("strix")) {
                Portable::Unavailable { reason, .. } => assert!(reason.contains("cannot write"), "{reason}"),
                other => panic!("expected Unavailable, got {other:?}"),
            }
        }
        std::fs::set_permissions(&data, std::fs::Permissions::from_mode(0o755)).unwrap();
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn hostile_paths_never_panic() {
        assert_eq!(detect(Path::new("")), Portable::Off);
        assert_eq!(detect(Path::new("/")), Portable::Off);
        assert!(!named_portable(Path::new("")));
        assert!(named_portable(Path::new("x/PORTABLE")));
    }
}

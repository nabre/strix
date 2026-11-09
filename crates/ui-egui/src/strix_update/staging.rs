//! The update Strix has downloaded and verified ahead of time, waiting to be installed when Strix
//! next starts (docs/updates.md). The package stays where `craft_update` downloads it (the
//! `downloads` folder of Strix's cache); this module keeps a small record beside Strix's update
//! data (`strix-staged-update.json`) naming the release and the file, decides at start whether it
//! can be installed without asking anything, and installs it there through
//! `craft_update::self_update::update`, whose restart helper starts the new version once this
//! process has exited.
//!
//! Only installations that are replaced without a question stage for a silent install: the
//! per-user Windows Installer package (run unattended by the helper), the AppImage and the
//! binary of a tar.gz (renamed over the old file). A `.deb`/`.rpm` needs the package manager's
//! password prompt, so it is downloaded ahead but installed only from the Software Update window.
//! Anything that goes wrong discards the staged file and starts the current version: the next
//! check offers the update again.

use std::path::Path;

use craft_update::self_update::{self, Install};
use craft_update::{App, Asset, Http, PackageKind, Paths, Platform, Release, Stop, pick_asset};
use serde::{Deserialize, Serialize};

/// Ends `self_update::update` once the package is downloaded and verified, before anything is
/// replaced: what remains happens at the next start.
const STAGED: &str = "strix: staged for the next start";

/// The record's file name, in Strix's update data folder.
const RECORD: &str = "strix-staged-update.json";

/// The file that turns the automatic download off.
const SETTINGS: &str = "strix-update-settings.json";

/// A downloaded, verified package waiting for the next start.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct Staged {
    /// The release, with every checksum filled in, so installing it needs no network.
    pub release: Release,
    /// The package's file name among the release's assets.
    pub asset: String,
}

impl Staged {
    fn asset(&self) -> Option<&Asset> {
        self.release.assets.iter().find(|a| a.name == self.asset)
    }
}

/// The update preferences Strix keeps beside its update data.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Settings {
    auto_download: bool,
}

/// Whether a newer release is downloaded as soon as it is found (on unless turned off).
pub(super) fn auto_download(data: &Path) -> bool {
    std::fs::read_to_string(data.join(SETTINGS)).ok().and_then(|t| serde_json::from_str::<Settings>(&t).ok()).is_none_or(|s| s.auto_download)
}

pub(super) fn set_auto_download(data: &Path, on: bool) -> Result<(), String> {
    std::fs::create_dir_all(data).map_err(|e| format!("couldn't create {} ({e})", data.display()))?;
    let text = serde_json::to_string(&Settings { auto_download: on }).map_err(|e| e.to_string())?;
    craft_update::paths::write_atomic(&data.join(SETTINGS), text.as_bytes())
}

/// Whether this installation is replaced at start without any question.
pub(super) fn silent(install: &Install) -> bool {
    matches!(install, Install::Msi(_) | Install::AppImage(_) | Install::Standalone(_))
}

/// The package format that updates `install` (as `craft_update` chooses it).
pub(super) fn kind_of(install: &Install) -> PackageKind {
    match install {
        Install::MacApp(_) => PackageKind::Dmg,
        Install::Msi(_) => PackageKind::Msi,
        Install::Portable(_) => PackageKind::PortableZip,
        Install::AppImage(_) => PackageKind::AppImage,
        Install::Package(kind, _) => *kind,
        Install::Flatpak => PackageKind::Flatpak,
        Install::Standalone(_) if cfg!(windows) => PackageKind::PortableZip,
        Install::Standalone(_) => PackageKind::TarGz,
    }
}

pub(super) fn load(data: &Path) -> Option<Staged> {
    let text = std::fs::read_to_string(data.join(RECORD)).ok()?;
    match serde_json::from_str(&text) {
        Ok(staged) => Some(staged),
        Err(e) => {
            log::warn!("Strix update: unreadable staged update record ({e}); discarded");
            let _ = std::fs::remove_file(data.join(RECORD));
            None
        }
    }
}

fn save(data: &Path, staged: &Staged) -> Result<(), String> {
    std::fs::create_dir_all(data).map_err(|e| format!("couldn't create {} ({e})", data.display()))?;
    let text = serde_json::to_string(staged).map_err(|e| e.to_string())?;
    craft_update::paths::write_atomic(&data.join(RECORD), text.as_bytes())
}

/// Forgets the staged update and deletes its package.
pub(super) fn discard(paths: &Paths) {
    if let Some(staged) = load(&paths.data)
        && let Some(asset) = staged.asset()
    {
        remove_download(paths, asset);
    }
    let _ = std::fs::remove_file(paths.data.join(RECORD));
}

/// Deletes a downloaded package, only ever inside the downloads folder.
fn remove_download(paths: &Paths, asset: &Asset) {
    if craft_update::release::valid_file_name(&asset.name) {
        let _ = std::fs::remove_file(craft_update::install::download_path(paths, asset));
    }
}

/// What the start does with a staged update.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Plan {
    /// Install it now, silently: the package and this installation match.
    Apply(Install),
    /// Leave it for the Software Update window (it needs a question, or this start opens
    /// documents and the restart helper would start the new version without them).
    Keep,
    /// It no longer applies (installed already, older, another kind of installation): forget it.
    Discard(String),
}

/// Decides what to do with `staged` at start. `running`: this version; `install`: how this copy
/// is installed, if it can update itself; `plain`: the start opens nothing particular (no files,
/// sample or control port), so the helper may start the new version in its place;
/// `platform`: this computer's.
pub(super) fn plan(staged: &Staged, running: &str, install: Option<Install>, plain: bool, platform: Option<Platform>) -> Plan {
    let newer = match (staged.release.version.as_ref(), craft_update::parse_version(running)) {
        (Some(new), Some(cur)) => craft_update::is_newer(new, &cur),
        _ => false,
    };
    if !newer {
        return Plan::Discard(format!("{} isn't newer than {running}", staged.release.version_label()));
    }
    let Some(install) = install.filter(super::replaces_itself) else {
        return Plan::Discard("this copy no longer updates itself".into());
    };
    let Some(asset) = staged.asset() else { return Plan::Discard("the record names no package of the release".into()) };
    if asset.sha256.as_deref().is_none_or(|s| s.len() != 64) {
        return Plan::Discard("the package has no checksum".into());
    }
    let picked = platform.and_then(|p| pick_asset(&staged.release, &[super::STRIX.id], p, &[kind_of(&install)]));
    if picked.is_none_or(|(a, _)| a.name != asset.name) {
        return Plan::Discard(format!("{} isn't the package for this installation", asset.name));
    }
    if !silent(&install) || !plain {
        return Plan::Keep;
    }
    Plan::Apply(install)
}

/// Whether the staged package is on disk and still the verified file.
fn intact(paths: &Paths, staged: &Staged) -> bool {
    let Some(asset) = staged.asset() else { return false };
    let file = craft_update::install::download_path(paths, asset);
    file.is_file() && asset.sha256.as_deref().is_some_and(|sha| craft_update::http::sha256_file(&file).is_ok_and(|h| h.eq_ignore_ascii_case(sha)))
}

/// Downloads and verifies `release` for `install` into the cache and records it for the next
/// start (runs on its thread). `stop` cancels the download. Returns the record.
pub(super) fn stage(app: &App, install: &Install, release: &Release, running: &str, stop: &Stop) -> Result<Staged, String> {
    let http = Http::new("Strix/Update")?;
    let paths = Paths::for_app(app)?;
    let platform = Platform::current().ok_or("this platform doesn't update itself")?;
    let mut release = release.clone();
    // The checksums go into the record, so installing it at start never waits for the network.
    if release.assets.iter().any(|a| a.sha256.is_none())
        && let Some(sums) = release.sums_asset().map(|a| a.url.clone())
        && let Some(text) = http.get_text(&sums).map_err(|e| format!("couldn't fetch the release's checksums ({e})"))?
    {
        release.apply_sums(&craft_update::release::parse_sha256sums(&text));
    }
    let (asset, _) = pick_asset(&release, &[app.id], platform, &[kind_of(install)])
        .ok_or_else(|| format!("{} has no {} for {}", release.version_label(), kind_of(install).label(), platform.label()))?;
    let staged = Staged { release: release.clone(), asset: asset.name.clone() };
    // A package staged earlier for another release makes way.
    if let Some(old) = load(&paths.data)
        && old.asset != staged.asset
    {
        discard(&paths);
    }
    let last = || Err(STAGED.to_string());
    match self_update::update(app, install, &release, platform, &http, &paths, stop, &mut |_| {}, &last, Some(running)) {
        Err(e) if e == STAGED => {}
        Err(e) => return Err(e),
        // `last` refuses, so nothing was replaced; never expected.
        Ok(()) => return Err("the update went further than downloading".into()),
    }
    save(&paths.data, &staged)?;
    Ok(staged)
}

/// At start, before the window opens: installs a staged update silently when it applies.
/// `plain`: see [`plan`]. True when the new version has been put in place and will be started
/// by the restart helper once this process exits, so the caller exits at once. Never fails the
/// start: a problem is logged, the staged file discarded, and the current version runs.
pub(super) fn apply_at_start(plain: bool) -> bool {
    let Ok(paths) = Paths::for_app(&super::STRIX) else { return false };
    if let Some(e) = self_update::take_failure(&paths) {
        log::warn!("the last Strix update failed: {e}");
    }
    let Some(staged) = load(&paths.data) else { return false };
    let running = self_update::current_version(&super::STRIX, env!("CARGO_PKG_VERSION"))
        .map_or_else(|| env!("CARGO_PKG_VERSION").to_string(), |v| v.to_string());
    let install = self_update::current(&super::STRIX);
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| match plan(&staged, &running, install, plain, Platform::current()) {
        Plan::Keep => Ok(false),
        Plan::Discard(why) => Err(why),
        Plan::Apply(install) => install_staged(&paths, &staged, &install, &running).map(|()| true),
    }))
    .unwrap_or_else(|_| Err("the update failed unexpectedly".into()));
    match outcome {
        Ok(applied) => {
            if applied {
                log::info!("Strix update: {} installed; it starts in place of {running}", staged.release.version_label());
                let _ = std::fs::remove_file(paths.data.join(RECORD));
            }
            applied
        }
        Err(e) => {
            log::warn!("Strix update: the staged {} wasn't installed: {e}", staged.release.version_label());
            discard(&paths);
            false
        }
    }
}

/// Puts the staged package in place and schedules the new version's start.
fn install_staged(paths: &Paths, staged: &Staged, install: &Install, running: &str) -> Result<(), String> {
    // Without the verified file `update` would download it again: no network at start.
    if !intact(paths, staged) {
        return Err("the downloaded package is missing or changed".into());
    }
    let http = Http::new("Strix/Update")?;
    let platform = Platform::current().ok_or("this platform doesn't update itself")?;
    self_update::update(&super::STRIX, install, &staged.release, platform, &http, paths, &Stop::default(), &mut |_| {}, &|| Ok(()), Some(running))
}

/// Installs the staged update while Strix quits ("Restart Now"), so the restart helper starts the
/// new version. Logged and discarded on failure: Strix then just quits.
pub(super) fn apply_at_exit() {
    let Ok(paths) = Paths::for_app(&super::STRIX) else { return };
    let Some(staged) = load(&paths.data) else { return };
    let running = self_update::current_version(&super::STRIX, env!("CARGO_PKG_VERSION"))
        .map_or_else(|| env!("CARGO_PKG_VERSION").to_string(), |v| v.to_string());
    let outcome = match plan(&staged, &running, self_update::current(&super::STRIX), true, Platform::current()) {
        Plan::Apply(install) => install_staged(&paths, &staged, &install, &running),
        Plan::Keep => Err("this installation can't be updated without a question".into()),
        Plan::Discard(why) => Err(why),
    };
    match outcome {
        Ok(()) => {
            log::info!("Strix update: {} installed; restarting", staged.release.version_label());
            let _ = std::fs::remove_file(paths.data.join(RECORD));
        }
        Err(e) => {
            log::warn!("Strix update: the staged {} wasn't installed: {e}", staged.release.version_label());
            discard(&paths);
        }
    }
}

/// The folder a test keeps its records in.
#[cfg(test)]
pub(super) fn test_dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("strix-update-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

#[cfg(test)]
mod tests {
    use super::*;
    use craft_update::{Arch, Os};
    use std::path::PathBuf;

    const SHA: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    fn staged(tag: &str, asset: &str) -> Staged {
        let v = tag.trim_start_matches('v');
        let names = [format!("strix-{v}-linux-x86_64.AppImage"), format!("strix-{v}-linux-x86_64.tar.gz"), format!("strix-{v}-windows-x64.msi")];
        Staged {
            release: Release {
                tag: tag.into(),
                version: craft_update::parse_version(v),
                html_url: format!("https://github.com/nabre/strix/releases/tag/{tag}"),
                assets: names
                    .iter()
                    .map(|n| Asset {
                        name: n.clone(),
                        size: 1,
                        url: format!("https://github.com/nabre/strix/releases/download/{tag}/{n}"),
                        sha256: Some(SHA.into()),
                    })
                    .collect(),
                ..Release::default()
            },
            asset: asset.into(),
        }
    }

    const LINUX: Option<Platform> = Some(Platform { os: Os::Linux, arch: Arch::X64 });

    fn appimage() -> Install {
        Install::AppImage(PathBuf::from("/home/u/Strix.AppImage"))
    }

    #[test]
    fn a_staged_appimage_installs_at_a_plain_start() {
        let s = staged("v0.22.0", "strix-0.22.0-linux-x86_64.AppImage");
        assert_eq!(plan(&s, "0.21.0", Some(appimage()), true, LINUX), Plan::Apply(appimage()));
        // A start that opens documents leaves it for later: the helper would start without them.
        assert_eq!(plan(&s, "0.21.0", Some(appimage()), false, LINUX), Plan::Keep);
    }

    #[test]
    fn a_staged_update_that_no_longer_applies_is_discarded() {
        let s = staged("v0.22.0", "strix-0.22.0-linux-x86_64.AppImage");
        // Already running it (or something newer).
        assert!(matches!(plan(&s, "0.22.0", Some(appimage()), true, LINUX), Plan::Discard(_)));
        assert!(matches!(plan(&s, "0.23.1", Some(appimage()), true, LINUX), Plan::Discard(_)));
        // This copy can't update itself any more, or is now another kind of installation.
        assert!(matches!(plan(&s, "0.21.0", None, true, LINUX), Plan::Discard(_)));
        assert!(matches!(plan(&s, "0.21.0", Some(Install::Flatpak), true, LINUX), Plan::Discard(_)));
        let deb = Install::Package(PackageKind::Deb, PathBuf::from("/usr/bin/strix"));
        assert!(matches!(plan(&s, "0.21.0", Some(deb), true, LINUX), Plan::Discard(_)));
        // A record naming a file the release doesn't have, or one without a checksum.
        let mut odd = staged("v0.22.0", "strix-0.22.0-other.bin");
        assert!(matches!(plan(&odd, "0.21.0", Some(appimage()), true, LINUX), Plan::Discard(_)));
        odd.asset = "strix-0.22.0-linux-x86_64.AppImage".into();
        odd.release.assets.iter_mut().for_each(|a| a.sha256 = None);
        assert!(matches!(plan(&odd, "0.21.0", Some(appimage()), true, LINUX), Plan::Discard(_)));
        // An unknown platform.
        assert!(matches!(plan(&s, "0.21.0", Some(appimage()), true, None), Plan::Discard(_)));
    }

    #[test]
    fn a_package_that_needs_the_password_prompt_waits_for_the_window() {
        let mut s = staged("v0.22.0", "strix-0.22.0-linux-x86_64.deb");
        s.release.assets.push(Asset {
            name: s.asset.clone(),
            size: 1,
            url: "https://github.com/nabre/strix/releases/download/v0.22.0/strix-0.22.0-linux-x86_64.deb".into(),
            sha256: Some(SHA.into()),
        });
        let deb = Install::Package(PackageKind::Deb, PathBuf::from("/usr/bin/strix"));
        assert!(!silent(&deb));
        assert_eq!(plan(&s, "0.21.0", Some(deb), true, LINUX), Plan::Keep);
    }

    #[test]
    fn only_unattended_installations_install_silently() {
        assert!(silent(&appimage()));
        assert!(silent(&Install::Msi(PathBuf::from("C:/x/strix.exe"))));
        assert!(silent(&Install::Standalone(PathBuf::from("/home/u/strix/strix"))));
        assert!(!silent(&Install::Flatpak));
        assert!(!silent(&Install::MacApp(PathBuf::from("/Applications/Strix.app"))));
        assert_eq!(kind_of(&appimage()), PackageKind::AppImage);
        assert_eq!(kind_of(&Install::Msi(PathBuf::from("C:/x/strix.exe"))), PackageKind::Msi);
    }

    #[test]
    fn the_record_and_the_setting_round_trip_and_discard_removes_the_package() {
        let dir = test_dir("record");
        let paths = Paths::under(&dir);
        assert_eq!(load(&paths.data), None);
        let s = staged("v0.22.0", "strix-0.22.0-linux-x86_64.AppImage");
        save(&paths.data, &s).unwrap();
        assert_eq!(load(&paths.data), Some(s.clone()));
        // The package in the downloads folder, as `craft_update` puts it there.
        let file = craft_update::install::download_path(&paths, s.asset().unwrap());
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, b"package").unwrap();
        assert!(!intact(&paths, &s), "the checksum doesn't match these bytes");
        let mut good = s.clone();
        let sha = craft_update::http::sha256_file(&file).unwrap();
        good.release.assets.iter_mut().for_each(|a| a.sha256 = Some(sha.clone()));
        assert!(intact(&paths, &good));
        discard(&paths);
        assert_eq!(load(&paths.data), None);
        assert!(!file.exists());
        // A damaged record is dropped, not trusted.
        std::fs::write(paths.data.join(RECORD), "{not json").unwrap();
        assert_eq!(load(&paths.data), None);
        assert!(!paths.data.join(RECORD).exists());
        // Automatic download: on until turned off.
        assert!(auto_download(&paths.data));
        set_auto_download(&paths.data, false).unwrap();
        assert!(!auto_download(&paths.data));
        set_auto_download(&paths.data, true).unwrap();
        assert!(auto_download(&paths.data));
        let _ = std::fs::remove_dir_all(&dir);
    }
}

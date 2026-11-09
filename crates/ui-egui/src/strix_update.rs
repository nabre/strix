//! Updates for Strix Writer (docs/updates.md): at start, and on Help › Check for Updates, Strix asks
//! GitHub for the newest published release of nabre/strix; when there is one it offers it in a
//! translated window. Installations Strix can replace safely update themselves and restart (the
//! Windows Installer package installed per user, the AppImage, the .deb/.rpm packages, a tar.gz
//! unpacked in the home folder on Linux); every other one (macOS, Flatpak, FreeBSD, portable and
//! per-machine Windows copies) gets the package for its platform downloaded in the browser.
//!
//! The release checking, downloading, verifying and swapping is `craft_update`'s (shared with the
//! Craft apps); Strix brings its own identity ([`STRIX`]), the choice of what may update itself,
//! and the window, so that it speaks the interface's language. Nothing here blocks a frame: the
//! check (and detecting how Strix was installed, which may ask the package manager) and the
//! installation run on threads.
//!
//! Where the installation can replace itself, a newer release is downloaded and verified in the
//! background as soon as it is found (unless that's turned off in the window), and installed
//! silently when Strix next starts, before its window opens ([`apply_staged_at_start`],
//! `staging`). "Restart Now" closes Strix through the normal close (which asks about unsaved
//! work) and installs it on the way out ([`finish_at_exit`]).

mod staging;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::time::Duration;

use craft_update::self_update::{self, Install};
use craft_update::{App, Http, Paths, Platform, Release, Stop, check_for_update, pick_asset};
use egui::RichText;

use crate::pane_kit;
use crate::theme::Tokens;

/// Strix as its releases know it: assets `strix-<version>-<platform>…` in nabre/strix. `name` is
/// the folder the Windows installer uses per user (`%LOCALAPPDATA%\Programs\Strix`).
pub const STRIX: App = App {
    icon_png: Some(include_bytes!("../../../assets/app-icon/hicolor/128x128/apps/io.github.nabre.strix.png")),
    ..App::new("strix", "Strix", "Writing, calculations and exercises", "nabre/strix")
};

/// Longest part of the release notes shown.
const NOTES_CHARS: usize = 1500;

/// What a requested check came to, for the window that answers Help › Check for Updates.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Status {
    #[default]
    Idle,
    Checking,
    UpToDate,
    Failed(String),
}

/// What the app should do after [`Updater::show`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    None,
    OpenUrl(String),
    /// Close through the normal close path: the update is in place and restarts Strix, or (Restart
    /// Now) is installed as Strix quits.
    Close,
}

/// How an offered release gets onto this computer.
#[derive(Clone, Debug)]
enum How {
    /// This installation replaces itself.
    Install(Install),
    /// The package for this platform (or the release page), opened in the browser.
    Download(String),
}

type Found = Result<Option<(Release, How)>, String>;

/// Stands in for GitHub in tests.
type CheckSource = Arc<dyn Fn() -> Found + Send + Sync>;

/// Downloads and verifies a release ahead of time for the next start; true when it then installs
/// without any question. Stands in for the download in tests.
type StageSource = Arc<dyn Fn(&Install, &Release, &str, &Stop) -> Result<bool, String> + Send + Sync>;

/// Where the download ahead of time of an offered release stands.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Staging {
    /// Not downloaded ahead (turned off, or the installation can't replace itself).
    Off,
    Downloading,
    /// Downloaded and verified; `silent`: installed at the next start without a question.
    Ready {
        silent: bool,
    },
    /// The download failed; the window offers the update as it would without it.
    Failed,
}

struct Offer {
    release: Release,
    how: How,
    message: Option<String>,
    installing: bool,
    staging: Staging,
}

/// "Restart Now" was chosen: the staged update is installed as Strix quits.
static RESTART_AT_EXIT: AtomicBool = AtomicBool::new(false);

#[derive(Default)]
pub struct Updater {
    version: String,
    source: Option<CheckSource>,
    status: Status,
    requested: bool,
    started: bool,
    check: Option<Receiver<Found>>,
    offer: Option<Offer>,
    install: Option<Receiver<Result<(), String>>>,
    /// Installed: Strix closes (and restarts) once it holds no unsaved work.
    installed: bool,
    stager: Option<StageSource>,
    /// Whether a newer release downloads by itself; read from the settings when first needed.
    auto_download: Option<bool>,
    /// The download ahead of time under way, for the release with this tag.
    stage: Option<(String, Receiver<Result<bool, String>>)>,
    stop: Arc<Stop>,
    /// The release (tag) downloaded ahead of time, and whether it installs silently.
    ready: Option<(String, bool)>,
    /// Restart Now: installed as Strix quits.
    restart: bool,
}

impl Updater {
    /// `version`: the running one, unless `STRIX_PRETEND_VERSION` says otherwise (docs/updates.md,
    /// Testing).
    pub fn new(version: impl Into<String>) -> Self {
        let version = version.into();
        let version = self_update::current_version(&STRIX, &version).map_or(version, |v| v.to_string());
        Self { version, ..Self::default() }
    }

    #[cfg(test)]
    fn with_source(mut self, source: CheckSource) -> Self {
        self.source = Some(source);
        self
    }

    #[cfg(test)]
    fn with_stager(mut self, stager: StageSource, auto_download: bool) -> Self {
        self.stager = Some(stager);
        self.auto_download = Some(auto_download);
        self
    }

    /// Whether a newer release downloads by itself (on unless turned off in the window).
    fn auto_download(&mut self) -> bool {
        *self.auto_download.get_or_insert_with(|| Paths::for_app(&STRIX).is_ok_and(|p| staging::auto_download(&p.data)))
    }

    /// The offer window's checkbox. Off cancels a download under way and deletes a staged package.
    fn set_auto_download(&mut self, ctx: &egui::Context, on: bool) {
        self.auto_download = Some(on);
        if self.stager.is_none()
            && let Ok(paths) = Paths::for_app(&STRIX)
        {
            if let Err(e) = staging::set_auto_download(&paths.data, on) {
                log::warn!("Strix update: couldn't save the automatic download setting: {e}");
            }
            if !on {
                staging::discard(&paths);
            }
        }
        if on {
            self.start_stage(ctx);
        } else {
            self.stop.cancel();
            self.stage = None;
            self.ready = None;
            self.set_restart(false);
            if let Some(offer) = &mut self.offer {
                offer.staging = Staging::Off;
            }
        }
    }

    fn set_restart(&mut self, on: bool) {
        self.restart = on;
        // Only the app's own updater speaks for the process; a test's never does.
        if self.stager.is_none() {
            RESTART_AT_EXIT.store(on, Ordering::SeqCst);
        }
    }

    /// Starts downloading the offered release ahead of time, when it installs by itself and the
    /// setting allows it.
    fn start_stage(&mut self, ctx: &egui::Context) {
        if !self.auto_download() {
            return;
        }
        let Some(offer) = &mut self.offer else { return };
        let How::Install(install) = offer.how.clone() else { return };
        let tag = offer.release.tag.clone();
        if let Some((ready, silent)) = &self.ready
            && *ready == tag
        {
            offer.staging = Staging::Ready { silent: *silent };
            return;
        }
        if self.stage.as_ref().is_some_and(|(t, _)| *t == tag) {
            offer.staging = Staging::Downloading;
            return;
        }
        offer.staging = Staging::Downloading;
        // A fresh stop: an earlier download may have been cancelled.
        self.stop = Arc::new(Stop::default());
        let (tx, rx) = channel();
        let (stager, release, version, stop, ctx) =
            (self.stager.clone(), offer.release.clone(), self.version.clone(), self.stop.clone(), ctx.clone());
        let spawned = std::thread::Builder::new().name("update-download".into()).spawn(move || {
            let run = || match &stager {
                Some(stage) => stage(&install, &release, &version, &stop),
                None => staging::stage(&STRIX, &install, &release, &version, &stop).map(|_| staging::silent(&install)),
            };
            let outcome =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(run)).unwrap_or_else(|_| Err("the download failed unexpectedly".into()));
            let _ = tx.send(outcome);
            ctx.request_repaint();
        });
        match spawned {
            Ok(_) => self.stage = Some((tag, rx)),
            Err(e) => self.finish_stage(&tag, Err(e.to_string())),
        }
    }

    fn finish_stage(&mut self, tag: &str, outcome: Result<bool, String>) {
        let staging = match outcome {
            Ok(silent) => {
                log::info!("Strix update: {tag} downloaded and verified{}", if silent { "; it installs at the next start" } else { "" });
                self.ready = Some((tag.to_string(), silent));
                Staging::Ready { silent }
            }
            Err(e) => {
                // Turned off meanwhile: the cancelled download says nothing.
                if self.auto_download == Some(false) {
                    Staging::Off
                } else {
                    log::warn!("Strix update: downloading {tag} failed: {e}");
                    Staging::Failed
                }
            }
        };
        if let Some(offer) = &mut self.offer
            && offer.release.tag == tag
        {
            offer.staging = staging;
        }
    }

    /// The check at start, once.
    pub fn check_at_start(&mut self, ctx: &egui::Context) {
        if !std::mem::replace(&mut self.started, true) {
            self.begin(ctx, false);
        }
    }

    /// Help › Check for Updates: [`Self::status`] reports the outcome.
    pub fn check_now(&mut self, ctx: &egui::Context) {
        self.started = true;
        self.begin(ctx, true);
    }

    pub fn status(&self) -> &Status {
        &self.status
    }

    pub fn dismiss_status(&mut self) {
        self.requested = false;
        self.status = Status::Idle;
    }

    fn begin(&mut self, ctx: &egui::Context, requested: bool) {
        if self.install.is_some() || self.installed || self.restart {
            return;
        }
        if requested {
            self.requested = true;
            self.status = Status::Checking;
        }
        if self.check.is_some() {
            return;
        }
        let (tx, rx) = channel();
        let (source, version, waker) = (self.source.clone(), self.version.clone(), ctx.clone());
        let spawned = std::thread::Builder::new().name("update-check".into()).spawn(move || {
            let ask = || match &source {
                Some(ask) => ask(),
                None => find(&version),
            };
            let answer =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(ask)).unwrap_or_else(|_| Err("the update check failed unexpectedly".into()));
            let _ = tx.send(answer);
            waker.request_repaint();
        });
        match spawned {
            Ok(_) => self.check = Some(rx),
            Err(e) => self.finish_check(ctx, Err(e.to_string())),
        }
    }

    fn finish_check(&mut self, ctx: &egui::Context, answer: Found) {
        let requested = std::mem::take(&mut self.requested);
        match answer {
            Ok(Some((release, how))) => {
                self.status = Status::Idle;
                if self.offer.is_none() && !self.installed {
                    self.offer = Some(Offer { release, how, message: None, installing: false, staging: Staging::Off });
                    self.start_stage(ctx);
                }
            }
            Ok(None) => self.status = if requested { Status::UpToDate } else { Status::Idle },
            Err(e) => {
                log::warn!("Strix update check failed: {e}");
                self.status = if requested { Status::Failed(e) } else { Status::Idle };
            }
        }
    }

    /// Picks up a finished check or installation; call once per frame before [`Self::show`].
    pub fn tick(&mut self, ctx: &egui::Context) {
        if let Some(rx) = &self.check {
            match rx.try_recv() {
                Ok(answer) => {
                    self.check = None;
                    self.finish_check(ctx, answer);
                }
                Err(TryRecvError::Empty) => ctx.request_repaint_after(Duration::from_millis(100)),
                Err(TryRecvError::Disconnected) => {
                    self.check = None;
                    self.finish_check(ctx, Err("the update check stopped unexpectedly".into()));
                }
            }
        }
        if let Some((tag, rx)) = &self.stage {
            let outcome = match rx.try_recv() {
                Ok(outcome) => Some(outcome),
                // The download may take minutes: a slow poll is enough.
                Err(TryRecvError::Empty) => {
                    ctx.request_repaint_after(Duration::from_millis(500));
                    None
                }
                Err(TryRecvError::Disconnected) => Some(Err("the download stopped unexpectedly".into())),
            };
            if let Some(outcome) = outcome {
                let tag = tag.clone();
                self.stage = None;
                self.finish_stage(&tag, outcome);
            }
        }
        if let Some(rx) = &self.install {
            let outcome = match rx.try_recv() {
                Ok(outcome) => Some(outcome),
                Err(TryRecvError::Empty) => {
                    ctx.request_repaint_after(Duration::from_millis(100));
                    None
                }
                Err(TryRecvError::Disconnected) => Some(Err("the update stopped unexpectedly".into())),
            };
            if let Some(outcome) = outcome {
                self.install = None;
                if let Some(offer) = &mut self.offer {
                    offer.installing = false;
                }
                match outcome {
                    Ok(()) => self.installed = true,
                    Err(e) => {
                        log::warn!("Strix self-update failed: {e}");
                        if let Some(offer) = &mut self.offer {
                            offer.message = Some(crate::i18n::fmt(tl!("The update couldn't be installed: {error}"), &[("error", &e)]));
                        }
                    }
                }
            }
        }
    }

    /// The offer window. `clean`: no unsaved work, so Strix may close to restart.
    pub fn show(&mut self, ctx: &egui::Context, clean: bool) -> Action {
        if self.installed && clean {
            self.installed = false;
            self.offer = None;
            return Action::Close;
        }
        let auto_download = match &self.offer {
            Some(offer) if matches!(offer.how, How::Install(_)) => Some(self.auto_download()),
            _ => None,
        };
        let restart_pending = self.restart;
        let Some(offer) = &mut self.offer else { return Action::None };
        let t = Tokens::get(ctx);
        let mut action = Action::None;
        let mut start = false;
        let mut later = false;
        let mut restart = None;
        let mut auto = auto_download;
        let mut open = true;
        let installed = self.installed;
        egui::Window::new(tl!("Software Update"))
            .open(&mut open)
            .resizable(false)
            .collapsible(false)
            .default_width(420.0)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.set_max_width(420.0);
                let new = offer.release.version_label();
                ui.label(RichText::new(crate::i18n::fmt(tl!("Strix Writer {version} is available"), &[("version", &new)])).size(15.0).strong());
                pane_kit::note(ui, &crate::i18n::fmt(tl!("You have version {version}."), &[("version", &self.version)]));
                let notes = notes(&offer.release.notes);
                if !notes.is_empty() {
                    pane_kit::section(ui, "", tl!("What's new"));
                    egui::ScrollArea::vertical().max_height(160.0).show(ui, |ui| {
                        ui.label(RichText::new(notes).size(11.5).color(t.text_dim));
                    });
                }
                ui.hyperlink_to(tl!("Release notes"), &offer.release.html_url);
                ui.add_space(6.0);
                if installed {
                    pane_kit::badge(ui, tl!("installed"), pane_kit::Status::Ok);
                    pane_kit::note(ui, tl!("Save your documents: Strix Writer then closes and restarts in the new version."));
                } else if offer.installing {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(tl!("Downloading and installing…"));
                    });
                } else if let How::Download(_) = offer.how {
                    pane_kit::note(ui, tl!("This copy of Strix Writer can't replace itself: download the new version and install it over this one."));
                } else {
                    match offer.staging {
                        Staging::Downloading => {
                            ui.horizontal(|ui| {
                                ui.spinner();
                                ui.label(tl!("Downloading in the background…"));
                            });
                            pane_kit::note(ui, tl!("It will be installed when you restart Strix Writer."));
                        }
                        Staging::Ready { silent: true } if restart_pending => {
                            pane_kit::badge(ui, tl!("downloaded"), pane_kit::Status::Ok);
                            pane_kit::note(ui, tl!("Strix Writer restarts in the new version as soon as it closes."));
                        }
                        Staging::Ready { silent: true } => {
                            pane_kit::badge(ui, tl!("downloaded"), pane_kit::Status::Ok);
                            pane_kit::note(ui, tl!("It will be installed when you restart Strix Writer."));
                        }
                        Staging::Ready { silent: false } => {
                            pane_kit::badge(ui, tl!("downloaded"), pane_kit::Status::Ok);
                            pane_kit::note(ui, tl!("Installing it asks for your password, then Strix Writer restarts."));
                        }
                        Staging::Failed => {
                            pane_kit::note(ui, tl!("The download in the background didn't succeed; Update and Restart tries again."));
                        }
                        Staging::Off => {}
                    }
                    if let Some(on) = &mut auto {
                        ui.checkbox(on, tl!("Download updates automatically"));
                    }
                }
                if let Some(m) = &offer.message {
                    ui.label(RichText::new(m).size(11.5).color(t.red));
                }
                ui.add_space(8.0);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if offer.installing || installed {
                        return;
                    }
                    match (&offer.how, &offer.staging) {
                        (How::Install(_), Staging::Ready { silent: true }) if restart_pending => {
                            if ui.button(tl!("Don't Restart")).clicked() {
                                restart = Some(false);
                            }
                            return;
                        }
                        (How::Install(_), Staging::Ready { silent: true }) => {
                            if ui.button(RichText::new(tl!("Restart Now")).strong()).clicked() {
                                restart = Some(true);
                            }
                        }
                        // Waiting for the download; Later leaves it running.
                        (How::Install(_), Staging::Downloading) => {}
                        (How::Install(_), _) => {
                            if ui.button(RichText::new(tl!("Update and Restart")).strong()).clicked() {
                                start = true;
                            }
                        }
                        (How::Download(url), _) => {
                            if ui.button(RichText::new(tl!("Download")).strong()).clicked() {
                                action = Action::OpenUrl(url.clone());
                                later = true;
                            }
                        }
                    }
                    if ui.button(tl!("Later")).clicked() {
                        later = true;
                    }
                });
            });
        if start {
            if clean {
                self.start_install(ctx);
            } else if let Some(offer) = &mut self.offer {
                offer.message = Some(tl!("Save or close your documents first: updating restarts Strix Writer.").to_string());
            }
        }
        if let Some(on) = auto
            && Some(on) != auto_download
        {
            self.set_auto_download(ctx, on);
        }
        match restart {
            // The normal close asks about unsaved work; the update is installed on the way out.
            Some(true) => {
                self.set_restart(true);
                return Action::Close;
            }
            Some(false) => self.set_restart(false),
            None => {}
        }
        // Closing the window while it installs lets the installation carry on and report back; a
        // download in the background carries on too, for the next start.
        if (later || !open) && self.install.is_none() && !self.installed && !self.restart {
            self.offer = None;
        }
        action
    }

    fn start_install(&mut self, ctx: &egui::Context) {
        let Some(offer) = &mut self.offer else { return };
        let How::Install(installed) = offer.how.clone() else { return };
        offer.installing = true;
        offer.message = None;
        let (tx, rx) = channel();
        let (release, version, ctx) = (offer.release.clone(), self.version.clone(), ctx.clone());
        let spawned = std::thread::Builder::new().name("self-update".into()).spawn(move || {
            let run = || install(&installed, &release, &version);
            let outcome =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(run)).unwrap_or_else(|_| Err("the update failed unexpectedly".into()));
            let _ = tx.send(outcome);
            ctx.request_repaint();
        });
        match spawned {
            Ok(_) => self.install = Some(rx),
            Err(e) => {
                offer.installing = false;
                offer.message = Some(crate::i18n::fmt(tl!("The update couldn't be installed: {error}"), &[("error", &e.to_string())]));
            }
        }
    }
}

/// At start, before the window opens: installs the update downloaded ahead of time, silently,
/// when this installation takes it without a question. `plain`: the start opens nothing in
/// particular (no files, sample or control port), since the new version starts without them.
/// True when the new version is in place and starts once this process exits: the caller exits
/// at once. Never fails the start (a problem is logged and the package discarded).
pub fn apply_staged_at_start(plain: bool) -> bool {
    staging::apply_at_start(plain)
}

/// After the event loop ends: when "Restart Now" closed Strix, installs the update downloaded
/// ahead of time, whose helper then starts the new version.
pub fn finish_at_exit() {
    if RESTART_AT_EXIT.swap(false, Ordering::SeqCst) {
        staging::apply_at_exit();
    }
}

/// The newest release newer than `version`, and how it would get here (runs on its thread).
fn find(version: &str) -> Found {
    let http = Http::new("Strix/Update")?;
    let Some(release) = check_for_update(&STRIX, &http, version)? else { return Ok(None) };
    // The version just installed, whose binary may still report the old one: offering it again
    // would loop.
    if Paths::for_app(&STRIX).is_ok_and(|p| self_update::just_updated_to(&p, &release)) {
        return Ok(None);
    }
    let how = match self_update::current(&STRIX).filter(replaces_itself) {
        Some(install) => How::Install(install),
        None => How::Download(download_url(&release, Platform::current())),
    };
    Ok(Some((release, how)))
}

/// Whether Strix may swap this installation for the new release. Strix's macOS bundle, Flatpak and
/// Windows portable copies use names of their own that `craft_update` doesn't replace, and a copy
/// outside the home folder on Linux belongs to whoever installed it there.
fn replaces_itself(install: &Install) -> bool {
    match install {
        Install::Msi(_) | Install::AppImage(_) | Install::Package(..) => true,
        Install::Standalone(exe) => {
            cfg!(target_os = "linux") && std::env::var_os("HOME").is_some_and(|home| !home.is_empty() && exe.starts_with(home))
        }
        Install::MacApp(_) | Install::Portable(_) | Install::Flatpak => false,
    }
}

/// The package for `platform` in `release`, or the release page.
fn download_url(release: &Release, platform: Option<Platform>) -> String {
    platform
        .and_then(|p| pick_asset(release, &[STRIX.id], p, p.default_kinds()))
        .map_or_else(|| release.html_url.clone(), |(asset, _)| asset.url.clone())
}

fn install(installed: &Install, release: &Release, version: &str) -> Result<(), String> {
    let http = Http::new("Strix/Update")?;
    let paths = Paths::for_app(&STRIX)?;
    let platform = Platform::current().ok_or("this platform doesn't update itself")?;
    self_update::update(&STRIX, installed, release, platform, &http, &paths, &Stop::default(), &mut |_| {}, &|| Ok(()), Some(version))
}

/// The release notes as plain lines: no Markdown headings or emphasis marks, links shortened to
/// their pull request number, at most [`NOTES_CHARS`] characters.
fn notes(markdown: &str) -> String {
    let mut out = String::new();
    for line in markdown.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("**Full Changelog**") || line.starts_with("## New Contributors") {
            continue;
        }
        let line = line.trim_start_matches('#').trim();
        let line = line.strip_prefix("* ").or_else(|| line.strip_prefix("- ")).map_or_else(|| line.to_string(), |rest| format!("• {rest}"));
        // "… by @someone in https://github.com/…/pull/123" → "… (#123)"
        let line = match line.rsplit_once(" in https://github.com/") {
            Some((text, url)) => match url.rsplit_once("/pull/") {
                Some((_, n)) if n.chars().all(|c| c.is_ascii_digit()) => {
                    let text = text.rsplit_once(" by @").map_or(text, |(t, _)| t);
                    format!("{text} (#{n})")
                }
                _ => line.clone(),
            },
            None => line,
        };
        out.push_str(&line.replace("**", ""));
        out.push('\n');
        if out.chars().count() > NOTES_CHARS {
            let cut: String = out.chars().take(NOTES_CHARS).collect();
            return format!("{}…", cut.trim_end());
        }
    }
    out.trim_end().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(tag: &str, assets: &[&str]) -> Release {
        Release {
            tag: tag.into(),
            version: craft_update::parse_version(tag.trim_start_matches('v')),
            html_url: format!("https://github.com/nabre/strix/releases/tag/{tag}"),
            assets: assets.iter().map(|n| craft_update::Asset { name: (*n).into(), size: 1, url: format!("https://x/{n}"), sha256: None }).collect(),
            ..Release::default()
        }
    }

    fn settle(u: &mut Updater, ctx: &egui::Context) {
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while (u.check.is_some() || u.stage.is_some()) && std::time::Instant::now() < deadline {
            u.tick(ctx);
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn a_requested_check_reports_up_to_date_or_the_failure() {
        let ctx = egui::Context::default();
        let mut u = Updater::new("0.18.0").with_source(Arc::new(|| Ok(None)));
        u.check_now(&ctx);
        assert_eq!(u.status(), &Status::Checking);
        settle(&mut u, &ctx);
        assert_eq!(u.status(), &Status::UpToDate);
        let mut u = Updater::new("0.18.0").with_source(Arc::new(|| Err("offline".into())));
        u.check_now(&ctx);
        settle(&mut u, &ctx);
        assert_eq!(u.status(), &Status::Failed("offline".into()));
        // The check at start reports nothing when nothing's new.
        let mut u = Updater::new("0.18.0").with_source(Arc::new(|| Err("offline".into())));
        u.check_at_start(&ctx);
        settle(&mut u, &ctx);
        assert_eq!(u.status(), &Status::Idle);
        assert!(u.offer.is_none());
    }

    #[test]
    fn a_newer_release_is_offered_and_an_installed_one_closes_when_clean() {
        let ctx = egui::Context::default();
        let r = release("v0.19.0", &[]);
        let how = How::Download(r.html_url.clone());
        let mut u = Updater::new("0.18.0").with_source(Arc::new(move || Ok(Some((r.clone(), how.clone())))));
        u.check_at_start(&ctx);
        settle(&mut u, &ctx);
        assert!(u.offer.is_some());
        assert_eq!(u.status(), &Status::Idle);
        // Installed: Strix closes (to restart) once nothing is unsaved, and nothing more starts.
        u.installed = true;
        u.check_now(&ctx);
        assert!(u.check.is_none());
        assert_eq!(u.show(&ctx, true), Action::Close);
        assert!(u.offer.is_none());
    }

    fn installable(tag: &'static str) -> CheckSource {
        Arc::new(move || Ok(Some((release(tag, &[]), How::Install(Install::AppImage("/home/u/Strix.AppImage".into()))))))
    }

    fn counting(calls: &Arc<std::sync::atomic::AtomicUsize>, outcome: Result<bool, String>) -> StageSource {
        let calls = calls.clone();
        Arc::new(move |_, _, _, _| {
            calls.fetch_add(1, Ordering::SeqCst);
            outcome.clone()
        })
    }

    #[test]
    fn an_installable_release_downloads_ahead_and_waits_for_the_restart() {
        let ctx = egui::Context::default();
        let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let mut u = Updater::new("0.21.0").with_source(installable("v0.22.0")).with_stager(counting(&calls, Ok(true)), true);
        u.check_at_start(&ctx);
        settle(&mut u, &ctx);
        assert_eq!(u.offer.as_ref().map(|o| o.staging.clone()), Some(Staging::Ready { silent: true }));
        assert_eq!(u.ready, Some(("v0.22.0".into(), true)));
        // Later hides the window; asking again shows the download that's there, not a new one.
        u.offer = None;
        u.check_now(&ctx);
        settle(&mut u, &ctx);
        assert_eq!(u.offer.as_ref().map(|o| o.staging.clone()), Some(Staging::Ready { silent: true }));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        // Restart Now waits for the close; no further check starts meanwhile, and Don't Restart
        // undoes it. A test's updater never speaks for the process.
        u.set_restart(true);
        u.check_now(&ctx);
        assert!(u.check.is_none());
        assert!(!RESTART_AT_EXIT.load(Ordering::SeqCst));
        u.set_restart(false);
        // Turning the automatic download off forgets the download.
        u.set_auto_download(&ctx, false);
        assert_eq!(u.offer.as_ref().map(|o| o.staging.clone()), Some(Staging::Off));
        assert_eq!(u.ready, None);
        // And on again downloads it again.
        u.set_auto_download(&ctx, true);
        settle(&mut u, &ctx);
        assert_eq!(u.offer.as_ref().map(|o| o.staging.clone()), Some(Staging::Ready { silent: true }));
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn without_the_automatic_download_or_when_it_fails_the_update_is_offered_as_before() {
        let ctx = egui::Context::default();
        let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let mut u = Updater::new("0.21.0").with_source(installable("v0.22.0")).with_stager(counting(&calls, Ok(true)), false);
        u.check_at_start(&ctx);
        settle(&mut u, &ctx);
        assert_eq!(u.offer.as_ref().map(|o| o.staging.clone()), Some(Staging::Off));
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        let mut u = Updater::new("0.21.0").with_source(installable("v0.22.0")).with_stager(counting(&calls, Err("offline".into())), true);
        u.check_at_start(&ctx);
        settle(&mut u, &ctx);
        assert_eq!(u.offer.as_ref().map(|o| o.staging.clone()), Some(Staging::Failed));
        assert_eq!(u.ready, None);
        // A package that asks for the password is downloaded, but not installed silently.
        let mut u = Updater::new("0.21.0").with_source(installable("v0.22.0")).with_stager(counting(&calls, Ok(false)), true);
        u.check_at_start(&ctx);
        settle(&mut u, &ctx);
        assert_eq!(u.offer.as_ref().map(|o| o.staging.clone()), Some(Staging::Ready { silent: false }));
        // A copy that can't replace itself downloads nothing ahead.
        let r = release("v0.22.0", &[]);
        let how = How::Download(r.html_url.clone());
        let before = calls.load(Ordering::SeqCst);
        let mut u =
            Updater::new("0.21.0").with_source(Arc::new(move || Ok(Some((r.clone(), how.clone()))))).with_stager(counting(&calls, Ok(true)), true);
        u.check_at_start(&ctx);
        settle(&mut u, &ctx);
        assert_eq!(u.offer.as_ref().map(|o| o.staging.clone()), Some(Staging::Off));
        assert_eq!(calls.load(Ordering::SeqCst), before);
    }

    #[test]
    fn downloads_pick_this_platforms_package() {
        use craft_update::{Arch, Os};
        let r = release(
            "v0.19.0",
            &["strix-0.19.0-macos-universal.dmg", "strix-0.19.0-windows-x64.msi", "strix-0.19.0-freebsd-x86_64.tar.gz", "strix-web-0.19.0.zip"],
        );
        let mac = Platform { os: Os::MacOs, arch: Arch::Arm64 };
        assert_eq!(download_url(&r, Some(mac)), "https://x/strix-0.19.0-macos-universal.dmg");
        let bsd = Platform { os: Os::FreeBsd, arch: Arch::X64 };
        assert_eq!(download_url(&r, Some(bsd)), "https://x/strix-0.19.0-freebsd-x86_64.tar.gz");
        assert_eq!(download_url(&r, None), r.html_url);
    }

    #[test]
    fn only_installations_strix_can_swap_update_themselves() {
        use std::path::PathBuf;
        assert!(replaces_itself(&Install::AppImage(PathBuf::from("/home/u/Strix.AppImage"))));
        assert!(replaces_itself(&Install::Msi(PathBuf::from("C:/x/strix.exe"))));
        assert!(!replaces_itself(&Install::MacApp(PathBuf::from("/Applications/Strix.app"))));
        assert!(!replaces_itself(&Install::Flatpak));
        assert!(!replaces_itself(&Install::Portable(PathBuf::from("D:/strix.exe"))));
        assert!(!replaces_itself(&Install::Standalone(PathBuf::from("/usr/local/bin/strix"))));
    }

    #[test]
    fn release_notes_read_as_plain_lines() {
        let md = "## What's Changed\n* Master documents by @nabre in https://github.com/nabre/strix/pull/29\n* **Bold** fix\n\n**Full Changelog**: https://x";
        assert_eq!(notes(md), "What's Changed\n• Master documents (#29)\n• Bold fix");
        let long = "x".repeat(5000);
        assert!(notes(&long).chars().count() <= NOTES_CHARS + 1);
    }
}

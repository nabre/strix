//! Updates for Strix Writer (docs/updates.md). Everything happens quietly, and everything is
//! shown in one place: the **Updates** section of File › Info ([`Updater::panel`]). Strix asks
//! GitHub for the newest published release of nabre/strix when it starts, again every day while it
//! stays open (an hour after a check that failed, such as one made offline), and on Help › Check
//! for Updates, which opens File › Info. No window ever pops up for it.
//!
//! Installations Strix can replace safely update themselves (the Windows Installer package
//! installed per user, the AppImage, the .deb/.rpm packages, a tar.gz unpacked in the home folder
//! on Linux): a newer release is downloaded and verified in the background as soon as it is found
//! (unless that's turned off), with its progress in File › Info, and installed silently when Strix
//! next starts, before its window opens ([`apply_staged_at_start`], `staging`). "Restart Now"
//! closes Strix through the normal close (which asks about unsaved work) and installs it on the
//! way out ([`finish_at_exit`]). Every other installation (macOS, Flatpak, FreeBSD, portable and
//! per-machine Windows copies) gets a Download button that opens its package in the browser.
//!
//! The release checking, downloading, verifying and swapping is `craft_update`'s (shared with the
//! Craft apps); Strix brings its own identity ([`STRIX`]), the choice of what may update itself,
//! and the panel, so that it speaks the interface's language. Nothing here blocks a frame: the
//! check (and detecting how Strix was installed, which may ask the package manager), the download
//! and the installation run on threads.

mod staging;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use craft_update::self_update::{self, Install};
use craft_update::{App, Http, JobPhase, Paths, Platform, Release, Stop, check_for_update, pick_asset};
use egui::RichText;

use crate::pane_kit;
use crate::theme::{Tokens, semibold};

/// Strix as its releases know it: assets `strix-<version>-<platform>…` in nabre/strix. `name` is
/// the folder the Windows installer uses per user (`%LOCALAPPDATA%\Programs\Strix`).
pub const STRIX: App = App {
    icon_png: Some(include_bytes!("../../../assets/app-icon/hicolor/128x128/apps/io.github.nabre.strix.png")),
    ..App::new("strix", "Strix", "Writing, calculations and exercises", "nabre/strix")
};

/// Longest part of the release notes shown.
const NOTES_CHARS: usize = 1500;

/// A Strix left open looks again this long after a check that answered.
const RECHECK: Duration = Duration::from_secs(24 * 60 * 60);

/// … and this long after one that failed (offline, GitHub unreachable).
const RETRY: Duration = Duration::from_secs(60 * 60);

/// What the last finished check came to, for File › Info.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Nothing newer than the running version.
    UpToDate,
    /// A newer release (it is the offer).
    Found,
    Failed(String),
}

/// What the app should do after [`Updater::panel`].
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

/// Downloads and verifies a release ahead of time for the next start, reporting how far it got;
/// true when it then installs without any question. Stands in for the download in tests.
type StageSource = Arc<dyn Fn(&Install, &Release, &str, &Stop, &Progress) -> Result<bool, String> + Send + Sync>;

/// How far the download in the background has come, written by its thread and read by the panel.
#[derive(Debug, Default)]
pub struct Progress {
    done: AtomicU64,
    total: AtomicU64,
    /// [`Progress::DOWNLOADING`], [`Progress::RECONNECTING`] or [`Progress::VERIFYING`].
    phase: AtomicU8,
}

impl Progress {
    const DOWNLOADING: u8 = 0;
    const RECONNECTING: u8 = 1;
    const VERIFYING: u8 = 2;

    fn hear(&self, phase: JobPhase) {
        let (p, done, total) = match phase {
            JobPhase::Downloading { done, total } => (Self::DOWNLOADING, done, total),
            JobPhase::Reconnecting { done, total } => (Self::RECONNECTING, done, total),
            JobPhase::Verifying => (Self::VERIFYING, self.total.load(Ordering::Relaxed), self.total.load(Ordering::Relaxed)),
            _ => return,
        };
        self.done.store(done.min(total), Ordering::Relaxed);
        self.total.store(total, Ordering::Relaxed);
        self.phase.store(p, Ordering::Relaxed);
    }

    fn reset(&self) {
        self.done.store(0, Ordering::Relaxed);
        self.total.store(0, Ordering::Relaxed);
        self.phase.store(Self::DOWNLOADING, Ordering::Relaxed);
    }

    /// (bytes done, bytes in all, phase); `total` is 0 until the size is known.
    fn read(&self) -> (u64, u64, u8) {
        (self.done.load(Ordering::Relaxed), self.total.load(Ordering::Relaxed), self.phase.load(Ordering::Relaxed))
    }
}

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
    /// The download failed; the panel offers the update as it would without it.
    Failed(String),
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
    /// The checks have begun (at start, or asked for): the daily ones follow.
    started: bool,
    check: Option<Receiver<Found>>,
    /// When the last check finished, as a monotonic instant (for the next one) and as Unix time
    /// (to show), and what it came to.
    last: Option<(Instant, i64, Outcome)>,
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
    progress: Arc<Progress>,
    /// The release (tag) downloaded ahead of time, and whether it installs silently.
    ready: Option<(String, bool)>,
    /// Restart Now: installed as Strix quits.
    restart: bool,
    /// Whether this copy can replace itself; worked out (on the check's thread) with the first
    /// answer, so the panel says what the automatic download means here.
    self_updating: Option<bool>,
}

/// Seconds since the Unix epoch, now.
fn unix_now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}

/// `secs` (Unix time) as the panel shows it: the time of day when it is today, else the date and
/// time, in local time.
fn when(secs: i64, now: i64) -> String {
    use chrono::{Local, TimeZone};
    let (Some(t), Some(n)) = (Local.timestamp_opt(secs, 0).earliest(), Local.timestamp_opt(now, 0).earliest()) else { return String::new() };
    if t.date_naive() == n.date_naive() { t.format("%H:%M").to_string() } else { t.format("%Y-%m-%d %H:%M").to_string() }
}

/// A size in MB with one decimal, as the progress line shows it.
fn megabytes(bytes: u64) -> String {
    format!("{:.1}", bytes as f64 / 1_048_576.0)
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

    /// Whether a newer release downloads by itself (on unless turned off).
    fn auto_download(&mut self) -> bool {
        *self.auto_download.get_or_insert_with(|| Paths::for_app(&STRIX).is_ok_and(|p| staging::auto_download(&p.data)))
    }

    /// The panel's checkbox. Off cancels a download under way and deletes a staged package.
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
    /// setting allows it. A download under way for an older offer is cancelled.
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
        // A download for another release makes way; then a fresh stop, as an earlier download may
        // have been cancelled.
        self.stop.cancel();
        self.stop = Arc::new(Stop::default());
        self.progress.reset();
        let (tx, rx) = channel();
        let (stager, release, version, stop, progress, ctx) =
            (self.stager.clone(), offer.release.clone(), self.version.clone(), self.stop.clone(), self.progress.clone(), ctx.clone());
        let spawned = std::thread::Builder::new().name("update-download".into()).spawn(move || {
            let run = || match &stager {
                Some(stage) => stage(&install, &release, &version, &stop, &progress),
                None => {
                    let mut hear = |phase: JobPhase| {
                        progress.hear(phase);
                        ctx.request_repaint();
                    };
                    staging::stage(&STRIX, &install, &release, &version, &stop, &mut hear).map(|_| staging::silent(&install))
                }
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
                    Staging::Failed(e)
                }
            }
        };
        if let Some(offer) = &mut self.offer
            && offer.release.tag == tag
        {
            offer.staging = staging;
        }
    }

    /// The check at start, once; the daily checks follow from [`Self::tick`].
    pub fn check_at_start(&mut self, ctx: &egui::Context) {
        if !std::mem::replace(&mut self.started, true) {
            self.begin(ctx);
        }
    }

    /// Help › Check for Updates and the panel's button: File › Info shows the outcome.
    pub fn check_now(&mut self, ctx: &egui::Context) {
        self.started = true;
        self.begin(ctx);
    }

    /// Whether a check is under way.
    pub fn checking(&self) -> bool {
        self.check.is_some()
    }

    /// What the last finished check came to.
    pub fn outcome(&self) -> Option<&Outcome> {
        self.last.as_ref().map(|(_, _, o)| o)
    }

    /// Nothing more is checked once an update is installing, installed or waiting for the close.
    fn busy(&self) -> bool {
        self.install.is_some() || self.installed || self.restart
    }

    fn begin(&mut self, ctx: &egui::Context) {
        if self.busy() || self.check.is_some() {
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
            Ok(_) => {
                self.check = Some(rx);
                ctx.request_repaint();
            }
            Err(e) => self.finish_check(ctx, Err(e.to_string())),
        }
    }

    fn finish_check(&mut self, ctx: &egui::Context, answer: Found) {
        let outcome = match answer {
            Ok(Some((release, how))) => {
                self.self_updating = Some(matches!(how, How::Install(_)));
                // A newer release than the one offered replaces it, unless that one is already
                // being installed.
                let replace = self.offer.as_ref().is_none_or(|o| o.release.tag != release.tag && !o.installing);
                if replace && !self.busy() {
                    self.offer = Some(Offer { release, how, message: None, installing: false, staging: Staging::Off });
                    self.start_stage(ctx);
                }
                Outcome::Found
            }
            Ok(None) => Outcome::UpToDate,
            Err(e) => {
                log::warn!("Strix update check failed: {e}");
                Outcome::Failed(e)
            }
        };
        self.last = Some((Instant::now(), unix_now(), outcome));
    }

    /// Whether the next check is due at `now`, after the last one.
    fn due(&self, now: Instant) -> bool {
        match &self.last {
            Some((at, _, Outcome::Failed(_))) => now.saturating_duration_since(*at) >= RETRY,
            Some((at, _, _)) => now.saturating_duration_since(*at) >= RECHECK,
            None => false,
        }
    }

    /// Picks up a finished check, download or installation, and starts the daily check when it is
    /// due; call once per frame.
    pub fn tick(&mut self, ctx: &egui::Context) {
        if self.started && self.check.is_none() && self.due(Instant::now()) {
            self.begin(ctx);
        }
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
        } else if self.started {
            // Wake up for the next daily check even when nothing else repaints.
            ctx.request_repaint_after(RETRY);
        }
        if let Some((tag, rx)) = &self.stage {
            let outcome = match rx.try_recv() {
                Ok(outcome) => Some(outcome),
                // The download may take minutes; its thread wakes the panel as bytes arrive.
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

    /// Once an update is installed and no unsaved work is left (`clean`), Strix closes so that it
    /// restarts in the new version; call once per frame.
    pub fn close_when_installed(&mut self, clean: bool) -> bool {
        if self.installed && clean {
            self.installed = false;
            self.offer = None;
            return true;
        }
        false
    }

    /// The Updates section of File › Info. `clean`: no unsaved work, so Strix may close to restart.
    pub fn panel(&mut self, ui: &mut egui::Ui, clean: bool) -> Action {
        let t = Tokens::get(ui.ctx());
        let mut action = Action::None;
        let mut start = false;
        let mut restart = None;
        let mut check = false;
        let auto_before = self.auto_download();
        let mut auto = auto_before;
        let restart_pending = self.restart;
        let installed = self.installed;
        let busy = self.busy();
        let checking = self.check.is_some();
        let now = unix_now();
        let (done, total, phase) = self.progress.read();

        ui.label(RichText::new(tl!("Updates")).font(semibold(15.0)));
        pane_kit::note(ui, &crate::i18n::fmt(tl!("Strix Writer {version}"), &[("version", &self.version)]));
        ui.add_space(4.0);
        if let Some(offer) = &mut self.offer {
            let new = offer.release.version_label();
            ui.label(RichText::new(crate::i18n::fmt(tl!("Strix Writer {version} is available"), &[("version", &new)])).strong());
            if let Some(published) = offer.release.published_at {
                pane_kit::note(ui, &crate::i18n::fmt(tl!("Published {date}"), &[("date", &when(published, now))]));
            }
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
                match &offer.staging {
                    Staging::Downloading => {
                        let text = match phase {
                            Progress::VERIFYING => tl!("Verifying the download…").to_string(),
                            Progress::RECONNECTING => tl!("The connection dropped; trying again shortly…").to_string(),
                            _ if total > 0 => crate::i18n::fmt(
                                tl!("Downloading in the background: {done} of {total} MB"),
                                &[("done", &megabytes(done)), ("total", &megabytes(total))],
                            ),
                            _ => tl!("Downloading in the background…").to_string(),
                        };
                        let fraction = if total > 0 { (done as f32 / total as f32).clamp(0.0, 1.0) } else { 0.0 };
                        ui.add(egui::ProgressBar::new(fraction).desired_width(320.0).animate(total == 0 || phase != Progress::DOWNLOADING));
                        pane_kit::note(ui, &text);
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
                    Staging::Failed(e) => {
                        pane_kit::note(ui, tl!("The download in the background didn't succeed; Update and Restart tries again."));
                        ui.label(RichText::new(e).size(11.0).color(t.text_dim));
                    }
                    Staging::Off => {}
                }
            }
            if let Some(m) = &offer.message {
                ui.label(RichText::new(m).size(11.5).color(t.red));
            }
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if offer.installing || installed {
                    return;
                }
                match (&offer.how, &offer.staging) {
                    (How::Install(_), Staging::Ready { silent: true }) if restart_pending => {
                        if ui.button(tl!("Don't Restart")).clicked() {
                            restart = Some(false);
                        }
                    }
                    (How::Install(_), Staging::Ready { silent: true }) => {
                        if ui.button(RichText::new(tl!("Restart Now")).strong()).clicked() {
                            restart = Some(true);
                        }
                    }
                    // Waiting for the download.
                    (How::Install(_), Staging::Downloading) => {}
                    (How::Install(_), _) => {
                        if ui.button(RichText::new(tl!("Update and Restart")).strong()).clicked() {
                            start = true;
                        }
                    }
                    (How::Download(url), _) => {
                        if ui.button(RichText::new(tl!("Download")).strong()).clicked() {
                            action = Action::OpenUrl(url.clone());
                        }
                    }
                }
                ui.hyperlink_to(tl!("Release notes"), &offer.release.html_url);
            });
            let notes = notes(&offer.release.notes);
            if !notes.is_empty() {
                egui::CollapsingHeader::new(tl!("What's new")).id_salt("update-notes").show(ui, |ui| {
                    egui::ScrollArea::vertical().max_height(160.0).show(ui, |ui| {
                        ui.label(RichText::new(notes).size(11.5).color(t.text_dim));
                    });
                });
            }
        } else if checking {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(tl!("Checking for updates…"));
            });
        } else {
            match &self.last {
                Some((_, at, Outcome::Failed(e))) => {
                    ui.label(tl!("Could not check for updates."));
                    ui.label(RichText::new(e).size(11.0).color(t.text_dim));
                    pane_kit::note(
                        ui,
                        &crate::i18n::fmt(tl!("Last attempt: {time}. Strix Writer tries again within an hour."), &[("time", &when(*at, now))]),
                    );
                }
                Some((_, at, _)) => {
                    ui.label(tl!("You have the newest version."));
                    pane_kit::note(ui, &crate::i18n::fmt(tl!("Last checked: {time}"), &[("time", &when(*at, now))]));
                }
                None => pane_kit::note(ui, tl!("Strix Writer looks for a newer version when it starts and once a day.")),
            }
        }

        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui.add_enabled(!checking && !busy, egui::Button::new(tl!("Check for Updates"))).clicked() {
                check = true;
            }
        });
        // Only copies that replace themselves download ahead; elsewhere the setting does nothing.
        if self.self_updating != Some(false) {
            ui.checkbox(&mut auto, tl!("Download updates automatically")).on_hover_text(tl!(
                "A newer version is downloaded in the background and installed, without any question, the next time Strix Writer starts."
            ));
        }

        if check {
            self.check_now(ui.ctx());
        }
        if start {
            if clean {
                self.start_install(ui.ctx());
            } else if let Some(offer) = &mut self.offer {
                offer.message = Some(tl!("Save or close your documents first: updating restarts Strix Writer.").to_string());
            }
        }
        if auto != auto_before {
            self.set_auto_download(ui.ctx(), auto);
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
    fn a_check_reports_up_to_date_or_the_failure() {
        let ctx = egui::Context::default();
        let mut u = Updater::new("0.18.0").with_source(Arc::new(|| Ok(None)));
        u.check_now(&ctx);
        assert!(u.checking());
        settle(&mut u, &ctx);
        assert_eq!(u.outcome(), Some(&Outcome::UpToDate));
        let mut u = Updater::new("0.18.0").with_source(Arc::new(|| Err("offline".into())));
        u.check_at_start(&ctx);
        settle(&mut u, &ctx);
        assert_eq!(u.outcome(), Some(&Outcome::Failed("offline".into())));
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
        assert_eq!(u.outcome(), Some(&Outcome::Found));
        assert_eq!(u.self_updating, Some(false));
        // Installed: Strix closes (to restart) once nothing is unsaved, and nothing more starts.
        u.installed = true;
        u.check_now(&ctx);
        assert!(u.check.is_none());
        assert!(!u.close_when_installed(false));
        assert!(u.close_when_installed(true));
        assert!(u.offer.is_none());
    }

    /// Strix left open looks again a day after a check that answered and an hour after one that
    /// failed; a newer release found then replaces the one offered.
    #[test]
    fn checks_repeat_daily_and_a_newer_release_replaces_the_offer() {
        let ctx = egui::Context::default();
        let tag = Arc::new(std::sync::Mutex::new("v0.22.0"));
        let t = tag.clone();
        let mut u = Updater::new("0.21.0").with_source(Arc::new(move || {
            let tag = t.lock().map(|g| *g).unwrap_or("v0.22.0");
            Ok(Some((release(tag, &[]), How::Download(String::new()))))
        }));
        // No check before the first one.
        u.tick(&ctx);
        assert!(!u.checking());
        u.check_at_start(&ctx);
        settle(&mut u, &ctx);
        assert_eq!(u.offer.as_ref().map(|o| o.release.tag.as_str()), Some("v0.22.0"));
        // Not due yet.
        u.tick(&ctx);
        assert!(!u.checking());
        // A day later the check is due, runs, and finds v0.23.0.
        let now = Instant::now();
        assert!(!u.due(now + RECHECK / 2));
        assert!(u.due(now + RECHECK));
        *tag.lock().unwrap() = "v0.23.0";
        u.begin(&ctx);
        assert!(u.checking());
        settle(&mut u, &ctx);
        assert_eq!(u.offer.as_ref().map(|o| o.release.tag.as_str()), Some("v0.23.0"));
        // After a failure, an hour is enough.
        let now = Instant::now();
        u.last = Some((now, 0, Outcome::Failed("offline".into())));
        assert!(u.due(now + RETRY));
        u.last = Some((now, 0, Outcome::UpToDate));
        assert!(!u.due(now + RETRY));
    }

    /// What File › Info shows for an update: its state, the progress of the download and the
    /// buttons, all in the panel (no window).
    #[test]
    fn the_info_panel_shows_each_state() {
        fn drawn(u: &mut Updater) -> Vec<String> {
            let ctx = egui::Context::default();
            let mut out = Vec::new();
            for _ in 0..2 {
                let mut o = ctx.run_ui(egui::RawInput::default(), |ui| {
                    u.panel(ui, true);
                });
                o.textures_delta.clear();
                out = o
                    .shapes
                    .iter()
                    .filter_map(|s| match &s.shape {
                        egui::Shape::Text(t) => Some(t.galley.text().to_string()),
                        _ => None,
                    })
                    .collect();
            }
            out
        }
        let has = |texts: &[String], s: &str| texts.iter().any(|t| t.contains(s));
        let mut u = Updater::new("0.21.0").with_stager(counting(&Arc::default(), Ok(true)), true);
        let texts = drawn(&mut u);
        assert!(has(&texts, "once a day") && has(&texts, "Check for Updates"), "{texts:?}");
        u.last = Some((Instant::now(), unix_now(), Outcome::UpToDate));
        assert!(has(&drawn(&mut u), "You have the newest version."));
        u.last = Some((Instant::now(), unix_now(), Outcome::Failed("offline".into())));
        let texts = drawn(&mut u);
        assert!(has(&texts, "Could not check for updates.") && has(&texts, "offline"), "{texts:?}");
        // Downloading: the bytes so far.
        let mut r = release("v0.22.0", &[]);
        r.notes = "* Faster".into();
        u.offer = Some(Offer {
            release: r,
            how: How::Install(Install::AppImage("/home/u/Strix.AppImage".into())),
            message: None,
            installing: false,
            staging: Staging::Downloading,
        });
        u.progress.hear(JobPhase::Downloading { done: 1_048_576, total: 4 * 1_048_576 });
        let texts = drawn(&mut u);
        assert!(has(&texts, "0.22.0 is available") && has(&texts, "1.0 of 4.0 MB") && has(&texts, "What's new"), "{texts:?}");
        u.progress.hear(JobPhase::Verifying);
        assert!(has(&drawn(&mut u), "Verifying"));
        if let Some(o) = &mut u.offer {
            o.staging = Staging::Ready { silent: true };
        }
        assert!(has(&drawn(&mut u), "Restart Now"));
        if let Some(o) = &mut u.offer {
            o.staging = Staging::Failed("disk full".into());
        }
        let texts = drawn(&mut u);
        assert!(has(&texts, "Update and Restart") && has(&texts, "disk full"), "{texts:?}");
        // A copy that can't replace itself: Download, and no automatic download setting.
        if let Some(o) = &mut u.offer {
            o.how = How::Download("https://x/strix.dmg".into());
        }
        u.self_updating = Some(false);
        let texts = drawn(&mut u);
        assert!(has(&texts, "Download") && !has(&texts, "Download updates automatically"), "{texts:?}");
    }

    fn installable(tag: &'static str) -> CheckSource {
        Arc::new(move || Ok(Some((release(tag, &[]), How::Install(Install::AppImage("/home/u/Strix.AppImage".into()))))))
    }

    fn counting(calls: &Arc<std::sync::atomic::AtomicUsize>, outcome: Result<bool, String>) -> StageSource {
        let calls = calls.clone();
        Arc::new(move |_, _, _, _, _| {
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
        // Asking again shows the download that's there, not a new one.
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
        assert_eq!(u.offer.as_ref().map(|o| o.staging.clone()), Some(Staging::Failed("offline".into())));
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

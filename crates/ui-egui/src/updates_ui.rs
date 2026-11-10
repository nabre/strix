// Modified by the Strix contributors (fork of WordCraft); see NOTICE.
//! Software updates: the quiet check at start (and daily after it), and the Updates section of
//! File › Info that shows everything about them. Help › Check for Updates opens that section.

use crate::WordApp;

// Strix's own releases (nabre/strix), shown in File › Info in the interface's language: `strix_update`.
#[cfg(not(target_arch = "wasm32"))]
use crate::strix_update::{Action, Updater};

pub(crate) struct Runtime {
    #[cfg(not(target_arch = "wasm32"))]
    updater: Updater,
    /// The desktop app turns the check at start on; tests and the web build leave it off.
    #[cfg(not(target_arch = "wasm32"))]
    startup_check: bool,
    #[cfg(not(target_arch = "wasm32"))]
    check_requested: bool,
    /// The context of the last frame, to wake the UI when a check is queued from a menu callback.
    #[cfg(not(target_arch = "wasm32"))]
    repaint: Option<egui::Context>,
}

impl Default for Runtime {
    fn default() -> Self {
        Self {
            #[cfg(not(target_arch = "wasm32"))]
            updater: Updater::new(env!("CARGO_PKG_VERSION")),
            #[cfg(not(target_arch = "wasm32"))]
            startup_check: false,
            #[cfg(not(target_arch = "wasm32"))]
            check_requested: false,
            #[cfg(not(target_arch = "wasm32"))]
            repaint: None,
        }
    }
}

impl WordApp {
    /// Whether the app asks for a newer release when it starts (off until the desktop app turns it on).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn set_startup_update_check(&mut self, on: bool) {
        self.updates_rt.startup_check = on;
    }
}

/// Help › Check for Updates: opens File › Info, whose Updates section shows the check and what it
/// finds, and starts the check.
pub(crate) fn check(app: &mut WordApp) {
    app.ui.backstage = true;
    app.ui.backstage_page = "info".into();
    #[cfg(not(target_arch = "wasm32"))]
    {
        app.updates_rt.check_requested = true;
        if let Some(ctx) = &app.updates_rt.repaint {
            ctx.request_repaint();
        }
    }
}

pub(crate) fn tick(app: &mut WordApp, ctx: &egui::Context) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let rt = &mut app.updates_rt;
        rt.repaint = Some(ctx.clone());
        if std::mem::take(&mut rt.check_requested) {
            rt.updater.check_now(ctx);
        } else if rt.startup_check {
            rt.updater.check_at_start(ctx);
        }
        rt.updater.tick(ctx);
    }
    #[cfg(target_arch = "wasm32")]
    let _ = (app, ctx);
}

/// No unsaved work, so Strix may close to restart in a new version. The restart helper is
/// scheduled before the app closes, so no update starts over unsaved work.
#[cfg(not(target_arch = "wasm32"))]
fn clean(app: &WordApp) -> bool {
    !(app.session.dirty || app.save_unconfirmed() || app.printing.busy() || crate::panes::has_comment_drafts(app))
}

/// Every frame: once an update is installed and nothing is left unsaved, Strix closes and the new
/// version starts. Nothing is ever shown here: File › Info has it all.
pub(crate) fn show(app: &mut WordApp, ctx: &egui::Context) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let clean = clean(app);
        if app.updates_rt.updater.close_when_installed(clean) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
    #[cfg(target_arch = "wasm32")]
    let _ = (app, ctx);
}

/// The Updates section of File › Info.
pub(crate) fn info_section(app: &mut WordApp, ui: &mut egui::Ui) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let clean = clean(app);
        match app.updates_rt.updater.panel(ui, clean) {
            Action::OpenUrl(url) => ui.ctx().open_url(egui::OpenUrl::new_tab(url)),
            Action::Close => ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close),
            Action::None => {}
        }
    }
    #[cfg(target_arch = "wasm32")]
    {
        // The web build has nothing to check: its runtime is empty.
        let _ = &app.updates_rt;
        ui.label(egui::RichText::new(tl!("Updates")).font(crate::theme::semibold(15.0)));
        crate::pane_kit::note(ui, tl!("The web version always runs the newest release of Strix Writer."));
    }
}

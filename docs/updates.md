# Software updates

Status: in use since 2026-11-09 (v0.19.0); downloads ahead and installs silently at the next start since 2026-11-09; everything shown in File › Info, no window, daily checks, since 2026-11-10.

Strix Writer looks for a newer release of [nabre/strix](https://github.com/nabre/strix/releases)
quietly: when it starts, again every 24 hours while it stays open (one hour after a check that
failed, for instance offline), and on **Help › Check for Updates** or the **Check for Updates**
button. No window ever pops up for it: the **Updates** section of **File › Info** shows
everything. Code: `crates/ui-egui/src/strix_update.rs` (the checks, the state and the panel) and
`crates/ui-egui/src/strix_update/staging.rs` (the download ahead of time and the install at
start), wired by `crates/ui-egui/src/updates_ui.rs`, `crates/ui-egui/src/backstage.rs` (the Info
page) and `apps/strix/src/main.rs`; the checking, downloading, checksum verification and swapping
come from `craft_update` (storytold/craft-libs), shared with the Craft apps.

## What the user sees (File › Info › Updates)

- The running version, then one of:
  - **Checking for updates…** with a spinner while a check runs (at start too: it is quiet, not
    hidden).
  - **You have the newest version.** with the time of the last check (the date too when it
    wasn't today).
  - **Could not check for updates.** with the reason and the time of the attempt; Strix tries
    again within the hour by itself. The failure is also logged.
  - Before any check: Strix says when it looks (at start and once a day).
- When a newer published release exists (drafts and pre-releases are never offered): "Strix
  Writer X is available", its publication date, a **Release notes** link and **What's new**
  (collapsed; the notes as plain lines, pull request links shortened to `#123`). A newer release
  found by a later check replaces the one offered (and its download), unless that one is already
  being installed or waits for the restart. Then:
  - Where Strix can replace this installation itself, the package **downloads in the
    background** right away, with a progress bar and "Downloading in the background: 12.3 of
    48.0 MB" (then "Verifying the download…"; "The connection dropped; trying again shortly…"
    while it reconnects), and is verified against the release's published SHA-256. Once it's
    there the section says it will be installed when Strix restarts, with:
    - **Restart Now**: closes Strix through the normal close, which asks about unsaved work; the
      update goes in on the way out and the new version starts. While that close waits, the
      section says so and **Don't Restart** takes it back.
    - Nothing to do otherwise: the next time Strix starts, **before its window opens**, the
      update is installed without any question and the new version starts in its place (see
      [Installing at the next start](#installing-at-the-next-start)).
  - **Update and Restart** where the installation can replace itself but the package wasn't
    downloaded ahead (the setting is off, or the download failed, which the section says with
    the reason), and for `.deb`/`.rpm` packages, whose install asks for the password (they are
    still downloaded ahead, so it's quick). It downloads the package if needed, verifies it,
    puts it in place, and closes Strix so that a helper restarts the new version. It won't start
    while documents hold unsaved work: it asks to save or close them first.
  - **Download** everywhere else: the package for this platform (the `.dmg` on macOS, the
    FreeBSD tarball…) opens in the browser, or the release page when there is none.
- **Check for updates automatically** (on by default) turns the check at start and the daily
  checks off; Strix then asks GitHub only on **Check for Updates** (or Help › Check for Updates),
  and before any check the section says so. The choice is kept in `strix-update-settings.json`
  (`check`). Until it is chosen there, the installation decides: a `strix-updates.ini` beside the
  program with `[Updates]` `Check=0` turns it off, and that "off" is then saved as the user's
  choice, so an update that drops the file doesn't turn the checks back on. The Windows installer
  writes the file (and `Software\Strix\UpdateCheck`, which the next installer reads to keep the
  choice) when its **Privacy** page's checkbox is cleared or `UPDATECHECK=0` is given to msiexec
  (`packaging/windows/privacy.wxi`). SignPath Foundation's conditions ask for this: software
  that sends data to a system the user didn't name shows its privacy policy when installed and
  lets it be turned off there ([code-signing.md](code-signing.md)).
- **Check for Updates** (disabled while a check runs or an update is installing or waiting for
  the restart) checks at once.
- **Download updates automatically** (checkbox, on by default; not shown on copies that can't
  replace themselves) turns the download ahead off; turning it off cancels a download under way
  and deletes a downloaded package. The choice is kept in `strix-update-settings.json` in
  Strix's update data folder.
- The version just installed is not offered again for a day (a binary that still reports the old
  version would otherwise loop).
- The browser version has no updates: the section says the site always serves the latest
  release.

## Installing at the next start

The downloaded package stays where `craft_update` downloads it (the `downloads` folder of
Strix's update cache: `~/.cache/strix/downloads` on Linux, `%LOCALAPPDATA%\Strix\downloads` on
Windows), and `strix-staged-update.json` in Strix's update data folder (`~/.local/share/strix`,
`%LOCALAPPDATA%\Strix`) records the
release (with the checksums, so the install needs no network) and the file. At the next start,
after logging is set up and before the window opens, Strix:

- installs it only when the start is plain (no documents, `--sample` or `--control` on the
  command line: the new version is started without arguments, so it would lose them; such a
  start leaves the package for a later one, and File › Info offers Restart Now as before), and
  when preferences are on (`WORDCRAFT_NO_PREFS` runs never install);
- discards it when it no longer applies: not newer than the running version, a different kind of
  installation, no checksum, or the file missing or changed (its SHA-256 is checked again);
- otherwise hands it to `craft_update::self_update::update` and exits; its helper starts the new
  version once this process has gone:

| Installation | Installed at start |
|---|---|
| Windows Installer, per user | the helper copy waits for Strix to exit, runs the new `.msi` unattended (`msiexec /passive /norestart`: a progress bar, no questions; signatures checked), then starts Strix |
| Linux AppImage | the new AppImage is renamed over the old one, then started |
| Linux tar.gz in the home folder | the binary is extracted and renamed over the old one, then started |
| Linux `.deb` / `.rpm` | not at start (the package manager asks for the password): Update and Restart in File › Info |

A failure never stops Strix from starting: it is logged, the package and the record are deleted,
the current version opens, and the next check offers the update again. The error of a failed
Windows helper is logged at the next start.

## Which installations update themselves

| Installation | Detected by | Update |
|---|---|---|
| Windows Installer, per user (`%LOCALAPPDATA%\Programs\Strix`) | the folder | the new `.msi`, then restart |
| Linux AppImage | `$APPIMAGE` | the new AppImage replaces the file |
| Linux `.deb` / `.rpm` (package `strix`) | dpkg / rpm | the new package through the package manager |
| Linux tar.gz unpacked in the home folder | the binary's path | the binary is replaced |
| macOS `Strix.app` | — | Download (Strix's bundle id `io.github.nabre.strix` isn't the Craft one `craft_update` replaces) |
| Windows per machine, portable zip or single-file portable exe | — | Download |
| Flatpak, Arch package, FreeBSD, a tar.gz outside the home folder | — | Download |

Assets must keep the release names `strix-<version>-<os>-<arch>…` that
`.github/workflows/release.yml` gives them: `craft_update` picks the package by them.

## Testing

`STRIX_PRETEND_VERSION=0.1.0` makes an installed copy believe it is 0.1.0, so it is offered the
current release (and can try the update against it). A development build (run from a Cargo
`target` folder) never replaces itself unless `CRAFT_UPDATE_DEV=1` is set.

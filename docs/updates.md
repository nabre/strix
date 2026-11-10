# Software updates

Status: in use since 2026-11-09 (v0.19.0); downloads ahead and installs silently at the next start since 2026-11-09.

Strix Writer looks for a newer release of [nabre/strix](https://github.com/nabre/strix/releases)
when it starts and on **Help › Check for Updates**. Code: `crates/ui-egui/src/strix_update.rs`
(the window and the choices) and `crates/ui-egui/src/strix_update/staging.rs` (the download
ahead of time and the install at start), wired by `crates/ui-egui/src/updates_ui.rs` and
`apps/strix/src/main.rs`; the checking,
downloading, checksum verification and swapping come from `craft_update` (storytold/craft-libs),
shared with the Craft apps.

## What the user sees

- Nothing, when there is nothing newer, the computer is offline, or the check fails (the failure
  is logged). Help › Check for Updates says "… is currently the newest version" or why the check
  failed.
- A **Software Update** window when a newer published release exists (drafts and pre-releases are
  never offered): the new and the current version, the release notes as plain lines (pull
  request links shortened to `#123`), a link to the release page, and:
  - Where Strix can replace this installation itself, the package **downloads in the
    background** right away ("Downloading in the background…"; Later leaves it running) and is
    verified against the release's published SHA-256. Once it's there the window says it will be
    installed when Strix restarts, with:
    - **Restart Now**: closes Strix through the normal close, which asks about unsaved work; the
      update goes in on the way out and the new version starts. While that close waits, the
      window says so and **Don't Restart** takes it back.
    - **Later**: nothing more to do. The next time Strix starts, **before its window opens**, the
      update is installed without any question and the new version starts in its place (see
      [Installing at the next start](#installing-at-the-next-start)).
    - **Download updates automatically** (checkbox, on by default) turns the download ahead
      off; turning it off cancels a download under way and deletes a downloaded package. The
      choice is kept in `strix-update-settings.json` in Strix's update data folder.
  - **Update and Restart** where the installation can replace itself but the package wasn't
    downloaded ahead (the setting is off, or the download failed, which the window says), and for
    `.deb`/`.rpm` packages, whose install asks for the password (they are still downloaded ahead,
    so it's quick). It downloads the package if needed, verifies it, puts it in place, and closes
    Strix so that a helper restarts the new version. It won't start while documents hold unsaved
    work: it asks to save or close them first.
  - **Download** everywhere else: the package for this platform (the `.dmg` on macOS, the
    FreeBSD tarball…) opens in the browser, or the release page when there is none.
  - **Later** closes the window; the next start asks again.
- The version just installed is not offered again for a day (a binary that still reports the old
  version would otherwise loop).
- The browser version has no updates: the site always serves the latest release.

## Installing at the next start

The downloaded package stays where `craft_update` downloads it (the `downloads` folder of
Strix's update cache: `~/.cache/strix/downloads` on Linux, `%LOCALAPPDATA%\Strix\downloads` on
Windows), and `strix-staged-update.json` in Strix's update data folder (`~/.local/share/strix`,
`%LOCALAPPDATA%\Strix`) records the
release (with the checksums, so the install needs no network) and the file. At the next start,
after logging is set up and before the window opens, Strix:

- installs it only when the start is plain (no documents, `--sample` or `--control` on the
  command line: the new version is started without arguments, so it would lose them; such a
  start leaves the package for a later one, and the window offers Restart Now as before), and
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
| Linux `.deb` / `.rpm` | not at start (the package manager asks for the password): Update and Restart in the window |

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

# Software updates

Status: in use since 2026-11-09 (v0.19.0).

Strix Writer looks for a newer release of [nabre/strix](https://github.com/nabre/strix/releases)
when it starts and on **Help › Check for Updates**. Code: `crates/ui-egui/src/strix_update.rs`
(the window and the choices), wired by `crates/ui-egui/src/updates_ui.rs`; the checking,
downloading, checksum verification and swapping come from `craft_update` (storytold/craft-libs),
shared with the Craft apps.

## What the user sees

- Nothing, when there is nothing newer, the computer is offline, or the check fails (the failure
  is logged). Help › Check for Updates says "… is currently the newest version" or why the check
  failed.
- A **Software Update** window when a newer published release exists (drafts and pre-releases are
  never offered): the new and the current version, the release notes as plain lines (pull
  request links shortened to `#123`), a link to the release page, and:
  - **Update and Restart** where Strix can replace this installation itself; it downloads the
    package, verifies it, puts it in place, and closes Strix (asking about unsaved work as any
    close does) so that a helper restarts the new version. It won't start while documents hold
    unsaved work: it asks to save or close them first.
  - **Download** everywhere else: the package for this platform (the `.dmg` on macOS, the
    FreeBSD tarball…) opens in the browser, or the release page when there is none.
  - **Later** closes the window; the next start asks again.
- The version just installed is not offered again for a day (a binary that still reports the old
  version would otherwise loop).
- The browser version has no updates: the site always serves the latest release.

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

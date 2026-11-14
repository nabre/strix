# Code signing policy

Status: Windows signing through SignPath Foundation is being set up (application pending,
2026-10-28). Until it is approved, Windows releases are unsigned and SmartScreen warns on first
run.

Free code signing provided by [SignPath.io](https://about.signpath.io), certificate by
[SignPath Foundation](https://signpath.org).

## What is signed

Only Strix's own Windows programs and installers, built from this repository by the GitHub
Actions [Release workflow](../.github/workflows/release.yml) on the `release` branch:

- `strix.exe` and `strix-cli.exe`, in the MSI installers and in the portable ZIP;
- the installers `strix-<version>-windows-<arch>.msi` (one dual-scope package per architecture);
- the single-file portable app `strix-<version>-windows-<arch>-portable.exe` (`strix.exe` renamed).

Every signed file carries the product name **Strix** and the release's version (Windows
VERSIONINFO, set by `apps/strix/build.rs` and `apps/strix-cli/build.rs`). Third-party open-source
programs and libraries are never signed with Strix's certificate. The signing configuration is in
[`.signpath/artifact-configuration.xml`](../.signpath/artifact-configuration.xml).

Each release is signed only after a person in the Approvers role checks the signing request in
SignPath: the request must come from the Release workflow on the `release` branch of
`github.com/nabre/strix`, for a version whose changes have been reviewed.

## Team roles

| Role | Who | Responsibility |
|---|---|---|
| Committers and reviewers | [nabre](https://github.com/nabre) ([repository collaborators](https://github.com/nabre/strix/graphs/contributors)) | Change the source; review every pull request from people outside the team before it is merged |
| Approvers | [nabre](https://github.com/nabre) (repository owner) | Approve each signing request |

Everyone in these roles uses multi-factor authentication on GitHub and on SignPath.

Strix is a fork of [WordCraft](https://github.com/storytold/wordcraft). The Strix team maintains
all of Strix's sources and build scripts, including the code that came from WordCraft. Upstream
changes reach Strix only through a reviewed merge of the `vendor/wordcraft` branch
([docs/upstream.md](upstream.md)); WordCraft's own builds are never re-signed.

## Privacy

This program will not transfer any information to other networked systems unless specifically
requested by the user or the person installing or operating it.

The exception is the check for software updates described below.

In detail:

- Strix collects no telemetry, crash reports or usage data, and has no account.
- **Software updates** are the one exception: the desktop app asks GitHub for the list of Strix's
  releases (`https://api.github.com/repos/nabre/strix/releases`) when it starts and once a day
  while it stays open, and downloads a newer release to install it at the next start
  ([updates.md](updates.md)). The request names only the program
  (`Strix/Update`): nothing about you, your computer or your documents. GitHub sees the usual
  request data (address) under its own privacy policy.
  The automatic check can be turned off: in the app, File › Info › Updates › **Check for updates
  automatically**; when installing, on the Windows installer's **Privacy** page, which shows this
  policy's summary before anything is installed (`msiexec … UPDATECHECK=0` for unattended
  installs); for any copy, a `strix-updates.ini` beside the program with `[Updates]` and
  `Check=0`. Strix then asks GitHub only when you choose **Check for Updates**.
- The network features run only when you use them: opening a link you click; the control channel
  (`--control PORT`) and the MCP server (`strix-cli mcp`), which listen on `127.0.0.1` only and
  require a per-window key; the Zotero integration, which talks to the Zotero app on your own
  computer.
- Documents, preferences and recovery copies stay on your computer.

The web version (`apps/strix-web`) runs in your browser; the site hosting it can see the usual
request data (address, browser) under that host's own privacy policy. It is not code-signed.

## System changes

The Windows installer adds Strix to Program Files (or your user profile for the per-user
installer), a Start menu shortcut, and file types: Strix becomes the default only for its own
`.xmath` and `.xmathm` files and is offered under *Open with* for Word, RTF and OpenDocument files,
without taking them over. Making Strix the default for those files is a separate choice in
File › Options, which says what it changes.

## Uninstall

- **Windows**: Settings › Apps › Installed apps › Strix › Uninstall (or *Add or remove programs*).
  The MSI removes the program, shortcut and file associations. The portable ZIP and the portable
  exe install nothing and touch neither the registry nor `%APPDATA%`: they keep every setting in
  the `StrixData` folder beside the program, so deleting the folder (or the exe and its
  `StrixData`) removes everything.
- **macOS**: drag Strix from Applications to the Trash.
- **Linux**: `sudo apt remove strix` (deb), `sudo dnf remove strix` (rpm),
  `flatpak uninstall io.github.nabre.strix`, or delete the AppImage.
- Installed copies keep preferences, logs and the custom dictionary in your user profile (`%APPDATA%\Strix` on
  Windows, `~/Library/Application Support/Strix` on macOS, `~/.config/strix` on Linux); delete
  that folder to remove them too.

## Reporting

Report a signed file you believe breaks this policy to the maintainers through
[GitHub issues](https://github.com/nabre/strix/issues), or to SignPath at support@signpath.io.

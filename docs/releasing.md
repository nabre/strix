<!-- Modified by the Strix contributors (fork of WordCraft); see NOTICE. -->
# Releasing Strix

The canonical recipe is [craftrules `release/playbook.md`](https://github.com/storytold/craftrules/blob/main/release/playbook.md);
this file has the Strix specifics.

1. Bump the version on `main`: `cargo xtask version set 0.2.0` (the one source of truth is
   `[workspace.package] version`; the app shows it in `--version` and About, with the commit from
   `STRIX_BUILD_SHA`).
2. Merge `main` into `release` and push. `.github/workflows/release.yml` builds:

| Platform | Artifacts | Signing |
|---|---|---|
| macOS | universal `Strix.app` in a `.dmg`, universal CLI zip | Developer ID + notarization (`APPLE_*` secrets) |
| Windows | x64, x86 and arm64 `.msi` (one dual-scope package: per-user by default, needs no admin rights; "Everyone on this computer" installs per machine) + portable `.zip` + single-file portable `.exe` (see below) | Azure Trusted Signing (`AZURE_*` secrets) |
| Linux | x86_64 and aarch64 `.AppImage` (+ `.zsync` for AppImageUpdate), `.flatpak` bundle, `.deb`, `.rpm`, `.tar.gz`; Flathub manifest | checksums |
| Linux riscv64 | `.tar.gz` (cross-compiled on `ubuntu-24.04`, glibc >= 2.39; CLI smoke-tested under QEMU) | checksums |
| FreeBSD | x86_64 `.tar.gz` | checksums |
| Web | `strix-web-<version>.zip` (static site: `index.html`, wasm, glue) | none |

**Windows portable builds.** `strix-<version>-windows-<arch>-portable.zip` holds `strix.exe`,
`strix-cli.exe`, the notices and a `StrixData` folder (with a README); `strix-<version>-windows-<arch>-portable.exe`
is the signed `strix.exe` alone, renamed. Both run in **portable mode**
(`crates/control-key/src/portable.rs`, through `strix_control_key::settings_dir`): when a
`StrixData` folder sits beside the running program, or the program's file name contains
`portable` (then `StrixData` is made on the first run, so a later rename keeps it), every
per-user file lives in `StrixData` instead of `%APPDATA%\Strix`: preferences (`ui.json`), logs,
recovery copies, the custom dictionary, personal templates, added fonts, control-key files, and
extra Hunspell dictionaries (`StrixData\dictionaries`). A portable run writes nothing to the
registry and hides File › Options › Make Strix the default. When `StrixData` can't be made or
written (a read-only drive, Program Files), Strix uses `%APPDATA%\Strix` and logs why.
`STRIX_PROFILE_DIR` still overrides both. The rule is the same on every platform, but only the
Windows builds ship portable.

3. The run drafts a GitHub Release `Strix v<version>` with `SHA256SUMS.txt`; review it, then
   publish it on GitHub or with Actions › **Publish release** (`publish-release.yml`, by hand:
   it makes the draft the published, latest release).

`.github/workflows/release.yml` is Strix's own pipeline: it reuses the suite's composite actions
(`storytold/craft-common-actions/actions/*`) but not its reusable release workflow, which requires
signing on the `release` branch and runs macOS on the paid `macos-15-xlarge` runner. Here:

- **Every platform builds whether or not it can be signed.** The **Signing (check only)** job
  reads the secrets and reports, as warnings and in the run's summary, what will be signed: it
  never fails the run. Without `APPLE_CERTIFICATE` macOS is ad-hoc signed and not notarized
  (Gatekeeper asks users to allow the app); without a `WINDOWS_CERTIFICATE` pair or the six
  `AZURE_*` secrets Windows is unsigned (SmartScreen warns).
- macOS builds on the standard `macos-latest` runner (Apple silicon, the x86_64 half too).
- FreeBSD x86_64 builds in a VM (`vmactions/freebsd-vm`): the app and the CLI in a tar.gz.
- The draft release waits for every build (not for signing).

Signing secrets live in the repository's `release` environment, synced from the release manager's
machine (`craftrules/release/signing-setup.md`); set them and the next run signs.

The packaging scripts must keep their executable bit in git (`git update-index --chmod=+x`):
a checkout made on Windows drops it, and the jobs then fail with "Permission denied" (first
release, 0.12.0).

Local packaging: `packaging/macos/package.sh --arch universal`, `packaging/windows/package.ps1`,
`packaging/linux/package.sh` (then `packaging/linux/flatpak-bundle.sh` for the Flatpak), `packaging/web/package.sh`.

Dry run: `gh workflow run release.yml --ref <branch>` builds the unsigned jobs (version, Linux,
Linux riscv64, Flatpak, FreeBSD, web) on any branch; the signing jobs are refused there by the `release`
environment's branch rule, so no draft release is made.

# Taz Reader

Personal Windows desktop app for saving articles from `taz.de` into a local
library and sending selected text to LingQ.

This is a personal tool, not a polished product. I use it to save articles from
[`taz.de`](https://taz.de) locally and sometimes push them into LingQ.

This is mainly a Windows GUI app. It does not provide a supported CLI or a stable library API.

## Current Scope

- Browse built-in `taz.de` sections and related topic pages.
- Search `taz.de` from inside the app.
- Save extracted article text and metadata into a local SQLite library.
- Filter the local library before uploading to LingQ.
- Preview the saved text before sending it to a LingQ course.
- Sync LingQ lesson status for stored articles.

## Caveats

- Really only for Windows.
- I mostly test it on my own machine.
- The `taz.de` parser is based on current HTML patterns and may break when the site changes.
- LingQ support is limited to the token, course list, upload, and status-sync
  flows exposed in the GUI.
- Public builds are unsigned, so Windows may show an Unknown publisher or SmartScreen warning.

## Running It

### Run the app from source

```powershell
cargo run
```

This starts the GUI.

### Build a release binary

```powershell
cargo build --release
```

### Check that it still works

```powershell
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test -- --test-threads=1
```

## Maintenance Docs

- [`docs/site-change-playbook.md`](docs/site-change-playbook.md) covers parser
  breakage, selector checks, and fixture updates.
- [`docs/manual-smoke-test.md`](docs/manual-smoke-test.md) covers GUI, LingQ,
  installer, and release-script checks that are not fully automated.
- [`docs/security-local-data.md`](docs/security-local-data.md) lists local token,
  settings, and SQLite data paths.

## LingQ

Taz Reader stores the LingQ token in the app data directory as a separate file,
not inside `settings.json`.

You can:

- paste a LingQ token into the GUI settings
- log in through the GUI and let the app save the token for you
- refresh course lists and sync status from inside the GUI

## Local Data

Taz Reader stores app data under:

`%LOCALAPPDATA%\taz-reader\`

That folder contains:

- the SQLite library database
- settings and UI state
- the saved LingQ token file

Older installs that used `%LOCALAPPDATA%\taz_lingq_tool\` are migrated
automatically on first run after the rename.

The GUI includes an **Open library folder** action for jumping directly to this location.

## Installer

One-time prerequisite:

```powershell
winget install JRSoftware.InnoSetup
```

Build the installer:

```powershell
.\scripts\build-installer.ps1
```

Output:

`installer\output\taz-reader-setup.exe`

The installer script reads the version from `Cargo.toml`, builds the release
binary, and passes the same version to Inno Setup.

## Release Script

To run the usual checks, build the installer, and optionally update the GitHub release:

```powershell
# Validate and build only
.\scripts\release.ps1

# Validate, build, and publish/update the GitHub release for the Cargo.toml version
.\scripts\release.ps1 -Publish
```

Run `cargo fmt --all -- --check` manually before a release. The release script
does not run `cargo fmt`; it:

- runs the test suite serially
- runs strict Clippy checks
- builds the Windows installer
- computes a SHA256 checksum
- creates or updates the GitHub release asset through GitHub CLI

## Project Layout

```text
src/
  gui/                  Slint UI state, callbacks, background work, view sync
  taz/                  taz.de discovery, extraction, cleanup, shared models
  database.rs           SQLite storage, queries, migrations
  lingq.rs              LingQ login, course listing, uploads, lesson sync
  settings.rs           Persistent settings and token storage
  lib.rs                App-data paths and migration helpers
docs/
  site-change-playbook.md  Parser selector diagnosis and fixture workflow
  manual-smoke-test.md     GUI, LingQ, installer, and release-script checks
  security-local-data.md   Token, settings, and SQLite data notes
ui/
  app-window.slint      Main Slint UI definition
assets/
  taz.ico               Embedded Windows app icon
installer/
  taz-reader.iss        Inno Setup installer definition
scripts/
  build-installer.ps1   Installer build helper
  release.ps1           Test, lint, package, and optional GitHub release helper
```

## Stack

- Rust 2024
- Slint for the desktop UI
- Tokio + Reqwest for async networking
- Scraper + Regex for extraction and cleanup
- Rusqlite for the local article library
- Inno Setup for Windows packaging

## Notes

- The release binary hides the console window on Windows.
- There is no separate service or cloud piece. Everything else stays local.

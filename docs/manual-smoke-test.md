# Manual Smoke Test

Most app behavior is GUI-driven and touches live `taz.de` or LingQ. The Rust
tests cover parser, database, settings, and helper behavior, but they do not
replace a manual pass through the desktop app.

Run this on Windows before trusting a parser, GUI, installer, or release-script
change.

## Before The GUI Pass

```powershell
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test -- --test-threads=1
cargo run
```

## Browse And Search

- Start the app with `cargo run`.
- Use the Browse view on a built-in section.
- Confirm the list populates or shows a clear error.
- Run a search query from inside the app.
- Confirm search results show article titles instead of only navigation or
  service links.

## Save And Preview

- Save one article from Browse or Search.
- Open the local library view.
- Confirm the saved row appears once.
- Open the preview.
- Check that the preview contains title and body text.
- Check that obvious navigation, share, footer, caption, and related-content
  text is not mixed into the article body.

## LingQ

These steps require a real LingQ account and token.

- Paste or log in with a LingQ token from the GUI settings.
- Refresh the course list.
- Select a course.
- Upload one saved article.
- Confirm the article row shows uploaded status.
- Run status sync.
- Confirm the status stays attached to the same saved article row.

Do not share the token file, settings file, or database when reporting a LingQ
issue. See `docs/security-local-data.md`.

## Installer Build

One-time prerequisite:

```powershell
winget install JRSoftware.InnoSetup
```

Build the installer:

```powershell
.\scripts\build-installer.ps1
```

The script builds the release binary and writes:

```text
installer\output\taz-reader-setup.exe
```

Install it on a test Windows profile if installer behavior changed. Confirm the
app starts and that existing app data under `%LOCALAPPDATA%\taz-reader\` is not
removed.

## Release Script

Validation and installer build only:

```powershell
.\scripts\release.ps1
```

Publish or update the GitHub release asset for the `Cargo.toml` version:

```powershell
.\scripts\release.ps1 -Publish
```

Use `-Publish` only when a release should actually be updated. The release
script runs the serial test suite, strict Clippy, installer build, checksum
calculation, and optional GitHub release upload. It does not run
`cargo fmt --all -- --check`, so run the format check manually before release.

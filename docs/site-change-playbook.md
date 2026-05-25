# Site Change Playbook

The `taz.de` parser is site-specific. It relies on the current HTML shapes and
selector assumptions in `src/taz/extract.rs` and `src/taz/client.rs`.

Use this when browsing, search, or article extraction starts returning empty or
obviously wrong results.

## Symptoms

- Browse or search returns no articles even though `taz.de` has matching pages.
- Saved articles have title `Untitled` unexpectedly.
- Saved articles have empty author, date, section, or teaser fields.
- Article previews contain navigation, share prompts, related links, captions,
  footer text, or other non-article text.
- Article extraction fails with `could not extract article body`.

## Code To Check

- Article parsing entry point: `src/taz/client.rs`, `parse_article_from_html`.
- Article body extraction: `src/taz/extract.rs`, `extract_body`.
- Listing and search extraction: `src/taz/extract.rs`,
  `collect_articles_from_document`.
- Selector constants: `src/taz/extract.rs`, `mod selectors`.
- Article URL identity: `src/identity.rs`.

## Fixture Rule

Do not commit a captured full `taz.de` page. Add a small synthetic fixture that
keeps only the structure needed to reproduce the breakage.

Good fixture content:

- fake title and body text;
- a few tags that match the current selector assumption;
- one or two boilerplate elements that should be ignored;
- a fake `taz.de` URL with a stable article id.

Avoid:

- copied article prose;
- large saved web pages;
- live network tests;
- parser changes without a fixture that shows the intended behavior.

## Workflow

1. Save the smallest local HTML snippet that reproduces the failure.
2. Reduce it to synthetic text and the few tags needed for the regression.
3. Add or update a focused test in `src/taz/client.rs` or `src/taz/extract.rs`.
4. Run the narrow test first, for example:

   ```powershell
   cargo test taz::client -- --test-threads=1
   cargo test taz::extract -- --test-threads=1
   ```

5. Change selectors or cleanup logic only as much as the fixture requires.
6. Run the full checks before committing:

   ```powershell
   cargo fmt --all -- --check
   cargo clippy --all-targets -- -D warnings
   cargo test -- --test-threads=1
   ```

If a change affects live browsing, search, saving, preview, or LingQ upload
flow, also run the manual smoke test in `docs/manual-smoke-test.md`.

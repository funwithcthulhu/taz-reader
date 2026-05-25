# Security And Local Data

Taz Reader stores local app data under:

```text
%LOCALAPPDATA%\taz-reader\
```

Older installs that used:

```text
%LOCALAPPDATA%\taz_lingq_tool\
```

are migrated by the app-data helper in `src/lib.rs`.

## LingQ Token

The LingQ token is stored in:

```text
%LOCALAPPDATA%\taz-reader\lingq_token
```

The token is intentionally outside `settings.json`. Older settings may still
contain the legacy `lingq_api_key` field until the app loads and migrates it.

Do not share `lingq_token` or screenshots that show the token.

## Settings

Settings and GUI state are stored in:

```text
%LOCALAPPDATA%\taz-reader\settings.json
```

This file contains values such as the last view, browse filters, library
filters, LingQ language, selected LingQ collection id, and UI toggles.

## SQLite Library

The article library database is stored in:

```text
%LOCALAPPDATA%\taz-reader\library.db
```

The database stores saved article URLs and metadata, including title, subtitle,
author, date, section, cleaned article text, word count, difficulty, fetch time,
paywall marker state, upload status, LingQ lesson ids, and LingQ lesson URLs.

Do not share the database if saved article text, reading history, LingQ lesson
links, or source URLs are private.

## Files To Avoid Sharing

- `%LOCALAPPDATA%\taz-reader\lingq_token`
- `%LOCALAPPDATA%\taz-reader\settings.json`
- `%LOCALAPPDATA%\taz-reader\library.db`
- old copies under `%LOCALAPPDATA%\taz_lingq_tool\`
- full captured `taz.de` pages used while debugging parser breakage

For parser bug reports, prefer a small synthetic fixture that keeps only the
HTML shape needed to reproduce the selector issue.

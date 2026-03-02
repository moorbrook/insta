# insta

A fast Rust CLI for archiving your entire Instapaper library locally with full-text search.

Downloads articles from an Instapaper CSV export, extracts clean text using a multi-tier extraction pipeline, and stores everything in a searchable SQLite database with FTS5.

## Stats

From a real 15-year Instapaper archive:

- **84.1%** success rate (7,116 / 8,457 articles)
- **14.8M** words indexed
- **~137** articles/minute with 20 concurrent workers

## Requirements

- Rust toolchain (`cargo`)
- `yt-dlp` (optional, for YouTube transcripts)

## Build

```bash
cargo build --release
# Binary at target/release/insta
```

## Quick Start

1. Export your bookmarks from [Instapaper Settings](https://www.instapaper.com/user) as CSV

2. Download all articles:
```bash
insta download Instapaper-Export.csv
```

3. Search your archive:
```bash
insta search "machine learning"
```

4. Read an article:
```bash
insta read 42
```

## Commands

### `insta download <CSV_FILE>`

Download articles from an Instapaper CSV export.

```bash
insta download export.csv                  # defaults: 20 workers, 3 retries, 30s timeout
insta download export.csv -j 10 -r 5       # 10 workers, 5 retries
insta download export.csv --retry-failed    # re-attempt previously failed articles
insta download export.csv -o ~/archive      # custom output directory
```

| Flag | Default | Description |
|------|---------|-------------|
| `-o, --output-dir` | `articles` | Output directory for articles and database |
| `-j, --workers` | `20` | Concurrent download workers |
| `-r, --retries` | `3` | Max retries per article |
| `-t, --timeout` | `30` | HTTP timeout in seconds |
| `--retry-failed` | off | Re-attempt previously failed articles |

Automatically skips articles already marked as successful. Safe to run multiple times with updated exports.

### `insta search <QUERY>`

Full-text search across all downloaded articles using SQLite FTS5.

```bash
insta search neural networks              # keyword search
insta search '"exact phrase"'             # phrase search
insta search 'rust OR golang'             # boolean operators
insta search machine learning -n 20       # more results
```

Returns article ID, title, URL, folder, word count, and a highlighted snippet.

### `insta read <ID>`

Display the full content of an article by its database ID (shown in search results).

```bash
insta read 42
```

### `insta stats`

Show database statistics.

```bash
insta stats
```

```
Instapaper Archive Stats
========================================
Total articles:  8457
Successful:      7116 (84.1%)
  From Archives: 496
Failed:          1341 (15.9%)
Pending:         0
Total words:     14872412
========================================
```

## Extraction Pipeline

For each article, `insta` tries these strategies in order, with automatic fallback:

1. **YouTube** — transcript extraction via `yt-dlp` (auto-generated or manual subtitles)
2. **GitHub** — README via API for repos, raw content for blob files
3. **Scraper-hostile domains** (Medium, etc.) — archive.ph first
4. **Paywalled sites** — Instapaper API `get_text` (requires OAuth setup via `insta login`, WIP)
5. **Primary extraction** — multi-tier HTML pipeline:
   - Tier 1: JSON-LD `articleBody` (fast path for structured pages)
   - Tier 2: CSS-targeted extraction with content scoring (50+ selectors)
   - Tier 3: Mozilla Readability (recovers ~32% of tier 2 failures)
   - Tier 4: Baseline body text extraction
6. **Archive fallback** (final retry only) — archive.ph, then Wayback Machine

Articles retrieved from archives are tagged with `archived` status in the database.

## Output Structure

```
articles/
├── index.db                                    # SQLite database with FTS5
├── afb911cd3b03_Cohere_Documentation.txt       # {sha256_prefix}_{title}.txt
├── 11e66ba53ced_Writing_Predictable_Elixir.txt
└── ...
```

Filenames are deterministic: first 12 hex chars of SHA-256(URL) + sanitized title (max 80 chars).

## Database

SQLite with WAL mode. The `articles` table stores URL, title, folder, tags, timestamp, content, word count, status, and error messages. An FTS5 virtual table (`articles_fts`) indexes title and content with porter stemming.

You can query it directly:

```bash
# All failed articles
sqlite3 articles/index.db "SELECT url, error_message FROM articles WHERE status='failed'"

# Articles by domain
sqlite3 articles/index.db "SELECT title FROM articles WHERE url LIKE '%nytimes.com%'"
```

## Instapaper API Integration (WIP)

The [Instapaper Full API](https://www.instapaper.com/api/full) can recover articles that scraping can't reach — dead sites, paywalled content, and scraper-hostile domains — by fetching the permanently archived copy that Instapaper stored at save time. Requires a Premium subscription and OAuth credentials.

Planned commands:
- `insta login` — authenticate via xAuth, cache OAuth tokens
- `insta repair` — re-fetch failed articles using the API's `get_text` endpoint

## Known Limitations

These are expected failures without the Instapaper API configured:

- **Paywalled articles** — NYTimes, Bloomberg, WSJ, Economist, and ~85 other domains are detected but content is behind login walls
- **Dead links / 404s** — sites that have gone offline since bookmarking
- **JavaScript SPAs** — sites that require a browser to render content
- **Login-required pages** — private or authenticated content

## Deprecated Python Version

The original Python implementation (`download_articles.py`, `analyze_bookmarks.py`, `paywalled_sites.py`) is deprecated. Use the Rust CLI instead — it's faster, has more extraction strategies, and includes built-in search.

## Credits

- HTML extraction ported to Rust from [trafilatura](https://github.com/adbar/trafilatura) by Adrien Barbaresi (Apache 2.0)
- [Mozilla Readability](https://github.com/mozilla/readability) via the `readability` crate
- YouTube transcripts via [yt-dlp](https://github.com/yt-dlp/yt-dlp)

## License

MIT

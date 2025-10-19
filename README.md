# Instapaper Article Downloader

Download and archive all your Instapaper articles locally with intelligent content extraction.

## Features

- **Smart Content Extraction**: Multiple extraction methods for maximum success rate
  - trafilatura (primary, fast)
  - Mozilla Readability (fallback)
  - GitHub README API (for repositories)
  - YouTube transcripts (auto-generated or manual subtitles)
- **SQLite Database**: Full-text search with metadata
- **Resume Support**: Automatically skips already-downloaded articles
- **Concurrent Downloads**: 20 workers for fast processing
- **Python 3.14**: Optimized for performance

## Success Rate

- **82.9%** of articles successfully extracted
- **6,915 / 8,345** articles from a real 15-year Instapaper archive

## Requirements

- Python 3.14+ (automatically handled by `uv`)
- Node.js (for Mozilla Readability fallback)
- `uv` package manager ([installation](https://docs.astral.sh/uv/getting-started/installation/))

## Installation

1. Install `uv` if you haven't already:
```bash
curl -LsSf https://astral.sh/uv/install.sh | sh
```

2. Clone or download this project

3. Export your Instapaper bookmarks as CSV:
   - Go to [Instapaper Settings](https://www.instapaper.com/user)
   - Click "Export" to download CSV file
   - Place the CSV file in the project directory

4. Update the CSV filename in `download_articles.py`:
```python
CSV_FILE = "Instapaper-Export-2025-10-19_02_13_29.csv"  # Change to your filename
```

## Usage

### Basic Download

```bash
uv run download_articles.py
```

That's it! The script will:
- Create an `articles/` directory
- Download all articles using intelligent extraction
- Store metadata in `articles/index.db` SQLite database
- Save articles as `.txt` files with hash-based filenames

### Output Structure

```
articles/
├── index.db                                    # SQLite database
├── afb911cd3b03_Cohere_Documentation.txt      # Article files
├── 11e66ba53ced_Writing_Predictable_Elixir.txt
└── ...
```

### Database Schema

The SQLite database includes:
- Full article metadata (URL, title, folder, tags, timestamp)
- Content preview and word count
- FTS5 full-text search index
- Status tracking (success/failed/pending)

### Search Your Articles

```bash
# Search by keyword
sqlite3 articles/index.db "SELECT title, filename FROM articles_fts WHERE articles_fts MATCH 'machine learning' LIMIT 10"

# View statistics
sqlite3 articles/index.db "SELECT status, COUNT(*) FROM articles GROUP BY status"

# Find articles by folder
sqlite3 articles/index.db "SELECT title, url FROM articles WHERE folder = 'Tech' LIMIT 10"
```

## Supported Content Types

### Regular Articles
- Blog posts, news articles, documentation
- Works with any standard HTML content

### GitHub
- **Repositories**: Fetches README via GitHub API
  - Example: `github.com/anthropics/anthropic-sdk-python`
- **Gists**: Extracts code via regular extractors
  - Example: `gist.github.com/user/12345`
- **GitHub Pages**: Regular blog posts work fine

### YouTube Videos
- Automatically downloads transcripts using `yt-dlp`
- Tries auto-generated subtitles first, then manual
- Converts VTT to clean plain text with deduplication
- Example: `youtube.com/watch?v=dQw4w9WgXcQ`

## Configuration

Edit these constants in `download_articles.py`:

```python
CSV_FILE = "your-export.csv"     # Input CSV file
OUTPUT_DIR = Path("articles")     # Output directory
MAX_WORKERS = 20                  # Concurrent workers (adjust based on your system)
MAX_RETRIES = 3                   # Retry attempts per article
TIMEOUT = 30                      # Timeout per extraction (seconds)
```

## Extraction Pipeline

For each article, the script tries in order:

1. **Special handlers** (if URL matches):
   - YouTube → transcript extraction
   - GitHub repos → README API

2. **trafilatura** (primary):
   - Fast, efficient
   - Works for most articles

3. **Mozilla Readability** (fallback):
   - Slower but more thorough
   - Recovers ~32% of trafilatura failures

## Performance

- **Speed**: ~137 articles/minute with 20 workers
- **Python 3.14**: ~5-10% faster than 3.12 (JIT compilation)
- **Checkpointing**: Resume from where you left off
- **No duplicate work**: Skips already-downloaded articles

## What Won't Work

The following cannot be extracted (expected failures):

- **Paywalled content**: NYTimes, Bloomberg, Economist, WSJ, etc.
- **Landing pages**: Product sites with no article content
- **Invalid URLs**: Broken or localhost URLs
- **JavaScript-heavy SPAs**: Sites that require browser rendering

## Troubleshooting

### No articles being downloaded
- Check the CSV filename matches in `download_articles.py`
- Verify the CSV is in the correct format (URL, Title, Selection, Folder, Timestamp, Tags)

### Mozilla Readability not working
```bash
# Install Node.js dependencies
cd /path/to/project
npm install @mozilla/readability jsdom
```

### YouTube transcripts failing
```bash
# Make sure uvx can run yt-dlp
uvx yt-dlp --version
```

### Database locked error
Only one instance of the script can run at a time. Kill any existing processes:
```bash
pkill -f download_articles.py
```

## File Naming

Articles are saved with the format: `{hash}_{title}.txt`

- **Hash**: First 12 chars of SHA-256(URL) - ensures uniqueness
- **Title**: Sanitized article title (max 80 chars)
- Example: `afb911cd3b03_Cohere_Documentation_Cohere.txt`

## Advanced Usage

### Query the Database

```bash
# Find all failed articles
sqlite3 articles/index.db "SELECT url, error_message FROM articles WHERE status='failed'"

# Get articles by domain
sqlite3 articles/index.db "SELECT title, url FROM articles WHERE url LIKE '%medium.com%'"

# Full-text search
sqlite3 articles/index.db "SELECT title FROM articles_fts WHERE articles_fts MATCH 'neural networks'"
```

### Re-run for New Exports

1. Export new CSV from Instapaper
2. Update `CSV_FILE` in script
3. Run `uv run download_articles.py`

The script automatically:
- Skips articles already marked as `success`
- Re-attempts articles marked as `failed` or `pending`
- Processes only new URLs from the CSV

## License

MIT

## Credits

- Built with [trafilatura](https://github.com/adbar/trafilatura)
- Uses [Mozilla Readability](https://github.com/mozilla/readability)
- YouTube transcripts via [yt-dlp](https://github.com/yt-dlp/yt-dlp)
- Powered by Python 3.14 and [uv](https://github.com/astral-sh/uv)

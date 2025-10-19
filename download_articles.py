#!/usr/bin/env python3
# /// script
# requires-python = ">=3.14"
# dependencies = ["trafilatura", "pandas", "requests"]
# ///

"""
Instapaper Article Downloader
Downloads all articles from Instapaper export CSV using trafilatura.
Stores articles in flat directory with SQLite database for metadata and search.
"""

import csv
import hashlib
import json
import re
import shutil
import sqlite3
import subprocess
import sys
import time
from concurrent.futures import ThreadPoolExecutor, as_completed
from pathlib import Path
from typing import Dict, Optional, Tuple
from urllib.request import Request, urlopen
from urllib.error import URLError, HTTPError

import trafilatura
import requests
from paywalled_sites import is_paywalled, get_paywalled_domain


# Configuration
CSV_FILE = "Instapaper-Export-2025-10-19_02_13_29.csv"
OUTPUT_DIR = Path("articles")
DB_FILE = OUTPUT_DIR / "index.db"
MAX_WORKERS = 20
MAX_RETRIES = 3
TIMEOUT = 30


def init_database(db_path: Path) -> sqlite3.Connection:
    """Initialize SQLite database with schema."""
    conn = sqlite3.connect(db_path, check_same_thread=False)
    conn.execute("""
        CREATE TABLE IF NOT EXISTS articles (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            url TEXT UNIQUE NOT NULL,
            title TEXT,
            folder TEXT,
            timestamp INTEGER,
            tags TEXT,
            filename TEXT,
            status TEXT DEFAULT 'pending',
            error_message TEXT,
            content_preview TEXT,
            word_count INTEGER,
            created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
        )
    """)
    conn.execute("""
        CREATE INDEX IF NOT EXISTS idx_url ON articles(url)
    """)
    conn.execute("""
        CREATE INDEX IF NOT EXISTS idx_status ON articles(status)
    """)
    conn.execute("""
        CREATE INDEX IF NOT EXISTS idx_folder ON articles(folder)
    """)
    conn.execute("""
        CREATE VIRTUAL TABLE IF NOT EXISTS articles_fts USING fts5(
            title, content_preview, content='articles', content_rowid='id'
        )
    """)
    conn.commit()
    return conn


def sanitize_filename(text: str, max_length: int = 100) -> str:
    """Convert text to safe filename."""
    # Remove or replace problematic characters
    safe = re.sub(r'[<>:"/\\|?*]', '', text)
    safe = re.sub(r'\s+', '_', safe.strip())
    # Limit length
    if len(safe) > max_length:
        safe = safe[:max_length]
    return safe if safe else "untitled"


def get_article_id(url: str) -> str:
    """Generate unique ID for article based on URL."""
    return hashlib.sha256(url.encode()).hexdigest()[:12]


def check_readability_available() -> bool:
    """Check if Mozilla readability wrapper is available."""
    wrapper_path = Path(__file__).parent / "readability_wrapper.js"
    return wrapper_path.exists() and shutil.which("node") is not None


def extract_with_readability(url: str) -> Optional[Tuple[str, str]]:
    """Extract article using Mozilla readability wrapper."""
    try:
        wrapper_path = Path(__file__).parent / "readability_wrapper.js"
        result = subprocess.run(
            ["node", str(wrapper_path), url],
            capture_output=True,
            text=True,
            timeout=TIMEOUT
        )
        if result.returncode == 0 and result.stdout.strip():
            content = result.stdout
            # Extract title from first line (usually # Title in markdown)
            lines = content.split('\n')
            title = lines[0].replace('# ', '').strip() if lines else "Untitled"
            return title, content
    except (subprocess.TimeoutExpired, subprocess.SubprocessError):
        pass
    return None


def extract_youtube_transcript(url: str) -> Optional[Tuple[str, str]]:
    """Extract transcript from YouTube video using yt-dlp."""
    try:
        import tempfile
        import os

        # Create temp directory for download
        with tempfile.TemporaryDirectory() as tmpdir:
            output_template = os.path.join(tmpdir, "transcript")

            # Try auto-generated subtitles first (most common)
            result = subprocess.run(
                ["uvx", "yt-dlp", "--write-auto-sub", "--skip-download",
                 "--sub-langs", "en", "--output", output_template, url],
                capture_output=True,
                text=True,
                timeout=TIMEOUT
            )

            # If auto-subs failed, try manual subtitles
            if result.returncode != 0:
                result = subprocess.run(
                    ["uvx", "yt-dlp", "--write-sub", "--skip-download",
                     "--sub-langs", "en", "--output", output_template, url],
                    capture_output=True,
                    text=True,
                    timeout=TIMEOUT
                )

            if result.returncode != 0:
                return None

            # Find the VTT file
            vtt_files = [f for f in os.listdir(tmpdir) if f.endswith('.vtt')]
            if not vtt_files:
                return None

            vtt_path = os.path.join(tmpdir, vtt_files[0])

            # Get video title
            title_result = subprocess.run(
                ["uvx", "yt-dlp", "--print", "%(title)s", url],
                capture_output=True,
                text=True,
                timeout=TIMEOUT
            )
            title = title_result.stdout.strip() if title_result.returncode == 0 else "YouTube Video"

            # Convert VTT to plain text with deduplication
            seen = set()
            lines = []
            with open(vtt_path, 'r', encoding='utf-8') as f:
                for line in f:
                    line = line.strip()
                    if line and not line.startswith('WEBVTT') and not line.startswith('Kind:') \
                       and not line.startswith('Language:') and '-->' not in line:
                        # Remove HTML tags
                        clean = re.sub('<[^>]*>', '', line)
                        # Decode HTML entities
                        clean = clean.replace('&amp;', '&').replace('&gt;', '>').replace('&lt;', '<')
                        if clean and clean not in seen:
                            lines.append(clean)
                            seen.add(clean)

            if lines:
                content = '\n'.join(lines)
                return title, content

    except (subprocess.TimeoutExpired, subprocess.SubprocessError, OSError):
        pass
    return None


def extract_github_readme(url: str) -> Optional[Tuple[str, str]]:
    """Extract README from GitHub repository using GitHub API."""
    try:
        # Parse GitHub URL to extract owner and repo
        # Handles: github.com/owner/repo, github.com/owner/repo/blob/main/..., etc.
        match = re.match(r'https?://(?:www\.)?github\.com/([^/]+)/([^/]+)', url)
        if not match:
            return None

        owner, repo = match.groups()

        # GitHub API endpoint for README
        api_url = f"https://api.github.com/repos/{owner}/{repo}/readme"

        # Make request with user agent (GitHub API requires it)
        req = Request(api_url)
        req.add_header('User-Agent', 'Mozilla/5.0')
        req.add_header('Accept', 'application/vnd.github.v3+json')

        with urlopen(req, timeout=TIMEOUT) as response:
            data = json.loads(response.read().decode())

            # Get README content (it's base64 encoded)
            import base64
            content = base64.b64decode(data['content']).decode('utf-8')

            # Use repo name as title
            title = f"{owner}/{repo}"

            return title, content

    except (URLError, HTTPError, KeyError, ValueError, UnicodeDecodeError):
        pass
    return None


def extract_with_trafilatura_lib(url: str) -> Optional[Tuple[str, str]]:
    """Extract article using trafilatura Python library."""
    try:
        downloaded = trafilatura.fetch_url(url)
        if downloaded:
            # Extract with metadata
            content = trafilatura.extract(
                downloaded,
                include_comments=False,
                include_tables=True,
                no_fallback=False
            )
            if content:
                # Get metadata for title
                metadata = trafilatura.extract_metadata(downloaded)
                title = metadata.title if metadata and metadata.title else "Untitled"
                return title, content
    except Exception:
        pass
    return None


def get_archive_snapshot(url: str) -> Optional[Tuple[str, str]]:
    """Get the latest snapshot URL from archive services (archive.ph, then Wayback Machine)."""
    # Try archive.ph first (better for paywalled content)
    try:
        # Archive.ph search API
        api_url = f"https://archive.ph/newest/{url}"
        headers = {
            'User-Agent': 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/91.0.4472.124 Safari/537.36'
        }
        response = requests.get(api_url, headers=headers, timeout=10, allow_redirects=True)

        if response.status_code == 200 and 'archive.ph' in response.url:
            return response.url, "archive.ph"
    except Exception:
        pass

    # Fallback to Wayback Machine
    try:
        api_url = f"https://archive.org/wayback/available?url={url}"
        response = requests.get(api_url, timeout=10)

        if response.status_code == 200:
            data = response.json()
            if data.get('archived_snapshots', {}).get('closest', {}).get('available'):
                snapshot_url = data['archived_snapshots']['closest']['url']
                return snapshot_url, "wayback"
    except Exception:
        pass

    return None


def extract_from_archive(url: str) -> Optional[Tuple[str, str]]:
    """Try to extract article from archive services (archive.ph or Wayback Machine)."""
    try:
        # Get the latest snapshot
        result = get_archive_snapshot(url)
        if not result:
            return None

        snapshot_url, source = result

        # Extract from the snapshot using trafilatura
        downloaded = trafilatura.fetch_url(snapshot_url)
        if downloaded:
            content = trafilatura.extract(
                downloaded,
                include_comments=False,
                include_tables=True,
                no_fallback=False
            )
            if content:
                metadata = trafilatura.extract_metadata(downloaded)
                title = metadata.title if metadata and metadata.title else "Untitled"
                # Add note about source
                source_name = "Archive.ph" if source == "archive.ph" else "Internet Archive Wayback Machine"
                note = f"\n\n---\nNote: This article was retrieved from {source_name}\nOriginal URL: {url}\nArchive URL: {snapshot_url}\n"
                return title, content + note
    except Exception:
        pass
    return None


def extract_article(url: str, use_readability: bool, try_wayback: bool = False) -> Optional[Tuple[str, str]]:
    """Extract article content with special handling for different URL types."""
    # Special case: YouTube videos - extract transcript
    if 'youtube.com' in url or 'youtu.be' in url:
        result = extract_youtube_transcript(url)
        if result:
            return result

    # Special case: GitHub repositories - try API first
    if 'github.com' in url and '/gist.' not in url:  # Exclude gists, they work with extractors
        result = extract_github_readme(url)
        if result:
            return result

    # Try trafilatura (fast, works for most articles)
    result = extract_with_trafilatura_lib(url)
    if result:
        return result

    # If trafilatura fails and readability is available, try it
    if use_readability:
        result = extract_with_readability(url)
        if result:
            return result

    # If all else fails and this is a paywalled site, try archive services
    if try_wayback and is_paywalled(url):
        result = extract_from_archive(url)
        if result:
            return result

    return None


def process_article(row: Dict, use_readability: bool, output_dir: Path, db_path: Path) -> Dict:
    """Process a single article: extract and save."""
    url = row['URL']
    instapaper_title = row['Title']
    folder = row['Folder']
    timestamp = row['Timestamp']
    tags = row['Tags']

    # Generate article ID and filename
    article_id = get_article_id(url)

    # Each thread needs its own connection
    conn = sqlite3.connect(db_path)

    result = {
        'url': url,
        'status': 'failed',
        'error': None,
        'filename': None
    }

    # Try extraction with retries
    for attempt in range(MAX_RETRIES):
        try:
            # On final attempt, try wayback for paywalled sites
            try_wayback = (attempt == MAX_RETRIES - 1)
            extracted = extract_article(url, use_readability, try_wayback)

            if extracted:
                title, content = extracted

                # Use extracted title, fallback to Instapaper title
                final_title = title if title and title != "Untitled" else instapaper_title

                # Create filename: id_sanitized-title.txt
                safe_title = sanitize_filename(final_title, max_length=80)
                filename = f"{article_id}_{safe_title}.txt"
                filepath = output_dir / filename

                # Save article content
                filepath.write_text(content, encoding='utf-8')

                # Calculate word count and preview
                word_count = len(content.split())
                preview = content[:500].replace('\n', ' ').strip()

                # Update database - add note if from archive
                is_from_archive = ('Internet Archive Wayback Machine' in content or 'Archive.ph' in content)
                cursor = conn.cursor()
                cursor.execute("""
                    UPDATE articles
                    SET status = ?,
                        title = ?,
                        filename = ?,
                        content_preview = ?,
                        word_count = ?
                    WHERE url = ?
                """, ('success' if not is_from_archive else 'archived', final_title, filename, preview, word_count, url))

                # Update FTS index
                cursor.execute("""
                    INSERT INTO articles_fts(rowid, title, content_preview)
                    SELECT id, title, content_preview FROM articles WHERE url = ?
                """, (url,))

                conn.commit()

                result['status'] = 'success'
                result['filename'] = filename
                break
            else:
                if attempt == MAX_RETRIES - 1:
                    # Check if it's a paywalled site for better error message
                    paywalled_domain = get_paywalled_domain(url)
                    if paywalled_domain:
                        result['error'] = f"Paywalled site ({paywalled_domain}) - no archive available"
                    else:
                        result['error'] = "Extraction returned no content"
        except Exception as e:
            if attempt == MAX_RETRIES - 1:
                result['error'] = str(e)

        if attempt < MAX_RETRIES - 1:
            time.sleep(1)

    # Update database with failure if needed
    if result['status'] == 'failed':
        conn.execute("""
            UPDATE articles
            SET status = 'failed', error_message = ?
            WHERE url = ?
        """, (result['error'], url))
        conn.commit()

    conn.close()
    return result


def load_csv(csv_path: Path, conn: sqlite3.Connection) -> list:
    """Load CSV and populate database with pending articles."""
    articles = []

    with open(csv_path, 'r', encoding='utf-8') as f:
        reader = csv.DictReader(f)
        for row in reader:
            url = row['URL']

            # Check if already processed
            cursor = conn.execute("SELECT status FROM articles WHERE url = ?", (url,))
            existing = cursor.fetchone()

            if existing and existing[0] == 'success':
                continue  # Skip already successfully downloaded articles

            # Insert or update in database
            conn.execute("""
                INSERT OR REPLACE INTO articles (url, title, folder, timestamp, tags, status)
                VALUES (?, ?, ?, ?, ?, 'pending')
            """, (url, row['Title'], row['Folder'], row['Timestamp'], row['Tags']))

            articles.append(row)

    conn.commit()
    return articles


def generate_report(conn: sqlite3.Connection, elapsed_time: float):
    """Generate summary report."""
    cursor = conn.cursor()

    total = cursor.execute("SELECT COUNT(*) FROM articles").fetchone()[0]
    success = cursor.execute("SELECT COUNT(*) FROM articles WHERE status IN ('success', 'archived')").fetchone()[0]
    archived = cursor.execute("SELECT COUNT(*) FROM articles WHERE status = 'archived'").fetchone()[0]
    failed = cursor.execute("SELECT COUNT(*) FROM articles WHERE status = 'failed'").fetchone()[0]
    pending = cursor.execute("SELECT COUNT(*) FROM articles WHERE status = 'pending'").fetchone()[0]

    total_words = cursor.execute("SELECT SUM(word_count) FROM articles WHERE status IN ('success', 'archived')").fetchone()[0] or 0

    print("\n" + "="*60)
    print("DOWNLOAD SUMMARY")
    print("="*60)
    print(f"Total articles:     {total}")
    print(f"Successfully saved: {success} ({success/total*100:.1f}%)")
    if archived > 0:
        print(f"  From Archives:    {archived}")
    print(f"Failed:             {failed} ({failed/total*100:.1f}%)")
    print(f"Pending:            {pending}")
    print(f"Total words:        {total_words:,}")
    print(f"Time elapsed:       {elapsed_time:.1f}s ({elapsed_time/60:.1f} minutes)")
    print(f"Average:            {elapsed_time/total:.2f}s per article")
    print("="*60)

    # Show failed URLs
    if failed > 0:
        print("\nFailed URLs (first 10):")
        cursor.execute("SELECT url, error_message FROM articles WHERE status = 'failed' LIMIT 10")
        for url, error in cursor.fetchall():
            print(f"  - {url}")
            if error:
                print(f"    Error: {error[:100]}")

    print(f"\nDatabase saved to: {DB_FILE}")
    print(f"Articles saved to: {OUTPUT_DIR}/")


def main():
    """Main execution."""
    start_time = time.time()

    # Setup
    OUTPUT_DIR.mkdir(exist_ok=True)
    conn = init_database(DB_FILE)

    print("Instapaper Article Downloader")
    print("="*60)

    # Check if readability is available for fallback
    use_readability = check_readability_available()
    if use_readability:
        print("Extraction: trafilatura (primary) + Mozilla Readability (fallback)")
    else:
        print("Extraction: trafilatura only")
        print("Note: Install Node.js and readability_wrapper.js for better recovery rate")

    # Load articles
    print(f"Loading articles from {CSV_FILE}...")
    articles = load_csv(Path(CSV_FILE), conn)

    if not articles:
        print("No articles to download (all already processed)")
        conn.close()
        return

    print(f"Found {len(articles)} articles to download")
    print(f"Using {MAX_WORKERS} concurrent workers")
    print("="*60 + "\n")

    # Process articles with threading
    success_count = 0
    failed_count = 0

    with ThreadPoolExecutor(max_workers=MAX_WORKERS) as executor:
        # Submit all tasks
        futures = {
            executor.submit(process_article, article, use_readability, OUTPUT_DIR, DB_FILE): article
            for article in articles
        }

        # Process with periodic status updates
        print(f"\nProcessing {len(articles)} articles...")
        print(f"{'='*60}")
        sys.stdout.flush()

        last_print = time.time()
        for i, future in enumerate(as_completed(futures), 1):
            result = future.result()
            if result['status'] == 'success':
                success_count += 1
            else:
                failed_count += 1

            # Print every 100 articles or every 10 seconds
            if i % 100 == 0 or time.time() - last_print > 10:
                percent = (i / len(articles)) * 100
                print(f"Progress: {i}/{len(articles)} ({percent:.1f}%) - Success: {success_count}, Failed: {failed_count}")
                sys.stdout.flush()
                last_print = time.time()

        # Final update
        print(f"Progress: {len(articles)}/{len(articles)} (100.0%) - Success: {success_count}, Failed: {failed_count}")
        sys.stdout.flush()

    # Generate report
    elapsed_time = time.time() - start_time
    generate_report(conn, elapsed_time)

    conn.close()


if __name__ == "__main__":
    main()

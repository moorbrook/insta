use anyhow::Context;
use rusqlite::{params, Connection};
use std::path::Path;
use std::sync::{Mutex, MutexGuard};

use crate::csv_reader::ArticleRow;

pub struct Database {
    conn: Mutex<Connection>,
}

pub struct SearchResult {
    pub id: i64,
    pub title: Option<String>,
    pub url: String,
    pub folder: Option<String>,
    pub word_count: Option<i64>,
    pub snippet: String,
}

pub struct Article {
    pub id: i64,
    pub title: Option<String>,
    pub url: String,
    pub folder: Option<String>,
    pub word_count: Option<i64>,
    pub content: Option<String>,
}

pub struct StatusCounts {
    pub total: i64,
    pub success: i64,
    pub archived: i64,
    pub failed: i64,
    pub pending: i64,
    pub total_words: i64,
}

impl Database {
    pub fn open(path: &Path) -> anyhow::Result<Self> {
        let conn = Connection::open(path).context("Failed to open database")?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Acquire the database connection lock, converting poison errors to anyhow.
    fn lock_conn(&self) -> anyhow::Result<MutexGuard<'_, Connection>> {
        self.conn
            .lock()
            .map_err(|e| anyhow::anyhow!("Database lock poisoned: {e}"))
    }

    pub fn init_schema(&self) -> anyhow::Result<()> {
        let conn = self.lock_conn()?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS articles (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                url TEXT UNIQUE NOT NULL,
                title TEXT,
                folder TEXT,
                timestamp INTEGER,
                tags TEXT,
                filename TEXT,
                status TEXT DEFAULT 'pending',
                error_message TEXT,
                content TEXT,
                word_count INTEGER,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
            );
            CREATE INDEX IF NOT EXISTS idx_url ON articles(url);
            CREATE INDEX IF NOT EXISTS idx_status ON articles(status);
            CREATE INDEX IF NOT EXISTS idx_folder ON articles(folder);
            CREATE VIRTUAL TABLE IF NOT EXISTS articles_fts USING fts5(
                title, content, content='articles', content_rowid='id',
                tokenize='porter unicode61'
            );

            -- Triggers to keep FTS in sync
            CREATE TRIGGER IF NOT EXISTS articles_ai AFTER INSERT ON articles BEGIN
                INSERT INTO articles_fts(rowid, title, content)
                VALUES (new.id, new.title, new.content);
            END;
            CREATE TRIGGER IF NOT EXISTS articles_ad AFTER DELETE ON articles BEGIN
                INSERT INTO articles_fts(articles_fts, rowid, title, content)
                VALUES ('delete', old.id, old.title, old.content);
            END;
            CREATE TRIGGER IF NOT EXISTS articles_au AFTER UPDATE ON articles BEGIN
                INSERT INTO articles_fts(articles_fts, rowid, title, content)
                VALUES ('delete', old.id, old.title, old.content);
                INSERT INTO articles_fts(rowid, title, content)
                VALUES (new.id, new.title, new.content);
            END;",
        )?;

        // Migrate: if old schema had content_preview but no content column, add it
        let has_content: bool = conn
            .prepare("SELECT content FROM articles LIMIT 0")
            .is_ok();
        if !has_content {
            conn.execute_batch(
                "ALTER TABLE articles ADD COLUMN content TEXT;
                 -- Move preview data into content for existing rows
                 UPDATE articles SET content = content_preview WHERE content IS NULL AND content_preview IS NOT NULL;",
            )?;
        }

        Ok(())
    }

    /// Check that required tables exist, bail with a friendly message if not.
    pub fn ensure_schema(&self) -> anyhow::Result<()> {
        let conn = self.lock_conn()?;
        let has_table: bool = conn
            .prepare("SELECT 1 FROM articles LIMIT 0")
            .is_ok();
        if !has_table {
            anyhow::bail!(
                "Database exists but has no articles table.\nRun `insta download <export.csv>` first."
            );
        }
        Ok(())
    }

    pub fn is_already_successful(&self, url: &str) -> anyhow::Result<bool> {
        let conn = self.lock_conn()?;
        let mut stmt = conn.prepare_cached("SELECT 1 FROM articles WHERE url = ? AND status = 'success'")?;
        Ok(stmt.exists(params![url])?)
    }

    pub fn is_already_failed(&self, url: &str) -> anyhow::Result<bool> {
        let conn = self.lock_conn()?;
        let mut stmt = conn.prepare_cached("SELECT 1 FROM articles WHERE url = ? AND status = 'failed'")?;
        Ok(stmt.exists(params![url])?)
    }

    pub fn insert_pending(&self, row: &ArticleRow) -> anyhow::Result<()> {
        let conn = self.lock_conn()?;
        let timestamp: Option<i64> = row.timestamp.parse().ok();
        conn.execute(
            "INSERT INTO articles (url, title, folder, timestamp, tags, status)
             VALUES (?1, ?2, ?3, ?4, ?5, 'pending')
             ON CONFLICT(url) DO UPDATE SET
                title = excluded.title,
                folder = excluded.folder,
                timestamp = excluded.timestamp,
                tags = excluded.tags,
                status = 'pending'",
            params![row.url, row.title, row.folder, timestamp, row.tags],
        )?;
        Ok(())
    }

    pub fn mark_success(
        &self,
        url: &str,
        title: &str,
        filename: &str,
        content: &str,
        word_count: i64,
        is_archived: bool,
    ) -> anyhow::Result<()> {
        let conn = self.lock_conn()?;
        let status = if is_archived { "archived" } else { "success" };
        // UPDATE triggers handle FTS sync automatically
        conn.execute(
            "UPDATE articles SET status = ?1, title = ?2, filename = ?3,
             content = ?4, word_count = ?5 WHERE url = ?6",
            params![status, title, filename, content, word_count, url],
        )?;
        Ok(())
    }

    pub fn mark_failed(&self, url: &str, error: &str) -> anyhow::Result<()> {
        let conn = self.lock_conn()?;
        conn.execute(
            "UPDATE articles SET status = 'failed', error_message = ?1 WHERE url = ?2",
            params![error, url],
        )?;
        Ok(())
    }

    pub fn get_status_counts(&self) -> anyhow::Result<StatusCounts> {
        let conn = self.lock_conn()?;
        let total: i64 = conn.query_row("SELECT COUNT(*) FROM articles", [], |r| r.get(0))?;
        let success: i64 = conn.query_row(
            "SELECT COUNT(*) FROM articles WHERE status IN ('success', 'archived')",
            [],
            |r| r.get(0),
        )?;
        let archived: i64 = conn.query_row(
            "SELECT COUNT(*) FROM articles WHERE status = 'archived'",
            [],
            |r| r.get(0),
        )?;
        let failed: i64 = conn.query_row(
            "SELECT COUNT(*) FROM articles WHERE status = 'failed'",
            [],
            |r| r.get(0),
        )?;
        let pending: i64 = conn.query_row(
            "SELECT COUNT(*) FROM articles WHERE status = 'pending'",
            [],
            |r| r.get(0),
        )?;
        let total_words: i64 = conn.query_row(
            "SELECT COALESCE(SUM(word_count), 0) FROM articles WHERE status IN ('success', 'archived')",
            [],
            |r| r.get(0),
        )?;
        Ok(StatusCounts {
            total,
            success,
            archived,
            failed,
            pending,
            total_words,
        })
    }

    pub fn search(
        &self,
        query: &str,
        limit: usize,
    ) -> anyhow::Result<Vec<SearchResult>> {
        let conn = self.lock_conn()?;
        let mut stmt = conn.prepare(
            "SELECT a.id, a.title, a.url, a.folder, a.word_count,
                    snippet(articles_fts, 1, '>>>','<<<', '...', 30) as snip
             FROM articles_fts
             JOIN articles a ON a.id = articles_fts.rowid
             WHERE articles_fts MATCH ?1
             ORDER BY rank
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![query, limit as i64], |row| {
            Ok(SearchResult {
                id: row.get(0)?,
                title: row.get(1)?,
                url: row.get(2)?,
                folder: row.get::<_, Option<String>>(3)?,
                word_count: row.get::<_, Option<i64>>(4)?,
                snippet: row.get(5)?,
            })
        })?;
        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }

    pub fn read_by_id(&self, id: i64) -> anyhow::Result<Option<Article>> {
        let conn = self.lock_conn()?;
        let mut stmt = conn.prepare_cached(
            "SELECT id, title, url, folder, word_count, content FROM articles WHERE id = ?1",
        )?;
        let result = stmt
            .query_row(params![id], |row| {
                Ok(Article {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    url: row.get(2)?,
                    folder: row.get(3)?,
                    word_count: row.get(4)?,
                    content: row.get(5)?,
                })
            })
            .ok();
        Ok(result)
    }

    pub fn get_failed_urls(&self, limit: usize) -> anyhow::Result<Vec<(String, Option<String>)>> {
        let conn = self.lock_conn()?;
        let mut stmt = conn.prepare(
            "SELECT url, error_message FROM articles WHERE status = 'failed' LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit as i64], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
        })?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }
}

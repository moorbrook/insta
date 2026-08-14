use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use std::path::Path;
use std::sync::{Mutex, MutexGuard};

use crate::csv_reader::ArticleRow;
use crate::error::DatabaseError;
use crate::status::{ArticleStatus, SuccessSource};

#[derive(Debug)]
pub(crate) struct Database {
    conn: Mutex<Connection>,
}

#[derive(Debug, Serialize)]
pub struct SearchResult {
    pub id: i64,
    pub title: Option<String>,
    pub url: String,
    pub folder: Option<String>,
    pub word_count: Option<i64>,
    pub snippet: String,
}

#[derive(Debug, Serialize)]
pub struct Article {
    pub title: Option<String>,
    pub url: String,
    pub word_count: Option<i64>,
    pub content: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct StatusCounts {
    pub total: i64,
    pub success: i64,
    pub archived: i64,
    pub failed: i64,
    pub pending: i64,
    pub total_words: i64,
}

impl Database {
    pub(crate) fn open(path: &Path) -> Result<Self, DatabaseError> {
        let conn = Connection::open(path).map_err(|source| DatabaseError::Open {
            path: path.to_path_buf(),
            source,
        })?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    fn lock_conn(&self) -> Result<MutexGuard<'_, Connection>, DatabaseError> {
        self.conn.lock().map_err(|_| DatabaseError::Poisoned)
    }

    pub(crate) fn init_schema(&self) -> Result<(), DatabaseError> {
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
        let has_content: bool = conn.prepare("SELECT content FROM articles LIMIT 0").is_ok();
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
    pub(crate) fn ensure_schema(&self) -> Result<(), DatabaseError> {
        let conn = self.lock_conn()?;
        let has_table: bool = conn.prepare("SELECT 1 FROM articles LIMIT 0").is_ok();
        if !has_table {
            return Err(DatabaseError::MissingSchema);
        }
        Ok(())
    }

    pub(crate) fn is_already_successful(&self, url: &str) -> Result<bool, DatabaseError> {
        let conn = self.lock_conn()?;
        let mut stmt = conn.prepare_cached(
            "SELECT 1 FROM articles WHERE url = ? AND status IN ('success', 'archived')",
        )?;
        Ok(stmt.exists(params![url])?)
    }

    pub(crate) fn is_already_failed(&self, url: &str) -> Result<bool, DatabaseError> {
        let conn = self.lock_conn()?;
        let mut stmt =
            conn.prepare_cached("SELECT 1 FROM articles WHERE url = ? AND status = ?")?;
        Ok(stmt.exists(params![url, ArticleStatus::Failed.as_str()])?)
    }

    pub(crate) fn insert_pending(&self, row: &ArticleRow) -> Result<(), DatabaseError> {
        let conn = self.lock_conn()?;
        let timestamp: Option<i64> = row.timestamp.parse().ok();
        conn.execute(
            "INSERT INTO articles (url, title, folder, timestamp, tags, status)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(url) DO UPDATE SET
                title = excluded.title,
                folder = excluded.folder,
                timestamp = excluded.timestamp,
                tags = excluded.tags,
                status = excluded.status",
            params![
                row.url,
                row.title,
                row.folder,
                timestamp,
                row.tags,
                ArticleStatus::Pending.as_str()
            ],
        )?;
        Ok(())
    }

    pub(crate) fn mark_success(
        &self,
        url: &str,
        title: &str,
        filename: &str,
        content: &str,
        word_count: i64,
        source: SuccessSource,
    ) -> Result<(), DatabaseError> {
        let conn = self.lock_conn()?;
        let status = source.status().as_str();
        // UPDATE triggers handle FTS sync automatically
        conn.execute(
            "UPDATE articles SET status = ?1, title = ?2, filename = ?3,
             content = ?4, word_count = ?5 WHERE url = ?6",
            params![status, title, filename, content, word_count, url],
        )?;
        Ok(())
    }

    pub(crate) fn mark_failed(&self, url: &str, error: &str) -> Result<(), DatabaseError> {
        let conn = self.lock_conn()?;
        conn.execute(
            "UPDATE articles SET status = ?1, error_message = ?2 WHERE url = ?3",
            params![ArticleStatus::Failed.as_str(), error, url],
        )?;
        Ok(())
    }

    pub(crate) fn get_status_counts(&self) -> Result<StatusCounts, DatabaseError> {
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

    pub(crate) fn search(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<SearchResult>, DatabaseError> {
        self.search_inner(query, limit)
            .map_err(|error| match error {
                DatabaseError::Sqlite(source) => DatabaseError::Search { source },
                other => other,
            })
    }

    fn search_inner(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>, DatabaseError> {
        let conn = self.lock_conn()?;
        let limit = i64::try_from(limit).map_err(|_| DatabaseError::LimitRange)?;
        let mut stmt = conn.prepare(
            "SELECT a.id, a.title, a.url, a.folder, a.word_count,
                    snippet(articles_fts, 1, '>>>','<<<', '...', 30) as snip
             FROM articles_fts
             JOIN articles a ON a.id = articles_fts.rowid
             WHERE articles_fts MATCH ?1
             ORDER BY rank
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![query, limit], |row| {
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

    pub(crate) fn read_by_id(&self, id: i64) -> Result<Option<Article>, DatabaseError> {
        let conn = self.lock_conn()?;
        let mut stmt = conn
            .prepare_cached("SELECT title, url, word_count, content FROM articles WHERE id = ?1")?;
        let result = stmt.query_row(params![id], |row| {
            Ok(Article {
                title: row.get(0)?,
                url: row.get(1)?,
                word_count: row.get(2)?,
                content: row.get(3)?,
            })
        });
        let result = result.optional()?;
        Ok(result)
    }

    pub(crate) fn get_failed_urls(
        &self,
        limit: usize,
    ) -> Result<Vec<(String, Option<String>)>, DatabaseError> {
        let conn = self.lock_conn()?;
        let limit = i64::try_from(limit).map_err(|_| DatabaseError::LimitRange)?;
        let mut stmt = conn
            .prepare("SELECT url, error_message FROM articles WHERE status = 'failed' LIMIT ?1")?;
        let rows = stmt.query_map(params![limit], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
        })?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::print_stdout,
        clippy::print_stderr
    )]

    use super::*;
    use tempfile::TempDir;

    fn initialized_database() -> (TempDir, Database) {
        let directory = tempfile::tempdir().unwrap();
        let database = Database::open(&directory.path().join("index.db")).unwrap();
        database.init_schema().unwrap();
        (directory, database)
    }

    fn article_row(url: &str, title: &str) -> ArticleRow {
        ArticleRow {
            url: url.to_string(),
            title: title.to_string(),
            _selection: String::new(),
            folder: "Unread".to_string(),
            timestamp: "1700000000".to_string(),
            tags: "[]".to_string(),
        }
    }

    #[test]
    fn success_updates_replace_fts_content_without_stale_terms() {
        let (_directory, database) = initialized_database();
        let row = article_row("https://example.com/article", "First title");
        database.insert_pending(&row).unwrap();

        let pending = database.get_status_counts().unwrap();
        assert_eq!(pending.total, 1);
        assert_eq!(pending.pending, 1);

        database
            .mark_success(
                &row.url,
                &row.title,
                "article.txt",
                "an orchard contains the original searchable phrase",
                7,
                SuccessSource::Live,
            )
            .unwrap();
        assert_eq!(database.search("orchard", 10).unwrap().len(), 1);

        database
            .mark_success(
                &row.url,
                "Revised title",
                "article.txt",
                "a meadow contains the replacement searchable phrase",
                7,
                SuccessSource::Live,
            )
            .unwrap();

        assert!(database.search("orchard", 10).unwrap().is_empty());
        let revised = database.search("meadow", 10).unwrap();
        assert_eq!(revised.len(), 1);
        assert_eq!(revised[0].title.as_deref(), Some("Revised title"));

        let article = database.read_by_id(revised[0].id).unwrap().unwrap();
        assert_eq!(article.title.as_deref(), Some("Revised title"));
        assert!(article.content.unwrap().contains("replacement"));

        let counts = database.get_status_counts().unwrap();
        assert_eq!(counts.total, 1);
        assert_eq!(counts.success, 1);
        assert_eq!(counts.archived, 0);
        assert_eq!(counts.failed, 0);
        assert_eq!(counts.pending, 0);
        assert_eq!(counts.total_words, 7);
    }

    #[test]
    fn status_counts_partition_all_rows() {
        let (_directory, database) = initialized_database();
        let live = article_row("https://example.com/live", "Live");
        let archived = article_row("https://example.com/archived", "Archived");
        let failed = article_row("https://example.com/failed", "Failed");

        for row in [&live, &archived, &failed] {
            database.insert_pending(row).unwrap();
        }
        database
            .mark_success(
                &live.url,
                &live.title,
                "live.txt",
                "live content",
                2,
                SuccessSource::Live,
            )
            .unwrap();
        database
            .mark_success(
                &archived.url,
                &archived.title,
                "archived.txt",
                "archived content",
                2,
                SuccessSource::Archive,
            )
            .unwrap();
        database.mark_failed(&failed.url, "network error").unwrap();

        let counts = database.get_status_counts().unwrap();
        assert_eq!(counts.total, 3);
        assert_eq!(counts.success, 2);
        assert_eq!(counts.archived, 1);
        assert_eq!(counts.failed, 1);
        assert_eq!(counts.pending, 0);
        assert_eq!(
            counts.total,
            counts.success + counts.failed + counts.pending
        );
        assert_eq!(counts.total_words, 4);
        assert!(database.is_already_successful(&live.url).unwrap());
        assert!(database.is_already_successful(&archived.url).unwrap());
        assert!(database.is_already_failed(&failed.url).unwrap());
    }

    #[test]
    fn schema_initialization_is_idempotent() {
        let (_directory, database) = initialized_database();
        database.init_schema().unwrap();
        database.init_schema().unwrap();
        assert_eq!(database.get_status_counts().unwrap().total, 0);
    }

    #[test]
    fn missing_row_is_distinct_from_a_broken_schema() {
        let (_directory, database) = initialized_database();
        assert!(database.read_by_id(404).unwrap().is_none());

        let directory = tempfile::tempdir().unwrap();
        let database_without_schema =
            Database::open(&directory.path().join("missing-schema.db")).unwrap();
        assert!(database_without_schema.read_by_id(404).is_err());
    }
}

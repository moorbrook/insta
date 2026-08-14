use std::path::PathBuf;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Database(#[from] DatabaseError),
    #[error(transparent)]
    Csv(#[from] CsvError),
    #[error(transparent)]
    Extract(#[from] ExtractError),
    #[error("all {failed} article(s) failed to download. run with --retry-failed to try again")]
    AllDownloadsFailed { failed: u64 },
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

#[derive(Debug, thiserror::Error)]
pub enum DatabaseError {
    #[error("failed to open database {path}")]
    Open {
        path: PathBuf,
        #[source]
        source: rusqlite::Error,
    },
    #[error("database lock poisoned")]
    Poisoned,
    #[error(
        "no database found at {path}\nrun `insta download <export.csv>` first to build the article index"
    )]
    Missing { path: PathBuf },
    #[error("database exists but has no articles table\nrun `insta download <export.csv>` first")]
    MissingSchema,
    #[error(
        "search failed: {source}\n  tip: check for unmatched quotes. use FTS5 syntax: \"exact phrase\", word1 OR word2, NOT word3"
    )]
    Search {
        #[source]
        source: rusqlite::Error,
    },
    #[error("search result limit exceeds sqlite range")]
    LimitRange,
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
}

#[derive(Debug, thiserror::Error)]
pub enum CsvError {
    #[error(
        "file not found: {path}\n  export your bookmarks from https://www.instapaper.com/user"
    )]
    NotFound { path: PathBuf },
    #[error("failed to read csv file: {source}")]
    Read {
        #[source]
        source: csv::Error,
    },
    #[error(
        "invalid csv format: {source}\n  expected instapaper export with columns: URL, Title, Selection, Folder, Timestamp, Tags"
    )]
    Invalid {
        #[source]
        source: csv::Error,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum ExtractError {
    #[error(transparent)]
    Http(#[from] reqwest::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("decompressed response body exceeds {limit}-byte limit")]
    BodyTooLarge { limit: usize },
    #[error("decoded text exceeds {limit}-byte limit")]
    TextTooLarge { limit: usize },
    #[error("response body is {len} bytes (limit: {limit} bytes)")]
    ContentLengthTooLarge { len: u64, limit: usize },
    #[error("response body size overflow")]
    BodySizeOverflow,
    #[error("decoded text size overflow")]
    TextSizeOverflow,
    #[error("text decoder input offset overflow")]
    DecoderOffsetOverflow,
    #[error("text decoder made no progress")]
    DecoderStalled,
    #[error("failed to reserve decoded text buffer")]
    BufferReserve,
    #[error("text decoder produced invalid utf-8")]
    InvalidUtf8(#[from] std::string::FromUtf8Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Base64(#[from] base64::DecodeError),
}

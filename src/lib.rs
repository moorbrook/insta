//! Instapaper article downloader and search engine.

mod bounded;
mod commands;
mod config;
mod csv_reader;
mod db;
pub mod error;
mod extractor;
mod extractors;
mod filename;
mod html_extract;
mod paywall;
mod status;

pub use commands::{download, read, search, stats, DownloadReport, FailedUrl};
pub use config::{DownloadConfig, OutputMode, ReadConfig, SearchConfig, StatsConfig};
pub use db::{Article, SearchResult, StatusCounts};
pub use error::{Error, Result};
pub use status::ArticleStatus;

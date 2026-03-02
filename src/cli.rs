use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "insta", about = "Instapaper article downloader and search engine")]
pub struct Args {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Download articles from Instapaper CSV export
    Download {
        /// Path to Instapaper CSV export file
        csv_file: PathBuf,

        /// Output directory for downloaded articles
        #[arg(short, long, default_value = "articles")]
        output_dir: PathBuf,

        /// Maximum concurrent downloads
        #[arg(short = 'j', long, default_value_t = 20)]
        workers: usize,

        /// Maximum retries per article
        #[arg(short, long, default_value_t = 3)]
        retries: u32,

        /// HTTP request timeout in seconds
        #[arg(short, long, default_value_t = 30)]
        timeout: u64,

        /// Re-attempt previously failed articles
        #[arg(long)]
        retry_failed: bool,
    },

    /// Full-text search across all downloaded articles
    Search {
        /// Search query (supports FTS5 syntax: quotes for phrases, OR, NOT)
        query: Vec<String>,

        /// Path to articles directory containing index.db
        #[arg(short, long, default_value = "articles")]
        db_dir: PathBuf,

        /// Maximum number of results
        #[arg(short = 'n', long, default_value_t = 10)]
        limit: usize,
    },

    /// Read full article content by ID
    Read {
        /// Article ID (shown in search results as [ID])
        id: i64,

        /// Path to articles directory containing index.db
        #[arg(short, long, default_value = "articles")]
        db_dir: PathBuf,
    },

    /// Show statistics about the article database
    Stats {
        /// Path to articles directory containing index.db
        #[arg(short, long, default_value = "articles")]
        db_dir: PathBuf,
    },
}

use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "insta",
    version,
    about = "Archive and search an Instapaper library",
    after_help = "Examples:\n  insta download export.csv\n  insta search 'machine learning'\n  insta search 'rust' --json"
)]
pub(crate) struct Args {
    /// Output JSON instead of human-readable text
    #[arg(long, global = true)]
    pub json: bool,

    /// Increase log verbosity on stderr (-v, -vv, -vvv)
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    pub verbose: u8,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub(crate) enum Command {
    /// Download articles from an Instapaper CSV export
    Download {
        /// Path to Instapaper CSV export file
        csv_file: PathBuf,

        /// Output directory for downloaded articles and database
        #[arg(
            short = 'd',
            long = "dir",
            alias = "output-dir",
            alias = "db-dir",
            default_value = "articles"
        )]
        output_dir: PathBuf,

        /// Maximum concurrent downloads
        #[arg(short = 'j', long, default_value_t = 20, value_parser = clap::value_parser!(u32).range(1..))]
        workers: u32,

        /// Maximum retries per article
        #[arg(short, long, default_value_t = 3, value_parser = clap::value_parser!(u32).range(1..))]
        retries: u32,

        /// HTTP request timeout in seconds
        #[arg(short, long, default_value_t = 30, value_parser = clap::value_parser!(u64).range(1..))]
        timeout: u64,

        /// Re-attempt previously failed articles
        #[arg(long)]
        retry_failed: bool,
    },

    /// Full-text search across downloaded articles
    Search {
        /// Search query (FTS5: quotes, OR, NOT)
        #[arg(required = true, num_args = 1..)]
        query: Vec<String>,

        /// Path to articles directory containing index.db
        #[arg(
            short = 'd',
            long = "dir",
            alias = "db-dir",
            default_value = "articles"
        )]
        db_dir: PathBuf,

        /// Maximum number of results
        #[arg(short = 'n', long, default_value_t = 10)]
        limit: usize,
    },

    /// Read full article content by ID
    Read {
        /// Article ID (shown in search results as `[ID]`)
        id: i64,

        /// Path to articles directory containing index.db
        #[arg(
            short = 'd',
            long = "dir",
            alias = "db-dir",
            default_value = "articles"
        )]
        db_dir: PathBuf,
    },

    /// Show statistics about the article database
    Stats {
        /// Path to articles directory containing index.db
        #[arg(
            short = 'd',
            long = "dir",
            alias = "db-dir",
            default_value = "articles"
        )]
        db_dir: PathBuf,
    },
}

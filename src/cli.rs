use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "instapaper-dl", about = "Download articles from Instapaper CSV export")]
pub struct Args {
    /// Path to Instapaper CSV export file
    pub csv_file: PathBuf,

    /// Output directory for downloaded articles
    #[arg(short, long, default_value = "articles")]
    pub output_dir: PathBuf,

    /// Maximum concurrent downloads
    #[arg(short = 'j', long, default_value_t = 20)]
    pub workers: usize,

    /// Maximum retries per article
    #[arg(short, long, default_value_t = 3)]
    pub retries: u32,

    /// HTTP request timeout in seconds
    #[arg(short, long, default_value_t = 30)]
    pub timeout: u64,

    /// Re-attempt previously failed articles
    #[arg(long)]
    pub retry_failed: bool,
}

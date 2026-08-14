use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::time::Duration;

/// How command results are written to stdout.
///
/// JSON is opt-in via `--json`. Piped human output is preserved so existing
/// `insta search … | …` workflows keep working.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputMode {
    Human,
    Json,
}

impl OutputMode {
    #[must_use]
    pub fn from_json_flag(json: bool) -> Self {
        if json {
            Self::Json
        } else {
            Self::Human
        }
    }

    #[must_use]
    pub fn is_json(self) -> bool {
        matches!(self, Self::Json)
    }
}

#[derive(Debug, Clone)]
pub struct DownloadConfig {
    pub csv_file: PathBuf,
    pub output_dir: PathBuf,
    pub workers: NonZeroUsize,
    pub retries: u32,
    pub timeout: Duration,
    pub retry_failed: bool,
}

#[derive(Debug, Clone)]
pub struct SearchConfig {
    pub query: String,
    pub db_dir: PathBuf,
    pub limit: usize,
}

#[derive(Debug, Clone)]
pub struct ReadConfig {
    pub id: i64,
    pub db_dir: PathBuf,
}

#[derive(Debug, Clone)]
pub struct StatsConfig {
    pub db_dir: PathBuf,
}

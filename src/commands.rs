use std::sync::Arc;
use std::time::Instant;

use indicatif::{ProgressBar, ProgressStyle};
use serde::Serialize;

use crate::config::{DownloadConfig, OutputMode, ReadConfig, SearchConfig, StatsConfig};
use crate::csv_reader;
use crate::db::{Article, Database, SearchResult, StatusCounts};
use crate::error::Result;
use crate::extractor::{ExtractionResult, Extractor};

#[derive(Debug, Serialize)]
pub struct DownloadReport {
    pub counts: StatusCounts,
    pub elapsed_secs: f64,
    pub failed_urls: Vec<FailedUrl>,
    pub batch_success: u64,
    pub batch_failed: u64,
    pub skipped_all: bool,
}

impl DownloadReport {
    #[must_use]
    pub fn all_failed(&self) -> bool {
        !self.skipped_all && self.batch_success == 0 && self.batch_failed > 0
    }
}

#[derive(Debug, Serialize)]
pub struct FailedUrl {
    pub url: String,
    pub error: Option<String>,
}

#[allow(clippy::print_stdout, reason = "human-mode progress is program output")]
pub async fn download(config: DownloadConfig, output: OutputMode) -> Result<DownloadReport> {
    let start = Instant::now();

    tokio::fs::create_dir_all(&config.output_dir).await?;

    let db_path = config.output_dir.join("index.db");
    let db = Arc::new(Database::open(&db_path)?);
    db.init_schema()?;

    if output == OutputMode::Human {
        println!("Instapaper Article Downloader");
        println!("{}", "=".repeat(60));
        println!("Loading articles from {}...", config.csv_file.display());
    }

    let all_rows = csv_reader::read_csv(&config.csv_file)?;
    if output == OutputMode::Human {
        println!("Loaded {} articles from CSV", all_rows.len());
    }

    let to_process: Vec<_> = all_rows
        .into_iter()
        .filter_map(
            |row| match should_process(&db, &row.url, config.retry_failed) {
                Ok(true) => Some(Ok(row)),
                Ok(false) => None,
                Err(error) => Some(Err(error)),
            },
        )
        .collect::<Result<_>>()?;

    for row in &to_process {
        db.insert_pending(row)?;
    }

    if to_process.is_empty() {
        if output == OutputMode::Human {
            println!("No articles to download (all already processed)");
        }
        return Ok(DownloadReport {
            counts: db.get_status_counts()?,
            elapsed_secs: start.elapsed().as_secs_f64(),
            failed_urls: Vec::new(),
            batch_success: 0,
            batch_failed: 0,
            skipped_all: true,
        });
    }

    if output == OutputMode::Human {
        println!("Found {} articles to download", to_process.len());
        println!("Using {} concurrent workers", config.workers);
        println!("{}\n", "=".repeat(60));
    }

    let pb = ProgressBar::new(to_process.len() as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} ({percent}%) | {msg}")
            .unwrap_or_else(|_| ProgressStyle::default_bar())
            .progress_chars("##-"),
    );

    let mut success_count = 0_u64;
    let mut failed_count = 0_u64;

    let ext = Arc::new(Extractor::new(
        Arc::clone(&db),
        config.output_dir.clone(),
        config.retries,
        config.timeout,
    )?);

    crate::bounded::for_each_bounded(
        to_process,
        config.workers,
        move |row| {
            let ext = Arc::clone(&ext);
            async move { ext.process_article(&row).await }
        },
        |result| {
            match result {
                Ok(ExtractionResult::Success) => success_count += 1,
                Ok(ExtractionResult::Failed) => failed_count += 1,
                Err(error) => {
                    tracing::warn!(error = %error, "download task panicked");
                    failed_count += 1;
                }
            }
            pb.inc(1);
            pb.set_message(format!("OK: {success_count} Fail: {failed_count}"));
        },
    )
    .await;
    pb.finish_with_message("Done");

    let elapsed = start.elapsed().as_secs_f64();
    let counts = db.get_status_counts()?;
    let failed_urls = db
        .get_failed_urls(10)?
        .into_iter()
        .map(|(url, error)| FailedUrl { url, error })
        .collect();

    Ok(DownloadReport {
        counts,
        elapsed_secs: elapsed,
        failed_urls,
        batch_success: success_count,
        batch_failed: failed_count,
        skipped_all: false,
    })
}

fn should_process(db: &Database, url: &str, retry_failed: bool) -> Result<bool> {
    if db.is_already_successful(url)? {
        return Ok(false);
    }
    if !retry_failed && db.is_already_failed(url)? {
        return Ok(false);
    }
    Ok(true)
}

pub(crate) fn open_db(db_dir: &std::path::Path) -> Result<Database> {
    let db_path = db_dir.join("index.db");
    if !db_path.exists() {
        return Err(crate::error::DatabaseError::Missing { path: db_path }.into());
    }
    let db = Database::open(&db_path)?;
    db.ensure_schema()?;
    Ok(db)
}

pub fn search(config: &SearchConfig) -> Result<Vec<SearchResult>> {
    let db = open_db(&config.db_dir)?;
    Ok(db.search(&config.query, config.limit)?)
}

pub fn read(config: &ReadConfig) -> Result<Option<Article>> {
    let db = open_db(&config.db_dir)?;
    Ok(db.read_by_id(config.id)?)
}

pub fn stats(config: &StatsConfig) -> Result<StatusCounts> {
    let db = open_db(&config.db_dir)?;
    Ok(db.get_status_counts()?)
}

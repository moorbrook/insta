use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;

use crate::csv_reader::ArticleRow;
use crate::db::Database;
use crate::extractors::{archive, github, instapaper, readability, youtube, ExtractedArticle};
use crate::filename::make_filename;
use crate::paywall::{get_paywalled_domain, is_paywalled};

/// Domains known to block scrapers - try archive.ph first
fn is_scraper_hostile(url: &str) -> bool {
    url.contains("medium.com") || url.contains("towardsdatascience.com")
}

pub enum ExtractionResult {
    Success { filename: String },
    Failed { error: String },
}

pub struct Extractor {
    client: reqwest::Client,
    db: Arc<Database>,
    output_dir: PathBuf,
    semaphore: Arc<Semaphore>,
    retries: u32,
    timeout: Duration,
}

impl Extractor {
    pub fn new(
        db: Arc<Database>,
        output_dir: PathBuf,
        workers: u32,
        retries: u32,
        timeout_secs: u64,
    ) -> Self {
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::limited(10))
            .build()
            .expect("Failed to build HTTP client");

        Self {
            client,
            db,
            output_dir,
            semaphore: Arc::new(Semaphore::new(workers as usize)),
            retries,
            timeout: Duration::from_secs(timeout_secs),
        }
    }

    pub async fn process_article(&self, row: &ArticleRow) -> ExtractionResult {
        let _permit = self.semaphore.acquire().await.unwrap();

        let mut last_error = String::new();

        for attempt in 0..self.retries {
            let try_wayback = attempt == self.retries - 1;

            match self.extract_article(&row.url, try_wayback).await {
                Ok(Some(article)) => {
                    let final_title = if article.title != "Untitled" && !article.title.is_empty() {
                        &article.title
                    } else if !row.title.is_empty() {
                        &row.title
                    } else {
                        "Untitled"
                    };

                    let filename = make_filename(&row.url, final_title);
                    let filepath = self.output_dir.join(&filename);

                    if let Err(e) = tokio::fs::write(&filepath, &article.content).await {
                        last_error = format!("Failed to write file: {e}");
                        continue;
                    }

                    let word_count = article.content.split_whitespace().count() as i64;

                    let is_archived = article.content.contains("Internet Archive Wayback Machine")
                        || article.content.contains("Archive.ph");

                    if let Err(e) = self.db.mark_success(
                        &row.url,
                        final_title,
                        &filename,
                        &article.content,
                        word_count,
                        is_archived,
                    ) {
                        last_error = format!("DB error: {e}");
                        continue;
                    }

                    return ExtractionResult::Success { filename };
                }
                Ok(None) => {
                    if attempt == self.retries - 1 {
                        last_error = if let Some(domain) = get_paywalled_domain(&row.url) {
                            format!("Paywalled site ({domain}) - no archive available")
                        } else {
                            "Extraction returned no content".to_string()
                        };
                    }
                }
                Err(e) => {
                    last_error = e.to_string();
                }
            }

            if attempt < self.retries - 1 {
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }

        let _ = self.db.mark_failed(&row.url, &last_error);
        ExtractionResult::Failed { error: last_error }
    }

    async fn extract_article(
        &self,
        url: &str,
        try_archive: bool,
    ) -> anyhow::Result<Option<ExtractedArticle>> {
        // 1. YouTube -> transcript
        if youtube::is_youtube(url) {
            if let Some(article) = youtube::extract(url, self.timeout).await? {
                return Ok(Some(article));
            }
        }

        // 2. GitHub (repos + blob paths) -> raw content / README
        if github::is_github(url) {
            if let Some(article) = github::extract(&self.client, url, self.timeout).await? {
                return Ok(Some(article));
            }
        }

        // 3. Medium/scraper-hostile sites -> try archive.ph first
        if is_scraper_hostile(url) {
            if let Some(article) = archive::extract(&self.client, url, self.timeout).await? {
                return Ok(Some(article));
            }
        }

        // 4. Paywalled sites -> try Instapaper API (requires OAuth setup via `insta login`)
        if is_paywalled(url) {
            if let Some(article) =
                instapaper::extract(&self.client, url, self.timeout).await?
            {
                return Ok(Some(article));
            }
        }

        // 5. Primary extraction (multi-tier HTML pipeline)
        if let Some(article) = readability::extract(&self.client, url, self.timeout).await? {
            return Ok(Some(article));
        }

        // 6. Archive.ph / Wayback fallback for ALL failures on final retry
        if try_archive {
            if let Some(article) = archive::extract(&self.client, url, self.timeout).await? {
                return Ok(Some(article));
            }
        }

        Ok(None)
    }
}

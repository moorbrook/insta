use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use crate::csv_reader::ArticleRow;
use crate::db::Database;
use crate::error::ExtractError;
use crate::extractors::{archive, github, instapaper, readability, youtube, ExtractedArticle};
use crate::filename::make_filename;
use crate::paywall::{get_paywalled_domain, is_paywalled};
use crate::status::SuccessSource;

/// Domains known to block scrapers - try archive.ph first
fn is_scraper_hostile(url: &str) -> bool {
    crate::extractors::host_is_domain_or_subdomain(url, "medium.com")
        || crate::extractors::host_is_domain_or_subdomain(url, "towardsdatascience.com")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExtractionResult {
    Success,
    Failed,
}

#[derive(Debug)]
pub(crate) struct Extractor {
    client: reqwest::Client,
    db: Arc<Database>,
    output_dir: PathBuf,
    retries: u32,
    timeout: Duration,
}

impl Extractor {
    pub(crate) fn new(
        db: Arc<Database>,
        output_dir: PathBuf,
        retries: u32,
        timeout: Duration,
    ) -> Result<Self, ExtractError> {
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::limited(10))
            .build()?;

        Ok(Self {
            client,
            db,
            output_dir,
            retries,
            timeout,
        })
    }

    pub(crate) async fn process_article(&self, row: &ArticleRow) -> ExtractionResult {
        tracing::debug!(url = %row.url, "extracting article");
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

                    if let Err(error) = tokio::fs::write(&filepath, &article.content).await {
                        last_error = format!("Failed to write file: {error}");
                        continue;
                    }

                    let word_count = i64::try_from(article.content.split_whitespace().count())
                        .unwrap_or(i64::MAX);

                    let source = if article.content.contains("Internet Archive Wayback Machine")
                        || article.content.contains("Archive.ph")
                    {
                        SuccessSource::Archive
                    } else {
                        SuccessSource::Live
                    };

                    let mut db_ok = false;
                    for _db_attempt in 0..3 {
                        if self
                            .db
                            .mark_success(
                                &row.url,
                                final_title,
                                &filename,
                                &article.content,
                                word_count,
                                source,
                            )
                            .is_ok()
                        {
                            db_ok = true;
                            break;
                        }
                        tokio::time::sleep(Duration::from_millis(100)).await;
                    }
                    if !db_ok {
                        last_error = format!("DB error after successful download of {}", row.url);
                        break;
                    }

                    return ExtractionResult::Success;
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
                Err(error) => {
                    last_error = error.to_string();
                }
            }

            if attempt < self.retries - 1 {
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }

        if let Err(error) = self.db.mark_failed(&row.url, &last_error) {
            tracing::warn!(url = %row.url, error = %error, "failed to update database");
        }
        ExtractionResult::Failed
    }

    async fn extract_article(
        &self,
        url: &str,
        try_archive: bool,
    ) -> Result<Option<ExtractedArticle>, ExtractError> {
        if youtube::is_youtube(url) {
            if let Some(article) = youtube::extract(url, self.timeout).await? {
                return Ok(Some(article));
            }
        }

        if github::is_github(url) {
            if let Some(article) = github::extract(&self.client, url, self.timeout).await? {
                return Ok(Some(article));
            }
        }

        if is_scraper_hostile(url) {
            if let Some(article) = archive::extract(&self.client, url, self.timeout).await? {
                return Ok(Some(article));
            }
        }

        if is_paywalled(url) {
            if let Some(article) = instapaper::extract(&self.client, url, self.timeout).await? {
                return Ok(Some(article));
            }
        }

        if let Some(article) = readability::extract(&self.client, url, self.timeout).await? {
            return Ok(Some(article));
        }

        if try_archive {
            if let Some(article) = archive::extract(&self.client, url, self.timeout).await? {
                return Ok(Some(article));
            }
        }

        Ok(None)
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

    use super::is_scraper_hostile;

    #[test]
    fn scraper_hostile_routing_uses_the_destination_host() {
        assert!(is_scraper_hostile("https://medium.com/example/story"));
        assert!(is_scraper_hostile(
            "https://blog.towardsdatascience.com/example"
        ));
        assert!(!is_scraper_hostile(
            "https://example.com/?next=https://medium.com/story"
        ));
        assert!(!is_scraper_hostile("https://medium.com.evil.example/story"));
    }
}

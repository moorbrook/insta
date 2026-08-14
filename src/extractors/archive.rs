use super::{article_from_html, fetch_html, host_is_domain_or_subdomain, ExtractedArticle};
use crate::error::ExtractError;
use serde::Deserialize;
use std::time::Duration;

#[derive(Deserialize)]
struct WaybackResponse {
    archived_snapshots: ArchivedSnapshots,
}

#[derive(Deserialize)]
struct ArchivedSnapshots {
    closest: Option<ClosestSnapshot>,
}

#[derive(Deserialize)]
struct ClosestSnapshot {
    available: bool,
    url: String,
}

enum ArchiveSource {
    ArchivePh,
    Wayback,
}

async fn get_archive_snapshot(
    client: &reqwest::Client,
    url: &str,
) -> Option<(String, ArchiveSource)> {
    let archive_url = format!("https://archive.ph/newest/{url}");
    if let Ok(response) = client
        .get(&archive_url)
        .header("User-Agent", super::USER_AGENT)
        .timeout(Duration::from_secs(10))
        .send()
        .await
    {
        if response.status().is_success() {
            let final_url = response.url().to_string();
            if host_is_domain_or_subdomain(&final_url, "archive.ph") {
                return Some((final_url, ArchiveSource::ArchivePh));
            }
        }
    }

    let api_url = format!("https://archive.org/wayback/available?url={url}");
    if let Ok(response) = client
        .get(&api_url)
        .timeout(Duration::from_secs(10))
        .send()
        .await
    {
        if let Ok(data) = response.json::<WaybackResponse>().await {
            if let Some(snapshot) = data.archived_snapshots.closest {
                if snapshot.available {
                    return Some((snapshot.url, ArchiveSource::Wayback));
                }
            }
        }
    }

    None
}

pub(crate) async fn extract(
    client: &reqwest::Client,
    url: &str,
    timeout: Duration,
) -> Result<Option<ExtractedArticle>, ExtractError> {
    let Some((snapshot_url, source)) = get_archive_snapshot(client, url).await else {
        return Ok(None);
    };

    let Some(html) = fetch_html(client, &snapshot_url, timeout).await? else {
        return Ok(None);
    };

    let Some(mut article) = article_from_html(&html, &snapshot_url) else {
        return Ok(None);
    };

    let source_name = match source {
        ArchiveSource::ArchivePh => "Archive.ph",
        ArchiveSource::Wayback => "Internet Archive Wayback Machine",
    };
    article.content = format!(
        "{}\n\n---\nNote: This article was retrieved from {source_name}\nOriginal URL: {url}\nArchive URL: {snapshot_url}\n",
        article.content
    );
    Ok(Some(article))
}

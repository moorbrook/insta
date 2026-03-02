use super::ExtractedArticle;
use serde::Deserialize;
use std::time::Duration;

const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/91.0.4472.124 Safari/537.36";

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
    // Try archive.ph first
    let archive_url = format!("https://archive.ph/newest/{url}");
    if let Ok(response) = client
        .get(&archive_url)
        .header("User-Agent", USER_AGENT)
        .timeout(Duration::from_secs(10))
        .send()
        .await
    {
        if response.status().is_success() {
            let final_url = response.url().to_string();
            if final_url.contains("archive.ph") {
                return Some((final_url, ArchiveSource::ArchivePh));
            }
        }
    }

    // Fallback to Wayback Machine
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

pub async fn extract(
    client: &reqwest::Client,
    url: &str,
    timeout: Duration,
) -> anyhow::Result<Option<ExtractedArticle>> {
    let (snapshot_url, source) = match get_archive_snapshot(client, url).await {
        Some(s) => s,
        None => return Ok(None),
    };

    // Fetch the archived page
    let response = client
        .get(&snapshot_url)
        .timeout(timeout)
        .header("User-Agent", USER_AGENT)
        .send()
        .await?;

    if !response.status().is_success() {
        return Ok(None);
    }

    let html = response.text().await?;

    // Use trafilatura-style extraction on the archived page
    match crate::trafilatura::extract(&html, &snapshot_url) {
        Some(result) => {
            let title = if result.title.is_empty() || result.title == "Untitled" {
                "Untitled".to_string()
            } else {
                result.title
            };
            let source_name = match source {
                ArchiveSource::ArchivePh => "Archive.ph",
                ArchiveSource::Wayback => "Internet Archive Wayback Machine",
            };
            let content = format!(
                "{}\n\n---\nNote: This article was retrieved from {source_name}\nOriginal URL: {url}\nArchive URL: {snapshot_url}\n",
                result.text
            );
            Ok(Some(ExtractedArticle { title, content }))
        }
        None => Ok(None),
    }
}

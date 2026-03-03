use super::ExtractedArticle;
use regex::Regex;
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::process::Command;

static YT_DLP_WARNED: AtomicBool = AtomicBool::new(false);

pub fn is_youtube(url: &str) -> bool {
    url.contains("youtube.com") || url.contains("youtu.be")
}

pub async fn extract(url: &str, timeout: Duration) -> anyhow::Result<Option<ExtractedArticle>> {
    // Check if yt-dlp is available
    if Command::new("yt-dlp").arg("--version").output().await.is_err() {
        if !YT_DLP_WARNED.swap(true, Ordering::Relaxed) {
            eprintln!("Warning: yt-dlp is not installed. YouTube transcripts will be skipped.");
            eprintln!("  Install: uv tool install yt-dlp  (https://docs.astral.sh/uv)");
        }
        return Ok(None);
    }

    let tmpdir = tempfile::tempdir()?;
    let output_template = tmpdir.path().join("transcript");

    // Try auto-generated subtitles first
    let result = Command::new("yt-dlp")
        .args([
            "--write-auto-sub",
            "--skip-download",
            "--sub-langs",
            "en",
            "--output",
            output_template.to_str().unwrap(),
            url,
        ])
        .output();

    let output = tokio::time::timeout(timeout, result).await;
    let success = matches!(&output, Ok(Ok(o)) if o.status.success());

    // Fallback to manual subtitles
    if !success {
        let result = Command::new("yt-dlp")
            .args([
                "--write-sub",
                "--skip-download",
                "--sub-langs",
                "en",
                "--output",
                output_template.to_str().unwrap(),
                url,
            ])
            .output();

        let output = tokio::time::timeout(timeout, result).await;
        if !matches!(&output, Ok(Ok(o)) if o.status.success()) {
            return Ok(None);
        }
    }

    // Find VTT file
    let mut vtt_path = None;
    let mut entries = tokio::fs::read_dir(tmpdir.path()).await?;
    while let Some(entry) = entries.next_entry().await? {
        if entry.file_name().to_string_lossy().ends_with(".vtt") {
            vtt_path = Some(entry.path());
            break;
        }
    }

    let vtt_path = match vtt_path {
        Some(p) => p,
        None => return Ok(None),
    };

    // Get video title
    let title_output = Command::new("yt-dlp")
        .args(["--print", "%(title)s", url])
        .output()
        .await;

    let title = match title_output {
        Ok(o) if o.status.success() => {
            String::from_utf8_lossy(&o.stdout).trim().to_string()
        }
        _ => "YouTube Video".to_string(),
    };

    // Parse VTT to plain text with deduplication
    let vtt_content = tokio::fs::read_to_string(&vtt_path).await?;
    let re_html = Regex::new("<[^>]*>")?;
    let mut seen = HashSet::new();
    let mut lines = Vec::new();

    for line in vtt_content.lines() {
        let line = line.trim();
        if line.is_empty()
            || line.starts_with("WEBVTT")
            || line.starts_with("Kind:")
            || line.starts_with("Language:")
            || line.contains("-->")
        {
            continue;
        }
        let clean = re_html.replace_all(line, "");
        let clean = clean
            .replace("&amp;", "&")
            .replace("&gt;", ">")
            .replace("&lt;", "<");
        let clean = clean.trim().to_string();
        if !clean.is_empty() && seen.insert(clean.clone()) {
            lines.push(clean);
        }
    }

    if lines.is_empty() {
        return Ok(None);
    }

    Ok(Some(ExtractedArticle {
        title,
        content: lines.join("\n"),
    }))
}

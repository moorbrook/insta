use super::ExtractedArticle;
use crate::error::ExtractError;
use regex::Regex;
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::LazyLock;
use std::time::Duration;
use tokio::process::Command;

static YT_DLP_WARNED: AtomicBool = AtomicBool::new(false);
static YT_DLP_STATE: AtomicU8 = AtomicU8::new(0); // 0 unknown, 1 yes, 2 no

#[allow(clippy::expect_used, reason = "hardcoded regex must compile")]
static VTT_HTML_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("<[^>]*>").expect("VTT tag regex must compile"));

pub(crate) fn is_youtube(url: &str) -> bool {
    super::host_is_domain_or_subdomain(url, "youtube.com")
        || super::host_is_domain_or_subdomain(url, "youtu.be")
}

fn yt_dlp_command() -> Command {
    let mut command = Command::new("yt-dlp");
    command.kill_on_drop(true);
    command
}

async fn yt_dlp_available(timeout: Duration) -> bool {
    match YT_DLP_STATE.load(Ordering::Relaxed) {
        1 => return true,
        2 => return false,
        _ => {}
    }

    let available = yt_dlp_output(timeout, &["--version"]).await.is_some();
    YT_DLP_STATE.store(if available { 1 } else { 2 }, Ordering::Relaxed);
    if !available && !YT_DLP_WARNED.swap(true, Ordering::Relaxed) {
        tracing::warn!("yt-dlp is not installed; YouTube transcripts will be skipped");
        tracing::warn!("install with: uv tool install yt-dlp  (https://docs.astral.sh/uv)");
    }
    available
}

async fn yt_dlp_output(timeout: Duration, args: &[&str]) -> Option<std::process::Output> {
    let result = tokio::time::timeout(timeout, yt_dlp_command().args(args).output()).await;
    match result {
        Ok(Ok(output)) if output.status.success() => Some(output),
        _ => None,
    }
}

async fn yt_dlp_succeeds(
    timeout: Duration,
    prefix: &[&str],
    output_template: &std::path::Path,
    url: &str,
) -> bool {
    let result = tokio::time::timeout(
        timeout,
        yt_dlp_command()
            .args(prefix)
            .arg(output_template)
            .arg(url)
            .output(),
    )
    .await;
    matches!(result, Ok(Ok(output)) if output.status.success())
}

pub(crate) async fn extract(
    url: &str,
    timeout: Duration,
) -> Result<Option<ExtractedArticle>, ExtractError> {
    if !yt_dlp_available(timeout).await {
        return Ok(None);
    }

    let tmpdir = tempfile::tempdir()?;
    let output_template = tmpdir.path().join("transcript");

    let success = yt_dlp_succeeds(
        timeout,
        &[
            "--write-auto-sub",
            "--skip-download",
            "--sub-langs",
            "en",
            "--output",
        ],
        &output_template,
        url,
    )
    .await;

    if !success {
        let manual = yt_dlp_succeeds(
            timeout,
            &[
                "--write-sub",
                "--skip-download",
                "--sub-langs",
                "en",
                "--output",
            ],
            &output_template,
            url,
        )
        .await;
        if !manual {
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

    let Some(vtt_path) = vtt_path else {
        return Ok(None);
    };

    // Get video title
    let title = match yt_dlp_output(timeout, &["--print", "%(title)s", url]).await {
        Some(output) => String::from_utf8_lossy(&output.stdout).trim().to_string(),
        None => "YouTube Video".to_string(),
    };

    // Parse VTT to plain text with deduplication
    let vtt_content = tokio::fs::read_to_string(&vtt_path).await?;
    let Some(content) = parse_vtt(&vtt_content) else {
        return Ok(None);
    };

    Ok(Some(ExtractedArticle { title, content }))
}

fn parse_vtt(vtt_content: &str) -> Option<String> {
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
        let clean = VTT_HTML_RE.replace_all(line, "");
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
        return None;
    }

    Some(lines.join("\n"))
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

    use super::{is_youtube, parse_vtt};

    #[test]
    fn youtube_classification_uses_the_destination_host() {
        assert!(is_youtube("https://youtu.be/abc"));
        assert!(is_youtube("https://m.youtube.com/watch?v=abc"));
        assert!(!is_youtube("https://example.com/?next=youtube.com"));
        assert!(!is_youtube("https://youtube.com.evil.example/watch"));
    }

    #[test]
    fn vtt_parser_filters_metadata_tags_and_duplicate_cues() {
        let vtt = r"WEBVTT
Kind: captions
Language: en

00:00:00.000 --> 00:00:01.000
<c>Hello &amp; welcome</c>

00:00:01.000 --> 00:00:02.000
Hello &amp; welcome

00:00:02.000 --> 00:00:03.000
Rust &lt;3 agents
";

        assert_eq!(
            parse_vtt(vtt).as_deref(),
            Some("Hello & welcome\nRust <3 agents")
        );
    }

    #[test]
    fn duplicate_cues_are_a_neutral_transformation() {
        let base = "WEBVTT\n\n00:00.000 --> 00:01.000\nOne line";
        let duplicated =
            "WEBVTT\n\n00:00.000 --> 00:01.000\nOne line\n\n00:01.000 --> 00:02.000\nOne line";

        assert_eq!(parse_vtt(base), parse_vtt(duplicated));
    }

    #[test]
    fn metadata_only_vtt_has_no_transcript() {
        assert_eq!(parse_vtt("WEBVTT\nKind: captions\nLanguage: en"), None);
    }
}

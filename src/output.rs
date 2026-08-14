use std::io::{self, Write};

use instapaper_dl::{Article, DownloadReport, OutputMode, SearchResult, StatusCounts};
use serde::Serialize;

#[allow(clippy::print_stdout, reason = "program output, not diagnostics")]
pub(crate) fn download_report(mode: OutputMode, report: &DownloadReport) -> io::Result<()> {
    match mode {
        OutputMode::Json => emit_json(report),
        OutputMode::Human => {
            if report.skipped_all {
                return Ok(());
            }
            print_download_summary(report);
            Ok(())
        }
    }
}

#[allow(clippy::print_stdout, reason = "program output, not diagnostics")]
pub(crate) fn search_results(
    mode: OutputMode,
    query: &str,
    results: &[SearchResult],
) -> io::Result<()> {
    match mode {
        OutputMode::Json => emit_json(&SearchPayload {
            query,
            count: results.len(),
            results,
        }),
        OutputMode::Human => {
            if results.is_empty() {
                println!("No results for: {query}");
                return Ok(());
            }

            println!("Found {} result(s) for: {query}\n", results.len());

            let highlight = io::IsTerminal::is_terminal(&io::stdout());
            for result in results {
                let title = result.title.as_deref().unwrap_or("Untitled");
                let words = result
                    .word_count
                    .map(|count| format!("{count} words"))
                    .unwrap_or_default();
                let folder = result.folder.as_deref().unwrap_or("");

                println!("[{}] {title}", result.id);
                println!("    {}", result.url);
                if !folder.is_empty() || !words.is_empty() {
                    let meta: Vec<&str> = [folder, words.as_str()]
                        .into_iter()
                        .filter(|value| !value.is_empty())
                        .collect();
                    println!("    [{}]", meta.join(" | "));
                }

                let snippet = if highlight {
                    result
                        .snippet
                        .replace(">>>", "\x1b[1;33m")
                        .replace("<<<", "\x1b[0m")
                } else {
                    result.snippet.replace(">>>", "").replace("<<<", "")
                };
                println!("    {snippet}");
                println!();
            }

            println!("Use `insta read <ID>` to view full article content.");
            Ok(())
        }
    }
}

#[allow(clippy::print_stdout, reason = "program output, not diagnostics")]
pub(crate) fn article(mode: OutputMode, id: i64, article: Option<&Article>) -> io::Result<()> {
    match mode {
        OutputMode::Json => emit_json(&ReadPayload { id, article }),
        OutputMode::Human => {
            if let Some(article) = article {
                let title = article.title.as_deref().unwrap_or("Untitled");
                let words = article.word_count.unwrap_or(0);
                println!("{title}");
                println!("{}", article.url);
                println!("[{words} words]\n");
                println!("{}", "=".repeat(60));
                match article.content.as_deref() {
                    Some(content) if !content.is_empty() => println!("{content}"),
                    _ => println!("(no content available)"),
                }
                Ok(())
            } else {
                println!("No article found with ID {id}");
                Ok(())
            }
        }
    }
}

#[allow(clippy::print_stdout, reason = "program output, not diagnostics")]
pub(crate) fn stats(mode: OutputMode, counts: &StatusCounts) -> io::Result<()> {
    match mode {
        OutputMode::Json => emit_json(counts),
        OutputMode::Human => {
            println!("Instapaper Archive Stats");
            println!("{}", "=".repeat(40));
            println!("Total articles:  {}", counts.total);
            if counts.total > 0 {
                println!(
                    "Successful:      {} ({:.1}%)",
                    counts.success,
                    percentage(counts.success, counts.total)
                );
                if counts.archived > 0 {
                    println!("  From Archives: {}", counts.archived);
                }
                println!(
                    "Failed:          {} ({:.1}%)",
                    counts.failed,
                    percentage(counts.failed, counts.total)
                );
                println!("Pending:         {}", counts.pending);
                println!("Total words:     {}", counts.total_words);
            }
            println!("{}", "=".repeat(40));
            Ok(())
        }
    }
}

#[allow(clippy::print_stdout, reason = "program output, not diagnostics")]
fn print_download_summary(report: &DownloadReport) {
    let counts = &report.counts;
    println!("\n{}", "=".repeat(60));
    println!("DOWNLOAD SUMMARY");
    println!("{}", "=".repeat(60));
    println!("Total articles:     {}", counts.total);
    if counts.total > 0 {
        println!(
            "Successfully saved: {} ({:.1}%)",
            counts.success,
            percentage(counts.success, counts.total)
        );
        if counts.archived > 0 {
            println!("  From Archives:    {}", counts.archived);
        }
        println!(
            "Failed:             {} ({:.1}%)",
            counts.failed,
            percentage(counts.failed, counts.total)
        );
        println!("Pending:            {}", counts.pending);
        println!("Total words:        {}", counts.total_words);
        println!(
            "Time elapsed:       {:.1}s ({:.1} minutes)",
            report.elapsed_secs,
            report.elapsed_secs / 60.0
        );
        println!(
            "Average:            {:.2}s per article",
            report.elapsed_secs / count_as_f64(counts.total)
        );
    }
    println!("{}", "=".repeat(60));

    if !report.failed_urls.is_empty() {
        println!("\nFailed URLs (first 10):");
        for failed in &report.failed_urls {
            println!("  - {}", failed.url);
            if let Some(error) = &failed.error {
                let truncated: String = error.chars().take(100).collect();
                println!("    Error: {truncated}");
            }
        }
    }
}

fn emit_json<T: Serialize>(value: &T) -> io::Result<()> {
    let mut stdout = io::stdout().lock();
    serde_json::to_writer(&mut stdout, value).map_err(io::Error::other)?;
    stdout.write_all(b"\n")?;
    Ok(())
}

fn percentage(part: i64, total: i64) -> f64 {
    count_as_f64(part) / count_as_f64(total) * 100.0
}

#[allow(
    clippy::cast_precision_loss,
    reason = "SQLite row counts are far below f64's exact integer range in this local archive"
)]
fn count_as_f64(count: i64) -> f64 {
    count as f64
}

#[derive(Serialize)]
struct SearchPayload<'a> {
    query: &'a str,
    count: usize,
    results: &'a [SearchResult],
}

#[derive(Serialize)]
struct ReadPayload<'a> {
    id: i64,
    article: Option<&'a Article>,
}

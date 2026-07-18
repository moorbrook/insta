mod bounded;
mod cli;
mod csv_reader;
mod db;
mod extractor;
mod extractors;
mod filename;
mod html_extract;
mod paywall;

use clap::Parser;
use cli::Command;
use extractor::ExtractionResult;
use indicatif::{ProgressBar, ProgressStyle};
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::time::Instant;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = cli::Args::parse();

    match args.command {
        Command::Download {
            csv_file,
            output_dir,
            workers,
            retries,
            timeout,
            retry_failed,
        } => {
            cmd_download(
                csv_file,
                output_dir,
                workers,
                retries,
                timeout,
                retry_failed,
            )
            .await
        }
        Command::Search {
            query,
            db_dir,
            limit,
        } => cmd_search(&query.join(" "), &db_dir, limit),
        Command::Read { id, db_dir } => cmd_read(id, &db_dir),
        Command::Stats { db_dir } => cmd_stats(&db_dir),
    }
}

async fn cmd_download(
    csv_file: std::path::PathBuf,
    output_dir: std::path::PathBuf,
    workers: u32,
    retries: u32,
    timeout: u64,
    retry_failed: bool,
) -> anyhow::Result<()> {
    let start = Instant::now();

    tokio::fs::create_dir_all(&output_dir).await?;

    let db_path = output_dir.join("index.db");
    let db = Arc::new(db::Database::open(&db_path)?);
    db.init_schema()?;

    println!("Instapaper Article Downloader");
    println!("{}", "=".repeat(60));

    println!("Loading articles from {}...", csv_file.display());
    let all_rows = csv_reader::read_csv(&csv_file)?;
    println!("Loaded {} articles from CSV", all_rows.len());

    let to_process: Vec<_> = all_rows
        .into_iter()
        .filter_map(|row| match should_process(&db, &row.url, retry_failed) {
            Ok(true) => Some(Ok(row)),
            Ok(false) => None,
            Err(error) => Some(Err(error)),
        })
        .collect::<anyhow::Result<_>>()?;

    for row in &to_process {
        db.insert_pending(row)?;
    }

    if to_process.is_empty() {
        println!("No articles to download (all already processed)");
        return Ok(());
    }

    println!("Found {} articles to download", to_process.len());
    println!("Using {} concurrent workers", workers);
    println!("{}\n", "=".repeat(60));

    let pb = ProgressBar::new(to_process.len() as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} ({percent}%) | {msg}")
            .unwrap()
            .progress_chars("##-"),
    );

    let worker_limit = NonZeroUsize::new(workers as usize)
        .ok_or_else(|| anyhow::anyhow!("worker count must be greater than zero"))?;
    let mut success_count = 0_u64;
    let mut failed_count = 0_u64;

    let ext = Arc::new(extractor::Extractor::new(
        db.clone(),
        output_dir.clone(),
        retries,
        timeout,
    ));

    bounded::for_each_bounded(
        to_process,
        worker_limit,
        move |row| {
            let ext = Arc::clone(&ext);
            async move { ext.process_article(&row).await }
        },
        |result| {
            match result {
                Ok(ExtractionResult::Success) => success_count += 1,
                Ok(ExtractionResult::Failed) => failed_count += 1,
                Err(error) => {
                    eprintln!("Warning: download task panicked: {error}");
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
    print_report(&db, elapsed)?;

    if success_count == 0 && failed_count > 0 {
        anyhow::bail!(
            "All {failed_count} article(s) failed to download. Run with --retry-failed to try again."
        );
    }

    Ok(())
}

fn should_process(db: &db::Database, url: &str, retry_failed: bool) -> anyhow::Result<bool> {
    if db.is_already_successful(url)? {
        return Ok(false);
    }
    if !retry_failed && db.is_already_failed(url)? {
        return Ok(false);
    }
    Ok(true)
}

fn open_db(db_dir: &std::path::Path) -> anyhow::Result<db::Database> {
    let db_path = db_dir.join("index.db");
    if !db_path.exists() {
        anyhow::bail!(
            "No database found at {}\nRun `insta download <export.csv>` first to build the article index.",
            db_path.display()
        );
    }
    let db = db::Database::open(&db_path)?;
    db.ensure_schema()?;
    Ok(db)
}

fn cmd_search(query: &str, db_dir: &std::path::Path, limit: usize) -> anyhow::Result<()> {
    let db = open_db(db_dir)?;

    let results = db.search(query, limit).map_err(|e| {
        anyhow::anyhow!(
            "Search failed: {e}\n  Tip: check for unmatched quotes. Use FTS5 syntax: \"exact phrase\", word1 OR word2, NOT word3"
        )
    })?;

    if results.is_empty() {
        println!("No results for: {query}");
        return Ok(());
    }

    println!("Found {} result(s) for: {query}\n", results.len());

    for r in &results {
        let title = r.title.as_deref().unwrap_or("Untitled");
        let words = r
            .word_count
            .map(|w| format!("{w} words"))
            .unwrap_or_default();
        let folder = r.folder.as_deref().unwrap_or("");

        println!("[{}] {}", r.id, title);
        println!("    {}", r.url);
        if !folder.is_empty() || !words.is_empty() {
            let meta: Vec<&str> = [folder, &words]
                .into_iter()
                .filter(|s| !s.is_empty())
                .collect();
            println!("    [{}]", meta.join(" | "));
        }

        let snippet = if std::io::IsTerminal::is_terminal(&std::io::stdout()) {
            r.snippet
                .replace(">>>", "\x1b[1;33m")
                .replace("<<<", "\x1b[0m")
        } else {
            r.snippet.replace(">>>", "").replace("<<<", "")
        };
        println!("    {snippet}");
        println!();
    }

    println!("Use `insta read <ID>` to view full article content.");

    Ok(())
}

fn cmd_read(id: i64, db_dir: &std::path::Path) -> anyhow::Result<()> {
    let db = open_db(db_dir)?;

    match db.read_by_id(id)? {
        Some(article) => {
            let title = article.title.as_deref().unwrap_or("Untitled");
            let words = article.word_count.unwrap_or(0);
            println!("{title}");
            println!("{}", article.url);
            println!("[{words} words]\n");
            println!("{}", "=".repeat(60));
            match article.content {
                Some(content) if !content.is_empty() => println!("{content}"),
                _ => println!("(no content available)"),
            }
        }
        None => {
            println!("No article found with ID {id}");
        }
    }

    Ok(())
}

fn cmd_stats(db_dir: &std::path::Path) -> anyhow::Result<()> {
    let db = open_db(db_dir)?;
    let counts = db.get_status_counts()?;

    println!("Instapaper Archive Stats");
    println!("{}", "=".repeat(40));
    println!("Total articles:  {}", counts.total);
    if counts.total > 0 {
        println!(
            "Successful:      {} ({:.1}%)",
            counts.success,
            counts.success as f64 / counts.total as f64 * 100.0
        );
        if counts.archived > 0 {
            println!("  From Archives: {}", counts.archived);
        }
        println!(
            "Failed:          {} ({:.1}%)",
            counts.failed,
            counts.failed as f64 / counts.total as f64 * 100.0
        );
        println!("Pending:         {}", counts.pending);
        println!("Total words:     {}", counts.total_words);
    }
    println!("{}", "=".repeat(40));

    Ok(())
}

fn print_report(db: &db::Database, elapsed: f64) -> anyhow::Result<()> {
    let counts = db.get_status_counts()?;

    println!("\n{}", "=".repeat(60));
    println!("DOWNLOAD SUMMARY");
    println!("{}", "=".repeat(60));
    println!("Total articles:     {}", counts.total);
    if counts.total > 0 {
        println!(
            "Successfully saved: {} ({:.1}%)",
            counts.success,
            counts.success as f64 / counts.total as f64 * 100.0
        );
        if counts.archived > 0 {
            println!("  From Archives:    {}", counts.archived);
        }
        println!(
            "Failed:             {} ({:.1}%)",
            counts.failed,
            counts.failed as f64 / counts.total as f64 * 100.0
        );
        println!("Pending:            {}", counts.pending);
        println!("Total words:        {}", counts.total_words);
        println!(
            "Time elapsed:       {elapsed:.1}s ({:.1} minutes)",
            elapsed / 60.0
        );
        println!(
            "Average:            {:.2}s per article",
            elapsed / counts.total as f64
        );
    }
    println!("{}", "=".repeat(60));

    let failed_urls = db.get_failed_urls(10)?;
    if !failed_urls.is_empty() {
        println!("\nFailed URLs (first 10):");
        for (url, error) in &failed_urls {
            println!("  - {url}");
            if let Some(err) = error {
                let truncated: String = err.chars().take(100).collect();
                println!("    Error: {truncated}");
            }
        }
    }

    Ok(())
}

mod cli;
mod csv_reader;
mod db;
mod extractor;
mod extractors;
mod filename;
mod paywall;
mod trafilatura;

use clap::Parser;
use cli::Command;
use extractor::ExtractionResult;
use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;
use std::sync::atomic::{AtomicU64, Ordering};
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
            cmd_download(csv_file, output_dir, workers, retries, timeout, retry_failed).await
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
    workers: usize,
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

    let to_process: Vec<_> = {
        let db_ref = &db;
        all_rows
            .par_iter()
            .filter(|row| {
                if db_ref.is_already_successful(&row.url).unwrap_or(false) {
                    return false;
                }
                if !retry_failed && db_ref.is_already_failed(&row.url).unwrap_or(false) {
                    return false;
                }
                true
            })
            .cloned()
            .collect()
    };

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

    let success_count = Arc::new(AtomicU64::new(0));
    let failed_count = Arc::new(AtomicU64::new(0));

    let ext = Arc::new(extractor::Extractor::new(
        db.clone(),
        output_dir.clone(),
        workers,
        retries,
        timeout,
    ));

    let mut handles = Vec::with_capacity(to_process.len());
    for row in to_process {
        let ext = ext.clone();
        let pb = pb.clone();
        let sc = success_count.clone();
        let fc = failed_count.clone();
        handles.push(tokio::spawn(async move {
            let result = ext.process_article(&row).await;
            match result {
                ExtractionResult::Success { .. } => {
                    sc.fetch_add(1, Ordering::Relaxed);
                }
                ExtractionResult::Failed { .. } => {
                    fc.fetch_add(1, Ordering::Relaxed);
                }
            }
            pb.inc(1);
            pb.set_message(format!(
                "OK: {} Fail: {}",
                sc.load(Ordering::Relaxed),
                fc.load(Ordering::Relaxed)
            ));
        }));
    }

    for handle in handles {
        let _ = handle.await;
    }
    pb.finish_with_message("Done");

    let elapsed = start.elapsed().as_secs_f64();
    print_report(&db, elapsed)?;

    Ok(())
}

fn cmd_search(
    query: &str,
    db_dir: &std::path::Path,
    limit: usize,
) -> anyhow::Result<()> {
    let db_path = db_dir.join("index.db");
    let db = db::Database::open(&db_path)?;

    let results = db.search(query, limit)?;

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

        let snippet = r
            .snippet
            .replace(">>>", "\x1b[1;33m")
            .replace("<<<", "\x1b[0m");
        println!("    {snippet}");
        println!();
    }

    println!("Use `insta read <ID>` to view full article content.");

    Ok(())
}

fn cmd_read(id: i64, db_dir: &std::path::Path) -> anyhow::Result<()> {
    let db_path = db_dir.join("index.db");
    let db = db::Database::open(&db_path)?;

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
    let db_path = db_dir.join("index.db");
    let db = db::Database::open(&db_path)?;
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

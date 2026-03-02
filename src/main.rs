mod cli;
mod csv_reader;
mod db;
mod extractor;
mod extractors;
mod filename;
mod paywall;
mod trafilatura;

use clap::Parser;
use extractor::ExtractionResult;
use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = cli::Args::parse();
    let start = Instant::now();

    // Create output directory
    tokio::fs::create_dir_all(&args.output_dir).await?;

    // Initialize database
    let db_path = args.output_dir.join("index.db");
    let db = Arc::new(db::Database::open(&db_path)?);
    db.init_schema()?;

    println!("Instapaper Article Downloader (Rust)");
    println!("{}", "=".repeat(60));

    // Load CSV
    println!("Loading articles from {}...", args.csv_file.display());
    let all_rows = csv_reader::read_csv(&args.csv_file)?;
    println!("Loaded {} articles from CSV", all_rows.len());

    // Use rayon to parallelize the filtering (DB lookups for each URL)
    let retry_failed = args.retry_failed;
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

    // Insert pending entries (sequential - DB writes)
    for row in &to_process {
        db.insert_pending(row)?;
    }

    if to_process.is_empty() {
        println!("No articles to download (all already processed)");
        return Ok(());
    }

    println!("Found {} articles to download", to_process.len());
    println!("Using {} concurrent workers", args.workers);
    println!("{}\n", "=".repeat(60));

    // Progress bar
    let pb = ProgressBar::new(to_process.len() as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} ({percent}%) | {msg}")
            .unwrap()
            .progress_chars("##-"),
    );

    let success_count = Arc::new(AtomicU64::new(0));
    let failed_count = Arc::new(AtomicU64::new(0));

    // Create extractor
    let ext = Arc::new(extractor::Extractor::new(
        db.clone(),
        args.output_dir.clone(),
        args.workers,
        args.retries,
        args.timeout,
    ));

    // Spawn all tasks - tokio handles async I/O concurrency via semaphore
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

    // Await all tasks
    for handle in handles {
        let _ = handle.await;
    }
    pb.finish_with_message("Done");

    // Generate report
    let elapsed = start.elapsed().as_secs_f64();
    generate_report(&db, elapsed)?;

    Ok(())
}

fn generate_report(db: &db::Database, elapsed: f64) -> anyhow::Result<()> {
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

    // Show failed URLs
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

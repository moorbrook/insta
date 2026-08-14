mod cli;
mod output;

use std::num::NonZeroUsize;
use std::time::Duration;

use clap::Parser;
use instapaper_dl::{
    download, read, search, stats, DownloadConfig, Error, OutputMode, ReadConfig, SearchConfig,
    StatsConfig,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = cli::Args::parse();
    init_tracing(args.verbose);
    let mode = OutputMode::from_json_flag(args.json);

    match args.command {
        cli::Command::Download {
            csv_file,
            output_dir,
            workers,
            retries,
            timeout,
            retry_failed,
        } => {
            #[allow(
                clippy::expect_used,
                reason = "clap value_parser rejects a zero worker count"
            )]
            let workers =
                NonZeroUsize::new(workers as usize).expect("clap rejects a zero worker count");
            let report = download(
                DownloadConfig {
                    csv_file,
                    output_dir,
                    workers,
                    retries,
                    timeout: Duration::from_secs(timeout),
                    retry_failed,
                },
                mode,
            )
            .await?;
            output::download_report(mode, &report)?;
            if report.all_failed() {
                return Err(Error::AllDownloadsFailed {
                    failed: report.batch_failed,
                }
                .into());
            }
        }
        cli::Command::Search {
            query,
            db_dir,
            limit,
        } => {
            let query = query.join(" ");
            let results = search(&SearchConfig {
                query: query.clone(),
                db_dir,
                limit,
            })?;
            output::search_results(mode, &query, &results)?;
        }
        cli::Command::Read { id, db_dir } => {
            let article = read(&ReadConfig { id, db_dir })?;
            output::article(mode, id, article.as_ref())?;
        }
        cli::Command::Stats { db_dir } => {
            let counts = stats(&StatsConfig { db_dir })?;
            output::stats(mode, &counts)?;
        }
    }

    Ok(())
}

fn init_tracing(verbose: u8) {
    use tracing_subscriber::EnvFilter;

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::builder()
                .with_default_directive(match verbose {
                    0 => tracing::Level::WARN.into(),
                    1 => tracing::Level::INFO.into(),
                    2 => tracing::Level::DEBUG.into(),
                    _ => tracing::Level::TRACE.into(),
                })
                .from_env_lossy(),
        )
        .with_target(false)
        .with_writer(std::io::stderr)
        .init();
}

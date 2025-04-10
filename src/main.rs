mod github_searcher;
use github_searcher::GitHubSearcher;

use clap::Parser;
use dotenv::dotenv;
use std::error::Error;
use tracing::info;

/// GitHub Code Search CLI
#[derive(Parser)]
#[clap(author, version, about)]
pub struct Args {
    /// Words to search for in GitHub code
    #[clap(short, long, num_args = 1.., required = true)]
    words: Vec<String>,

    /// Output file path for search results
    #[clap(short, long, default_value = "search_results.json")]
    output: String,

    /// Maximum number of pages to retrieve per search term
    #[clap(short = 'p', long, value_name = "NUM")]
    max_pages: Option<u32>,

    /// GitHub API token (will use GITHUB_TOKEN from environment if not provided)
    #[clap(short, long)]
    token: Option<String>,

    /// Maximum number of concurrent searches
    #[clap(short = 'c', long, default_value = "4")]
    concurrency: usize,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    // Initialize logger
    tracing_subscriber::fmt::init();

    // Load environment variables
    dotenv().ok();

    // Parse command-line arguments
    let args = Args::parse();

    // Create and run the searcher
    let searcher = GitHubSearcher::new(&args).await?;
    searcher.run(args.words).await?;

    info!("Finished saving filtered results to '{}'", args.output);
    Ok(())
}

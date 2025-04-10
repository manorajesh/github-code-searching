//! # GitHub Code Searcher
//!
//! A powerful, concurrent CLI tool for searching code across GitHub repositories
//! with advanced rate-limit handling and progress visualization.
//!
//! ## Features
//!
//! - **Efficient Code Searching**: Search GitHub's codebase for specific keywords or phrases
//! - **Concurrent Execution**: Run multiple searches in parallel with configurable concurrency
//! - **Smart Rate-Limit Handling**: Automatically detects and waits for GitHub API rate limits with visual feedback
//! - **Detailed Progress Tracking**: Real-time progress indicators for each search term
//! - **Pagination Support**: Configure maximum pages per search term
//! - **JSON Output**: Structured output format for post-processing
//!
//! ## Example Usage
//!
//! ```bash
//! # Basic usage
//! github-code-searching --words "rust concurrency" "tokio async" --token YOUR_GITHUB_TOKEN
//!
//! # Search with environment variable for token
//! export GITHUB_TOKEN=your_github_token
//! github-code-searching -w "axum router" "rocket framework"
//!
//! # Control concurrency and output location
//! github-code-searching -w "security vulnerability" "authentication bypass" -c 3 -o security_findings.json
//!
//! # Limit pages per search term
//! github-code-searching -w "kubernetes operator" -p 5
//! ```

mod github_searcher;
use github_searcher::GitHubSearcher;

use clap::Parser;
use dotenv::dotenv;
use std::error::Error;
use tracing::info;

/// GitHub Code Search CLI tool for searching code on GitHub with advanced features
/// including concurrency control, rate-limit handling and progress visualization.
#[derive(Parser)]
#[clap(
    author,
    version,
    about,
    long_about = "A powerful, concurrent CLI tool for searching code across GitHub repositories with advanced rate-limit handling and progress visualization."
)]
pub struct Args {
    /// Words or phrases to search for in GitHub code.
    #[clap(short, long, num_args = 1.., required = true)]
    words: Vec<String>,

    /// Output file path for search results in JSON format.
    #[clap(short, long, default_value = "search_results.json")]
    output: String,

    /// Maximum number of pages to retrieve per search term.
    /// Each page contains up to 100 results.
    #[clap(short = 'p', long, value_name = "NUM")]
    max_pages: Option<u32>,

    /// GitHub API token for authentication.
    #[clap(short, long)]
    token: Option<String>,

    /// Maximum number of concurrent searches.
    #[clap(short = 'c', long, default_value = "4")]
    concurrency: usize,
}

/// Main entry point for the GitHub Code Searcher application.
/// Parses command-line arguments, initializes the searcher,
/// and runs the search operations.
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

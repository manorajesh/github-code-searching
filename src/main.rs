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

use github_code_searching_lib::GitHubSearcher;
use github_code_searching_lib::Args;

use dotenv::dotenv;
use std::error::Error;
use tracing::info;
use clap::Parser;

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

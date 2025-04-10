//! # GitHub Code Searching
//!
//! A Rust library for searching code on GitHub with advanced features like
//! concurrent searches, rate-limit handling, and progress visualization.
//!
//! ## Main Components
//!
//! - [`GitHubSearcher`]: The core component that handles all search operations
//! - [`Args`]: Command line argument structure for configuring search parameters
//!
//! ## Example
//!
//! ```no_run
//! use github_code_searching::{Args, GitHubSearcher};
//! use clap::Parser;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
//!     // Parse command line arguments
//!     let args = Args::parse();
//!
//!     // Initialize the searcher
//!     let searcher = GitHubSearcher::new(&args).await?;
//!
//!     // Run searches for the provided terms
//!     searcher.run(vec!["rust async".to_string(), "tokio select".to_string()]).await?;
//!
//!     Ok(())
//! }
//! ```

mod github_searcher;
mod args;

// Re-export main components for documentation and external use
pub use crate::github_searcher::GitHubSearcher;
pub use crate::args::Args;

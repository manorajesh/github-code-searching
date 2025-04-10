use clap::Parser;

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
    pub words: Vec<String>,

    /// Output file path for search results in JSON format.
    #[clap(short, long, default_value = "search_results.json")]
    pub output: String,

    /// Maximum number of pages to retrieve per search term.
    /// Each page contains up to 100 results.
    #[clap(short = 'p', long, value_name = "NUM")]
    pub max_pages: Option<u32>,

    /// GitHub API token for authentication.
    #[clap(short, long)]
    pub token: Option<String>,

    /// Maximum number of concurrent searches.
    #[clap(short = 'c', long, default_value = "4")]
    pub concurrency: usize,
}

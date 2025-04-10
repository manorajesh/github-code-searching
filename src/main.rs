use reqwest::{ Client, StatusCode };
use std::error::Error;
use tokio::time::{ sleep, Duration };
use chrono::Utc;
use serde_json::{ Value, json };
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;
use tracing::{ info, warn, error, debug };
use indicatif::{ MultiProgress, ProgressBar, ProgressStyle };
use dotenv::dotenv;
use std::env;
use std::sync::Arc;
use tokio::sync::Semaphore;
use futures::future::join_all;
use clap::{ Parser, ArgAction };

/// GitHub Code Search CLI
#[derive(Parser)]
#[clap(author, version, about)]
struct Args {
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
    #[clap(short = 'c', long, default_value = "2")]
    concurrency: usize,
}

struct GitHubSearcher {
    client: Client,
    token: String,
    output_path: String,
    max_page_limit: Option<u32>,
    progress: Arc<MultiProgress>,
    concurrency: usize,
}

impl GitHubSearcher {
    /// Create a new GitHubSearcher instance
    async fn new(args: &Args) -> Result<Self, Box<dyn Error + Send + Sync>> {
        // Get GitHub API token from arguments or environment
        let token = match &args.token {
            Some(t) if !t.trim().is_empty() => t.clone(),
            _ => {
                match env::var("GITHUB_TOKEN") {
                    Ok(token) if !token.trim().is_empty() => token,
                    _ => {
                        error!("GitHub token not provided or found in environment");
                        return Err("GitHub token is required".into());
                    }
                }
            }
        };

        // Create HTTP client
        let client = Client::builder().user_agent("Rust GitHub API Client").build()?;

        // Create progress display
        let progress = Arc::new(MultiProgress::new());

        Ok(GitHubSearcher {
            client,
            token,
            output_path: args.output.clone(),
            max_page_limit: args.max_pages,
            progress,
            concurrency: args.concurrency,
        })
    }

    /// Run all searches with concurrency control
    async fn run(&self, words: Vec<String>) -> Result<(), Box<dyn Error + Send + Sync>> {
        // Create semaphore for concurrency control
        let semaphore = Arc::new(Semaphore::new(self.concurrency));

        // Create output file
        let file = Arc::new(
            tokio::sync::Mutex::new(
                OpenOptions::new().create(true).append(true).open(&self.output_path).await?
            )
        );

        // Launch tasks for each word
        let mut tasks = Vec::new();

        for word in words {
            let word_clone = word.clone();
            let sem_clone = semaphore.clone();
            let client_clone = self.client.clone();
            let token_clone = self.token.clone();
            let file_clone = file.clone();
            let progress_clone = self.progress.clone();
            let max_pages = self.max_page_limit;

            let task = tokio::spawn(async move {
                // Acquire semaphore permit
                let _permit = sem_clone.acquire().await.unwrap();

                // Create progress bar for this word
                let pb = progress_clone.add(ProgressBar::new_spinner());
                pb.set_style(
                    ProgressStyle::default_spinner()
                        .template("{spinner:.green} [{elapsed_precise}] {msg}")
                        .unwrap()
                        .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏")
                );
                pb.set_message(format!("Starting search for '{}'", word_clone));

                // Process this word
                let result = GitHubSearcher::search_word(
                    &client_clone,
                    &token_clone,
                    &word_clone,
                    file_clone,
                    max_pages,
                    pb.clone()
                ).await;

                // Finalize progress bar
                if result.is_ok() {
                    pb.finish_with_message(format!("✓ Completed '{}'", word_clone));
                } else {
                    pb.finish_with_message(format!("✗ Failed '{}'", word_clone));
                }

                result
            });

            tasks.push(task);
        }

        // Await all tasks
        let results = join_all(tasks).await;

        // Check for errors
        for result in results {
            if let Ok(Err(e)) = result {
                error!("Search task error: {}", e);
                return Err(e);
            }
        }

        info!("All searches completed successfully");
        Ok(())
    }

    /// Search for a specific word
    async fn search_word(
        client: &Client,
        token: &str,
        word: &str,
        file: Arc<tokio::sync::Mutex<tokio::fs::File>>,
        max_page_limit: Option<u32>,
        pb: ProgressBar
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let mut page: u32 = 1;

        loop {
            pb.set_message(format!("{} - page {}", word, page));

            // Search this page
            match GitHubSearcher::search_page(client, token, word, page, &file, &pb).await {
                Ok(has_more_pages) => {
                    if !has_more_pages {
                        debug!("No more results for '{}'", word);
                        break;
                    }
                }
                Err(e) => {
                    error!("Error searching '{}' page {}: {}", word, page, e);
                    return Err(e);
                }
            }

            page += 1;

            // Check page limit
            if let Some(max_page) = max_page_limit {
                if page > max_page {
                    info!("Max page limit reached for '{}' (limit: {})", word, max_page);
                    break;
                }
            }

            pb.tick();
        }

        Ok(())
    }

    /// Search a specific page for a word
    async fn search_page(
        client: &Client,
        token: &str,
        word: &str,
        page: u32,
        file: &Arc<tokio::sync::Mutex<tokio::fs::File>>,
        pb: &ProgressBar
    ) -> Result<bool, Box<dyn Error + Send + Sync>> {
        let url = format!(
            "https://api.github.com/search/code?q={}&page={}&per_page=100",
            word,
            page
        );

        debug!("Requesting URL: {}", url);
        let response = client
            .get(&url)
            .header("Accept", "application/vnd.github.text-match+json")
            .header("Authorization", format!("Bearer {}", token))
            .header("X-GitHub-Api-Version", "2022-11-28")
            .send().await?;

        // Handle pagination limit
        if response.status() == StatusCode::UNPROCESSABLE_ENTITY {
            warn!("Reached search limit for '{}' at page {}", word, page);
            return Ok(false);
        }

        // Handle rate limiting
        GitHubSearcher::handle_rate_limit(response.headers(), pb).await?;

        // Check for other errors
        if !response.status().is_success() {
            return Err(
                format!("API error: {} on word '{}' page {}", response.status(), word, page).into()
            );
        }

        // Parse response
        let json: Value = response.json().await?;
        let mut filtered_items = Vec::new();

        // Process items
        if let Some(items) = json["items"].as_array() {
            if items.is_empty() {
                return Ok(false);
            }

            for item in items {
                let name = item
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let html_url = item
                    .get("html_url")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                let repo_owner = if let Some(repo) = item.get("repository") {
                    repo.get("owner")
                } else {
                    None
                };

                let owner_login = repo_owner
                    .and_then(|o| o.get("login"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let owner_avatar_url = repo_owner
                    .and_then(|o| o.get("avatar_url"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let owner_html_url = repo_owner
                    .and_then(|o| o.get("html_url"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                let text_matches = item
                    .get("text_matches")
                    .cloned()
                    .unwrap_or_else(|| json!([]));

                // Build filtered JSON object
                let new_item =
                    json!({
                    "name": name,
                    "html_url": html_url,
                    "search_term": word,
                    "repository_owner": {
                        "login": owner_login,
                        "avatar_url": owner_avatar_url,
                        "html_url": owner_html_url,
                    },
                    "text_matches": text_matches
                });

                filtered_items.push(new_item);
            }
        } else {
            warn!("No 'items' array found in response for '{}'", word);
            return Ok(false);
        }

        // Write results to file
        let filtered_json = serde_json::to_string(&filtered_items)?;
        let mut file_guard = file.lock().await;
        file_guard.write_all(filtered_json.as_bytes()).await?;
        file_guard.write_all(b"\n").await?;
        file_guard.flush().await?;

        info!("Saved {} results for '{}' page {}", filtered_items.len(), word, page);
        Ok(true)
    }

    /// Handle GitHub API rate limiting
    async fn handle_rate_limit(
        headers: &reqwest::header::HeaderMap,
        pb: &ProgressBar
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        if let Some(remaining_header) = headers.get("X-RateLimit-Remaining") {
            if let Ok(remaining_str) = remaining_header.to_str() {
                if let Ok(remaining) = remaining_str.parse::<u32>() {
                    if remaining == 0 {
                        if let Some(reset_header) = headers.get("X-RateLimit-Reset") {
                            if let Ok(reset_str) = reset_header.to_str() {
                                if let Ok(reset_timestamp) = reset_str.parse::<u64>() {
                                    let now = Utc::now().timestamp() as u64;
                                    if reset_timestamp > now {
                                        let wait_secs = reset_timestamp - now;
                                        warn!(
                                            "Rate limit reached. Waiting {} seconds...",
                                            wait_secs + 1
                                        );
                                        pb.set_message(
                                            format!(
                                                "Rate limit hit - waiting {} seconds",
                                                wait_secs + 1
                                            )
                                        );
                                        sleep(Duration::from_secs(wait_secs + 1)).await;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }
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

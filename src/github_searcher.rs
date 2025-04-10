use chrono::Utc;
use clap::Parser;
use futures::future::join_all;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use reqwest::{Client, StatusCode};
use serde_json::{json, Value};
use std::env;
use std::error::Error;
use std::sync::Arc;
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;
use tokio::sync::Semaphore;
use tokio::time::{Duration, Instant};
use tracing::{debug, error, info, warn};

use crate::Args;

pub struct GitHubSearcher {
    client: Client,
    token: String,
    output_path: String,
    max_page_limit: Option<u32>,
    progress: Arc<MultiProgress>,
    concurrency: usize,
}

impl GitHubSearcher {
    /// Create a new GitHubSearcher instance
    pub async fn new(args: &Args) -> Result<Self, Box<dyn Error + Send + Sync>> {
        // Get GitHub API token from arguments or environment
        let token = match &args.token {
            Some(t) if !t.trim().is_empty() => t.clone(),
            _ => match env::var("GITHUB_TOKEN") {
                Ok(token) if !token.trim().is_empty() => token,
                _ => {
                    error!("GitHub token not provided or found in environment");
                    return Err("GitHub token is required".into());
                }
            },
        };

        // Create HTTP client
        let client = Client::builder()
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64)")
            .build()?;

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
    pub async fn run(&self, words: Vec<String>) -> Result<(), Box<dyn Error + Send + Sync>> {
        // Create semaphore for concurrency control
        let semaphore = Arc::new(Semaphore::new(self.concurrency));

        // Create output file
        let file = Arc::new(tokio::sync::Mutex::new(
            OpenOptions::new()
                .create(true)
                .append(true)
                .open(&self.output_path)
                .await?,
        ));

        // Create progress bars for all words upfront
        let progress_bars: Arc<std::collections::HashMap<String, ProgressBar>> = {
            let mut bars = std::collections::HashMap::new();

            // Create a main spinner style
            let spinner_style = ProgressStyle::default_spinner()
                .template("{spinner:.green} [{elapsed_precise}] {wide_msg}")
                .unwrap()
                .progress_chars("=>-")
                .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏");

            // Create a progress bar style for rate limiting
            let progress_style = ProgressStyle::default_bar()
                .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos:>7}/{len:7} {wide_msg}")
                .unwrap()
                .progress_chars("=>-");

            // Add a progress bar for each word
            for word in &words {
                let pb = self.progress.add(ProgressBar::new_spinner());
                pb.set_style(spinner_style.clone());
                pb.set_message(format!("Waiting to search for '{}'", word));
                bars.insert(word.clone(), pb);
            }

            Arc::new(bars)
        };

        // Add a rate limit progress bar at the bottom
        let rate_limit_pb = Arc::new(self.progress.add(ProgressBar::new(100)));
        rate_limit_pb.set_style(
            ProgressStyle::default_bar()
                .template("[{elapsed_precise}] {bar:40.red/yellow} {pos:>7}/{len:7} {wide_msg}")
                .unwrap()
                .progress_chars("=>-"),
        );
        rate_limit_pb.set_message("Rate limit status: OK");
        rate_limit_pb.set_position(100); // Start full

        // Launch tasks for each word
        let mut tasks = Vec::new();
        let mut spinner_tasks = Vec::new();

        for word in words {
            let word_clone = word.clone();
            let sem_clone = semaphore.clone();
            let client_clone = self.client.clone();
            let token_clone = self.token.clone();
            let file_clone = file.clone();
            let max_pages = self.max_page_limit;
            let pb = progress_bars.get(&word).unwrap().clone();
            let rate_limit_pb_clone = rate_limit_pb.clone();

            // Start a background ticker to keep spinner animated
            let pb_ticker = pb.clone();
            let spinner_task = tokio::spawn(async move {
                loop {
                    pb_ticker.tick();
                    tokio::time::sleep(Duration::from_millis(80)).await;
                }
            });
            spinner_tasks.push(spinner_task);

            // Main search task
            let task = tokio::spawn(async move {
                // Acquire semaphore permit
                let _permit = sem_clone.acquire().await.unwrap();

                pb.set_message(format!("Starting search for '{}'", word_clone));

                // Process this word
                let result = GitHubSearcher::search_word(
                    &client_clone,
                    &token_clone,
                    &word_clone,
                    file_clone,
                    max_pages,
                    pb.clone(),
                    rate_limit_pb_clone,
                )
                .await;

                // Finalize progress bar
                if result.is_ok() {
                    pb.set_message(format!("✓ Completed '{}'", word_clone));
                } else {
                    pb.set_message(format!("✗ Failed '{}'", word_clone));
                }

                result
            });

            tasks.push(task);
        }

        // Await all tasks
        let results = join_all(tasks).await;

        // Abort ticker tasks now that main tasks are done
        for task in spinner_tasks {
            task.abort();
        }

        // Clear rate limit progress bar
        rate_limit_pb.finish_and_clear();

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
        pb: ProgressBar,
        rate_limit_pb: Arc<ProgressBar>,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let mut page: u32 = 1;

        loop {
            pb.set_message(format!("Searching {} - page {}", word, page));

            // Search this page
            match GitHubSearcher::search_page(client, token, word, page, &file, &pb, &rate_limit_pb)
                .await
            {
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
                    info!(
                        "Max page limit reached for '{}' (limit: {})",
                        word, max_page
                    );
                    break;
                }
            }
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
        pb: &ProgressBar,
        rate_limit_pb: &ProgressBar,
    ) -> Result<bool, Box<dyn Error + Send + Sync>> {
        let url = format!(
            "https://api.github.com/search/code?q={}&page={}&per_page=100",
            word, page
        );

        debug!("Requesting URL: {}", url);
        let response = client
            .get(&url)
            .header("Accept", "application/vnd.github.text-match+json")
            .header("Authorization", format!("Bearer {}", token))
            .header("X-GitHub-Api-Version", "2022-11-28")
            .send()
            .await?;

        // Handle pagination limit
        if response.status() == StatusCode::UNPROCESSABLE_ENTITY {
            warn!("Reached search limit for '{}' at page {}", word, page);
            return Ok(false);
        }

        // Handle rate limiting
        GitHubSearcher::handle_rate_limit(response.headers(), pb, rate_limit_pb).await?;

        // Check for other errors
        if !response.status().is_success() {
            return Err(format!(
                "API error: {} on word '{}' page {}",
                response.status(),
                word,
                page
            )
            .into());
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
                let name = item.get("name").and_then(|v| v.as_str()).unwrap_or("");
                let html_url = item.get("html_url").and_then(|v| v.as_str()).unwrap_or("");
                // Extract SHA hash
                let sha = item.get("sha").and_then(|v| v.as_str()).unwrap_or("");

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
                let new_item = json!({
                    "name": name,
                    "html_url": html_url,
                    "sha": sha,
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

        info!(
            "Saved {} results for '{}' page {}",
            filtered_items.len(),
            word,
            page
        );
        Ok(true)
    }

    /// Handle GitHub API rate limiting with progress bar
    async fn handle_rate_limit(
        headers: &reqwest::header::HeaderMap,
        pb: &ProgressBar,
        rate_limit_pb: &ProgressBar,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        // Update rate limit indicator
        if let Some(remaining_header) = headers.get("X-RateLimit-Remaining") {
            if let Ok(remaining_str) = remaining_header.to_str() {
                if let Ok(remaining) = remaining_str.parse::<u32>() {
                    if let Some(limit_header) = headers.get("X-RateLimit-Limit") {
                        if let Ok(limit_str) = limit_header.to_str() {
                            if let Ok(limit) = limit_str.parse::<u32>() {
                                // Calculate percentage of rate limit remaining
                                let percentage = if limit > 0 {
                                    (remaining * 100) / limit
                                } else {
                                    100
                                };

                                rate_limit_pb
                                    .set_message(format!("Rate limit: {}/{}", remaining, limit));
                                rate_limit_pb.set_position(percentage.into());

                                // If we're out of requests, wait for reset
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

                                                    // Save the current message to restore later
                                                    let original_msg = pb.message();
                                                    pb.set_message(format!(
                                                        "Rate limited - waiting {} seconds",
                                                        wait_secs + 1
                                                    ));

                                                    // Create a visual countdown
                                                    let start = Instant::now();
                                                    let duration =
                                                        Duration::from_secs(wait_secs + 1);
                                                    let end = start + duration;

                                                    while Instant::now() < end {
                                                        let elapsed = start.elapsed();
                                                        if elapsed < duration {
                                                            let remaining = duration - elapsed;
                                                            let secs_remaining =
                                                                remaining.as_secs();
                                                            let percentage = ((duration
                                                                - remaining)
                                                                .as_millis()
                                                                * 100)
                                                                / duration.as_millis();

                                                            rate_limit_pb
                                                                .set_position(percentage as u64);
                                                            rate_limit_pb.set_message(
                                                                format!("Rate limit cooldown: {}s remaining", secs_remaining)
                                                            );
                                                            pb.set_message(format!(
                                                                "Rate limited - waiting {}s",
                                                                secs_remaining
                                                            ));

                                                            tokio::time::sleep(
                                                                Duration::from_millis(500),
                                                            )
                                                            .await;
                                                        } else {
                                                            break;
                                                        }
                                                    }

                                                    // Reset rate limit bar and restore original message
                                                    rate_limit_pb.set_position(100);
                                                    rate_limit_pb
                                                        .set_message("Rate limit status: Ready");
                                                    pb.set_message(original_msg);
                                                }
                                            }
                                        }
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

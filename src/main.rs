use reqwest::{ Client, StatusCode };
use std::error::Error;
use tokio::time::{ sleep, Duration };
use chrono::Utc;
use serde_json::{ Value, json };
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;
use tracing::{ info, warn, error, debug };
use indicatif::{ ProgressBar, ProgressStyle };
use dotenv::dotenv;
use std::env;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Initialize logger.
    tracing_subscriber::fmt::init();

    dotenv().ok();

    // Get GitHub API token from environment.
    let bearer_token = match env::var("GITHUB_TOKEN") {
        Ok(token) if !token.trim().is_empty() => token,
        Ok(_) => {
            error!("GITHUB_TOKEN environment variable is empty");
            return Err("GitHub token is empty".into());
        }
        Err(_) => {
            error!("GITHUB_TOKEN environment variable not found");
            return Err("GitHub token not found in environment".into());
        }
    };

    // Array of words to search for.
    let words = vec!["shit", "fuck", "damn", "penis", "poop", "funky"];

    // Configure maximum page limit (Some(n) or None for no limit).
    let max_page_limit: Option<u32> = None;

    // Create reqwest client with required User-Agent.
    let client = Client::builder().user_agent("Rust GitHub API Client").build()?;

    // Open (or create) output file in append mode.
    let file_path = "big_results.json";
    let mut file = OpenOptions::new().create(true).append(true).open(file_path).await?;

    // Create a single progress bar that remains at the bottom.
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.green} {msg}")
            .unwrap()
            .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏")
    );

    // Process each search word.
    for word in words {
        info!("Starting search for word: '{}'", word);
        pb.set_message(format!("Processing word: {}", word));
        let mut page: u32 = 1;

        loop {
            pb.set_message(format!("{} - page {}", word, page));
            let url = format!(
                "https://api.github.com/search/code?q={}&page={}&per_page=100",
                word,
                page
            );

            debug!("Requesting URL: {}", url);
            let response = client
                .get(&url)
                .header("Accept", "application/vnd.github.text-match+json")
                .header("Authorization", format!("Bearer {}", bearer_token))
                .header("X-GitHub-Api-Version", "2022-11-28")
                .send().await?;

            // If status code is 422, we've reached search limit.
            if response.status() == StatusCode::UNPROCESSABLE_ENTITY {
                warn!("Reached search limit for '{}' at page {}", word, page);
                break;
            }

            // Check rate limit and sleep if needed.
            if let Some(remaining_header) = response.headers().get("X-RateLimit-Remaining") {
                if let Ok(remaining_str) = remaining_header.to_str() {
                    if let Ok(remaining) = remaining_str.parse::<u32>() {
                        if remaining == 0 {
                            if let Some(reset_header) = response.headers().get("X-RateLimit-Reset") {
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

            if !response.status().is_success() {
                error!("Error: {} on word '{}' page {}", response.status(), word, page);
                break;
            }

            // Parse full JSON response.
            let json: Value = response.json().await?;
            // Build a new array of filtered items.
            let mut filtered_items = Vec::new();

            if let Some(items) = json["items"].as_array() {
                if items.is_empty() {
                    info!("No more results for '{}'", word);
                    break;
                }
                for item in items {
                    // Use safe extraction.
                    let name = item
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let html_url = item
                        .get("html_url")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");

                    // The repository owner info is nested in "repository.owner".
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

                    // Build a new JSON object with only the desired fields.
                    let new_item =
                        json!({
                        "name": name,
                        "html_url": html_url,
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
                break;
            }

            // Convert the filtered results to a string (each page’s data as a JSON array).
            let filtered_json = serde_json::to_string(&filtered_items)?;
            // Append filtered JSON followed by newline for NDJSON-style file.
            file.write_all(filtered_json.as_bytes()).await?;
            file.write_all(b"\n").await?;
            file.flush().await?;

            info!("Saved filtered results for word '{}' page {}", word, page);
            page += 1;

            // Respect max page limit if specified.
            if let Some(max_page) = max_page_limit {
                if page > max_page {
                    info!("Max page limit reached for '{}' (limit: {})", word, max_page);
                    break;
                }
            }

            pb.tick();
        } // end page loop

        pb.set_message(format!("Finished '{}' search", word));
    } // end word loop

    pb.finish_and_clear();
    info!("Finished saving filtered results to '{}'", file_path);
    Ok(())
}

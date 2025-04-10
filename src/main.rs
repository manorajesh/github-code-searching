use reqwest::{ Client, StatusCode };
use std::error::Error;
use tokio::time::{ sleep, Duration };
use chrono::Utc;
use serde_json::Value;
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;
use tracing::{ info, warn, error, debug };
use indicatif::{ ProgressBar, ProgressStyle };
use dotenv::dotenv;
use std::env;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Initialize the tracing logger
    tracing_subscriber::fmt::init();

    dotenv().ok();

    // Replace with your GitHub API token.
    let bearer_token = match env::var("GITHUB_TOKEN") {
        Ok(token) => {
            if token.trim().is_empty() {
                error!("GITHUB_TOKEN environment variable is empty");
                return Err("GitHub token is empty".into());
            }
            token
        }
        Err(_) => {
            error!("GITHUB_TOKEN environment variable not found");
            return Err("GitHub token not found in environment".into());
        }
    };

    // Array of words to search for.
    let words = vec!["shit", "fuck", "damn", "penis", "poop", "funky"];

    // Configure the maximum page limit (set to Some(limit) or None to use no limit)
    let max_page_limit: Option<u32> = None; // For example, Some(50) to limit to 50 pages

    // Create the reqwest client with a User-Agent.
    let client = Client::builder().user_agent("Rust GitHub API Client").build()?;

    // Open (or create) our output file in append mode.
    let file_path = "big_results.json";
    let mut file = OpenOptions::new().create(true).append(true).open(file_path).await?;

    // Process each search word
    for word in words {
        info!("Starting search for word: '{}'", word);

        // Create a spinner progress bar for the current word.
        let pb = ProgressBar::new_spinner();
        pb.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.green} {msg}")
                .unwrap()
                .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏")
        );

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

            // If status is 422, we assume we've reached the limit or exceeded searchable results.
            if response.status() == StatusCode::UNPROCESSABLE_ENTITY {
                warn!("Reached search limit for '{}' at page {}", word, page);
                break;
            }

            // Check rate limit and wait if needed.
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

            // Parse the JSON response.
            let json: Value = response.json().await?;
            let json_string = serde_json::to_string(&json)?;
            file.write_all(json_string.as_bytes()).await?;
            file.write_all(b"\n").await?;
            file.flush().await?;

            // Check if there are no items.
            if let Some(items) = json["items"].as_array() {
                if items.is_empty() {
                    info!("No more results for '{}'", word);
                    break;
                }
            } else {
                warn!("No 'items' array found in response for '{}'", word);
                break;
            }

            info!("Saved results for word '{}' page {}", word, page);
            page += 1;

            // If a max page limit is specified, exit if reached.
            if let Some(max_page) = max_page_limit {
                if page > max_page {
                    info!("Max page limit reached for '{}' (limit: {})", word, max_page);
                    break;
                }
            }

            // Tick the progress spinner.
            pb.tick();
        } // end page loop

        pb.finish_with_message(format!("Finished '{}' search", word));
    } // end word loop

    info!("Finished saving results to '{}'", file_path);
    Ok(())
}

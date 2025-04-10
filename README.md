# GitHub Code Searcher

A powerful, concurrent CLI tool for searching code across GitHub repositories with advanced rate-limit handling and progress visualization.

## Features

- **Efficient Code Searching**: Search GitHub's codebase for specific keywords or phrases
- **Concurrent Execution**: Run multiple searches in parallel with configurable concurrency
- **Smart Rate-Limit Handling**: Automatically detects and waits for GitHub API rate limits with visual feedback
- **Detailed Progress Tracking**: Real-time progress indicators for each search term
- **Pagination Support**: Configure maximum pages per search term
- **JSON Output**: Structured output format for post-processing

## Installation

### Prerequisites

- Rust and Cargo installed ([Install Rust](https://www.rust-lang.org/tools/install))
- GitHub Personal Access Token ([Create token](https://github.com/settings/tokens))

### Building from Source

```bash
# Clone the repository
git clone https://github.com/manorajesh/github-code-searching.git
cd github-code-searching

# Build the project
cargo build --release

# The binary will be available at
# ./target/release/github-code-searching
```

## Usage

```bash
# Basic usage
github-code-searching --words "rust concurrency" "tokio async" --token YOUR_GITHUB_TOKEN

# Search with environment variable for token
export GITHUB_TOKEN=your_github_token
github-code-searching -w "axum router" "rocket framework"

# Control concurrency and output location
github-code-searching -w "security vulnerability" "authentication bypass" -c 3 -o security_findings.json

# Limit pages per search term
github-code-searching -w "kubernetes operator" -p 5
```

### Command-Line Arguments

| Argument     | Short | Long            | Description                                      | Default                     |
| ------------ | ----- | --------------- | ------------------------------------------------ | --------------------------- |
| Search terms | `-w`  | `--words`       | Words to search for (required, multiple allowed) | -                           |
| Output file  | `-o`  | `--output`      | Path for JSON results                            | `search_results.json`       |
| Max pages    | `-p`  | `--max-pages`   | Maximum pages per search term                    | No limit                    |
| GitHub token | `-t`  | `--token`       | GitHub API token                                 | Uses `GITHUB_TOKEN` env var |
| Concurrency  | `-c`  | `--concurrency` | Number of concurrent searches                    | `2`                         |

## Authentication

The tool requires a GitHub token with appropriate permissions:

1. Use the `--token` argument to provide your token directly
2. Set the `GITHUB_TOKEN` environment variable
3. Create a `.env` file in the project directory with `GITHUB_TOKEN=your_token`

## Output Format

Results are saved in JSON format with the following structure for each match:

```json
{
  "name": "filename.ext",
  "html_url": "https://github.com/owner/repo/blob/...",
  "sha": "commit-hash",
  "search_term": "search-word",
  "repository_owner": {
    "login": "owner-name",
    "avatar_url": "https://avatars.githubusercontent.com/...",
    "html_url": "https://github.com/owner"
  },
  "text_matches": [
    {
      "object_url": "https://api.github.com/...",
      "object_type": "FileContent",
      "property": "content",
      "fragment": "code fragment containing match",
      "matches": [
        {
          "text": "matched text",
          "indices": [start, end]
        }
      ]
    }
  ]
}
```

## How It Works

GitHub Code Searcher performs the following operations:

1. Parses command-line arguments and authenticates with GitHub
2. Creates progress indicators for each search term
3. Spawns concurrent search tasks limited by the specified concurrency
4. Each search task handles pagination through search results
5. Monitors and respects GitHub API rate limits with visual feedback
6. Filters and saves relevant search results to the specified output file

## Rate Limit Handling

The tool includes sophisticated rate-limit management:

- Monitors remaining API calls through GitHub response headers
- Displays real-time rate limit status
- Automatically pauses with countdown when limits are reached
- Resumes operations when rate limits reset

## Development

### Project Structure

- `src/main.rs`: Entry point and CLI argument parsing
- `src/github_searcher.rs`: Core search functionality and API interaction

### Building and Testing

```bash
# Run with debug output
RUST_LOG=debug cargo run -- -w "example search term"

# Run tests
cargo test
```

## License

[MIT License](LICENSE)

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

use scraper::{Html, Selector};
use thiserror::Error;
use reqwest;
use tokio;
use std::collections::{HashSet, VecDeque};
use std::time::Duration;

#[derive(Debug, Error)]
pub enum DocLoaderError {
    #[error("HTTP Error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("CSS selector error: {0}")]
    Selector(String),
    #[error("Parsing error: {0}")]
    Parsing(String),
    #[error("Network error: {0}")]
    Network(String),
    #[error("Rate limited: {0}")]
    RateLimited(String),
}

// Simple struct to hold document content
#[derive(Debug, Clone)]
pub struct Document {
    pub path: String,
    pub content: String,
}

// Result struct that includes version information
#[derive(Debug)]
pub struct LoadResult {
    pub documents: Vec<Document>,
    pub version: Option<String>,
}

/// Load documentation from docs.rs for a given crate
pub async fn load_documents_from_docs_rs(
    crate_name: &str,
    _version: &str,
    _features: Option<&Vec<String>>,
    max_pages: Option<usize>,
) -> Result<LoadResult, DocLoaderError> {
    println!("Fetching documentation from docs.rs for crate: {}", crate_name);

    let base_url = format!("https://docs.rs/{}/latest/{}/", crate_name, crate_name);
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| DocLoaderError::Network(e.to_string()))?;

    let mut documents = Vec::new();
    let mut visited = HashSet::new();
    let mut to_visit = VecDeque::new();
    to_visit.push_back(base_url.clone());
    let mut extracted_version = None;

    // Define the CSS selector for the main content area
    let content_selector = Selector::parse("div.docblock, section.docblock, .rustdoc .docblock")
        .map_err(|e| DocLoaderError::Selector(e.to_string()))?;

    let max_pages = max_pages.unwrap_or(200); // Default to 200 pages if not specified
    let mut processed = 0;

    while let Some(url) = to_visit.pop_front() {
        if processed >= max_pages {
            eprintln!("Reached maximum page limit ({}), stopping", max_pages);
            break;
        }

        if visited.contains(&url) {
            continue;
        }

        visited.insert(url.clone());
        processed += 1;

        eprintln!("Processing page {}/{}: {}", processed, max_pages, url);

        // Fetch the page with retry logic
        let html_content = match fetch_with_retry(&client, &url, 3).await {
            Ok(content) => content,
            Err(e) => {
                eprintln!("Failed to fetch {} after retries: {}", url, e);
                continue;
            }
        };

        let document = Html::parse_document(&html_content);

        // Extract version from the first page (usually in the header)
        if extracted_version.is_none() && processed == 1 {
            // Try to find version in the docs.rs header
            // docs.rs shows version in format "crate-name 1.2.3"
            if let Ok(version_selector) = Selector::parse(".version") {
                if let Some(version_elem) = document.select(&version_selector).next() {
                    let version_text = version_elem.text().collect::<String>();
                    extracted_version = Some(version_text.trim().to_string());
                    eprintln!("Extracted version: {:?}", extracted_version);
                }
            }

            // Alternative: Look in the title or URL path
            if extracted_version.is_none() {
                // The URL might contain version like /crate-name/1.2.3/
                if let Some(version_match) = url.split('/').nth_back(2) {
                    if version_match != "latest" && version_match.chars().any(|c| c.is_numeric()) {
                        extracted_version = Some(version_match.to_string());
                        eprintln!("Extracted version from URL: {:?}", extracted_version);
                    }
                }
            }
        }

        // Extract text content from documentation blocks
        let mut page_content = Vec::new();
        for element in document.select(&content_selector) {
            let text_content: String = element
                .text()
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .collect::<Vec<&str>>()
                .join("\n");

            if !text_content.is_empty() {
                page_content.push(text_content);
            }
        }

        if !page_content.is_empty() {
            let relative_path = url.strip_prefix("https://docs.rs/")
                .unwrap_or(&url)
                .to_string();

            eprintln!("  -> Extracted content from: {} ({} blocks, {} chars)",
                     relative_path, page_content.len(), page_content.join("\n\n").len());

            documents.push(Document {
                path: relative_path,
                content: page_content.join("\n\n"),
            });
        } else {
            eprintln!("  -> No content extracted from: {}", url);
        }

        // Extract links to other documentation pages within the same crate
        // Follow links for first 75% of pages to get deeper coverage
        if processed < (max_pages * 3 / 4) {
            let link_selector = Selector::parse("a").unwrap();
            let mut found_links = 0;
            let mut added_links = 0;

            for link in document.select(&link_selector) {
                if let Some(href) = link.value().attr("href") {
                    found_links += 1;

                    // Follow various types of relative links
                    let should_follow = href.starts_with("./") ||
                                       href.starts_with("../") ||
                                       // Add support for simple relative paths
                                       (!href.starts_with("http") &&
                                        !href.starts_with("#") &&
                                        !href.starts_with("/") &&
                                        href.ends_with(".html"));

                    if should_follow {
                        if let Ok(absolute_url) = reqwest::Url::parse(&url) {
                            if let Ok(new_url) = absolute_url.join(href) {
                                let new_url_str = new_url.to_string();
                                if new_url_str.contains("docs.rs") &&
                                   new_url_str.contains(crate_name) &&
                                   !visited.contains(&new_url_str) {
                                    to_visit.push_back(new_url_str.clone());
                                    added_links += 1;
                                    if added_links <= 5 { // Only show first 5 for brevity
                                        eprintln!("  -> Adding link: {}", href);
                                    }
                                }
                            }
                        }
                    }
                }
            }
            eprintln!("  Found {} links, added {} new ones to visit", found_links, added_links);
        }

        // Add a longer delay to be respectful to docs.rs and avoid rate limiting
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    }

    eprintln!("Finished loading {} documents from docs.rs", documents.len());
    Ok(LoadResult {
        documents,
        version: extracted_version,
    })
}

/// Synchronous wrapper that uses current tokio runtime
pub fn load_documents(
    crate_name: &str,
    crate_version_req: &str,
    features: Option<&Vec<String>>,
) -> Result<LoadResult, DocLoaderError> {
    // Check if we're already in a tokio runtime
    if tokio::runtime::Handle::try_current().is_ok() {
        // We're in a runtime, but we can't use block_on.
        // We need to make this function async or use a different approach.
        // For now, let's return an error suggesting the async version
        return Err(DocLoaderError::Parsing(
            "Cannot run synchronous load_documents from within async context. Use load_documents_from_docs_rs directly.".to_string()
        ));
    }

    let rt = tokio::runtime::Runtime::new()
        .map_err(|e| DocLoaderError::Parsing(format!("Failed to create tokio runtime: {}", e)))?;

    rt.block_on(load_documents_from_docs_rs(crate_name, crate_version_req, features, None))
}

/// Fetch a URL with retry logic and rate limiting
async fn fetch_with_retry(
    client: &reqwest::Client,
    url: &str,
    max_retries: usize,
) -> Result<String, DocLoaderError> {
    let mut attempts = 0;
    let mut delay = Duration::from_millis(1000); // Start with 1 second

    loop {
        match client.get(url).send().await {
            Ok(response) => {
                if response.status().is_success() {
                    match response.text().await {
                        Ok(text) => return Ok(text),
                        Err(e) => {
                            eprintln!("Failed to read response body for {}: {}", url, e);
                            if attempts >= max_retries {
                                return Err(DocLoaderError::Http(e));
                            }
                        }
                    }
                } else if response.status() == 429 {
                    // Rate limited
                    eprintln!("Rate limited for {}, waiting {:?} before retry {}/{}",
                             url, delay, attempts + 1, max_retries + 1);
                    if attempts >= max_retries {
                        return Err(DocLoaderError::RateLimited(
                            format!("Rate limited after {} attempts", attempts + 1)
                        ));
                    }
                } else {
                    eprintln!("HTTP error for {}: {}", url, response.status());
                    if attempts >= max_retries {
                        return Err(DocLoaderError::Network(
                            format!("HTTP {}", response.status())
                        ));
                    }
                }
            }
            Err(e) => {
                eprintln!("Network error for {}: {}", url, e);
                if attempts >= max_retries {
                    return Err(DocLoaderError::Http(e));
                }
            }
        }

        // Wait before retrying with exponential backoff
        tokio::time::sleep(delay).await;
        delay = std::cmp::min(delay * 2, Duration::from_secs(30)); // Cap at 30 seconds
        attempts += 1;
    }
}
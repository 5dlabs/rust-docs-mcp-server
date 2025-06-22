use scraper::{Html, Selector};
use std::collections::HashMap;
use thiserror::Error;
use reqwest;
use tokio;

#[derive(Debug, Error)]
pub enum DocLoaderError {
    #[error("HTTP Error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("CSS selector error: {0}")]
    Selector(String),
    #[error("Parsing error: {0}")]
    Parsing(String),
}

// Simple struct to hold document content
#[derive(Debug, Clone)]
pub struct Document {
    pub path: String,
    pub content: String,
}

/// Loads documentation for a crate from docs.rs
/// This is a simplified version that doesn't require the cargo crate
pub async fn load_documents_from_docs_rs(
    crate_name: &str,
    _crate_version_req: &str, // We'll use latest version from docs.rs
    _features: Option<&Vec<String>>, // Features not supported in this simple version
) -> Result<Vec<Document>, DocLoaderError> {
    let client = reqwest::Client::new();

    // Start with the main crate page
    let base_url = format!("https://docs.rs/{}/latest/{}/", crate_name, crate_name);

    eprintln!("Fetching documentation from docs.rs for crate: {}", crate_name);

    let mut documents = Vec::new();
    let mut visited_urls = std::collections::HashSet::new();
    let mut urls_to_visit = vec![base_url.clone()];

    // Define the CSS selector for the main content area
    let content_selector = Selector::parse("div.docblock, section.docblock, .rustdoc .docblock")
        .map_err(|e| DocLoaderError::Selector(e.to_string()))?;

    // Limit the number of pages to avoid infinite loops
    let max_pages = 50;
    let mut processed = 0;

    while let Some(url) = urls_to_visit.pop() {
        if processed >= max_pages {
            eprintln!("Reached maximum page limit ({}), stopping", max_pages);
            break;
        }

        if visited_urls.contains(&url) {
            continue;
        }

        visited_urls.insert(url.clone());
        processed += 1;

        eprintln!("Processing page {}/{}: {}", processed, max_pages, url);

        // Fetch the page
        let response = match client.get(&url).send().await {
            Ok(resp) => resp,
            Err(e) => {
                eprintln!("Failed to fetch {}: {}", url, e);
                continue;
            }
        };

        if !response.status().is_success() {
            eprintln!("HTTP error for {}: {}", url, response.status());
            continue;
        }

        let html_content = match response.text().await {
            Ok(content) => content,
            Err(e) => {
                eprintln!("Failed to read response body for {}: {}", url, e);
                continue;
            }
        };

        let document = Html::parse_document(&html_content);

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

            documents.push(Document {
                path: relative_path,
                content: page_content.join("\n\n"),
            });
        }

        // Find links to other documentation pages (limited scope to avoid too many pages)
        if processed < max_pages / 2 { // Only follow links for first half of pages
            let link_selector = Selector::parse("a[href]")
                .map_err(|e| DocLoaderError::Selector(e.to_string()))?;

            for link_element in document.select(&link_selector) {
                if let Some(href) = link_element.value().attr("href") {
                    // Only follow links that are relative and look like documentation
                    if href.starts_with("./") || href.starts_with("../") {
                        if let Ok(absolute_url) = reqwest::Url::parse(&url) {
                            if let Ok(new_url) = absolute_url.join(href) {
                                let new_url_str = new_url.to_string();
                                if new_url_str.contains("docs.rs") &&
                                   new_url_str.contains(crate_name) &&
                                   !visited_urls.contains(&new_url_str) {
                                    urls_to_visit.push(new_url_str);
                                }
                            }
                        }
                    }
                }
            }
        }

        // Add a small delay to be respectful to docs.rs
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }

    eprintln!("Finished loading {} documents from docs.rs", documents.len());
    Ok(documents)
}

/// Synchronous wrapper that uses tokio runtime
pub fn load_documents(
    crate_name: &str,
    crate_version_req: &str,
    features: Option<&Vec<String>>,
) -> Result<Vec<Document>, DocLoaderError> {
    let rt = tokio::runtime::Runtime::new()
        .map_err(|e| DocLoaderError::Parsing(format!("Failed to create tokio runtime: {}", e)))?;

    rt.block_on(load_documents_from_docs_rs(crate_name, crate_version_req, features))
}
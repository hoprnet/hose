pub mod channels;
pub mod subscriptions;

use std::ops::Deref;

use blokli_client::{BlokliClient as InnerBlokliClient, BlokliClientConfig, errors::BlokliClientError, exports::Url};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum BlokliError {
    #[error("{0}")]
    Client(#[from] BlokliClientError),

    #[error("invalid indexer endpoint URL: {0}")]
    InvalidEndpoint(String),

    #[error("Indexer endpoint not configured")]
    NotConfigured,
}

/// Client for querying the Blokli indexer with typed APIs.
#[derive(Debug, Clone)]
pub struct BlokliClient {
    inner: InnerBlokliClient,
}

impl BlokliClient {
    /// Create a new Blokli client from either a base URL or `/graphql` endpoint URL.
    pub fn new(endpoint: String) -> Result<Self, BlokliError> {
        let base_url = normalize_base_url(&endpoint).map_err(|e| BlokliError::InvalidEndpoint(e.to_string()))?;
        let cfg = BlokliClientConfig {
            auto_compatibility_check: false,
            ..BlokliClientConfig::default()
        };

        Ok(Self {
            inner: InnerBlokliClient::new(base_url, cfg),
        })
    }
}

impl Deref for BlokliClient {
    type Target = InnerBlokliClient;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

fn normalize_base_url(endpoint: &str) -> Result<Url, String> {
    let mut url = Url::parse(endpoint).map_err(|e| e.to_string())?;
    if url.path().ends_with("/graphql") {
        let trimmed = url.path().trim_end_matches("/graphql").to_string();
        let new_path = if trimmed.is_empty() { "/" } else { trimmed.as_str() };
        url.set_path(new_path);
    }
    Ok(url)
}

pub mod channels;

use reqwest::Client;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum BlokliError {
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),

    #[error("GraphQL error: {0}")]
    GraphQL(String),

    #[error("Indexer endpoint not configured")]
    NotConfigured,
}

/// A raw GraphQL request payload.
#[derive(Debug, Serialize)]
struct GraphQLRequest {
    query: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    variables: Option<serde_json::Value>,
}

/// A raw GraphQL response envelope.
#[derive(Debug, Deserialize)]
struct GraphQLResponse<T> {
    data: Option<T>,
    errors: Option<Vec<GraphQLError>>,
}

/// A single GraphQL error entry.
#[derive(Debug, Deserialize)]
struct GraphQLError {
    message: String,
}

/// Client for querying the Blokli blockchain indexer.
#[derive(Debug, Clone)]
pub struct BlokliClient {
    http: Client,
    endpoint: String,
}

impl BlokliClient {
    /// Create a new Blokli client targeting the given GraphQL endpoint.
    pub fn new(endpoint: String) -> Self {
        Self {
            http: Client::new(),
            endpoint,
        }
    }

    /// Execute a GraphQL query and deserialize the response data.
    pub async fn query<T: serde::de::DeserializeOwned>(
        &self,
        query: &str,
        variables: Option<serde_json::Value>,
    ) -> Result<T, BlokliError> {
        let request = GraphQLRequest {
            query: query.to_string(),
            variables,
        };

        let response: GraphQLResponse<T> = self
            .http
            .post(&self.endpoint)
            .json(&request)
            .send()
            .await?
            .json()
            .await?;

        if let Some(errors) = response.errors {
            if !errors.is_empty() {
                let messages: Vec<String> = errors.into_iter().map(|e| e.message).collect();
                return Err(BlokliError::GraphQL(messages.join("; ")));
            }
        }

        response
            .data
            .ok_or_else(|| BlokliError::GraphQL("no data in response".to_string()))
    }
}

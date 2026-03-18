use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::blokli::{BlokliClient, BlokliError};

/// Bidirectional mapping between blockchain key IDs and peer IDs.
#[derive(Debug, Clone)]
pub struct IdentityBridge {
    /// key_id -> peer_id
    key_to_peer: Arc<RwLock<HashMap<String, String>>>,
    /// peer_id -> key_id
    peer_to_key: Arc<RwLock<HashMap<String, String>>>,
    client: Option<BlokliClient>,
}

impl IdentityBridge {
    pub fn new(client: Option<BlokliClient>) -> Self {
        Self {
            key_to_peer: Arc::new(RwLock::new(HashMap::new())),
            peer_to_key: Arc::new(RwLock::new(HashMap::new())),
            client,
        }
    }

    /// Look up a peer ID from a blockchain key ID. Returns cached value or queries indexer.
    pub async fn peer_id_for_key(&self, key_id: &str) -> Result<Option<String>, BlokliError> {
        // Check cache first
        {
            let cache = self.key_to_peer.read().await;
            if let Some(peer_id) = cache.get(key_id) {
                return Ok(Some(peer_id.clone()));
            }
        }

        // Query indexer if available
        let Some(client) = &self.client else {
            return Err(BlokliError::NotConfigured);
        };

        let query = r#"query($keyId: String!) {
            account(id: $keyId) {
                peerId
            }
        }"#;

        let variables = serde_json::json!({ "keyId": key_id });

        #[derive(serde::Deserialize)]
        struct AccountResponse {
            account: Option<AccountData>,
        }

        #[derive(serde::Deserialize)]
        struct AccountData {
            #[serde(rename = "peerId")]
            peer_id: Option<String>,
        }

        let response: AccountResponse = client.query(query, Some(variables)).await?;

        if let Some(account) = response.account
            && let Some(peer_id) = account.peer_id
        {
            // Update both caches
            self.key_to_peer
                .write()
                .await
                .insert(key_id.to_string(), peer_id.clone());
            self.peer_to_key
                .write()
                .await
                .insert(peer_id.clone(), key_id.to_string());
            return Ok(Some(peer_id));
        }

        Ok(None)
    }

    /// Look up a blockchain key ID from a peer ID.
    pub async fn key_id_for_peer(&self, peer_id: &str) -> Option<String> {
        self.peer_to_key.read().await.get(peer_id).cloned()
    }

    /// Manually insert a known mapping (e.g., from telemetry attributes).
    pub async fn insert_mapping(&self, key_id: String, peer_id: String) {
        self.key_to_peer
            .write()
            .await
            .insert(key_id.clone(), peer_id.clone());
        self.peer_to_key.write().await.insert(peer_id, key_id);
    }

    /// Get all cached mappings.
    pub async fn cached_mappings(&self) -> HashMap<String, String> {
        self.key_to_peer.read().await.clone()
    }
}

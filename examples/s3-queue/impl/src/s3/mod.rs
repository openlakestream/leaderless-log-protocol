pub mod minio;

use async_trait::async_trait;
use bytes::Bytes;
use chrono::{DateTime, Utc};

use crate::error::{Result, S3QueueError};

/// Result of a conditional put operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CasResult {
    Ok { etag: String },
    PreconditionFailed,
}

/// Object metadata from list operations.
#[derive(Debug, Clone)]
pub struct ObjectInfo {
    pub key: String,
    pub last_modified: DateTime<Utc>,
}

/// Result of a list objects operation.
#[derive(Debug, Clone)]
pub struct ListObjectsResult {
    pub contents: Vec<ObjectInfo>,
    pub is_truncated: bool,
}

/// S3-compatible storage interface (§5.4).
#[async_trait]
pub trait S3Client: Send + Sync {
    /// Get object content and ETag. Returns None if not found.
    async fn get(&self, key: &str) -> Result<Option<(Bytes, String)>>;

    /// Put with If-Match precondition.
    async fn put_conditional_match(
        &self,
        key: &str,
        value: Bytes,
        etag: &str,
    ) -> Result<CasResult>;

    /// Put with If-None-Match: * precondition.
    async fn put_conditional_none_match(&self, key: &str, value: Bytes) -> Result<CasResult>;

    /// Unconditional put.
    async fn put(&self, key: &str, value: Bytes) -> Result<()>;

    /// Delete single object (idempotent).
    async fn delete(&self, key: &str) -> Result<()>;

    /// Batch delete up to 1000 keys (idempotent).
    async fn batch_delete(&self, keys: &[String]) -> Result<()>;

    /// List objects with prefix, start-after, and max-keys.
    async fn list_objects(
        &self,
        prefix: &str,
        start_after: &str,
        max_keys: i32,
    ) -> Result<ListObjectsResult>;
}

/// Helper: get an object and deserialize from JSON.
pub async fn get_json<T: serde::de::DeserializeOwned>(
    client: &dyn S3Client,
    key: &str,
) -> Result<Option<(T, String)>> {
    match client.get(key).await? {
        Some((data, etag)) => {
            let value: T = serde_json::from_slice(&data)
                .map_err(|e| S3QueueError::SerializationError(e.to_string()))?;
            Ok(Some((value, etag)))
        }
        None => Ok(None),
    }
}

/// Helper: serialize to JSON and put.
pub async fn put_json<T: serde::Serialize>(
    client: &dyn S3Client,
    key: &str,
    value: &T,
) -> Result<()> {
    let data = serde_json::to_vec(value)
        .map_err(|e| S3QueueError::SerializationError(e.to_string()))?;
    client.put(key, Bytes::from(data)).await
}

/// Helper: serialize to JSON and put with If-Match.
pub async fn put_json_conditional_match<T: serde::Serialize>(
    client: &dyn S3Client,
    key: &str,
    value: &T,
    etag: &str,
) -> Result<CasResult> {
    let data = serde_json::to_vec(value)
        .map_err(|e| S3QueueError::SerializationError(e.to_string()))?;
    client
        .put_conditional_match(key, Bytes::from(data), etag)
        .await
}

/// Helper: serialize to JSON and put with If-None-Match: *.
pub async fn put_json_conditional_none_match<T: serde::Serialize>(
    client: &dyn S3Client,
    key: &str,
    value: &T,
) -> Result<CasResult> {
    let data = serde_json::to_vec(value)
        .map_err(|e| S3QueueError::SerializationError(e.to_string()))?;
    client
        .put_conditional_none_match(key, Bytes::from(data))
        .await
}

use bytes::Bytes;
use std::time::Duration;
use tokio::time::sleep;

use crate::config::S3QueueConfig;
use crate::error::{Result, S3QueueError};
use crate::keys;
use crate::model::{MetaSequenceCounter, PendingEntry};
use crate::s3::{CasResult, S3Client};

/// Materialize a pending index entry to its own S3 key (§16.1).
async fn materialize_pending(client: &dyn S3Client, topic: &str, pending: &PendingEntry) -> Result<()> {
    let index_key = keys::index_key(topic, pending.offset);
    let entry = pending.to_index_entry();
    let data = serde_json::to_vec(&entry)
        .map_err(|e| S3QueueError::SerializationError(e.to_string()))?;
    // Put is idempotent for same content; safe to re-execute
    client.put(&index_key, Bytes::from(data)).await
}

/// AtomicIncrementWithPending (§8.1, §16.1).
///
/// Atomically increments the sequence counter AND records a pending index entry
/// in a single CAS. Returns the new counter value.
pub async fn atomic_increment_with_pending(
    client: &dyn S3Client,
    topic: &str,
    delta: u64,
    pending_entry: PendingEntry,
    config: &S3QueueConfig,
) -> Result<u64> {
    let counter_key = keys::sequence_counter_key(topic);

    for attempt in 0..config.max_cas_retries {
        // Read current counter
        let (data, etag) = client
            .get(&counter_key)
            .await?
            .ok_or(S3QueueError::TopicNotInitialized)?;

        let counter: MetaSequenceCounter = serde_json::from_slice(&data)
            .map_err(|e| S3QueueError::SerializationError(e.to_string()))?;

        // If a previous pending entry exists, materialize it first
        if let Some(ref prev_pending) = counter.pending {
            let _ = materialize_pending(client, topic, prev_pending).await;
        }

        // Compute new counter
        let new_value = counter.value + delta;
        let new_counter = MetaSequenceCounter {
            value: new_value,
            pending: Some(pending_entry.clone()),
        };

        let new_data = serde_json::to_vec(&new_counter)
            .map_err(|e| S3QueueError::SerializationError(e.to_string()))?;

        match client
            .put_conditional_match(&counter_key, Bytes::from(new_data), &etag)
            .await?
        {
            CasResult::Ok { .. } => {
                // Best-effort materialization (if this fails, next caller handles it)
                let _ = materialize_pending(client, topic, &pending_entry).await;
                return Ok(new_value);
            }
            CasResult::PreconditionFailed => {
                if attempt + 1 < config.max_cas_retries {
                    sleep(Duration::from_millis(config.backoff_ms(attempt))).await;
                }
            }
        }
    }

    Err(S3QueueError::MaxRetriesExceeded {
        retries: config.max_cas_retries,
    })
}

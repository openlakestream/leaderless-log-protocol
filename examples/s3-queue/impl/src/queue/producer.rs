use bytes::Bytes;
use uuid::Uuid;

use crate::config::S3QueueConfig;
use crate::error::{Result, S3QueueError};
use crate::keys;
use crate::model::{EntryType, LogState, Message, MetaLogState, MetaSequenceCounter, PendingEntry};
use crate::s3::{get_json, CasResult, S3Client};

/// Produce a single message (§10, §16.6).
///
/// Returns the assigned offset.
pub async fn produce(
    client: &dyn S3Client,
    topic: &str,
    message: &Message,
    config: &S3QueueConfig,
) -> Result<u64> {
    // Step 1: WALWrite — write data to UUID-keyed object
    let uuid = Uuid::new_v4().to_string();
    let data_key = keys::wal_data_key(topic, &uuid);
    let data = serde_json::to_vec(message)
        .map_err(|e| S3QueueError::SerializationError(e.to_string()))?;
    client.put(&data_key, Bytes::from(data)).await?;

    // Step 2: Fence check
    let (log_state, _) = get_json::<MetaLogState>(client, &keys::log_state_key(topic))
        .await?
        .ok_or(S3QueueError::TopicNotInitialized)?;

    if log_state.state == LogState::Fenced {
        return Err(S3QueueError::LogFenced);
    }

    // Step 3: AtomicIncrementWithPending (inlined to compute offset inside CAS loop)
    produce_internal(client, topic, &data_key, 1, config).await
}

/// Produce a batch of messages (§10.4, §16.7).
///
/// Returns the end offset (last offset in the batch).
pub async fn produce_batch(
    client: &dyn S3Client,
    topic: &str,
    messages: &[Message],
    config: &S3QueueConfig,
) -> Result<u64> {
    if messages.is_empty() {
        return Err(S3QueueError::EmptyBatch);
    }
    if messages.len() > config.max_batch_size {
        return Err(S3QueueError::BatchTooLarge {
            reason: format!(
                "batch size {} exceeds max {}",
                messages.len(),
                config.max_batch_size
            ),
        });
    }

    let total_bytes: usize = messages.iter().map(|m| m.payload.len()).sum();
    if total_bytes > config.max_batch_bytes {
        return Err(S3QueueError::BatchTooLarge {
            reason: format!(
                "batch bytes {} exceeds max {}",
                total_bytes, config.max_batch_bytes
            ),
        });
    }

    let batch_size = messages.len() as u64;

    // WALWrite: write NDJSON
    let uuid = Uuid::new_v4().to_string();
    let data_key = keys::wal_data_key(topic, &uuid);
    let ndjson = messages
        .iter()
        .map(|m| serde_json::to_string(m).unwrap())
        .collect::<Vec<_>>()
        .join("\n");
    client.put(&data_key, Bytes::from(ndjson)).await?;

    // Fence check
    let (log_state, _) = get_json::<MetaLogState>(client, &keys::log_state_key(topic))
        .await?
        .ok_or(S3QueueError::TopicNotInitialized)?;

    if log_state.state == LogState::Fenced {
        return Err(S3QueueError::LogFenced);
    }

    produce_internal(client, topic, &data_key, batch_size, config).await
}

/// Internal produce: CAS loop on sequence counter with inline pending entry construction.
///
/// The pending entry's offset is computed inside the CAS loop because it depends
/// on the current counter value (§16.1, §16.6).
async fn produce_internal(
    client: &dyn S3Client,
    topic: &str,
    data_key: &str,
    batch_size: u64,
    config: &S3QueueConfig,
) -> Result<u64> {
    use std::time::Duration;
    use tokio::time::sleep;

    let counter_key = keys::sequence_counter_key(topic);

    for attempt in 0..config.max_cas_retries {
        let (data, etag) = client
            .get(&counter_key)
            .await?
            .ok_or(S3QueueError::TopicNotInitialized)?;

        let counter: MetaSequenceCounter = serde_json::from_slice(&data)
            .map_err(|e| S3QueueError::SerializationError(e.to_string()))?;

        // Materialize any existing pending entry
        if let Some(ref prev_pending) = counter.pending {
            let idx_key = keys::index_key(topic, prev_pending.offset);
            let entry = prev_pending.to_index_entry();
            let entry_data = serde_json::to_vec(&entry)
                .map_err(|e| S3QueueError::SerializationError(e.to_string()))?;
            let _ = client.put(&idx_key, Bytes::from(entry_data)).await;
        }

        let new_value = counter.value + batch_size;
        let end_offset = new_value - 1;

        let pending = PendingEntry {
            offset: end_offset,
            entry_type: EntryType::Wal,
            msg_count: batch_size,
            data_key: data_key.to_string(),
        };

        let new_counter = MetaSequenceCounter {
            value: new_value,
            pending: Some(pending.clone()),
        };

        let new_data = serde_json::to_vec(&new_counter)
            .map_err(|e| S3QueueError::SerializationError(e.to_string()))?;

        match client
            .put_conditional_match(&counter_key, Bytes::from(new_data), &etag)
            .await?
        {
            CasResult::Ok { .. } => {
                // Best-effort materialization
                let idx_key = keys::index_key(topic, end_offset);
                let entry = pending.to_index_entry();
                let entry_data = serde_json::to_vec(&entry)
                    .map_err(|e| S3QueueError::SerializationError(e.to_string()))?;
                let _ = client.put(&idx_key, Bytes::from(entry_data)).await;

                return Ok(end_offset);
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

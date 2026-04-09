use bytes::Bytes;

use crate::config::S3QueueConfig;
use crate::coordination::ceiling_get::ceiling_get;
use crate::error::{AckResult, Result, S3QueueError};
use crate::keys;
use crate::model::{ConsumerCursor, Message};
use crate::s3::{get_json, put_json_conditional_none_match, CasResult, S3Client};

/// Result of a Consume operation.
#[derive(Debug, Clone)]
pub struct ConsumeResult {
    pub offset: u64,
    pub message: Message,
}

/// Consume the next message for a consumer (§11.1, §16.8).
///
/// Returns the message at the cursor position WITHOUT advancing the cursor.
/// Returns None if no message is available.
pub async fn consume(
    client: &dyn S3Client,
    topic: &str,
    consumer_id: &str,
    config: &S3QueueConfig,
) -> Result<Option<ConsumeResult>> {
    let cursor_key = keys::consumer_cursor_key(topic, consumer_id);

    // Read cursor, create if not found
    let cursor = match get_json::<ConsumerCursor>(client, &cursor_key).await? {
        Some((cursor, _etag)) => cursor,
        None => {
            // First consume: create cursor at offset 1
            let initial = ConsumerCursor { offset: 1 };
            let _ = put_json_conditional_none_match(client, &cursor_key, &initial).await?;
            // Re-read to get the actual value (in case of concurrent init)
            get_json::<ConsumerCursor>(client, &cursor_key)
                .await?
                .ok_or(S3QueueError::TopicNotInitialized)?
                .0
        }
    };

    // CeilingGet to find the covering index entry
    let result = ceiling_get(client, topic, cursor.offset, config).await?;

    let (entry_offset, entry) = match result {
        Some(r) => r,
        None => return Ok(None),
    };

    // Compute position within batch
    let start_offset = entry_offset - entry.msg_count + 1;
    let position_in_batch = (cursor.offset - start_offset) as usize;

    // Read the data object
    let (data, _) = client
        .get(&entry.data_key)
        .await?
        .ok_or(S3QueueError::OffsetNotFound)?;

    let message = if entry.msg_count == 1 {
        serde_json::from_slice(&data)
            .map_err(|e| S3QueueError::SerializationError(e.to_string()))?
    } else {
        // NDJSON: parse all messages, pick the one at position
        let text = String::from_utf8_lossy(&data);
        let messages: Vec<Message> = text
            .lines()
            .map(|line| serde_json::from_str(line))
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| S3QueueError::SerializationError(e.to_string()))?;

        messages
            .into_iter()
            .nth(position_in_batch)
            .ok_or(S3QueueError::OffsetNotFound)?
    };

    Ok(Some(ConsumeResult {
        offset: cursor.offset,
        message,
    }))
}

/// Acknowledge a consumed message (§11.2, §16.9).
///
/// Advances the consumer's cursor if the offset matches.
pub async fn acknowledge(
    client: &dyn S3Client,
    topic: &str,
    consumer_id: &str,
    offset: u64,
    config: &S3QueueConfig,
) -> Result<AckResult> {
    let cursor_key = keys::consumer_cursor_key(topic, consumer_id);

    for _attempt in 0..config.max_cas_retries {
        let (cursor, etag) = get_json::<ConsumerCursor>(client, &cursor_key)
            .await?
            .ok_or(S3QueueError::TopicNotInitialized)?;

        // Already past this offset
        if offset + 1 <= cursor.offset {
            return Ok(AckResult::AckAlreadyProcessed);
        }

        // Wrong offset
        if offset != cursor.offset {
            return Ok(AckResult::AckOffsetMismatch {
                expected: cursor.offset,
                actual: offset,
            });
        }

        // Advance cursor
        let new_cursor = ConsumerCursor {
            offset: offset + 1,
        };
        let data = serde_json::to_vec(&new_cursor)
            .map_err(|e| S3QueueError::SerializationError(e.to_string()))?;

        match client
            .put_conditional_match(&cursor_key, Bytes::from(data), &etag)
            .await?
        {
            CasResult::Ok { .. } => return Ok(AckResult::AckAdvanced),
            CasResult::PreconditionFailed => {
                // Another process modified the cursor; retry
                continue;
            }
        }
    }

    Err(S3QueueError::MaxRetriesExceeded {
        retries: config.max_cas_retries,
    })
}

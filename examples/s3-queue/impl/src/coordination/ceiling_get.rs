use crate::config::S3QueueConfig;
use crate::error::{Result, S3QueueError};
use crate::keys;
use crate::model::{IndexEntry, MetaSequenceCounter};
use crate::s3::{get_json, S3Client};

/// CeilingGet (§8.2, §16.2).
///
/// Finds the smallest index key whose offset >= target_offset.
/// Returns (entryOffset, IndexEntry) or None if no entry at or after target.
///
/// If the index has no covering entry, falls back to checking the sequence
/// counter's pending field — a pending entry represents an offset that has
/// been assigned (CAS succeeded) but whose index entry has not yet been
/// materialized. This closes the visibility gap described in SPEC §8.2.
pub async fn ceiling_get(
    client: &dyn S3Client,
    topic: &str,
    target_offset: u64,
    config: &S3QueueConfig,
) -> Result<Option<(u64, IndexEntry)>> {
    let prefix = keys::index_prefix(topic);

    for _retry in 0..config.ceiling_get_max_retries {
        let start_after = if target_offset == 0 {
            prefix.clone()
        } else {
            format!("{}{}", prefix, keys::pad20(target_offset - 1))
        };

        let results = client.list_objects(&prefix, &start_after, 1).await?;

        if results.contents.is_empty() {
            // No index entry found; check the pending field in the sequence
            // counter before giving up. A pending entry means the offset was
            // assigned but index materialization hasn't completed yet.
            return check_pending_entry(client, topic, target_offset).await;
        }

        let key = &results.contents[0].key;
        let entry_offset = keys::parse_offset_from_key(key)
            .ok_or_else(|| S3QueueError::SerializationError(format!("bad index key: {}", key)))?;

        // Read the index entry
        match get_json::<IndexEntry>(client, key).await? {
            Some((entry, _etag)) => {
                // Verify the entry covers the target offset
                let start_offset = entry_offset - entry.msg_count + 1;
                if target_offset < start_offset {
                    return Ok(None);
                }
                return Ok(Some((entry_offset, entry)));
            }
            None => {
                // Key deleted between LIST and GET (compaction race); retry
                continue;
            }
        }
    }

    Err(S3QueueError::MaxRetriesExceeded {
        retries: config.ceiling_get_max_retries,
    })
}

/// Check the sequence counter's pending field for an unmaterialized entry
/// that covers the target offset. If found, materialize it and return it.
async fn check_pending_entry(
    client: &dyn S3Client,
    topic: &str,
    target_offset: u64,
) -> Result<Option<(u64, IndexEntry)>> {
    let counter_key = keys::sequence_counter_key(topic);

    let counter: MetaSequenceCounter = match get_json(client, &counter_key).await? {
        Some((c, _)) => c,
        None => return Ok(None),
    };

    if let Some(pending) = &counter.pending {
        let start = pending.offset - pending.msg_count + 1;
        if target_offset >= start && target_offset <= pending.offset {
            // Pending entry covers the target offset. Materialize it
            // best-effort, then return it regardless of materialization success.
            let index_key = keys::index_key(topic, pending.offset);
            let entry = pending.to_index_entry();
            let data = serde_json::to_vec(&entry)
                .map_err(|e| S3QueueError::SerializationError(e.to_string()))?;
            let _ = client.put(&index_key, bytes::Bytes::from(data)).await;
            return Ok(Some((pending.offset, entry)));
        }
    }

    Ok(None)
}

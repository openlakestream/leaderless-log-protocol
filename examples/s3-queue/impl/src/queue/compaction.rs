use bytes::Bytes;

use crate::config::S3QueueConfig;
use crate::coordination::ceiling_get::ceiling_get;
use crate::coordination::range_delete::range_delete;
use crate::error::{Result, S3QueueError};
use crate::keys;
use crate::model::{EntryType, IndexEntry, Message, MetaCompactionCursor};
use crate::s3::{get_json, put_json, S3Client};

/// Compact a range of offsets (§12, §16.10).
///
/// Three-step update: write compacted → delete old → advance cursor.
pub async fn compact(
    client: &dyn S3Client,
    topic: &str,
    start: u64,
    end: u64,
    config: &S3QueueConfig,
) -> Result<()> {
    // Step 0: Validate range — collect all entries
    let mut entries: Vec<(u64, IndexEntry)> = Vec::new();
    let mut offset = start;
    while offset <= end {
        let result = ceiling_get(client, topic, offset, config).await?;
        let (entry_offset, entry) = result.ok_or(S3QueueError::GapInRange { offset })?;

        if entry.entry_type != EntryType::Wal {
            return Err(S3QueueError::RangeNotAllWal {
                offset: entry_offset,
            });
        }

        let entry_start = entry_offset - entry.msg_count + 1;
        if entry_start > offset {
            return Err(S3QueueError::GapInRange { offset });
        }

        entries.push((entry_offset, entry));
        offset = entry_offset + 1;
    }

    // Step 0b: Read all WAL data and merge
    let mut merged_messages: Vec<Message> = Vec::new();
    for (_, entry) in &entries {
        let (data, _) = client
            .get(&entry.data_key)
            .await?
            .ok_or(S3QueueError::OffsetNotFound)?;

        if entry.msg_count == 1 {
            let msg: Message = serde_json::from_slice(&data)
                .map_err(|e| S3QueueError::SerializationError(e.to_string()))?;
            merged_messages.push(msg);
        } else {
            let text = String::from_utf8_lossy(&data);
            for line in text.lines() {
                let msg: Message = serde_json::from_str(line)
                    .map_err(|e| S3QueueError::SerializationError(e.to_string()))?;
                merged_messages.push(msg);
            }
        }
    }

    // Step 1: Write compacted data and index
    let compacted_key = keys::compacted_data_key(topic, start, end);
    let ndjson = merged_messages
        .iter()
        .map(|m| serde_json::to_string(m).unwrap())
        .collect::<Vec<_>>()
        .join("\n");
    client.put(&compacted_key, Bytes::from(ndjson)).await?;

    let compacted_entry = IndexEntry {
        entry_type: EntryType::Compacted,
        msg_count: end - start + 1,
        data_key: compacted_key,
    };
    put_json(client, &keys::index_key(topic, end), &compacted_entry).await?;

    // Step 2: Delete old index entries [start, end) and optionally old WAL data
    range_delete(client, topic, start, end).await?;

    for (_, entry) in &entries {
        let _ = client.delete(&entry.data_key).await;
    }

    // Step 3: Advance compaction cursor
    advance_compaction_cursor(client, topic, end + 1, config).await?;

    Ok(())
}

/// CompactWithRecovery (§16.11).
///
/// Detects incomplete compaction and resumes from the appropriate step.
pub async fn compact_with_recovery(
    client: &dyn S3Client,
    topic: &str,
    start: u64,
    end: u64,
    config: &S3QueueConfig,
) -> Result<()> {
    let index_at_end = get_json::<IndexEntry>(client, &keys::index_key(topic, end)).await?;
    let cursor = get_json::<MetaCompactionCursor>(client, &keys::compaction_cursor_key(topic))
        .await?
        .map(|(c, _)| c);

    match index_at_end {
        None => {
            // Not started; run full compaction
            compact(client, topic, start, end, config).await
        }
        Some((entry, _)) if entry.entry_type == EntryType::Wal => {
            // WAL entry at end; compaction not started
            compact(client, topic, start, end, config).await
        }
        Some((entry, _))
            if entry.entry_type == EntryType::Compacted
                && cursor.map_or(true, |c| c.value <= end) =>
        {
            // Verify range matches
            if entry.msg_count != (end - start + 1) {
                return Err(S3QueueError::CompactionRangeMismatch);
            }
            // Verify compacted data exists
            if client.get(&entry.data_key).await?.is_none() {
                return Err(S3QueueError::CompactedDataMissing);
            }

            // Resume from step 2
            range_delete(client, topic, start, end).await?;
            advance_compaction_cursor(client, topic, end + 1, config).await?;
            Ok(())
        }
        _ => {
            // Fully complete; no-op
            Ok(())
        }
    }
}

/// Advance compaction cursor using max() guard (§12.2).
async fn advance_compaction_cursor(
    client: &dyn S3Client,
    topic: &str,
    new_value: u64,
    config: &S3QueueConfig,
) -> Result<()> {
    use crate::coordination::cas::compare_and_swap;

    let cursor_key = keys::compaction_cursor_key(topic);
    compare_and_swap(
        client,
        &cursor_key,
        |data| {
            let cursor: MetaCompactionCursor = serde_json::from_slice(data)
                .map_err(|e| S3QueueError::SerializationError(e.to_string()))?;
            let updated = MetaCompactionCursor {
                value: cursor.value.max(new_value),
            };
            let bytes = serde_json::to_vec(&updated)
                .map_err(|e| S3QueueError::SerializationError(e.to_string()))?;
            Ok(Bytes::from(bytes))
        },
        config,
    )
    .await?;

    Ok(())
}

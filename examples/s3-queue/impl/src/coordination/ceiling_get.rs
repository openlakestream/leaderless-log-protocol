use crate::config::S3QueueConfig;
use crate::error::{Result, S3QueueError};
use crate::keys;
use crate::model::IndexEntry;
use crate::s3::{get_json, S3Client};

/// CeilingGet (§8.2, §16.2).
///
/// Finds the smallest index key whose offset >= target_offset.
/// Returns (entryOffset, IndexEntry) or None if no entry at or after target.
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
            return Ok(None);
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

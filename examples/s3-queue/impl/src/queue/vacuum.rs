use chrono::Utc;

use crate::error::Result;
use crate::keys;
use crate::model::IndexEntry;
use crate::s3::{get_json, S3Client};
use std::collections::HashSet;

/// VACUUM (§16.13). Cleans up orphan WAL data objects.
///
/// Returns the number of orphan objects deleted.
pub async fn vacuum(
    client: &dyn S3Client,
    topic: &str,
    retention_period_seconds: u64,
) -> Result<u64> {
    // Collect all WAL data keys
    let wal_prefix = keys::wal_data_prefix(topic);
    let mut wal_objects = Vec::new();
    let mut marker = String::new();

    loop {
        let page = client.list_objects(&wal_prefix, &marker, 1000).await?;
        for obj in &page.contents {
            wal_objects.push((obj.key.clone(), obj.last_modified));
        }
        if !page.is_truncated || page.contents.is_empty() {
            break;
        }
        marker = page.contents.last().unwrap().key.clone();
    }

    // Collect all referenced data keys from index entries
    let index_prefix = keys::index_prefix(topic);
    let mut referenced_keys = HashSet::new();
    let mut marker = String::new();

    loop {
        let page = client.list_objects(&index_prefix, &marker, 1000).await?;
        for obj in &page.contents {
            if let Some((entry, _)) = get_json::<IndexEntry>(client, &obj.key).await? {
                referenced_keys.insert(entry.data_key);
            }
        }
        if !page.is_truncated || page.contents.is_empty() {
            break;
        }
        marker = page.contents.last().unwrap().key.clone();
    }

    // Also check the sequence counter's pending field
    if let Some((counter, _)) = get_json::<crate::model::MetaSequenceCounter>(
        client,
        &keys::sequence_counter_key(topic),
    )
    .await?
    {
        if let Some(pending) = counter.pending {
            referenced_keys.insert(pending.data_key);
        }
    }

    // Delete unreferenced WAL data objects older than retention
    let now = Utc::now();
    let mut deleted = 0u64;

    for (key, last_modified) in &wal_objects {
        if !referenced_keys.contains(key.as_str()) {
            let age = now.signed_duration_since(*last_modified);
            if age.num_seconds() > retention_period_seconds as i64 {
                client.delete(key).await?;
                deleted += 1;
            }
        }
    }

    Ok(deleted)
}

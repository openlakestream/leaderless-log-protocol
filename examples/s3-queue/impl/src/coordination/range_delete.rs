use crate::error::Result;
use crate::keys;
use crate::s3::S3Client;

/// RangeDelete (§8.4, §16.4).
///
/// Deletes all index entries in the half-open range [start, end).
pub async fn range_delete(
    client: &dyn S3Client,
    topic: &str,
    start: u64,
    end: u64,
) -> Result<()> {
    let prefix = keys::index_prefix(topic);
    let mut marker = format!("{}{}", prefix, keys::pad20(start.saturating_sub(1)));
    let mut all_keys = Vec::new();

    loop {
        let page = client.list_objects(&prefix, &marker, 1000).await?;

        for obj in &page.contents {
            if let Some(offset) = keys::parse_offset_from_key(&obj.key) {
                if offset >= end {
                    // Past the range; stop collecting
                    break;
                }
                if offset >= start {
                    all_keys.push(obj.key.clone());
                }
            }
        }

        // Check if we've gone past the range or no more pages
        if !page.is_truncated || page.contents.is_empty() {
            break;
        }

        // Check if last key is past range
        if let Some(last) = page.contents.last() {
            if let Some(offset) = keys::parse_offset_from_key(&last.key) {
                if offset >= end {
                    break;
                }
            }
            marker = last.key.clone();
        } else {
            break;
        }
    }

    // Batch delete in chunks of 1000
    for chunk in all_keys.chunks(1000) {
        client.batch_delete(&chunk.to_vec()).await?;
    }

    Ok(())
}

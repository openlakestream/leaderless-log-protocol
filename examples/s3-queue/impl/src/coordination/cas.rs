use bytes::Bytes;
use tokio::time::sleep;
use std::time::Duration;

use crate::config::S3QueueConfig;
use crate::error::{Result, S3QueueError};
use crate::s3::{CasResult, S3Client};

/// Generic CompareAndSwap (§8.3, §16.3).
///
/// Reads an S3 object, applies a transform function, writes back conditionally.
/// If the transform returns bytes identical to the original, returns without writing.
pub async fn compare_and_swap<F>(
    client: &dyn S3Client,
    key: &str,
    transform: F,
    config: &S3QueueConfig,
) -> Result<Bytes>
where
    F: Fn(&Bytes) -> Result<Bytes>,
{
    for attempt in 0..config.max_cas_retries {
        let (value, etag) = client
            .get(key)
            .await?
            .ok_or(S3QueueError::TopicNotInitialized)?;

        let new_value = transform(&value)?;

        // If no change needed, return current value
        if new_value == value {
            return Ok(value);
        }

        match client
            .put_conditional_match(key, new_value.clone(), &etag)
            .await?
        {
            CasResult::Ok { .. } => return Ok(new_value),
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

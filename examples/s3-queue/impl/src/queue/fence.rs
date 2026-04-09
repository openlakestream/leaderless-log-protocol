use bytes::Bytes;

use crate::config::S3QueueConfig;
use crate::coordination::cas::compare_and_swap;
use crate::error::{Result, S3QueueError};
use crate::keys;
use crate::model::{LogState, MetaLogState};

/// FenceLog (§13.2, §16.12). Halts all future writes.
pub async fn fence_log(
    client: &dyn crate::s3::S3Client,
    topic: &str,
    config: &S3QueueConfig,
) -> Result<()> {
    let key = keys::log_state_key(topic);
    compare_and_swap(
        client,
        &key,
        |data| {
            let state: MetaLogState = serde_json::from_slice(data)
                .map_err(|e| S3QueueError::SerializationError(e.to_string()))?;
            if state.state == LogState::Fenced {
                // Already fenced; no-op
                return Ok(Bytes::copy_from_slice(data));
            }
            let new_state = MetaLogState {
                state: LogState::Fenced,
                version: state.version + 1,
            };
            let bytes = serde_json::to_vec(&new_state)
                .map_err(|e| S3QueueError::SerializationError(e.to_string()))?;
            Ok(Bytes::from(bytes))
        },
        config,
    )
    .await?;
    Ok(())
}

/// UnfenceLog (§13.3, §16.12). Resumes writes.
pub async fn unfence_log(
    client: &dyn crate::s3::S3Client,
    topic: &str,
    config: &S3QueueConfig,
) -> Result<()> {
    let key = keys::log_state_key(topic);
    compare_and_swap(
        client,
        &key,
        |data| {
            let state: MetaLogState = serde_json::from_slice(data)
                .map_err(|e| S3QueueError::SerializationError(e.to_string()))?;
            if state.state == LogState::Open {
                // Already open; no-op
                return Ok(Bytes::copy_from_slice(data));
            }
            let new_state = MetaLogState {
                state: LogState::Open,
                version: state.version + 1,
            };
            let bytes = serde_json::to_vec(&new_state)
                .map_err(|e| S3QueueError::SerializationError(e.to_string()))?;
            Ok(Bytes::from(bytes))
        },
        config,
    )
    .await?;
    Ok(())
}

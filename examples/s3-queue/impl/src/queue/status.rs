use crate::error::{Result, S3QueueError};
use crate::keys;
use crate::model::{MetaCompactionCursor, MetaLogState, MetaSequenceCounter, TopicStatus};
use crate::s3::{get_json, S3Client};

/// Read and return topic status (all meta objects).
pub async fn get_status(client: &dyn S3Client, topic: &str) -> Result<TopicStatus> {
    let (counter, _) =
        get_json::<MetaSequenceCounter>(client, &keys::sequence_counter_key(topic))
            .await?
            .ok_or(S3QueueError::TopicNotInitialized)?;

    let (log_state, _) = get_json::<MetaLogState>(client, &keys::log_state_key(topic))
        .await?
        .ok_or(S3QueueError::TopicNotInitialized)?;

    let (cursor, _) =
        get_json::<MetaCompactionCursor>(client, &keys::compaction_cursor_key(topic))
            .await?
            .ok_or(S3QueueError::TopicNotInitialized)?;

    Ok(TopicStatus {
        sequence_counter: counter,
        log_state,
        compaction_cursor: cursor,
    })
}

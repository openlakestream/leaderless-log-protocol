use crate::error::Result;
use crate::keys;
use crate::model::{LogState, MetaCompactionCursor, MetaLogState, MetaSequenceCounter};
use crate::s3::{put_json_conditional_none_match, S3Client};

/// InitializeTopic (§13.1, §16.5).
///
/// Creates all required meta objects for a topic. Idempotent — safe to call
/// concurrently or repeatedly.
pub async fn initialize_topic(client: &dyn S3Client, topic: &str) -> Result<()> {
    let counter = MetaSequenceCounter {
        value: 1,
        pending: None,
    };
    let _ = put_json_conditional_none_match(client, &keys::sequence_counter_key(topic), &counter)
        .await?;

    let log_state = MetaLogState {
        state: LogState::Open,
        version: 0,
    };
    let _ =
        put_json_conditional_none_match(client, &keys::log_state_key(topic), &log_state).await?;

    let cursor = MetaCompactionCursor { value: 1 };
    let _ = put_json_conditional_none_match(
        client,
        &keys::compaction_cursor_key(topic),
        &cursor,
    )
    .await?;

    Ok(())
}

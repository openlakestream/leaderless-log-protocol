use thiserror::Error;

#[derive(Debug, Error)]
pub enum S3QueueError {
    // S3 layer errors
    #[error("S3 operation failed: {0}")]
    S3Error(String),

    // Coordination layer errors
    #[error("Topic not initialized")]
    TopicNotInitialized,

    #[error("CAS max retries exceeded ({retries} attempts)")]
    MaxRetriesExceeded { retries: u32 },

    // Log layer errors
    #[error("Log is fenced")]
    LogFenced,

    #[error("Gap in compaction range at offset {offset}")]
    GapInRange { offset: u64 },

    #[error("Range contains non-WAL entry at offset {offset}")]
    RangeNotAllWal { offset: u64 },

    #[error("Compaction range mismatch")]
    CompactionRangeMismatch,

    #[error("Compacted data missing")]
    CompactedDataMissing,

    // Queue layer errors
    #[error("Empty batch")]
    EmptyBatch,

    #[error("Batch too large: {reason}")]
    BatchTooLarge { reason: String },

    #[error("Offset not found")]
    OffsetNotFound,

    // Serialization
    #[error("Serialization error: {0}")]
    SerializationError(String),
}

/// Result of an Acknowledge operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AckResult {
    /// Cursor was successfully advanced.
    AckAdvanced,
    /// The acknowledged offset was already behind the cursor.
    AckAlreadyProcessed,
    /// The acknowledged offset does not match the current cursor position.
    AckOffsetMismatch { expected: u64, actual: u64 },
}

pub type Result<T> = std::result::Result<T, S3QueueError>;

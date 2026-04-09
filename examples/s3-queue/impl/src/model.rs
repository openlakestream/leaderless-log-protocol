use serde::{Deserialize, Serialize};

/// Type of an index entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EntryType {
    Wal,
    Compacted,
}

/// Metadata stored at each occupied log index position.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexEntry {
    #[serde(rename = "type")]
    pub entry_type: EntryType,
    pub msg_count: u64,
    pub data_key: String,
}

/// A pending index entry recorded atomically with the sequence counter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingEntry {
    pub offset: u64,
    #[serde(rename = "type")]
    pub entry_type: EntryType,
    pub msg_count: u64,
    pub data_key: String,
}

impl PendingEntry {
    pub fn to_index_entry(&self) -> IndexEntry {
        IndexEntry {
            entry_type: self.entry_type.clone(),
            msg_count: self.msg_count,
            data_key: self.data_key.clone(),
        }
    }
}

/// The next offset to assign, with optional pending index entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetaSequenceCounter {
    pub value: u64,
    pub pending: Option<PendingEntry>,
}

/// Log operational state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum LogState {
    Open,
    Fenced,
}

/// Log state with ABA prevention via version counter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetaLogState {
    pub state: LogState,
    pub version: u64,
}

/// Compaction progress tracker.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetaCompactionCursor {
    pub value: u64,
}

/// Per-consumer read position.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConsumerCursor {
    pub offset: u64,
}

/// A single queue message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    pub payload: String,
}

/// Topic status information.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TopicStatus {
    pub sequence_counter: MetaSequenceCounter,
    pub log_state: MetaLogState,
    pub compaction_cursor: MetaCompactionCursor,
}

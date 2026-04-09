/// Zero-pad an offset to exactly 20 decimal digits.
pub fn pad20(offset: u64) -> String {
    format!("{:020}", offset)
}

/// Parse an offset from an index key like `{topic}/index/00000000000000000042`.
pub fn parse_offset_from_key(key: &str) -> Option<u64> {
    let filename = key.rsplit('/').next()?;
    filename.parse::<u64>().ok()
}

// --- Key builders ---

pub fn sequence_counter_key(topic: &str) -> String {
    format!("{}/meta/sequence-counter", topic)
}

pub fn log_state_key(topic: &str) -> String {
    format!("{}/meta/log-state", topic)
}

pub fn compaction_cursor_key(topic: &str) -> String {
    format!("{}/meta/compaction-cursor", topic)
}

pub fn index_key(topic: &str, offset: u64) -> String {
    format!("{}/index/{}", topic, pad20(offset))
}

pub fn index_prefix(topic: &str) -> String {
    format!("{}/index/", topic)
}

pub fn wal_data_key(topic: &str, uuid: &str) -> String {
    format!("{}/data/wal/{}", topic, uuid)
}

pub fn wal_data_prefix(topic: &str) -> String {
    format!("{}/data/wal/", topic)
}

pub fn compacted_data_key(topic: &str, start: u64, end: u64) -> String {
    format!("{}/data/compacted/{}-{}", topic, pad20(start), pad20(end))
}

pub fn consumer_cursor_key(topic: &str, consumer_id: &str) -> String {
    format!("{}/consumers/{}/cursor", topic, consumer_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pad20() {
        assert_eq!(pad20(0), "00000000000000000000");
        assert_eq!(pad20(42), "00000000000000000042");
        assert_eq!(pad20(u64::MAX), "18446744073709551615");
    }

    #[test]
    fn test_parse_offset_from_key() {
        assert_eq!(
            parse_offset_from_key("my-topic/index/00000000000000000042"),
            Some(42)
        );
        assert_eq!(
            parse_offset_from_key("my-topic/index/00000000000000000001"),
            Some(1)
        );
        assert_eq!(parse_offset_from_key("bad-key"), None);
    }

    #[test]
    fn test_key_builders() {
        assert_eq!(sequence_counter_key("t"), "t/meta/sequence-counter");
        assert_eq!(log_state_key("t"), "t/meta/log-state");
        assert_eq!(compaction_cursor_key("t"), "t/meta/compaction-cursor");
        assert_eq!(
            index_key("t", 5),
            "t/index/00000000000000000005"
        );
        assert_eq!(
            wal_data_key("t", "abc-123"),
            "t/data/wal/abc-123"
        );
        assert_eq!(
            compacted_data_key("t", 1, 5),
            "t/data/compacted/00000000000000000001-00000000000000000005"
        );
        assert_eq!(
            consumer_cursor_key("t", "reader1"),
            "t/consumers/reader1/cursor"
        );
    }
}

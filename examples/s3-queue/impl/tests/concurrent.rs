#![cfg(feature = "integration")]

mod common;

use std::collections::HashSet;
use s3_queue::coordination::ceiling_get::ceiling_get;
use s3_queue::error::AckResult;
use s3_queue::keys;
use s3_queue::model::{ConsumerCursor, EntryType, LogState, Message};
use s3_queue::queue::{compaction, consumer, fence, init, producer, status};
use s3_queue::s3::get_json;

use common::{create_test_client, setup_topic, unique_topic};

// ---------------------------------------------------------------------------
// Test 1: S1 (NoOffsetDuplicates) + S2 (MonotonicOffsets)
// §17.3 — "Two concurrent producers each get distinct offsets"
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_concurrent_producers_distinct_offsets() {
    let (client, topic, config) = setup_topic().await;
    let num_tasks = 10u64;
    let msgs_per_task = 5u64;

    let mut handles = Vec::new();
    for task_id in 0..num_tasks {
        let c = client.clone();
        let t = topic.clone();
        let cfg = config.clone();
        handles.push(tokio::spawn(async move {
            let mut offsets = Vec::new();
            for i in 0..msgs_per_task {
                let msg = Message {
                    payload: format!("task{}-msg{}", task_id, i),
                };
                let offset = producer::produce(c.as_ref(), &t, &msg, &cfg)
                    .await
                    .expect("produce failed");
                offsets.push(offset);
            }
            offsets
        }));
    }

    let mut all_offsets = Vec::new();
    for handle in handles {
        let offsets = handle.await.unwrap();
        all_offsets.extend(offsets);
    }

    let total = (num_tasks * msgs_per_task) as usize;
    assert_eq!(all_offsets.len(), total, "should have {} offsets", total);

    // S1: No duplicates
    let unique: HashSet<u64> = all_offsets.iter().copied().collect();
    assert_eq!(
        unique.len(),
        total,
        "all offsets must be unique (got {} unique out of {})",
        unique.len(),
        total
    );

    // S2: All offsets in valid range [1, total]
    for &offset in &all_offsets {
        assert!(
            offset >= 1 && offset <= total as u64,
            "offset {} out of range [1, {}]",
            offset,
            total
        );
    }

    // All messages readable
    for offset in 1..=total as u64 {
        let result = ceiling_get(client.as_ref(), &topic, offset, &config)
            .await
            .expect("ceiling_get failed");
        assert!(result.is_some(), "offset {} should be readable", offset);
    }
}

// ---------------------------------------------------------------------------
// Test 2: S2 (MonotonicOffsets) — counter value correct after concurrency
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_concurrent_producers_monotonic_counter() {
    let (client, topic, config) = setup_topic().await;
    let num_tasks = 5u64;
    let msgs_per_task = 10u64;

    let mut handles = Vec::new();
    for task_id in 0..num_tasks {
        let c = client.clone();
        let t = topic.clone();
        let cfg = config.clone();
        handles.push(tokio::spawn(async move {
            for i in 0..msgs_per_task {
                let msg = Message {
                    payload: format!("t{}-m{}", task_id, i),
                };
                producer::produce(c.as_ref(), &t, &msg, &cfg)
                    .await
                    .expect("produce failed");
            }
        }));
    }

    for handle in handles {
        handle.await.unwrap();
    }

    let total = num_tasks * msgs_per_task;
    let s = status::get_status(client.as_ref(), &topic)
        .await
        .expect("status failed");

    assert_eq!(
        s.sequence_counter.value,
        total + 1,
        "counter should be {} (started at 1, {} increments)",
        total + 1,
        total
    );
}

// ---------------------------------------------------------------------------
// Test 3: Concurrent first Consume creates exactly one cursor (§17.4)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_concurrent_first_consume_creates_one_cursor() {
    let (client, topic, config) = setup_topic().await;

    // Produce one message so consume has something to return
    let msg = Message {
        payload: "first".into(),
    };
    producer::produce(client.as_ref(), &topic, &msg, &config)
        .await
        .unwrap();

    let consumer_id = "shared-consumer";
    let mut handles = Vec::new();

    for _ in 0..10 {
        let c = client.clone();
        let t = topic.clone();
        let cid = consumer_id.to_string();
        let cfg = config.clone();
        handles.push(tokio::spawn(async move {
            consumer::consume(c.as_ref(), &t, &cid, &cfg).await
        }));
    }

    for handle in handles {
        let result = handle.await.unwrap();
        let result = result.expect("consume should not error");
        // All should return the same message at offset 1
        let r = result.expect("should have a message");
        assert_eq!(r.offset, 1);
        assert_eq!(r.message.payload, "first");
    }

    // Cursor should be at offset 1 (not corrupted)
    let cursor_key = keys::consumer_cursor_key(&topic, consumer_id);
    let (cursor, _) = get_json::<ConsumerCursor>(client.as_ref(), &cursor_key)
        .await
        .unwrap()
        .expect("cursor should exist");
    assert_eq!(cursor.offset, 1, "cursor should be at offset 1");
}

// ---------------------------------------------------------------------------
// Test 4: Concurrent CAS — all producers succeed (§17.1, §17.2)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_concurrent_cas_exactly_one_wins() {
    let (client, topic, config) = setup_topic().await;
    let num_tasks = 10;

    let mut handles = Vec::new();
    for i in 0..num_tasks {
        let c = client.clone();
        let t = topic.clone();
        let cfg = config.clone();
        handles.push(tokio::spawn(async move {
            let msg = Message {
                payload: format!("cas-{}", i),
            };
            producer::produce(c.as_ref(), &t, &msg, &cfg).await
        }));
    }

    let mut offsets = Vec::new();
    for handle in handles {
        let offset = handle.await.unwrap().expect("produce should succeed");
        offsets.push(offset);
    }

    let unique: HashSet<u64> = offsets.iter().copied().collect();
    assert_eq!(unique.len(), num_tasks, "all offsets must be unique");

    let s = status::get_status(client.as_ref(), &topic)
        .await
        .unwrap();
    assert_eq!(s.sequence_counter.value, num_tasks as u64 + 1);
}

// ---------------------------------------------------------------------------
// Test 5: Concurrent InitializeTopic is safe (§17.6)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_concurrent_init_topic_is_safe() {
    let client = create_test_client().await;
    let topic = unique_topic();

    let mut handles = Vec::new();
    for _ in 0..10 {
        let c = client.clone();
        let t = topic.clone();
        handles.push(tokio::spawn(async move {
            init::initialize_topic(c.as_ref(), &t).await
        }));
    }

    for handle in handles {
        handle.await.unwrap().expect("init should succeed");
    }

    let s = status::get_status(client.as_ref(), &topic)
        .await
        .unwrap();
    assert_eq!(s.sequence_counter.value, 1);
    assert_eq!(s.log_state.state, LogState::Open);
    assert_eq!(s.compaction_cursor.value, 1);
}

// ---------------------------------------------------------------------------
// Test 6: S6 (CursorConsistency) — compaction cursor max() guard
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_compaction_cursor_max_guard() {
    let (client, topic, config) = setup_topic().await;

    // Produce 10 messages
    for i in 1..=10 {
        let msg = Message {
            payload: format!("msg-{}", i),
        };
        producer::produce(client.as_ref(), &topic, &msg, &config)
            .await
            .unwrap();
    }

    // Compact [1,3] first (sequential, sets cursor to 4)
    compaction::compact(client.as_ref(), &topic, 1, 3, &config)
        .await
        .unwrap();

    // Then concurrently compact [4,6] and [7,9]
    let c1 = client.clone();
    let t1 = topic.clone();
    let cfg1 = config.clone();
    let h1 = tokio::spawn(async move {
        compaction::compact(c1.as_ref(), &t1, 4, 6, &cfg1).await
    });

    let c2 = client.clone();
    let t2 = topic.clone();
    let cfg2 = config.clone();
    let h2 = tokio::spawn(async move {
        compaction::compact(c2.as_ref(), &t2, 7, 9, &cfg2).await
    });

    h1.await.unwrap().unwrap();
    h2.await.unwrap().unwrap();

    let s = status::get_status(client.as_ref(), &topic)
        .await
        .unwrap();
    assert!(
        s.compaction_cursor.value >= 10,
        "cursor should be >= 10 (got {})",
        s.compaction_cursor.value
    );

    // S4: All messages still readable
    let consumer_id = "verify-reader";
    for expected_offset in 1..=10u64 {
        let result = consumer::consume(client.as_ref(), &topic, consumer_id, &config)
            .await
            .unwrap();
        let r = result.expect("message should be readable");
        assert_eq!(r.offset, expected_offset);
        consumer::acknowledge(client.as_ref(), &topic, consumer_id, expected_offset, &config)
            .await
            .unwrap();
    }
}

// ---------------------------------------------------------------------------
// Test 7: Q2 (AtLeastOnceDelivery)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_at_least_once_delivery() {
    let (client, topic, config) = setup_topic().await;

    for i in 1..=3 {
        let msg = Message {
            payload: format!("msg-{}", i),
        };
        producer::produce(client.as_ref(), &topic, &msg, &config)
            .await
            .unwrap();
    }

    let consumer_id = "alo-consumer";

    // Consume without ack — simulates crash
    let r1 = consumer::consume(client.as_ref(), &topic, consumer_id, &config)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(r1.offset, 1);
    assert_eq!(r1.message.payload, "msg-1");

    // Consume again without ack — should re-deliver same message
    let r2 = consumer::consume(client.as_ref(), &topic, consumer_id, &config)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(r2.offset, 1, "should re-deliver offset 1");
    assert_eq!(r2.message.payload, "msg-1");

    // Now acknowledge
    let ack = consumer::acknowledge(client.as_ref(), &topic, consumer_id, 1, &config)
        .await
        .unwrap();
    assert_eq!(ack, AckResult::AckAdvanced);

    // Next consume should return offset 2
    let r3 = consumer::consume(client.as_ref(), &topic, consumer_id, &config)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(r3.offset, 2);
    assert_eq!(r3.message.payload, "msg-2");
}

// ---------------------------------------------------------------------------
// Test 8: Q1 (ConsumerProgressMonotonicity)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_consumer_progress_monotonicity() {
    let (client, topic, config) = setup_topic().await;

    for i in 1..=5 {
        let msg = Message {
            payload: format!("msg-{}", i),
        };
        producer::produce(client.as_ref(), &topic, &msg, &config)
            .await
            .unwrap();
    }

    let consumer_id = "mono-consumer";
    let cursor_key = keys::consumer_cursor_key(&topic, consumer_id);
    let mut prev_cursor = 0u64;

    for expected_offset in 1..=5u64 {
        let result = consumer::consume(client.as_ref(), &topic, consumer_id, &config)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(result.offset, expected_offset);

        let ack = consumer::acknowledge(
            client.as_ref(),
            &topic,
            consumer_id,
            expected_offset,
            &config,
        )
        .await
        .unwrap();
        assert_eq!(ack, AckResult::AckAdvanced);

        let (cursor, _) = get_json::<ConsumerCursor>(client.as_ref(), &cursor_key)
            .await
            .unwrap()
            .unwrap();
        assert!(
            cursor.offset > prev_cursor,
            "cursor must be strictly increasing: {} > {}",
            cursor.offset,
            prev_cursor
        );
        prev_cursor = cursor.offset;
    }

    assert_eq!(prev_cursor, 6, "final cursor should be 6");
}

// ---------------------------------------------------------------------------
// Test 9: S4 (CompactionPreservesData)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_compaction_preserves_data() {
    let (client, topic, config) = setup_topic().await;

    let payloads: Vec<String> = (1..=6).map(|i| format!("payload-{}", i)).collect();
    for p in &payloads {
        let msg = Message { payload: p.clone() };
        producer::produce(client.as_ref(), &topic, &msg, &config)
            .await
            .unwrap();
    }

    // Compact first 3
    compaction::compact(client.as_ref(), &topic, 1, 3, &config)
        .await
        .unwrap();

    // Read all 6 via a fresh consumer
    let consumer_id = "compact-reader";
    for (i, expected_payload) in payloads.iter().enumerate() {
        let result = consumer::consume(client.as_ref(), &topic, consumer_id, &config)
            .await
            .unwrap();
        let r = result.unwrap_or_else(|| {
            panic!("message at offset {} should be readable", i + 1);
        });
        assert_eq!(r.offset, (i + 1) as u64);
        assert_eq!(&r.message.payload, expected_payload);

        consumer::acknowledge(
            client.as_ref(),
            &topic,
            consumer_id,
            (i + 1) as u64,
            &config,
        )
        .await
        .unwrap();
    }
}

// ---------------------------------------------------------------------------
// Test 10: S8 + L3 — read during compaction
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_read_during_compaction() {
    let (client, topic, config) = setup_topic().await;

    // Produce 10 messages
    for i in 1..=10 {
        let msg = Message {
            payload: format!("msg-{}", i),
        };
        producer::produce(client.as_ref(), &topic, &msg, &config)
            .await
            .unwrap();
    }

    // Spawn a reader that reads all 10 messages
    let rc = client.clone();
    let rt = topic.clone();
    let rcfg = config.clone();
    let reader = tokio::spawn(async move {
        let consumer_id = "race-reader";
        let mut read_count = 0u64;
        for expected in 1..=10u64 {
            let result = consumer::consume(rc.as_ref(), &rt, consumer_id, &rcfg)
                .await
                .expect("consume should not error");
            let r = result.expect("message should be readable");
            assert_eq!(r.offset, expected);
            consumer::acknowledge(rc.as_ref(), &rt, consumer_id, expected, &rcfg)
                .await
                .unwrap();
            read_count += 1;
        }
        read_count
    });

    // Concurrently compact [1,5]
    let cc = client.clone();
    let ct = topic.clone();
    let ccfg = config.clone();
    let compactor = tokio::spawn(async move {
        compaction::compact(cc.as_ref(), &ct, 1, 5, &ccfg).await
    });

    let read_count = reader.await.unwrap();
    assert_eq!(read_count, 10, "reader should have read all 10 messages");

    // Compaction may or may not have completed — that's fine
    let _ = compactor.await.unwrap();
}

// ---------------------------------------------------------------------------
// Test 11: Fence → Unfence → Fence with ABA prevention (§17.6)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_fence_unfence_aba_prevention() {
    let (client, topic, config) = setup_topic().await;

    // Fence (version 0 → 1)
    fence::fence_log(client.as_ref(), &topic, &config)
        .await
        .unwrap();
    let s = status::get_status(client.as_ref(), &topic).await.unwrap();
    assert_eq!(s.log_state.state, LogState::Fenced);
    assert_eq!(s.log_state.version, 1);

    // Unfence (version 1 → 2)
    fence::unfence_log(client.as_ref(), &topic, &config)
        .await
        .unwrap();
    let s = status::get_status(client.as_ref(), &topic).await.unwrap();
    assert_eq!(s.log_state.state, LogState::Open);
    assert_eq!(s.log_state.version, 2);

    // Fence again (version 2 → 3) — ABA: same content as version 1, but version prevents stale CAS
    fence::fence_log(client.as_ref(), &topic, &config)
        .await
        .unwrap();
    let s = status::get_status(client.as_ref(), &topic).await.unwrap();
    assert_eq!(s.log_state.state, LogState::Fenced);
    assert_eq!(s.log_state.version, 3);
}

// ---------------------------------------------------------------------------
// Test 12: S5 (NoPhantomEntries) — RangeDelete precision
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_no_phantom_entries() {
    let (client, topic, config) = setup_topic().await;

    // Produce 10 messages
    for i in 1..=10 {
        let msg = Message {
            payload: format!("msg-{}", i),
        };
        producer::produce(client.as_ref(), &topic, &msg, &config)
            .await
            .unwrap();
    }

    // Compact [3,5]
    compaction::compact(client.as_ref(), &topic, 3, 5, &config)
        .await
        .unwrap();

    // Check: entries 1, 2 still WAL
    for offset in [1, 2] {
        let result = ceiling_get(client.as_ref(), &topic, offset, &config)
            .await
            .unwrap();
        let (entry_offset, entry) = result.unwrap_or_else(|| {
            panic!("offset {} should still have an entry", offset);
        });
        assert_eq!(entry_offset, offset);
        assert_eq!(entry.entry_type, EntryType::Wal);
    }

    // Check: entry at 5 is COMPACTED
    let result = ceiling_get(client.as_ref(), &topic, 5, &config)
        .await
        .unwrap();
    let (entry_offset, entry) = result.expect("offset 5 should have an entry");
    assert_eq!(entry_offset, 5);
    assert_eq!(entry.entry_type, EntryType::Compacted);
    assert_eq!(entry.msg_count, 3); // covers [3,4,5]

    // Check: entries 3, 4 are gone (they were inside compaction range)
    for offset in [3, 4] {
        let result = ceiling_get(client.as_ref(), &topic, offset, &config)
            .await
            .unwrap();
        // CeilingGet should return the COMPACTED entry at 5 (covering offset 3-5)
        let (entry_offset, entry) = result.unwrap();
        assert_eq!(entry_offset, 5, "offset {} should be covered by compacted entry at 5", offset);
        assert_eq!(entry.entry_type, EntryType::Compacted);
    }

    // Check: entries 6-10 still WAL
    for offset in 6..=10 {
        let result = ceiling_get(client.as_ref(), &topic, offset, &config)
            .await
            .unwrap();
        let (entry_offset, entry) = result.unwrap_or_else(|| {
            panic!("offset {} should still have an entry", offset);
        });
        assert_eq!(entry_offset, offset);
        assert_eq!(entry.entry_type, EntryType::Wal);
    }
}

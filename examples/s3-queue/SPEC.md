# S3-Queue Service Specification

**Status:** Draft v1 (language-agnostic)
**Foundation:** [Leaderless Log Protocol](../../1-leaderless-log-protocol.md) (Layer 1) atop [Coordination-Delegated Pattern](../../0-coordination-delegated-pattern.md) (Layer 0)

---

## 1. Problem Statement

S3-Queue implements a distributed message queue built entirely on S3-compatible object storage. No external coordination store (etcd, ZooKeeper, Redis) is required — S3's conditional writes provide all coordination primitives. The queue is a thin application layer (Layer 2) on top of the Leaderless Log Protocol (Layer 1), which delegates coordination to an abstract linearizable store (Layer 0). In this spec, S3 itself serves as that store.

The spec is detailed enough for a coding agent to generate a correct, complete implementation in any language without additional context.

## 2. Goals and Non-Goals

### Goals

- Operate on a single S3-compatible store with zero additional infrastructure
- Provide at-least-once message delivery with explicit consumer acknowledgment
- Support concurrent producers appending without leader election
- Support background compaction that reorganizes WAL entries without data loss
- Support administrative fencing to halt writes
- Serve as a teaching example of the coordination-delegated pattern on object storage

### Non-Goals

- High throughput (>100 messages/sec) — use a dedicated coordination store for that
- Exactly-once delivery — consumers must be idempotent
- Consumer groups with shared cursor coordination
- Automated compaction triggers (time-based or size-based)
- Message TTL, expiration, or dead-letter queues

## 3. System Overview

**Main Components:**

1. **Coordination Layer** — implements Layer 0 primitives (AtomicIncrement, CeilingGet, CompareAndSwap, RangeDelete) using S3 conditional writes
2. **Log Layer** — implements Leaderless Log Protocol actions 1–12 on S3, mapping each protocol state variable to an S3 object
3. **Queue Layer** — thin wrapper adding Produce, Consume, and Acknowledge operations with consumer cursor management
4. **Compaction** — background process that merges WAL entries into compacted entries via the protocol's 3-step index update
5. **Fencing** — administrative control to halt all writes to a topic
6. **VACUUM** — periodic cleanup of orphan data objects left by crashed producers

```
┌──────────────────────────────────────────────────┐
│              Queue Layer (§10–§11)                │
│     Produce · ProduceBatch · Consume · Ack       │
├──────────────────────────────────────────────────┤
│          Log Layer (§7 state machines)            │
│     Actions 1–12 from Leaderless Log Protocol    │
├──────────────────────────────────────────────────┤
│       Coordination Layer (§8 primitives)         │
│  AtomicIncrement · CeilingGet · CAS · RangeDel   │
├──────────────────────────────────────────────────┤
│        S3-Compatible Object Storage (§9)         │
│  PutConditional · Get · ListObjects · Delete      │
└──────────────────────────────────────────────────┘
```

## 4. Core Domain Model

### 4.1 Entities

**IndexEntry** — metadata stored at each occupied log index position:
- `type`: one of `WAL`, `COMPACTED`
- `msgCount`: positive integer — number of messages covered by this entry (1 for single-message entries, N for batch entries)
- `dataKey`: string — full S3 key of the data object containing the message payload(s)

**MetaSequenceCounter** — the next offset to assign, with optional pending index entry:
- `value`: positive integer — starts at 1, monotonically increasing
- `pending`: optional IndexEntry with offset — if present, an index entry that was committed atomically with the counter increment but not yet materialized to its own S3 key. Contains `offset`, `type`, `msgCount`, `dataKey`. Set to null after materialization. This field ensures that the counter increment and index write are logically atomic (matching the protocol spec's single AssignOffset action), following the Delta Lake DynamoDB pattern where the coordination store is the source of truth.

**MetaLogState** — log operational state with ABA prevention:
- `state`: one of `OPEN`, `FENCED`
- `version`: non-negative integer — incremented on every state change, ensures every write produces a unique ETag even when the state cycles back to a previous value

**MetaCompactionCursor** — tracks compaction progress:
- `value`: positive integer — the first offset not yet compacted, starts at 1

**ConsumerCursor** — per-consumer read position:
- `offset`: positive integer — the next offset to consume, starts at 1

**Message** — a single queue message:
- `payload`: opaque byte sequence — the message content, treated as a black box by the queue

**BatchData** — an ordered collection of messages:
- `messages`: ordered list of Message — preserves insertion order within a batch

### 4.2 Stable Identifiers and Normalization

- **Topic name**: alphanumeric string with hyphens, used as S3 key prefix. Example: `orders`, `events-v2`
- **Offset**: non-negative integer, zero-padded to exactly 20 decimal digits for S3 key construction. `pad20(42)` produces `00000000000000000042`. 20 digits accommodates values up to 10^20, exceeding the maximum of a 64-bit unsigned integer (~1.8×10^19).
- **UUID**: version 4 UUID, used as data object keys. Generated fresh for each WALWrite operation. Format: `xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx`.
- **Consumer ID**: non-empty string chosen by the consumer, restricted to characters safe for S3 keys: `[A-Za-z0-9._-]`. Characters outside this set must be replaced with `_` before use in key construction. Each distinct consumer ID tracks an independent cursor.

## 5. S3 Key Schema and Serialization

### 5.1 Key Layout

All S3 keys are prefixed with `{topic}/`:

```
{topic}/
├── meta/
│   ├── sequence-counter        — MetaSequenceCounter
│   ├── log-state               — MetaLogState
│   └── compaction-cursor       — MetaCompactionCursor
├── index/
│   └── {pad20(offset)}         — IndexEntry (one per occupied offset)
├── data/
│   ├── wal/{uuid}              — Message or BatchData (per write)
│   └── compacted/{pad20(start)}-{pad20(end)} — BatchData (per compaction)
└── consumers/
    └── {consumerId}/cursor     — ConsumerCursor
```

### 5.2 Key Patterns

| Logical Name | S3 Key | Content |
|---|---|---|
| Sequence Counter | `{topic}/meta/sequence-counter` | MetaSequenceCounter |
| Log State | `{topic}/meta/log-state` | MetaLogState |
| Compaction Cursor | `{topic}/meta/compaction-cursor` | MetaCompactionCursor |
| Index Entry | `{topic}/index/{pad20(offset)}` | IndexEntry |
| WAL Data | `{topic}/data/wal/{uuid}` | Message (single) or BatchData (batch) |
| Compacted Data | `{topic}/data/compacted/{pad20(start)}-{pad20(end)}` | BatchData |
| Consumer Cursor | `{topic}/consumers/{consumerId}/cursor` | ConsumerCursor |

### 5.3 Serialization Rules

- **Metadata and index entries** (all `meta/*`, all `index/*`, all `consumers/*/cursor`): JSON. Field names use camelCase.
- **Single-message data** (`msgCount` = 1): the WAL data object contains a single Message serialized as JSON.
- **Batch data** (`msgCount` > 1): the WAL or compacted data object contains messages serialized as newline-delimited JSON (NDJSON) — one Message JSON per line, preserving insertion order.
- **Distinguishing single vs batch**: readers check `msgCount` in the covering IndexEntry. If 1, parse as single JSON. If >1, parse as NDJSON.

### 5.4 S3 Abstract Interface

Implementations must provide these abstract operations, mapped to the S3 REST API:

```
Get(key) → (value, etag) | NOT_FOUND
PutConditional(key, value, IfMatch: etag) → OK | PRECONDITION_FAILED
PutConditional(key, value, IfNoneMatch: *) → OK | PRECONDITION_FAILED
Put(key, value) → OK
Delete(key) → OK
BatchDelete(keys[]) → OK                  — up to 1000 keys per call
ListObjects(prefix, startAfter, maxKeys) → ObjectList
```

All operations are synchronous from the caller's perspective. S3 must provide strong read-after-write consistency for Put, Delete, and ListObjects.

## 6. Configuration Specification

### 6.1 Required Configuration

- `s3_endpoint`: S3-compatible endpoint URL
- `s3_bucket`: bucket name
- `s3_credentials`: access key + secret key (or IAM role)

### 6.2 Optional Configuration with Defaults

| Setting | Default | Description |
|---|---|---|
| `max_batch_size` | 1000 | Maximum messages per batch |
| `max_batch_bytes` | 10485760 (10 MB) | Maximum total payload bytes per batch |
| `max_cas_retries` | 10 | Maximum CAS loop attempts before error |
| `cas_backoff_base_ms` | 50 | Exponential backoff base for CAS retries |
| `cas_backoff_max_ms` | 5000 | Maximum backoff delay |
| `ceiling_get_max_retries` | 3 | Maximum retries for GET-after-LIST 404 |
| `vacuum_retention_period_s` | 3600 (1 hour) | Minimum age before orphan data eligible for VACUUM |

### 6.3 Topic Convention

Topics are logical partitions within a single S3 bucket. Each topic's objects share a common key prefix (`{topic}/`). Topics are created by calling InitializeTopic. Multiple topics in the same bucket are fully independent.

## 7. Protocol State Machines

### 7.1 Writer States

States: `IDLE`, `WRITING_WAL`, `ASSIGNING_OFFSET`, `DONE`, `FAILED`

Transitions:

- **IDLE → WRITING_WAL**: StartAppend called with valid batch size. UUID generated.
- **WRITING_WAL → ASSIGNING_OFFSET**: WAL data successfully written to S3 (Put to `data/wal/{uuid}`).
- **WRITING_WAL → FAILED**: S3 Put failed. No data object exists; no cleanup needed.
- **ASSIGNING_OFFSET → DONE**: AtomicIncrement succeeded, index entry written with dataKey pointer.
- **ASSIGNING_OFFSET → FAILED**: Log is fenced (fence check returned FENCED), or AtomicIncrement exceeded max retries, or index entry write failed. If AtomicIncrement succeeded but index write failed, the offset is reserved but unreadable (gap).
- **DONE → IDLE**: AppendComplete returns the assigned offset.
- **FAILED → IDLE**: AppendComplete returns the error.

Important: WRITING_WAL → ASSIGNING_OFFSET ordering is essential. Data is written to a UUID-keyed S3 object *before* the offset is known (Delta Lake pattern). The index entry written during ASSIGNING_OFFSET contains a `dataKey` pointer back to the UUID object.

### 7.2 Compactor States

States: `IDLE`, `WRITING_COMPACTED_INDEX`, `DELETING_OLD`, `UPDATING_CURSOR`, `DONE`

Transitions:

- **IDLE → WRITING_COMPACTED_INDEX**: CompactStart validated that all offsets in `[start, end]` are covered by WAL entries with aligned boundaries.
- **WRITING_COMPACTED_INDEX → DELETING_OLD**: Compacted data object written, index entry at `end` overwritten with type COMPACTED and new dataKey.
- **DELETING_OLD → UPDATING_CURSOR**: Old index entries in `[start, end-1]` deleted via RangeDelete. Old WAL data objects optionally deleted.
- **UPDATING_CURSOR → DONE**: Compaction cursor advanced. Uses `max(currentCursor, end + 1)` to prevent regression from concurrent compactors.
- **DONE → IDLE**: CompactReset.

Crash at any state is recoverable by restarting from the appropriate step (see §12.4).

### 7.3 Log States

States: `OPEN`, `FENCED`

Transitions:

- **OPEN → FENCED**: FenceLog called. CAS on log-state sets state to FENCED and increments version.
- **FENCED → OPEN**: UnfenceLog called. CAS on log-state sets state to OPEN and increments version.
- **OPEN → OPEN** or **FENCED → FENCED**: Idempotent — no-op, no version increment.

ABA prevention: the `version` field increments on every actual state change. Since S3 ETags are content-derived, cycling `OPEN → FENCED → OPEN` would produce identical ETags without the version counter. The version ensures every write produces unique content and therefore a unique ETag.

### 7.4 Consumer Lifecycle

Consumers are stateless processes. Their only persistent state is the cursor object in S3. On first Consume, if no cursor exists, one is created at offset 1 using PutConditional with IfNoneMatch (concurrent-safe). Consumer crash before Acknowledge leaves the cursor unchanged, causing re-delivery on next Consume.

### 7.5 Transition Triggers

- **S3 Put success/failure**: drives writer and compactor state transitions
- **CAS success/failure**: drives AtomicIncrement retries, cursor advances, fence toggles
- **Fence check result**: determines whether AssignOffset proceeds or fails
- **Process crash**: writer returns to IDLE on next invocation; compactor state is inferred from S3 objects on restart
- **Admin action**: triggers FenceLog, UnfenceLog, InitializeTopic, Compact, VACUUM

### 7.6 Idempotency and Recovery Rules

- Every S3 Put is either to a unique key (UUID data, unique offset index) or guarded by CAS (meta objects, cursors)
- RangeDelete and Delete are idempotent — deleting an already-deleted key succeeds
- Compaction's 3-step update is idempotent at each step — safe to re-execute after crash
- FenceLog and UnfenceLog are idempotent — re-fencing an already-fenced log is a no-op
- InitializeTopic is idempotent — PutConditional IfNoneMatch ignores PRECONDITION_FAILED

## 8. Coordination Primitives on S3

These primitives implement the Layer 0 abstract interface from `0-coordination-delegated-pattern.md` using S3 operations.

### 8.1 AtomicIncrementWithPending

Atomically increments the sequence counter AND records a pending index entry in a single CAS. Maps to the `AtomicIncrement` primitive from Layer 0, extended to carry index metadata following the Delta Lake DynamoDB pattern.

This combined operation ensures that offset assignment and index commitment are logically atomic, matching the protocol spec's single `AssignOffset` action where `sequenceCounter := endOff + 1` and `logIndex[endOff] := {type: WAL, msgCount: count}` happen together.

Behavioral rules:
- Read the current counter value and ETag from S3
- If a `pending` field exists in the current value, **materialize it first**: write the pending index entry to its own S3 key, then clear the pending field via CAS (if clear fails, another process handled it — proceed)
- Compute new counter value as current value plus delta
- Write the new counter value AND the new pending index entry using PutConditional with IfMatch on the read ETag
- If PRECONDITION_FAILED (another writer modified the object), retry from the read step
- On first access (key NOT_FOUND), this should not occur — InitializeTopic must be called first. Return TopicNotInitialized error.
- After the CAS succeeds, materialize the pending entry: write the index entry to `{topic}/index/{pad20(offset)}` as a separate Put. If this Put fails, the pending field in the counter ensures the next caller will materialize it.
- Retry at most `max_cas_retries` times with exponential backoff
- Correctness: the counter value is monotonically increasing and carries a unique pending entry, so every written value is unique (unique ETag). No ABA risk. No gaps: even if materialization fails, the pending field persists in the counter.

Postconditions:
- Returned value is strictly greater than the value before the call
- Returned value equals the previous value plus delta
- The pending index entry is durably recorded in the counter object
- The index entry at the assigned offset will be materialized (either by this caller or a subsequent one)

### 8.2 CeilingGet

Finds the smallest index key whose offset is greater than or equal to a target offset. Maps to the `CeilingGet` primitive from Layer 0.

Behavioral rules:
- Construct start-after key as `{topic}/index/{pad20(targetOffset - 1)}`
- Call ListObjects with prefix `{topic}/index/`, start-after as computed, max-keys 1
- If result is empty, return NONE (no entry at or after target offset)
- Parse the offset from the returned key; read the IndexEntry via Get
- Verify the entry covers the target offset: `targetOffset >= entryEndOffset - entry.msgCount + 1`
- If Get on the index entry returns NOT_FOUND (index key deleted between LIST and GET during compaction), retry from the ListObjects step, up to `ceiling_get_max_retries` times
- Before returning, also check the counter's `pending` field — if a pending entry exists for the target offset range, materialize it and return

Postconditions:
- If result is not NONE: returned offset ≥ targetOffset
- If result is not NONE: entry.msgCount ≥ 1
- If result is not NONE: targetOffset falls within the range `[offset - msgCount + 1, offset]`

### 8.3 CompareAndSwap

Reads an S3 object, applies a transformation, and writes the result back conditionally. Generic helper used by FenceLog, UnfenceLog, CompactUpdateCursor, and Acknowledge.

Behavioral rules:
- Read the current value and ETag via Get
- If NOT_FOUND, return error
- Apply the caller-provided transform function to the current value to produce a new value
- If the new value is identical to the current value (no change needed), return the current value without writing
- Write the new value using PutConditional with IfMatch on the read ETag
- If PRECONDITION_FAILED, retry from the read step
- Same retry policy as AtomicIncrement

### 8.4 RangeDelete

Deletes all index entries in a half-open range `[start, end)`. Maps to the `RangeDelete` primitive from Layer 0.

Behavioral rules:
- Paginate through ListObjects with prefix `{topic}/index/`, start-after `pad20(start - 1)`, collecting keys until reaching an offset ≥ end
- Issue BatchDelete calls (up to 1000 keys per call) for all collected keys
- Both ListObjects pagination and BatchDelete are idempotent — safe to re-execute after partial failure

## 9. Data Management and Safety

### 9.1 S3 Consistency Requirements

This spec requires an S3-compatible store that provides:
1. Strong read-after-write consistency for PutConditional, Put, and Delete
2. Strong consistency for ListObjects (LIST reflects all preceding writes and deletes)
3. Content-derived ETags for non-multipart uploads (different content produces different ETags)
4. PutConditional with `If-Match` returning HTTP 412 on ETag mismatch
5. PutConditional with `If-None-Match: *` returning HTTP 412 if the object already exists
6. BatchDelete of up to 1000 keys per call, idempotent

The S3 compatibility test suite (§17.1) validates these requirements against a specific store.

### 9.2 Data Lifecycle

| Phase | Objects Created | Objects Deleted |
|---|---|---|
| Produce | WAL data (`data/wal/{uuid}`), index entry (`index/{pad20(offset)}`) | — |
| Compaction step 1 | Compacted data (`data/compacted/...`), overwritten index entry at end | — |
| Compaction step 2 | — | Old index entries in `[start, end-1]` |
| Compaction step 2 (optional) | — | Old WAL data objects referenced by deleted index entries |
| VACUUM | — | Orphan WAL data objects (unreferenced, older than retention) |

Important: compaction reorganizes data from WAL format into compacted format. The data is never lost — both WAL entries and COMPACTED entries are readable. This is protocol property S4 (CompactionPreservesData).

### 9.3 Orphan Data (VACUUM)

When a producer crashes after WALWrite but before AssignOffset completes, the data object at `data/wal/{uuid}` has no corresponding index entry. These orphan objects waste storage but do not affect correctness.

VACUUM detection:
- List all objects under `{topic}/data/wal/`
- List all index entries, collect the set of all `dataKey` values
- Any WAL data object not in the `dataKey` set AND older than `vacuum_retention_period_s` is an orphan

The retention period prevents deleting data that is in-flight (a producer may have written the data but not yet assigned the offset). The retention period MUST exceed the maximum expected producer stall time — if a producer can stall for up to T seconds between WALWrite and AssignOffset, the retention period must be greater than T. A slow or paused producer that outlives the retention period may have its data deleted before it can assign the offset, resulting in an index entry pointing to a deleted object. This follows the same pattern as Delta Lake's VACUUM command, which has a default retention of 7 days.

### 9.4 Safety Invariants

- Offset keys are never reused: AtomicIncrement guarantees monotonically increasing offsets
- Index entries at non-end offsets are only written during compaction (overwrite with COMPACTED type)
- Compaction cursor never decreases: CompareAndSwap uses `max(current, proposed)` (see §12.2)
- Consumer cursors never decrease: Acknowledge validates that the acknowledged offset matches the current cursor position

## 10. Producer Protocol

### 10.1 Write Path Overview

The produce path follows four sequential steps matching Leaderless Log Protocol actions 1–4:

```
StartAppend → WALWrite → AssignOffset → AppendComplete
```

Each step corresponds to one protocol action. The writer progresses through the state machine defined in §7.1.

### 10.2 WALWrite Contract (Action 2)

Data is written to a UUID-keyed S3 object *before* the offset is known. This is the Delta Lake pattern — data files use content-addressed (UUID) names, decoupled from version numbers. The index entry (written in AssignOffset) will contain a `dataKey` pointer to the UUID data object.

WALWrite rules:
- Generate a fresh UUID
- Serialize the message (single) or messages (batch) to JSON or NDJSON
- Write to `{topic}/data/wal/{uuid}` using unconditional Put (UUID guarantees no key collision)
- On failure: writer transitions to FAILED; no data object exists; no orphan created; no cleanup needed

### 10.3 AssignOffset Contract (Action 3)

After data is written, the producer reserves an offset range and creates an index entry.

AssignOffset rules:
- Read log state; if FENCED, transition to FAILED and return LogFenced error
- Call AtomicIncrement on the sequence counter with delta = batchSize
- Compute: firstOffset = newCounter − batchSize, endOffset = newCounter − 1
- Write index entry at `{topic}/index/{pad20(endOffset)}` with type WAL, msgCount = batchSize, dataKey = the UUID data key from WALWrite

Important nuance — fence check is not atomic with offset assignment:
- The fence check (reading log-state) and offset assignment (AtomicIncrement on sequence-counter) are two separate S3 calls
- A fence can land between them, allowing a write to sneak through after fencing
- This is the same transient violation documented in the Leaderless Log Protocol spec (footnote 1)
- Correctness is not affected: the written data is valid; fencing is administrative, not a safety invariant

Failure modes:
- AtomicIncrement exceeds max retries → error; data blob becomes an orphan (VACUUM handles)
- Index entry Put fails → offset is reserved (counter incremented) but unreadable (gap in log); retry the Put

### 10.4 Batch Produce

When batchSize > 1:
- A single UUID data object contains all N messages serialized as NDJSON
- A single index entry is written at the end offset (endOffset = firstOffset + batchSize − 1) with `msgCount` = batchSize
- This creates a sparse index: intermediate offsets in `[firstOffset, endOffset − 1]` have no index entry
- CeilingGet naturally resolves any offset within the batch to the end-offset index entry
- Compaction operates on whole entries — it cannot split a multi-message entry at an arbitrary boundary

### 10.5 Error Mapping

| Failure Point | Writer State After | Data Object | Index Entry | Recovery |
|---|---|---|---|---|
| S3 Put in WALWrite fails | FAILED | Does not exist | Does not exist | None needed |
| Log is fenced | FAILED | Exists (orphan) | Does not exist | VACUUM |
| AtomicIncrement max retries | FAILED | Exists (orphan) | Does not exist | VACUUM |
| Index entry Put fails | FAILED | Exists | Does not exist (gap) | Retry Put |

## 11. Consumer Protocol

### 11.1 Consume Contract

Consume reads the next message for a consumer without advancing the cursor. This is the first half of at-least-once delivery.

Consume rules:
- Read the consumer's cursor from `{topic}/consumers/{consumerId}/cursor`
- If NOT_FOUND (first consume for this consumer), create cursor at offset 1 using PutConditional with IfNoneMatch; re-read to obtain ETag
- Call ReadEntry (Action 12) at the cursor's offset position
- If ReadEntry returns NONE (no message at or after the cursor): return NONE
- Return the offset and message to the caller
- **The cursor is NOT modified** — the message is "in-flight" until acknowledged

At-least-once guarantee: if the consumer crashes after Consume returns but before Acknowledge is called, the next Consume returns the same message again. The consumer application must be idempotent.

### 11.2 Acknowledge Contract

Acknowledge advances the consumer's cursor, committing the consumption of a message.

Acknowledge rules:
- Read the current cursor value
- If the acknowledged offset + 1 is less than or equal to the current cursor, the message was already acknowledged. Return `AckAlreadyProcessed` (idempotent, no write).
- If the acknowledged offset does not equal the current cursor, the caller is trying to skip messages or ack out of order. Return `AckOffsetMismatch` (no write).
- Otherwise, write new cursor value (offset + 1) using PutConditional with IfMatch on the read ETag. Return `AckAdvanced`.

Return values:
- `AckAdvanced`: cursor was successfully advanced. This is the commit point — the message will not be re-delivered.
- `AckAlreadyProcessed`: the acknowledged offset was already behind the cursor. No action taken. Idempotent re-ack.
- `AckOffsetMismatch`: the acknowledged offset does not match the current cursor position. No action taken. Caller should check what offset the cursor is actually at.

The caller MUST distinguish these return values. `AckAdvanced` is the only result that means the message was committed.

### 11.3 First Consume (New Consumer)

When a consumer ID has no cursor object in S3:
- Create cursor with offset 1 using PutConditional with IfNoneMatch
- If PRECONDITION_FAILED (concurrent consumer with same ID created the cursor first), ignore and re-read
- This is concurrent-safe: exactly one creator succeeds; all others observe the created cursor

### 11.4 Reading from Sparse Index

When the cursor points to an offset in the middle of a multi-message batch entry:
- CeilingGet finds the covering index entry at the end offset
- The entry's `msgCount` determines the range: `[endOffset - msgCount + 1, endOffset]`
- The message position within the batch is: `cursorOffset - (endOffset - msgCount + 1)`
- Read the data object at the entry's `dataKey` and extract the message at the computed position

### 11.5 Races During Read

Two races can occur during the read path when compaction runs concurrently:

**Race 1: Index key deleted between LIST and GET.** ListObjects returns an index key that the compactor deletes before the subsequent Get. Recovery: retry CeilingGet from the ListObjects step. The retry will find the COMPACTED entry that now covers the range.

**Race 2: WAL data object deleted between index read and data fetch.** The reader successfully reads a WAL index entry, but the compactor deletes the WAL data object (optional cleanup in compaction step 2) before the reader can fetch it. Recovery: retry the entire read from CeilingGet. The retry will find the COMPACTED entry whose dataKey points to the compacted data object.

Both races share the same recovery pattern: retry from CeilingGet. Maximum retries: `ceiling_get_max_retries` (default 3). After max retries, return an error indicating a persistent inconsistency.

## 12. Compaction Protocol

### 12.1 Compaction Overview

Compaction merges a contiguous range of WAL index entries `[start, end]` into a single COMPACTED index entry at position `end`. The merged data is written to a new compacted data object. Both WAL and COMPACTED entries are readable — compaction reorganizes data, never deletes it.

### 12.2 Three-Step Update

Compaction proceeds in three sequential steps (matching Leaderless Log Protocol actions 6–8):

**Step 1 — Write compacted data and index (Action 6):**
- Read all WAL data objects in the range via their index entries' `dataKey` pointers
- Merge all messages into a single NDJSON data object
- Write merged data to `{topic}/data/compacted/{pad20(start)}-{pad20(end)}`
- Overwrite the index entry at `{topic}/index/{pad20(end)}` with type COMPACTED, msgCount = (end − start + 1), dataKey = the compacted data key

**Step 2 — Delete old index entries (Action 7):**
- Delete index entries in `[start, end − 1]` using RangeDelete
- Optionally delete old WAL data objects referenced by the deleted entries (storage reclamation)

**Step 3 — Advance compaction cursor (Action 8):**
- Update the compaction cursor using CompareAndSwap
- The transform function applies `max(currentCursor, end + 1)` to prevent cursor regression from concurrent compactors operating on overlapping ranges

The ordering is critical: step 1 must complete before step 2. This guarantees that readers always find a covering entry for any committed offset (the COMPACTED entry exists before old entries are deleted).

### 12.3 Entry Boundary Alignment

Before starting compaction:
- Verify all offsets in `[start, end]` are covered by WAL entries
- Verify all entries are fully contained within the range (no entry straddles the range boundary)
- With non-deterministic batch sizes, some write sequences produce entries that don't align with the compaction range. Compaction rejects misaligned ranges.

### 12.4 Crash Recovery

The 3-step update is idempotent and resumable. A new compactor process detects incomplete state and resumes:

| Crash After | Observable State | Recovery Action |
|---|---|---|
| Step 1 | COMPACTED entry at end + old WAL entries still exist in `[start, end-1]` | Resume from step 2 (delete old entries) |
| Step 2 | COMPACTED entry at end + old entries deleted + cursor not advanced | Resume from step 3 (advance cursor) |
| Step 3 | Fully complete | No action needed |

Detection: read the index entry at `end`. If it has type COMPACTED and the compaction cursor is ≤ end, compaction is incomplete. If the index entry has type WAL, compaction has not started.

No lease, heartbeat, or external failure detector is needed. Recovery is purely state-driven from S3 object inspection.

### 12.5 Concurrent Compactors

**Same range:** Multiple compactors on the same range are safe but wasteful (all operations are idempotent). However, implementations should prevent this via external coordination (e.g., a single compaction trigger).

**Disjoint ranges:** Multiple compactors on non-overlapping ranges are fully independent and safe.

**Overlapping ranges:** UNSAFE. Compactor A on [1,5] and compactor B on [3,7] can interfere: A's RangeDelete can remove index entries that B's COMPACTED entry at offset 7 depends on for coverage. The cursor `max()` guard only prevents cursor regression — it does not protect index coverage. The base protocol only proves safety for sequential disjoint compaction rounds (protocol spec S9: SequentialCompactionSafety). Implementations MUST NOT run compactors on overlapping ranges.

The manual compaction trigger (v1) naturally prevents concurrent compactors. Production systems implementing automated compaction MUST serialize compaction or ensure ranges are always disjoint.

## 13. Fencing and Administrative Operations

### 13.1 InitializeTopic

Creates all required meta objects for a topic. Must be called before any other operation on the topic.

InitializeTopic rules:
- Write MetaSequenceCounter with value 1 using PutConditional with IfNoneMatch
- Write MetaLogState with state OPEN and version 0 using PutConditional with IfNoneMatch
- Write MetaCompactionCursor with value 1 using PutConditional with IfNoneMatch
- Ignore PRECONDITION_FAILED on each write (object already exists from a concurrent or prior initialization)

Concurrent-safe: multiple callers can initialize the same topic simultaneously. Exactly one creator succeeds per meta object; all others observe the created object.

### 13.2 FenceLog

Halts all future writes to a topic.

FenceLog rules:
- Read current MetaLogState
- If already FENCED, return without modification (idempotent)
- Write new state with state = FENCED and version = currentVersion + 1 using CompareAndSwap

After fencing, all AssignOffset calls will read FENCED and return a LogFenced error. In-flight writes (between WALWrite and AssignOffset) may still complete — see §10.3.

### 13.3 UnfenceLog

Resumes writes to a fenced topic. Same rules as FenceLog but with state = OPEN.

### 13.4 ABA Prevention

The `version` field in MetaLogState prevents the ABA problem inherent to content-derived ETags. Without it:
- State cycles `OPEN → FENCED → OPEN` would produce identical content ("OPEN" with no version)
- Identical content produces identical ETags
- A stale CAS from a slow process could succeed against the recycled ETag

With the version counter, every state change produces unique content (`{state: OPEN, version: 2}` differs from `{state: OPEN, version: 0}`), and therefore unique ETags. This ensures CAS correctly detects all intervening modifications.

The sequence counter and compaction cursor do not need this mitigation because their values are monotonically increasing and therefore always unique.

## 14. Failure Model and Recovery Strategy

### 14.1 Failure Classes

1. **Producer failures**
   - WAL write failure (S3 Put error, network timeout)
   - CAS contention on sequence counter (too many concurrent producers)
   - Fencing rejection (log was fenced between fence check and offset assignment)
   - Orphan data (WALWrite succeeded but AssignOffset failed)

2. **Consumer failures**
   - Cursor creation race (concurrent first-consume; handled by IfNoneMatch)
   - Read error during CeilingGet or data fetch (S3 error, network timeout)
   - Acknowledge CAS contention (concurrent consumer with same ID; should not happen in practice)
   - GET-after-LIST 404 (compaction race; handled by retry)

3. **Compactor failures**
   - Crash between any two compaction steps (handled by idempotent resume)
   - Range validation failure (misaligned entry boundaries)
   - Cursor regression attempt (prevented by max() guard)

4. **S3 failures**
   - Temporary unavailability (all CAS loops fail after max retries; no data corruption)
   - Eventual consistency violation (spec assumes strong consistency; violating stores will produce incorrect behavior)
   - ETag semantic differences across S3-compatible stores (content-derived assumption may not hold)

5. **Configuration failures**
   - Missing or inaccessible bucket
   - Invalid topic name
   - Authentication errors

### 14.2 Recovery Behavior

| Failure Class | Detection | Recovery | Impact on Properties |
|---|---|---|---|
| Producer WAL write failure | S3 error response | Caller retries or reports error | None (no state changed) |
| Producer CAS contention | MaxRetriesExceeded | Caller retries or reports error; orphan data object left | VACUUM cleans orphan |
| Producer fencing rejection | LogFenced error | Caller handles; orphan data object left | VACUUM cleans orphan |
| Consumer GET-after-LIST 404 | NOT_FOUND on Get after ListObjects | Automatic retry (up to ceiling_get_max_retries) | None if retry succeeds |
| Compactor crash | State inference from S3 objects | Resume from appropriate step | None (idempotent) |
| S3 temporary unavailability | All operations return errors | Wait and retry; no data corruption | Operations blocked during outage |

### 14.3 Restart Recovery

No persistent state exists outside S3. A fresh process reads S3 state and operates correctly:
- Producers: start new writes; no state to recover
- Consumers: read cursor from S3; resume consuming
- Compactors: inspect S3 for incomplete compaction state and resume

### 14.4 Operator Intervention Points

- **FenceLog**: halt writes to investigate issues or perform maintenance
- **UnfenceLog**: resume writes after investigation
- **Manual compaction**: trigger compaction with specific range
- **VACUUM**: clean up orphan data objects
- **Inspect S3 objects**: all state is human-readable JSON in S3

## 15. Security and Operational Safety

### 15.1 Access Control

- S3 bucket policies control who can read/write queue data
- IAM policies can restrict access to specific topic prefixes (e.g., `orders/*` vs `events/*`)
- Separate read-only and read-write credentials for consumers vs producers

### 15.2 Data Encryption

- S3 Server-Side Encryption (SSE) provides at-rest encryption
- TLS provides in-transit encryption for all S3 API calls
- Client-side encryption of message payloads is the caller's responsibility

### 15.3 Tenant Isolation

Topics provide logical isolation within a single bucket via key prefixes. For stronger isolation, use separate buckets per tenant with distinct IAM credentials.

### 15.4 CAS Contention Amplification

Many concurrent producers on the same topic amplify CAS retry rates on the sequence counter. Under N concurrent producers, the expected retry rate grows with contention. This design targets approximately 10–50 appends per second. At 50 appends/sec, expect roughly 700,000 S3 requests per hour. Production systems requiring higher throughput should use a dedicated coordination store (Apache Oxia, etcd, FoundationDB) instead of S3 conditional writes.

### 15.5 S3 Compatibility Risk

S3 ETag semantics vary across providers. AWS S3 uses MD5-of-content for non-multipart uploads, but this is not guaranteed for all S3-compatible stores (MinIO, Cloudflare R2, Ceph RadosGW). The S3 compatibility test suite (§17.1) validates these assumptions. Run the test suite against any new S3-compatible store before deploying.

## 16. Reference Algorithms (Language-Agnostic)

### 16.1 AtomicIncrementWithPending

```
function AtomicIncrementWithPending(counterKey, delta, pendingEntry):
  for attempt in 1..max_cas_retries:
    (counter, etag) := Get(counterKey)
    if NOT_FOUND:
      return error TopicNotInitialized

    -- If a previous pending entry exists, materialize it first
    if counter.pending is not null:
      MaterializePending(counter.pending)

    new_value := counter.value + delta
    new_counter := {value: new_value, pending: pendingEntry}
    result := PutConditional(counterKey, new_counter, IfMatch: etag)
    if OK:
      -- Best-effort materialization (if this fails, next caller handles it)
      MaterializePending(pendingEntry)
      return new_value
    else:
      wait(backoff(attempt)); continue
  return error MaxRetriesExceeded

function MaterializePending(pending):
  indexKey := "{topic}/index/{pad20(pending.offset)}"
  entry := {type: pending.type, msgCount: pending.msgCount, dataKey: pending.dataKey}
  Put(indexKey, SerializeJSON(entry))
  -- Note: Put is idempotent for same content; safe to re-execute
```

### 16.2 CeilingGet

```
function CeilingGet(topic, targetOffset):
  prefix := "{topic}/index/"
  for retry in 1..ceiling_get_max_retries:
    startAfter := prefix + pad20(targetOffset - 1)
    results := ListObjects(prefix, startAfter, maxKeys=1)
    if results is empty: return NONE
    key := results[0].key
    entryOffset := parseOffsetFromKey(key)
    entry := Get(key)
    if NOT_FOUND: continue                  -- deleted between LIST and GET; retry
    startOffset := entryOffset - entry.msgCount + 1
    if targetOffset < startOffset: return NONE
    return (entryOffset, entry)
  return error MaxRetriesExceeded
```

### 16.3 CompareAndSwap

```
function CompareAndSwap(key, transformFn):
  for attempt in 1..max_cas_retries:
    (value, etag) := Get(key)
    if NOT_FOUND: return error NotFound
    new_value := transformFn(value)
    if new_value = value: return value       -- no change needed
    result := PutConditional(key, new_value, IfMatch: etag)
    if OK: return new_value
    else: wait(backoff(attempt)); continue
  return error MaxRetriesExceeded
```

### 16.4 RangeDelete

```
function RangeDelete(topic, start, end):
  prefix := "{topic}/index/"
  keys := []
  marker := prefix + pad20(start - 1)
  loop:
    page := ListObjects(prefix, marker, maxKeys=1000)
    for obj in page.contents:
      offset := parseOffsetFromKey(obj.key)
      if offset >= end: break to after-loop
      keys.append(obj.key)
    if not page.isTruncated: break
    marker := page.contents[last].key
  for chunk in keys grouped by 1000:
    BatchDelete(chunk)
```

### 16.5 InitializeTopic

```
function InitializeTopic(topic):
  PutConditional("{topic}/meta/sequence-counter", {value: 1}, IfNoneMatch: *)
    -- ignore PRECONDITION_FAILED
  PutConditional("{topic}/meta/log-state", {state: OPEN, version: 0}, IfNoneMatch: *)
    -- ignore PRECONDITION_FAILED
  PutConditional("{topic}/meta/compaction-cursor", {value: 1}, IfNoneMatch: *)
    -- ignore PRECONDITION_FAILED
```

### 16.6 Produce (Single Message)

```
function Produce(topic, message):
  uuid := GenerateUUID()
  dataKey := "{topic}/data/wal/{uuid}"
  Put(dataKey, SerializeJSON(message))

  (logState, _) := Get("{topic}/meta/log-state")
  if logState.state = FENCED:
    return error LogFenced

  counterKey := "{topic}/meta/sequence-counter"
  -- The CAS atomically increments the counter AND records the pending index entry
  newCounter := AtomicIncrementWithPending(counterKey, 1,
    {type: WAL, msgCount: 1, dataKey: dataKey})
  offset := newCounter - 1
  -- Index entry is materialized by AtomicIncrementWithPending (or next caller)
  return offset
```

### 16.7 ProduceBatch

```
function ProduceBatch(topic, messages[]):
  if messages is empty: return error EmptyBatch
  if length(messages) > max_batch_size: return error BatchTooLarge
  totalBytes := sum of serialized size of each message
  if totalBytes > max_batch_bytes: return error BatchTooLarge
  batchSize := length(messages)

  uuid := GenerateUUID()
  dataKey := "{topic}/data/wal/{uuid}"
  Put(dataKey, SerializeNDJSON(messages))

  (logState, _) := Get("{topic}/meta/log-state")
  if logState.state = FENCED:
    return error LogFenced

  counterKey := "{topic}/meta/sequence-counter"
  newCounter := AtomicIncrementWithPending(counterKey, batchSize,
    {type: WAL, msgCount: batchSize, dataKey: dataKey})
  endOffset := newCounter - 1
  return endOffset
```

### 16.8 Consume

```
function Consume(topic, consumerId):
  cursorKey := "{topic}/consumers/{consumerId}/cursor"
  (cursor, etag) := Get(cursorKey)
  if NOT_FOUND:
    PutConditional(cursorKey, {offset: 1}, IfNoneMatch: *)
      -- ignore PRECONDITION_FAILED (concurrent init)
    (cursor, etag) := Get(cursorKey)

  result := CeilingGet(topic, cursor.offset)
  if NONE: return NONE

  (entryOffset, entry) := result
  startOffset := entryOffset - entry.msgCount + 1
  positionInBatch := cursor.offset - startOffset

  if entry.msgCount = 1:
    message := DeserializeJSON(Get(entry.dataKey))
  else:
    allMessages := DeserializeNDJSON(Get(entry.dataKey))
    message := allMessages[positionInBatch]

  return (cursor.offset, message)
```

### 16.9 Acknowledge

```
function Acknowledge(topic, consumerId, offset):
  cursorKey := "{topic}/consumers/{consumerId}/cursor"
  (cursor, etag) := Get(cursorKey)
  if NOT_FOUND: return error TopicNotInitialized

  if offset + 1 <= cursor.offset:
    return AckAlreadyProcessed               -- already past this offset

  if offset != cursor.offset:
    return AckOffsetMismatch                 -- trying to skip or ack wrong offset

  result := PutConditional(cursorKey, {offset: offset + 1}, IfMatch: etag)
  if PRECONDITION_FAILED:
    -- Another process modified the cursor; retry from read
    return Acknowledge(topic, consumerId, offset)  -- bounded by max_cas_retries

  return AckAdvanced                         -- cursor successfully moved forward
```

### 16.10 Compact

```
function Compact(topic, start, end):
  -- Step 0: Validate range
  entries := []
  offset := start
  while offset <= end:
    result := CeilingGet(topic, offset)
    if NONE: return error GapInRange
    (entryOffset, entry) := result
    if entry.type != WAL: return error RangeNotAllWAL
    entryStart := entryOffset - entry.msgCount + 1
    if entryStart > offset: return error GapInRange
    entries.append((entryOffset, entry))
    offset := entryOffset + 1

  -- Step 0b: Read all WAL data
  mergedMessages := []
  for (_, entry) in entries:
    data := Get(entry.dataKey)
    if entry.msgCount = 1:
      mergedMessages.append(DeserializeJSON(data))
    else:
      mergedMessages.extend(DeserializeNDJSON(data))

  -- Step 1: Write compacted data and index
  compactedKey := "{topic}/data/compacted/{pad20(start)}-{pad20(end)}"
  Put(compactedKey, SerializeNDJSON(mergedMessages))
  compactedEntry := {type: COMPACTED, msgCount: end - start + 1, dataKey: compactedKey}
  Put("{topic}/index/{pad20(end)}", SerializeJSON(compactedEntry))

  -- Step 2: Delete old index entries and optionally old WAL data
  RangeDelete(topic, start, end)             -- deletes [start, end)
  for (_, entry) in entries:
    Delete(entry.dataKey)                    -- optional; storage reclamation

  -- Step 3: Advance compaction cursor
  CompareAndSwap("{topic}/meta/compaction-cursor", function(cursor):
    return {value: max(cursor.value, end + 1)}
  )
```

### 16.11 CompactWithRecovery

```
function CompactWithRecovery(topic, start, end):
  indexAtEnd := Get("{topic}/index/{pad20(end)}")
  cursor := Get("{topic}/meta/compaction-cursor")

  if indexAtEnd is NOT_FOUND or indexAtEnd.type = WAL:
    -- Compaction not started; run full compaction
    Compact(topic, start, end)
  else if indexAtEnd.type = COMPACTED and cursor.value <= end:
    -- Verify the COMPACTED entry matches the requested range
    if indexAtEnd.msgCount != (end - start + 1):
      return error CompactionRangeMismatch
    -- Verify the compacted data object exists
    dataExists := Get(indexAtEnd.dataKey)
    if NOT_FOUND:
      return error CompactedDataMissing
    -- Compaction partially complete; resume from step 2
    RangeDelete(topic, start, end)
    CompareAndSwap("{topic}/meta/compaction-cursor", function(c):
      return {value: max(c.value, end + 1)}
    )
  -- else: fully complete; no-op
```

### 16.12 FenceLog and UnfenceLog

```
function FenceLog(topic):
  CompareAndSwap("{topic}/meta/log-state", function(state):
    if state.state = FENCED: return state    -- already fenced; no-op
    return {state: FENCED, version: state.version + 1}
  )

function UnfenceLog(topic):
  CompareAndSwap("{topic}/meta/log-state", function(state):
    if state.state = OPEN: return state      -- already open; no-op
    return {state: OPEN, version: state.version + 1}
  )
```

### 16.13 VACUUM (Orphan Cleanup)

```
function Vacuum(topic, retentionPeriodSeconds):
  -- Collect all WAL data keys
  walDataKeys := ListAllObjects("{topic}/data/wal/")

  -- Collect all referenced data keys from index entries
  referencedKeys := set()
  for indexEntry in ListAllObjects("{topic}/index/"):
    entry := Get(indexEntry.key)
    referencedKeys.add(entry.dataKey)

  -- Delete unreferenced WAL data objects older than retention
  now := CurrentTimestamp()
  for obj in walDataKeys:
    if obj.key not in referencedKeys:
      if (now - obj.lastModified) > retentionPeriodSeconds:
        Delete(obj.key)
```

## 17. Test and Validation Matrix

Conforming implementations should include tests covering the behaviors in this specification.

Validation profiles:
- **Core Conformance**: deterministic tests required for all implementations
- **Extension Conformance**: required only for optional features shipped
- **Real Integration Profile**: environment-dependent smoke/integration checks recommended pre-production

Unless noted, §17.1–§17.7 are Core Conformance.

### 17.1 S3 Compatibility

- PutConditional with IfMatch succeeds when ETag matches, returns PRECONDITION_FAILED when stale
- PutConditional with IfNoneMatch succeeds when object absent, returns PRECONDITION_FAILED when present
- Two concurrent CAS loops on same key: exactly one succeeds per round
- Different content written to same key produces different ETags
- ListObjects with StartAfter returns keys in lexicographic order; zero-padded offsets sort correctly
- ListObjects on sparse index (gaps between keys) returns the correct nearest key
- BatchDelete of 1–1000 keys succeeds; re-deleting already-deleted keys succeeds (idempotent)
- ListObjects reflects immediately preceding Put and Delete operations (strong consistency)

### 17.2 Coordination Primitives

- AtomicIncrementWithPending on uninitialized topic returns TopicNotInitialized error
- AtomicIncrementWithPending from existing value returns old + delta and records pending entry
- AtomicIncrementWithPending materializes a prior pending entry before recording the new one (gap filling)
- AtomicIncrementWithPending with concurrent callers produces unique, monotonically increasing values
- CeilingGet returns NONE for offset beyond all entries
- CeilingGet returns covering entry for mid-batch offset
- CeilingGet retries on GET-after-LIST 404 (simulate by deleting between LIST and GET)
- CompareAndSwap applies transform and writes conditionally
- CompareAndSwap returns current value without writing when transform produces no change
- RangeDelete removes all keys in specified range; keys outside range are untouched

### 17.3 Producer

- Single-message Produce writes UUID-keyed data object and index entry; offset is returned
- Batch Produce writes single UUID data object with NDJSON; sparse index entry at end offset
- Two concurrent producers each get distinct offsets; all messages readable after both complete
- Produce on fenced topic returns LogFenced error; data object becomes orphan
- Empty batch is rejected
- Batch exceeding max message count is rejected
- Batch exceeding max_batch_bytes is rejected

### 17.4 Consumer

- Consume returns message without advancing cursor (call Consume twice → same message)
- Acknowledge returns AckAdvanced and advances cursor to offset + 1
- Acknowledge on already-processed offset returns AckAlreadyProcessed (no write)
- Acknowledge on wrong offset returns AckOffsetMismatch (no write)
- Consume after Acknowledge returns next message (or NONE if at end)
- Simulated crash: Consume → (no Acknowledge) → Consume returns same message (at-least-once)
- First Consume for new consumer creates cursor at offset 1
- Concurrent first Consume for same consumer ID creates exactly one cursor
- Consume from mid-batch offset returns correct message within batch
- Consume retries from CeilingGet when WAL data object returns NOT_FOUND (data-key race with compaction)

### 17.5 Compaction

- Full cycle: Produce multiple messages → Compact range → read compacted entry at end offset
- Compacted data contains all original messages in order
- Read from within compacted range succeeds (CompactionPreservesData)
- Compact on misaligned range is rejected
- Compact on range with gaps is rejected
- CompactWithRecovery detects and resumes incomplete compaction (simulate crash after step 1)
- Compaction cursor uses max() — concurrent compaction on smaller range does not regress cursor
- Old WAL data objects are deleted after compaction (if optional cleanup implemented — Extension Conformance)

### 17.6 Fencing and Administration

- FenceLog on OPEN topic sets state to FENCED
- UnfenceLog on FENCED topic sets state to OPEN
- FenceLog on already-FENCED topic is no-op (idempotent)
- UnfenceLog on already-OPEN topic is no-op (idempotent)
- Fence → Unfence → Fence succeeds (ABA prevention via version counter; all three CAS operations succeed)
- InitializeTopic creates all three meta objects
- Concurrent InitializeTopic calls succeed without conflict

### 17.7 VACUUM and Data Safety (Extension Conformance)

- VACUUM deletes unreferenced WAL data objects older than retention period
- VACUUM does not delete referenced WAL data objects
- VACUUM does not delete objects younger than retention period
- VACUUM does not delete compacted data objects or index entries

Real Integration Profile:
- Run full S3 compatibility suite against target S3-compatible store
- Run producer → consumer → compaction → consumer-reads-compacted end-to-end
- Verify with at least 2 concurrent producers and 2 consumers

## 18. Implementation Checklist (Definition of Done)

### 18.1 Required for Conformance

- S3 abstract interface (§5.4): Get, PutConditional (IfMatch + IfNoneMatch), Put, Delete, BatchDelete, ListObjects
- Coordination primitives (§8): AtomicIncrement, CeilingGet, CompareAndSwap, RangeDelete
- Topic initialization (§13.1): creates all meta objects, concurrent-safe
- Producer (§10): Produce and ProduceBatch with UUID data keys, fence check, AtomicIncrement, index write
- Consumer (§11): Consume without cursor advance, Acknowledge with offset validation
- Compaction (§12): three-step update, crash recovery detection, cursor max() guard
- Fencing (§13): FenceLog and UnfenceLog with version counter ABA prevention
- State machines (§7): writer, compactor, and log state transitions enforced
- Serialization (§5.3): JSON for metadata/index, NDJSON for batch data
- Error types: distinct named error conditions (LogFenced, MaxRetriesExceeded, EmptyBatch, BatchTooLarge, GapInRange, RangeNotAllWAL, OffsetNotFound)
- Configuration (§6): all required and optional settings with documented defaults
- All Core Conformance tests (§17.1–§17.6) passing

### 18.2 Recommended Extensions (Not Required)

- VACUUM (§16.13) for orphan data cleanup
- CompactWithRecovery (§16.11) for automatic crash detection and resume
- CLI interface for all operations (init, produce, consume, ack, compact, fence, unfence, vacuum)
- Structured logging for all S3 operations (operation, key, result, latency, retry count)
- Docker Compose configuration for local MinIO testing
- TODO: Consumer groups with shared cursor coordination
- TODO: Automated compaction triggers (time-based or size-based)
- TODO: Message schemas and payload validation

### 18.3 Operational Validation Before Production

- Run S3 compatibility suite (§17.1) against target S3-compatible store
- Verify ETag uniqueness assumption holds for the specific store
- Run full end-to-end with concurrent producers and consumers
- Measure CAS retry rates under expected load to verify throughput assumptions (§15.4)
- Configure S3 bucket policies for appropriate access control (§15.1)

## Appendix A. Correctness Properties

### A.1 Property Inheritance from Leaderless Log Protocol

This queue inherits correctness properties from the Leaderless Log Protocol. The table below maps each protocol property to its status in the S3-Queue implementation.

| Property | Protocol Spec | S3-Queue Status | Justification |
|---|---|---|---|
| S1: NoOffsetDuplicates | Safety | **Preserved** | AtomicIncrement via CAS loop guarantees each offset assigned exactly once |
| S2: MonotonicOffsets | Safety | **Preserved** | CAS loop only increments; sequence counter never decreases |
| S3: FencedRejectsAppends | Safety | **Weakened** | Fence check and offset assignment are two separate S3 calls (not atomic). A write can sneak through if fencing lands between the Get on log-state and the AtomicIncrement on sequence-counter. Same transient violation as protocol spec footnote 1. |
| S4: CompactionPreservesData | Safety | **Preserved** | COMPACTED entry and data written before old entries deleted (step 1 before step 2) |
| S5: NoPhantomEntries | Safety | **Preserved** | RangeDelete removes exactly the specified range |
| S6: CursorConsistency | Safety | **Preserved** | Compaction cursor advanced via CAS; max() guard prevents regression |
| S7: NoOverlappingRanges | Safety | **Preserved** | Same write-before-delete ordering as protocol spec |
| S8: NoReaderError | Safety | **Weakened** | GET-after-LIST can return NOT_FOUND during compaction (race between ListObjects and concurrent Delete). Mitigated by retry in CeilingGet. After retry, covering entry is always found. |
| S9: SequentialCompactionSafety | Safety | **Preserved** | Sequential compaction ranges are disjoint; CAS with max() guards cursor |
| L1: AppendProgress | Liveness | **Preserved** | CAS retry loop with exponential backoff guarantees eventual progress under fairness |
| L2: CompactionCompletes | Liveness | **Preserved** | Idempotent 3-step design ensures completion even after crashes |
| L3: ReaderEventuallySucceeds | Liveness | **Preserved** | Retry on NOT_FOUND always resolves to a covering entry |

### A.2 Queue-Specific Properties

**Q1: ConsumerProgressMonotonicity**
- Type: Safety (Invariant)
- Statement: For all consumers c, the cursor offset never decreases. Each Acknowledge sets the cursor to `offset + 1`, and the CAS transform validates that the acknowledged offset matches the current cursor position.

**Q2: AtLeastOnceDelivery**
- Type: Safety
- Statement: A message at offset O is delivered to consumer C at least once before C's cursor advances past O. Consume returns the message at the cursor position without advancing the cursor. Only Acknowledge advances the cursor. If the consumer crashes before Acknowledge, the next Consume re-delivers the same message.

**Q3: NoMessageLossFromCompaction**
- Type: Safety
- Statement: Compaction never causes message loss for any consumer. Compaction reorganizes data (WAL → COMPACTED) but both entry types are readable. The compaction cursor is orthogonal to consumer cursors — compaction does not consider or modify consumer state.

---

**End of S3-Queue Service Specification (Draft v1)**

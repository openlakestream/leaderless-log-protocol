# Leaderless Log Protocol — Canonical Specification

## 1. Protocol Overview

This protocol implements a **leaderless log architecture** where any writer can append to the log without being elected leader. Coordination is delegated to an external linearizable store (see `0-coordination-delegated-pattern.md`):

- **Sequence Counter** — assigns monotonic offsets via the coordination store's `AtomicIncrement`
- **Log Index** — linearizable key-value store providing atomic CAS operations for the log offset index

This protocol models three interacting sub-protocols:

1. **Writer Append Path**: A writer appends data to WAL storage and atomically assigns an offset via the coordination store
2. **Compaction Index Update**: A compactor replaces a range of WAL index entries with a single COMPACTED entry, then deletes old entries and advances the compaction cursor
3. **Reader Path**: A reader reads entries by scanning the log index; both WAL entries and COMPACTED entries are readable (the ML layer dispatches on file type to route reads to the appropriate storage backend)

Compaction's **non-atomic 3-step index update** can leave the index in an intermediate state where old WAL entries in `[startOffset, endOffset-1]` are deleted but the compaction cursor has not yet been updated. However, this does not cause data loss — the COMPACTED entry at `endOffset` covers the entire range and points to the PARQUET file containing the reorganized data.

## 2. System Model

### Processes
- **Writers** (`w1, w2`): Stateless processes that write and read entries. Any writer can operate on the log.
- **Compactor** (single process): Reads a range of WAL entries, produces a compacted file, and updates the index.

### Communication Model
All coordination goes through the **coordination store** (e.g., Oxia), modeled as an atomic linearizable KV store. There is no direct writer-to-writer or writer-to-compactor communication.

### Failure Model
- Writers may crash and restart (modeled by returning to IDLE state)
- Compactor may crash between any two steps of its 3-step update
- The coordination store is assumed reliable (no split-brain, no data loss)
- WAL storage writes may fail non-deterministically

### Coordination Store Abstract Interface (inlined)
```
Get(key) → (value, version) | NIL
Put(key, value, {IfRecordDoesNotExist}) → OK | KeyAlreadyExists
Put(key, value) → OK                         -- unconditional put
Delete(key) → OK
RangeDelete(startKey, endKey) → OK            -- deletes [start, end)
AtomicIncrement(prefix, delta) → newValue     -- atomic increment-and-return
```

All operations are atomic and linearizable.

### Multi-Record Entries (Sparse Index)

The log index uses **sparse keys**: each index entry covers N records where N ≥ 1. The key is stored at the **end offset** of the entry range.

- **Batch writes**: `AtomicIncrement` atomically increments the offset counter by `numberOfMessages` (the batch size), producing a single index entry at the end offset with `entryCount` and `entryOffsets` covering the entire batch.
- **Reader lookup**: Uses `CeilingGet` — a ceiling query that finds the smallest key ≥ the requested offset. This naturally handles sparse keys: looking up any offset within a multi-record entry finds the entry at its end offset.
- **Compaction**: Operates on whole entries — it cannot split a multi-record entry at an arbitrary boundary.

The model captures this with `[type |-> EntryType, msgCount |-> Nat]` records and ceiling-based lookup helpers.

## 3. State Space

### Constants

| Name | Type | Value | Description |
|------|------|-------|-------------|
| `Writers` | set | `{w1, w2}` | Set of writer process identifiers |
| `MaxOffset` | nat | `5` | Upper bound on offsets for bounded model checking |
| `CompactRangeStart` | nat | `1` | First offset in compaction range (round 1) |
| `CompactRangeEnd` | nat | `3` | Last offset in compaction range (round 1) |
| `CompactRange2Start` | nat | `4` | First offset in compaction range (round 2) |
| `CompactRange2End` | nat | `5` | Last offset in compaction range (round 2) |
| `MaxBatch` | nat | `2` | Max records per write batch (1 = dense/legacy, >1 = sparse) |

### Variables

| Variable | Type | Initial Value | Description |
|----------|------|---------------|-------------|
| `logIndex` | `[Nat → {type: EntryTypes, msgCount: Nat}]` | All `{type: EMPTY, msgCount: 0}` | Log offset index in the coordination store. Maps offset to entry metadata. Only end-offsets of entries have non-EMPTY values; intermediate offsets are EMPTY. |
| `sequenceCounter` | `Nat` | 1 | Next offset to assign (monotonic counter via AtomicIncrement) |
| `logState` | `{OPEN, FENCED}` | OPEN | Log operational state |
| `compactionCursor` | `Nat` | 1 | First offset not yet compacted |
| `writerState` | `[Writers → {IDLE, WRITING_WAL, ASSIGNING_OFFSET, DONE, FAILED}]` | All IDLE | Writer state machine |
| `writerOffset` | `[Writers → Nat ∪ {NONE}]` | All NONE | End offset assigned to current write |
| `writerBatchSize` | `[Writers → 0..MaxBatch]` | All 0 | Batch size (number of records) for current write |
| `compactorState` | `{IDLE, WRITING_COMPACTED_INDEX, DELETING_OLD, UPDATING_CURSOR, DONE}` | IDLE | Compactor state machine |
| `readerResult` | `[Writers → {NONE, OK, ERROR}]` | All NONE | Result of most recent read operation |
| `compactRound` | `{1, 2}` | 1 | Which sequential compaction round (1 or 2) |

## 4. Actions

### Writer Append Path

#### Action 1: `StartAppend(w)`
**Guard:** `writerState[w] = IDLE ∧ logState = OPEN`
**Effect:**
- Non-deterministically choose `count ∈ 1..MaxBatch`
- `writerBatchSize[w] := count`
- `writerState[w] := WRITING_WAL`

*The batch size determines how many records this write covers, corresponding to the `numberOfMessages` parameter of `AtomicIncrement`.*

#### Action 2: `WALWrite(w)`
**Guard:** `writerState[w] = WRITING_WAL`
**Effect:** Non-deterministic choice:
- **Success:** `writerState[w] := ASSIGNING_OFFSET`
- **Failure:** `writerState[w] := FAILED`

*Models the WAL storage write — the data-path operation that precedes coordination.*

#### Action 3: `AssignOffset(w)`
**Guard:** `writerState[w] = ASSIGNING_OFFSET ∧ logState = OPEN ∧ sequenceCounter + writerBatchSize[w] - 1 ≤ MaxOffset`
**Effect:** Let `off = sequenceCounter`, `count = writerBatchSize[w]`, `endOff = off + count - 1`:
- `sequenceCounter := endOff + 1`
- `logIndex[endOff] := {type: WAL, msgCount: count}`
- `writerOffset[w] := endOff`
- `writerState[w] := DONE`

*Models the coordination store's `AtomicIncrement` — the atomic offset assignment + index write. The sequence counter is incremented by `count` (the batch size), and a single index entry is written at the end offset covering offsets `[off, endOff]`.*

#### Action 4: `AppendComplete(w)`
**Guard:** `writerState[w] ∈ {DONE, FAILED}`
**Effect:**
- `writerState[w] := IDLE`
- `writerOffset[w] := NONE`
- `writerBatchSize[w] := 0`

*Completion callback returning to caller.*

### Compaction (Non-Atomic 3-Step Update)

#### Action 5: `CompactStart`
**Guard:** `compactorState = IDLE` and:
1. All offsets in `[CompactRangeStart, CompactRangeEnd]` are covered by WAL entries
2. All non-EMPTY entries in the range are fully contained (no entry straddles the range start boundary)

**Effect:**
- `compactorState := WRITING_COMPACTED_INDEX`

*Compaction reads WAL entries and produces a compacted file (external to index protocol). The guard ensures entry boundary alignment — with non-deterministic batch sizes, some write sequences produce entries that don't align with the compaction range. When alignment fails, compaction simply doesn't start.*

#### Action 6: `CompactWriteCompactedIndex`
**Guard:** `compactorState = WRITING_COMPACTED_INDEX`
**Effect:**
- `logIndex[CompactRangeEnd] := {type: COMPACTED, msgCount: CompactRangeEnd - CompactRangeStart + 1}`
- `compactorState := DELETING_OLD`

*Overwrites the end offset's index entry with COMPACTED type. The COMPACTED entry covers the entire compaction range.*

#### Action 7: `CompactDeleteOldEntries`
**Guard:** `compactorState = DELETING_OLD`
**Effect:** For all `off ∈ {CompactRangeStart..CompactRangeEnd - 1}`:
- `logIndex[off] := {type: EMPTY, msgCount: 0}`
- `compactorState := UPDATING_CURSOR`

*Uses the coordination store's `RangeDelete` to remove WAL entries from `[start, end-1]`.*

#### Action 8: `CompactUpdateCursor`
**Guard:** `compactorState = UPDATING_CURSOR`
**Effect:**
- `compactionCursor := CompactRangeEnd + 1`
- `compactorState := DONE`

*Advances the compaction cursor past the compacted range.*

#### Action 9: `CompactReset`
**Guard:** `compactorState = DONE`
**Effect:**
- `compactorState := IDLE`

*Compactor returns to main loop ready for next task.*

### Fencing

#### Action 10: `FenceLog`
**Guard:** `logState = OPEN`
**Effect:**
- `logState := FENCED`

*External trigger (e.g., ownership change) fences the log via `CompareAndSet` on the log state.*

#### Action 11: `UnfenceLog`
**Guard:** `logState = FENCED`
**Effect:**
- `logState := OPEN`

*Log recovery or re-assignment.*

### Reader Path

#### Action 12: `ReadEntry(w, off)`
**Guard:** `writerState[w] = IDLE ∧ off ≥ 1 ∧ off < sequenceCounter`
**Effect:** Ceiling lookup — find the smallest end-offset `o ≥ off` where `logIndex[o]` is non-EMPTY and covers `off`:
- If a covering entry exists: `readerResult[w] := OK`
- If no covering entry exists (offset was deleted mid-compaction): `readerResult[w] := ERROR`

*Both WAL and COMPACTED entries are readable. Compaction reorganizes data from WAL files into compacted files but does not delete the data. A reader implementation dispatches on entry file type to read from the appropriate storage backend. The ceiling lookup models the coordination store's nearest-neighbor query on sorted keys.*

## 5. Correctness Properties

### Safety Properties

#### S1: `NoOffsetDuplicates`
**Type:** Invariant
**Statement:** For all offsets `o`: if `logIndex[o].type ≠ EMPTY` and `logIndex[o2].type ≠ EMPTY`, then they have distinct positions (unless one is a COMPACTED entry covering a range).
**Expected Verdict:** PASS

#### S2: `MonotonicOffsets`
**Type:** Invariant
**Statement:** `sequenceCounter ≥ 1` (sequenceCounter never decreases below initial value)
**Expected Verdict:** PASS

#### S3: `FencedRejectsAppends`
**Type:** Invariant
**Statement:** If `logState = FENCED`, then no `AssignOffset(w)` action is enabled for any writer `w`.
**Expected Verdict:** PASS

#### S4: `CompactionPreservesData`
**Type:** Invariant
**Statement:** After `CompactWriteCompactedIndex`, `logIndex[CompactRangeEnd].type = COMPACTED`. The COMPACTED entry logically covers offsets `[CompactRangeStart, CompactRangeEnd]`.
**Expected Verdict:** PASS

#### S5: `NoPhantomEntries`
**Type:** Invariant
**Statement:** After `CompactDeleteOldEntries`, for all `off ∈ {CompactRangeStart..CompactRangeEnd - 1}`: `logIndex[off].type = EMPTY`.
**Expected Verdict:** PASS

#### S6: `CursorConsistency`
**Type:** Invariant
**Statement:** `compactionCursor ≤ sequenceCounter` — the compaction cursor never exceeds the next assignable offset.
**Expected Verdict:** PASS

#### S7: `NoOverlappingRanges`
**Type:** Invariant
**Statement:** No two non-EMPTY index entries at distinct offsets cover overlapping offset ranges. For entries at `o1` and `o2`, the ranges `[o1 - msgCount1 + 1, o1]` and `[o2 - msgCount2 + 1, o2]` do not overlap.
**Expected Verdict:** PASS

#### S8: `NoReaderError`
**Type:** Invariant
**Statement:** No reader ever encounters an ERROR result: `∀ w: readerResult[w] ≠ ERROR`.
**Expected Verdict:** PASS — The 3-step ordering (write COMPACTED before delete) guarantees that the ceiling lookup always finds a covering entry for any committed offset. This invariant directly verifies the continuous readability guarantee.

#### S9: `SequentialCompactionSafety`
**Type:** Invariant (compositionality)
**Statement:** After round 1 completes (`compactRound = 2`), round 1's COMPACTED entry persists: `compactRound = 2 ⇒ logIndex[CompactRangeEnd].type = COMPACTED`.
**Expected Verdict:** PASS — Round 2 operates on a disjoint, strictly higher offset range (`[CompactRange2Start, CompactRange2End]`) and never overwrites round 1's index entries. This machine-checks the compositionality argument for sequential compaction.

### Liveness Properties

#### L1: `AppendProgress`
**Type:** Temporal (leads-to)
**Statement:** If a writer starts writing WAL, it eventually completes (DONE or FAILED).
**Formal:** `∀ w: (writerState[w] = WRITING_WAL) ~> (writerState[w] = DONE ∨ writerState[w] = FAILED)`
**Expected Verdict:** PASS (under weak fairness on writer actions)

#### L2: `CompactionCompletes`
**Type:** Temporal (leads-to)
**Statement:** If compaction starts, it eventually reaches DONE.
**Formal:** `(compactorState = WRITING_COMPACTED_INDEX) ~> (compactorState = DONE)`
**Expected Verdict:** PASS (under weak fairness on compactor actions)

#### L3: `ReaderEventuallySucceeds`
**Type:** Temporal (leads-to)
**Statement:** A reader that gets `ERROR` result eventually gets `OK`.
**Formal:** `∀ w: (readerResult[w] = ERROR) ~> (readerResult[w] = OK)`
**Expected Verdict:** **PASS** — Under the current 3-step ordering (write COMPACTED before delete), ERROR is unreachable: the ceiling lookup always finds a covering entry for any committed offset. The property holds vacuously but is retained to guard against future specification changes that might alter the step ordering.

## 6. Fairness Assumptions

| Action | Fairness | Rationale |
|--------|----------|-----------|
| `StartAppend(w)` | Weak fairness | Writers continuously attempt writes |
| `WALWrite(w)` | Weak fairness | WAL storage is reliable |
| `AssignOffset(w)` | Weak fairness | Coordination store is reliable |
| `AssignOffsetFenced(w)` | Weak fairness | Failure path must complete for L1 |
| `AssignOffsetExhausted(w)` | Weak fairness | Failure path completes (modeling artifact) |
| `AppendComplete(w)` | Weak fairness | Completion always runs |
| `CompactStart` | Weak fairness | Compaction is periodically triggered |
| `CompactWriteCompactedIndex` | Weak fairness | Each compaction step completes |
| `CompactDeleteOldEntries` | Weak fairness | Each compaction step completes |
| `CompactUpdateCursor` | Weak fairness | Each compaction step completes |
| `CompactReset` | Weak fairness | Compactor resets |
| `FenceLog` | None | Fencing is an external event, not guaranteed |
| `UnfenceLog` | None | Unfencing is an external event |
| `ReadEntry(w, off)` | Weak fairness | Readers continuously read |

## 7. Model Checking Parameters

| Parameter | Value | Rationale |
|-----------|-------|-----------|
| `Writers` | `{w1, w2}` | Two writers sufficient for write-write and write-compact interleavings |
| `MaxOffset` | `5` | Enough for two compaction ranges + concurrent writes |
| `CompactRangeStart` | `1` | Round 1: compact from beginning |
| `CompactRangeEnd` | `3` | Round 1: 3-entry range tests all compaction steps |
| `CompactRange2Start` | `4` | Round 2: immediately after round 1 |
| `CompactRange2End` | `5` | Round 2: covers remaining offsets |
| `MaxBatch` | `2` | Tests both single-record and multi-record writes |

**Expected state space:** ~2×10^5 states with small config (manageable by TLC and Fizzbee). Sequential compaction adds ~2.4× states vs. single-round model.

## 8. Modeling Simplifications

The following intentional simplifications exist between this spec and implementations:

- **Compaction fast path**: Implementations may have a 2-step fast path when the `endOffset` entry is already deleted (skips the update step). The model always uses the 3-step sequence (write COMPACTED → delete old → update cursor), which is a strict superset of behaviors.
- **Reader dispatch on file type**: In implementations, the ML layer dispatches on `position.fileType()` to route reads to the appropriate backend (WAL reader for RAW, lakehouse reader for PARQUET). The model abstracts this as returning OK for any non-EMPTY covering entry, since both WAL and COMPACTED data is readable.
- **EMPTY models key absence**: The `EMPTY` entry type in the model represents the absence of a key in the coordination store (i.e., a `null` return from `Get()`), not an explicit stored value.
- **CompactStart guard**: The model requires all offsets in the compaction range to have WAL coverage and all entries to be fully contained within the range before starting. Implementations may discover entry types during iteration. The model's stronger guard is conservative and does not exclude valid behaviors.
- **Ceiling lookup vs range scan**: The model uses a `CHOOSE`-based ceiling lookup over the offset array to find covering entries. Real implementations use the coordination store's native `CeilingGet` query. The semantics are equivalent for correctness.
- **Entry boundary alignment**: With non-deterministic batch sizes, some write sequences produce entries that don't align with the fixed `CompactRangeEnd`. The `CompactStart` guard naturally prevents compaction in these cases — matching real implementations' inability to compact partial entries.

## 9. Coordination Store Requirements

This protocol requires the following primitives from Layer 0 (`0-coordination-delegated-pattern.md`):

| Primitive | Usage |
|-----------|-------|
| `AtomicIncrement` | Sequence counter for offset assignment |
| `Put` | Writing index entries, updating compaction cursor |
| `Get` | Reading index entries, reading log state |
| `CeilingGet` | Sparse index lookup for covering entries |
| `Delete` | Removing individual index entries |
| `RangeDelete` | Batch deletion of old WAL entries during compaction |

## 10. Reference Implementation Mapping (S3-Queue)

The following shows how the [S3-Queue Rust example](examples/s3-queue/) implements this protocol on S3-compatible object storage. This is one possible implementation, not the only way — see [`examples/s3-queue/SPEC.md`](examples/s3-queue/SPEC.md) for the full system-level design.

| Spec Action | Rust Source | Coordination Primitive |
|-------------|-------------|------------------------|
| `StartAppend(w)` | [`queue/producer.rs`](examples/s3-queue/impl/src/queue/producer.rs) | — |
| `WALWrite(w)` | [`queue/producer.rs`](examples/s3-queue/impl/src/queue/producer.rs) | S3 `PutObject` to WAL prefix |
| `AssignOffset(w)` | [`queue/producer.rs`](examples/s3-queue/impl/src/queue/producer.rs) (inlined CAS loop on `meta/sequence-counter`) | [`coordination/cas.rs`](examples/s3-queue/impl/src/coordination/cas.rs) |
| `AppendComplete(w)` | [`queue/producer.rs`](examples/s3-queue/impl/src/queue/producer.rs) (pending-entry materialization) | — |
| `CompactStart` | [`queue/compaction.rs`](examples/s3-queue/impl/src/queue/compaction.rs) | — |
| `CompactWriteCompactedIndex` | [`queue/compaction.rs`](examples/s3-queue/impl/src/queue/compaction.rs) | S3 `PutObject` |
| `CompactDeleteOldEntries` | [`queue/compaction.rs`](examples/s3-queue/impl/src/queue/compaction.rs) | [`coordination/range_delete.rs`](examples/s3-queue/impl/src/coordination/range_delete.rs) |
| `CompactUpdateCursor` | [`queue/compaction.rs`](examples/s3-queue/impl/src/queue/compaction.rs) | [`coordination/cas.rs`](examples/s3-queue/impl/src/coordination/cas.rs) |
| `FenceLog` | [`queue/fence.rs`](examples/s3-queue/impl/src/queue/fence.rs) | [`coordination/cas.rs`](examples/s3-queue/impl/src/coordination/cas.rs) |
| `ReadEntry(w, off)` | [`queue/consumer.rs`](examples/s3-queue/impl/src/queue/consumer.rs) | [`coordination/ceiling_get.rs`](examples/s3-queue/impl/src/coordination/ceiling_get.rs) |

**Note:** The `coordination/` directory also contains [`atomic_increment.rs`](examples/s3-queue/impl/src/coordination/atomic_increment.rs), a reusable `AtomicIncrementWithPending` primitive. The producer currently inlines the CAS loop for efficiency; this module demonstrates how the same logic can be factored out as a reusable primitive.

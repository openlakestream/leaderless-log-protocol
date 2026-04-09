# Coordination-Delegated Protocol Specifications

Formal TLA+ and Fizzbee models for **coordination-delegated distributed systems protocols** — protocols where stateless worker nodes delegate all coordination to an external linearizable store.

## Directory Structure

```
├── 0-coordination-delegated-pattern.md      -- Layer 0: The pattern
├── 1-leaderless-log-protocol.md             -- Protocol 1: Canonical spec
├── 2-coordination-delegated-task-claiming.md -- Protocol 2: Canonical spec
├── tlaplus/
│   ├── LeaderlessLog.tla                    -- TLA+: Protocol 1
│   ├── LeaderlessLog.cfg                    -- Small TLC config
│   ├── LeaderlessLog-medium.cfg             -- Medium TLC config
│   ├── TaskClaiming.tla                     -- TLA+: Protocol 2
│   ├── TaskClaiming.cfg                     -- Small TLC config
│   └── TaskClaiming-medium.cfg              -- Medium TLC config
├── fizzbee/
│   ├── LeaderlessLog.fizz                   -- Fizzbee: Protocol 1
│   └── TaskClaiming.fizz                    -- Fizzbee: Protocol 2
└── examples/
    └── s3-queue/                            -- Reference implementation: S3-Queue (Rust)
        ├── SPEC.md                          -- System SPEC
        └── impl/                            -- Rust implementation
```

## Protocol Summaries

### Protocol 1: Leaderless Log

Spec: `1-leaderless-log-protocol.md`

Distributed append-only log with concurrent writers, compaction, and readers. Uses the coordination store for monotonic offset assignment (AtomicIncrement) and log index management (atomic CAS).

**Resolved modeling issue (L3 — ReaderEventuallySucceeds, formerly ReaderNotPermanentlyStuck):** The original formal model incorrectly treated COMPACTED entries as unreadable (STUCK). In reality, compaction reorganizes data from WAL files into compacted files — the data is never deleted. A reader implementation dispatches on entry file type to read from the appropriate storage backend. The model was corrected so that `ReadEntry` returns OK for both WAL and COMPACTED entries. L3 was reformulated from a vacuously-true STUCK→OK leads-to into a meaningful ERROR→OK leads-to property. The unused "STUCK" reader result was removed.

### Protocol 2: Task Claiming

Spec: `2-coordination-delegated-task-claiming.md`

Distributed task claiming with ephemeral locks and crash recovery. Workers scan for tasks, acquire ephemeral locks via the coordination store, execute tasks, and release locks. Crashed workers' locks expire via session timeout.

All properties **PASS** in both TLA+ and Fizzbee.

## Key Files for Review

### TLA+
- `tlaplus/LeaderlessLog.tla` — Actions 1-12, properties S1-S6 and L1-L3
- `tlaplus/TaskClaiming.tla` — Actions 1-7, properties S1-S5 and L1-L3

### Fizzbee
- `fizzbee/LeaderlessLog.fizz` — All actions and properties
- `fizzbee/TaskClaiming.fizz` — All actions and properties

### Reference Implementation (Rust): `examples/s3-queue/` — Protocol 1 only
- `examples/s3-queue/SPEC.md` — System SPEC: Leaderless Log applied to message queue on S3
- `examples/s3-queue/impl/src/queue/producer.rs` — Append flow (StartAppend → WALWrite → AssignOffset → AppendComplete)
- `examples/s3-queue/impl/src/queue/consumer.rs` — ReadEntry path
- `examples/s3-queue/impl/src/queue/compaction.rs` — Compaction (3-step CAS update)
- `examples/s3-queue/impl/src/queue/fence.rs` — Fence/Unfence
- `examples/s3-queue/impl/src/coordination/atomic_increment.rs` — AtomicIncrement via S3
- `examples/s3-queue/impl/src/coordination/cas.rs` — CAS via S3 conditional writes
- `examples/s3-queue/impl/src/coordination/ceiling_get.rs` — CeilingGet for sparse index lookup
- `examples/s3-queue/impl/src/coordination/range_delete.rs` — RangeDelete for WAL cleanup

**Note:** Protocol 2 (Task Claiming) does not currently have a reference implementation in this repo.

## Spec-First Approach

The canonical **spec docs** (Markdown) define each protocol. Both TLA+ and Fizzbee are independent implementations of the same spec. The spec doc is the source of truth.

- If TLA+ and Fizzbee disagree on a property verdict, the spec doc resolves which is wrong
- Every state variable, action, and property in the spec has a corresponding construct in both TLA+ and Fizzbee

## Running Models Locally

### TLA+ (TLC Model Checker)

```bash
cd tlaplus

# Protocol 1 — Leaderless Log (all PASS)
java -jar tla2tools.jar -config LeaderlessLog.cfg LeaderlessLog.tla

# Protocol 2 — Task Claiming (all PASS)
java -jar tla2tools.jar -config TaskClaiming.cfg TaskClaiming.tla
```

### Fizzbee

```bash
# Install Fizzbee CLI locally (one-time setup)
make fizzbee-install

# Run Fizzbee model checker on all specs
make fizzbee

# Or run individual specs (requires fizzbee installed)
cd fizzbee && ~/.local/fizzbee/fizz LeaderlessLog.fizz
cd fizzbee && ~/.local/fizzbee/fizz TaskClaiming.fizz
```

### Run All Checks

```bash
make verify    # Run both Fizzbee and TLA+ model checkers
make check     # Alias for verify
```

## Issue Filing Rules

- **Spec or model issues** → file in `lakestream-io/leaderless-log-protocol`
- **Reference implementation drift** (`examples/s3-queue/impl/` doesn't match `examples/s3-queue/SPEC.md`) → file in `lakestream-io/leaderless-log-protocol`
- Label spec issues with `formal-verification`
- Label implementation drift with `spec-drift`

## Expected Property Verdicts

### Protocol 1: Leaderless Log

| Property | Type | TLA+ | Fizzbee | Notes |
|----------|------|------|---------|-------|
| TypeOK | Invariant | PASS | — | TLA+ only |
| SequenceCounterPositive | Safety | PASS | — | TLA+ only (MonotonicOffsets in Fizzbee) |
| NoOffsetDuplicates | Safety | — | PASS | Fizzbee only |
| MonotonicOffsets | Safety | — | PASS | Fizzbee only |
| FencedRejectsAppends | Safety | PASS | — | See note ¹ |
| CompactionPreservesData | Safety | PASS | PASS | |
| NoPhantomEntries | Safety | PASS | PASS | |
| CursorConsistency | Safety | PASS | PASS | |
| NoOverlappingRanges | Safety | PASS | PASS | WAL-WAL only; see note ³ |
| NoReaderError | Safety | PASS | PASS | |
| SequentialCompactionSafety | Safety (compositionality) | PASS | PASS | |
| AppendProgress | Liveness | PASS | — | Requires fairness (TLA+ only) |
| CompactionCompletes | Liveness | PASS | — | Requires fairness (TLA+ only) |
| ReaderEventuallySucceeds | Liveness (vacuous) | PASS | — | Requires fairness (TLA+ only) |

¹ FencedRejectsAppends: The spec says "AssignOffset is not enabled when fenced", which is guaranteed by action guards. As a state invariant, it can be transiently violated when FenceLog fires while a writer is in ASSIGNING_OFFSET. Omitted from Fizzbee; correctness is enforced by action guards.

³ NoOverlappingRanges: Checks WAL-WAL overlap only. WAL-COMPACTED overlap is intentionally allowed during compaction's write-before-delete (COMPACTED entry is written before old WAL entries are deleted). After CompactorCrash during DELETING_OLD, the overlap may persist permanently, which is harmless for readers.

### Protocol 2: Task Claiming

| Property | Type | TLA+ | Fizzbee | Notes |
|----------|------|------|---------|-------|
| MutualExclusion | Safety | PASS | PASS | |
| NoDoubleExecution | Safety | PASS | PASS | |
| DLQOnlyAfterMaxFailures | Safety | PASS | PASS | |
| LockConsistency | Safety | PASS | PASS | |
| NoOrphanExecution | Safety | PASS | PASS | See note ² |
| TaskCompletion | Liveness | PASS | — | Requires fairness (TLA+ only) |
| CrashRecovery | Liveness | PASS | — | Requires fairness (TLA+ only) |
| NoStarvation | Liveness | PASS | — | Requires fairness (TLA+ only) |

² NoOrphanExecution: Fizzbee version excludes the dead worker itself from the check (w2 ≠ w), since the dead worker's state is not cleaned up until SessionExpiry.

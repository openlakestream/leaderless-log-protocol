# Layer 0: The Coordination-Delegated Pattern

## 1. Abstract

The **Coordination-Delegated Pattern** is a distributed systems architectural approach where stateless worker nodes delegate *all* coordination responsibilities — ordering, mutual exclusion, state transitions, and failure detection — to an external linearizable store. By externalizing consensus rather than embedding it (as in Raft or Paxos), the pattern enables horizontally scalable, leaderless architectures where any node can perform any operation without election or quorum protocols.

## 2. Motivation

Traditional distributed systems embed consensus into the data path:

| Approach | Mechanism | Trade-off |
|----------|-----------|-----------|
| **Raft / Paxos** | Replicated state machine with leader election | Throughput bottlenecked by single leader; complex implementation |
| **Chain Replication** | Write head → tail pipeline | Ordered but sequential; complex reconfiguration |
| **Leaderless (Dynamo-style)** | Quorum reads/writes, vector clocks | Eventual consistency; conflict resolution complexity |

The Coordination-Delegated Pattern takes a different approach: it **separates coordination from the data path entirely**. Worker nodes handle data operations (reads, writes, compaction) while an external linearizable store handles all coordination decisions. This yields:

1. **No leader election**: Any node can perform any operation — horizontal write scaling is trivial
2. **Simplified correctness**: Coordination logic is confined to well-defined atomic operations on a linearizable store
3. **Crash recovery via sessions**: Node failures are detected by session expiry in the coordination store; no heartbeat protocol needed
4. **Linearizable ordering**: The coordination store provides total ordering where needed (e.g., offset assignment)

The trade-off is a hard dependency on the coordination store's availability and the latency cost of coordination RPCs. This pattern is appropriate when coordination is *infrequent relative to the data path* — e.g., assigning an offset once per batch write, rather than coordinating every individual record.

## 3. The Pattern

```
┌─────────────────────────────────────────────────────────────┐
│                    Coordination Store                        │
│            (Linearizable Key-Value Store)                    │
│                                                             │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐     │
│  │ Atomic CAS   │  │ Atomic       │  │ Ephemeral    │     │
│  │ Operations   │  │ Increment    │  │ Records      │     │
│  └──────────────┘  └──────────────┘  └──────────────┘     │
└──────────┬──────────────────┬───────────────┬───────────────┘
           │                  │               │
     ┌─────┴─────┐     ┌─────┴─────┐   ┌─────┴─────┐
     │  Worker A  │     │  Worker B  │   │  Worker C  │
     │ (stateless)│     │ (stateless)│   │ (stateless)│
     └─────┬─────┘     └─────┬─────┘   └─────┬─────┘
           │                  │               │
     ┌─────┴──────────────────┴───────────────┴─────┐
     │              Data Storage Layer               │
     │      (WAL, object store, local disk, etc.)    │
     └───────────────────────────────────────────────┘
```

**Core principle:** Worker nodes are stateless with respect to coordination. All shared state — counters, locks, fences, cursors — lives in the coordination store. Workers read from and write to the coordination store using atomic primitives, never communicating with each other directly.

**Coordination responsibilities delegated:**

| Responsibility | Mechanism | Example |
|---------------|-----------|---------|
| **Ordering** | AtomicIncrement on a sequence counter | Offset assignment in a log |
| **Mutual exclusion** | ConditionalCreate of an ephemeral lock record | Task claiming |
| **State transitions** | CompareAndSet on a state key | Fencing a log |
| **Failure detection** | Session-scoped ephemeral records | Worker crash releases locks |
| **Index management** | Put/Get/Delete on structured keys | Log entry metadata |

## 4. Required Coordination Primitives

The coordination store must provide the following abstract interface. Not all implementations need to support every primitive — specific protocols use specific subsets.

### Core Primitives

```
CompareAndSet(key, expected, new) → OK | ConflictError
```
Atomically update `key` to `new` only if current value equals `expected`. Foundation for state transitions (e.g., OPEN → FENCED).

```
AtomicIncrement(key, delta) → newValue
```
Atomically increment a counter by `delta` and return the new value. Foundation for total ordering (e.g., offset assignment in a log).

```
ConditionalCreate(key, value) → OK | AlreadyExists
```
Create `key` with `value` only if `key` does not already exist. Foundation for mutual exclusion (e.g., lock acquisition).

```
EphemeralRecord(key, value, session) → OK
```
Create a record tied to a client session. The record is **automatically deleted** when the session expires (client crash, network partition, explicit close). Foundation for crash recovery without explicit heartbeats.

### Data Primitives

```
Get(key) → (value, version) | NIL
```
Read a key's value and version. Version enables conditional operations.

```
Put(key, value) → OK
```
Unconditionally write a value at a key.

```
Delete(key) → OK
```
Unconditionally delete a key.

```
ConditionalDelete(key, expectedVersion) → OK | VersionMismatch
```
Delete a key only if its current version matches `expectedVersion`. Enables safe lock release without race conditions.

```
RangeDelete(startKey, endKey) → OK
```
Atomically delete all keys in the range `[startKey, endKey)`. Foundation for batch cleanup (e.g., compaction deleting old index entries).

```
CeilingGet(key) → (key', value) | NIL
```
Return the smallest key `key' ≥ key` that exists. Foundation for sparse index lookup (e.g., finding the covering entry for an offset in a multi-record index).

## 5. Properties Guaranteed

When applied correctly, the Coordination-Delegated Pattern guarantees:

1. **No leader election**: Workers are interchangeable. Scaling is achieved by adding workers, not by changing the consensus topology.

2. **Horizontal write scaling**: Multiple writers can operate concurrently on the same data structure (e.g., a log), with ordering resolved atomically by the coordination store.

3. **Crash recovery via sessions**: When a worker crashes, its ephemeral records in the coordination store are automatically deleted after session timeout. No explicit failure detector or gossip protocol is needed.

4. **Linearizable ordering**: Operations that require total ordering (e.g., offset assignment) inherit linearizability from the coordination store's AtomicIncrement.

5. **Fence-based safety**: Critical sections can be guarded by fencing tokens stored in the coordination store. A fenced resource rejects further mutations until explicitly unfenced.

## 6. Limitations

1. **Coordination store availability dependency**: The coordination store is a single point of failure for *coordination* (not data). If the coordination store is unavailable, workers cannot make coordination decisions (assign offsets, acquire locks, fence resources). Data reads from local storage may still succeed.

2. **Latency bound by coordination store RTT**: Every coordinated operation requires at least one round-trip to the coordination store. This is acceptable when coordination is infrequent relative to the data path (e.g., one coordination RPC per batch of thousands of records) but prohibitive when coordination IS the data path.

3. **Not suitable when coordination IS the data path**: If every operation requires coordination (e.g., strongly consistent reads from a replicated state machine), the pattern adds overhead without benefit. Use embedded consensus (Raft/Paxos) instead.

4. **Session timeout granularity**: Crash detection depends on the coordination store's session timeout, typically seconds to tens of seconds. During this window, locks held by a crashed worker are not released. Applications must tolerate this delay.

5. **Coordination store capacity**: The coordination store must handle the aggregate coordination load of all workers. For very high-throughput systems, the coordination store itself may become a bottleneck. Techniques like batching and hierarchical coordination can mitigate this.

## 7. Reference Implementations

### Coordination Stores

| Store | Primitives Supported | Notes |
|-------|---------------------|-------|
| **Apache Oxia** | All primitives | Purpose-built for this pattern. Native support for AtomicIncrement (SequenceKeysDeltas), ephemeral records with session management, CeilingGet (ComparisonHigher), and RangeDelete. Primary reference implementation. |
| **etcd** | CompareAndSet, Get, Put, Delete, ConditionalDelete | Lease-based ephemeral records. No native AtomicIncrement (must use CAS loop). No CeilingGet (must use range queries with sorting). |
| **Apache ZooKeeper** | ConditionalCreate, EphemeralRecord, Get, Put, Delete | Native ephemeral znodes. Sequential znodes provide ordering. No AtomicIncrement or CeilingGet. |
| **FoundationDB** | All except EphemeralRecord | Full transactional support enables implementing any primitive. Ephemeral records require application-level session tracking. |

### Protocol Implementations

| Protocol | Reference Implementation | Coordination Store |
|----------|-------------------------|-------------------|
| Leaderless Log Protocol (Layer 1) | [`examples/s3-queue/`](examples/s3-queue/) (Rust) | S3-compatible object storage |
| Task Claiming Protocol (Layer 2) | — (contributions welcome) | — |

## 8. Protocol Family

This pattern is instantiated by two formally verified protocols:

### Protocol 1: Leaderless Log Protocol (Layer 1)

A protocol for maintaining a distributed append-only log where multiple writers concurrently append entries without leader election. Coordination primitives used: `AtomicIncrement`, `Put`, `Get`, `CeilingGet`, `Delete`, `RangeDelete`.

→ See `1-leaderless-log-protocol.md`

### Protocol 2: Coordination-Delegated Task Claiming Protocol (Layer 2)

A protocol for distributed task claiming where multiple workers independently discover, lock, execute, and release tasks with crash recovery via session-scoped ephemeral locks. Coordination primitives used: `ConditionalCreate`, `EphemeralRecord`, `ConditionalDelete`, `Get`.

→ See `2-coordination-delegated-task-claiming.md`

Both protocols are independent and composable. A system may use Protocol 1 alone (e.g., for a distributed log without background task processing), Protocol 2 alone (e.g., for distributed job scheduling without a log), or both together (e.g., a log with background compaction).

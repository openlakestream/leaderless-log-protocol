# Coordination-Delegated Task Claiming Protocol — Canonical Specification

## 1. Protocol Overview

This protocol enables multiple **workers** to independently discover, claim, execute, and release tasks. Coordination uses **ephemeral locks** in an external coordination store (see `0-coordination-delegated-pattern.md`) to ensure mutual exclusion: at most one worker executes a given task at any time.

The protocol must handle:
- **Concurrent claiming**: Multiple workers racing to lock the same task
- **Worker crashes**: A worker dies while holding a lock; the lock must eventually be released via coordination store session expiry
- **Task lifecycle**: Tasks progress through INIT → PREPARED → COMMITTED, or INIT → DLQ on repeated failures

Key components:
- **Task manager** — task CRUD and lock management via the coordination store
- **Ephemeral lock** — coordination-store-backed CAS lock with ephemeral record semantics
- **Worker** — main worker loop: scan → lock → execute → unlock

## 2. System Model

### Processes
- **Workers** (`w1, w2, w3`): Independent processes running the worker loop. Each worker scans for available tasks, attempts to lock one, executes the task, and releases the lock.

### Communication Model
All coordination goes through the **coordination store** (e.g., Oxia). Workers do not communicate directly. Task state and locks are stored in the coordination store as key-value pairs.

### Failure Model
- Workers may **crash** at any point during execution. A crashed worker stops all activity.
- When a worker crashes, its **coordination store session** eventually expires, causing all ephemeral records (locks) held by that session to be automatically deleted.
- Workers may **recover** after crashing and rejoin the worker pool.
- The coordination store is assumed reliable (linearizable, no data loss).
- Task execution may fail non-deterministically.

### Coordination Store Abstract Interface (inlined)
```
Get(key) → (value, version) | NIL
ConditionalCreate(key, value, {AsEphemeralRecord}) → OK | AlreadyExists
    -- Creates ephemeral record tied to worker's session
    -- Auto-deleted when session expires
ConditionalDelete(key, {IfVersionEquals(v)}) → OK | VersionMismatch
    -- Conditional delete for safe lock release
```

## 3. State Space

### Constants

| Name | Type | Value | Description |
|------|------|-------|-------------|
| `Workers` | set | `{w1, w2, w3}` | Set of worker process identifiers |
| `Tasks` | set | `{t1, t2}` | Set of tasks |
| `MaxFailures` | nat | `2` | Maximum failures before task moves to DLQ |

### Variables

| Variable | Type | Initial Value | Description |
|----------|------|---------------|-------------|
| `taskStatus` | `[Tasks → {INIT, PREPARED, COMMITTED, DLQ}]` | All INIT | Task lifecycle state |
| `lockState` | `[Tasks → {UNLOCKED} ∪ {LOCKED(w, v) : w ∈ Workers, v ∈ Nat}]` | All UNLOCKED | Ephemeral lock. LOCKED(w, v) means worker w holds lock at version v. |
| `workerAlive` | `[Workers → BOOLEAN]` | All TRUE | Whether worker process is alive |
| `workerState` | `[Workers → {IDLE, SCANNING, EXECUTING, UNLOCKING}]` | All IDLE | Worker state machine |
| `workerTask` | `[Workers → Tasks ∪ {NONE}]` | All NONE | Task currently being processed by worker |
| `taskFailCount` | `[Tasks → Nat]` | All 0 | Number of failed execution attempts |
| `lockVersion` | `Nat` | 1 | Global version counter for lock operations (models coordination store version IDs) |

## 4. Actions

### Worker Lifecycle

#### Action 1: `ScanTasks(w, t)`
**Guard:** `workerAlive[w] = TRUE ∧ workerState[w] = IDLE ∧ taskStatus[t] = INIT`
**Effect:**
- `workerState[w] := SCANNING`
- `workerTask[w] := t`

*Worker scans for available tasks and picks one to process.*

#### Action 2: `TryLockTask(w)` — Success
**Guard:** `workerAlive[w] = TRUE ∧ workerState[w] = SCANNING ∧ lockState[workerTask[w]] = UNLOCKED ∧ taskStatus[workerTask[w]] = INIT`
**Effect:** Let `t = workerTask[w]`:
- `lockState[t] := LOCKED(w, lockVersion)`
- `lockVersion := lockVersion + 1`
- `workerState[w] := EXECUTING`

*Lock acquisition uses `ConditionalCreate` with ephemeral record semantics on the coordination store. The task status is re-checked to prevent locking a task that was completed between scan and lock.*

#### Action 2b: `TryLockTask(w)` — Failure
**Guard:** `workerAlive[w] = TRUE ∧ workerState[w] = SCANNING ∧ (lockState[workerTask[w]] = LOCKED(w', v) for some w' ≠ w  ∨  taskStatus[workerTask[w]] ≠ INIT)`
**Effect:**
- `workerState[w] := IDLE`
- `workerTask[w] := NONE`

*Lock held by another worker, or task no longer INIT (completed or DLQ'd between scan and lock attempt). Worker backs off.*

#### Action 3: `ExecuteTask(w)` — Success
**Guard:** `workerAlive[w] = TRUE ∧ workerState[w] = EXECUTING ∧ taskStatus[workerTask[w]] = INIT`
**Effect:** Let `t = workerTask[w]`:
- `taskStatus[t] := COMMITTED`
- `workerState[w] := UNLOCKING`

*Task execution succeeds and the task advances to COMMITTED.*

#### Action 3b: `ExecuteTask(w)` — Failure
**Guard:** `workerAlive[w] = TRUE ∧ workerState[w] = EXECUTING`
**Effect:** Let `t = workerTask[w]`:
- `taskFailCount[t] := taskFailCount[t] + 1`
- If `taskFailCount[t] ≥ MaxFailures`: `taskStatus[t] := DLQ`
- `workerState[w] := UNLOCKING`

*Task execution fails. The task is moved to Dead Letter State (DLQ) after exceeding the maximum failure count.*

#### Action 4: `UnlockTask(w)`
**Guard:** `workerAlive[w] = TRUE ∧ workerState[w] = UNLOCKING`
**Effect:** Let `t = workerTask[w]`:
- If `lockState[t] = LOCKED(w, v)`: `lockState[t] := UNLOCKED`
- (If lock was already released by session expiry, this is a no-op)
- `workerState[w] := IDLE`
- `workerTask[w] := NONE`

*Uses `ConditionalDelete` with version matching to safely release the lock.*

### Crash and Recovery

#### Action 5: `WorkerCrash(w)`
**Guard:** `workerAlive[w] = TRUE`
**Effect:**
- `workerAlive[w] := FALSE`

*Process crash (OOM kill, hardware failure, etc.). The worker stops executing but its coordination store session persists briefly.*

#### Action 6: `SessionExpiry(w)`
**Guard:** `workerAlive[w] = FALSE ∧ ∃ t ∈ Tasks: lockState[t] = LOCKED(w, v)`
**Effect:** For all tasks `t` where `lockState[t] = LOCKED(w, v)`:
- `lockState[t] := UNLOCKED`
- `workerState[w] := IDLE`
- `workerTask[w] := NONE`

*Coordination store ephemeral record auto-deletion when session expires. All ephemeral records (locks) created by the crashed worker's session are automatically deleted.*

#### Action 7: `WorkerRecover(w)`
**Guard:** `workerAlive[w] = FALSE`
**Effect:**
- `workerAlive[w] := TRUE`
- `workerState[w] := IDLE`
- `workerTask[w] := NONE`

*Worker process restart. The recovered worker has no memory of previous state and starts scanning fresh.*

## 5. Correctness Properties

### Safety Properties

#### S1: `MutualExclusion`
**Type:** Invariant
**Statement:** For all tasks `t`, at most one worker is in EXECUTING state for that task.
**Formal:** `∀ t ∈ Tasks: |{w ∈ Workers : workerState[w] = EXECUTING ∧ workerTask[w] = t}| ≤ 1`
**Expected Verdict:** PASS

#### S2: `NoDoubleExecution`
**Type:** Invariant
**Statement:** A COMMITTED task is never being executed.
**Formal:** `∀ t ∈ Tasks: taskStatus[t] = COMMITTED ⟹ ¬∃ w ∈ Workers: workerState[w] = EXECUTING ∧ workerTask[w] = t`
**Expected Verdict:** PASS

#### S3: `DLQOnlyAfterMaxFailures`
**Type:** Invariant
**Statement:** A task reaches DLQ only if it has failed at least `MaxFailures` times.
**Formal:** `∀ t ∈ Tasks: taskStatus[t] = DLQ ⟹ taskFailCount[t] ≥ MaxFailures`
**Expected Verdict:** PASS

#### S4: `LockConsistency`
**Type:** Invariant
**Statement:** If a lock is held by worker w and w is alive, then w considers itself working on that task.
**Formal:** `∀ t ∈ Tasks, w ∈ Workers: lockState[t] = LOCKED(w, v) ∧ workerAlive[w] ⟹ workerTask[w] = t`
**Expected Verdict:** PASS

#### S5: `NoOrphanExecution`
**Type:** Invariant (conditional)
**Statement:** A lock held by a dead worker has no other worker executing that task. The dead worker's own `EXECUTING` state is residual — it persists until `SessionExpiry` cleans it up.
**Formal:** `∀ t ∈ Tasks, w ∈ Workers: lockState[t] = LOCKED(w, v) ∧ ¬workerAlive[w] ⟹ ¬∃ w' ∈ Workers: w' ≠ w ∧ workerState[w'] = EXECUTING ∧ workerTask[w'] = t`
**Expected Verdict:** PASS

### Liveness Properties

#### L1: `TaskCompletion`
**Type:** Temporal (leads-to)
**Statement:** Every INIT task eventually reaches COMMITTED or DLQ.
**Formal:** `∀ t ∈ Tasks: (taskStatus[t] = INIT) ~> (taskStatus[t] ∈ {COMMITTED, DLQ})`
**Expected Verdict:** PASS (under fairness assumptions below)

#### L2: `CrashRecovery`
**Type:** Temporal (leads-to)
**Statement:** If a worker crashes while holding a lock, the lock is eventually released.
**Formal:** `∀ w ∈ Workers, t ∈ Tasks: (lockState[t] = LOCKED(w, _) ∧ ¬workerAlive[w]) ~> (lockState[t] = UNLOCKED)`
**Expected Verdict:** PASS (under weak fairness on SessionExpiry)

#### L3: `NoStarvation`
**Type:** Temporal (leads-to)
**Statement:** Every INIT task is eventually claimed by some worker.
**Formal:** `∀ t ∈ Tasks: (taskStatus[t] = INIT) ~> (∃ w ∈ Workers: workerTask[w] = t ∧ workerState[w] = EXECUTING)`
**Expected Verdict:** PASS (under weak fairness on ScanTasks and TryLockTask)

## 6. Fairness Assumptions

| Action | Fairness | Rationale |
|--------|----------|-----------|
| `ScanTasks(w, t)` | Strong fairness | Crash/recover cycles (WorkerCrash has no fairness) toggle enablement; SF ensures workers poll when alive periodically |
| `TryLockTask(w)` — Success | Strong fairness | Lock contention and crash/recover cycles toggle enablement; SF ensures eventual acquisition |
| `TryLockTask(w)` — Failure | Weak fairness | After crash+recover, worker returns to IDLE (not SCANNING); no lasso keeps it stuck |
| `ExecuteTask(w)` — Success | Strong fairness | Crash/recover cycles toggle enablement; SF ensures eventual task execution |
| `ExecuteTask(w)` — Failure | Strong fairness | Same as success — crash/recover cycles toggle enablement |
| `UnlockTask(w)` | Weak fairness | Worker is alive and in UNLOCKING — no external event toggles enablement |
| `WorkerCrash(w)` | None | Crashes are not guaranteed to happen |
| `SessionExpiry(w)` | Weak fairness | Coordination store sessions have bounded timeout; expiry is guaranteed for dead workers |
| `WorkerRecover(w)` | Weak fairness | We assume crashed workers eventually restart |

## 7. Model Checking Parameters

| Parameter | Value | Rationale |
|-----------|-------|-----------|
| `Workers` | `{w1, w2, w3}` | Three workers: enough for contention and crash scenarios |
| `Tasks` | `{t1, t2}` | Two tasks: enough for concurrent claiming and starvation checks |
| `MaxFailures` | `2` | Small enough for bounded checking, large enough to test failure counting |

**Expected state space:** ~10^4 states (small model, fast checking).

## 8. Modeling Simplifications

The following intentional simplifications exist between this spec and implementations:

- **PREPARED state collapsed**: Implementations may include a `PREPARED` state between `INIT` and `COMMITTED`. In the modeled code path, tasks transition directly from `INIT` to `COMMITTED` on success, so `PREPARED` is collapsed out of the model. The `PREPARED` constant is retained in TLA+ `TaskStatuses` for completeness but is unreachable.
- **DLQ via failure count vs time-based quarantine**: Implementations may use a time-based quarantine mechanism (e.g., `banDuration`) to prevent retrying failed tasks too quickly. The model abstracts this as a simple failure counter with a `MaxFailures` threshold. Under fairness assumptions, both achieve the same effect: a task that fails too many times is permanently sidelined.

## 9. Coordination Store Requirements

This protocol requires the following primitives from Layer 0 (`0-coordination-delegated-pattern.md`):

| Primitive | Usage |
|-----------|-------|
| `ConditionalCreate` | Lock acquisition (create lock key only if it doesn't exist) |
| `EphemeralRecord` | Lock records tied to worker sessions (auto-deleted on crash) |
| `ConditionalDelete` | Safe lock release (delete only if version matches) |
| `Get` | Reading task status and lock state |

## 10. Reference Implementation

This repository does not currently ship a reference implementation of the Task Claiming Protocol. The spec above is self-contained — it includes state variables, actions, safety/liveness properties, and the required coordination primitives — and is sufficient to drive an implementation on any coordination store that supports `ConditionalCreate`, `EphemeralRecord`, and `ConditionalDelete`.

See the sibling [S3-Queue example](examples/s3-queue/) for how the Leaderless Log Protocol (Layer 1) is implemented in Rust on S3-compatible object storage using the same coordination-delegation patterns. Contributions adding a reference implementation for Task Claiming are welcome.

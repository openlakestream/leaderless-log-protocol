--------------------------- MODULE TaskClaiming ---------------------------
(***************************************************************************)
(* Coordination-Delegated Task Claiming Protocol — TLA+ Implementation     *)
(*                                                                         *)
(* Canonical spec: ../2-coordination-delegated-task-claiming.md           *)
(*                                                                         *)
(* Models multiple workers independently claiming, executing, and          *)
(* releasing tasks using coordination-store-backed ephemeral locks.        *)
(***************************************************************************)

EXTENDS Integers, Sequences, FiniteSets, TLC

(***************************************************************************)
(* Constants                                                               *)
(***************************************************************************)
CONSTANTS
    Workers,       \* Set of worker process IDs, e.g., {w1, w2, w3}
    Tasks,         \* Set of tasks, e.g., {t1, t2}
    MaxFailures    \* Max failures before task goes to DLQ

ASSUME MaxFailures >= 1

(***************************************************************************)
(* Type definitions                                                        *)
(***************************************************************************)
TaskStatuses == {"INIT", "PREPARED", "COMMITTED", "DLQ"}
WorkerStates == {"IDLE", "SCANNING", "EXECUTING", "UNLOCKING"}

\* Lock state: either "UNLOCKED" or a record [worker |-> w, version |-> v]
UNLOCKED == [worker |-> "NONE", version |-> 0]

(***************************************************************************)
(* Variables                                                               *)
(***************************************************************************)
VARIABLES
    taskStatus,      \* [Tasks -> TaskStatuses]
    lockState,       \* [Tasks -> [worker: Workers \cup {"NONE"}, version: Nat]]
    workerAlive,     \* [Workers -> BOOLEAN]
    workerState,     \* [Workers -> WorkerStates]
    workerTask,      \* [Workers -> Tasks \cup {"NONE"}]
    taskFailCount,   \* [Tasks -> Nat]
    lockVersion      \* Global version counter for lock operations

vars == <<taskStatus, lockState, workerAlive, workerState,
          workerTask, taskFailCount, lockVersion>>

(***************************************************************************)
(* Initial state                                                           *)
(***************************************************************************)
Init ==
    /\ taskStatus = [t \in Tasks |-> "INIT"]
    /\ lockState = [t \in Tasks |-> UNLOCKED]
    /\ workerAlive = [w \in Workers |-> TRUE]
    /\ workerState = [w \in Workers |-> "IDLE"]
    /\ workerTask = [w \in Workers |-> "NONE"]
    /\ taskFailCount = [t \in Tasks |-> 0]
    /\ lockVersion = 1

(***************************************************************************)
(* Helper: Is a lock held?                                                *)
(***************************************************************************)
IsLocked(t) == lockState[t].worker # "NONE"
IsLockedBy(t, w) == lockState[t].worker = w

(***************************************************************************)
(* Action 1: ScanTasks(w, t)                                              *)
(* Worker picks an INIT task non-deterministically.                        *)
(***************************************************************************)
ScanTasks(w, t) ==
    /\ workerAlive[w] = TRUE
    /\ workerState[w] = "IDLE"
    /\ taskStatus[t] = "INIT"
    /\ workerState' = [workerState EXCEPT ![w] = "SCANNING"]
    /\ workerTask' = [workerTask EXCEPT ![w] = t]
    /\ UNCHANGED <<taskStatus, lockState, workerAlive, taskFailCount, lockVersion>>

(***************************************************************************)
(* Action 2: TryLockTask(w) — success                                    *)
(* CAS: lock is UNLOCKED, worker acquires it.                              *)
(* Models ConditionalCreate with ephemeral record semantics.              *)
(***************************************************************************)
TryLockSuccess(w) ==
    /\ workerAlive[w] = TRUE
    /\ workerState[w] = "SCANNING"
    /\ LET t == workerTask[w]
       IN
       /\ ~IsLocked(t)
       /\ lockState' = [lockState EXCEPT ![t] =
              [worker |-> w, version |-> lockVersion]]
       /\ lockVersion' = lockVersion + 1
       /\ workerState' = [workerState EXCEPT ![w] = "EXECUTING"]
    /\ UNCHANGED <<taskStatus, workerAlive, workerTask, taskFailCount>>

(***************************************************************************)
(* Action 2: TryLockTask(w) — failure                                     *)
(* Lock already held by another worker. Worker backs off.                  *)
(***************************************************************************)
TryLockFail(w) ==
    /\ workerAlive[w] = TRUE
    /\ workerState[w] = "SCANNING"
    /\ LET t == workerTask[w]
       IN
       /\ IsLocked(t)
       /\ ~IsLockedBy(t, w)
    /\ workerState' = [workerState EXCEPT ![w] = "IDLE"]
    /\ workerTask' = [workerTask EXCEPT ![w] = "NONE"]
    /\ UNCHANGED <<taskStatus, lockState, workerAlive, taskFailCount, lockVersion>>

(***************************************************************************)
(* Action 3: ExecuteTask(w) — success                                     *)
(* Task execution succeeds, task advances to COMMITTED.                    *)
(***************************************************************************)
ExecuteTaskSuccess(w) ==
    /\ workerAlive[w] = TRUE
    /\ workerState[w] = "EXECUTING"
    /\ LET t == workerTask[w]
       IN
       /\ taskStatus[t] = "INIT"
       /\ taskStatus' = [taskStatus EXCEPT ![t] = "COMMITTED"]
    /\ workerState' = [workerState EXCEPT ![w] = "UNLOCKING"]
    /\ UNCHANGED <<lockState, workerAlive, workerTask, taskFailCount, lockVersion>>

(***************************************************************************)
(* Action 3: ExecuteTask(w) — failure                                     *)
(* Task execution fails, increment failure count. DLQ if max reached.      *)
(***************************************************************************)
ExecuteTaskFailure(w) ==
    /\ workerAlive[w] = TRUE
    /\ workerState[w] = "EXECUTING"
    /\ LET t == workerTask[w]
       IN
       /\ taskFailCount' = [taskFailCount EXCEPT ![t] = taskFailCount[t] + 1]
       /\ taskStatus' = [taskStatus EXCEPT ![t] =
              IF taskFailCount[t] + 1 >= MaxFailures
              THEN "DLQ"
              ELSE taskStatus[t]]
    /\ workerState' = [workerState EXCEPT ![w] = "UNLOCKING"]
    /\ UNCHANGED <<lockState, workerAlive, workerTask, lockVersion>>

(***************************************************************************)
(* Action 4: UnlockTask(w)                                                *)
(* Release lock and return to IDLE.                                        *)
(* Uses ConditionalDelete to safely release the lock.                     *)
(***************************************************************************)
UnlockTask(w) ==
    /\ workerAlive[w] = TRUE
    /\ workerState[w] = "UNLOCKING"
    /\ LET t == workerTask[w]
       IN
       \* Only unlock if we still hold the lock (session may have expired)
       /\ lockState' = [lockState EXCEPT ![t] =
              IF IsLockedBy(t, w) THEN UNLOCKED ELSE lockState[t]]
    /\ workerState' = [workerState EXCEPT ![w] = "IDLE"]
    /\ workerTask' = [workerTask EXCEPT ![w] = "NONE"]
    /\ UNCHANGED <<taskStatus, workerAlive, taskFailCount, lockVersion>>

(***************************************************************************)
(* Action 5: WorkerCrash(w)                                               *)
(* Worker process crashes. Stops all activity.                             *)
(***************************************************************************)
WorkerCrash(w) ==
    /\ workerAlive[w] = TRUE
    /\ workerAlive' = [workerAlive EXCEPT ![w] = FALSE]
    /\ UNCHANGED <<taskStatus, lockState, workerState, workerTask,
                   taskFailCount, lockVersion>>

(***************************************************************************)
(* Action 6: SessionExpiry(w)                                             *)
(* Dead worker's session expires, releasing all ephemeral locks.           *)
(* Models coordination store ephemeral record auto-deletion.              *)
(***************************************************************************)
SessionExpiry(w) ==
    /\ workerAlive[w] = FALSE
    /\ \E t \in Tasks : IsLockedBy(t, w)  \* At least one lock to release
    /\ lockState' = [t \in Tasks |->
           IF IsLockedBy(t, w) THEN UNLOCKED ELSE lockState[t]]
    /\ workerState' = [workerState EXCEPT ![w] = "IDLE"]
    /\ workerTask' = [workerTask EXCEPT ![w] = "NONE"]
    /\ UNCHANGED <<taskStatus, workerAlive, taskFailCount, lockVersion>>

(***************************************************************************)
(* Action 7: WorkerRecover(w)                                             *)
(* Dead worker restarts with fresh state.                                  *)
(* Guard: all ephemeral locks held by w must have expired first.           *)
(* In the real system, a restarting process gets a new session; the old    *)
(* session's ephemeral records are cleaned up independently by the         *)
(* coordination store (modeled by SessionExpiry).                          *)
(***************************************************************************)
WorkerRecover(w) ==
    /\ workerAlive[w] = FALSE
    /\ \A t \in Tasks : ~IsLockedBy(t, w)  \* Session must have expired first
    /\ workerAlive' = [workerAlive EXCEPT ![w] = TRUE]
    /\ workerState' = [workerState EXCEPT ![w] = "IDLE"]
    /\ workerTask' = [workerTask EXCEPT ![w] = "NONE"]
    /\ UNCHANGED <<taskStatus, lockState, taskFailCount, lockVersion>>

(***************************************************************************)
(* Next-state relation                                                     *)
(***************************************************************************)
Next ==
    \/ \E w \in Workers, t \in Tasks : ScanTasks(w, t)
    \/ \E w \in Workers : TryLockSuccess(w)
    \/ \E w \in Workers : TryLockFail(w)
    \/ \E w \in Workers : ExecuteTaskSuccess(w)
    \/ \E w \in Workers : ExecuteTaskFailure(w)
    \/ \E w \in Workers : UnlockTask(w)
    \/ \E w \in Workers : WorkerCrash(w)
    \/ \E w \in Workers : SessionExpiry(w)
    \/ \E w \in Workers : WorkerRecover(w)

(***************************************************************************)
(* Fairness                                                                *)
(***************************************************************************)
Fairness ==
    /\ \A w \in Workers, t \in Tasks : WF_vars(ScanTasks(w, t))
    \* SF for TryLockSuccess: if the lock becomes free infinitely often,
    \* w eventually acquires it. WF is insufficient because competing
    \* workers can repeatedly disable the action between steps.
    /\ \A w \in Workers : SF_vars(TryLockSuccess(w))
    /\ \A w \in Workers : WF_vars(TryLockFail(w))
    \* SF for ExecuteTaskSuccess: if enabled infinitely often, it eventually
    \* fires. WF is insufficient because crash-recover cycles can repeatedly
    \* disable it (crash disables, recover re-enables) — WF is satisfied
    \* vacuously, allowing infinite crash loops without progress.
    /\ \A w \in Workers : SF_vars(ExecuteTaskSuccess(w))
    /\ \A w \in Workers : WF_vars(ExecuteTaskFailure(w))
    /\ \A w \in Workers : WF_vars(UnlockTask(w))
    /\ \A w \in Workers : WF_vars(SessionExpiry(w))
    /\ \A w \in Workers : WF_vars(WorkerRecover(w))

Spec == Init /\ [][Next]_vars /\ Fairness

(***************************************************************************)
(* Safety Properties                                                       *)
(***************************************************************************)

\* S1: Mutual exclusion — at most one worker executes a given task
MutualExclusion ==
    \A t \in Tasks :
        Cardinality({w \in Workers :
            workerState[w] = "EXECUTING" /\ workerTask[w] = t}) <= 1

\* S2: A COMMITTED task is never being executed
NoDoubleExecution ==
    \A t \in Tasks :
        taskStatus[t] = "COMMITTED" =>
            ~\E w \in Workers :
                workerState[w] = "EXECUTING" /\ workerTask[w] = t

\* S3: DLQ only after max failures
DLQOnlyAfterMaxFailures ==
    \A t \in Tasks :
        taskStatus[t] = "DLQ" => taskFailCount[t] >= MaxFailures

\* S4: Lock consistency — if lock held by alive worker, worker has that task
LockConsistency ==
    \A t \in Tasks, w \in Workers :
        (IsLockedBy(t, w) /\ workerAlive[w]) =>
            workerTask[w] = t

\* S5: No alive worker executes a task locked by a dead worker
NoOrphanExecution ==
    \A t \in Tasks, w \in Workers :
        (IsLockedBy(t, w) /\ ~workerAlive[w]) =>
            ~\E w2 \in Workers :
                workerState[w2] = "EXECUTING" /\ workerTask[w2] = t

(***************************************************************************)
(* Liveness Properties                                                     *)
(***************************************************************************)

\* L1: Every INIT task eventually reaches COMMITTED or DLQ
TaskCompletion ==
    \A t \in Tasks :
        taskStatus[t] = "INIT" ~>
            (taskStatus[t] = "COMMITTED" \/ taskStatus[t] = "DLQ")

\* L2: Crashed worker's lock is eventually released
CrashRecovery ==
    \A w \in Workers, t \in Tasks :
        (IsLockedBy(t, w) /\ ~workerAlive[w]) ~>
            ~IsLockedBy(t, w)

\* L3: Every INIT task is eventually claimed
NoStarvation ==
    \A t \in Tasks :
        taskStatus[t] = "INIT" ~>
            \E w \in Workers :
                workerState[w] = "EXECUTING" /\ workerTask[w] = t

(***************************************************************************)
(* Type invariant (for debugging)                                          *)
(***************************************************************************)
TypeOK ==
    /\ \A t \in Tasks : taskStatus[t] \in TaskStatuses
    /\ \A t \in Tasks : lockState[t].worker \in Workers \cup {"NONE"}
    /\ \A t \in Tasks : lockState[t].version \in Nat
    /\ \A w \in Workers : workerAlive[w] \in BOOLEAN
    /\ \A w \in Workers : workerState[w] \in WorkerStates
    /\ \A w \in Workers : workerTask[w] \in Tasks \cup {"NONE"}
    /\ \A t \in Tasks : taskFailCount[t] \in 0..MaxFailures
    \* Bounded for TLC — Nat is infinite but reachable values are finite.
    \* Upper bound: each TryLockSuccess increments once; at most
    \* |Workers| * |Tasks| * (MaxFailures+1) lock acquisitions are reachable.
    /\ lockVersion \in 1..(1 + Cardinality(Workers) * Cardinality(Tasks) * (MaxFailures + 2))

=============================================================================

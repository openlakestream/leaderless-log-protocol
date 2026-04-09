# Protocol Modeling Patterns

Key modeling decisions and skeleton code for common distributed protocols.

## 1. Raft

### Key Modeling Decisions

- **Processes**: Server (each can be follower, candidate, or leader)
- **State per server**: `currentTerm`, `votedFor`, `state`, `log`, `commitIndex`, `matchIndex` (leader only)
- **Messages**: RequestVote, RequestVoteResponse, AppendEntries, AppendEntriesResponse
- **Network**: Set of messages (Raft handles duplicates and reordering)

### Key Variables

```tla
VARIABLES
    currentTerm,    \* [Server -> Nat] latest term server has seen
    state,          \* [Server -> {"follower", "candidate", "leader"}]
    votedFor,       \* [Server -> Server \union {Nil}] who this server voted for
    log,            \* [Server -> Seq([term: Nat, value: Value])] log entries
    commitIndex,    \* [Server -> Nat] highest committed index
    messages        \* set of in-flight messages
```

### Key Actions (TLA+ Skeleton)

```tla
\* Server s starts an election
RequestVote(s) ==
    /\ state[s] \in {"follower", "candidate"}
    /\ currentTerm' = [currentTerm EXCEPT ![s] = currentTerm[s] + 1]
    /\ state' = [state EXCEPT ![s] = "candidate"]
    /\ votedFor' = [votedFor EXCEPT ![s] = s]
    /\ messages' = messages \union
        {[type |-> "RequestVote", term |-> currentTerm[s] + 1,
          candidate |-> s, lastLogTerm |-> LastTerm(log[s]),
          lastLogIndex |-> Len(log[s]),
          dest |-> dest] : dest \in Server \ {s}}
    /\ UNCHANGED <<log, commitIndex>>

\* Server s becomes leader after receiving majority votes
BecomeLeader(s) ==
    /\ state[s] = "candidate"
    /\ \E Q \in Quorum :
        /\ s \in Q
        /\ \A v \in Q : \E m \in messages :
            /\ m.type = "RequestVoteResponse"
            /\ m.term = currentTerm[s]
            /\ m.voter = v
            /\ m.granted = TRUE
    /\ state' = [state EXCEPT ![s] = "leader"]
    /\ UNCHANGED <<currentTerm, votedFor, log, commitIndex, messages>>

\* Leader s appends an entry and sends AppendEntries
AppendEntries(s) ==
    /\ state[s] = "leader"
    /\ \E v \in Value :
        /\ log' = [log EXCEPT ![s] = Append(log[s], [term |-> currentTerm[s], value |-> v])]
    /\ messages' = messages \union
        {[type |-> "AppendEntries", term |-> currentTerm[s],
          leader |-> s, entries |-> <<log[s][Len(log[s])]>>,
          prevLogIndex |-> Len(log[s]) - 1,
          prevLogTerm |-> IF Len(log[s]) > 1 THEN log[s][Len(log[s])-1].term ELSE 0,
          leaderCommit |-> commitIndex[s]] : dest \in Server \ {s}}
    /\ UNCHANGED <<currentTerm, state, votedFor, commitIndex>>
```

### Key Invariants

```tla
\* Election Safety: at most one leader per term
ElectionSafety ==
    \A s1, s2 \in Server :
        (state[s1] = "leader" /\ state[s2] = "leader")
        => currentTerm[s1] # currentTerm[s2]

\* Log Matching: if two logs contain an entry with the same index and term,
\* then the logs are identical through that index
LogMatching ==
    \A s1, s2 \in Server : \A i \in 1..Min(Len(log[s1]), Len(log[s2])) :
        log[s1][i].term = log[s2][i].term =>
            SubSeq(log[s1], 1, i) = SubSeq(log[s2], 1, i)

\* Leader Completeness: if an entry is committed, it appears in all future leaders' logs
\* (typically modeled with an auxiliary history variable)
```

---

## 2. Paxos (Single-Decree)

### Key Modeling Decisions

- **Roles**: Proposers, Acceptors (often combined on same nodes)
- **State per acceptor**: `maxBal` (highest ballot promised), `maxVBal` (highest ballot accepted), `maxVal` (value accepted at maxVBal)
- **Messages**: Prepare, Promise, Accept, Accepted
- **Key insight**: Ballot numbers totally order proposals

### Key Variables

```tla
VARIABLES
    maxBal,     \* [Acceptor -> Nat \union {-1}] highest ballot promised
    maxVBal,    \* [Acceptor -> Nat \union {-1}] highest ballot accepted
    maxVal,     \* [Acceptor -> Value \union {Nil}] value at maxVBal
    messages    \* set of in-flight messages
```

### Skeleton (TLA+)

```tla
\* Phase 1a: Proposer sends Prepare with ballot b
Prepare(b) ==
    /\ messages' = messages \union {[type |-> "prepare", bal |-> b]}
    /\ UNCHANGED <<maxBal, maxVBal, maxVal>>

\* Phase 1b: Acceptor a responds to Prepare
Promise(a) ==
    /\ \E m \in messages :
        /\ m.type = "prepare"
        /\ m.bal > maxBal[a]
        /\ maxBal' = [maxBal EXCEPT ![a] = m.bal]
        /\ messages' = messages \union
            {[type |-> "promise", bal |-> m.bal, acc |-> a,
              vbal |-> maxVBal[a], vval |-> maxVal[a]]}
    /\ UNCHANGED <<maxVBal, maxVal>>

\* Phase 2a: Proposer sends Accept after receiving majority promises
Accept(b) ==
    /\ \E Q \in Quorum :
        LET promises == {m \in messages : m.type = "promise" /\ m.bal = b /\ m.acc \in Q}
            maxPromise == CHOOSE m \in promises :
                \A m2 \in promises : m.vbal >= m2.vbal
            v == IF maxPromise.vbal = -1
                 THEN CHOOSE val \in Value : TRUE  \* free to choose
                 ELSE maxPromise.vval              \* must use highest accepted
        IN /\ Cardinality(promises) = Cardinality(Q)
           /\ messages' = messages \union
               {[type |-> "accept", bal |-> b, val |-> v]}
    /\ UNCHANGED <<maxBal, maxVBal, maxVal>>

\* Phase 2b: Acceptor a accepts the proposal
Accepted(a) ==
    /\ \E m \in messages :
        /\ m.type = "accept"
        /\ m.bal >= maxBal[a]
        /\ maxBal' = [maxBal EXCEPT ![a] = m.bal]
        /\ maxVBal' = [maxVBal EXCEPT ![a] = m.bal]
        /\ maxVal' = [maxVal EXCEPT ![a] = m.val]
        /\ messages' = messages \union
            {[type |-> "accepted", bal |-> m.bal, val |-> m.val, acc |-> a]}
    /\ UNCHANGED <<>>
```

### Key Invariant

```tla
\* Agreement: at most one value is chosen
\* A value v is chosen if a majority of acceptors have accepted it at the same ballot
Chosen(v) ==
    \E b \in Ballot : \E Q \in Quorum :
        \A a \in Q : maxVBal[a] = b /\ maxVal[a] = v

Agreement ==
    \A v1, v2 \in Value : Chosen(v1) /\ Chosen(v2) => v1 = v2
```

---

## 3. Multi-Paxos / Zab (ZooKeeper Atomic Broadcast)

### Key Modeling Decisions

- **Leader-based**: A distinguished leader sequences all proposals
- **Epochs/Views**: Leader changes increment epoch number
- **Transaction ordering**: All transactions within an epoch are ordered by the leader
- **Key difference from Paxos**: Leader is long-lived, amortizing election cost

### Skeleton (PlusCal)

```
variables
    epoch = [s \in Server |-> 0],
    history = [s \in Server |-> <<>>],  \* ordered transaction log
    leader = "none",
    messages = {};

process server \in Server
begin
    Main:
        while TRUE do
            either
                \* Phase 1: Leader Discovery
                await leader = "none";
                \* Nondeterministically become leader candidate
                epoch[self] := epoch[self] + 1;
                messages := messages \union
                    {[type |-> "NEWEPOCH", epoch |-> epoch[self], sender |-> self]};
            or
                \* Phase 2: Synchronization (NEWLEADER)
                \* Leader sends its history to followers
                await leader = self;
                messages := messages \union
                    {[type |-> "NEWLEADER", epoch |-> epoch[self],
                      history |-> history[self], sender |-> self]};
            or
                \* Phase 3: Broadcast (normal operation)
                \* Leader proposes a transaction
                await leader = self;
                with v \in Value do
                    history[self] := Append(history[self],
                        [epoch |-> epoch[self], value |-> v]);
                    messages := messages \union
                        {[type |-> "PROPOSE", epoch |-> epoch[self],
                          txn |-> [epoch |-> epoch[self], value |-> v],
                          sender |-> self]};
                end with;
            or
                \* Follower accepts proposal
                with msg \in {m \in messages : m.type = "PROPOSE" /\ m.epoch = epoch[self]} do
                    history[self] := Append(history[self], msg.txn);
                    messages := messages \union
                        {[type |-> "ACK", epoch |-> epoch[self],
                          sender |-> self, dest |-> msg.sender]};
                end with;
            end either;
        end while;
end process;
```

### Key Invariants

```tla
\* Prefix consistency: committed histories are prefixes of each other
PrefixConsistency ==
    \A s1, s2 \in Server :
        LET len == Min(Len(committed[s1]), Len(committed[s2]))
        IN SubSeq(committed[s1], 1, len) = SubSeq(committed[s2], 1, len)

\* Total order: all servers see transactions in the same order
TotalOrder ==
    \A s1, s2 \in Server : \A i \in 1..Min(Len(history[s1]), Len(history[s2])) :
        history[s1][i] = history[s2][i]
```

---

## 4. Two-Phase Commit (2PC)

### Key Modeling Decisions

- **Roles**: Coordinator (single), Participants (set)
- **Phases**: Prepare (voting), Commit/Abort (decision)
- **Blocking**: 2PC blocks if coordinator crashes after sending prepare
- **Network**: Messages can be lost (reveals blocking problem)

### Skeleton (PlusCal)

```
variables
    coordState = "init",     \* "init", "preparing", "committed", "aborted"
    partState = [p \in Participant |-> "working"],  \* "working", "prepared", "committed", "aborted"
    messages = {};

process coordinator = "coord"
begin
    Prepare:
        coordState := "preparing";
        messages := messages \union
            {[type |-> "prepare", dest |-> p] : p \in Participant};
    Decide:
        either
            \* All voted yes
            await \A p \in Participant :
                [type |-> "vote_yes", sender |-> p] \in messages;
            coordState := "committed";
            messages := messages \union
                {[type |-> "commit", dest |-> p] : p \in Participant};
        or
            \* At least one voted no (or timeout)
            await \E p \in Participant :
                [type |-> "vote_no", sender |-> p] \in messages;
            coordState := "aborted";
            messages := messages \union
                {[type |-> "abort", dest |-> p] : p \in Participant};
        end either;
end process;

process participant \in Participant
begin
    Vote:
        await [type |-> "prepare", dest |-> self] \in messages;
        either
            partState[self] := "prepared";
            messages := messages \union
                {[type |-> "vote_yes", sender |-> self]};
        or
            partState[self] := "aborted";
            messages := messages \union
                {[type |-> "vote_no", sender |-> self]};
        end either;
    WaitDecision:
        either
            await [type |-> "commit", dest |-> self] \in messages;
            partState[self] := "committed";
        or
            await [type |-> "abort", dest |-> self] \in messages;
            partState[self] := "aborted";
        end either;
end process;
```

### Key Invariants

```tla
\* Agreement: no two participants decide differently
Agreement ==
    \A p1, p2 \in Participant :
        ~ (partState[p1] = "committed" /\ partState[p2] = "aborted")

\* Validity: if all participants vote yes, coordinator must commit
\* (liveness, needs fairness)

\* Abort validity: if coordinator aborts, at least one participant voted no
\* (or coordinator timed out)
```

---

## 5. Three-Phase Commit (3PC)

### Key Modeling Decisions

- Adds **pre-commit** phase between prepare and commit
- Non-blocking under crash-stop failures (no network partitions)
- Partition tolerance: 3PC can still block under network partitions

### Additional Phase (PlusCal Fragment)

```
\* After receiving all yes votes:
PreCommit:
    coordState := "pre-committed";
    messages := messages \union
        {[type |-> "pre_commit", dest |-> p] : p \in Participant};

WaitPreCommitAck:
    await \A p \in Participant :
        [type |-> "pre_commit_ack", sender |-> p] \in messages;

FinalCommit:
    coordState := "committed";
    messages := messages \union
        {[type |-> "commit", dest |-> p] : p \in Participant};
```

The key insight: if the coordinator crashes after pre-commit, a recovery coordinator can safely commit because all participants have acknowledged pre-commit.

---

## 6. PBFT (Practical Byzantine Fault Tolerance)

### Key Modeling Decisions

- **Fault model**: Byzantine (up to f faults, requires 3f+1 replicas)
- **Quorum**: 2f+1 (ensures overlap despite f Byzantine nodes)
- **Phases**: Pre-prepare (leader), Prepare (all), Commit (all)
- **View changes**: Replace faulty leader

### Key Variables

```tla
VARIABLES
    view,           \* [Replica -> Nat] current view number
    prepareLog,     \* [Replica -> set of prepare messages]
    commitLog,      \* [Replica -> set of commit messages]
    executed,       \* [Replica -> Seq(Value)] executed operations
    messages        \* set of all messages
```

### Skeleton (TLA+ Fragment)

```tla
\* Leader of view v
Leader(v) == CHOOSE r \in Replica : r = (v % Cardinality(Replica))

\* Pre-prepare: leader assigns sequence number to request
PrePrepare(r, v, n, req) ==
    /\ r = Leader(v)
    /\ messages' = messages \union
        {[type |-> "pre-prepare", view |-> v, seq |-> n,
          digest |-> req, sender |-> r]}
    /\ UNCHANGED <<view, prepareLog, commitLog, executed>>

\* Prepare: replica sends prepare after receiving valid pre-prepare
PrepareMsg(r, v, n, d) ==
    /\ \E m \in messages : m.type = "pre-prepare" /\ m.view = v
        /\ m.seq = n /\ m.digest = d
    /\ messages' = messages \union
        {[type |-> "prepare", view |-> v, seq |-> n,
          digest |-> d, sender |-> r]}

\* Prepared predicate: 2f+1 matching prepares received
Prepared(r, v, n, d) ==
    Cardinality({m \in messages : m.type = "prepare" /\ m.view = v
        /\ m.seq = n /\ m.digest = d}) >= 2 * F + 1
```

### Key Invariant

```tla
\* Safety: no two correct replicas commit different values for same sequence number
Agreement ==
    \A r1, r2 \in CorrectReplica : \A n \in SeqNum :
        (committed[r1][n] # Nil /\ committed[r2][n] # Nil)
        => committed[r1][n] = committed[r2][n]
```

---

## 7. Chain Replication

### Key Modeling Decisions

- **Topology**: Linear chain of nodes (head -> ... -> tail)
- **Updates**: Enter at head, propagate to tail
- **Reads**: Served by tail only (strong consistency)
- **Failure handling**: Chain reconfiguration on node failure

### Skeleton (TLA+ Fragment)

```tla
VARIABLES
    chain,      \* sequence of server IDs (head first, tail last)
    store,      \* [Server -> [Key -> Value]] local store per server
    pending,    \* [Server -> set of pending updates]
    messages    \* in-flight messages (FIFO per adjacent pair)

Head == Head(chain)
Tail == Last(chain)
Successor(s) ==
    LET i == CHOOSE j \in 1..Len(chain) : chain[j] = s
    IN chain[i + 1]

\* Client sends update to head
ClientUpdate(key, val) ==
    /\ messages' = messages \union
        {[type |-> "update", key |-> key, val |-> val, dest |-> Head]}
    /\ UNCHANGED <<chain, store, pending>>

\* Server processes update and forwards to successor
ProcessUpdate(s) ==
    /\ \E m \in messages : m.type = "update" /\ m.dest = s
    /\ store' = [store EXCEPT ![s][m.key] = m.val]
    /\ IF s = Tail
       THEN \* Tail sends ACK back up the chain
            messages' = (messages \ {m}) \union
                {[type |-> "ack", key |-> m.key, val |-> m.val]}
       ELSE \* Forward to successor
            messages' = (messages \ {m}) \union
                {[m EXCEPT !.dest = Successor(s)]}
    /\ UNCHANGED <<chain, pending>>
```

### Key Invariant

```tla
\* Strong consistency: tail's store reflects all committed updates in order
\* Reads from tail always return the latest committed value
```

---

## 8. EPaxos (Egalitarian Paxos)

### Key Modeling Decisions

- **No designated leader**: Any replica can propose (egalitarian)
- **Fast path**: Commit in 1 round-trip if no conflicts
- **Slow path**: Falls back to Paxos-like protocol on conflicts
- **Dependencies**: Each command tracks dependencies on other commands

### Key Variables

```tla
VARIABLES
    cmdLog,     \* [Replica -> [Instance -> [cmd, deps, status, ballot]]]
    messages    \* set of messages

\* Instance: <<replica, sequence_number>>
\* Status: "pre-accepted", "accepted", "committed", "executed"
```

### Skeleton (TLA+ Fragment)

```tla
\* Fast path: propose with initial dependencies
PreAccept(r, cmd) ==
    LET inst == <<r, nextSeq[r]>>
        deps == {i \in AllInstances : Conflicts(cmdLog[r][i].cmd, cmd)}
    IN /\ cmdLog' = [cmdLog EXCEPT ![r][inst] =
            [cmd |-> cmd, deps |-> deps, status |-> "pre-accepted", ballot |-> 0]]
       /\ messages' = messages \union
            {[type |-> "pre-accept", inst |-> inst, cmd |-> cmd,
              deps |-> deps, sender |-> r] : dest \in Replica \ {r}}

\* If all replies agree on dependencies: fast commit
FastCommit(r, inst) ==
    LET replies == {m \in messages : m.type = "pre-accept-reply"
                    /\ m.inst = inst /\ m.deps = cmdLog[r][inst].deps}
    IN /\ Cardinality(replies) >= FastQuorumSize
       /\ cmdLog' = [cmdLog EXCEPT ![r][inst].status = "committed"]
       /\ messages' = messages \union
            {[type |-> "commit", inst |-> inst,
              cmd |-> cmdLog[r][inst].cmd,
              deps |-> cmdLog[r][inst].deps] : dest \in Replica \ {r}}
```

---

## 9. Viewstamped Replication (VR)

### Key Modeling Decisions

- **View-based**: Leader changes increment view number
- **Numbering**: op-number (sequence within view), commit-number (global commit point)
- **Normal operation**: Leader assigns op-numbers, broadcasts, commits after f+1 acks
- **View change**: New leader collects state from f+1 replicas

### Key Variables

```tla
VARIABLES
    view,           \* [Replica -> Nat] current view
    status,         \* [Replica -> {"normal", "view-change", "recovering"}]
    opNumber,       \* [Replica -> Nat] latest op-number
    commitNumber,   \* [Replica -> Nat] latest committed op
    log,            \* [Replica -> Seq(Operation)] operation log
    messages

Leader(v) == CHOOSE r \in Replica : r = (v % Cardinality(Replica))
```

### Skeleton (TLA+ Fragment)

```tla
\* Normal operation: leader receives client request
HandleRequest(r, op) ==
    /\ r = Leader(view[r])
    /\ status[r] = "normal"
    /\ opNumber' = [opNumber EXCEPT ![r] = opNumber[r] + 1]
    /\ log' = [log EXCEPT ![r] = Append(log[r], op)]
    /\ messages' = messages \union
        {[type |-> "prepare", view |-> view[r],
          opNum |-> opNumber[r] + 1, op |-> op,
          commitNum |-> commitNumber[r], sender |-> r] : dest \in Replica \ {r}}
    /\ UNCHANGED <<view, status, commitNumber>>

\* Replica prepares and acknowledges
HandlePrepare(r) ==
    /\ \E m \in messages :
        /\ m.type = "prepare"
        /\ m.view = view[r]
        /\ m.opNum = opNumber[r] + 1
        /\ opNumber' = [opNumber EXCEPT ![r] = m.opNum]
        /\ log' = [log EXCEPT ![r] = Append(log[r], m.op)]
        /\ messages' = messages \union
            {[type |-> "prepare-ok", view |-> view[r],
              opNum |-> m.opNum, sender |-> r]}
    /\ UNCHANGED <<view, status, commitNumber>>

\* Leader commits after receiving f+1 prepare-oks
Commit(r) ==
    /\ r = Leader(view[r])
    /\ \E n \in (commitNumber[r]+1)..opNumber[r] :
        LET acks == {m \in messages : m.type = "prepare-ok"
                     /\ m.view = view[r] /\ m.opNum = n}
        IN Cardinality(acks) + 1 >= (Cardinality(Replica) \div 2) + 1
           /\ commitNumber' = [commitNumber EXCEPT ![r] = n]
    /\ UNCHANGED <<view, status, opNumber, log, messages>>
```

### Key Invariants

```tla
\* Agreement on committed operations
Agreement ==
    \A r1, r2 \in Replica :
        \A i \in 1..Min(commitNumber[r1], commitNumber[r2]) :
            log[r1][i] = log[r2][i]
```

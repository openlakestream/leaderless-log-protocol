# Protocol Modeling Patterns in Fizzbee

Skeleton specifications for common distributed systems protocols. Each shows the key modeling decisions and Fizzbee code structure. These are starting points -- real specifications require additional detail based on your specific requirements.

---

## 1. Raft Consensus

### Key Modeling Decisions

- **Roles**: All nodes are the same role (Node) but transition between follower/candidate/leader states
- **Channels**: One channel per pair of nodes (or a shared channel with addressing)
- **Key insight**: The term number is the core mechanism preventing split-brain; the model must explore term transitions thoroughly

### Skeleton Specification

```python
# --- Assertions ---

always assertion NoTwoLeadersInSameTerm:
    for i in range(len(nodes)):
        for j in range(i + 1, len(nodes)):
            if nodes[i].state == "leader" and nodes[j].state == "leader":
                assert nodes[i].term != nodes[j].term

always assertion LogMatching:
    """If two logs have an entry with the same index and term,
    then all preceding entries also match."""
    for i in range(len(nodes)):
        for j in range(i + 1, len(nodes)):
            min_len = min(len(nodes[i].log), len(nodes[j].log))
            for k in range(min_len):
                if nodes[i].log[k].term == nodes[j].log[k].term:
                    assert nodes[i].log[k].value == nodes[j].log[k].value

always eventually assertion LeaderElected:
    assert any(n.state == "leader" for n in nodes)

# --- Role Definition ---

role Node:
    term = 0
    state = "follower"
    voted_for = None
    log = []
    commit_index = 0
    votes_received = set()

    def init():
        pass

    # --- Leader Election ---

    fair def HandleElectionTimeout():
        """Follower or candidate times out, starts election."""
        requires(state != "leader")
        global term, state, voted_for, votes_received
        term += 1
        state = "candidate"
        voted_for = self_id
        votes_received = {self_id}
        for peer in peers:
            chan[peer].send({
                "type": "request_vote",
                "term": term,
                "candidate_id": self_id,
                "last_log_index": len(log) - 1,
                "last_log_term": log[-1].term if log else 0,
            })

    def HandleRequestVote(msg):
        """Process incoming vote request."""
        global term, state, voted_for
        if msg["term"] > term:
            term = msg["term"]
            state = "follower"
            voted_for = None

        vote_granted = False
        if msg["term"] == term and voted_for in (None, msg["candidate_id"]):
            if is_log_up_to_date(msg):
                voted_for = msg["candidate_id"]
                vote_granted = True

        chan[msg["candidate_id"]].send({
            "type": "vote_response",
            "term": term,
            "granted": vote_granted,
        })

    atomic def HandleVoteResponse(msg):
        """Process incoming vote response."""
        global state, votes_received
        if msg["term"] == term and state == "candidate" and msg["granted"]:
            votes_received.add(msg["from"])
            if len(votes_received) > len(nodes) // 2:
                state = "leader"

    # --- Log Replication ---

    fair def SendAppendEntries():
        """Leader sends log entries to followers."""
        requires(state == "leader")
        for peer in peers:
            chan[peer].send({
                "type": "append_entries",
                "term": term,
                "leader_id": self_id,
                "entries": log[next_index[peer]:],
                "leader_commit": commit_index,
            })

    def HandleAppendEntries(msg):
        """Follower processes log entries from leader."""
        global term, state, log, commit_index
        if msg["term"] >= term:
            term = msg["term"]
            state = "follower"
            # Append new entries (simplified)
            log = log[:msg["prev_log_index"] + 1] + msg["entries"]
            if msg["leader_commit"] > commit_index:
                commit_index = min(msg["leader_commit"], len(log) - 1)

    # --- Helper Functions ---

    def is_log_up_to_date(msg):
        my_last_term = log[-1].term if log else 0
        my_last_index = len(log) - 1
        if msg["last_log_term"] != my_last_term:
            return msg["last_log_term"] > my_last_term
        return msg["last_log_index"] >= my_last_index

# --- Instantiation ---

nodes = role(Node, 3)
chan = {i: channel() for i in range(3)}
```

### Assertions to Add

- **Leader completeness**: If a log entry is committed in a given term, that entry is present in the logs of all leaders for all higher terms
- **State machine safety**: If a server has applied a log entry at a given index, no other server will ever apply a different log entry for that index
- **Commit durability**: Once committed, an entry is never lost

---

## 2. Paxos (Single-Decree)

### Key Modeling Decisions

- **Roles**: Proposer and Acceptor are distinct roles
- **Channels**: Proposers send to acceptors, acceptors respond -- two types of messages
- **Key insight**: The two-phase structure (prepare/accept) and the promise mechanism are what guarantee agreement

### Skeleton Specification

```python
# --- Assertions ---

always assertion Agreement:
    """At most one value is chosen."""
    chosen_values = set()
    for a in acceptors:
        if a.accepted_value is not None:
            # A value is chosen if accepted by a majority
            count = sum(1 for a2 in acceptors
                       if a2.accepted_value == a.accepted_value
                       and a2.accepted_proposal >= a.accepted_proposal)
            if count > len(acceptors) // 2:
                chosen_values.add(a.accepted_value)
    assert len(chosen_values) <= 1

always assertion Validity:
    """A chosen value was proposed by some proposer."""
    for a in acceptors:
        if a.accepted_value is not None:
            assert a.accepted_value in proposed_values

# --- Acceptor Role ---

role Acceptor:
    promised_proposal = 0
    accepted_proposal = 0
    accepted_value = None

    def HandlePrepare(msg):
        """Phase 1b: respond to prepare request."""
        global promised_proposal
        if msg["proposal_num"] > promised_proposal:
            promised_proposal = msg["proposal_num"]
            chan[msg["from"]].send({
                "type": "promise",
                "proposal_num": msg["proposal_num"],
                "accepted_proposal": accepted_proposal,
                "accepted_value": accepted_value,
                "from": self_id,
            })

    def HandleAccept(msg):
        """Phase 2b: respond to accept request."""
        global promised_proposal, accepted_proposal, accepted_value
        if msg["proposal_num"] >= promised_proposal:
            promised_proposal = msg["proposal_num"]
            accepted_proposal = msg["proposal_num"]
            accepted_value = msg["value"]
            chan[msg["from"]].send({
                "type": "accepted",
                "proposal_num": msg["proposal_num"],
                "from": self_id,
            })

# --- Proposer Role ---

role Proposer:
    proposal_num = 0
    value = None
    promises = []
    accepts = set()

    def Propose():
        """Phase 1a: send prepare to all acceptors."""
        global proposal_num, promises
        proposal_num = next_proposal_num()
        promises = []
        for a in acceptors:
            chan[a].send({
                "type": "prepare",
                "proposal_num": proposal_num,
                "from": self_id,
            })

    def HandlePromise(msg):
        """Collect promises, then send accept."""
        global promises, value
        if msg["proposal_num"] == proposal_num:
            promises.append(msg)
            if len(promises) > len(acceptors) // 2:
                # Phase 2a: choose value
                highest = max(promises, key=lambda p: p["accepted_proposal"])
                if highest["accepted_value"] is not None:
                    value = highest["accepted_value"]
                else:
                    value = my_proposed_value
                for a in acceptors:
                    chan[a].send({
                        "type": "accept",
                        "proposal_num": proposal_num,
                        "value": value,
                        "from": self_id,
                    })

    def HandleAccepted(msg):
        """Collect accepted messages."""
        global accepts
        if msg["proposal_num"] == proposal_num:
            accepts.add(msg["from"])

# --- Instantiation ---

acceptors = role(Acceptor, 3)
proposers = role(Proposer, 2)
proposed_values = {"A", "B"}
chan = {i: channel() for i in range(5)}  # Channels for all roles
```

---

## 3. Two-Phase Commit (2PC)

### Key Modeling Decisions

- **Roles**: Coordinator and Participant are distinct
- **Channels**: Coordinator broadcasts to all participants; participants respond to coordinator
- **Key insight**: 2PC blocks if the coordinator crashes after sending some but not all prepare messages -- the model should explore this

### Skeleton Specification

```python
# --- Assertions ---

always assertion AtomicCommit:
    """All participants decide the same way."""
    decisions = [p.decision for p in participants if p.decision is not None]
    if len(decisions) > 1:
        assert all(d == decisions[0] for d in decisions)

always assertion SafetyNoCommitAfterAbort:
    """If any participant aborted, no one commits."""
    if any(p.decision == "abort" for p in participants):
        assert all(p.decision != "commit" for p in participants)

always assertion ValidCommit:
    """Commit only if all participants voted yes."""
    if coordinator.decision == "commit":
        assert all(p.vote == "yes" for p in participants)

# --- Coordinator Role ---

role Coordinator:
    decision = None
    votes = {}

    def StartTransaction():
        """Phase 1: send prepare to all participants."""
        requires(decision is None)
        for p in participants:
            chan[p].send({"type": "prepare", "from": self_id})

    def HandleVote(msg):
        """Collect votes from participants."""
        global votes, decision
        votes[msg["from"]] = msg["vote"]
        if len(votes) == len(participants):
            if all(v == "yes" for v in votes.values()):
                decision = "commit"
            else:
                decision = "abort"
            # Phase 2: send decision
            for p in participants:
                chan[p].send({"type": "decision", "decision": decision})

    # Model coordinator crash
    def Crash():
        """Coordinator crashes -- participants may block."""
        global decision
        oneof:
            pass  # Crash before sending any decision
        or:
            # Crash after partial send
            partial = any p in participants
            chan[partial].send({"type": "decision", "decision": decision})

# --- Participant Role ---

role Participant:
    vote = None
    decision = None

    def HandlePrepare(msg):
        """Vote yes or no."""
        global vote
        oneof:
            vote = "yes"
            chan[msg["from"]].send({"type": "vote", "vote": "yes", "from": self_id})
        or:
            vote = "no"
            chan[msg["from"]].send({"type": "vote", "vote": "no", "from": self_id})

    def HandleDecision(msg):
        """Apply the coordinator's decision."""
        global decision
        decision = msg["decision"]

# --- Instantiation ---

coordinator = role(Coordinator, 1)
participants = role(Participant, 3)
chan = {i: channel() for i in range(4)}
```

### Exploring 2PC Blocking

To demonstrate that 2PC blocks under coordinator failure, use lossy channels for the coordinator's outbound messages:

```python
# Coordinator can crash, and its decision messages may be lost
coordinator_chan = channel(lossy=True)
```

Then add a liveness assertion:

```python
# This will FAIL, demonstrating the blocking problem
always eventually assertion AllDecide:
    assert all(p.decision is not None for p in participants)
```

This counterexample demonstrates exactly why 2PC has the blocking problem and motivates 3PC or Paxos-based commit.

---

## 4. Leader-Based Replication

### Key Modeling Decisions

- **Roles**: Leader and Follower (or a single Node role with leader/follower states)
- **Channels**: Leader sends to followers, followers acknowledge
- **Key insight**: The commit rule (when the leader considers a write durable) is the critical correctness property

### Skeleton Specification

```python
# --- Assertions ---

always assertion CommittedDataDurable:
    """Once data is committed, it exists on a majority."""
    for entry_idx in range(committed_index + 1):
        count = sum(1 for n in nodes if len(n.log) > entry_idx
                    and n.log[entry_idx] == leader.log[entry_idx])
        assert count > len(nodes) // 2

always assertion LogPrefix:
    """Follower logs are a prefix of the leader's log (when in sync)."""
    for f in followers:
        if f.synced:
            for i in range(len(f.log)):
                assert f.log[i] == leader.log[i]

# --- State ---

committed_index = -1

# --- Leader Role ---

role Leader:
    log = []
    ack_count = {}

    def ClientWrite():
        """Accept a client write and append to log."""
        global log, ack_count
        entry = any v in ["A", "B", "C"]
        log.append(entry)
        idx = len(log) - 1
        ack_count[idx] = 1  # Leader counts itself
        # Replicate to followers
        for f in followers:
            chan[f].send({
                "type": "replicate",
                "index": idx,
                "entry": entry,
                "from": self_id,
            })

    def HandleAck(msg):
        """Process replication acknowledgment."""
        global ack_count, committed_index
        idx = msg["index"]
        ack_count[idx] = ack_count.get(idx, 0) + 1
        # Commit when majority acknowledges
        if ack_count[idx] > (len(nodes)) // 2:
            if idx > committed_index:
                committed_index = idx

# --- Follower Role ---

role Follower:
    log = []
    synced = False

    def HandleReplicate(msg):
        """Append entry from leader."""
        global log, synced
        if msg["index"] == len(log):
            log.append(msg["entry"])
            synced = True
            chan[msg["from"]].send({
                "type": "ack",
                "index": msg["index"],
                "from": self_id,
            })
        else:
            synced = False

# --- Instantiation ---

leader = role(Leader, 1)
followers = role(Follower, 2)
nodes = [leader] + list(followers)
chan = {i: channel() for i in range(3)}
```

### Extending the Pattern

**Write-ahead log**: Add durability by requiring entries to be "flushed" before acknowledging:

```python
def HandleReplicate(msg):
    global log, flushed
    log.append(msg["entry"])
    # Model flush as a separate step (serial action allows interleaving)
    flushed.add(msg["index"])
    chan[msg["from"]].send({"type": "ack", "index": msg["index"]})
```

**Leader failover**: Add leader election and test that committed entries survive leader changes. This converges toward a full Raft model.

**Configurable commit rule**: Parameterize the quorum size to test different durability guarantees:

```python
def is_committed(idx):
    return ack_count.get(idx, 0) >= quorum_size

# Test with different quorum sizes
quorum_size = len(nodes) // 2 + 1  # Majority
# quorum_size = len(nodes)          # All replicas (stronger but slower)
# quorum_size = 1                   # Leader only (weakest)
```

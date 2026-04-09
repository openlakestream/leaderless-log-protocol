# Fizzbee Language Fundamentals

Comprehensive reference for the Fizzbee specification language. Fizzbee uses Starlark (a Python-like language) as its base syntax, extended with constructs for modeling concurrency, nondeterminism, and distributed communication.

---

## File Structure

A `.fizz` file has the following top-level structure:

```python
# 1. Assertions (safety and liveness properties)
# 2. State variable declarations
# 3. Init block (initialization)
# 4. Actions (top-level functions that the model checker explores)
# 5. Helper functions
# 6. Role definitions (for multi-process specs)
```

Assertions are typically placed at the top so readers see the properties being verified before the model logic.

---

## State Variables

State variables represent the system state that the model checker tracks. They are declared at the top level.

```python
# Simple variables
leader = None
term = 0
committed = False

# Collections
log = []
votes = {}
nodes = set()
```

**Supported types**:
- Integers, strings, booleans, `None`
- Lists: `[]` -- ordered, mutable within actions
- Dicts: `{}` -- key-value maps
- Sets: `set()` -- unordered unique elements
- Tuples: `()` -- immutable ordered sequences

**Important**: Starlark is immutable by default for many operations. In Fizzbee, state variables can be mutated within actions, but you should be aware that intermediate mutations within an `atomic` block are not visible to other actions until the block completes.

---

## Init Block

The `init` block runs once at the start of model checking to set up initial state.

```python
def init():
    global log, term, voted_for
    log = []
    term = 0
    voted_for = None
```

If you have roles, each role instance runs its own init. State variables declared inside a role are per-instance.

---

## Actions

Actions are top-level functions that the model checker nondeterministically schedules. They represent events or operations in the system.

```python
def HandleTimeout():
    global term, state
    term += 1
    state = "candidate"
```

### Action Modifiers

Actions can have modifiers that control atomicity and interleaving:

#### `atomic`

The entire action body executes without interleaving. No other action can execute between any two statements in an `atomic` action.

```python
atomic def CompareAndSwap():
    global value
    if value == expected:
        value = new_value
```

Use `atomic` when the real system operation is truly atomic (e.g., a single database write, a CAS instruction, a transaction that holds a lock for its entire duration).

#### `serial` (default)

Statements execute sequentially, but interleaving can happen between any two statements. This is the default if no modifier is specified.

```python
serial def ReplicateLog():
    global log, ack_count
    # Other actions can interleave here
    log.append(entry)
    # Other actions can interleave here
    ack_count += 1
```

Use `serial` (or no modifier) for operations where the real system does not hold a lock across the entire operation.

#### `parallel`

Sub-blocks within the action execute concurrently. Each sub-block is explored in all possible interleavings with every other sub-block.

```python
parallel def BroadcastVoteRequests():
    for node in nodes:
        send(node, vote_request)
```

Use `parallel` for operations that the real system performs concurrently (e.g., sending RPCs to multiple nodes in parallel).

### `yield` Points

Within a `serial` action, `yield` explicitly marks a point where other actions may interleave. Without explicit yields, interleaving happens between every statement in a serial action.

```python
serial def TwoPhaseOperation():
    global phase
    phase = "prepare"
    yield  # Explicitly allow interleaving here
    phase = "commit"
```

In `atomic` actions, `yield` breaks atomicity at that point -- use with care.

---

## Nondeterminism

### `oneof`

Nondeterministic choice between code blocks. The model checker explores all branches.

```python
def HandleMessage():
    oneof:
        # Branch 1: Accept the message
        global state
        state = "accepted"
    or:
        # Branch 2: Reject the message
        global state
        state = "rejected"
    or:
        # Branch 3: Drop the message (crash)
        pass
```

Use `oneof` when the system can take one of several mutually exclusive paths (e.g., a node deciding to accept or reject, a nondeterministic timeout).

### `any`

Nondeterministic selection from a set or range. The model checker explores all possible selections.

```python
def ChooseLeader():
    node = any node in nodes
    global leader
    leader = node

def ChooseValue():
    v = any v in range(1, 4)  # Explores v=1, v=2, v=3
    global value
    value = v
```

Use `any` when the system picks from a set of options (e.g., which node to contact, which value to propose).

---

## Preconditions

The `requires` keyword constrains when an action can fire. If the precondition is false, the action is not explored from the current state.

```python
def AcceptVote():
    requires(state == "candidate")
    requires(votes_received < majority)
    global votes_received
    votes_received += 1
```

Use `requires` to:
- Prevent actions from firing in states where they are meaningless
- Reduce the state space by eliminating impossible transitions
- Model real system conditions (e.g., "only the leader can append entries")

---

## Fairness

The `fair` keyword marks an action as fair, meaning the model checker assumes it will eventually be scheduled if it is continuously enabled.

```python
fair def DeliverMessage():
    requires(len(message_queue) > 0)
    msg = message_queue.pop(0)
    process(msg)
```

Without `fair`, the model checker can find counterexamples where an enabled action is never taken. This is correct for modeling adversarial scheduling but causes spurious liveness violations for actions that the real system guarantees will eventually execute.

**When to use `fair`**:
- Message delivery on reliable networks
- Timeouts that the OS guarantees will fire
- Scheduled tasks that the runtime guarantees will run

**When NOT to use `fair`**:
- Actions that may genuinely never happen (e.g., a message on a lossy network)
- Voluntary actions (e.g., a client deciding to send a request)

---

## Roles

Roles define distinct types of processes in a distributed system. Each role can have its own state variables, init block, and actions.

```python
role Node:
    # Per-instance state
    term = 0
    state = "follower"
    voted_for = None
    log = []

    def init():
        pass

    def HandleTimeout():
        requires(state == "follower")
        global term, state
        term += 1
        state = "candidate"

    def HandleVoteRequest(msg):
        # Process vote request
        pass
```

**Instantiation**: Role instances are created by specifying the count:

```python
nodes = role(Node, 3)  # Creates 3 Node instances
```

Each instance has its own copy of the role's state variables. Actions from different instances can interleave freely.

---

## Channels

Channels model network communication between roles.

### Declaration

```python
chan = channel()           # Default: reliable, unordered
chan = channel(fifo=True)  # FIFO ordering
chan = channel(lossy=True) # Messages may be lost
chan = channel(capacity=5) # Bounded buffer
```

### Sending and Receiving

```python
# Send a message
chan.send({"type": "vote_request", "term": term, "from": self_id})

# Receive a message (blocks until available)
msg = chan.receive()

# Non-blocking receive
msg = chan.receive(timeout=0)
```

### Channel Types and Their Effects

| Channel Config | Behavior | Models |
|---|---|---|
| `channel()` | Reliable, unordered | Reliable network with reordering |
| `channel(fifo=True)` | Reliable, ordered | TCP-like connection |
| `channel(lossy=True)` | Messages may be dropped | UDP, unreliable network |
| `channel(lossy=True, fifo=True)` | Ordered but may lose | TCP with connection drops |
| `channel(duplicating=True)` | Messages may be duplicated | Network with retransmission |

See `references/fault-injection.md` for detailed fault injection patterns.

---

## Assertions

Assertions define the properties the model checker verifies.

### Safety: `always`

The predicate must hold in every reachable state.

```python
always assertion NoTwoLeaders:
    leader_count = sum(1 for n in nodes if n.state == "leader")
    assert leader_count <= 1

always assertion LogConsistency:
    for i in range(min(len(n1.log), len(n2.log))):
        if n1.log[i].term == n2.log[i].term:
            assert n1.log[i].value == n2.log[i].value
```

### Liveness: `eventually`

The predicate must hold in some future state (from the initial state).

```python
eventually assertion AllNodesConverge:
    assert all(n.value == nodes[0].value for n in nodes)
```

### Repeated Liveness: `always eventually`

The predicate must hold infinitely often -- it may be temporarily false but must keep becoming true.

```python
always eventually assertion ProgressMade:
    assert commit_index > 0
```

### Stability: `eventually always`

The predicate eventually becomes true and stays true forever.

```python
eventually always assertion StableLeader:
    leader_count = sum(1 for n in nodes if n.state == "leader")
    assert leader_count == 1
```

### Assertion Syntax Details

Assertions are defined at the top level of the `.fizz` file. The assertion name is used in error messages when the model checker finds a violation.

```python
# Assertion with helper function
def is_consistent():
    return all(n.committed_value == nodes[0].committed_value
               for n in nodes if n.committed_value is not None)

always assertion Consistency:
    assert is_consistent()
```

---

## Functions

Helper functions are regular Starlark functions. They are not actions -- the model checker does not schedule them independently. They are called from within actions or assertions.

```python
def quorum_size(n):
    return n // 2 + 1

def has_quorum(votes, total):
    return len(votes) >= quorum_size(total)

def find_max_term(nodes):
    return max(n.term for n in nodes)
```

Functions cannot modify state variables directly (they are not actions). Pass state in and return results.

---

## For Loops in Actions

For loops iterate over collections. In `serial` and `parallel` actions, interleaving can happen between iterations.

```python
serial def BroadcastHeartbeat():
    for node in followers:
        send(node, {"type": "heartbeat", "term": term})
        # Interleaving can happen here between iterations
```

In `atomic` actions, the entire loop executes without interleaving:

```python
atomic def CountVotes():
    global vote_count
    vote_count = 0
    for node in nodes:
        if node.voted_for == self_id:
            vote_count += 1
```

---

## Complete Example: Simple Lock Service

```python
# State
lock_holder = None
waiting = []

# Safety: mutual exclusion
always assertion MutualExclusion:
    # At most one holder
    assert lock_holder is None or isinstance(lock_holder, int)

# Liveness: no starvation (requires fair actions)
always eventually assertion NoStarvation:
    assert len(waiting) == 0

def init():
    global lock_holder, waiting
    lock_holder = None
    waiting = []

role Client:
    has_lock = False

    fair def RequestLock():
        requires(not has_lock)
        requires(self_id not in waiting)
        global waiting
        waiting.append(self_id)

    fair def AcquireLock():
        requires(not has_lock)
        requires(len(waiting) > 0 and waiting[0] == self_id)
        requires(lock_holder is None)
        global lock_holder, has_lock, waiting
        lock_holder = self_id
        has_lock = True
        waiting.pop(0)

    def ReleaseLock():
        requires(has_lock)
        global lock_holder, has_lock
        lock_holder = None
        has_lock = False

clients = role(Client, 3)
```

---

## Key Differences from Python

While Fizzbee uses Starlark (Python-like), note these differences:

1. **No classes** (besides `role`): Use dicts and functions instead
2. **No exceptions**: Use return values for error handling
3. **No imports**: All code is in a single `.fizz` file (or use Fizzbee's include mechanism)
4. **Limited standard library**: No `os`, `sys`, etc. -- this is a specification language
5. **Global keyword required**: Must declare `global` to modify top-level state in actions
6. **Deterministic execution**: Nondeterminism comes only from `oneof`, `any`, action scheduling, and channel behavior -- not from hash maps or floating point

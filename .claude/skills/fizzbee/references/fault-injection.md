# Fizzbee Fault Injection Patterns

Fizzbee's primary advantage for fault injection is that faults are injected implicitly through channel configuration. You change how the network behaves by changing channel properties, and the model checker automatically explores all possible fault scenarios.

---

## Implicit Fault Injection via Channel Types

### Reliable Channels (Default)

```python
chan = channel()
```

- Messages are always delivered
- Messages may arrive in any order (unordered by default)
- No duplicates
- Models a reliable network with potential reordering

**What the model checker explores**: All possible orderings of in-flight messages.

### FIFO Channels

```python
chan = channel(fifo=True)
```

- Messages are always delivered
- Messages arrive in the order they were sent
- No duplicates
- Models a TCP-like reliable, ordered connection

**What the model checker explores**: Only the send-order delivery sequence. This dramatically reduces the state space but hides reordering bugs.

### Lossy Channels

```python
chan = channel(lossy=True)
```

- Messages may be dropped at any point
- Surviving messages may arrive in any order
- No duplicates (unless also duplicating)
- Models UDP or an unreliable network

**What the model checker explores**: For every message send, the model checker explores both the "delivered" and "lost" outcomes. With N messages, this creates 2^N possible delivery combinations.

### Duplicating Channels

```python
chan = channel(duplicating=True)
```

- Messages are always delivered (at least once)
- Messages may be delivered multiple times
- Models a network with automatic retransmission or at-least-once delivery

**What the model checker explores**: For every message, the model checker explores receiving it once or multiple times.

### Combined Configurations

```python
# Lossy + unordered (hostile network)
chan = channel(lossy=True)

# Lossy + FIFO (TCP with connection drops)
chan = channel(lossy=True, fifo=True)

# Duplicating + unordered (at-least-once with reordering)
chan = channel(duplicating=True)

# Lossy + duplicating (worst case: messages lost OR duplicated OR reordered)
chan = channel(lossy=True, duplicating=True)

# Bounded buffer (models backpressure)
chan = channel(capacity=3)
```

---

## Channel Configuration Effects

| Configuration | Delivery | Ordering | Duplicates | State Space Impact |
|---|---|---|---|---|
| `channel()` | Guaranteed | Unordered | No | Moderate (permutations of in-flight messages) |
| `channel(fifo=True)` | Guaranteed | FIFO | No | Small (single ordering) |
| `channel(lossy=True)` | May lose | Unordered | No | Large (2^N delivery combinations) |
| `channel(lossy=True, fifo=True)` | May lose | FIFO for delivered | No | Moderate |
| `channel(duplicating=True)` | At-least-once | Unordered | Yes | Large (repeated deliveries) |
| `channel(lossy=True, duplicating=True)` | May lose or dup | Unordered | Yes | Very large |
| `channel(capacity=N)` | Blocks when full | Unordered | No | Depends on N |

---

## Crash Modeling

### Role Crashes

Model a process crash by having an action that resets the role's state to its initial values, simulating a restart.

```python
role Node:
    term = 0
    state = "follower"
    log = []
    voted_for = None

    # Persistent state (survives crashes)
    # In Fizzbee, model this by NOT resetting these in crash
    persistent_log = []
    persistent_term = 0

    def Crash():
        """Model crash and recovery."""
        global state, voted_for
        # Volatile state is lost
        state = "follower"
        voted_for = None
        # Persistent state is recovered
        # (log and term survive because we don't reset them)
```

### Crash-Stop vs. Crash-Recovery

**Crash-stop**: The node crashes and never recovers. Model by having the crash action set a flag that disables all other actions via `requires`.

```python
role Node:
    crashed = False

    def Crash():
        requires(not crashed)
        global crashed
        crashed = True

    def HandleMessage(msg):
        requires(not crashed)  # Dead nodes don't process messages
        # ...
```

**Crash-recovery**: The node crashes and eventually restarts. Model by resetting volatile state but preserving persistent state.

```python
role Node:
    crashed = False

    def Crash():
        requires(not crashed)
        global crashed, state, volatile_cache
        crashed = True
        state = "recovering"
        volatile_cache = {}
        # persistent_log is NOT reset

    fair def Recover():
        requires(crashed)
        global crashed, state
        crashed = False
        state = "follower"
```

---

## Network Partition Modeling

Network partitions are modeled by selectively making channels lossy between groups of nodes.

### Simple Partition (Two Groups)

```python
# Model a partition between group A and group B
# Messages within each group are reliable
# Messages between groups are lost

def is_partitioned(src, dst):
    group_a = {0, 1}
    group_b = {2, 3, 4}
    return (src in group_a and dst in group_b) or \
           (src in group_b and dst in group_a)

# Use per-pair channels with different configurations
for i in range(len(nodes)):
    for j in range(len(nodes)):
        if is_partitioned(i, j):
            chan[i][j] = channel(lossy=True)  # Cross-partition: lossy
        else:
            chan[i][j] = channel()  # Same partition: reliable
```

### Nondeterministic Partition

Model partitions that can form and heal nondeterministically:

```python
partitioned = set()  # Set of (src, dst) pairs that are partitioned

def FormPartition():
    """Nondeterministically partition some links."""
    global partitioned
    src = any s in range(len(nodes))
    dst = any d in range(len(nodes))
    requires(src != dst)
    partitioned.add((src, dst))
    partitioned.add((dst, src))  # Symmetric

def HealPartition():
    """Heal a partition."""
    requires(len(partitioned) > 0)
    pair = any p in partitioned
    global partitioned
    partitioned.remove(pair)
    # Also remove reverse
    partitioned.discard((pair[1], pair[0]))

def SendMessage(src, dst, msg):
    if (src, dst) in partitioned:
        pass  # Message lost due to partition
    else:
        chan[dst].send(msg)
```

---

## Partial Failure Scenarios

### Slow Node

Model a node that is slow (not crashed, but delayed). In Fizzbee, this is naturally modeled by the scheduler -- a serial action on the slow node just takes more "steps" before completing.

For explicit delay modeling:

```python
role Node:
    slow = False

    def ProcessMessage(msg):
        if slow:
            yield  # Extra yield point allows more interleaving
            yield  # Models the delay
        # ... process message ...
```

### Disk Failure

Model a node that cannot persist writes:

```python
role Node:
    disk_failed = False

    def WriteLog(entry):
        if disk_failed:
            oneof:
                pass  # Write silently fails
            or:
                # Write partially succeeds (corruption)
                global log
                log.append(None)  # Corrupted entry
        else:
            global log
            log.append(entry)
```

---

## Byzantine Fault Modeling (Limited)

Fizzbee can model some Byzantine behaviors through nondeterministic actions, though it is not as expressive as dedicated Byzantine fault tools.

### Equivocation (Sending Different Values to Different Nodes)

```python
role ByzantineNode:
    def SendConflicting():
        """Byzantine node sends different values to different peers."""
        for peer in peers:
            value = any v in ["A", "B", "C"]  # Different value to each peer
            chan[peer].send({"type": "proposal", "value": value})
```

### Arbitrary Message Injection

```python
def ByzantineSend():
    """Inject arbitrary messages into the network."""
    target = any t in nodes
    fake_term = any t in range(0, max_term + 1)
    fake_value = any v in all_values
    chan[target].send({
        "type": any t in ["vote_request", "append_entries", "heartbeat"],
        "term": fake_term,
        "value": fake_value,
    })
```

**Limitation**: Full Byzantine fault tolerance verification (e.g., BFT consensus with f < n/3) requires exploring all possible combinations of Byzantine behaviors, which can cause state explosion. For thorough BFT verification, consider specialized tools or use Fizzbee with small instance counts.

---

## Choosing Channel Configuration

### Decision Guide

| Real System Property | Channel Configuration |
|---|---|
| TCP between co-located services | `channel(fifo=True)` |
| TCP across data centers | `channel(fifo=True)` or `channel()` (reordering at app level) |
| UDP or unreliable transport | `channel(lossy=True)` |
| At-least-once delivery (retries) | `channel(duplicating=True)` |
| Message broker (Kafka, Pulsar) | `channel(fifo=True)` per partition |
| Gossip protocol | `channel(lossy=True)` |
| Hostile network (adversarial) | `channel(lossy=True, duplicating=True)` |

### Progressive Hardening Strategy

Start with the simplest channel configuration and progressively harden:

1. **Start reliable + FIFO**: `channel(fifo=True)` -- verify basic protocol correctness
2. **Allow reordering**: `channel()` -- find ordering assumptions
3. **Allow message loss**: `channel(lossy=True)` -- find reliability assumptions
4. **Allow duplication**: `channel(duplicating=True)` -- find idempotency issues
5. **Full hostile**: `channel(lossy=True, duplicating=True)` -- verify in worst case

At each step, fix any assertion violations before moving to the next level.

---

## Example: Same Spec, Different Channel Configs

Consider a simple replication protocol:

```python
role Leader:
    value = None

    def Write():
        requires(value is None)
        global value
        value = "X"
        for f in followers:
            chan[f].send({"type": "write", "value": "X"})

role Follower:
    value = None

    def HandleWrite(msg):
        global value
        value = msg["value"]

always assertion Consistency:
    if leader.value is not None:
        for f in followers:
            if f.value is not None:
                assert f.value == leader.value
```

### With `channel(fifo=True)`: PASSES

All messages arrive in order and are delivered. Consistency holds trivially.

### With `channel()`: PASSES

Messages may reorder, but since there is only one write, reordering does not matter.

### With `channel(lossy=True)`: PASSES (safety)

Messages may be lost, so followers may never receive the write. But the consistency assertion only checks followers that have a value -- it still passes.

However, add a liveness assertion:

```python
always eventually assertion AllFollowersUpdated:
    if leader.value is not None:
        assert all(f.value == leader.value for f in followers)
```

This FAILS with lossy channels because messages can be permanently lost. The fix: add retransmission logic to the leader:

```python
fair def RetransmitToFollower():
    requires(value is not None)
    for f in followers:
        if f.value != value:
            chan[f].send({"type": "write", "value": value})
```

This pattern -- start simple, add faults, observe failures, fix the protocol -- is the core Fizzbee workflow.

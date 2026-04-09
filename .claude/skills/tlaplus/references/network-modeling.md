# Network Modeling Patterns

How to model network communication, failures, and timing in TLA+ and PlusCal.

## Message Channel Models

The choice of channel model significantly affects state space size and what behaviors are captured.

### Sets (Unordered, No Duplicates)

The simplest and smallest state space. Use when the protocol handles reordering and idempotent message processing.

```tla
VARIABLE messages  \* messages \subseteq MessageType

\* Send a message
Send(m) == messages' = messages \union {m}

\* Receive and remove a message
Receive(m) ==
    /\ m \in messages
    /\ messages' = messages \ {m}

\* Send one message while receiving another
SendAndReceive(send, recv) ==
    messages' = (messages \ {recv}) \union {send}

\* Receive without removing (message stays available)
\* Models broadcast or persistent messages
ReceiveWithoutRemove(m) ==
    /\ m \in messages
    /\ UNCHANGED messages
```

State space: 2^|MessageType| -- each possible message is either present or absent.

When to use:
- Default choice for most protocols
- Protocols that tolerate reordering (most do)
- Protocols with idempotent message handling
- When you want the smallest state space

### Bags / Multisets (Unordered, With Duplicates)

Models duplicate message delivery. Messages can appear multiple times.

```tla
EXTENDS Bags

VARIABLE messages  \* messages is a bag (function from Message to Nat)

\* Send a message (add one copy)
Send(m) == messages' = messages (+) SetToBag({m})

\* Receive a message (remove one copy)
Receive(m) ==
    /\ BagIn(m, messages)
    /\ messages' = messages (-) SetToBag({m})

\* Check how many copies exist
CopiesOf(m) == CopiesIn(m, messages)
```

State space: grows with the maximum count of each message type.

When to use:
- When message duplication is a real concern
- When the protocol might behave differently on receiving duplicates
- UDP-like networks

### Sequences (Ordered, FIFO)

Models TCP-like channels with guaranteed ordering per connection.

```tla
VARIABLE channels  \* channels \in [Server \X Server -> Seq(Message)]

\* Send message from src to dest
Send(src, dest, m) ==
    channels' = [channels EXCEPT ![<<src, dest>>] = Append(@, m)]

\* Receive next message from src at dest
Receive(src, dest) ==
    /\ Len(channels[<<src, dest>>]) > 0
    /\ LET m == Head(channels[<<src, dest>>])
       IN /\ channels' = [channels EXCEPT ![<<src, dest>>] = Tail(@)]
          /\ \* process m ...

\* Per-pair FIFO channels
Init == channels = [pair \in Server \X Server |-> <<>>]
```

State space: exponential in maximum sequence length. Very expensive.

When to use:
- Only when FIFO ordering is essential to the property being checked
- TCP-based protocols where ordering guarantees matter
- Streaming systems with ordered delivery

### Recommendation

Start with **sets**. Only switch to bags or sequences if the property you are checking specifically depends on message duplication behavior or ordering guarantees.

## Failure Models

### Message Loss

Model by nondeterministically dropping messages from the channel.

```tla
\* Drop any message nondeterministically
DropMessage ==
    /\ \E m \in messages :
        messages' = messages \ {m}
    /\ UNCHANGED <<otherVars>>
```

Include this as a possible action in the `Next` relation:

```tla
Next ==
    \/ \E s \in Server : NormalAction(s)
    \/ DropMessage
```

### Message Duplication

With set-based channels, duplicates are impossible (adding the same message is a no-op). With bags:

```tla
\* Duplicate any message in the network
DuplicateMessage ==
    /\ \E m \in DOMAIN messages :
        /\ BagIn(m, messages)
        /\ messages' = messages (+) SetToBag({m})
    /\ UNCHANGED <<otherVars>>
```

With sets, you can model duplication by not removing messages on receive:

```tla
\* Receive without removing (models potential re-delivery)
ReceiveKeep(m) ==
    /\ m \in messages
    \* process m without removing it from messages
    /\ UNCHANGED messages
```

### Message Reordering

Set-based channels naturally model reordering: any message in the set can be received at any time. No special action needed.

For sequence-based channels, add an explicit reorder action:

```tla
\* Reorder messages in a channel
ReorderChannel(src, dest) ==
    /\ Len(channels[<<src, dest>>]) >= 2
    /\ \E i, j \in 1..Len(channels[<<src, dest>>]) :
        /\ i # j
        /\ channels' = [channels EXCEPT
            ![<<src, dest>>][i] = channels[<<src, dest>>][j],
            ![<<src, dest>>][j] = channels[<<src, dest>>][i]]
    /\ UNCHANGED <<otherVars>>
```

### Network Partitions

Model partitions by maintaining a partition structure and filtering messages.

```tla
VARIABLE partition  \* partition \in SUBSET (Server \X Server)
                    \* set of pairs that can communicate

\* Nondeterministic partition change
ChangePartition ==
    /\ \E newPartition \in SUBSET (Server \X Server) :
        partition' = newPartition
    /\ UNCHANGED <<otherVars>>

\* Only deliver messages between connected nodes
DeliverMessage ==
    /\ \E m \in messages :
        /\ <<m.sender, m.dest>> \in partition  \* only if connected
        /\ \* process message
    /\ UNCHANGED partition
```

Simpler approach -- partition as message filtering:

```tla
VARIABLE partitioned  \* partitioned \subseteq Server
                      \* set of servers cut off from the rest

CanCommunicate(s1, s2) ==
    ~(s1 \in partitioned /\ s2 \notin partitioned) /\
    ~(s2 \in partitioned /\ s1 \notin partitioned)
```

### Crash-Stop Failures

A process crashes and never recovers.

```tla
VARIABLE crashed  \* crashed \subseteq Server

\* Crash a server
Crash(s) ==
    /\ s \notin crashed
    /\ crashed' = crashed \union {s}
    /\ UNCHANGED <<otherVars>>

\* Guard all normal actions: only non-crashed servers act
NormalAction(s) ==
    /\ s \notin crashed
    /\ \* rest of action
```

In PlusCal, model as a process that can nondeterministically go to `Done`:

```
process server \in Server
begin
    Main:
        while self \notin crashed do
            either
                \* normal actions
            or
                \* crash
                crashed := crashed \union {self};
                goto Done;
            end either;
        end while;
end process;
```

### Crash-Recovery Failures

A process crashes, loses volatile state, but retains durable state.

```tla
VARIABLE
    crashed,        \* crashed \subseteq Server
    durableState,   \* [Server -> ...] survives crashes
    volatileState   \* [Server -> ...] lost on crash

Crash(s) ==
    /\ s \notin crashed
    /\ crashed' = crashed \union {s}
    /\ volatileState' = [volatileState EXCEPT ![s] = InitialVolatile]
    /\ UNCHANGED <<durableState, messages>>

Recover(s) ==
    /\ s \in crashed
    /\ crashed' = crashed \ {s}
    \* Rebuild volatile state from durable state
    /\ volatileState' = [volatileState EXCEPT ![s] = RebuildFrom(durableState[s])]
    /\ UNCHANGED <<durableState, messages>>
```

### Byzantine Faults

Byzantine nodes can send arbitrary messages. Model by allowing arbitrary message injection.

```tla
VARIABLE byzantine  \* byzantine \subseteq Server (fixed set of faulty nodes)

\* Byzantine node can send any message
ByzantineAction ==
    /\ \E b \in byzantine :
        /\ \E m \in AllPossibleMessages :
            /\ m.sender = b
            /\ messages' = messages \union {m}
    /\ UNCHANGED <<otherVars>>
```

For more targeted Byzantine behavior:

```tla
\* Byzantine node equivocates (sends conflicting messages)
ByzantineEquivocate(b) ==
    /\ b \in byzantine
    /\ \E v1, v2 \in Value : v1 # v2
    /\ \E s1, s2 \in Server \ byzantine : s1 # s2
    /\ messages' = messages \union
        {[type |-> "propose", val |-> v1, sender |-> b, dest |-> s1],
         [type |-> "propose", val |-> v2, sender |-> b, dest |-> s2]}
    /\ UNCHANGED <<otherVars>>
```

## Timeout Modeling

Real systems use timeouts, but TLA+ has no built-in notion of time. Model timeouts as nondeterministic actions.

### Simple Timeout

```tla
\* Any server can timeout at any time (nondeterministic)
Timeout(s) ==
    /\ state[s] = "follower"
    /\ state' = [state EXCEPT ![s] = "candidate"]
    /\ \* start election...
    /\ UNCHANGED <<otherVars>>
```

### Heartbeat Timeout

```tla
\* Leader timeout: follower has not heard from leader
LeaderTimeout(s) ==
    /\ state[s] = "follower"
    /\ ~(\E m \in messages : m.type = "heartbeat" /\ m.dest = s
         /\ m.term = currentTerm[s])
    /\ \* begin election
```

### Modeling Time (When Needed)

For protocols where timing matters (e.g., partial synchrony), add an explicit clock:

```tla
VARIABLE clock  \* [Server -> Nat] logical clock per server

\* Advance clock nondeterministically
Tick(s) ==
    /\ clock' = [clock EXCEPT ![s] = clock[s] + 1]
    /\ UNCHANGED <<otherVars>>

\* Timeout after delta time units
TimerExpired(s) ==
    /\ clock[s] - lastHeard[s] > Delta
    /\ \* timeout action
```

Warning: adding clocks dramatically increases state space. Only use when timing is essential to the property.

## Channel Capacity Bounds

For state space reduction, bound the number of in-flight messages:

```tla
\* State constraint
CONSTRAINT Cardinality(messages) <= MaxMessages

\* Or as a guard in Send
Send(m) ==
    /\ Cardinality(messages) < MaxMessages
    /\ messages' = messages \union {m}
```

Choose `MaxMessages` to be large enough that the bound does not hide bugs. A good rule of thumb: `MaxMessages >= 2 * Cardinality(Server)` for pairwise protocols.

## Complete Example: Unreliable Network

```tla
---- MODULE UnreliableNetwork ----
EXTENDS Integers, FiniteSets

CONSTANTS Server, MaxMessages

VARIABLE messages

MessageType == [type: {"request", "response"}, sender: Server,
                dest: Server, body: {"a", "b", "c"}]

\* Send a message (if under capacity)
Send(m) ==
    /\ m \in MessageType
    /\ Cardinality(messages) < MaxMessages
    /\ messages' = messages \union {m}

\* Receive a message (remove from network)
Receive(s) ==
    /\ \E m \in messages :
        /\ m.dest = s
        /\ messages' = messages \ {m}

\* Drop a message (message loss)
Drop ==
    /\ messages # {}
    /\ \E m \in messages :
        messages' = messages \ {m}

\* Duplicate a message (keep original, add to set is no-op for sets)
\* With sets, this is automatically handled -- no explicit action needed

\* Reorder: automatic with sets -- any message can be received at any time

====
```

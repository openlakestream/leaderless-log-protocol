# State Space Reduction

Techniques for making TLC model checking feasible by reducing the number of states it must explore.

## Understanding State Space Explosion

The state space is the set of all reachable states. Its size is roughly the product of the domains of all variables, multiplied by the number of interleavings.

Example: 3 servers, each with a term in 0..5, a state in {"follower", "candidate", "leader"}, and a log of up to 3 entries from 2 possible values.
- Terms: 6^3 = 216
- States: 3^3 = 27
- Logs: each server can have sequences of length 0-3 over 2 values, roughly 15 possibilities per server, so 15^3 = 3375
- Combined (without messages): 216 * 27 * 3375 = ~19.7 million
- Add messages and interleavings: easily billions

## Technique 1: Symmetry Sets

If the specification treats certain values interchangeably, symmetry reduction avoids exploring equivalent permutations.

### How to Apply

In the TLA+ module:
```tla
ServerSymmetry == Permutations(Server)
ValueSymmetry == Permutations(Value)
AllSymmetry == ServerSymmetry \union ValueSymmetry
```

In the .cfg file:
```
SYMMETRY AllSymmetry
```

### Impact

For a set of n model values, symmetry reduces the states by up to n! (factorial). With 3 servers: 6x reduction. With 5 servers: 120x reduction.

### Requirements for Soundness

Symmetry reduction is only correct when:
1. The specification is symmetric with respect to the set (all elements are treated identically)
2. The invariants and properties are symmetric
3. You are NOT checking liveness properties (symmetry with liveness can produce spurious counterexamples — symmetry maps distinct states to the same equivalence class, which can create cycles that don't exist in the original state graph)

Common violations:
- One server has special initial state (e.g., initial leader)
- Constants reference specific model values (e.g., `Leader = s1`)
- Invariants mention specific model values

## Technique 2: State Constraints (CONSTRAINT)

Bound variables to prevent unbounded growth.

```tla
StateConstraint ==
    /\ \A s \in Server : currentTerm[s] <= MaxTerm
    /\ \A s \in Server : Len(log[s]) <= MaxLogLen
    /\ Cardinality(messages) <= MaxMessages
```

### Choosing Good Bounds

- Start small: MaxTerm = 2, MaxLogLen = 2, MaxMessages = 6
- If TLC finds no violations, increase incrementally
- Many bugs manifest with small constants. Raft's safety bug was found with 3 servers and term bound of 3.

### Risk

Constraints can hide bugs by preventing TLC from reaching states that would violate invariants. Always ask: could the constraint prevent the buggy execution from occurring?

Mitigation: Run without constraints at smaller scale to validate, then add constraints for larger scale.

## Technique 3: Action Constraints (ACTION_CONSTRAINT)

Filter transitions rather than states. Useful for bounding message creation without bounding the reachable states.

```tla
ActionConstraint ==
    \/ Cardinality(messages') <= MaxMessages
    \/ Cardinality(messages') <= Cardinality(messages)  \* allow shrinking
```

This allows messages to be consumed (reducing count) but prevents unbounded generation.

## Technique 4: Choosing Appropriate Constants

The most impactful reduction is often the simplest: use smaller constants.

| Parameter | Start with | Scale to | Notes |
|-----------|-----------|----------|-------|
| Number of servers | 2-3 | 4-5 | Many quorum-based bugs appear with 3 |
| Number of values | 1-2 | 2-3 | Need 2+ to check agreement |
| Term/ballot bound | 2-3 | 4-5 | Need 2+ for leader change scenarios |
| Log length bound | 2-3 | 3-4 | Need 2+ for log matching bugs |
| Number of clients | 1-2 | 2-3 | Need 2+ for concurrent request bugs |

### Evidence-Based Scaling

1. Run with minimal constants first
2. Check that all actions are covered (use `-coverage`)
3. Increase one constant at a time
4. If no new violations appear and coverage stays the same, the constant is large enough

## Technique 5: Abstracting Away Detail

Remove details that are irrelevant to the property you are checking.

### Examples

- **Omit message contents** when checking a leader election property that depends only on vote counts, not log entries
- **Collapse multiple message types** if they behave identically for your property
- **Use nondeterminism** instead of computing: replace a deterministic function with nondeterministic choice if the specific value does not matter
- **Remove persistence** if crash-recovery is not being modeled

### Over-Abstraction Warning

If you abstract too much, the model becomes vacuously correct. Check:
- Can the model produce the specific scenarios you want to test?
- Do all relevant failure modes remain reachable?

## Technique 6: Reducing Process Count

Instead of modeling N processes, model a smaller fixed set and reason about generalization.

For quorum-based protocols, 3 is often the sweet spot:
- 2f+1 = 3 means f = 1, so you can model 1 failure
- Quorum intersection is tested
- Much smaller than 5 or 7 nodes

## Technique 7: Message Channel Optimization

The choice of message channel model has a dramatic impact on state space.

| Model | State space | When to use |
|-------|-------------|-------------|
| **Set** (unordered, no duplicates) | Smallest | Protocol tolerates reordering and handles duplicates |
| **Bag/Multiset** (unordered, with duplicates) | Medium | Need to model duplicate delivery |
| **Sequence** (ordered, FIFO) | Largest | TCP-like channels, FIFO required |

### Set (Recommended Default)

```tla
messages \subseteq MessageType
Send(m) == messages' = messages \union {m}
```

State space: 2^|MessageType|. Each message is either present or not.

### Bag

```tla
EXTENDS Bags
messages \in BagType
Send(m) == messages' = messages (+) SetToBag({m})
```

State space: grows with message counts. Bounded by MaxMessages.

### Sequence Per Pair

```tla
channels \in [Server \X Server -> Seq(MessageType)]
```

State space: exponential in sequence length. Use only when FIFO ordering is essential to the property.

### Recommendation

Start with sets. Only switch to bags or sequences if the property you are checking depends on message duplication or ordering.

## Technique 8: DFID (Depth-First Iterative Deepening)

BFS stores all states at the current depth, which consumes memory. DFID uses DFS with increasing depth limits.

```bash
java -jar tla2tools.jar -dfid 20 Spec.tla
```

### When to Use

- BFS runs out of memory
- Bugs are expected at shallow depths (most protocol bugs are)
- Not suitable for liveness checking

### Trade-offs

- Uses O(depth) memory instead of O(diameter * branching_factor)
- Re-explores states at each depth level (slower in wall-clock time)
- Does not support liveness checking

## Technique 9: Simulation Mode

For state spaces that are infeasible to exhaust, random simulation finds bugs quickly.

```bash
java -jar tla2tools.jar -simulate num=50000 -depth 200 Spec.tla
```

### Advantages

- Constant memory usage
- Can explore very deep traces
- Often finds bugs quickly for property testing

### Disadvantages

- Not complete: may miss bugs
- Cannot check liveness properties
- Relies on randomness (set seed for reproducibility)

## Technique 10: Profiling State Generation

Use coverage statistics to identify optimization targets:

```bash
java -jar tla2tools.jar -workers 4 -coverage 1 Spec.tla
```

Look for:
- **Actions generating the most distinct states**: These are your optimization targets. Can you reduce their nondeterminism?
- **Actions never enabled**: Dead code in the spec. Remove or fix.
- **Unbalanced coverage**: If one action dominates, consider adding action constraints.

## Common Patterns

### Bounding Message Queues

```tla
CONSTRAINT Cardinality(messages) <= 2 * Cardinality(Server)
```

### Limiting Terms/Epochs

```tla
CONSTRAINT \A s \in Server : currentTerm[s] <= 3
```

### Bounding Log Length

```tla
CONSTRAINT \A s \in Server : Len(log[s]) <= 3
```

### Combining Constraints

```tla
StateConstraint ==
    /\ \A s \in Server : currentTerm[s] <= MaxTerm
    /\ \A s \in Server : Len(log[s]) <= MaxLogLen
    /\ Cardinality(messages) <= MaxMessages
```

## Decision Checklist

When TLC is too slow or runs out of memory:

1. Are you using the smallest meaningful constants? (Technique 4)
2. Can you apply symmetry? (Technique 1)
3. Can you add state constraints? (Technique 2)
4. Can you simplify the message model from sequences to sets? (Technique 7)
5. Can you abstract away irrelevant detail? (Technique 5)
6. Can you reduce the number of processes? (Technique 6)
7. Switch to DFID if memory is the bottleneck (Technique 8)
8. Switch to simulation if exhaustive checking is infeasible (Technique 9)

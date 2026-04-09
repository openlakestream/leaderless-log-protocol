# Fizzbee vs. TLA+: Decision Guide

When to use Fizzbee versus TLA+ for formal verification of distributed systems.

---

## Syntax Comparison

### Fizzbee: Starlark (Python-like)

```python
role Node:
    term = 0
    state = "follower"

    def HandleTimeout():
        requires(state == "follower")
        global term, state
        term += 1
        state = "candidate"

always assertion NoTwoLeaders:
    leaders = sum(1 for n in nodes if n.state == "leader")
    assert leaders <= 1
```

### TLA+: Mathematical Notation

```tla
VARIABLE term, state

HandleTimeout(n) ==
    /\ state[n] = "follower"
    /\ term' = [term EXCEPT ![n] = term[n] + 1]
    /\ state' = [state EXCEPT ![n] = "candidate"]

NoTwoLeaders ==
    Cardinality({n \in Nodes : state[n] = "leader"}) <= 1
```

**Key difference**: Fizzbee reads like a program. TLA+ reads like a mathematical formula. Fizzbee mutates state in place; TLA+ describes the relationship between current and next states using primed variables.

---

## Learning Curve

| Aspect | Fizzbee | TLA+ |
|---|---|---|
| Time to first spec | Hours | Days to weeks |
| Background needed | Python programming | Set theory, temporal logic |
| Syntax familiarity | Familiar to most developers | Unique mathematical notation |
| Concurrency model | Actions, roles, channels (explicit) | Next-state relations (implicit) |
| Error messages | Readable traces with state diffs | Traces require TLA+ fluency to interpret |
| Documentation | Growing, focused | Extensive (Lamport's book, Hillel Wayne's book, community resources) |

**Bottom line**: A developer with Python experience can write a useful Fizzbee spec in a day. TLA+ typically requires a week of study before writing a non-trivial spec.

---

## Expressiveness

### Fizzbee Advantages
- **Channels as first-class constructs**: Network communication is built in; TLA+ requires explicit modeling of message sets
- **Implicit fault injection**: Change a channel config to get automatic message loss/reordering; TLA+ requires writing the fault logic
- **Role-based decomposition**: Multi-process systems map naturally to `role` definitions
- **Performance modeling**: Built-in support for performance/cost properties (not just correctness)

### TLA+ Advantages
- **Refinement mapping**: Prove that a concrete spec implements an abstract spec (not available in Fizzbee)
- **Parameterized modules**: Reusable specification modules with parameters
- **Richer temporal logic**: Full TLA (Temporal Logic of Actions) including `\cdot` (composition), `ENABLED`, `WF`/`SF` (weak/strong fairness) with fine-grained control
- **Quantification over actions**: Express properties like "for all possible actions, if enabled, then..." which is hard in Fizzbee
- **Symmetry reduction**: TLC supports symmetry sets to reduce state space

### Equivalent in Both
- Safety properties (invariants)
- Basic liveness properties (eventually, always eventually)
- Nondeterministic choice
- Bounded model checking
- Counterexample generation

---

## Fault Injection

### Fizzbee: Implicit via Channels

```python
# Change one line to inject faults:
chan = channel()              # Reliable
chan = channel(lossy=True)    # Automatic message loss
chan = channel(lossy=True, duplicating=True)  # Loss + duplication
```

The model checker automatically explores all fault scenarios (which messages are lost, in what order they arrive, etc.) without any changes to the protocol logic.

### TLA+: Explicit Modeling

```tla
\* Must explicitly model message loss
Send(msg) ==
    \/ messages' = messages \cup {msg}     \* Message delivered
    \/ UNCHANGED messages                   \* Message lost

\* Must explicitly model reordering
\* (inherent in set-based message modeling, but FIFO requires explicit sequence modeling)
```

In TLA+, you write the fault logic as part of the spec. This gives more control but requires more effort and is error-prone -- it is easy to accidentally model faults incorrectly.

---

## Tooling Ecosystem

### Fizzbee
| Tool | Purpose |
|---|---|
| `fizz verify` | Model checker (CLI) |
| fizzbee.io | Online playground for quick experiments |
| VS Code extension | Syntax highlighting, inline verification |
| MBT (Model-Based Testing) | Generate Go tests from verified specs |
| State graph visualization | Visual state exploration |
| Performance modeling | Cost/performance property checking |

### TLA+
| Tool | Purpose |
|---|---|
| TLC | Primary model checker (mature, battle-tested) |
| TLAPS | Proof system for deductive verification |
| Apalache | Symbolic model checker (bounded, SMT-based) |
| TLA+ Toolbox | Eclipse-based IDE |
| VS Code extension | Community-maintained editor support |
| tla-web | Experimental web-based visualization |
| Specifying Systems (Lamport) | Definitive textbook |

**Ecosystem maturity**: TLA+ has a significantly larger ecosystem. TLC has been used in production at Amazon (DynamoDB, S3, EBS), Microsoft (Azure Cosmos DB), MongoDB, CockroachDB, Elastic, and many others. Fizzbee's ecosystem is newer and growing.

---

## Community and Industry Adoption

### TLA+
- **Amazon**: DynamoDB, S3, EBS, internal services (published paper: "Use of Formal Methods at Amazon Web Services")
- **Microsoft**: Azure Cosmos DB, Xbox Live services
- **MongoDB**: Replication protocol verification
- **CockroachDB**: Raft-based replication verification
- **Elastic**: Elasticsearch cluster coordination
- **Published specs**: Hundreds of publicly available specifications on GitHub
- **Conferences**: TLA+ Community Event, academic papers
- **Books**: "Specifying Systems" (Lamport), "Practical TLA+" (Hillel Wayne), "Learn TLA+" (Hillel Wayne)

### Fizzbee
- **Growing adoption**: Newer tool with increasing community interest
- **Focus on accessibility**: Designed to lower the barrier to formal methods
- **Published examples**: Protocol examples on fizzbee.io
- **Integration focus**: MBT bridges the spec-to-implementation gap

---

## Model-Based Testing

### Fizzbee: Native Support

Fizzbee natively generates test traces from verified specifications and provides tooling to replay those traces against Go implementations.

```
Spec -> fizz verify -> verified traces -> Go test harness -> implementation tests
```

### TLA+: External Tooling Required

TLA+ does not have built-in MBT. Options:
- **Apalache + custom scripts**: Extract traces from Apalache and convert to test inputs
- **TLC trace replay**: Write custom code to parse TLC traces and drive tests
- **Informal**: Manually extract scenarios from counterexamples and write test cases

---

## Performance Modeling

### Fizzbee
Fizzbee supports performance properties natively. You can annotate actions with costs and verify bounds:
- Latency bounds on operations
- Message count bounds
- Resource usage constraints

### TLA+
TLA+ focuses on correctness (safety and liveness). Performance modeling requires:
- Adding cost variables manually to the spec
- Writing invariants over those cost variables
- This is doable but not a first-class feature

---

## Decision Matrix

| Situation | Recommendation |
|---|---|
| Team is new to formal methods | **Fizzbee** -- lower barrier to entry |
| Python-familiar team | **Fizzbee** -- Starlark syntax is immediately familiar |
| Need fast iteration on a protocol design | **Fizzbee** -- quicker to write and modify |
| Need model-based testing for Go | **Fizzbee** -- native MBT support |
| Want to verify fault tolerance easily | **Fizzbee** -- implicit fault injection via channels |
| Extending existing TLA+ specs | **TLA+** -- don't rewrite working specs |
| Need refinement proofs | **TLA+** -- Fizzbee does not support refinement |
| Complex temporal properties (beyond always/eventually) | **TLA+** -- richer temporal logic |
| Industry-standard verification for audit/compliance | **TLA+** -- larger track record, more published validation |
| Need deductive proofs (not just model checking) | **TLA+** -- TLAPS proof system |
| Large existing TLA+ ecosystem specs to reference | **TLA+** -- extensive library of published specs |
| Performance/cost modeling | **Fizzbee** -- first-class performance properties |

---

## Migration Path

### From TLA+ to Fizzbee

If you have existing TLA+ specs and want to try Fizzbee:
1. Start with a new, simple spec in Fizzbee -- do not try to transliterate complex TLA+
2. Use Fizzbee for new protocols where MBT is valuable
3. Keep existing TLA+ specs for protocols that use refinement or advanced temporal properties

### From Fizzbee to TLA+

If you outgrow Fizzbee:
1. The mental model transfers -- actions, state variables, invariants are conceptually similar
2. The main learning curve is TLA+'s mathematical syntax and primed variable convention
3. Refinement and TLAPS are the main capabilities you gain

---

## Summary

**Choose Fizzbee** when you want to get started quickly, iterate fast, and bridge to implementation via MBT. Fizzbee excels when your team knows Python, you need fault injection without manual modeling, and you want to generate tests from specs.

**Choose TLA+** when you need the full power of temporal logic, refinement proofs, deductive verification (TLAPS), or when you are working in an ecosystem with existing TLA+ specs. TLA+ has a steeper learning curve but more expressiveness and a larger community.

Both tools verify the same fundamental properties (safety and liveness) and both catch real bugs in distributed systems. The best tool is the one your team will actually use.

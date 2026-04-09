---
name: tlaplus
description: TLA+ and PlusCal model checking for distributed systems. Use when creating, editing, reviewing, or optimizing formal specifications for consensus protocols (Raft, Paxos, Zab), replication, transactions, and distributed algorithms. Guides from first principles: start from what failures to prevent, work backwards to the model.
---

# TLA+ and PlusCal Skill

**Version**: 1.0.0
**License**: Apache 2.0

Formal specification and model checking for distributed systems using TLA+ and PlusCal.

---

## Workflow 1: Create or Edit a TLA+ Specification

Use this workflow when starting a new specification or significantly modifying an existing one. The approach is first-principles: start from the problem, not the syntax.

### Step 1: Identify What Can Go Wrong

Before writing any TLA+, answer these questions:

1. **What failure are you trying to prevent?** This becomes a safety property.
   - "Two nodes must never both believe they are leader in the same term" -> `\A i, j \in Server : (state[i] = "leader" /\ state[j] = "leader") => currentTerm[i] # currentTerm[j]`
   - "A committed entry must never be lost" -> invariant over committed log entries
   - "No two participants decide differently" -> agreement property

2. **What must eventually happen?** This becomes a liveness property.
   - "A client request is eventually processed" -> `<>(request_processed)`
   - "A leader is eventually elected" -> `<>(\E s \in Server : state[s] = "leader")`
   - "Every transaction eventually commits or aborts" -> leads-to property

3. **What kind of failures should the model handle?**
   - Crash-stop: process halts permanently
   - Crash-recovery: process halts then restarts with durable state
   - Network partition: messages between subsets are dropped
   - Message loss, duplication, reordering
   - Byzantine faults: arbitrary behavior

### Step 2: Decide PlusCal vs Raw TLA+

Use this decision tree:

- **Use PlusCal** when:
  - The algorithm has clear sequential or multi-process structure
  - You want to model specific processes with explicit control flow
  - The algorithm maps naturally to imperative pseudocode
  - You need labels to control atomicity granularity
  - You are modeling a protocol with message-passing between named processes

- **Use raw TLA+** when:
  - The specification is more declarative than procedural
  - You need maximum flexibility in defining the next-state relation
  - You want to compose multiple sub-actions with disjunction
  - The system does not map cleanly to a fixed set of processes
  - You are modeling a shared-memory concurrent system with arbitrary interleaving

See `knowledge/pluscal-guide.md` for PlusCal syntax and conventions.
See `knowledge/tlaplus-fundamentals.md` for raw TLA+ syntax and operators.

### Step 3: Define the State Space

Identify the core variables:

1. **Per-node state**: What does each node/process track? (e.g., `currentTerm`, `votedFor`, `log`, `commitIndex`)
2. **Network state**: How are messages modeled? (e.g., a set `messages`, a bag, or per-pair sequences)
3. **Global state** (if any): Auxiliary variables for specification purposes (e.g., `allCommitted` for stating invariants)

Keep the state space minimal. Every variable you add multiplies the number of states TLC must explore. See `knowledge/state-space-reduction.md`.

### Step 4: Write the Specification

Structure of a TLA+ module:

```tla
---- MODULE ProtocolName ----
EXTENDS Integers, Sequences, FiniteSets, TLC

CONSTANTS
    Server,        \* set of server IDs
    Value,         \* set of values that can be proposed
    MaxTerm        \* bound for model checking

VARIABLES
    currentTerm,   \* currentTerm[s]: latest term server s has seen
    state,         \* state[s]: "follower", "candidate", or "leader"
    log,           \* log[s]: sequence of log entries on server s
    messages       \* set of messages in the network

vars == <<currentTerm, state, log, messages>>

TypeOK ==
    /\ currentTerm \in [Server -> 0..MaxTerm]
    /\ state \in [Server -> {"follower", "candidate", "leader"}]
    /\ log \in [Server -> Seq(Value)]
    /\ messages \subseteq MessageType

Init == \* Initial state predicate
    /\ currentTerm = [s \in Server |-> 0]
    /\ state = [s \in Server |-> "follower"]
    /\ log = [s \in Server |-> <<>>]
    /\ messages = {}

\* Define individual actions here...
\* Action1(s) == ...
\* Action2(s, m) == ...

Next ==
    \/ \E s \in Server : Action1(s)
    \/ \E s \in Server : \E m \in messages : Action2(s, m)

Spec == Init /\ [][Next]_vars /\ WF_vars(Next)

\* Safety properties (invariants)
SafetyProperty == \* ... your invariant ...

\* Liveness properties (temporal formulas)
LivenessProperty == \* ... your temporal formula ...

====
```

### Step 5: Write the Configuration

Create a `.cfg` file. See `knowledge/tlc-configuration.md` for full reference.

```
INIT Init
NEXT Next

CONSTANTS
    Server = {s1, s2, s3}
    Value = {v1, v2}
    MaxTerm = 3

INVARIANT TypeOK
INVARIANT SafetyProperty

PROPERTY LivenessProperty

SYMMETRY ServerSymmetry
```

### Step 6: Run TLC and Interpret Results

```bash
# Basic run with 4 workers
java -jar tla2tools.jar -workers 4 ProtocolName.tla

# With depth-first iterative deepening (for deep bugs)
java -jar tla2tools.jar -workers 4 -dfid 30 ProtocolName.tla

# Simulation mode (random walks, good for large state spaces)
java -jar tla2tools.jar -simulate -depth 100 ProtocolName.tla
```

When TLC finds a violation:

1. **Read the error trace** from bottom to top. The last state is where the invariant breaks.
2. **Identify which action** transitioned into the bad state.
3. **Ask**: Is this a real bug in the protocol, or a modeling error?
   - Real bug: the protocol needs fixing. The trace is a counterexample.
   - Modeling error: the spec is too permissive or missing a constraint.
4. **Common false positives**: missing `UNCHANGED` for variables, over-broad nondeterminism, missing type constraints.

See `references/protocol-patterns.md` for protocol-specific modeling patterns.
See `references/network-modeling.md` for network and failure modeling.
See `references/spec-template.md` for starter templates.

---

## Workflow 2: Review a TLA+ Specification

Use this workflow when reviewing an existing specification for correctness, completeness, and quality. Launches 3 review agents in parallel, each analyzing from a different perspective, then aggregates findings into a unified report.

### Step 1: Understand Intent

Before launching agents, read the specification and answer:
- What distributed system does this spec model?
- What properties does it claim to verify (safety and liveness)?
- What assumptions does it make about failures and the environment?

This context is passed to all three agents.

### Step 2: Launch Review Agents

Launch 3 agents in parallel using the Agent tool (`subagent_type: general-purpose`). Each agent should read the specification files and the relevant sections of `references/review-checklist.md`. Pass the intent context from Step 1 to each agent.

**Agent 1: Correctness & Properties**
- Focus: Safety/liveness properties, TypeOK, initial state correctness, action correctness, fairness conditions
- Checklist sections: Correctness (Safety Properties, Liveness Properties, Initial State, Actions), Fairness (WF, SF, No Fairness)
- Key questions: Do the invariants actually express the requirements? Are fairness conditions appropriate and not hiding bugs? Does Init set all variables correctly?

**Agent 2: Completeness & Modeling**
- Focus: Action coverage, failure mode coverage, state coverage, abstraction level assessment
- Checklist sections: Completeness (Actions and Transitions, Failure Modes, State Coverage), Abstraction Level (Too Detailed, Too Abstract, Right Level)
- Key questions: Are all protocol actions and failure modes modeled? Is the abstraction level right — detailed enough to catch real bugs, abstract enough to check?

**Agent 3: Configuration & Quality**
- Focus: TLC configuration, common coding mistakes, final verification steps
- Checklist sections: Configuration (Constants, Symmetry, Constraints, Deadlock), Common Mistakes (Missing UNCHANGED, Forgotten Variables, PlusCal Labels, Deadlock vs Termination, Set vs Function, Quantifier Scoping, CHOOSE Pitfalls), Final Verification Steps
- Key questions: Are constants sized correctly? Is symmetry applied where valid? Are there common TLA+ coding errors?

Each agent should return structured findings: `[severity (critical/warning/info), category, description, recommendation]`.

### Step 3: Aggregate and Report

Collect findings from all three agents. Deduplicate overlapping issues. Sort by severity (critical > warning > info). Present using the review report template in `references/review-checklist.md`.

---

## Workflow 3: Optimize an Existing Specification

Use this workflow when TLC is too slow or runs out of memory. The goal is to reduce the state space without losing the ability to find bugs.

### Step 1: Profile First

Before optimizing, understand where the state space comes from:

```bash
# Run with coverage statistics
java -jar tla2tools.jar -workers 4 -coverage 1 ProtocolName.tla
```

Look at the coverage output:
- Which actions generate the most distinct states?
- Which variables have the largest domains?
- Is one action dominating exploration time?

### Step 2: Apply Reductions

Work through these techniques in order of impact. See `knowledge/state-space-reduction.md` for detailed guidance.

1. **Reduce constants**: Start with the smallest values that are meaningful (e.g., 2 servers, 1 value, term bound of 2). Scale up only after checking passes.

2. **Add symmetry**: If `Server = {s1, s2, s3}` and the protocol is symmetric, declare `ServerSymmetry == Permutations(Server)` and add `SYMMETRY ServerSymmetry` to the `.cfg` file.

3. **Add state constraints**: Bound variables that grow without limit.
   ```
   CONSTRAINT
       \A s \in Server : Len(log[s]) <= MaxLogLen
       /\ \A s \in Server : currentTerm[s] <= MaxTerm
   ```

4. **Simplify the network model**: Use sets instead of bags or sequences if message ordering and duplication are not essential to the property you are checking.

5. **Use simulation mode**: For very large state spaces, random simulation can find bugs much faster than exhaustive checking.
   ```bash
   java -jar tla2tools.jar -simulate -depth 100 -num 10000 ProtocolName.tla
   ```

6. **Try DFID**: Depth-first iterative deepening finds shallow bugs with less memory.
   ```bash
   java -jar tla2tools.jar -dfid 20 ProtocolName.tla
   ```

### Step 3: Verify the Optimization is Sound

After applying a reduction, ask:
- Does the reduced model still cover the failure scenarios I care about?
- Could the reduction hide a real bug? (e.g., a constraint that prevents the buggy state from being reached)
- Run the optimized model and compare results with a smaller but exhaustive run.

---

## Quick Reference

| Task | Reference File |
|------|---------------|
| TLA+ syntax and operators | `knowledge/tlaplus-fundamentals.md` |
| PlusCal syntax and idioms | `knowledge/pluscal-guide.md` |
| TLC configuration and CLI | `knowledge/tlc-configuration.md` |
| State space reduction | `knowledge/state-space-reduction.md` |
| Protocol modeling patterns | `references/protocol-patterns.md` |
| Network and failure modeling | `references/network-modeling.md` |
| Starter templates | `references/spec-template.md` |
| Review checklist | `references/review-checklist.md` |

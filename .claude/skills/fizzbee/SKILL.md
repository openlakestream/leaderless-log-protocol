---
name: fizzbee
description: Fizzbee formal verification for distributed systems. Use when creating, editing, reviewing, or testing Starlark-based specifications with implicit fault injection and model-based testing for consensus protocols (Raft, Paxos), replication, transactions, and distributed algorithms.
---

# Fizzbee Formal Verification Skill

**Version**: 1.0.0
**License**: Apache 2.0

Guides the creation, review, and testing of Fizzbee specifications for distributed systems. Fizzbee uses a Starlark-based language (Python-like syntax) to model concurrent and distributed protocols, with built-in support for fault injection via channels, exhaustive model checking, and model-based testing.

---

## Overview

### Core Capabilities

1. **Specification Authoring**: Create Fizzbee `.fizz` files from distributed systems requirements
2. **Model Review**: Structured review of existing specifications for correctness and completeness
3. **Model-Based Testing**: Bridge verified specifications to implementation via Go test generation
4. **Fault Analysis**: Leverage implicit fault injection through channel configuration

### When to Use Fizzbee

- Designing a new distributed protocol or algorithm
- Verifying correctness of consensus mechanisms (Raft, Paxos, etc.)
- Testing replication, transaction, or coordination logic
- Need fast iteration with Python-like syntax (vs. TLA+'s mathematical notation)
- Want built-in model-based testing to connect specs to Go implementations

### When to Consider TLA+ Instead

- Need refinement proofs or parameterized modules
- Extending existing TLA+ specifications
- Require the larger TLA+ ecosystem (TLC, TLAPS, Apalache)
- Complex temporal properties beyond `always`/`eventually`

See `knowledge/fizzbee-vs-tlaplus.md` for a detailed comparison.

---

## Workflow 1: Create or Edit a Fizzbee Specification

### Phase 1: Understand the Problem

Start from the distributed systems problem, not from the syntax.

#### Step 1: Identify Safety Properties

Ask: **"What failure are you trying to prevent?"**

The answer maps directly to `always` assertions in Fizzbee:

| Failure to Prevent | Safety Property | Fizzbee Assertion |
|---|---|---|
| Two leaders at once | Leader uniqueness | `always(at_most_one_leader)` |
| Committed data lost | Durability | `always(committed_implies_durable)` |
| Conflicting decisions | Agreement | `always(agreement)` |
| Inconsistent replicas after commit | Consistency | `always(committed_values_match)` |

#### Step 2: Identify Liveness Properties

Ask: **"What must eventually happen?"**

The answer maps to `always eventually` or `eventually` assertions:

| Required Progress | Liveness Property | Fizzbee Assertion |
|---|---|---|
| Requests get responses | Request completion | `always eventually(all_requests_served)` |
| Leader elected after failure | Leader election | `always eventually(has_leader)` |
| All replicas converge | Convergence | `eventually(replicas_converged)` |
| Transactions commit or abort | Termination | `always eventually(decided)` |

#### Step 3: Determine Architecture

Walk through these decisions to determine the specification structure:

**Roles (processes)**:
- "How many distinct types of participants exist?" -- Each becomes a `role` in Fizzbee
- "How many instances of each?" -- Sets the role count
- Examples: Leader/Follower, Proposer/Acceptor, Coordinator/Participant

**Communication**:
- "How do participants communicate?" -- Maps to channel declarations
- "What delivery guarantees does the real network provide?"
  - Reliable, ordered delivery -> `channel` (default)
  - Reliable, unordered delivery -> `channel` with unordered option
  - Unreliable delivery (message loss) -> lossy channel
  - See `references/fault-injection.md` for channel configuration details

**Action structure**:
- "Which operations are truly atomic in the real system?" -> `atomic` actions
- "Which operations can be interrupted?" -> `serial` actions (default)
- "Which operations happen concurrently?" -> `parallel` actions
- See `knowledge/fizzbee-fundamentals.md` for action modifier semantics

#### Step 4: Write the Specification

Use the templates in `references/spec-template.md` as starting points.

**File structure** for a `.fizz` file:

```
# 1. State variables (top-level assignments)
# 2. Assertions (always/eventually blocks)
# 3. Init block
# 4. Actions (top-level functions)
# 5. Helper functions
# 6. Role definitions (if multi-process)
```

**Key decisions during authoring**:

- **State granularity**: Model only the state relevant to the properties you are checking. Abstract away implementation details.
- **Nondeterminism**: Use `oneof` for nondeterministic choice between code blocks, `any` for nondeterministic selection from a set.
- **Fairness**: Add `fair` to actions that the system guarantees will eventually execute (e.g., message delivery on reliable channels). Without `fair`, the model checker may find spurious counterexamples where an action is never taken.
- **Preconditions**: Use `requires` to constrain when actions can fire.
- **Interleaving**: Every `yield` point and every action boundary is a point where other actions can interleave. Make sure `atomic` blocks are truly atomic in the real system.

See `knowledge/fizzbee-fundamentals.md` for complete syntax reference.

#### Step 5: Run the Model Checker

```bash
fizz verify path/to/spec.fizz
```

**Interpreting results**:

- **No errors**: All assertions hold for all explored states. Check the state count -- if it is very small, your model may be too constrained.
- **Assertion violation**: The model checker found a counterexample. Read the trace carefully:
  1. What is the initial state?
  2. What sequence of actions leads to the violation?
  3. Which action is the "culprit" -- the one that breaks the invariant?
  4. Is this a real bug, or is the model too permissive (missing a precondition)?
- **State explosion**: If the state space is too large, reduce:
  - Number of role instances
  - Range of nondeterministic choices
  - Size of data structures (e.g., log length)

#### Step 6: Iterate

After fixing issues found by the model checker:
1. Re-run verification
2. Consider adding more assertions to cover additional properties
3. Consider relaxing constraints to explore more failure scenarios
4. Review channel configuration -- try lossy channels to test fault tolerance

### Phase 2: Refine and Harden

Once the basic specification verifies:

1. **Add fault injection**: Switch to lossy/unordered channels. See `references/fault-injection.md`.
2. **Increase instance counts**: Verify with 3 nodes, then 5.
3. **Add crash-recovery**: Model node crashes and restarts.
4. **Review with the checklist**: Use `references/review-checklist.md`.

---

## Workflow 2: Review a Fizzbee Model

Use this workflow when reviewing an existing `.fizz` specification for correctness and completeness. Launches 3 review agents in parallel, each analyzing from a different perspective, then aggregates findings into a unified report.

### Step 1: Understand Intent

Before launching agents, read the specification and answer:
- What distributed system does this model?
- What properties is it trying to verify?
- What assumptions does it make about the environment (network, failures, timing)?

This context is passed to all three agents.

### Step 2: Launch Review Agents

Launch 3 agents in parallel using the Agent tool (`subagent_type: general-purpose`). Each agent should read the `.fizz` files and the relevant sections of `references/review-checklist.md`. Pass the intent context from Step 1 to each agent.

**Agent 1: Correctness & Assertions**
- Focus: Safety/liveness assertions, assertion strength and correctness, fairness conditions
- Checklist sections: Correctness of Assertions (Safety Properties, Liveness Properties, Assertion Strength), Fairness (Fair Actions, Fairness and Liveness Interaction)
- Key questions: Does every requirement have a corresponding assertion? Are assertions strong enough without being stronger than the real guarantees? Are fairness annotations correct?

**Agent 2: Completeness & Modeling**
- Focus: Roles/processes, message types, actions, failure modes, channel configuration, action modifiers, yield points
- Checklist sections: Completeness (Roles and Processes, Message Types, Actions, Failure Modes), Channel Configuration (Delivery Guarantees, Progressive Testing, Bounded Channels), Action Modifiers (Atomicity, Parallel Actions, Yield Points)
- Key questions: Are all participant types and message types modeled? Do channel types match the real network? Are action modifiers (atomic/serial/parallel) correct for the real system's atomicity?

**Agent 3: State Space & Quality**
- Focus: State space bounds and sizing, symmetry, common coding mistakes
- Checklist sections: State Space Management (Bounds, State Space Size, Symmetry), Common Mistakes (Missing Yield Points, Wrong Action Modifier, Overly Permissive/Restrictive Channels, Missing Preconditions, Incomplete Init, Global Keyword Missing, Unbounded State Growth)
- Key questions: Is the state space finite and reasonably sized? Are there common Fizzbee coding errors? Does the model checker complete in reasonable time?

Each agent should return structured findings: `[severity (critical/warning/info), category, description, recommendation]`.

### Step 3: Aggregate and Report

Collect findings from all three agents. Deduplicate overlapping issues. Sort by severity (critical > warning > info). Present using the review report template in `references/review-checklist.md`.

---

## Workflow 3: Model-Based Testing

Bridge a verified Fizzbee specification to a Go implementation.

### Step 1: Assess Readiness

Before generating tests, verify:
- The specification passes all assertions with `fizz verify`
- The specification actions map cleanly to implementation functions
- The Go implementation structure matches the spec's role/action decomposition

### Step 2: Connect Spec to Implementation

For each action in the spec, identify the corresponding Go function:

```
Spec Action          -> Go Function
RequestVote          -> node.HandleRequestVote()
AppendEntries        -> node.HandleAppendEntries()
ClientRequest        -> node.HandleClientRequest()
HandleTimeout        -> node.HandleElectionTimeout()
```

### Step 3: Generate Test Harness

Fizzbee's MBT generates test traces from the verified model. Each trace is a sequence of actions that the model checker proved is valid.

See `knowledge/fizzbee-tooling.md` for detailed MBT setup instructions:
- Configuring the MBT output format
- Writing adapter code that maps spec actions to Go function calls
- Handling state comparison between spec and implementation
- Running the generated tests

### Step 4: Interpret Results

- **Test passes**: The implementation matches the spec for this trace
- **Test fails**: Either the implementation has a bug, or the spec-to-implementation mapping is incorrect
- **Action not mapped**: A spec action has no corresponding implementation -- this may indicate missing functionality or an abstraction mismatch

### Step 5: Iterate

1. Fix implementation bugs found by MBT
2. Add more assertions to the spec to generate more diverse traces
3. Increase state space bounds to generate longer traces
4. Re-run tests after implementation changes

---

## Knowledge Base

### Fundamentals
- **`knowledge/fizzbee-fundamentals.md`**: Complete Fizzbee language reference
  - Starlark syntax, state variables, actions, roles, channels
  - Action modifiers (atomic, serial, parallel)
  - Assertions (always, eventually, always eventually)
  - Nondeterminism (oneof, any), fairness, preconditions

### Comparisons
- **`knowledge/fizzbee-vs-tlaplus.md`**: Decision guide for choosing between Fizzbee and TLA+
  - Syntax, learning curve, expressiveness
  - Tooling ecosystem comparison
  - Decision matrix by use case

### Tooling
- **`knowledge/fizzbee-tooling.md`**: Fizzbee tooling reference
  - CLI commands, online playground, VS Code extension
  - Model-based testing setup and usage
  - State graph visualization, CI/CD integration

### References
- **`references/protocol-patterns.md`**: Protocol modeling patterns (Raft, Paxos, 2PC, replication)
- **`references/fault-injection.md`**: Fault injection via channel configuration
- **`references/spec-template.md`**: Starter templates for new specifications
- **`references/review-checklist.md`**: Comprehensive review checklist

---

## License

Apache License 2.0 -- See LICENSE.txt for details.

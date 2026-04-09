# Fizzbee Specification Review Checklist

Structured checklist for reviewing Fizzbee specifications. Work through each section systematically.

## Agent Assignment

When used with the multi-agent review workflow (SKILL.md Workflow 2), sections are assigned as follows:

| Agent | Sections |
|-------|----------|
| **Agent 1: Correctness & Assertions** | Correctness of Assertions (Safety, Liveness, Assertion Strength), Fairness (Fair Actions, Fairness and Liveness Interaction) |
| **Agent 2: Completeness & Modeling** | Completeness (Roles and Processes, Message Types, Actions, Failure Modes), Channel Configuration, Action Modifiers |
| **Agent 3: State Space & Quality** | State Space Management (Bounds, Size, Symmetry), Common Mistakes |

---

## 1. Completeness

### Roles and Processes

- [ ] All distinct participant types in the real system are modeled as roles
- [ ] Role instance counts are sufficient to expose bugs (typically 3 for quorum-based protocols, 2 for client-server)
- [ ] Each role has an `init` block that sets all state variables to well-defined values

### Message Types

- [ ] All message types exchanged in the real protocol are modeled
- [ ] Message fields contain all information needed for the receiver to process them
- [ ] Response messages include enough context for the sender to match them to requests (e.g., term number, request ID)

### Actions

- [ ] All state transitions in the real system have corresponding actions
- [ ] Timeout/failure actions are modeled (election timeout, heartbeat timeout, crash, etc.)
- [ ] Client interactions are modeled if relevant to the properties being verified

### Failure Modes

- [ ] Node crashes are modeled (crash-stop or crash-recovery, as appropriate)
- [ ] Network failures are modeled via channel configuration (lossy, reordering)
- [ ] The failure model matches the real system's assumptions (e.g., f < n/2 for majority-based protocols)

---

## 2. Correctness of Assertions

### Safety Properties

- [ ] Every "must never happen" requirement has an `always` assertion
- [ ] Safety assertions check the actual invariant, not a weaker condition
  - Example: `assert leaders <= 1` is correct; `assert leaders < 3` is too weak for a 3-node cluster
- [ ] Safety assertions cover all roles/instances, not just a subset
- [ ] Assertion predicates are pure functions (no side effects, no state modification)

### Liveness Properties

- [ ] Every "must eventually happen" requirement has an `eventually` or `always eventually` assertion
- [ ] Liveness assertions are achievable under the modeled fairness assumptions
- [ ] `always eventually` is used (not just `eventually`) for properties that must hold repeatedly
- [ ] Liveness is not asserted under crash-stop failures where it genuinely cannot hold

### Assertion Strength

- [ ] Assertions are as strong as the real system's guarantees (not weaker)
- [ ] Assertions are not stronger than the real system's guarantees (would cause false failures)
- [ ] Multiple related assertions are not redundant (each tests something distinct)

---

## 3. Channel Configuration

### Delivery Guarantees

- [ ] Channel types match the real network's delivery guarantees
- [ ] FIFO channels are only used where the real transport guarantees ordering (e.g., TCP)
- [ ] Lossy channels are used where the real network can drop messages
- [ ] If the protocol must tolerate message reordering, channels are not FIFO
- [ ] If the protocol must handle duplicates, channels are configured with `duplicating=True` or the handler is tested for idempotency

### Progressive Testing

- [ ] The spec has been verified with the most permissive (hostile) channel configuration the real system must tolerate
- [ ] If currently using reliable channels, document why and whether lossy channels should be tested

### Bounded Channels

- [ ] If using bounded channels (`capacity=N`), the bound reflects real system constraints
- [ ] Unbounded channels (default) are acceptable if the real system has no backpressure issues being modeled

---

## 4. Action Modifiers

### Atomicity

- [ ] `atomic` actions correspond to operations that are truly atomic in the real system
  - Atomic in practice: single database write, CAS operation, holding a lock for the entire operation
  - NOT atomic in practice: multi-step RPC, operations that release and reacquire locks
- [ ] No `atomic` blocks that hide real concurrency bugs
- [ ] `serial` (default) is used for operations where interleaving can occur between steps

### Parallel Actions

- [ ] `parallel` is used where the real system performs operations concurrently
- [ ] Parallel actions do not share mutable state without proper synchronization modeled

### Yield Points

- [ ] `yield` points in serial actions correspond to real interleaving points
- [ ] No missing yields that would hide concurrency bugs
- [ ] No unnecessary yields that would explode the state space without finding real bugs

---

## 5. Fairness

### Fair Actions

- [ ] `fair` is applied to actions that the real system guarantees will eventually execute
  - Message delivery on reliable channels: should be `fair`
  - Timeout handlers: should be `fair` (OS guarantees timeout fires)
  - Client actions: typically NOT fair (client may not send more requests)
- [ ] No `fair` on actions that can genuinely be starved in the real system
- [ ] Liveness assertions that fail only with `fair` removed are documented as depending on the fairness assumption

### Fairness and Liveness Interaction

- [ ] Without fairness, only safety properties should be expected to hold
- [ ] Liveness failures are re-checked: is the failure due to missing `fair` or a genuine liveness bug?

---

## 6. State Space Management

### Bounds

- [ ] State space is finite and bounded
- [ ] Instance counts are the minimum needed to expose bugs (3 is usually enough for majority-based protocols)
- [ ] Data structures are bounded (log length, queue size, counter range)
- [ ] Nondeterministic choices (`any`, `oneof`) explore a reasonable range

### State Space Size

- [ ] The model checker completes in reasonable time (minutes, not hours)
- [ ] If state space is too large, identify which dimension to reduce:
  - Fewer role instances
  - Smaller data bounds
  - Fewer nondeterministic choices
  - More atomic blocks (if justified by the real system)
- [ ] If state space is very small (< 100 states), the model may be too constrained

### Symmetry

- [ ] Symmetric roles use the same role definition (not copy-pasted with different names)
- [ ] If applicable, symmetry reduction is noted

---

## 7. Common Mistakes

### Missing Yield Points

**Symptom**: The model checker does not find bugs that should exist.

**Cause**: A `serial` action does not have yields between operations that can be interrupted in the real system.

**Fix**: Add explicit `yield` at real interleaving points, or change the action to `serial` (default yields between every statement).

### Wrong Action Modifier

**Symptom**: The model is either too permissive (finds false bugs) or too restrictive (misses real bugs).

**Cause**: `atomic` used for non-atomic operations, or `serial` used for truly atomic operations.

**Fix**: Review each action: "In the real system, can another operation interleave between these two statements?" If yes, the action should be `serial`. If no, it should be `atomic`.

### Overly Permissive Channels

**Symptom**: The model checker finds valid counterexamples that do not correspond to real failures.

**Cause**: Using `channel(lossy=True)` when the real system uses TCP (reliable delivery).

**Fix**: Match channel configuration to real network guarantees. Document which failures are assumed possible.

### Overly Restrictive Channels

**Symptom**: The model checker passes, but the real system has bugs due to message loss or reordering.

**Cause**: Using `channel(fifo=True)` when the real system can experience reordering at the application level.

**Fix**: Test with more permissive channels. If the protocol should tolerate message loss, use `channel(lossy=True)`.

### Missing Preconditions

**Symptom**: The model checker explores impossible states (e.g., a follower sending heartbeats).

**Cause**: Actions fire in states where they should not.

**Fix**: Add `requires` clauses to constrain when actions can fire. Check each action: "Under what conditions does this action make sense in the real system?"

### Incomplete Init

**Symptom**: State variables have unexpected initial values; assertions fail in the initial state.

**Cause**: Not all state variables are set in `init`.

**Fix**: Initialize every state variable explicitly in `init`. Do not rely on default values.

### Global Keyword Missing

**Symptom**: State variables are not updated; actions appear to have no effect.

**Cause**: Forgetting `global` declaration when modifying top-level state variables in actions.

**Fix**: Always declare `global var_name` before modifying a top-level state variable inside an action.

### Unbounded State Growth

**Symptom**: Model checker runs forever or runs out of memory.

**Cause**: Lists, dicts, or counters grow without bound.

**Fix**: Add bounds: limit log length, queue size, counter range. Use `requires(len(log) < MAX_LOG)` to cap growth.

---

## Review Report Template

After completing the review, summarize findings:

```
## Specification Review: [Spec Name]

### Summary
- Spec file: [path]
- Protocol: [what it models]
- Verification result: [PASS/FAIL with details]
- States explored: [number]

### Properties Verified
1. [Property name]: [PASS/FAIL] - [description]
2. [Property name]: [PASS/FAIL] - [description]

### Assumptions
- Network: [channel configuration and rationale]
- Failures: [what failures are modeled]
- Fairness: [what fairness assumptions are made]

### Findings
- [Issue 1]: [severity] - [description and recommendation]
- [Issue 2]: [severity] - [description and recommendation]

### Recommendations
1. [Recommendation 1]
2. [Recommendation 2]
```

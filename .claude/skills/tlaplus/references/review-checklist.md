# TLA+ Specification Review Checklist

Structured checklist for reviewing TLA+ and PlusCal specifications. Work through each category systematically.

## Agent Assignment

When used with the multi-agent review workflow (SKILL.md Workflow 2), sections are assigned as follows:

| Agent | Sections |
|-------|----------|
| **Agent 1: Correctness & Properties** | Correctness (Safety, Liveness, Initial State, Actions), Fairness (WF, SF, No Fairness) |
| **Agent 2: Completeness & Modeling** | Completeness (Actions and Transitions, Failure Modes, State Coverage), Abstraction Level (Too Detailed, Too Abstract, Right Level) |
| **Agent 3: Configuration & Quality** | Configuration (Constants, Symmetry, Constraints, Deadlock), Common Mistakes, Final Verification Steps |

## Completeness

### Actions and Transitions

- [ ] Are all relevant protocol actions modeled? (e.g., propose, accept, commit, abort, timeout, recovery)
- [ ] Are all message types defined and handled?
- [ ] Can every message type be sent AND received? (Check for dead message types)
- [ ] Are client interactions modeled if relevant?
- [ ] Is there a stuttering step or is termination explicitly handled?

### Failure Modes

- [ ] Are crash failures modeled if the real system can crash?
- [ ] Are network failures modeled? (message loss, reordering, duplication, partition)
- [ ] Are timeout actions included where the real system uses timeouts?
- [ ] Is crash-recovery modeled if the real system has persistent state?
- [ ] Are Byzantine faults modeled if that is part of the threat model?
- [ ] Can failures occur at any point, or only at unrealistic "convenient" times?

### State Coverage

- [ ] Does every variable get modified by at least one action?
- [ ] Are there reachable dead states (states where no action is enabled and the system is not in a terminal state)?
- [ ] Run with `-coverage 1` to check: are all actions exercised?

## Correctness

### Safety Properties

- [ ] Is `TypeOK` defined and listed as an invariant?
- [ ] Does `TypeOK` cover ALL variables?
- [ ] Do safety invariants accurately express the real-world requirements?
- [ ] Are invariants strong enough? (A trivially true invariant like `TRUE` is useless)
- [ ] Could the invariant pass even when the protocol is clearly wrong? (Test by introducing a deliberate bug)
- [ ] For agreement properties: does the invariant cover ALL pairs of processes, not just adjacent ones?

### Liveness Properties

- [ ] Are liveness properties specified if the system has progress requirements?
- [ ] Do liveness properties use `<>` (eventually), `~>` (leads-to), or `[]<>` (infinitely often) correctly?
- [ ] Are liveness properties checked with `PROPERTY` in the .cfg, not `INVARIANT`?
- [ ] Is the specification formula using `SPECIFICATION Spec` (not `INIT/NEXT`) when checking liveness?

### Initial State

- [ ] Does `Init` set ALL variables to well-defined values?
- [ ] Is the initial state realistic? (e.g., no node starts as leader unless that is the protocol's design)
- [ ] Are nondeterministic initial values intentional? (`x \in S` in Init)

### Actions

- [ ] Does every action specify the new value of EVERY variable (either update or `UNCHANGED`)?
- [ ] Are preconditions (guards) correct and complete?
- [ ] Do actions correctly model atomicity? (What happens in one step vs multiple steps)
- [ ] Are there race conditions between actions that the model captures?

## Abstraction Level

### Too Detailed (State Explosion)

- [ ] Are there variables that do not affect the properties being checked? (Remove them)
- [ ] Is the network model more detailed than necessary? (e.g., sequences when sets suffice)
- [ ] Are there unnecessary intermediate steps that could be combined?
- [ ] Is there computation that could be replaced by nondeterministic choice?

### Too Abstract (Missing Bugs)

- [ ] Does the model capture the essential concurrency of the real system?
- [ ] Are critical ordering constraints preserved?
- [ ] Could a real bug be hidden by overly coarse atomicity?
- [ ] Does the model allow the failure scenarios that motivate the protocol?
- [ ] Test: can you reproduce known bugs from the protocol's history in this model?

### Right Level

- [ ] Does each variable correspond to real system state or a necessary specification artifact?
- [ ] Is the message granularity appropriate? (One message type per real network message)
- [ ] Does the atomicity match what the real implementation provides? (e.g., disk writes, CAS operations)

## Fairness

### Weak Fairness (WF)

- [ ] Is weak fairness applied to actions that must eventually execute in a correct system?
- [ ] Is `WF_vars(Next)` sufficient, or do individual actions need separate fairness?
- [ ] Does weak fairness match the real system? (A server that can always make progress should have WF)

### Strong Fairness (SF)

- [ ] Is strong fairness used for actions that may be transiently disabled but must still eventually happen?
- [ ] Example: message receive is repeatedly enabled (messages arrive) but may be interrupted -- needs SF
- [ ] Is SF used only where needed? (SF is stronger than WF and can hide real problems)

### No Fairness

- [ ] Are there actions that intentionally have no fairness? (e.g., crash actions, partition changes)
- [ ] Crashes should typically NOT have fairness (we do not require crashes to happen)

## Configuration

### Constants

- [ ] Are constant values large enough to exercise the protocol meaningfully?
- [ ] Minimum recommendations:
  - Nodes/servers: at least 3 for quorum-based protocols
  - Values: at least 2 for agreement properties
  - Rounds/terms: at least 2 for leader-change scenarios
  - Log entries: at least 2 for log-matching properties
- [ ] Are constant values small enough that TLC finishes in reasonable time?

### Symmetry

- [ ] Is symmetry reduction applied to model-value sets? (Servers, Values)
- [ ] Is symmetry NOT used with liveness properties?
- [ ] Is the specification actually symmetric? (No asymmetric initial state, no hardcoded server references)

### Constraints

- [ ] Are state constraints (`CONSTRAINT`) documented with justification?
- [ ] Could the constraint hide a real bug?
- [ ] Has the model been run without constraints at a smaller scale to validate?

### Deadlock

- [ ] Is deadlock detection appropriate? (Disable with `-deadlock` only if termination is expected)
- [ ] If deadlock is found, is it real deadlock or expected termination?

## Common Mistakes

### Missing UNCHANGED

The most common TLA+ bug. Every action must specify what happens to every variable.

- [ ] Check each action: does it mention every variable in either a primed assignment or UNCHANGED?
- [ ] Search for actions that update some variables but forget others

### Forgotten Variables in vars Tuple

- [ ] Does the `vars` tuple include ALL declared variables?
- [ ] Are newly added variables also added to `vars`?

### PlusCal Label Issues

- [ ] Is there a label at the start of every process body?
- [ ] Is there a label at the start of every `while` loop body?
- [ ] Is there a label after every `call` statement?
- [ ] Are macros free of labels?
- [ ] Is atomicity (label granularity) appropriate for the property being checked?

### Deadlock vs Termination

- [ ] If the protocol terminates, is termination distinguished from deadlock?
- [ ] In PlusCal, processes that terminate normally reach the `Done` label
- [ ] If checking for deadlock, make sure non-terminating processes have a `while TRUE` loop

### Set vs Function Confusion

- [ ] `\in` for membership test vs `=` for equality
- [ ] `[S -> T]` is the SET of all functions from S to T
- [ ] `[s \in S |-> expr]` is a SPECIFIC function
- [ ] Function application uses `f[x]`, not `f(x)` (which is operator application)

### Quantifier Scoping

- [ ] `\E x \in S : P /\ Q` -- does Q depend on x? If not, it might be a scoping error
- [ ] Parenthesize when mixing quantifiers with other operators

### CHOOSE Pitfalls

- [ ] Does the CHOOSE set always have at least one element? (Empty set causes runtime error)
- [ ] Is CHOOSE used for nondeterminism? It should not be -- CHOOSE is deterministic. Use `\E` or `with` instead.

## Final Verification Steps

- [ ] Run TLC with TypeOK as invariant -- does it pass?
- [ ] Run TLC with all safety invariants -- do they pass?
- [ ] Run TLC with liveness properties (if any) -- do they pass?
- [ ] Introduce a deliberate bug (e.g., remove a guard) -- does TLC catch it?
- [ ] Check coverage output -- are all actions explored?
- [ ] Review the counterexample trace if TLC finds a violation
- [ ] Consider: does the model tell you something new about the protocol, or just confirm what you already knew?

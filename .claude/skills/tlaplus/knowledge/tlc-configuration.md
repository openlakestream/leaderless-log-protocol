# TLC Configuration Reference

TLC is the model checker for TLA+ specifications. This document covers configuration files, command-line options, and alternative tools.

## Configuration File (.cfg)

The `.cfg` file tells TLC what to check. It must have the same base name as the `.tla` file (e.g., `Spec.tla` uses `Spec.cfg`), or be specified with `-config`.

### Basic Structure

```
\* Spec.cfg

INIT Init
NEXT Next

CONSTANTS
    Server = {s1, s2, s3}
    Value = {v1, v2}
    MaxTerm = 3
    Nil = Nil

INVARIANT TypeOK
INVARIANT Safety

PROPERTY Liveness

SYMMETRY ServerSymmetry

CONSTRAINT StateConstraint

ACTION_CONSTRAINT ActionConstraint
```

### Directives

#### SPECIFICATION

Alternative to INIT/NEXT. Points to the complete temporal formula:

```
SPECIFICATION Spec
```

Where `Spec == Init /\ [][Next]_vars /\ WF_vars(Next)` in the TLA+ module.

#### INIT and NEXT

Separate initial state predicate and next-state relation:

```
INIT Init
NEXT Next
```

#### CONSTANTS

Assign values to declared constants:

```
CONSTANTS
    Server = {s1, s2, s3}
    Value = {v1, v2}
    MaxTerm = 3
    Nil = Nil               \* model value
    Quorum <- QuorumDef     \* use operator QuorumDef from the spec
```

Model values are uninterpreted constants. Use `Nil = Nil` to create a model value named `Nil` that is distinct from all other values.

Symmetry sets: `Server = {s1, s2, s3}` creates a set of model values. These can be declared symmetric.

#### INVARIANT

State invariants (safety properties). TLC checks these hold in every reachable state:

```
INVARIANT TypeOK
INVARIANT NoTwoLeadersInSameTerm
INVARIANT LogMatching
```

#### PROPERTY

Temporal properties (including liveness). TLC checks these hold for all behaviors:

```
PROPERTY Liveness
PROPERTY EventualConsistency
```

#### SYMMETRY

Declares a symmetry set to reduce state space. The named operator must evaluate to a set of permutations:

```
SYMMETRY ServerSymmetry
```

In the spec:
```tla
ServerSymmetry == Permutations(Server)
```

This tells TLC that all permutations of `Server` values produce equivalent states, so only one representative per equivalence class needs to be explored.

Requirements for sound symmetry reduction:
- The spec must be truly symmetric with respect to the set
- Invariants and properties must also be symmetric
- Do not use symmetry with liveness properties (can cause false positives)

#### CONSTRAINT

State constraint: TLC only explores states satisfying this predicate. Used to bound the state space:

```
CONSTRAINT StateConstraint
```

In the spec:
```tla
StateConstraint ==
    /\ \A s \in Server : currentTerm[s] <= MaxTerm
    /\ \A s \in Server : Len(log[s]) <= MaxLogLen
    /\ Cardinality(messages) <= MaxMessages
```

Warning: constraints can hide bugs if they prevent TLC from reaching violating states.

#### ACTION_CONSTRAINT

Constrains which actions TLC explores. Unlike CONSTRAINT (which filters states), this filters transitions:

```
ACTION_CONSTRAINT ActionConstraint
```

In the spec:
```tla
ActionConstraint ==
    Cardinality(messages') <= MaxMessages
```

## Command-Line Options

### Basic Usage

```bash
java -jar tla2tools.jar [options] SpecName.tla
```

### Common Options

| Option | Description |
|--------|-------------|
| `-workers N` | Number of parallel worker threads (default: 1). Use number of CPU cores. |
| `-config file.cfg` | Specify configuration file (default: SpecName.cfg) |
| `-deadlock` | Do not check for deadlock |
| `-depth N` | Maximum depth of state graph exploration |
| `-seed N` | Random seed for simulation mode |
| `-terse` | Terse output |
| `-cleanup` | Clean up temporary files after run |
| `-dump format file` | Dump state graph (format: dot, json) |
| `-continue` | Continue checking after finding first violation |
| `-coverage N` | Print coverage statistics every N minutes |
| `-checkpoint N` | Checkpoint every N minutes (for recovery) |
| `-recover path` | Recover from a checkpoint |
| `-noGenerateSpecTE` | Do not generate a trace explorer spec on error |

### Memory and Performance

```bash
# Set Java heap size (critical for large models)
java -Xmx8g -jar tla2tools.jar -workers 8 Spec.tla

# Use off-heap storage for large state sets
java -XX:MaxDirectMemorySize=4g -jar tla2tools.jar -workers 8 Spec.tla
```

### Depth-First Iterative Deepening (-dfid)

Explores states depth-first up to increasing depth limits. Uses less memory than BFS.

```bash
java -jar tla2tools.jar -dfid 30 Spec.tla
```

When to use:
- When BFS runs out of memory
- When bugs are expected at shallow depths
- Not suitable for liveness checking

### Simulation Mode (-simulate)

Random walk through the state space. Does not guarantee completeness but can find bugs in huge state spaces.

```bash
# Random simulation with depth limit
java -jar tla2tools.jar -simulate -depth 100 Spec.tla

# Run N traces
java -jar tla2tools.jar -simulate num=10000 -depth 100 Spec.tla

# With specific seed (reproducible)
java -jar tla2tools.jar -simulate -seed 42 -depth 100 Spec.tla
```

When to use:
- State space is too large for exhaustive checking
- Quick smoke test before committing to a long run
- Finding bugs in deep execution paths

### Liveness Checking

Liveness checking requires additional options because TLC must find cycles in the state graph:

```bash
# Standard liveness check (enabled by default when PROPERTY is in cfg)
java -jar tla2tools.jar -workers 4 Spec.tla

# Liveness checking mode
java -jar tla2tools.jar -lncheck final Spec.tla
```

Note: Liveness checking with `-dfid` is not supported — DFID explores bounded depths and cannot detect the full cycles in the state graph that liveness checking requires (BFS or complete DFS is needed). Symmetry reduction with liveness can produce false counterexamples because symmetry maps distinct states to the same equivalence class, potentially creating spurious cycles.

## VS Code Extension

The TLA+ for VS Code extension provides:

- Syntax highlighting for TLA+ and PlusCal
- One-click PlusCal translation
- One-click TLC model checking
- Parse error highlighting
- Trace visualization

Install: Search "TLA+" in VS Code extensions marketplace.

### Extension Commands

- **TLA+: Parse Module** - Parse the current TLA+ file
- **TLA+: Check Model with TLC** - Run TLC on the current spec
- **TLA+: Translate PlusCal** - Translate PlusCal to TLA+

## Coverage Statistics

Run with `-coverage N` to get coverage statistics every N minutes:

```bash
java -jar tla2tools.jar -workers 4 -coverage 1 Spec.tla
```

Output shows:
- How many times each action was evaluated
- How many distinct states each action produced
- Actions that were never enabled (potential modeling errors)

Use coverage to identify:
- Actions that dominate state generation (optimization targets)
- Actions never taken (dead code in the spec)
- Unbalanced exploration

## Trace Exploration

When TLC finds a violation, it outputs a counterexample trace. Each step shows:
1. The state number
2. Which action was taken
3. The full state (all variable values)

### Reading Traces

Start from the **last state** (where the invariant breaks) and work backwards:
1. What invariant is violated?
2. What action led to the violating state?
3. What precondition allowed that action?
4. Trace back to find the root cause.

### State Dumps

```bash
# Dump state graph in DOT format (for visualization)
java -jar tla2tools.jar -dump dot states.dot Spec.tla

# Dump in JSON format
java -jar tla2tools.jar -dump json states.json Spec.tla
```

## Apalache: Symbolic Model Checker

Apalache is an alternative to TLC that uses SMT solving instead of explicit state enumeration.

### Key Differences from TLC

| Feature | TLC | Apalache |
|---------|-----|----------|
| Approach | Explicit state enumeration | Symbolic (SMT-based) |
| State space | Must be finite | Can handle unbounded |
| Speed | Fast for small models | Better for some large models |
| Completeness | Complete (for bounded) | Bounded model checking |
| Type system | Dynamic | Static (requires type annotations) |

### Usage

```bash
# Check an invariant
apalache-mc check --inv=Safety Spec.tla

# Bounded model checking (up to k steps)
apalache-mc check --length=10 --inv=Safety Spec.tla

# Type check only
apalache-mc typecheck Spec.tla
```

Apalache requires type annotations:

```tla
\* @type: Set(SERVER) => Set(Set(SERVER));
Quorum(S) == {Q \in SUBSET S : Cardinality(Q) * 2 > Cardinality(S)}
```

## TLAPS: TLA+ Proof System

TLAPS enables machine-checked proofs of TLA+ theorems. Unlike model checking (which verifies for specific constants), proofs verify for all possible values.

### When to Use TLAPS

- When model checking is infeasible (unbounded parameters)
- When you need mathematical certainty, not just testing
- For critical safety properties of production protocols

### Basic Structure

```tla
THEOREM Safety == Spec => []Inv
PROOF
    <1>1. Init => Inv
        BY DEF Init, Inv
    <1>2. Inv /\ [Next]_vars => Inv'
        <2>1. CASE Action1
            BY <2>1 DEF Action1, Inv
        <2>2. CASE Action2
            BY <2>2 DEF Action2, Inv
        <2>3. CASE UNCHANGED vars
            BY <2>3 DEF Inv, vars
        <2>4. QED
            BY <2>1, <2>2, <2>3 DEF Next
    <1>3. QED
        BY <1>1, <1>2, PTL DEF Spec
```

TLAPS is powerful but has a steep learning curve. Start with model checking; move to proofs only when needed.

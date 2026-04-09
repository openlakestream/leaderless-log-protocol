---
description: Create or edit a TLA+ .tla specification from distributed systems requirements
argument-hint: <spec-file.tla>
allowed-tools: [Read, Glob, Grep, Write, Edit, Agent]
---

# TLA+: Create or Edit Specification

Follow **Workflow 1** from the TLA+ skill to create or edit a TLA+ specification.

## Target File

The target specification file is: `$ARGUMENTS`

If no argument was provided, ask the user which `.tla` file to create or edit. If the file already exists, read it first to understand the current state.

## Workflow

Execute the workflow from the TLA+ SKILL.md:

### Step 1: Identify What Can Go Wrong

1. **Safety Properties** — Ask: "What failure are you trying to prevent?" Map to invariants.
2. **Liveness Properties** — Ask: "What must eventually happen?" Map to temporal formulas.
3. **Failure Modes** — Determine: crash-stop, crash-recovery, network partition, message loss/duplication/reordering, Byzantine faults.

### Step 2: Decide PlusCal vs Raw TLA+

- **PlusCal**: Clear sequential/multi-process structure, imperative pseudocode, labeled atomicity
- **Raw TLA+**: Declarative specs, flexible next-state relation, composed sub-actions, shared-memory concurrency

### Step 3: Define the State Space

Identify per-node state, network state, and global/auxiliary state. Keep variables minimal — every variable multiplies the state space.

### Step 4: Write the Specification

Follow the module structure:
- `EXTENDS`, `CONSTANTS`, `VARIABLES`
- `TypeOK` invariant
- `Init` predicate
- Individual actions
- `Next` state predicate (disjunction of actions)
- `Spec` with fairness
- Safety and liveness properties

### Step 5: Write the Configuration

Create a `.cfg` file with `INIT`, `NEXT`, `CONSTANTS`, `INVARIANT`, `PROPERTY`, and `SYMMETRY` declarations.

### Step 6: Run TLC and Iterate

Run the model checker, interpret violations, fix issues.

## Knowledge References

Read these files from the TLA+ skill for guidance:

- `knowledge/tlaplus-fundamentals.md` — TLA+ syntax and operators
- `knowledge/pluscal-guide.md` — PlusCal syntax and idioms
- `knowledge/tlc-configuration.md` — TLC configuration and CLI
- `knowledge/state-space-reduction.md` — State space reduction techniques
- `references/spec-template.md` — Starter templates
- `references/protocol-patterns.md` — Protocol modeling patterns
- `references/network-modeling.md` — Network and failure modeling

All paths are relative to the TLA+ skill directory (`.claude/skills/tlaplus/`).

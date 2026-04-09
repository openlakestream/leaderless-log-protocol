---
description: Create or edit a Fizzbee .fizz specification from distributed systems requirements
argument-hint: <spec-file.fizz>
allowed-tools: [Read, Glob, Grep, Write, Edit, Agent]
---

# Fizzbee: Create or Edit Specification

Follow **Workflow 1** from the Fizzbee skill to create or edit a Fizzbee specification.

## Target File

The target specification file is: `$ARGUMENTS`

If no argument was provided, ask the user which `.fizz` file to create or edit. If the file already exists, read it first to understand the current state.

## Workflow

Execute the phased workflow from the Fizzbee SKILL.md:

### Phase 1: Understand the Problem

1. **Identify Safety Properties** — Ask: "What failure are you trying to prevent?" Map answers to `always` assertions.
2. **Identify Liveness Properties** — Ask: "What must eventually happen?" Map answers to `always eventually` or `eventually` assertions.
3. **Determine Architecture** — Identify roles (processes), communication (channel types), and action structure (atomic/serial/parallel).
4. **Write the Specification** — Use the file structure from the skill:
   - State variables (top-level assignments)
   - Assertions (always/eventually blocks)
   - Init block
   - Actions (top-level functions)
   - Helper functions
   - Role definitions (if multi-process)
5. **Run the Model Checker** — Execute `fizzbee check` and interpret results.
6. **Iterate** — Fix issues, add assertions, relax constraints.

### Phase 2: Refine and Harden

1. Add fault injection (lossy/unordered channels)
2. Increase instance counts
3. Add crash-recovery modeling
4. Review with checklist

## Knowledge References

Read these files from the Fizzbee skill for guidance:

- `knowledge/fizzbee-fundamentals.md` — Complete language reference (syntax, actions, roles, channels, assertions)
- `references/spec-template.md` — Starter templates for new specifications
- `references/fault-injection.md` — Fault injection via channel configuration
- `references/review-checklist.md` — Comprehensive review checklist
- `references/protocol-patterns.md` — Protocol modeling patterns (Raft, Paxos, 2PC, replication)

All paths are relative to the Fizzbee skill directory (`.claude/skills/fizzbee/`).

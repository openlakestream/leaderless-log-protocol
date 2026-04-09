---
description: Review a TLA+ .tla spec with 3 parallel analysis agents
argument-hint: <spec-file.tla>
allowed-tools: [Read, Glob, Grep, Agent]
---

# TLA+: Review Specification

Follow **Workflow 2** from the TLA+ skill to review an existing `.tla` specification using 3 parallel review agents.

## Target File

The target specification file is: `$ARGUMENTS`

If no argument was provided, look for `.tla` files in the repository. If exactly one exists, use it. Otherwise, ask the user which file to review.

## Step 1: Understand Intent

Read the specification file and its `.cfg` configuration. Determine:
- What distributed system does this spec model?
- What properties does it claim to verify (safety and liveness)?
- What assumptions does it make about failures and the environment?

## Step 2: Launch 3 Review Agents in Parallel

Launch all 3 agents simultaneously using the Agent tool (`subagent_type: general-purpose`). Pass the intent context from Step 1 to each agent. Each agent must read the `.tla` file, its `.cfg` file, and the relevant sections of `references/review-checklist.md` (relative to `.claude/skills/tlaplus/`).

**Agent 1: Correctness & Properties**
- Focus: Safety/liveness properties, TypeOK, initial state correctness, action correctness, fairness conditions
- Checklist sections: Correctness (Safety Properties, Liveness Properties, Initial State, Actions), Fairness (WF, SF, No Fairness)
- Key questions: Do invariants express the requirements? Are fairness conditions appropriate? Does Init set all variables correctly?

**Agent 2: Completeness & Modeling**
- Focus: Action coverage, failure mode coverage, state coverage, abstraction level
- Checklist sections: Completeness (Actions and Transitions, Failure Modes, State Coverage), Abstraction Level
- Key questions: Are all protocol actions and failure modes modeled? Is the abstraction level right?

**Agent 3: Configuration & Quality**
- Focus: TLC configuration, common coding mistakes, verification steps
- Checklist sections: Configuration (Constants, Symmetry, Constraints, Deadlock), Common Mistakes (Missing UNCHANGED, Forgotten Variables, PlusCal Labels, Deadlock vs Termination, Set vs Function, Quantifier Scoping, CHOOSE Pitfalls), Final Verification Steps
- Key questions: Are constants sized correctly? Is symmetry applied where valid? Are there common TLA+ coding errors?

Each agent must return structured findings: `[severity (critical/warning/info), category, description, recommendation]`.

## Step 3: Aggregate and Report

Collect findings from all three agents. Deduplicate overlapping issues. Sort by severity (critical > warning > info). Present a unified report.

## Knowledge References

- `references/review-checklist.md` — Full review checklist and report template
- `knowledge/tlaplus-fundamentals.md` — Language reference for verifying correctness
- `knowledge/tlc-configuration.md` — TLC configuration reference

All paths are relative to the TLA+ skill directory (`.claude/skills/tlaplus/`).

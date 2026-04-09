---
description: Review a Fizzbee .fizz spec with 3 parallel analysis agents
argument-hint: <spec-file.fizz>
allowed-tools: [Read, Glob, Grep, Agent]
---

# Fizzbee: Review Specification

Follow **Workflow 2** from the Fizzbee skill to review an existing `.fizz` specification using 3 parallel review agents.

## Target File

The target specification file is: `$ARGUMENTS`

If no argument was provided, look for `.fizz` files in the repository. If exactly one exists, use it. Otherwise, ask the user which file to review.

## Step 1: Understand Intent

Read the specification file and determine:
- What distributed system does this model?
- What properties is it trying to verify?
- What assumptions does it make about the environment (network, failures, timing)?

## Step 2: Launch 3 Review Agents in Parallel

Launch all 3 agents simultaneously using the Agent tool (`subagent_type: general-purpose`). Pass the intent context from Step 1 to each agent. Each agent must read the `.fizz` file and the relevant sections of `references/review-checklist.md` (relative to `.claude/skills/fizzbee/`).

**Agent 1: Correctness & Assertions**
- Focus: Safety/liveness assertions, assertion strength, fairness conditions
- Checklist sections: Correctness of Assertions (Safety Properties, Liveness Properties, Assertion Strength), Fairness (Fair Actions, Fairness and Liveness Interaction)
- Key questions: Does every requirement have a corresponding assertion? Are assertions strong enough? Are fairness annotations correct?

**Agent 2: Completeness & Modeling**
- Focus: Roles/processes, message types, actions, failure modes, channel configuration, action modifiers, yield points
- Checklist sections: Completeness (Roles and Processes, Message Types, Actions, Failure Modes), Channel Configuration, Action Modifiers
- Key questions: Are all participant types modeled? Do channel types match the real network? Are action modifiers correct?

**Agent 3: State Space & Quality**
- Focus: State space bounds/sizing, symmetry, common coding mistakes
- Checklist sections: State Space Management, Common Mistakes
- Key questions: Is the state space finite and reasonably sized? Are there common Fizzbee coding errors?

Each agent must return structured findings: `[severity (critical/warning/info), category, description, recommendation]`.

## Step 3: Aggregate and Report

Collect findings from all three agents. Deduplicate overlapping issues. Sort by severity (critical > warning > info). Present a unified report.

## Knowledge References

- `references/review-checklist.md` — Full review checklist and report template
- `knowledge/fizzbee-fundamentals.md` — Language reference for verifying correctness

All paths are relative to the Fizzbee skill directory (`.claude/skills/fizzbee/`).

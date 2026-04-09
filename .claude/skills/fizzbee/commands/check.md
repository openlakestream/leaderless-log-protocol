---
description: Run fizzbee check on a .fizz file and interpret results
argument-hint: <spec-file.fizz>
allowed-tools: [Read, Glob, Grep, Bash]
---

# Fizzbee: Run Model Checker

Run `fizzbee check` on a Fizzbee specification and interpret the results.

## Target File

The target specification file is: `$ARGUMENTS`

If no argument was provided, look for `.fizz` files in the repository. If exactly one exists, use it. Otherwise, list available `.fizz` files and ask the user which to check.

## Step 1: Validate the File

Read the target `.fizz` file to confirm it exists and contains a valid-looking Fizzbee specification (has assertions, actions, etc.).

## Step 2: Run the Model Checker

```bash
fizzbee check <path-to-file.fizz>
```

If `fizzbee` is not installed, inform the user and provide installation instructions.

## Step 3: Interpret Results

### If all properties pass:
- Report the number of states explored
- If the state count is very small, warn that the model may be too constrained
- Summarize which safety and liveness properties were verified

### If an assertion is violated:
- Read the counterexample trace carefully
- Identify: What is the initial state? What sequence of actions leads to the violation? Which action breaks the invariant?
- Determine: Is this a real protocol bug, or is the model too permissive (missing a precondition)?
- Suggest specific fixes based on the violation type

### If state explosion occurs:
- Suggest reducing: number of role instances, range of nondeterministic choices, data structure sizes
- Recommend checking `knowledge/fizzbee-fundamentals.md` for state space management guidance

### If there are syntax or runtime errors:
- Parse the error message and suggest fixes
- Reference `knowledge/fizzbee-fundamentals.md` for correct syntax

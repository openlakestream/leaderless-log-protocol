---
description: Run TLC model checker on a .tla file and interpret results
argument-hint: <spec-file.tla> [-config <file.cfg>]
allowed-tools: [Read, Glob, Grep, Bash]
---

# TLA+: Run Model Checker

Run TLC on a TLA+ specification and interpret the results.

## Target File

The target specification file is: `$ARGUMENTS`

If no argument was provided, look for `.tla` files in the repository. If exactly one exists, use it. Otherwise, list available `.tla` files and ask the user which to check.

## Step 1: Validate and Find Configuration

1. Read the target `.tla` file to confirm it exists and is a valid TLA+ module.
2. Look for a matching `.cfg` file:
   - Same name as the `.tla` file (e.g., `Foo.tla` -> `Foo.cfg`)
   - If multiple `.cfg` files exist for the same module (e.g., `Foo.cfg`, `Foo-medium.cfg`), list them and ask which to use, or default to the smallest configuration.
   - If a `-config` flag was passed in `$ARGUMENTS`, use that config file.

## Step 2: Run TLC

```bash
java -jar tla2tools.jar -config <config-file> <spec-file.tla>
```

If `tla2tools.jar` is not found in the current directory, search common locations:
- `tlaplus/tla2tools.jar`
- `~/.tla+/tla2tools.jar`
- Check if `tlc` command is available

If TLC is not available, inform the user and suggest downloading from https://github.com/tlaplus/tlaplus/releases.

## Step 3: Interpret Results

### If all properties pass:
- Report the number of distinct states found and the diameter
- Summarize which invariants and temporal properties were verified
- If state count seems small relative to the configuration, note that the model may be under-constrained

### If an invariant is violated:
- Read the error trace from bottom to top (last state is where the invariant breaks)
- Identify which action transitioned into the bad state
- Determine: Is this a real protocol bug, or a modeling error?
- Common false positives: missing `UNCHANGED` for variables, over-broad nondeterminism, missing type constraints
- Suggest specific fixes

### If a temporal property is violated:
- Examine the lasso-shaped counterexample (prefix + loop)
- Check if fairness conditions are correctly specified
- Determine if the liveness violation is genuine or caused by missing fairness

### If TLC runs out of memory or is too slow:
- Suggest reducing constants (fewer servers, smaller value sets, lower bounds)
- Recommend adding `SYMMETRY` if applicable
- Suggest `STATE CONSTRAINT` to bound growing variables
- Recommend simulation mode: `java -jar tla2tools.jar -simulate -depth 100`
- Reference `knowledge/state-space-reduction.md` for detailed guidance

### If there are parse or evaluation errors:
- Parse the error message and suggest fixes
- Reference `knowledge/tlaplus-fundamentals.md` for correct syntax
- Check for common mistakes: missing `EXTENDS`, undefined operators, wrong module name

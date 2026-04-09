# Fizzbee Tooling Reference

Complete reference for Fizzbee tools, integrations, and workflows.

---

## CLI: `fizz` Command

### Installation

```bash
# Install via Go
go install github.com/fizzbee-io/fizzbee@latest

# Verify installation
fizz --version
```

### Core Commands

#### `fizz verify`

Run the model checker on a specification.

```bash
# Basic verification
fizz verify spec.fizz

# With increased state space bounds
fizz verify --max-states 100000 spec.fizz

# With verbose output (shows states explored)
fizz verify --verbose spec.fizz

# Output state graph
fizz verify --graph output.dot spec.fizz
```

**Output interpretation**:
- `PASS`: All assertions hold for all explored states
- `FAIL`: An assertion was violated; a counterexample trace follows
- `TIMEOUT` / `MAX_STATES`: State space not fully explored; increase bounds or reduce model size

#### `fizz run`

Run a single execution trace (useful for debugging).

```bash
fizz run spec.fizz
```

### CLI Options

| Flag | Description |
|---|---|
| `--max-states N` | Maximum number of states to explore |
| `--verbose` | Print progress during verification |
| `--graph FILE` | Output state graph in DOT format |
| `--trace` | Print detailed execution traces |
| `--seed N` | Random seed for simulation mode |

---

## Online Playground

The Fizzbee online playground at **https://fizzbee.io** provides:

- **Browser-based editor**: Write and edit `.fizz` specs without local installation
- **Instant verification**: Run the model checker directly in the browser
- **State graph visualization**: Interactive state space exploration
- **Shareable links**: Share specifications via URL
- **Example library**: Pre-built examples of common protocols

**Best for**:
- Quick experiments and prototyping
- Learning Fizzbee syntax
- Sharing specs with team members for review
- Trying Fizzbee before local installation

**Limitations**:
- State space bounds are more constrained than local CLI
- No MBT (model-based testing) support
- No CI/CD integration

---

## VS Code Extension

### Installation

Search for "Fizzbee" in the VS Code extension marketplace, or:

```bash
code --install-extension fizzbee.fizzbee-vscode
```

### Features

- **Syntax highlighting** for `.fizz` files
- **Inline diagnostics**: Syntax errors highlighted as you type
- **Run verification**: Execute `fizz verify` from the editor
- **Counterexample navigation**: Click through counterexample traces
- **Snippets**: Quick templates for common constructs (roles, channels, assertions)

---

## Model-Based Testing (MBT)

MBT bridges verified specifications to implementation tests. The core idea: the model checker generates valid execution traces (sequences of actions and states), and a test harness replays those traces against the real implementation.

### How MBT Works

```
                 fizz verify
.fizz spec  ──────────────────>  verified traces
                                      |
                                      v
                              test adapter code
                                      |
                                      v
                              Go test execution
                                      |
                                      v
                               PASS / FAIL
```

1. **Verification**: `fizz verify` explores the state space and records valid execution traces
2. **Trace extraction**: Each trace is a sequence of (state, action, next_state) tuples
3. **Adaptation**: Adapter code maps spec actions to Go function calls and spec state to Go struct fields
4. **Execution**: The test harness replays each trace, calling Go functions and asserting state matches

### Setting Up MBT for Go Projects

#### Step 1: Make Spec Actions Testable

Design spec actions to correspond to implementation functions:

```python
# In spec.fizz
role Node:
    term = 0
    state = "follower"
    log = []

    def HandleRequestVote(candidate_term, candidate_id):
        # ... vote logic ...

    def HandleAppendEntries(leader_term, entries):
        # ... replication logic ...
```

Each action name should map to a Go method:

```go
// In node.go
func (n *Node) HandleRequestVote(candidateTerm int, candidateID string) VoteResponse {
    // ... implementation ...
}

func (n *Node) HandleAppendEntries(leaderTerm int, entries []Entry) AppendResponse {
    // ... implementation ...
}
```

#### Step 2: Write the Test Adapter

The adapter translates between spec state/actions and Go state/functions:

```go
// In node_mbt_test.go
package raft

import (
    "testing"
    "github.com/fizzbee-io/fizzbee/mbt"
)

func TestMBT(t *testing.T) {
    runner := mbt.NewRunner("spec.fizz")

    // Map spec actions to Go functions
    runner.MapAction("HandleRequestVote", func(state mbt.State, args mbt.Args) {
        node := getNode(state)
        node.HandleRequestVote(args.Int("candidate_term"), args.String("candidate_id"))
    })

    runner.MapAction("HandleAppendEntries", func(state mbt.State, args mbt.Args) {
        node := getNode(state)
        entries := convertEntries(args.List("entries"))
        node.HandleAppendEntries(args.Int("leader_term"), entries)
    })

    // Map spec state to Go state for comparison
    runner.MapState("term", func(node interface{}) interface{} {
        return node.(*Node).Term
    })

    runner.MapState("state", func(node interface{}) interface{} {
        return string(node.(*Node).State)
    })

    // Run all generated traces
    runner.Run(t)
}
```

#### Step 3: Handle State Comparison

The test harness compares spec state to implementation state after each action. Define how to extract comparable state from your Go implementation:

```go
// State extraction function
func extractState(node *Node) map[string]interface{} {
    return map[string]interface{}{
        "term":      node.Term,
        "state":     string(node.State),
        "log_len":   len(node.Log),
        "voted_for": node.VotedFor,
    }
}
```

**Common state comparison issues**:
- Implementation has more state than the spec (expected -- spec is an abstraction)
- Spec uses different types than implementation (adapter must convert)
- Timing-dependent state (avoid comparing timestamps)

#### Step 4: Run MBT Tests

```bash
# Generate traces and run tests
fizz mbt spec.fizz --go-test ./...

# Or generate traces separately
fizz mbt spec.fizz --output traces/
go test -run TestMBT ./...
```

### Interpreting MBT Results

| Result | Meaning | Action |
|---|---|---|
| All tests pass | Implementation matches spec for all explored traces | Increase state bounds for more coverage |
| Assertion failure | Implementation state diverges from spec state | Compare the specific state diff to find the implementation bug |
| Action not mapped | Spec action has no adapter | Add adapter or mark action as abstract |
| Adapter crash | Adapter code threw an exception | Fix adapter code (not an implementation bug) |

---

## State Graph Visualization

The model checker can output the state graph in DOT format for visualization.

```bash
# Generate state graph
fizz verify --graph states.dot spec.fizz

# Convert to SVG for viewing
dot -Tsvg states.dot -o states.svg

# Convert to PNG
dot -Tpng states.dot -o states.png
```

**Reading state graphs**:
- **Nodes**: Each node is a unique state (combination of all state variable values)
- **Edges**: Each edge is an action transition (labeled with the action name)
- **Red nodes**: States where an assertion is violated
- **Green nodes**: States satisfying liveness properties
- **Cycles**: Indicate potential liveness issues or expected steady-state behavior

The online playground at fizzbee.io provides interactive state graph visualization without needing Graphviz locally.

---

## Counterexample Interpretation

When the model checker finds a violation, it produces a counterexample trace.

### Reading a Counterexample

```
ASSERTION VIOLATION: NoTwoLeaders
Trace:
  State 0: {nodes: [{term: 0, state: "follower"}, {term: 0, state: "follower"}, {term: 0, state: "follower"}]}
  Action: Node[0].HandleTimeout
  State 1: {nodes: [{term: 1, state: "candidate"}, {term: 0, state: "follower"}, {term: 0, state: "follower"}]}
  Action: Node[1].HandleTimeout
  State 2: {nodes: [{term: 1, state: "candidate"}, {term: 1, state: "candidate"}, {term: 0, state: "follower"}]}
  Action: Node[0].BecomeLeader
  State 3: {nodes: [{term: 1, state: "leader"}, {term: 1, state: "candidate"}, {term: 0, state: "follower"}]}
  Action: Node[1].BecomeLeader
  State 4: {nodes: [{term: 1, state: "leader"}, {term: 1, state: "leader"}, {term: 0, state: "follower"}]}
  VIOLATION: NoTwoLeaders at State 4
```

**Analysis steps**:
1. Read the final state -- what assertion is violated?
2. Work backward from the violation -- which action caused the bad state?
3. Identify the root cause -- is it a missing precondition? A missing check? A protocol flaw?
4. In this example: `BecomeLeader` lacks a quorum check, allowing two leaders in the same term.

### Common Counterexample Patterns

| Pattern | Typical Cause |
|---|---|
| Two actions racing to the same state | Missing lock or CAS in the protocol |
| Message delivered after state change | Missing term/epoch check in message handler |
| Action fires in unexpected state | Missing `requires` precondition |
| Infinite loop without progress | Missing fairness annotation or genuine liveness bug |

---

## Performance Modeling

Fizzbee supports performance properties alongside correctness properties.

### Annotating Actions with Costs

```python
def HandleRequest():
    cost("latency", 10)  # 10ms per request
    cost("messages", 1)  # 1 message sent
    # ... action body ...

def ReplicateToFollower():
    cost("latency", 5)
    cost("messages", 2)  # Request + response
    # ... action body ...
```

### Performance Assertions

```python
# Latency bound
always assertion LatencyBound:
    assert total_cost("latency") < 100  # Under 100ms total

# Message complexity
always assertion MessageBound:
    assert total_cost("messages") < 3 * len(nodes)  # Linear message complexity
```

---

## CI/CD Integration

### GitHub Actions

```yaml
name: Verify Fizzbee Specs
on: [push, pull_request]

jobs:
  verify:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Install Go
        uses: actions/setup-go@v5
        with:
          go-version: '1.22'

      - name: Install Fizzbee
        run: go install github.com/fizzbee-io/fizzbee@latest

      - name: Verify specifications
        run: |
          for spec in specs/*.fizz; do
            echo "Verifying $spec..."
            fizz verify "$spec"
          done

      - name: Run MBT tests
        run: go test -run TestMBT ./...
```

### Pre-commit Hook

```bash
#!/bin/bash
# .git/hooks/pre-commit

# Find modified .fizz files
fizz_files=$(git diff --cached --name-only --diff-filter=ACM | grep '\.fizz$')

if [ -n "$fizz_files" ]; then
    echo "Verifying Fizzbee specifications..."
    for spec in $fizz_files; do
        if ! fizz verify "$spec"; then
            echo "FAIL: $spec"
            exit 1
        fi
    done
    echo "All specifications verified."
fi
```

### Integration Best Practices

1. **Verify on every PR**: Run `fizz verify` for all `.fizz` files in CI
2. **Run MBT tests**: Include MBT tests in the regular Go test suite
3. **Gate merges**: Block PRs that break spec verification
4. **Version specs**: Keep `.fizz` files in the same repository as the implementation
5. **Review spec changes**: Treat spec changes with the same rigor as code changes

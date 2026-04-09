# PlusCal Guide

PlusCal is an algorithm language that translates to TLA+. It provides imperative-style syntax for specifying concurrent algorithms while generating TLA+ that TLC can check.

## Algorithm Structure

PlusCal code is embedded in a TLA+ module inside a comment block:

```tla
---- MODULE MyAlgorithm ----
EXTENDS Integers, Sequences, FiniteSets, TLC

CONSTANTS Server, Value

(*
--algorithm MyAlgorithm

variables
    \* global variables here

define
    \* operator definitions (available to all processes)
end define;

\* macros and procedures here

\* process definitions here

end algorithm;
*)

\* BEGIN TRANSLATION (auto-generated TLA+ appears here)
\* END TRANSLATION

====
```

## Uniprocess vs Multiprocess

### Uniprocess (Single Process)

```
--algorithm Simple
variables x = 0, y = 0;
begin
    x := 1;
    y := x + 1;
end algorithm;
```

### Multiprocess (Concurrent Processes)

```
--algorithm Distributed
variables messages = {};

process server \in Server
variables localState = "init";
begin
    ServerLoop:
        while TRUE do
            \* server logic
        end while;
end process;

process client \in Client
variables request = <<>>;
begin
    SendRequest:
        \* client logic
end process;

end algorithm;
```

### Fair Processes

Adding `fair` ensures weak fairness (the process cannot halt if it has enabled steps):

```
fair algorithm FairAlgo
\* ...
end algorithm;
```

```
fair process server \in Server
\* ...
end process;
```

For strong fairness, use `fair+`:

```
fair+ process server \in Server
\* ...
end process;
```

## Variables Block

```
variables
    x = 0,                          \* scalar
    y \in {1, 2, 3},               \* nondeterministic choice
    log = [s \in Server |-> <<>>],  \* function
    msgs = {},                      \* set
    count = [s \in Server |-> 0];   \* semicolon ends the block
```

Variables declared in the top-level `variables` block are global. Variables declared inside a `process` block are local to that process.

```
process server \in Server
variables
    localTerm = 0,       \* each process gets its own copy
    localState = "init";
begin
    \* ...
end process;
```

## Labels

Labels define atomic steps. Everything between two labels (or between a label and `end`) executes atomically in one state transition.

```
A:  x := 1;      \* step A: x becomes 1
    y := x + 1;   \* still part of step A (atomic with above)
B:  z := y;       \* step B: separate atomic step
```

### Label Placement Rules

Labels are **required**:
- At the beginning of every process body
- At the beginning of every `while` loop body
- After every `call` to a procedure (at the next statement)
- After every `return` or `goto` (at the next statement)

Labels are **forbidden**:
- Inside a `with` statement
- Inside a `macro`

### Atomicity Considerations

- Coarse-grained labels (fewer labels): Faster model checking but may miss real bugs due to overly atomic steps
- Fine-grained labels (more labels): More realistic interleaving but larger state space
- Rule of thumb: Each label boundary represents a point where another process could interleave

## Assignment

```
x := 5;
x := x + 1;
log[self] := Append(log[self], entry);
state[self] := "leader";
```

Multiple assignments to **different** variables in the same label are fine:

```
A:  x := 1;
    y := 2;
```

Multiple assignments to the **same** variable in one label are allowed as sequential updates:

```
A:  x := 1;
    x := x + 1;  \* x ends up as 2
```

## Control Flow

### If/Then/Else

```
if condition1 then
    action1;
elsif condition2 then
    action2;
else
    action3;
end if;
```

### While Loop

```
Loop:
    while condition do
        body;
    end while;
```

The label before `while` is required.

### Either/Or (Nondeterminism)

Models nondeterministic choice -- TLC explores all branches:

```
either
    action1;
or
    action2;
or
    action3;
end either;
```

### With (Local Binding with Nondeterminism)

```
with msg \in messages do
    \* process msg -- TLC explores all possible msg values
    messages := messages \ {msg};
end with;
```

`with` selects a value nondeterministically from a set. If the set is empty, the step is disabled (blocks).

### Goto

```
Start:
    if condition then
        goto Done;    \* terminate this process
    else
        goto Start;   \* loop back
    end if;
```

`Done` is a special label that marks process termination.

## Await / When

`await` (or equivalently `when`) blocks until a condition is true:

```
ReceiveMsg:
    await messages # {};
    with msg \in messages do
        messages := messages \ {msg};
    end with;
```

If the condition is false, the process blocks at this step. TLC will only explore states where the condition holds.

## Print Statement

For debugging during TLC execution:

```
print <<"Server", self, "received", msg>>;
```

## Assert

For debugging and sanity checks:

```
assert x > 0;   \* TLC reports an error if x <= 0
```

## Define Block

The `define` block contains TLA+ operator definitions accessible to all processes:

```
define
    TypeOK ==
        /\ state \in [Server -> {"follower", "candidate", "leader"}]
        /\ currentTerm \in [Server -> Nat]

    SafetyInvariant ==
        \A s1, s2 \in Server :
            (state[s1] = "leader" /\ state[s2] = "leader")
            => currentTerm[s1] # currentTerm[s2]

    Quorum == {Q \in SUBSET Server : Cardinality(Q) * 2 > Cardinality(Server)}
end define;
```

## Macros

Macros are textually expanded at each call site. They cannot contain labels.

```
macro Send(msg) begin
    messages := messages \union {msg};
end macro;

macro SendAndReceive(send, recv) begin
    messages := (messages \union {send}) \ {recv};
end macro;
```

Usage:

```
Send([type |-> "vote_req", term |-> currentTerm[self], sender |-> self]);
```

## Procedures

Procedures are like subroutines with their own variables and labels. They can contain labels (unlike macros).

```
procedure HandleRequest(req)
variables response = <<>>;
begin
    ProcessReq:
        response := ComputeResponse(req);
    SendResp:
        Send([type |-> "response", body |-> response]);
        return;
end procedure;
```

Calling a procedure:

```
CallSite:
    call HandleRequest(incomingReq);
AfterCall:  \* label is required after call
    \* continue execution
```

## Process Sets and Parameters

```
process server \in {"s1", "s2", "s3"}
```

or with a constant:

```
process server \in Server
```

`self` refers to the current process identity inside the process body.

```
process server \in Server
variables localTerm = 0;
begin
    Main:
        localTerm := currentTerm[self];
        print <<"I am server", self, "with term", localTerm>>;
end process;
```

## Translation to TLA+

The PlusCal translator converts PlusCal into TLA+. The generated TLA+ appears between `\* BEGIN TRANSLATION` and `\* END TRANSLATION` markers.

What happens during translation:
- Each process becomes a set of TLA+ actions (one per label)
- A `pc` (program counter) variable is added to track which label each process is at
- Local variables become functions indexed by process identity
- The `Next` relation is a disjunction of all process actions
- `either/or` becomes disjunction within an action
- `with x \in S` becomes existential quantification `\E x \in S`
- `await cond` becomes a conjunct in the action's precondition
- The `stack` variable is added if procedures are used

### Translating

Using the TLA+ tools:
```bash
java -cp tla2tools.jar pcal.trans MyAlgorithm.tla
```

Or use the VS Code TLA+ extension which translates automatically.

## Common Pitfalls

### Missing Labels

```
\* WRONG: while loop body needs a label
while x > 0 do
    x := x - 1;
end while;

\* CORRECT
LoopBody:
    while x > 0 do
        x := x - 1;
    end while;
```

### Label After Call

```
\* WRONG: missing label after call
call Procedure(arg);
x := 1;

\* CORRECT
call Procedure(arg);
AfterCall:
    x := 1;
```

### Macro With Labels

```
\* WRONG: macros cannot contain labels
macro BadMacro() begin
    A: x := 1;
end macro;

\* Use a procedure instead if you need labels
```

### Forgetting self

```
\* WRONG: accessing global state without indexing by self
process server \in Server
begin
    Main:
        currentTerm := currentTerm + 1;  \* ERROR: currentTerm is a function

\* CORRECT
    Main:
        currentTerm[self] := currentTerm[self] + 1;
```

### Empty Set in With

```
\* This blocks (is never enabled) if messages is empty
with msg \in messages do
    \* ...
end with;

\* Add an await guard if you want to be explicit
await messages # {};
with msg \in messages do
    \* ...
end with;
```

## Complete Example: Simple Leader Election

```tla
---- MODULE SimpleElection ----
EXTENDS Integers, FiniteSets, TLC

CONSTANTS Server, MaxTerm

(*
--fair algorithm LeaderElection

variables
    currentTerm = [s \in Server |-> 0],
    state = [s \in Server |-> "follower"],
    votedFor = [s \in Server |-> "none"],
    votes = [s \in Server |-> {}],
    messages = {};

define
    TypeOK ==
        /\ currentTerm \in [Server -> 0..MaxTerm]
        /\ state \in [Server -> {"follower", "candidate", "leader"}]

    ElectionSafety ==
        \A s1, s2 \in Server :
            (state[s1] = "leader" /\ state[s2] = "leader")
            => currentTerm[s1] # currentTerm[s2]

    Quorum == {Q \in SUBSET Server : Cardinality(Q) * 2 > Cardinality(Server)}
end define;

macro Send(msg) begin
    messages := messages \union {msg};
end macro;

process server \in Server
begin
    ServerMain:
        while TRUE do
            either
                \* Start election
                await state[self] # "leader" /\ currentTerm[self] < MaxTerm;
                currentTerm[self] := currentTerm[self] + 1;
                state[self] := "candidate";
                votedFor[self] := self;
                votes[self] := {self};
                Send([type |-> "vote_req", term |-> currentTerm[self],
                      sender |-> self]);
            or
                \* Handle vote request
                with msg \in {m \in messages : m.type = "vote_req"} do
                    if msg.term > currentTerm[self] then
                        currentTerm[self] := msg.term;
                        state[self] := "follower";
                        votedFor[self] := msg.sender;
                        Send([type |-> "vote_resp", term |-> msg.term,
                              sender |-> self, dest |-> msg.sender,
                              granted |-> TRUE]);
                    end if;
                end with;
            or
                \* Handle vote response
                with msg \in {m \in messages : m.type = "vote_resp"
                              /\ m.dest = self /\ m.granted} do
                    if msg.term = currentTerm[self]
                       /\ state[self] = "candidate" then
                        votes[self] := votes[self] \union {msg.sender};
                        if votes[self] \in Quorum then
                            state[self] := "leader";
                        end if;
                    end if;
                end with;
            end either;
        end while;
end process;

end algorithm;
*)
====
```

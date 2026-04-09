# TLA+ Fundamentals

Comprehensive reference for the TLA+ specification language.

## Module Structure

Every TLA+ specification is a module:

```tla
---- MODULE ModuleName ----
\* Module body goes here
====
```

The four dashes and four equals signs are required delimiters. The file name must match the module name.

## EXTENDS

Import standard modules:

```tla
EXTENDS Integers, Sequences, FiniteSets, TLC, Bags, Reals, Naturals
```

Common modules:
- **Integers**: `+`, `-`, `*`, `..` (range), `Int`, `Nat`, `%`, `\div` (re-exports `Nat` from Naturals)
- **Naturals**: `Nat` is defined here (non-negative integers); Integers re-exports it
- **Sequences**: `Seq(S)`, `Len(s)`, `Head(s)`, `Tail(s)`, `Append(s, e)`, `s \o t` (concatenation), `SubSeq(s, i, j)`
- **FiniteSets**: `IsFiniteSet(S)`, `Cardinality(S)`
- **TLC**: `Print(val, expr)`, `PrintT(val)`, `Assert(cond, msg)`, `@@` (function merge), `:>` (singleton function)
- **Bags**: `BagIn(e, B)`, `EmptyBag`, `BagUnion(B1, B2)`, `BagOfAll(op, B)`, `CopiesIn(e, B)`
- **Reals**: `Real`, real number operations (rarely needed)

## CONSTANTS and VARIABLES

```tla
CONSTANTS
    Server,     \* a set of server identifiers
    Value,      \* a set of proposable values
    Quorum,     \* a set of quorums (sets of servers)
    MaxBallot   \* upper bound on ballot numbers for model checking

VARIABLES
    currentTerm,
    state,
    log,
    messages

\* Group all variables for use in temporal formulas
vars == <<currentTerm, state, log, messages>>
```

Constants are fixed for a given model run. Variables change between states.

## Operators

### Logic Operators

| Operator | Meaning | ASCII alternative |
|----------|---------|-------------------|
| `/\` | conjunction (AND) | `\land` |
| `\/` | disjunction (OR) | `\lor` |
| `~` | negation (NOT) | `\lnot`, `\neg` |
| `=>` | implication | none |
| `<=>` | equivalence (iff) | `\equiv` |
| `TRUE` | boolean true | none |
| `FALSE` | boolean false | none |

### Quantifiers

```tla
\A x \in S : P(x)    \* for all x in S, P(x) holds
\E x \in S : P(x)    \* there exists x in S such that P(x) holds
```

### Set Operators

| Operator | Meaning |
|----------|---------|
| `\in` | membership |
| `\notin` | non-membership |
| `\union` | union (also `\cup`) |
| `\intersect` | intersection (also `\cap`) |
| `\` | set difference |
| `\subseteq` | subset or equal |
| `SUBSET S` | power set (set of all subsets of S) |
| `UNION S` | union of all elements of S (S is a set of sets) |
| `{x \in S : P(x)}` | set filter |
| `{f(x) : x \in S}` | set map |

### Arithmetic Operators (with EXTENDS Integers)

| Operator | Meaning |
|----------|---------|
| `+`, `-`, `*` | addition, subtraction, multiplication |
| `\div` | integer division |
| `%` | modulo |
| `a..b` | integer range from a to b inclusive |
| `<`, `>`, `<=`, `>=` | comparison |

## Functions

Functions in TLA+ map from a domain to a range.

```tla
\* Function type: set of all functions from S to T
[S -> T]

\* Function literal (explicit mapping)
[s \in Server |-> 0]               \* maps every server to 0

\* Function application
f[x]                                \* apply function f to argument x

\* DOMAIN: get the domain of a function
DOMAIN f                            \* returns the set of inputs

\* Function update (EXCEPT)
[f EXCEPT ![x] = v]                \* f with f[x] changed to v
[f EXCEPT ![x] = @ + 1]            \* f with f[x] incremented (@ is old value)
[f EXCEPT ![x][y] = v]             \* nested update

\* Function merge (with TLC module)
f1 @@ f2                            \* merge: f1 takes priority on conflicts

\* Singleton function (with TLC module)
x :> v                              \* function mapping x to v
```

## Records

Records are functions from field names to values.

```tla
\* Record type
[type: {"request", "response"}, term: Nat, sender: Server]

\* Record literal
[type |-> "request", term |-> 1, sender |-> s1]

\* Field access
record.type
record["type"]    \* equivalent

\* Record update
[record EXCEPT !.term = 5]
[record EXCEPT !.term = @ + 1]
```

## Tuples and Sequences

Tuples are ordered collections. Sequences are tuples with sequence operations.

```tla
\* Tuple literal
<<1, 2, 3>>

\* Cartesian product (set of tuples)
S \X T                      \* set of all <<s, t>> where s \in S, t \in T

\* Sequence operations (with EXTENDS Sequences)
Seq(S)                      \* set of all finite sequences over S
Len(s)                      \* length of sequence s
Head(s)                     \* first element
Tail(s)                     \* all but first element
Append(s, e)                \* add e to end of s
s \o t                      \* concatenation
SubSeq(s, i, j)             \* subsequence from index i to j (1-based)
s[i]                        \* element at index i (1-based)
```

## Actions and Primed Variables

An action is a formula relating current state to next state.

```tla
\* Primed variable: refers to value in the next state
x' = x + 1                  \* x increases by 1

\* UNCHANGED: variable stays the same
UNCHANGED <<y, z>>           \* equivalent to y' = y /\ z' = z
UNCHANGED vars               \* no variable changes (stuttering)

\* ENABLED: tests if an action can be taken
ENABLED Action               \* TRUE if there exists a next state satisfying Action
```

### Action Patterns

```tla
\* Typical action structure
ActionName(param1, param2) ==
    \* Precondition (guard)
    /\ some_condition
    /\ variable1' = new_value1
    /\ variable2' = new_value2
    /\ UNCHANGED <<variable3, variable4>>

\* Next-state relation: disjunction of all possible actions
Next ==
    \/ \E s \in Server : Action1(s)
    \/ \E s \in Server : \E m \in messages : Action2(s, m)
    \/ StutterAction
```

## Temporal Operators

Temporal operators reason about behavior over time (sequences of states).

| Operator | Name | Meaning |
|----------|------|---------|
| `[]P` | always (box) | P holds in every state of every behavior |
| `<>P` | eventually (diamond) | P holds in some state of every behavior |
| `P ~> Q` | leads-to | whenever P holds, Q eventually holds later |
| `[]<>P` | infinitely often | P holds in infinitely many states |
| `<>[]P` | eventually always | P eventually holds and stays true forever |

### Fairness

Fairness conditions prevent unrealistic behaviors where an enabled action is never taken.

```tla
\* Weak fairness: if Action is continuously enabled, it must eventually happen
WF_vars(Action)

\* Strong fairness: if Action is repeatedly enabled (not necessarily continuously), it must eventually happen
SF_vars(Action)

\* Specification with fairness
Spec == Init /\ [][Next]_vars /\ WF_vars(Next)

\* Per-action fairness (more precise)
Spec == Init /\ [][Next]_vars
    /\ WF_vars(Action1)
    /\ SF_vars(Action2)
```

### Common Temporal Patterns

```tla
\* Safety: something bad never happens
Safety == []~BadState

\* Liveness: something good eventually happens
Liveness == <>(GoalReached)

\* Responsiveness: every request eventually gets a response
Responsiveness == \A c \in Client : [](requested[c] => <>(responded[c]))

\* Leads-to: if P, then eventually Q
Progress == P ~> Q
```

## Type-Correctness Invariant (TypeOK)

Always define a `TypeOK` invariant. It catches modeling errors early and documents the expected types.

```tla
TypeOK ==
    /\ currentTerm \in [Server -> Nat]
    /\ state \in [Server -> {"follower", "candidate", "leader"}]
    /\ log \in [Server -> Seq([term: Nat, value: Value])]
    /\ messages \subseteq Message
```

## THEOREM Declarations

```tla
THEOREM Spec => []TypeOK
THEOREM Spec => []Safety
THEOREM Spec => Liveness
```

These are documentation and proof obligations. TLC checks them; TLAPS can prove them.

## LET...IN Expressions

```tla
Operator(x) ==
    LET helper == x + 1
        double(y) == y * 2
    IN double(helper)
```

## IF...THEN...ELSE

```tla
Max(a, b) == IF a >= b THEN a ELSE b
```

## CASE Expressions

```tla
Response(status) ==
    CASE status = "ok"    -> [code |-> 200, body |-> "success"]
      [] status = "error" -> [code |-> 500, body |-> "failure"]
      [] OTHER            -> [code |-> 400, body |-> "unknown"]
```

`OTHER` is the default case. Omitting `OTHER` when no case matches causes a runtime error in TLC.

## CHOOSE Operator

`CHOOSE` selects an arbitrary element satisfying a predicate. It is deterministic: the same CHOOSE always returns the same value.

```tla
\* Pick some element from a set
Min(S) == CHOOSE x \in S : \A y \in S : x <= y

\* Pick the unique element (if it exists)
TheLeader == CHOOSE s \in Server : state[s] = "leader"
```

Warning: `CHOOSE` with no satisfying element is a runtime error. Always ensure the set is non-empty.

```tla
\* Defensive pattern: guard CHOOSE with non-empty check
SafeMin(S) == IF S # {} THEN CHOOSE x \in S : \A y \in S : x <= y ELSE -1
```

## Recursive Operators

```tla
RECURSIVE Sum(_)
Sum(S) ==
    IF S = {} THEN 0
    ELSE LET x == CHOOSE x \in S : TRUE
         IN x + Sum(S \ {x})
```

Recursive operators must be declared with `RECURSIVE` before definition.

## INSTANCE and Parameterized Modules

```tla
\* Import a module with parameter substitution
INSTANCE ModuleName WITH param1 <- expr1, param2 <- expr2

\* Local instance (no namespace pollution)
LOCAL INSTANCE ModuleName

\* Named instance
M == INSTANCE ModuleName WITH param1 <- expr1
\* Use as: M!OperatorName
```

## Common Idioms

### Sending and Receiving Messages

```tla
Send(m) == messages' = messages \union {m}

Receive(m) == messages' = messages \ {m}

SendAndReceive(send, recv) ==
    messages' = (messages \ {recv}) \union {send}
```

### Quorum Intersection

```tla
\* Define quorums as all majority subsets
Quorum == {Q \in SUBSET Server : Cardinality(Q) * 2 > Cardinality(Server)}

\* Any two quorums intersect (follows from majority property)
ASSUME QuorumAssumption == \A Q1, Q2 \in Quorum : Q1 \intersect Q2 # {}
```

### Sequence Utilities

```tla
\* Last element of a sequence
Last(s) == s[Len(s)]

\* Check if element is in a sequence
InSeq(e, s) == \E i \in 1..Len(s) : s[i] = e

\* Sequence to set
SeqToSet(s) == {s[i] : i \in 1..Len(s)}
```

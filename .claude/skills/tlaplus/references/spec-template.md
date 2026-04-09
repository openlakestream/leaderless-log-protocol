# Specification Templates

Starter templates for distributed protocol specifications.

## Template 1: PlusCal Distributed Protocol (Multiprocess with Message Passing)

```tla
---- MODULE ProtocolName ----
\*
\* Specification of [Protocol Name]
\*
\* [Brief description of what this protocol does and what properties it ensures]
\*
EXTENDS Integers, Sequences, FiniteSets, TLC

\* ============================================================================
\* Constants
\* ============================================================================

CONSTANTS
    Node,           \* Set of node identifiers (e.g., {n1, n2, n3})
    Value,          \* Set of proposable values (e.g., {v1, v2})
    MaxRound,       \* Upper bound on round/term/epoch numbers (for model checking)
    Nil             \* A model value representing "none" / "unset"

\* ============================================================================
\* Message type definition
\* ============================================================================

\* Define the set of all possible messages.
\* Customize fields based on your protocol.
Message ==
    [type: {"request"},  sender: Node, value: Value]
    \union
    [type: {"response"}, sender: Node, dest: Node, value: Value, round: Nat]
    \union
    [type: {"ack"},      sender: Node, dest: Node, round: Nat]

\* ============================================================================
\* PlusCal algorithm
\* ============================================================================

(*
--algorithm ProtocolName

variables
    \* ---- Global state ----
    messages = {},                              \* Set of in-flight messages

    \* ---- Per-node state ----
    round     = [n \in Node |-> 0],             \* Current round/term/epoch
    status    = [n \in Node |-> "idle"],         \* Node status
    decided   = [n \in Node |-> Nil];           \* Decided value (Nil if undecided)

define
    \* ==== Type invariant ====
    \* Always define TypeOK first -- it catches modeling errors early
    TypeOK ==
        /\ round \in [Node -> 0..MaxRound]
        /\ status \in [Node -> {"idle", "waiting", "decided"}]
        /\ decided \in [Node -> Value \union {Nil}]
        /\ messages \subseteq Message

    \* ==== Safety properties ====
    \* Customize these for your protocol

    \* Agreement: all decided nodes agree on the value
    Agreement ==
        \A n1, n2 \in Node :
            (decided[n1] # Nil /\ decided[n2] # Nil)
            => decided[n1] = decided[n2]

    \* Validity: decided value was actually proposed
    Validity ==
        \A n \in Node :
            decided[n] # Nil => decided[n] \in Value

    \* ==== Helper operators ====
    Quorum == {Q \in SUBSET Node : Cardinality(Q) * 2 > Cardinality(Node)}
end define;

\* ==== Macros ====
\* Use macros for common operations (cannot contain labels)

macro Send(msg) begin
    messages := messages \union {msg};
end macro;

macro SendAndReceive(send, recv) begin
    messages := (messages \union {send}) \ {recv};
end macro;

\* ==== Process definitions ====

fair process node \in Node  \* weak fairness: each process eventually takes a step if continuously enabled
variables
    \* Local variables for this process
    localVar = "init";
begin
    \* ---- Main loop ----
    \* Rename labels to match your protocol phases
    MainLoop:
        while TRUE do
            either
                \* ---- Action 1: Initiate ----
                \* Guard: precondition for this action
                await status[self] = "idle" /\ round[self] < MaxRound;
                round[self] := round[self] + 1;
                status[self] := "waiting";
                Send([type |-> "request", sender |-> self,
                      value |-> CHOOSE v \in Value : TRUE]);

            or
                \* ---- Action 2: Handle incoming message ----
                with msg \in {m \in messages : m.type = "request"} do
                    \* Process the message
                    Send([type |-> "response", sender |-> self,
                          dest |-> msg.sender, value |-> msg.value,
                          round |-> round[self]]);
                end with;

            or
                \* ---- Action 3: Decide based on responses ----
                await status[self] = "waiting";
                with v \in Value do
                    \* Check if enough responses received to decide
                    await \E Q \in Quorum : \A n \in Q :
                        \E m \in messages : m.type = "response"
                            /\ m.dest = self /\ m.value = v;
                    decided[self] := v;
                    status[self] := "decided";
                end with;

            end either;
        end while;
end process;

end algorithm;
*)

\* ============================================================================
\* TLA+ translation appears below (auto-generated)
\* ============================================================================

\* BEGIN TRANSLATION
\* ... (auto-generated by PlusCal translator)
\* END TRANSLATION

\* ============================================================================
\* Liveness properties (defined after translation so they can reference pc)
\* ============================================================================

\* Eventually some node decides
EventualDecision == <>(\E n \in Node : decided[n] # Nil)

\* ============================================================================
\* Symmetry (for model checking optimization)
\* ============================================================================

NodeSymmetry == Permutations(Node)
ValueSymmetry == Permutations(Value)
\* NOTE: Use ONE symmetry set per TLC model configuration.
\* Multiple symmetry sets cannot be unioned — assign each to a
\* separate SYMMETRY declaration in the TLC config, or use only
\* the most beneficial one (typically NodeSymmetry).

====
```

## Template 2: Raw TLA+ Distributed Protocol

```tla
---- MODULE ProtocolNameRaw ----
\*
\* Specification of [Protocol Name] in raw TLA+
\*
\* [Brief description]
\*
EXTENDS Integers, Sequences, FiniteSets, TLC

\* ============================================================================
\* Constants
\* ============================================================================

CONSTANTS
    Node,           \* Set of node identifiers
    Value,          \* Set of proposable values
    MaxRound,       \* Bound for model checking
    Nil             \* Model value for "none"

ASSUME NilAssumption == Nil \notin Value

\* ============================================================================
\* Variables
\* ============================================================================

VARIABLES
    round,          \* [Node -> Nat]  current round per node
    status,         \* [Node -> Status] node status
    decided,        \* [Node -> Value \union {Nil}] decided value
    messages        \* set of in-flight messages

\* Group all variables (required for temporal formulas and UNCHANGED)
vars == <<round, status, decided, messages>>

\* ============================================================================
\* Type definitions
\* ============================================================================

Status == {"idle", "waiting", "decided"}

Message ==
    [type: {"request"},  sender: Node, value: Value]
    \union
    [type: {"response"}, sender: Node, dest: Node, value: Value, round: Nat]

\* ============================================================================
\* Type invariant
\* ============================================================================

TypeOK ==
    /\ round \in [Node -> 0..MaxRound]
    /\ status \in [Node -> Status]
    /\ decided \in [Node -> Value \union {Nil}]
    /\ messages \subseteq Message

\* ============================================================================
\* Helper operators
\* ============================================================================

Quorum == {Q \in SUBSET Node : Cardinality(Q) * 2 > Cardinality(Node)}

Send(m) == messages' = messages \union {m}

\* ============================================================================
\* Initial state
\* ============================================================================

Init ==
    /\ round    = [n \in Node |-> 0]
    /\ status   = [n \in Node |-> "idle"]
    /\ decided  = [n \in Node |-> Nil]
    /\ messages = {}

\* ============================================================================
\* Actions
\* ============================================================================

\* --- Action 1: Node n initiates a round ---
Initiate(n) ==
    /\ status[n] = "idle"
    /\ round[n] < MaxRound
    /\ round'   = [round EXCEPT ![n] = round[n] + 1]
    /\ status'  = [status EXCEPT ![n] = "waiting"]
    /\ Send([type |-> "request", sender |-> n,
             value |-> CHOOSE v \in Value : TRUE])
    /\ UNCHANGED decided

\* --- Action 2: Node n handles a request message ---
HandleRequest(n) ==
    /\ \E m \in messages :
        /\ m.type = "request"
        /\ messages' = messages \union
            {[type |-> "response", sender |-> n, dest |-> m.sender,
              value |-> m.value, round |-> round[n]]}
    /\ UNCHANGED <<round, status, decided>>

\* --- Action 3: Node n decides ---
Decide(n) ==
    /\ status[n] = "waiting"
    /\ \E v \in Value :
        /\ \E Q \in Quorum :
            \A q \in Q : \E m \in messages :
                /\ m.type = "response"
                /\ m.dest = n
                /\ m.value = v
        /\ decided' = [decided EXCEPT ![n] = v]
    /\ status' = [status EXCEPT ![n] = "decided"]
    /\ UNCHANGED <<round, messages>>

\* ============================================================================
\* Next-state relation
\* ============================================================================

Next ==
    \E n \in Node :
        \/ Initiate(n)
        \/ HandleRequest(n)
        \/ Decide(n)

\* ============================================================================
\* Specification
\* ============================================================================

Spec == Init /\ [][Next]_vars

\* With fairness (needed for liveness)
FairSpec == Init /\ [][Next]_vars /\ WF_vars(Next)

\* ============================================================================
\* Safety properties
\* ============================================================================

\* Agreement: all decided nodes agree
Agreement ==
    \A n1, n2 \in Node :
        (decided[n1] # Nil /\ decided[n2] # Nil) => decided[n1] = decided[n2]

\* Validity: decided value was proposed
Validity ==
    \A n \in Node : decided[n] # Nil => decided[n] \in Value

\* ============================================================================
\* Liveness properties
\* ============================================================================

\* Eventually some node decides
EventualDecision == <>(\E n \in Node : decided[n] # Nil)

\* ============================================================================
\* Symmetry
\* ============================================================================

NodeSymmetry == Permutations(Node)
ValueSymmetry == Permutations(Value)
AllSymmetry == NodeSymmetry \union ValueSymmetry

\* ============================================================================
\* Theorems (proof obligations)
\* ============================================================================

THEOREM Spec => []TypeOK
THEOREM Spec => []Agreement
THEOREM FairSpec => EventualDecision

====
```

## Template 3: TLC Configuration File (.cfg)

```
\* ============================================================================
\* ProtocolName.cfg -- TLC configuration for ProtocolName.tla
\* ============================================================================

\* --- Specification ---
\* Use SPECIFICATION for temporal formula (includes fairness):
\* SPECIFICATION FairSpec
\*
\* Or use INIT/NEXT for basic safety checking (faster):
INIT Init
NEXT Next

\* --- Constants ---
\* Assign concrete values for model checking.
\* Start small, increase if no violations found.
CONSTANTS
    Node  = {n1, n2, n3}       \* 3 nodes (minimum for quorum-based protocols)
    Value = {v1, v2}           \* 2 values (minimum to check agreement)
    MaxRound = 3               \* bound on rounds (start at 2-3)
    Nil = Nil                  \* model value

\* --- Safety properties (state invariants) ---
\* TLC checks these hold in every reachable state
INVARIANT TypeOK
INVARIANT Agreement
INVARIANT Validity

\* --- Liveness properties (temporal formulas) ---
\* Uncomment when using SPECIFICATION with fairness
\* PROPERTY EventualDecision

\* --- Symmetry reduction ---
\* Reduces state space by treating symmetric values as equivalent
\* WARNING: Do not use with liveness properties
SYMMETRY AllSymmetry

\* --- State constraint ---
\* Bounds the state space. Remove or increase if checking passes.
\* CONSTRAINT StateConstraint

\* --- Action constraint ---
\* Bounds transitions (e.g., limit message creation)
\* ACTION_CONSTRAINT ActionConstraint

\* --- Deadlock checking ---
\* Uncomment to disable deadlock detection (e.g., when termination is expected)
\* CHECK_DEADLOCK FALSE
```

## Usage Guide

1. Copy the appropriate template (PlusCal or raw TLA+) and rename the module
2. Customize constants, variables, and message types for your protocol
3. Replace placeholder actions with your protocol's actual actions
4. Define safety properties based on your requirements
5. Copy the .cfg template and adjust constants and properties
6. Run TLC: `java -jar tla2tools.jar -workers 4 ProtocolName.tla`
7. If PlusCal, translate first: `java -cp tla2tools.jar pcal.trans ProtocolName.tla`

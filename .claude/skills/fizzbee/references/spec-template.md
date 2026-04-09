# Fizzbee Specification Templates

Starter templates for new specifications. Copy the appropriate template and fill in the protocol-specific logic.

---

## Template 1: Simple Single-Process Specification

Use for algorithms that run on a single node (e.g., lock-free data structures, state machines, sequential protocols).

```python
# ============================================================
# [PROTOCOL NAME] Specification
# ============================================================
# Description: [What this spec models]
# Properties:  [What it verifies]
# ============================================================

# --- Assertions ---
# Define safety and liveness properties FIRST.
# These are the "requirements" -- the spec exists to verify these.

always assertion SafetyProperty:
    """[Describe what must always be true.]"""
    assert True  # TODO: Replace with actual property

eventually assertion LivenessProperty:
    """[Describe what must eventually become true.]"""
    assert True  # TODO: Replace with actual property

# --- State Variables ---
# Only model state relevant to the properties above.
# Abstract away implementation details.

state_var_1 = None       # [Describe purpose]
state_var_2 = 0          # [Describe purpose]
history = []             # [Track events for assertions if needed]

# --- Initialization ---

def init():
    global state_var_1, state_var_2, history
    state_var_1 = None
    state_var_2 = 0
    history = []

# --- Actions ---
# Each action represents an event or operation the system can perform.
# The model checker explores all possible orderings of these actions.

def Action1():
    """[Describe when and why this action fires.]"""
    requires(state_var_1 is None)  # Precondition
    global state_var_1
    state_var_1 = "active"

def Action2():
    """[Describe this action.]"""
    requires(state_var_1 == "active")
    global state_var_2
    oneof:
        state_var_2 += 1   # Nondeterministic: increment
    or:
        state_var_2 -= 1   # Nondeterministic: decrement

fair def Action3():
    """[Fair action -- guaranteed to eventually fire if enabled.]"""
    requires(state_var_2 != 0)
    global state_var_2
    state_var_2 = 0

# --- Helper Functions ---
# Pure functions called from actions or assertions.
# These are NOT scheduled by the model checker.

def is_valid_state():
    return state_var_1 is not None or state_var_2 == 0
```

---

## Template 2: Multi-Role Distributed Protocol

Use for distributed systems with distinct participant types communicating over a network.

```python
# ============================================================
# [PROTOCOL NAME] Specification
# ============================================================
# Description: [What distributed system this models]
# Roles:       [List participant types]
# Properties:  [Safety and liveness properties]
# Channel:     [Network assumptions]
# ============================================================

# --- Configuration ---
# Adjust these to control state space size.

NUM_SERVERS = 3          # Number of server instances
NUM_CLIENTS = 2          # Number of client instances
MAX_OPERATIONS = 3       # Bound on operations (limits state space)

# --- Assertions ---

always assertion AgreementSafety:
    """[All participants agree on committed values.]"""
    committed = [s.committed_value for s in servers if s.committed_value is not None]
    if len(committed) > 1:
        assert all(v == committed[0] for v in committed)

always assertion NoSplitBrain:
    """[At most one leader at a time.]"""
    leaders = [s for s in servers if s.role == "leader"]
    assert len(leaders) <= 1

always eventually assertion Progress:
    """[System eventually makes progress.]"""
    assert any(s.committed_value is not None for s in servers)

# --- Channels ---
# Configure based on real network assumptions.
# Start with channel() and progressively harden.

server_chan = {i: channel() for i in range(NUM_SERVERS)}
client_chan = {i: channel() for i in range(NUM_CLIENTS)}

# For fault tolerance testing, switch to:
# server_chan = {i: channel(lossy=True) for i in range(NUM_SERVERS)}

# --- Server Role ---

role Server:
    # Per-instance state
    role_state = "follower"   # follower, candidate, leader
    term = 0                  # Logical clock / epoch
    committed_value = None    # Last committed value
    log = []                  # Operation log
    peers = []                # Other server IDs

    def init():
        global peers
        peers = [i for i in range(NUM_SERVERS) if i != self_id]

    # --- Core Protocol Actions ---

    def HandleClientRequest(msg):
        """[Process a client request.]"""
        requires(role_state == "leader")
        global log
        log.append({"value": msg["value"], "term": term})
        # Replicate to followers
        for peer in peers:
            server_chan[peer].send({
                "type": "replicate",
                "entry": log[-1],
                "index": len(log) - 1,
                "from": self_id,
            })

    def HandleReplicate(msg):
        """[Follower receives entry from leader.]"""
        requires(role_state == "follower")
        global log
        if msg["entry"]["term"] >= term:
            log.append(msg["entry"])
            server_chan[msg["from"]].send({
                "type": "ack",
                "index": msg["index"],
                "from": self_id,
            })

    def HandleAck(msg):
        """[Leader processes follower acknowledgment.]"""
        requires(role_state == "leader")
        global committed_value
        # TODO: Track ack count, commit when quorum reached
        pass

    # --- Leader Election Actions ---

    fair def ElectionTimeout():
        """[Follower times out, starts election.]"""
        requires(role_state == "follower")
        global role_state, term
        term += 1
        role_state = "candidate"
        # TODO: Send vote requests

    # --- Crash and Recovery ---

    def Crash():
        """[Model node crash.]"""
        global role_state
        role_state = "follower"
        # Persistent state (log, term) survives
        # Volatile state is reset

# --- Client Role ---

role Client:
    request_sent = False
    response = None

    def SendRequest():
        """[Client sends a request to the leader.]"""
        requires(not request_sent)
        global request_sent
        request_sent = True
        leader = any s in servers
        requires(leader.role_state == "leader")
        server_chan[leader].send({
            "type": "client_request",
            "value": "op_" + str(self_id),
            "from": self_id,
        })

# --- Instantiation ---

servers = role(Server, NUM_SERVERS)
clients = role(Client, NUM_CLIENTS)
```

---

## Template 3: Model-Based Testing Setup

Use when you want to generate Go tests from a verified specification.

```python
# ============================================================
# [PROTOCOL NAME] Specification (MBT-Ready)
# ============================================================
# Description: [What this models]
# MBT Target:  [Go package and test file]
# ============================================================
#
# MBT CONVENTIONS:
# - Action names must match Go method names
# - State variable names should map to Go struct fields
# - Use simple types (int, string, bool, list) for state
# - Avoid complex nested structures that are hard to map

# --- Assertions ---

always assertion CoreInvariant:
    """[The main property under test.]"""
    assert True  # TODO

# --- State ---
# Keep state simple and directly mappable to Go structs.
# Document the Go type mapping in comments.

counter = 0          # Go: node.Counter (int)
status = "idle"      # Go: node.Status (string)
items = []           # Go: node.Items ([]string)

# --- Init ---

def init():
    global counter, status, items
    counter = 0
    status = "idle"
    items = []

# --- Actions ---
# NAMING CONVENTION: Action names = Go method names
# The MBT adapter will call node.ActionName() for each action.

def Increment():
    """Go: node.Increment()"""
    requires(status == "active")
    global counter
    counter += 1

def Activate():
    """Go: node.Activate()"""
    requires(status == "idle")
    global status
    status = "active"

def AddItem():
    """Go: node.AddItem(item string)"""
    requires(status == "active")
    requires(len(items) < 5)  # Bound for state space
    item = any i in ["a", "b", "c"]
    global items
    items.append(item)

def Deactivate():
    """Go: node.Deactivate()"""
    requires(status == "active")
    global status
    status = "idle"

# --- Helper Functions ---

def item_count():
    return len(items)
```

**Corresponding Go test adapter** (see `knowledge/fizzbee-tooling.md` for full details):

```go
// protocol_mbt_test.go
package protocol

import (
    "testing"
    "github.com/fizzbee-io/fizzbee/mbt"
)

func TestProtocolMBT(t *testing.T) {
    runner := mbt.NewRunner("spec.fizz")

    runner.MapAction("Increment", func(state mbt.State, args mbt.Args) {
        node := state.(*Node)
        node.Increment()
    })

    runner.MapAction("Activate", func(state mbt.State, args mbt.Args) {
        node := state.(*Node)
        node.Activate()
    })

    runner.MapAction("AddItem", func(state mbt.State, args mbt.Args) {
        node := state.(*Node)
        node.AddItem(args.String("item"))
    })

    runner.MapAction("Deactivate", func(state mbt.State, args mbt.Args) {
        node := state.(*Node)
        node.Deactivate()
    })

    runner.MapState("counter", func(s interface{}) interface{} {
        return s.(*Node).Counter
    })
    runner.MapState("status", func(s interface{}) interface{} {
        return s.(*Node).Status
    })
    runner.MapState("items", func(s interface{}) interface{} {
        return s.(*Node).Items
    })

    runner.Run(t)
}
```

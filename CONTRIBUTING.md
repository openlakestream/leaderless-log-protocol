# Contributing to the Leaderless Log Protocol

Thank you for your interest in contributing! This project uses a **spec-first** approach: the canonical specification documents (Markdown) are the source of truth, and all formal models (TLA+, Fizzbee) and reference implementations must stay aligned with them.

## Getting Started

### Prerequisites

- **Java 17+** — required for TLA+ model checker (TLC)
- **Make** — for running verification targets
- **Rust toolchain** — for the S3-Queue reference implementation (optional)
- **Docker** — for running MinIO in integration tests (optional)

### Setup

```bash
# Install formal verification tools
make tlaplus-install    # Download tla2tools.jar
make fizzbee-install    # Download Fizzbee CLI

# Verify everything passes
make verify             # Run both TLA+ and Fizzbee model checkers

# See all available targets
make help
```

### Reading Order

New to the project? Read in this order:

1. [`0-coordination-delegated-pattern.md`](0-coordination-delegated-pattern.md) — The foundational pattern
2. [`1-leaderless-log-protocol.md`](1-leaderless-log-protocol.md) — Protocol 1: Leaderless Log
3. [`2-coordination-delegated-task-claiming.md`](2-coordination-delegated-task-claiming.md) — Protocol 2: Task Claiming
4. [`examples/s3-queue/SPEC.md`](examples/s3-queue/SPEC.md) — Reference implementation spec

## How to Contribute

### Contribution Types

| Type | Workflow |
|------|----------|
| **Spec change** | Modify the canonical spec doc first, then update both TLA+ and Fizzbee models to match |
| **Model correction** | If a model disagrees with the spec, the model is wrong — submit a PR fixing the model |
| **New property** | Propose in an issue first, then add to spec → both models → update property verdicts tables |
| **Implementation fix** | Must match [`examples/s3-queue/SPEC.md`](examples/s3-queue/SPEC.md); if impl and SPEC diverge, file a spec-drift issue |
| **New protocol** | Propose via issue first; must follow the Layer 0 pattern |

### The Spec-First Rule

**All protocol changes start with the spec document.** The order is always:

1. Update the canonical spec (`.md` file at repo root)
2. Update the TLA+ model (`tlaplus/`)
3. Update the Fizzbee model (`fizzbee/`)
4. Update the reference implementation (`examples/`) if applicable
5. Update the property verdicts table in `CLAUDE.md` and `README.md` if properties changed

### Running Verification

Before submitting a PR:

```bash
make verify                # Both TLA+ and Fizzbee must pass

# If changing the Rust implementation:
cd examples/s3-queue/impl
cargo build
cargo test
cargo test --features integration  # Requires MinIO via docker compose
```

### Pull Request Requirements

- [ ] `make verify` passes (both TLA+ and Fizzbee)
- [ ] `cargo test` passes (if implementation changes)
- [ ] Property verdicts table updated (if properties added/removed/changed)
- [ ] Spec-to-model mapping tables updated (if actions/variables changed)
- [ ] Spec doc updated before model/impl changes (spec-first rule)

## Filing Issues

### Issue Categories

| Label | Use For |
|-------|---------|
| `spec-issue` | Protocol correctness concern in a canonical spec |
| `spec-drift` | Implementation doesn't match its spec |
| `model-bug` | TLA+ or Fizzbee model defect |
| `formal-verification` | General formal verification concerns |
| `bug` | Reference implementation bug |
| `feature-request` | New protocol, property, or implementation |

### Where to File

- **Spec or model issues** → file in this repo with the `formal-verification` label
- **Implementation drift** (code doesn't match SPEC.md) → file with the `spec-drift` label

## Code Style

- **TLA+**: Follow the existing naming conventions (PascalCase for actions, camelCase for variables)
- **Fizzbee**: Follow the existing Python-like naming conventions
- **Rust**: Standard `rustfmt` formatting; run `cargo fmt` before committing
- **Markdown**: Keep spec documents self-contained; define terms before use

## License

By contributing, you agree that your contributions will be licensed under the [Apache License 2.0](LICENSE).

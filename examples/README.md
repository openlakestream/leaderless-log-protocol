# Examples

Each example demonstrates two workflows that turn formally verified protocols into working systems.

## Workflow 1: Protocol → System SPEC

Pick a [protocol](../README.md#protocol-family) from this repo and apply it to your specific system to generate a detailed, language-agnostic SPEC.

### Option A. Use your preferred coding agent

Give it the protocol spec and describe the system you want to build:

> I want to build a distributed message queue on S3. Generate a system SPEC based on:
> https://github.com/lakestream-io/leaderless-log-protocol/blob/main/1-leaderless-log-protocol.md

### Option B. Use gstack

Start with `/office-hours` to brainstorm your system design, then generate the system SPEC:

```
/office-hours
```

Describe your system, the protocol you want to use, and gstack will help you think through the design and produce a complete SPEC.

**Example:** S3-Queue's [`SPEC.md`](s3-queue/SPEC.md) was generated this way — brainstormed via `/office-hours` with the Leaderless Log protocol applied to a message queue on S3.

## Workflow 2: SPEC → Implementation

Take an existing SPEC and hand it to a coding agent for one-shot implementation. The SPEC is self-contained — it includes the domain model, state machines, coordination primitives, operation pseudocode, error handling, and a test matrix.

> Implement S3-Queue according to the following spec:
> https://github.com/lakestream-io/leaderless-log-protocol/blob/main/examples/s3-queue/SPEC.md

See the [S3-Queue example](s3-queue/) for how this works end-to-end — from SPEC to a working Rust CLI.

## Available Examples

| Example | Based On | SPEC | Description |
|---------|----------|------|-------------|
| [s3-queue](s3-queue/) | [Leaderless Log](../1-leaderless-log-protocol.md) | [SPEC.md](s3-queue/SPEC.md) | Distributed message queue on S3-compatible object storage. Concurrent producers, background compaction, consumer cursors, fencing — zero external dependencies beyond S3. |

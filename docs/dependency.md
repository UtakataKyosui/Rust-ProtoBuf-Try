ProtoLive — Dependency & Tooling Plan

> Status: Proposed / MVP · Updated: 2026-10-17

1. Purpose

ProtoLive is an experimental real-time text chat application for validating Google’s official Rust Protocol Buffers runtime in a practical Rust–TypeScript environment. The transport is WebSocket binary frames, and the shared wire format is Protocol Buffers (proto3). It is inspired by the communication approach described in REALITY’s engineering articles; it is not a REALITY client.

MVP success: Open two browser tabs with different nicknames, connect both to one Rust server, and exchange text messages encoded as Protobuf binary frames. No database, Redis, avatar, media streaming, or authentication is required for the initial local experiment.

2. Selection Principles

1. Prioritize the Google-maintained Rust protobuf runtime, not prost, because that is the experiment’s purpose.
2. Own the .proto schema once; generate Rust and TypeScript representations rather than duplicating DTOs.
3. Separate transport, codec, application logic, and UI so encoding benchmarks can be run independently of network I/O.
4. Favor stable, widely used dependencies for everything outside the official Rust protobuf experiment.
5. Keep the first release in-memory and single-process; add dependencies only when they solve a real requirement.
6. Pin matching Google protobuf runtime, code generator, and protoc versions; commit lockfiles.

3. Rust Server Dependencies

|Crate                                                                  |Classification                                                    |Purpose                                                         |Phase|
|-----------------------------------------------------------------------|------------------------------------------------------------------|----------------------------------------------------------------|-----|
|`google-protobuf`                                                      |Required; version must be pinned                                  |Official Google Rust Protobuf runtime; beta                     |1    |
|`google-protobuf-codegen`                                              |Build dependency; confirm published crate/API for selected release|Generate Rust definitions from `.proto` using Google’s toolchain|1    |
|`axum` (feature `ws`)                                                  |Required                                                          |HTTP health check and WebSocket endpoint                        |2    |
|`tokio` (features `rt-multi-thread`, `macros`, `net`, `sync`, `signal`)|Required                                                          |Async runtime, channels, graceful shutdown                      |2    |
|`futures-util`                                                         |Required if split stream/sink helpers are used                    |WebSocket frame sending and receiving                           |2    |
|`tracing`                                                              |Required                                                          |Structured event and error logging                              |2    |
|`tracing-subscriber` (features `fmt`, `env-filter`)                    |Required                                                          |Local log formatting and filtering                              |2    |
|`uuid` (feature `v7`)                                                  |Recommended                                                       |Server-generated message and connection IDs                     |2    |
|`thiserror`                                                            |Optional                                                          |Domain/application error types when needed                      |2    |
|`tokio-tungstenite`                                                    |Dev-only candidate                                                |Real WebSocket integration tests                                |2    |
|`criterion`                                                            |Dev-only candidate                                                |Encode/decode microbenchmarks                                   |4    |
|`serde`, `serde_json`                                                  |Benchmark-only                                                    |Comparable JSON encoding baseline; not on the chat wire         |4    |

Key implementation rule: Client-supplied nickname and text are untrusted. The server allocates authoritative message IDs and timestamps; apply nickname/message length checks and frame size limits. Use bounded queues, handle slow receivers, and log protocol failures without dumping personal chat text.

Cargo Manifest Sketch

This example intentionally uses placeholders for the Google beta release. Do not commit wildcard or guessed official protobuf versions. Verify the current release’s exact crate naming and feature/API requirements, then pin the runtime and codegen to the same version and install matching protoc.

[package]
name = "protolive-server"
version = "0.1.0"
edition = "2024"

[dependencies]
# Set both protobuf crates to the same verified Google release.
# google-protobuf = "=VERIFIED_VERSION"
axum = { version = "0.8", features = ["ws"] }
tokio = { version = "1", features = ["rt-multi-thread", "macros", "net", "sync", "signal"] }
futures-util = "0.3"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["fmt", "env-filter"] }
uuid = { version = "1", features = ["v7"] }

[build-dependencies]
# google-protobuf-codegen = "=VERIFIED_VERSION"

Some official releases also expose the Google runtime through the protobuf v4 crate, which reexports google_protobuf. Choose one public dependency path and document it instead of mixing multiple runtimes. The official runtime uses C/upb or C++ kernels, so toolchain and linking requirements differ from pure-Rust protobuf libraries.

4. Frontend Dependencies

|Package                                                       |Classification           |Purpose                                                  |Phase|
|--------------------------------------------------------------|-------------------------|---------------------------------------------------------|-----|
|`react`, `react-dom`                                          |Required                 |Chat UI                                                  |3    |
|`typescript`                                                  |Required                 |Static types                                             |3    |
|`vite`, `@vitejs/plugin-react`                                |Required                 |Development/build                                        |3    |
|`@bufbuild/protobuf`                                          |Required                 |TypeScript Protobuf encode/decode                        |1 / 3|
|`@bufbuild/protoc-gen-es`                                     |Dev dependency           |Generate TS declarations and message definitions         |1    |
|`@bufbuild/buf` or separately installed `buf` CLI             |Development tool         |Schema lint, breaking checks, TS generation orchestration|1    |
|`tailwindcss`, `@tailwindcss/vite`                            |UI option                |Styling                                                  |3    |
|`shadcn` CLI and selected components                          |UI option                |Chat inputs/cards                                        |3    |
|`lucide-react`                                                |Optional                 |UI icons                                                 |3    |
|`vitest`                                                      |Dev dependency           |Codec, hook, utility tests                               |3    |
|`@testing-library/react`, `@testing-library/jest-dom`, `jsdom`|Optional dev dependencies|UI tests                                                 |3    |

Use the browser’s native WebSocket API for the MVP; a React-specific WebSocket hook library is not needed. Set binaryType = "arraybuffer" and send/receive Protobuf bytes in binary frames. Keep the connection lifecycle in one reusable feature module or hook.

5. Schema and Code Generation

Source of truth:

proto/protolive/v1/
  envelope.proto     # oneof client/server event envelopes
  chat.proto         # chat content and server-issued metadata
  presence.proto     # join/leave events

flowchart LR
    S["Shared .proto files"] --> B["Buf lint / breaking checks"]
    S --> R["Google protoc + Rust codegen"]
    S --> T["Buf + protoc-gen-es"]
    R --> RS["Rust protocol module"]
    T --> TS["TypeScript protocol package"]
    RS --> W["WebSocket binary frames"]
    TS --> W

• Use syntax = "proto3", a versioned package such as protolive.v1, and oneof for event envelopes.
• Never reuse removed field numbers or change assigned field numbers.
• Separate ClientMessage (send request) from ServerMessage (authoritative broadcast).
• Make each WebSocket binary message contain one serialized Protobuf envelope; no extra length prefix is required at the WebSocket message layer.
• Treat protobuf wire compatibility and application-semantic compatibility as separate concerns.

Build caveat: Google’s official Rust code generation is strict about the protoc version. The official README specifies that the protobuf crate’s x.y.z version requires protoc y.z. Before adopting the build script, verify the chosen release’s generated-file include pattern and actual public codegen API against the upstream example. The Rust codegen integration is an explicit Phase 1 acceptance gate, not an assumed completed setup.

6. Workspace Layout

protolive/
├── README.md
├── docs/
│   └── DEPENDENCIES.md
├── Cargo.toml
├── Cargo.lock
├── package.json
├── pnpm-workspace.yaml
├── pnpm-lock.yaml
├── buf.yaml
├── buf.gen.yaml
├── proto/protolive/v1/
│   ├── envelope.proto
│   ├── chat.proto
│   └── presence.proto
├── apps/
│   ├── server/
│   │   ├── Cargo.toml
│   │   ├── build.rs
│   │   └── src/
│   └── web/
│       ├── package.json
│       └── src/
├── packages/
│   └── protocol-ts/
└── benchmarks/

Cargo workspace controls Rust crates; pnpm workspace controls TypeScript packages. Do not introduce moonrepo yet: the task graph is small and native workspace tools suffice.

7. Runtime Responsibilities

|Layer                |Responsibility                                 |Explicitly not responsible for|
|---------------------|-----------------------------------------------|------------------------------|
|`.proto`             |Wire schema and field numbering                |React UI state                |
|Rust protobuf adapter|Encode/decode generated messages               |Socket lifecycle              |
|Axum transport       |Connections, binary frames, limits             |Domain validation decisions   |
|Chat service         |Nickname rules, server-issued IDs/times, fanout|Wire encoding details         |
|React transport      |Reconnection state and socket lifecycle        |Rendering UI                  |
|React UI             |Message list, input, connection display        |Defining wire DTOs            |

Use tokio::sync::broadcast for the first in-memory shared room, with an explicit capacity and policy for lagging clients. Do not introduce a database, queue broker, Redis, or authentication until they become requirements. Local experiment exposure should be limited to trusted development networks; a public deployment would require abuse/rate controls and operational review.

8. Test and Benchmark Plan

Phase 1 — Serialization

☐ Verify official runtime/codegen/protoc build with one simple .proto.
☐ Rust encode → Rust decode round-trip.
☐ Rust encode → TypeScript decode, and the reverse, using golden binary fixtures.
☐ Validate oneof, optional values, repeated fields, and unknown-field behavior needed by the protocol.

Phase 2 — WebSocket server

☐ Two connected clients receive the same broadcast.
☐ Reject malformed Protobuf and unsupported frame types.
☐ Enforce payload limits and nickname/text validation.
☐ Disconnect cleanup, cancellation, lagging consumer behavior.

Phase 3 — Browser UI

☐ Display connected/disconnected/reconnecting states.
☐ Send and receive binary frames without JSON conversion.
☐ UI retains a bounded in-memory message list.

Phase 4 — Performance

☐ Compare Protobuf and JSON payload bytes on identical logical messages.
☐ Measure encoding and decoding separately from transport latency.
☐ Record runtime versions, OS/hardware, payload distribution, warmup, and sample counts.

9. Deferred / Not Selected

|Dependency           |Decision                         |Why                                                              |
|---------------------|---------------------------------|-----------------------------------------------------------------|
|`prost`              |Not selected for the main runtime|Would obscure evaluation of Google’s official Rust implementation|
|`tonic`              |Deferred                         |MVP uses WebSocket, not gRPC                                     |
|`react-use-websocket`|Deferred                         |Native WebSocket API is sufficient                               |
|TanStack Query       |Not required                     |No REST-backed server state in the MVP                           |
|Zustand              |Not required                     |Local React state is adequate                                    |
|SQLx / SeaORM        |Not required                     |No persistence                                                   |
|Redis                |Not required                     |Single-process, one-room experiment                              |
|Docker / moonrepo    |Deferred                         |Not required for the first successful round-trip                 |

10. Adoption Gates / Decisions

|Gate                |Exit criterion                                                                               |
|--------------------|---------------------------------------------------------------------------------------------|
|G1: Official codegen|Google-generated Rust messages compile on the selected Rust edition and environment          |
|G2: Cross-language  |Rust and TypeScript exchange fixtures bidirectionally without schema drift                   |
|G3: Chat MVP        |Two browser tabs exchange text over a single WebSocket server using binary protobuf envelopes|
|G4: Robustness      |Invalid inputs, disconnects, lagging clients, and bounded queues behave predictably          |
|G5: Evaluation      |Benchmarks can be reproduced from committed fixtures and documentation                       |

Known uncertainty: Google Rust Protobuf is beta and upstream APIs or crate distribution can change. Version and exact build.rs integration are intentionally left as verification tasks rather than fabricated working commands.

11. References

• Google Protocol Buffers Rust documentation: https://protobuf.dev/reference/rust/
• Google official protobuf Rust crate README: https://github.com/protocolbuffers/protobuf/blob/main/rust/release_crates/protobuf/README.md
• Google Protocol Buffers releases: https://github.com/protocolbuffers/protobuf/releases
• Axum: https://docs.rs/axum/
• Tokio: https://tokio.rs/
• Protobuf-ES: https://protobuf-es.com/
• Buf: https://buf.build/docs/
• Cargo: https://doc.rust-lang.org/cargo/
# CLAUDE.md

## Rules

- **Only commit and push when explicitly instructed.** Never amend commits. Never add `Co-Authored-By` headers.
- This is a public open-source project. All code must be clean, production-quality, and free of secrets or credentials.
- Prefer generic, abstracted, clean code. No hacky fixes (e.g. prefixing unused variables with `_` instead of implementing properly).
- Always optimize for performance. This is a game networking library — speed matters.
- Run `cargo test` and `cargo clippy -- -W clippy::all` before considering any change complete.
- No magic numbers or hardcoded strings. Use named constants.

## Project Overview

GB-Net is a production-ready Rust game networking library providing reliable UDP transport with bitpacked serialization. Transport-level (like ENet/LiteNetLib/yojimbo), sync/polling game-loop model.

## Structure

- `gbnet/` - Core library crate
  - `src/lib.rs` - Public API, re-exports, prelude
  - `src/serialize.rs` - Bitpacked serialization (BitBuffer, BitSerialize/BitDeserialize traits)
  - `src/packet.rs` - Packet header and type definitions, wire format
  - `src/channel.rs` - 5 delivery modes (Unreliable, UnreliableSequenced, ReliableUnordered, ReliableOrdered, ReliableSequenced)
  - `src/connection.rs` - Connection state machine, per-connection channel management
  - `src/reliability.rs` - Jacobson/Karels RTT, adaptive RTO, fast retransmit, packet loss tracking
  - `src/security.rs` - CRC32C integrity, connect tokens, rate limiting
  - `src/fragment.rs` - Message fragmentation/reassembly, MTU discovery
  - `src/server.rs` - NetServer API (bind, update, send, broadcast, disconnect)
  - `src/client.rs` - NetClient API (connect, update, send, disconnect)
  - `src/congestion.rs` - Binary congestion control, message batching, bandwidth tracking
  - `src/simulator.rs` - Network condition simulator (loss, latency, jitter, duplicates)
  - `src/config.rs` - NetworkConfig, ChannelConfig, DeliveryMode
  - `src/util.rs` - Sequence number utilities
  - `src/tests/` - Unit tests
  - `tests/integration.rs` - Integration tests
- `gbnet_macros/` - Proc macro crate for `#[derive(NetworkSerialize)]`

## Build & Test

```
cargo test          # Run all tests
cargo clippy -- -W clippy::all   # Lint
cargo fmt --check   # Format check
```

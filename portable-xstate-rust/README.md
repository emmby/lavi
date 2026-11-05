# Portable XState Rust

A pure Rust implementation of the Portable XState runtime configuration and execution logic.

## Features

- **Pure Rust**: Zero dependencies.
- **Domain Model**: Defines the explicit structure of a State Machine (`MachineDefinition`, `StateDefinition`, etc.).
- **Runtime**: Implements the `RuntimeMachine` capable of performing transitions based on events.

## Usage

This crate can be used directly in Rust or by target adapters like `portable-xstate-wasm` and `portable-xstate-c-cpp`.

```rust
use portable_xstate_rust::{RuntimeMachine, MachineDefinition, Value};

// Construct definition manually...
let machine = RuntimeMachine::new(definition);
```

# Portable XState Core

A pure Rust implementation of the Portable XState runtime configuration and execution logic.

## Features

- **Pure Rust**: Zero dependencies.
- **Domain Model**: Defines the explicit structure of a State Machine (`MachineDefinition`, `StateDefinition`, etc.).
- **Runtime**: Implements the `RuntimeMachine` capable of performing transitions based on events.

## Usage

This crate is primarily intended to be used by consumers like `portable-xstate-json` or `portable-xstate-wasm`, but can be used directly if you manually construct the `MachineDefinition`.

```rust
use portable_xstate_core::{RuntimeMachine, MachineDefinition, Value};

// Construct definition manually...
let machine = RuntimeMachine::new(definition);
```

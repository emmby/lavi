# Lavi (`lavi`)

A pure `#![no_std]` + `alloc` Rust statechart and actor runtime compatible with the **XState v5** specification and JSON schema.

## Features

- **`#![no_std]` + `alloc` Core**: Depends only on `indexmap` (zero `serde` overhead).
- **Bounded $O(1)$ Call-Stack**: Iterative JSON parser (`src/json.rs`), state tree builder (`src/parser.rs`), and runtime traversal using explicit heap work-stacks (`Vec`), safe for 8 KB microcontroller stacks.
- **XState v5 Semantics**: Hierarchical and parallel states, LCCA transition conflict resolution, entry/exit/transition action ordering, `always` eventless transitions, `assign`, `raise`, `after` timers, `SimulatedClock`, history states, `.matches()`, `.can()`, and `.has_tag()`.

## Usage

This crate can be used directly in Rust or through target adapters like `lavi-wasm` and `lavi-c-cpp`.

```rust
use lavi::{RuntimeMachine, Value};
use indexmap::IndexMap;

let mut machine = RuntimeMachine::from_json(r#"{
    "id": "toggle",
    "initial": "inactive",
    "states": {
        "inactive": { "on": { "TOGGLE": "active" } },
        "active": { "on": { "TOGGLE": "inactive" } }
    }
}"#).unwrap();

machine.start(&IndexMap::new(), &IndexMap::new());
assert!(machine.matches("inactive"));

machine.transition("TOGGLE", &Value::event("TOGGLE"), &IndexMap::new(), &IndexMap::new());
assert!(machine.matches("active"));
```

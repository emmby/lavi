# Lavi (`lavi`)

**Lavi** is a portable, `#![no_std]` + `alloc` statechart and actor runtime compatible with **XState v5** and W3C SCXML semantics.

## Features

- **`#![no_std]` + `alloc` Core**: `indexmap` (`default-features = false`) is the sole dependency; zero `serde` overhead.
- **Bounded $O(1)$ CPU Call Stack**: Iterative JSON parsing and state-tree traversal using explicit heap work-stacks (`Vec`), fitting comfortably inside an 8 KB microcontroller RTOS stack.
- **XState v5 Semantics**: Hierarchical and parallel states, LCCA transition domains, `entry` / `exit` / transition actions, `assign` context updates, `raise` internal events, `always` eventless transitions, `after` delayed transitions, shallow & deep `history` states, `.matches()`, `.can()`, `.has_tag()`, and snapshot persistence.

## Installation

```bash
cargo add lavi
```

Or in a `#![no_std]` crate:

```toml
[dependencies]
lavi = { version = "0.1.0", default-features = false }
```

## Usage

```rust
use indexmap::IndexMap;
use lavi::{ActionFunction, GuardFunction, RuntimeMachine, SimulatedClock, Value};
use std::rc::Rc;

const MACHINE_JSON: &str = r#"{
  "id": "stoplight",
  "initial": "red",
  "context": { "cycles": 1 },
  "states": {
    "red": { "after": { "1000": "green" } },
    "green": {
      "on": { "PEDESTRIAN_WAIT": { "target": "yellow", "guard": "canCross" } },
      "after": { "1000": "yellow" }
    },
    "yellow": { "after": { "1000": "red" } }
  }
}"#;

let clock = SimulatedClock::with_initial(0);
let mut machine = RuntimeMachine::from_json(MACHINE_JSON).unwrap();
machine.register_clock(&clock);

let mut guards: IndexMap<String, GuardFunction> = IndexMap::new();
guards.insert(
    "canCross".to_string(),
    Rc::new(|ctx, _evt, _params| {
        ctx.as_object()
            .and_then(|o| o.get("cycles"))
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0) >= 1.0
    }),
);
let actions: IndexMap<String, ActionFunction> = IndexMap::new();

machine.start(&guards, &actions);
assert!(machine.matches("red"));

clock.increment(1000, &mut machine, &guards, &actions);
assert!(machine.matches("green"));

machine.transition("PEDESTRIAN_WAIT", &Value::event("PEDESTRIAN_WAIT"), &guards, &actions);
assert!(machine.matches("yellow"));
```

## License

MIT

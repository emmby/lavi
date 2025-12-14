# Portable XState JSON

Handles parsing of XState-compatible JSON configurations into `portable-xstate-core` domain models.

## Features

- **JSON Parsing**: Uses `serde` and `serde_json` to parse JSON strings.
- **Mapping**: Converts loose JSON structures (DTOs) into strict `portable-xstate-core` structs.

## Usage

```rust
use portable_xstate_json::from_json_str;
use portable_xstate_core::RuntimeMachine;

let json = r#"{ "id": "toggle", "initial": "inactive", "states": { ... } }"#;
let definition = from_json_str(json).expect("Invalid JSON");
let machine = RuntimeMachine::new(definition);
```

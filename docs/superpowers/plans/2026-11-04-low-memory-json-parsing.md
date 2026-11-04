# Low-Memory, Zero-Serde JSON Parsing Implementation Plan [flowos: aab4f5c7-5b4d-4211-b92d-54a807f40ef1]

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace `serde` and `serde_json` across the `portable-xstate` workspace with a bounded-$O(1)$ call-stack, move-based iterative JSON parser and serializer in `portable-xstate-core`, consolidating `portable-xstate-json` into `portable-xstate-core` and eliminating 3x heap peak duplication during state machine creation.

**Architecture:** `portable-xstate-core` gains two `#![no_std]` modules: `json.rs` (iterative byte-cursor `Value::from_json` and `Value::to_json_string` backed by explicit `Vec` heap stacks) and `parser.rs` (move-based `MachineDefinition::from_value` using an explicit `Vec<StateWorkFrame>` heap stack and `IndexMap::into_iter` / `swap_remove`). `RuntimeMachine::from_definition` takes ownership of `MachineDefinition` without cloning, `portable-xstate-wasm` converts `JsValue` directly via `js_sys`, and `portable-xstate-json`, `serde`, `serde_json`, `serde-wasm-bindgen`, and `cty` are removed.

**Tech Stack:** Rust (`edition = "2021"`, `#![no_std]` + `alloc`), `indexmap = "2.7"` (without `"serde"`), `wasm-bindgen`, `js-sys`, `web-sys`.

**Spec:** `docs/superpowers/specs/2026-11-04-low-memory-json-parsing-design.md`

## Global Constraints

- `#![no_std]` + `alloc` Compatibility: `portable-xstate-core` and `portable-xstate-ffi` must compile with `#![no_std]` using only `core`, `alloc`, and `indexmap`.
- Bounded $O(1)$ Call-Stack Parsing: Parse arbitrary-depth JSON definitions and construct `MachineDefinition` using explicit heap work-stacks (`Vec`) instead of CPU call-stack recursion, comfortably running within the default 8 KB ESP32 `loopTask` stack without `SET_LOOP_TASK_STACK_SIZE`.
- Eliminate 3x Heap Peak: Parse JSON into `portable_xstate_core::Value` iteratively and consume `Value` by move (`into_iter()` / `swap_remove()`) to build `MachineDefinition` without reallocating strings or cloning `context`, then move `MachineDefinition` directly into `RuntimeMachine`.
- Drop `serde` Ecosystem & Unused Dependencies: Remove `serde`, `serde_derive`, `serde_json`, `serde-wasm-bindgen`, and `cty` from the workspace; keep `indexmap` with its `"serde"` feature removed.
- Insertion-Order Preservation: Preserve exact definition order of `states`, `on` transitions, `after` timers, and object keys via `IndexMap`.
- No Breaking Public API Changes: Existing C FFI (`portable_xstate.h`), C++ wrapper (`portable_xstate.hpp`), and WASM/JS (`Actor`, `StateNode`, `createMachine`, `createActor`, `setup`, `assign`) signatures and behavior must remain 100% compatible with existing tests.
- Full Test Suite Parity: All existing tests (`npm run test:native`, `npm run test:wasm`, `npm run test:e2e`, `npm run test:ffi`, `npm run test:unit`) must pass without relaxing assertions.

## Review Focus

1. **Unpaired UTF-16 surrogates in JSON strings** (`"\uD800"` or `"\uD800\u0041"` or lone low surrogate `"\uDC00"`): must return `Err(JsonError)` rather than panicking or silently producing replacement characters. (Tested in Task 1: `test_json_unpaired_utf16_surrogates_rejected`)
2. **Malformed JSON numbers** (`"-"` alone, leading zeros like `"01"`, trailing decimal `"1."`, or overflow to `Infinity`): must return `Err(JsonError)` rather than accepting invalid JSON grammar or storing `f64::INFINITY`/`NaN`. (Tested in Task 1: `test_json_invalid_numbers_rejected`)
3. **State definition order when building `states` iteratively with a heap work-stack**: pushing children onto a LIFO `Vec` stack can accidentally reverse sibling or child insertion order in `StateDefinition.states` and `id_map` if not iterated via `IndexMap::into_iter()` in forward order. (Tested in Task 2: `test_iterative_states_preserves_exact_insertion_order`)
4. **Polymorphic `cond` vs `guard` alias and `after` array vs object format with string/numeric delays**: XState v5 configs use both `"cond"` and `"guard"` on transitions and both `"after": { "1000": "next" }` and `"after": [{ "delay": 1000, "target": "next" }]`. (Tested in Task 2: `test_guard_alias_and_after_formats`)
5. **JS objects containing `undefined` or function properties passed into `js_to_value` in WASM**: `serde_wasm_bindgen` and `JSON.stringify` omit or null out `undefined`/function properties on context/event objects rather than panicking; `js_to_value` must handle them cleanly. (Tested in Task 4: `test_verification_assign_machine` and `test_js_to_value_ignores_undefined_and_functions`)

## File Structure

- **Create `portable-xstate-core/src/json.rs`**: Defines `JsonError`, iterative `Value::from_json(&str) -> Result<Value, JsonError>`, and iterative `Value::to_json_string(&self) -> String`.
- **Create `portable-xstate-core/src/parser.rs`**: Defines `MachineDefinition::from_value(Value) -> Result<MachineDefinition, JsonError>`, `create_machine`, `from_json_str`, `SetupObject`, `setup`, `assign`, and `value_to_state_value`.
- **Modify `portable-xstate-core/src/lib.rs`**: Exports `json` and `parser` modules and root re-exports; adds `RuntimeMachine::from_definition(MachineDefinition)` and `RuntimeMachine::from_json(&str)`.
- **Create `portable-xstate-core/tests/*.rs`**: `json_tests.rs`, `parser_tests.rs`, plus migrated tests (`deserialization.rs`, `history_mapping.rs`, `raise_action_test.rs`, `repro_error.rs`).
- **Modify `portable-xstate-ffi/Cargo.toml` & `portable-xstate-ffi/src/lib.rs`**: Drops `portable-xstate-json` and `cty`; switches `fsm_create` to `portable_xstate_core::from_json_str` and `RuntimeMachine::from_definition`.
- **Modify `portable-xstate-wasm/Cargo.toml`, `src/utils.rs`, `src/lib.rs`, `tests/verification.rs`**: Replaces `serde_json` / `serde_wasm_bindgen` / `portable_xstate_json` with `js_to_value`, `value_to_js`, `Value::from_json`, `Value::to_json_string`, and `portable_xstate_core` functions.
- **Modify `Cargo.toml` & remove `portable-xstate-json/`**: Removes `portable-xstate-json`, `serde`, `serde_json`, and `serde-wasm-bindgen` from workspace.

---

## Phase 1: Low-Memory JSON Parser & Workspace Consolidation [flowos: e513192d-44a5-481e-8495-6530eba5c43a]

### Task 1: Iterative `Value::from_json` and `Value::to_json_string` in `portable-xstate-core` [flowos: 42c31dd5-fb5c-4646-9bf8-e0e70b551b7e]

**Files:**
- Create: `portable-xstate-core/src/json.rs`
- Modify: `portable-xstate-core/src/lib.rs:1-60`
- Test: `portable-xstate-core/tests/json_tests.rs`

**Interfaces:**
- Consumes: `portable_xstate_core::Value`, `indexmap::IndexMap`
- Produces:
  - `pub struct JsonError { pub message: alloc::string::String, pub offset: usize }` (implements `core::fmt::Display`, re-exported at `portable_xstate_core::JsonError`)
  - `impl Value { pub fn from_json(input: &str) -> Result<Value, JsonError>; pub fn to_json_string(&self) -> alloc::string::String; }`

- [ ] **Step 1: Write the failing tests in `portable-xstate-core/tests/json_tests.rs`**

```rust
use portable_xstate_core::{JsonError, Value};

#[test]
fn test_json_primitives_and_containers() {
    let input = r#"{"a":null,"b":true,"c":false,"d":42,"e":-3.5e2,"f":"hello\n\"world\"","g":[1,2,{"nested":"ok"}]}"#;
    let val = Value::from_json(input).expect("should parse valid JSON");
    let obj = val.as_object().unwrap();
    assert_eq!(obj.get("a"), Some(&Value::Null));
    assert_eq!(obj.get("b"), Some(&Value::Bool(true)));
    assert_eq!(obj.get("c"), Some(&Value::Bool(false)));
    assert_eq!(obj.get("d"), Some(&Value::Number(42.0)));
    assert_eq!(obj.get("e"), Some(&Value::Number(-350.0)));
    assert_eq!(obj.get("f"), Some(&Value::String("hello\n\"world\"".to_string())));

    // Roundtrip serialization preserves key order and formats whole numbers as integers
    let out = val.to_json_string();
    assert_eq!(
        out,
        r#"{"a":null,"b":true,"c":false,"d":42,"e":-350,"f":"hello\n\"world\"","g":[1,2,{"nested":"ok"}]}"#
    );
}

#[test]
fn test_json_unicode_and_surrogate_pairs() {
    let val = Value::from_json(r#"{"crab":"\uD83E\uDD80","ascii":"\u0041"}"#).unwrap();
    let obj = val.as_object().unwrap();
    assert_eq!(obj.get("crab"), Some(&Value::String("🦀".to_string())));
    assert_eq!(obj.get("ascii"), Some(&Value::String("A".to_string())));
}

#[test]
fn test_json_unpaired_utf16_surrogates_rejected() {
    for bad in [r#""\uD800""#, r#""\uD800\u0041""#, r#""\uDC00""#, r#""\uD800x""#] {
        let res = Value::from_json(bad);
        assert!(res.is_err(), "Expected error for unpaired surrogate: {}", bad);
    }
}

#[test]
fn test_json_invalid_numbers_rejected() {
    for bad in ["-", "01", "-01", "1.", ".5", "1e", "1e+", "NaN", "Infinity", "1 2"] {
        let res = Value::from_json(bad);
        assert!(res.is_err(), "Expected error for invalid number/trailing token: {}", bad);
    }
}

#[test]
fn test_json_deep_nesting_in_small_stack_thread() {
    let depth = 256;
    let mut s = String::new();
    for _ in 0..depth {
        s.push_str(r#"{"a":["#);
    }
    s.push_str("1");
    for _ in 0..depth {
        s.push_str("]}");
    }

    let handle = std::thread::Builder::new()
        .stack_size(64 * 1024)
        .spawn(move || {
            let v = Value::from_json(&s).expect("deep parse succeeds on small stack");
            let roundtrip = v.to_json_string();
            assert_eq!(roundtrip, s);
        })
        .unwrap();
    handle.join().unwrap();
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --package portable-xstate-core --test json_tests`
Expected: FAIL with `unresolved import portable_xstate_core::JsonError` and `no function or associated item named from_json found for enum Value`.

- [ ] **Step 3: Implement `JsonError`, `Value::from_json`, and `Value::to_json_string` in `portable-xstate-core/src/json.rs` and export in `portable-xstate-core/src/lib.rs`**

In `portable-xstate-core/src/lib.rs`, add `pub mod json;` and `pub use json::JsonError;`.
In `portable-xstate-core/src/json.rs`:
- Define `JsonError { pub message: String, pub offset: usize }` with `core::fmt::Display`.
- Implement `Value::from_json(input: &str) -> Result<Value, JsonError>` using a byte cursor `(bytes: &[u8], pos: usize)` and an explicit heap stack `Vec<ContainerFrame>`:
  ```rust
  enum ObjectParseState {
      KeyOrEnd,
      Colon,
      Value,
      CommaOrEnd,
  }

  enum ContainerFrame {
      Array {
          items: Vec<Value>,
          expect_value_or_end: bool,
          expect_comma_or_end: bool,
      },
      Object {
          map: IndexMap<String, Value>,
          pending_key: Option<String>,
          state: ObjectParseState,
      },
  }
  ```
  - When a leaf value (`Null`, `Bool`, `Number`, `String`) or a closed container (`}` / `]`) completes, attach it to the top `ContainerFrame` on `stack` (moving `Object` state to `CommaOrEnd` and `Array` to `expect_comma_or_end = true`), or if `stack` is empty, verify only trailing whitespace remains and return `Ok(val)`.
  - Fast-path strings without `\` by slicing `&input[start..end]`; handle `\"`, `\\`, `\/`, `\b`, `\f`, `\n`, `\r`, `\t`, and `\uXXXX` (combining high surrogates `0xD800..=0xDBFF` with a mandatory following `\uDC00..=0xDFFF` low surrogate, returning `Err(JsonError)` on unpaired surrogates or unescaped control chars `< 0x20`).
  - Validate JSON number syntax (`-? (0 | [1-9][0-9]*) (\.[0-9]+)? ([eE][+-]?[0-9]+)?`) before calling `str::parse::<f64>()`, and reject `!n.is_finite()`.
- Implement `Value::to_json_string(&self) -> String` using an explicit heap stack `Vec<SerializeFrame<'_>>`:
  - Format `Value::Number(n)` as `(n as i64)` when `n.is_finite() && n.fract() == 0.0 && n.abs() < ((1i64 << 53) as f64)`, as `"null"` if `!n.is_finite()`, and via `format!("{}", n)` otherwise.
  - Escape `"`, `\`, `\n`, `\r`, `\t`, `\x08` (`\b`), `\x0c` (`\f`), and other control characters `< 0x20` as `\u00XX`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --package portable-xstate-core --test json_tests`
Expected: PASS (5 tests passing)

- [ ] **Step 5: Commit**

```bash
git add portable-xstate-core/src/lib.rs portable-xstate-core/src/json.rs portable-xstate-core/tests/json_tests.rs
git commit -m "feat(core): add iterative zero-serde Value::from_json and to_json_string"
```

---

### Task 2: Iterative Move-Based `MachineDefinition` Builder & Public XState API in `portable-xstate-core` [flowos: e4e2425d-7695-4984-abf1-355d68a1502a]

**Files:**
- Create: `portable-xstate-core/src/parser.rs`
- Modify: `portable-xstate-core/src/lib.rs:210-265`
- Test: `portable-xstate-core/tests/parser_tests.rs`

**Interfaces:**
- Consumes: `Value::from_json`, `JsonError` from Task 1
- Produces (in `portable_xstate_core`):
  - `impl MachineDefinition { pub fn from_value(val: Value) -> Result<MachineDefinition, JsonError>; }`
  - `pub fn create_machine(json: &str) -> Result<MachineDefinition, JsonError>`
  - `pub fn from_json_str(json: &str) -> Result<MachineDefinition, JsonError>`
  - `pub struct SetupObject` with `pub fn create_machine(&self, json: &str) -> Result<MachineDefinition, JsonError>`
  - `pub fn setup(options: &str) -> SetupObject`
  - `pub fn assign(assignment_json: &str) -> Result<Action, JsonError>`
  - `pub fn value_to_state_value(v: Value) -> StateValue`
  - `impl RuntimeMachine { pub fn from_definition(definition: MachineDefinition) -> Result<Self, String>; pub fn from_json(json: &str) -> Result<Self, String>; }`

- [ ] **Step 1: Write the failing tests in `portable-xstate-core/tests/parser_tests.rs`**

```rust
use portable_xstate_core::{
    assign, create_machine, setup, value_to_state_value, Action, HistoryType,
    RuntimeMachine, StateType, StateValue, TransitionGuard, Value,
};

#[test]
fn test_iterative_states_preserves_exact_insertion_order() {
    let json = r#"{
        "id": "orderTest",
        "states": {
            "z_first": {
                "states": {
                    "c_sub1": {},
                    "a_sub2": {},
                    "b_sub3": {}
                }
            },
            "a_second": {},
            "m_third": {}
        }
    }"#;
    let def = create_machine(json).expect("valid machine");
    // When initial is omitted at root, first key in states definition order is selected
    assert_eq!(def.initial, "z_first");
    let top_keys: Vec<&str> = def.states.keys().map(|k| k.as_str()).collect();
    assert_eq!(top_keys, vec!["z_first", "a_second", "m_third"]);

    let z_sub_keys: Vec<&str> = def.states["z_first"].states.keys().map(|k| k.as_str()).collect();
    assert_eq!(z_sub_keys, vec!["c_sub1", "a_sub2", "b_sub3"]);
}

#[test]
fn test_guard_alias_and_after_formats() {
    let json = r#"{
        "id": "poly",
        "initial": "s1",
        "states": {
            "s1": {
                "after": [
                    { "delay": 250, "target": "s2", "guard": "isReady" },
                    { "delay": "500", "target": "s3", "cond": { "type": "minCount", "params": { "min": 3 } } }
                ]
            },
            "s2": {
                "after": {
                    "1000": "s3"
                }
            },
            "s3": {}
        }
    }"#;
    let def = create_machine(json).expect("valid machine");
    let s1 = &def.states["s1"];
    assert_eq!(s1.after[&250][0].cond, Some(TransitionGuard::Name("isReady".to_string())));
    match &s1.after[&500][0].cond {
        Some(TransitionGuard::Object(g)) => assert_eq!(g.guard_type, "minCount"),
        other => panic!("Expected GuardObject, got {:?}", other),
    }
    assert_eq!(def.states["s2"].after[&1000][0].target, Some(vec!["s3".to_string()]));
}

#[test]
fn test_setup_assign_and_value_to_state_value() {
    let def = setup("{}").create_machine(r#"{"id":"m","initial":"a","states":{"a":{}}}"#).unwrap();
    assert_eq!(def.id, "m");

    let act = assign(r#"{"count": 5}"#).unwrap();
    match act {
        Action::Assign(a) => assert_eq!(a.action_type, "xstate.assign"),
        _ => panic!("Expected Action::Assign"),
    }

    let sv = value_to_state_value(Value::from_json(r#"{"parent":{"r1":"a","r2":"b"}}"#).unwrap());
    assert_eq!(sv.get_active_paths(), vec!["parent.r1.a".to_string(), "parent.r2.b".to_string()]);
}

#[test]
fn test_deep_machine_on_small_stack() {
    // Build a 32-level nested compound state machine
    let depth = 32;
    let mut json = String::from(r#"{"id":"deep","initial":"s0","context":{"n":1},"states":{"#);
    for i in 0..depth {
        if i < depth - 1 {
            json.push_str(&format!(r#""s{}":{{"initial":"s{}","states":{{"#, i, i + 1));
        } else {
            json.push_str(&format!(r#""s{}":{{"on":{{"PING":"s{}"}}}}"#, i, i));
        }
    }
    for _ in 0..depth - 1 {
        json.push_str("}}");
    }
    json.push_str("}}");

    let handle = std::thread::Builder::new()
        .stack_size(64 * 1024)
        .spawn(move || {
            let mut machine = RuntimeMachine::from_json(&json).expect("should build on small stack");
            machine.start(&indexmap::IndexMap::new());
            assert!(machine.state().ends_with("s31"));
        })
        .unwrap();
    handle.join().unwrap();
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --package portable-xstate-core --test parser_tests`
Expected: FAIL with `unresolved import portable_xstate_core::create_machine`.

- [ ] **Step 3: Implement `portable-xstate-core/src/parser.rs` and `RuntimeMachine::from_definition` / `from_json` in `portable-xstate-core/src/lib.rs`**

In `portable-xstate-core/src/lib.rs`:
- Update `RuntimeMachine` with:
  - `pub fn from_definition(mut definition: MachineDefinition) -> Result<Self, String>`: resolves `initial_state` and `initial_actions` from `&definition`, extracts `let context = definition.context.take().unwrap_or_else(|| Value::Object(IndexMap::new()));`, and moves `definition` directly into `Self` without cloning `MachineDefinition`.
  - `pub fn from_json(json: &str) -> Result<Self, String>`: calls `crate::from_json_str(json).map_err(|e| e.to_string()).and_then(Self::from_definition)`.
  - `pub fn new(definition: &MachineDefinition) -> Result<Self, String>`: delegates to `Self::from_definition(definition.clone())`.
- Export `pub mod parser;` and `pub use parser::{assign, create_machine, from_json_str, setup, value_to_state_value, SetupObject};`.

In `portable-xstate-core/src/parser.rs`:
- Implement `MachineDefinition::from_value(val: Value) -> Result<MachineDefinition, JsonError>` consuming `Value::Object(mut root)` by move (`root.swap_remove(...)`):
  - Use an explicit heap stack `Vec<StateWorkFrame>` to build nested `states` iteratively without CPU call-stack recursion while preserving exact `IndexMap` insertion order:
    ```rust
    struct StateWorkFrame {
        key: String,
        current_path: String,
        state_def: StateDefinition,
        remaining_children: indexmap::map::IntoIter<String, Value>,
    }
    ```
  - Mirror all field mappings from `portable-xstate-json/src/dto.rs` and `mapper.rs` by move:
    - `id` (defaults to `"(machine)"` at root; populates `id_map` with `id` or `"{machine_id}.{current_path}"` for states).
    - `initial`: string or object `{ target, actions }`. If root `initial` is `None`, fallback to `states.keys().next().cloned().unwrap_or_default()`.
    - `type` (`type_`) and `StateType` inference (`"parallel"`, `"history"`, `"final"`, `"compound"`, `"atomic"`).
    - `history`: `Some(Value::String("deep"))` or `Some(Value::Object(o))` with `o.get("history") == Some("deep")` sets `deep: true`; any `history` field (unless `Bool(false)` or `Null`) or `type == "history"` sets `HistoryType::Deep` / `HistoryType::Shallow`.
    - `target` and `actions` on history states mapped to `history_target` (first target string) and `history_actions`.
    - `always`: parsed via `parse_transition_list` and inserted into `on_map` under key `"".to_string()` if non-empty.
    - `on`: `Value::Object(map)` consumed via `into_iter()`, mapping each value via `parse_transition_list` (handles string target, transition object, array of transitions, or `null`).
    - `after`: handles both `Value::Object(map)` (keys parsed as `u64`) and `Value::Array(arr)` (extracting `"delay"` as `u64` from number or string and grouping transitions into `IndexMap<u64, Vec<TransitionDefinition>>`).
    - `cond` or `"guard"` alias on transition objects mapped to `TransitionGuard::Name` or `TransitionGuard::Object`.
    - `entry`, `exit`, `actions`: parsed via `parse_action_list`, handling `"xstate.assign"` (`assignment` or `params`), `"xstate.raise"` (`event` or `params`), and generic action objects.
    - `invoke`, `tags`, `context`, `types`: moved directly without cloning.
- Implement `create_machine`, `from_json_str`, `SetupObject`, `setup`, `assign`, and iterative/bounded `value_to_state_value(v: Value) -> StateValue`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --package portable-xstate-core`
Expected: PASS (all unit, `json_tests`, and `parser_tests` passing)

- [ ] **Step 5: Commit**

```bash
git add portable-xstate-core/src/lib.rs portable-xstate-core/src/parser.rs portable-xstate-core/tests/parser_tests.rs
git commit -m "feat(core): add iterative move-based MachineDefinition parser and RuntimeMachine::from_definition"
```

---

### Task 3: Migrate `portable-xstate-json` Tests & Update `portable-xstate-ffi` [flowos: 9e4b50fa-7a85-4b58-9b0e-bd49a1e20619]

**Files:**
- Create: `portable-xstate-core/tests/deserialization.rs`
- Create: `portable-xstate-core/tests/history_mapping.rs`
- Create: `portable-xstate-core/tests/raise_action_test.rs`
- Create: `portable-xstate-core/tests/repro_error.rs`
- Modify: `portable-xstate-ffi/Cargo.toml:1-14`
- Modify: `portable-xstate-ffi/src/lib.rs:1-82`

**Interfaces:**
- Consumes: `portable_xstate_core::{create_machine, from_json_str, setup, assign, RuntimeMachine}` from Task 2
- Produces: `portable-xstate-ffi` compiled with zero `portable-xstate-json` or `cty` dependencies and zero `MachineDefinition` cloning in `fsm_create`.

- [ ] **Step 1: Migrate tests from `portable-xstate-json` to `portable-xstate-core/tests/`**

Create `portable-xstate-core/tests/deserialization.rs`, `history_mapping.rs`, `raise_action_test.rs`, and `repro_error.rs`:
- In `portable-xstate-core/tests/deserialization.rs`:
  - Migrate `test_deserialize_states`, `test_deserialize_always_string`, `test_reproduce_vec_or_single_error`, and `test_vec_or_single_null` from `portable-xstate-json/tests/deserialization.rs`, plus `test_create_machine_api`, `test_setup_api`, `test_assign_api`, `test_history_mapping`, `test_implicit_history_mapping`, and `test_explicit_deep_history_mapping` from `portable-xstate-json/src/lib.rs`.
  - Replace `StateDefinitionDto` / `MachineDefinitionDto` / `serde_json` usage with `portable_xstate_core::create_machine` inspecting `MachineDefinition` and `StateDefinition` directly (e.g., `s.on.get("")` for `always`).
- In `history_mapping.rs`, `raise_action_test.rs`, and `repro_error.rs`: replace `use portable_xstate_json::...` with `use portable_xstate_core::...`.

- [ ] **Step 2: Run the migrated `portable-xstate-core` tests**

Run: `cargo test --package portable-xstate-core`
Expected: PASS

- [ ] **Step 3: Update `portable-xstate-ffi/Cargo.toml` and `portable-xstate-ffi/src/lib.rs`**

In `portable-xstate-ffi/Cargo.toml`:
- Remove `portable-xstate-json = { path = "../portable-xstate-json" }` and `cty = "0.2"`.

In `portable-xstate-ffi/src/lib.rs`:
- Replace `use portable_xstate_json::from_json_str;` with `use portable_xstate_core::{from_json_str, RuntimeMachine, Value};`.
- In `fsm_create` (lines 64-72), pass owned `def` to `RuntimeMachine::from_definition(def)` instead of `RuntimeMachine::new(&def)`.

- [ ] **Step 4: Run FFI native, C, and C++ tests to verify they pass**

Run: `cargo test --package portable-xstate-ffi && npm run test:ffi`
Expected: PASS (`All C FFI tests passed!`, `All C++ FFI tests passed!`)

- [ ] **Step 5: Commit**

```bash
git add portable-xstate-core/tests/ portable-xstate-ffi/Cargo.toml portable-xstate-ffi/src/lib.rs
git commit -m "refactor(ffi,core): migrate json tests to core and use RuntimeMachine::from_definition in ffi"
```

---

### Task 4: Remove `serde` Ecosystem & `portable-xstate-json` from `portable-xstate-wasm` and Workspace [flowos: 4566cbff-fa3f-4f55-8379-d7842b5572e7]

**Files:**
- Modify: `portable-xstate-wasm/Cargo.toml:1-21`
- Modify: `portable-xstate-wasm/src/utils.rs:1-52`
- Modify: `portable-xstate-wasm/src/lib.rs:1-2160`
- Modify: `portable-xstate-wasm/tests/verification.rs:70-135`
- Modify: `Cargo.toml:1-16`
- Delete: `portable-xstate-json/`

**Interfaces:**
- Consumes: `Value::from_json`, `Value::to_json_string`, `create_machine`, `value_to_state_value`, `RuntimeMachine::from_definition` from `portable_xstate_core`
- Produces (in `portable-xstate-wasm/src/utils.rs`):
  - `pub fn js_to_value(val: &JsValue) -> Value`
  - `pub fn value_to_js(val: &Value) -> JsValue`

- [ ] **Step 1: Add unit test in `portable-xstate-wasm/src/utils.rs` and update `portable-xstate-wasm/tests/verification.rs` away from `serde_json` / `serde_wasm_bindgen`**

In `portable-xstate-wasm/tests/verification.rs`:
- Replace `serde_json::from_str` in `verify_scenario` (lines 77-92) with `portable_xstate_core::Value::from_json(w_json).unwrap()` and `portable_xstate_core::Value::from_json(&x_json).unwrap()`, using `as_object().unwrap().clone()` and `swap_remove("type")` / `swap_remove("machine")`.
- Replace `serde_wasm_bindgen::to_value(&serde_json::json!({ "count": 10 })).unwrap()` in `test_assign_identity` (line 125) with `js_sys::JSON::parse(r#"{"count":10}"#).unwrap()`.
- Add a `#[wasm_bindgen_test]` `test_js_to_value_ignores_undefined_and_functions` in `portable-xstate-wasm/tests/verification.rs` verifying that an object with `{ a: 1, b: undefined, c: () => {} }` converts via `Actor::new` / `provide` context without error and preserves `a: 1`.

- [ ] **Step 2: Implement `js_to_value` and `value_to_js` in `portable-xstate-wasm/src/utils.rs` and replace all `serde` / `portable_xstate_json` usages in `portable-xstate-wasm/src/lib.rs`**

In `portable-xstate-wasm/src/utils.rs`:
- Implement `pub fn js_to_value(val: &JsValue) -> Value`:
  - `val.is_null() || val.is_undefined() || val.is_function()` $\rightarrow$ `Value::Null`
  - `val.as_bool()` $\rightarrow$ `Value::Bool(b)`
  - `val.as_f64()` $\rightarrow$ `Value::Number(n)`
  - `val.as_string()` $\rightarrow$ `Value::String(s)`
  - `js_sys::Array::is_array(val)` $\rightarrow$ maps elements with `js_to_value` into `Value::Array`
  - `val.is_object()` $\rightarrow$ iterates `js_sys::Object::keys(&obj)` and `js_sys::Reflect::get(&obj, &k)`; skips properties whose value is `is_undefined()` or `is_function()`, inserting remaining `(key, js_to_value(&v))` into `Value::Object(IndexMap)`.
- Implement `pub fn value_to_js(val: &Value) -> JsValue` converting `Value` to `JsValue` (`NULL`, `from_bool`, `from_f64`, `from_str`, `js_sys::Array`, `js_sys::Object`).

In `portable-xstate-wasm/src/lib.rs`:
- Replace `use crate::utils::{json_to_value, value_to_json};` with `use crate::utils::{js_to_value, value_to_js};`.
- Remove `core_value_to_js` and use `value_to_js`.
- Replace all `serde_wasm_bindgen::from_value(...)` + `json_to_value(...)` / `portable_xstate_json::map_value(...)` calls with `js_to_value(&...)`.
- Replace all `portable_xstate_json::map_value_to_state_value(v_json)` calls with `portable_xstate_core::value_to_state_value(js_to_value(&v))`.
- Replace all `let json_val = value_to_json(v); let json_str = serde_json::to_string(&json_val)...; js_sys::JSON::parse(&json_str)` sequences with `value_to_js(v)` (or `v.to_json_string()` where a JSON string is needed).
- Replace `portable_xstate_json::create_machine` with `portable_xstate_core::create_machine`, and pass owned `def` to `RuntimeMachine::from_definition(def)`.

- [ ] **Step 3: Remove `serde`, `serde_json`, `serde-wasm-bindgen`, and `portable-xstate-json` from `Cargo.toml` files and delete `portable-xstate-json/`**

- In `portable-xstate-wasm/Cargo.toml`: remove `serde`, `portable-xstate-json`, `serde_json`, `serde-wasm-bindgen`, and remove `features = ["serde"]` from `indexmap`.
- In root `Cargo.toml`: remove `"portable-xstate-json"` from `members`, and remove `serde`, `serde_json`, `portable-xstate-json`, and `serde-wasm-bindgen` from `[workspace.dependencies]`.
- Delete `portable-xstate-json/` using `trash portable-xstate-json`.

- [ ] **Step 4: Build and run the full test suite across native, FFI, WASM, Vitest, and Playwright**

Run: `npm test`
Expected: PASS (`test:native`, `test:wasm`, `test:e2e`, `test:ffi`, and `test:unit` all pass with zero `serde` dependencies in `Cargo.lock` for workspace crates).

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml Cargo.lock portable-xstate-wasm/ portable-xstate-json
git commit -m "refactor(wasm,workspace): remove serde ecosystem and consolidate portable-xstate-json into core"
```

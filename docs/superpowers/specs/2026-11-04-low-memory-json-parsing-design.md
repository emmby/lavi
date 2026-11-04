# Low-Memory, Zero-Serde JSON Parsing for `portable-xstate`

## 1. Intent and Why

`portable-xstate` is a `#![no_std]`-compatible Rust state machine runtime designed to execute XState v5 JSON state machine definitions across both WebAssembly (`portable-xstate-wasm`) and resource-constrained microcontrollers via C/C++ FFI (`portable-xstate-ffi` / `portable-xstate-platformio`).

When integrating `portable-xstate` into the [`perlerspiral`](file:///Users/mike/GitHub/perlerspiral/src/main.cpp#L18-L19) Arduino/PlatformIO project on an Adafruit ESP32-S3 Reverse TFT Feather, parsing even a small 15-line JSON state machine overflowed the default 8 KB FreeRTOS `loopTask` stack and required:

```cpp
// Increase loopTask stack size from default 8KB to 16KB to handle deep serde_json parsing recursion
SET_LOOP_TASK_STACK_SIZE(16 * 1024);
```

### Root Causes in Current Architecture

1. **Deep Call-Stack Frames ($O(\text{depth})$ CPU Stack Growth)**:
   - `portable-xstate-json` uses `serde` derived `Deserialize` on recursive structs (`MachineDefinitionDto`, `StateDefinitionDto`). `StateDefinitionDto` has 14 fields (multiple `IndexMap`s and `Vec`s), causing the generated `serde` visitor to allocate 14 `Option<Field>` local variables on the call stack at every state nesting depth before constructing and returning the struct by value.
   - `#[serde(untagged)]` enums (`ActionDto`, `TransitionDefinitionDto`, `TransitionGuardDto`, `MapVecOrSingle`, `AfterFormat`) buffer JSON token subtrees into `serde`'s private `Content` enum tree and recursively re-deserialize them.
   - `mapper::map_states` recursively traverses the DTO tree a second time, passing `StateDefinitionDto` and `StateDefinition` by value on the call stack.
   - `fsm_create` constructs `MachineDefinition`, `Result<RuntimeMachine, String>`, and `MachineContext` on the call stack before boxing.

2. **3x Peak Heap Duplication During `fsm_create`**:
   - Custom deserializers (`deserialize_map_vec_or_single`, `deserialize_after`) materialize intermediate `IndexMap<String, serde_json::Value>` trees before converting them to DTOs.
   - `from_json_str` holds the entire `MachineDefinitionDto` tree in heap memory while `mapper::map_machine` allocates the `MachineDefinition` tree.
   - `RuntimeMachine::new(&def)` clones `MachineDefinition` (`definition: definition.clone()`) while `def` is still alive in `fsm_create`.

3. **Heavyweight Serialization Dependencies (`serde`, `serde_derive`, `serde_json`, `serde-wasm-bindgen`, `cty`)**:
   - `serde`, `serde_derive`, `serde_json`, and `serde-wasm-bindgen` add significant stack depth, intermediate heap allocations, binary code size, and proc-macro compile-time overhead to embedded and WASM targets.
   - `cty` in `portable-xstate-ffi` is completely unused (`core::ffi` is already used directly).

---

## 2. Goals, Non-Goals, and Anti-Goals

### Goals
- **JSON as Source of Truth**: Keep plain XState JSON strings as the single source of truth so humans and AI agents can inspect and edit state machines directly in source files.
- **Bounded $O(1)$ Call-Stack Parsing**: Parse arbitrary-depth JSON definitions and construct `MachineDefinition` using explicit heap work-stacks (`Vec`) instead of CPU call-stack recursion, comfortably running within the default 8 KB ESP32 `loopTask` stack without `SET_LOOP_TASK_STACK_SIZE`.
- **Eliminate 3x Heap Peak**: Parse JSON into `portable_xstate_core::Value` iteratively and consume `Value` **by move** (`into_iter()` / `swap_remove()`) to build `MachineDefinition` without reallocating strings or cloning `context`, then move `MachineDefinition` directly into `RuntimeMachine`.
- **Drop `serde` Ecosystem & Unused Dependencies**:
  - Remove `serde`, `serde_derive`, `serde_json`, `serde-wasm-bindgen`, and `cty` from the workspace.
  - Keep `indexmap` (with its `"serde"` feature removed) so state/transition maps retain $O(1)$ lookup, $O(N)$ insertion scaling on large machines, and exact insertion-order semantics with minimal code churn.
- **Workspace Consolidation**: Fold `portable-xstate-json` into `portable-xstate-core`, reducing the workspace from 4 crates to 3 (`portable-xstate-core`, `portable-xstate-ffi`, `portable-xstate-wasm`).

### Non-Goals
- Chunked/streaming reader FFI (`fsm_create_reader`) from filesystem/network streams (can be added later if needed; in-memory `const char*` / `&str` is the scope of this project).
- Full integer string-interning (`u16` symbol tables) or replacing `IndexMap`.

### Anti-Goals
- **No Binary Formats or Build-Time Preprocessing**: Do not require CBOR, MessagePack, bincode, or extra codegen steps in user PlatformIO/Arduino projects.
- **No Breaking Public API Changes**: Existing C FFI (`portable_xstate.h`), C++ wrapper (`portable_xstate.hpp`), and WASM/JS (`Actor`, `StateNode`, `createMachine`, `createActor`, `setup`, `assign`) signatures and behavior must remain 100% compatible with existing tests.

---

## 3. Constraints

- **`#![no_std]` + `alloc` Compatibility**: `portable-xstate-core` and `portable-xstate-ffi` must compile with `#![no_std]` using only `core`, `alloc`, and `indexmap`.
- **Insertion-Order Preservation**: XState v5 semantics depend on preserving the exact definition order of `states`, `on` transitions, `after` timers, and object keys via `IndexMap`.
- **Full Test Suite Parity**: All existing tests (`npm run test:native`, `npm run test:wasm`, `npm run test:e2e`, `npm run test:ffi`, `npm run test:unit`) must pass without relaxing assertions.

---

## 4. Architecture & Design Decisions

### 4.1 Workspace Consolidation (`portable-xstate-json` $\rightarrow$ `portable-xstate-core`)

- Remove `portable-xstate-json` from the workspace `Cargo.toml` and delete the `portable-xstate-json/` crate directory.
- Remove `serde`, `serde_json`, `serde-wasm-bindgen`, and `cty` across all `Cargo.toml` manifests, and remove `features = ["serde"]` from `indexmap`.
- Move the JSON parser and XState builder functions into `portable-xstate-core` (in `portable-xstate-core/src/json.rs` and `portable-xstate-core/src/parser.rs`, re-exported at the root of `portable_xstate_core`):
  - `pub fn create_machine(json: &str) -> Result<MachineDefinition, JsonError>`
  - `pub fn from_json_str(json: &str) -> Result<MachineDefinition, JsonError>`
  - `pub fn setup(options: &str) -> SetupObject`
  - `pub fn assign(assignment_json: &str) -> Result<Action, JsonError>`
  - `pub fn value_to_state_value(v: Value) -> StateValue` (replacing `map_value_to_state_value`)
- Migrate all tests from `portable-xstate-json/src/lib.rs` and `portable-xstate-json/tests/*.rs` into `portable-xstate-core/tests/`. Tests in `deserialization.rs` that previously inspected `StateDefinitionDto` will inspect `MachineDefinition` / `StateDefinition` directly.

### 4.2 Step 1 of Parser: Iterative `Value::from_json` and `Value::to_json_string`

In `portable-xstate-core/src/json.rs`:

1. **Error Type**:
   ```rust
   #[derive(Debug, Clone, PartialEq, Eq)]
   pub struct JsonError {
       pub message: alloc::string::String,
       pub offset: usize,
   }
   ```
   Implements `core::fmt::Display`.

2. **Iterative Deserializer (`Value::from_json(input: &str) -> Result<Value, JsonError>`)**:
   - Uses a byte-slice cursor (`input.as_bytes()`, `pos: usize`) and an explicit heap stack:
     ```rust
     enum ContainerFrame {
         Array {
             items: Vec<Value>,
             expect_value_or_end: bool,
             expect_comma_or_end: bool,
         },
         Object {
             map: IndexMap<String, Value>,
             pending_key: Option<String>,
             state: ObjectParseState, // KeyOrEnd, Colon, Value, CommaOrEnd
         },
     }
     ```
   - **Zero CPU Call-Stack Recursion**: Nested `{` and `[` push a `ContainerFrame` onto `stack: Vec<ContainerFrame>`. Closing `}` and `]` pop the completed `Value::Object` or `Value::Array` and attach it to the parent frame (or return it if `stack` is empty).
   - **Token Support**:
     - Strings: Handles `\"`, `\\`, `\/`, `\b`, `\f`, `\n`, `\r`, `\t`, and `\uXXXX` hex escapes (including UTF-16 surrogate pairs `\uD800`..`\uDFFF` combined into a single UTF-8 `char`). When a string contains no backslashes, slices directly from `&input[start..end]` into `String::from(...)`.
     - Numbers: Validates JSON number grammar (`-? (0 | [1-9][0-9]*) (\.[0-9]+)? ([eE][+-]?[0-9]+)?`) and parses via `str::parse::<f64>()`, rejecting `NaN` or `Infinity`.
     - Literals: `true` $\rightarrow$ `Value::Bool(true)`, `false` $\rightarrow$ `Value::Bool(false)`, `null` $\rightarrow$ `Value::Null`.
     - Trailing non-whitespace bytes after the root value or unclosed frames return `Err(JsonError)`.

3. **Iterative Serializer (`Value::to_json_string(&self) -> String`)**:
   - Uses an explicit `Vec<SerializeFrame<'_>>` on the heap (zero call-stack recursion) to format `Value` into a JSON string.
   - Escapes control characters, quotes, and backslashes in strings.
   - Formats numbers with `n.fract() == 0.0 && n.abs() < (1i64 << 53) as f64` as integers (`n as i64`) so whole numbers serialize cleanly as `0` instead of `0.0`.

### 4.3 Step 2 of Parser: Iterative Move-Based `MachineDefinition` Builder

In `portable-xstate-core/src/parser.rs`:

`MachineDefinition::from_value(val: Value) -> Result<MachineDefinition, JsonError>` consumes the parsed `Value` **by move** with zero string reallocations and zero call-stack recursion:

1. **Root Extraction**:
   - Matches `Value::Object(mut root)` (returning `Err(JsonError)` for non-object roots).
   - Removes and maps top-level fields by move (`root.swap_remove(...)` in $O(1)$):
     - `id`: `String` (defaults to `"(machine)"`).
     - `type`: `Option<String>`.
     - `initial`: resolved via `resolve_initial_info(root.swap_remove("initial"))`.
     - `on`: `parse_on_map(root.swap_remove("on"))`.
     - `after`: `parse_after_map(root.swap_remove("after"))`.
     - `entry`, `exit`: `parse_action_list(root.swap_remove("entry"))`, `parse_action_list(root.swap_remove("exit"))`.
     - `invoke`: `parse_invoke_list(root.swap_remove("invoke"))`.
     - `context`: `root.swap_remove("context")` (moved in $O(1)$ directly as `Option<Value>`!).
     - `types`: `root.swap_remove("types")` (moved in $O(1)$ directly as `Option<Value>`!).

2. **Iterative `states` Tree Construction (Zero Call-Stack Recursion)**:
   - Instead of recursive `map_states()`, uses an explicit heap stack `Vec<StateWorkFrame>`:
     ```rust
     struct StateWorkFrame {
         key: String,
         current_path: String,
         state_def: StateDefinition,
         remaining_children: indexmap::map::IntoIter<String, Value>,
     }
     ```
   - Processes each state and its `states` children depth-first in exact definition order:
     - Populates `id_map` with explicit `id` (if present) or `"{machine_id}.{current_path}"` $\rightarrow$ `current_path`.
     - Infers `type_` (`"history"`, `"compound"`, `"parallel"`, `"final"`, `"atomic"`) and `StateType` (`Parallel`, `History { deep }`, `Final`, `Compound`, `Atomic`) identically to existing `mapper.rs`.
     - Converts `always` transitions and inserts them into `state_def.on` under key `""`.
     - Pushes child states onto the heap work-stack and attaches completed child `StateDefinition`s to their parent's `states: IndexMap<String, StateDefinition>`.

3. **Polymorphic XState Field Parsers (By Move)**:
   - **`parse_vec_or_single` pattern**: Any field that accepts a single item, an array of items, or `null` matches `Some(Value::Array(arr))` $\rightarrow$ `arr.into_iter()`, `Some(Value::Null) | None` $\rightarrow$ empty `Vec`, or `Some(single)` $\rightarrow$ single-element `Vec`.
   - **`on` transitions (`parse_on_map`)**: Consumes `Value::Object(map)` via `.into_iter()` (preserving insertion order). For each `(event_key, val)`, accepts `Value::String(target)`, `Value::Object(trans_obj)`, `Value::Array(arr)`, or `Value::Null`.
   - **`after` transitions (`parse_after_map`)**:
     - If `Value::Object(map)`: consumes via `.into_iter()`, parsing each key as `u64` delay and value as transition list.
     - If `Value::Array(arr)`: extracts `"delay"` (number or string parsed as `u64`) from each transition object and groups transitions into `IndexMap<u64, Vec<TransitionDefinition>>`.
   - **Actions (`parse_action`)**:
     - `Value::String(name)` $\rightarrow$ `Action::Named(name)`
     - `Value::Object(mut map)` $\rightarrow$ extracts `"type"`:
       - `"xstate.assign"` $\rightarrow$ `Action::Assign` using `map.swap_remove("assignment").or_else(|| map.swap_remove("params")).unwrap_or(Value::Null)`
       - `"xstate.raise"` $\rightarrow$ `Action::Raise` using `map.swap_remove("event").or_else(|| map.swap_remove("params")).unwrap_or(Value::Null)`
       - Any other string $\rightarrow$ `Action::Generic { action_type, params: map.swap_remove("params") }`
   - **Guards (`parse_guard`)**:
     - `Value::String(name)` $\rightarrow$ `TransitionGuard::Name(name)`
     - `Value::Object(mut map)` $\rightarrow$ `TransitionGuard::Object(GuardObject { guard_type, params: map.swap_remove("params") })`

### 4.4 Move Semantics & Stack Reduction in `RuntimeMachine` and `portable-xstate-ffi`

1. **Ownership in `RuntimeMachine`**:
   - Add `pub fn from_definition(definition: MachineDefinition) -> Result<Self, String>` to `RuntimeMachine`.
     - Resolves `initial_state` from `&definition`, then moves `definition.context` and `definition` directly into `Self` without cloning `MachineDefinition`.
   - Add `pub fn from_json(json: &str) -> Result<Self, String>` which parses `MachineDefinition` and calls `Self::from_definition(def)`.
   - Keep `pub fn new(definition: &MachineDefinition) -> Result<Self, String>` implemented as `Self::from_definition(definition.clone())` for backwards compatibility with existing callers.
2. **FFI `fsm_create` Optimization**:
   - In `portable-xstate-ffi/src/lib.rs`, `fsm_create` calls `create_machine(json_str)` and passes the owned `MachineDefinition` into `RuntimeMachine::from_definition(def)`, eliminating the duplicate `MachineDefinition` clone.

### 4.5 Removing `serde` and `serde-wasm-bindgen` from `portable-xstate-wasm`

In `portable-xstate-wasm/src/utils.rs` and `portable-xstate-wasm/src/lib.rs`:
- Implement `pub fn js_to_value(val: &JsValue) -> Value` using an explicit heap stack (or bounded traversal) over `js_sys`:
  - `val.is_null() || val.is_undefined()` $\rightarrow$ `Value::Null`
  - `val.as_bool()` $\rightarrow$ `Value::Bool(b)`
  - `val.as_f64()` $\rightarrow$ `Value::Number(n)`
  - `val.as_string()` $\rightarrow$ `Value::String(s)`
  - `js_sys::Array::is_array(val)` $\rightarrow$ `Value::Array(...)`
  - `val.is_object()` $\rightarrow$ iterates `js_sys::Object::keys` and `js_sys::Reflect::get` into `Value::Object(IndexMap)`
- Implement `pub fn value_to_js(val: &Value) -> JsValue` (consolidating `core_value_to_js`).
- Replace all calls to `serde_wasm_bindgen::from_value`, `json_to_value`, `value_to_json`, and `serde_json::to_string` in `portable-xstate-wasm` with `js_to_value`, `value_to_js`, and `Value::to_json_string`.

---

## 5. What Is Left to the Builder

- Exact internal helper organization between `portable-xstate-core/src/json.rs` and `portable-xstate-core/src/parser.rs`.
- Specific error messages inside `JsonError`, provided malformed JSON returns `Err` and `fsm_create` continues to return `null` on invalid JSON.
- Adding a regression test in `portable-xstate-core` that parses a deeply nested state machine inside a small-stack thread (e.g. `std::thread::Builder::new().stack_size(...)` in native tests) to verify bounded stack usage.

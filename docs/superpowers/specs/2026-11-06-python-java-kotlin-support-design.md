# Python, Java, and Kotlin Language Support Design

**Date**: 2026-11-06  
**Status**: Draft (Pending User Approval)

---

## 1. Intent & Why

**Lavi** enables developers to author Harel statecharts once in XState v5 / Stately Studio and execute the exact same declarative JSON state machine definition across environments—from bare-metal C/C++ microcontrollers (`lavi-c-cpp`) and native Rust (`lavi`) to browsers/Node.js (`lavi-wasm`).

Adding **Python**, **Java**, and **Kotlin** extends this "Design in XState, Run Everywhere" model to:
- **Backend & Edge Services (Python, Java, Kotlin Server)**: Run the identical statechart logic on cloud backends and IoT gateways that runs on embedded devices.
- **Android Mobile Apps (Java & Kotlin)**: Share device/protocol statecharts directly between Android companion apps and microcontroller firmware.
- **Hardware-in-the-Loop & Simulation Harnesses (Python)**: Drive deterministic statechart tests and simulations using `SimulatedClock` from Python test suites (`pytest`).

The core priorities for these bindings are:
1. **Idiomatic, ergonomic target-language APIs** (feeling native to Python, Java, and Kotlin developers rather than raw C pointers or awkward IDL codegen).
2. **Low ongoing maintenance** by keeping a uniform architecture across crates, aligning 1:1 with the ~20 operations in `lavi.hpp`, and compiling all native binaries from a single build host.

---

## 2. Goals, Non-Goals, and Anti-Goals

### Goals
- **JSON + Named Callback Execution Model**: Load XState v5 / Stately JSON configuration strings and register named `action`, `guard`, and `clock` callbacks using native functions, lambdas, or decorators—matching the mental model of `lavi` (Rust) and `lavi.hpp` (C++).
- **Full Runtime Feature Parity with `lavi-c-cpp` (`lavi.hpp`)**:
  - Lifecycle: `start()`, `stop()`, deterministic cleanup (`with` context manager in Python; `AutoCloseable` / `.use {}` in Java/Kotlin).
  - State & Transition Inspection: `matches(path)`, `can(event)`, `has_tag(tag)` / `hasTag(tag)`, `is_done()` / `isDone()`.
  - Event Dispatch: `send(event_name)`.
  - Timers & Clocks: `tick()`, `get_next_deadline()` / `getNextDeadline()`, custom clock callbacks, and `SimulatedClock` (`now`, `set`, `increment`).
  - Persistence: `get_snapshot()` / `getSnapshot()` and `restore_snapshot(json)` / `restoreSnapshot(json)`.
  - Context & Event Inspection (`Value`): Read-only access to string, number, boolean, null, and nested object properties inside `action` and `guard` callbacks.
- **Zero-Rust End-User Experience**:
  - Python users install a pre-built wheel (`pip install lavi`) with full `.pyi` type stubs for IDE autocomplete.
  - Java/Kotlin Server users depend on a standard `.jar` that bundles and auto-loads the native library from resources.
  - Android users depend on a standard `.aar` with `jniLibs/<abi>/liblavi_jvm.so`.
- **Single-Host Build & Test Workflow**:
  - Everyday local testing (`npm test`) compiles only for the developer's host machine in seconds.
  - Release packaging cross-compiles all desktop and Android targets from a single build host using `cargo-zigbuild` / `maturin --zig` and `cargo-ndk`.

### Non-Goals (Current Scope)
- **Programmatic Machine Definition Builders in Python/Java/Kotlin**: Users load declarative XState v5 JSON configs and attach named callbacks rather than constructing state trees via a full programmatic DSL (`createMachine({...})` with inline callbacks).
- **Additional Languages in This Track**: Future languages with built-in C-ABI interop (C#, Swift, Go, Dart/Flutter) are out of scope for this track and will bind directly to `lavi-c-cpp` (`lavi.h`) when added later.

### Anti-Goals (What Would Make This a Failure Even If It Works)
- **Exposing Raw C Pointers or Un-Idiomatic Codegen**: Forcing Python users to subclass trait interfaces instead of passing `lambda ctx, evt: ...`, or forcing Java/Kotlin users to manage raw memory addresses without `AutoCloseable` / `@FunctionalInterface`.
- **Requiring Multi-VM Build Farms for Routine Development**: Local development (`npm test`) must never require Docker containers, multi-OS VMs, or cross-compilers just to run unit tests on the host machine.
- **Violating Core `lavi` Invariants**: `lavi` itself remains strictly `#![no_std]` + `alloc` with zero `std` dependencies and $O(1)$ bounded CPU call stack. (`lavi-py` and `lavi-jvm` are separate workspace crates that depend on `lavi`, just as `lavi-wasm` does.)

---

## 3. Architecture & Decisions

### 3.1 Workspace Structure
Two new crates join the Cargo workspace alongside `lavi`, `lavi-wasm`, and `lavi-c-cpp`:

```text
Cargo.toml
├── lavi/          # Core #![no_std] + alloc engine (unchanged)
├── lavi-wasm/     # WebAssembly / JS adapter
├── lavi-c-cpp/    # C / C++ staticlib & cdylib (lavi.h, lavi.hpp)
├── lavi-py/       # NEW: Python native extension via PyO3 + maturin
└── lavi-jvm/      # NEW: Java & Kotlin native JNI bridge via jni-rs + Gradle
```

### 3.2 Python Binding (`lavi-py`)
- **Technology**: Rust crate using `pyo3` with `features = ["extension-module", "abi3-py38"]`, built via `maturin`.
  - Using `abi3-py38` (Python Stable ABI) means a single compiled wheel per OS/CPU works across Python 3.8, 3.9, 3.10, 3.11, 3.12, 3.13+.
- **Idiomatic Python API (`lavi` module)**:
  ```python
  from lavi import Machine, SimulatedClock, Value

  clock = SimulatedClock(initial_ms=0)

  with Machine(json_str, clock=clock) as machine:
      # Standard callable registration
      machine.register_action("doTimerAction", lambda ctx, evt: print(ctx["cycles"]))
      machine.register_guard("isConnected", lambda ctx, evt: bool(ctx["connected"]))

      # Decorator registration (builder's call for ergonomic multi-line callbacks)
      @machine.action("pollWifi")
      def on_poll(ctx: Value, evt: Value) -> None:
          pass

      machine.start()
      assert machine.matches("ConnectToNetwork.Polling")
      clock.increment(500)
      snapshot: str = machine.get_snapshot()
  ```
- **`Value` Ergonomics in Python**:
  - Wraps a read-only snapshot/view of `lavi::Value` for the duration of the callback (or converts on access).
  - Supports Pythonic indexing (`ctx["key"]`), `.get("key", default=None)`, `in` (`"key" in ctx`), `.as_str()`, `.as_float()`, `.as_bool()`, `.is_null()`, and `.to_python()` (converting primitives/objects to native Python `dict` / `str` / `float` / `bool` / `None`).
- **Lifecycle & Safety**:
  - Follows the same deferred-initialization pattern as `lavi.hpp`: `Machine(json_str, clock=...)` stores the JSON config and registered callbacks; `.start()` (or `.restore_snapshot(snap)`) initializes the underlying `RuntimeMachine` and binds handlers.
  - Supports `with Machine(...) as m:` (`__enter__` returns `self`, `__exit__` stops and releases resources).

### 3.3 Java & Kotlin Binding (`lavi-jvm`)
- **Technology**: A single Rust `cdylib` crate using `jni` (`jni-rs`), paired with a Gradle project inside `lavi-jvm/` that compiles both Java 8+ classes and Kotlin extension helpers against the same native JNI library.
- **Shared Native Core, Dual Idiomatic Surface**:
  1. **Java API (`io.lavi.*`)**:
     - `Machine` and `SimulatedClock` implement `AutoCloseable` for `try-with-resources`.
     - Fluent registration methods returning `this` so builders chain cleanly.
     - `@FunctionalInterface` callback types (`ActionCallback`, `GuardCallback`, `ClockCallback`) so Java 8+ lambdas and method references work natively:
       ```java
       try (SimulatedClock clock = new SimulatedClock(0);
            Machine machine = new Machine(jsonStr, clock)
                .registerAction("doTimerAction", (ctx, evt) -> {
                    double cycles = ctx.getProperty("cycles").asNumber().orElse(0.0);
                })
                .registerGuard("isConnected", (ctx, evt) ->
                    ctx.getProperty("connected").asBoolean().orElse(false))) {

           machine.start();
           machine.send("TIMER");
           boolean active = machine.matches("yellow");
       }
       ```
  2. **Kotlin API (`io.lavi.*` extensions)**:
     - Thin Kotlin extension functions (`MachineExtensions.kt`) in the same module provide idiomatic trailing-lambda registration (`onAction`, `onGuard`, `onClock`) and `operator fun get(key: String): Value` indexing on `Value`:
       ```kotlin
       SimulatedClock(0).use { clock ->
           Machine(jsonStr, clock).use { machine ->
               machine.onAction("doTimerAction") { ctx, evt ->
                   val cycles = ctx["cycles"].asDoubleOrNull() ?: 0.0
               }
               machine.onGuard("isConnected") { ctx, evt ->
                   ctx["connected"].asBooleanOrNull() ?: false
               }
               machine.start()
               machine.send("TIMER")
           }
       }
       ```
- **Native Library Loading (Server `.jar` + Android `.aar`)**:
  - `NativeLoader` in `io.lavi` first checks if running on Android (or if `-Dlavi.library.path` is set) and calls `System.loadLibrary("lavi_jvm")`.
  - On Desktop/Server JVM, if `System.loadLibrary` is not pre-configured, `NativeLoader` detects OS (`linux`, `macos`, `windows`) and CPU (`x86_64`, `aarch64`), extracts `/natives/<os>-<arch>/liblavi_jvm.<ext>` from the `.jar` resources to a temporary file (`deleteOnExit`), and calls `System.load(...)`.

### 3.4 Single-Host Build, Test, and Cross-Compilation
- **Local Development (`npm test`)**:
  - Compiles **only** for the local host machine (`cargo build -p lavi-py -p lavi-jvm`).
  - `npm run test:py`: Builds/links `lavi-py` for the local Python environment and runs `pytest lavi-py/tests`.
  - `npm run test:jvm`: Runs `./gradlew test` in `lavi-jvm`, executing both Java JUnit tests (`JavaMachineTest.java`) and Kotlin JUnit tests (`KotlinMachineTest.kt`) against the host `target/debug/liblavi_jvm.<ext>`.
- **Single-Host Release Cross-Compilation (`scripts/build-release-natives.mjs` - Builder's Call)**:
  - Uses `cargo-zigbuild` / `maturin build --zig` for desktop targets (`x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`, `aarch64-apple-darwin`, `x86_64-apple-darwin`, `x86_64-pc-windows-gnu`) and `cargo-ndk` for Android ABIs (`arm64-v8a`, `armeabi-v7a`, `x86_64`) from a single build host without requiring multi-OS VMs.

---

## 4. Builder's Calls & What Is Left to the Builder

The following minor implementation details are settled as builder's calls or left to the implementation plan:

1. **Callback Exception Handling Across FFI (Builder's Call)**:
   - Because Rust panics cannot safely unwind across `extern "C"` / JNI boundaries, if a Python or JVM callback raises an exception:
     - In `lavi-py`: Capture the Python exception via `PyErr::Restore` / pending exception state, abort remaining callbacks in the current step cleanly (treating a faulted guard as `false`), and re-raise the Python exception when control returns from the Rust method (`start`, `send`, `tick`, `clock.set`, `clock.increment`) back to Python.
     - In `lavi-jvm`: Check `env.exception_check()` immediately after `call_method`; if a Java/Kotlin exception is pending on the `JNIEnv`, short-circuit further JNI calls in that transition step and let the JVM surface the exception to the caller as soon as the native call returns.
2. **Thread Safety & Ownership in `lavi-py` and `lavi-jvm` (Builder's Call)**:
   - In `lavi-py`, use `#[pyclass(unsendable)]` (or `Mutex` wrapper) so Python's runtime enforces safe single-thread access while allowing re-entrant read calls (`matches`, `can`, `has_tag`) or clock queries if needed.
   - In `lavi-jvm`, synchronize native entrypoints on the Java `Machine` / `SimulatedClock` instance and store the `JNIEnv*` on a scoped thread-local during `start()`, `send()`, `tick()`, and `SimulatedClock.set()` / `increment()` so Rust callbacks can invoke registered Java `ActionCallback` / `GuardCallback` / `ClockCallback` objects on the active thread.
3. **`SimulatedClock` Multi-Machine Binding (Builder's Call)**:
   - Match `lavi-c-cpp`'s `SimulatedClock` behavior: advancing a `SimulatedClock` via `set(ms)` or `increment(dt_ms)` automatically steps all `Machine` instances bound to that clock in chronological deadline order.
4. **Python Virtualenv / Build Tooling in `npm run test:py` (Builder's Call)**:
   - Allow `npm run test:py` to either use `maturin develop` in a repo-local `.venv` or compile `cdylib` via `cargo build -p lavi-py` and copy/symlink `target/debug/liblavi_py.dylib`/`.so` to `lavi.abi3.so` on `PYTHONPATH` so tests run cleanly even on machines without `maturin` pre-installed globally.

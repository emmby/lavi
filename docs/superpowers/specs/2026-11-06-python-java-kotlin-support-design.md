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
2. **Low ongoing maintenance** by keeping a uniform architecture across crates, aligning 1:1 with the core operations in `lavi.hpp`, and compiling all native binaries from a single build host.

---

## 2. Goals, Non-Goals, and Anti-Goals

### Goals
- **JSON + Named Callback Execution Model**: Load XState v5 / Stately JSON configuration strings and register named `action`, `guard`, and `clock` callbacks using native functions, lambdas, or decorators—matching the mental model of `lavi` (Rust) and `lavi.hpp` (C++).
- **Full Runtime Feature Parity with `lavi` & `lavi-c-cpp` (`lavi.hpp`)**:
  - Lifecycle: Fail-fast JSON validation in `Machine(json_str)`, `start()`, `stop()`, and deterministic cleanup (`with` context manager in Python; `AutoCloseable` / `.use {}` in Java/Kotlin) plus GC fallback cleanup (`__traverse__`/`__clear__` in Python; `Cleaner` in JVM).
  - State & Transition Inspection: `matches(path)`, `can(event, payload=...)`, `has_tag(tag)` / `hasTag(tag)`, `is_done()` / `isDone()`.
  - Event Dispatch: `send(event_name, payload=...)` (supporting both bare event names and optional JSON/dict/Map payloads so callbacks can inspect `evt` properties).
  - Timers & Clocks: `tick()`, `get_next_deadline()` / `getNextDeadline()`, custom clock callbacks, and `SimulatedClock` (`now`, `set`, `increment`).
  - Persistence: `get_snapshot()` / `getSnapshot()` and `restore_snapshot(json)` / `restoreSnapshot(json)`.
  - Context & Event Inspection (`Value`): Memory-safe read-only access to string, number (`int`/`long` and `float`/`double`), boolean, null, array, and nested object properties inside and outside `action` and `guard` callbacks.
- **Zero-Rust End-User Experience**:
  - Python users install a pre-built wheel (`pip install lavi`, including both `manylinux` and Alpine `musllinux` wheels) with full `.pyi` type stubs for IDE autocomplete.
  - Java/Kotlin Server users depend on a standard `.jar` that bundles and auto-loads the native library from resources (across macOS, Windows, Linux `glibc`, and Alpine Linux `musl`).
  - Android users depend on a standard `.aar` with 16 KB page-aligned `jniLibs/<abi>/liblavi_jvm.so` compatible with Android 15+ / Google Play requirements.
- **Single-Host Build & Test Workflow**:
  - Everyday local testing (`npm test`) compiles only for the developer's host machine in seconds and does **not** require the Android SDK (`ANDROID_HOME`) or cross-compilers.
  - Release packaging cross-compiles all desktop (`gnu` and `musl`) and Android targets from a single macOS build host using `cargo-zigbuild` / `maturin --zig` and `cargo-ndk`.

### Non-Goals (Current Scope)
- **Programmatic Machine Definition Builders in Python/Java/Kotlin**: Users load declarative XState v5 JSON configs and attach named callbacks rather than constructing state trees via a full programmatic DSL (`createMachine({...})` with inline callbacks).
- **Additional Languages in This Track**: Future languages with built-in C-ABI interop (C#, Swift, Go, Dart/Flutter) are out of scope for this track and will bind directly to `lavi-c-cpp` (`lavi.h`) when added later.

### Anti-Goals (What Would Make This a Failure Even If It Works)
- **Leaky Native Installation or Deployment Ergonomics**: Consumers of the Python, Java, Kotlin, or Android packages must never need to know that Lavi uses JNI or Rust under the hood—no installing `cargo`, no manual `.so`/`.dylib`/`.dll` unpacking, no setting `-Djava.library.path` or `LD_LIBRARY_PATH`, and no non-standard build steps. Installing and deploying Lavi must work identically to any pure-Python wheel, standard Maven/Gradle `.jar`, or Android `.aar`.
- **Exposing Raw C Pointers or Un-Idiomatic Codegen**: Forcing Python users to subclass trait interfaces instead of passing `lambda ctx, evt: ...`, or forcing Java/Kotlin users to manage raw memory addresses without `AutoCloseable` / `@FunctionalInterface`.
- **Requiring Multi-VM Build Farms or Heavy SDKs for Routine Development**: Local development (`npm test`) must never require Docker containers, multi-OS VMs, Android SDK installations, or cross-compilers just to run unit tests on the host machine.
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
- **Technology**: Rust crate using `pyo3 >= 0.22` with `features = ["extension-module", "abi3-py38"]` and `pyo3-build-config` in `build.rs` (`add_extension_module_link_args()`).
  - Using `abi3-py38` (Python Stable ABI) means a single compiled binary per OS/CPU works across Python 3.8–3.14+, and `pyo3 >= 0.22` generates self-contained `python3.dll` import libraries for Windows cross-compilation without needing a target Windows Python install.
- **Idiomatic Python API (`lavi` module)**:
  ```python
  from lavi import Machine, SimulatedClock, Value

  clock = SimulatedClock(initial_ms=0)

  with Machine(json_str, clock=clock) as machine:
      # Standard callable registration
      machine.register_action("doTimerAction", lambda ctx, evt: print(ctx["cycles"].as_int()))
      machine.register_guard("isConnected", lambda ctx, evt: bool(ctx["connected"]))

      # Decorator registration for multi-line callbacks
      @machine.action("pollWifi")
      def on_poll(ctx: Value, evt: Value) -> None:
          pass

      machine.start()
      assert machine.matches("ConnectToNetwork.Polling")
      machine.send("SENSOR_DATA", payload={"rssi": -42})
      clock.increment(500)
      snapshot: str = machine.get_snapshot()
  ```
- **`Value` Ergonomics & Memory Safety in Python**:
  - Backed by an owned `Arc<lavi::Value>` (using `lavi`'s iterative, stack-safe `Clone`) so retaining `ctx` or `evt` in a variable after a callback returns is 100% memory-safe and never causes a Use-After-Free segfault.
  - Supports:
    - Object key indexing (`ctx["key"]`), array index access (`ctx["items"][0]`), `.get("key", default=None)`, `in` (`"key" in ctx`), and `len(val)` (`__len__` for objects, arrays, and strings).
    - Pythonic truthiness (`__bool__`): returns `False` for `Null`, `Bool(False)`, `Number(0.0)` / `NaN`, and empty string/array/object; `True` otherwise.
    - Value equality (`__eq__`) against both `Value` and native Python primitives (`str`, `int`, `float`, `bool`, `None`, `list`, `dict`).
    - Explicit typed getters: `.as_str()`, `.as_float()`, `.as_int()` (when whole number), `.as_bool()`, `.is_null()`, `.is_array()`, `.is_object()`, and `.to_python()` (converting recursively via an explicit heap work-stack to native Python `dict` / `list` / `int` / `float` / `str` / `bool` / `None`).
- **Lifecycle, Fail-Fast Validation & Cyclic GC Safety**:
  - `Machine(json_str, clock=...)` validates and parses the JSON into a `MachineDefinition` immediately in the constructor (raising `ValueError` on malformed JSON) and initializes the `RuntimeMachine`.
  - `restore_snapshot(snapshot_json)` raises `ValueError` on malformed or incompatible snapshot JSON.
  - Implements PyO3's cyclic garbage collector hooks (`__traverse__` and `__clear__`) over all stored `Py<PyAny>` callbacks and clock references so lambdas that capture `machine` (`lambda ctx, evt: machine.send("NEXT")`) never leak memory even if the user omits `with`.

### 3.3 Java & Kotlin Binding (`lavi-jvm`)
- **Technology**: A single Rust `cdylib` crate using `jni = "0.21"` (default features only; `invocation` feature disabled so it never links against `libjvm` at compile time), paired with a standard `java-library` + `org.jetbrains.kotlin.jvm` Gradle project inside `lavi-jvm/`.
- **Shared Native Core, Dual Idiomatic Surface**:
  1. **Java API (`io.lavi.*`)**:
     - `Machine` and `SimulatedClock` implement `AutoCloseable` for `try-with-resources`, backed by `java.lang.ref.Cleaner` (or `finalize` fallback) to free the native Rust peer if `.close()` is forgotten.
     - Fluent registration methods returning `this`.
     - `@FunctionalInterface` callback types (`ActionCallback`, `GuardCallback`, `ClockCallback`):
       ```java
       try (SimulatedClock clock = new SimulatedClock(0);
            Machine machine = new Machine(jsonStr, clock)
                .registerAction("doTimerAction", (ctx, evt) -> {
                    long cycles = ctx.getProperty("cycles").asLong().orElse(0L);
                })
                .registerGuard("isConnected", (ctx, evt) ->
                    ctx.getProperty("connected").asBoolean().orElse(false))) {

           machine.start();
           machine.send("TIMER", "{\"attempt\": 1}");
           boolean active = machine.matches("yellow");
       }
       ```
  2. **Kotlin API (`io.lavi.*` extensions)**:
     - Thin Kotlin extension functions (`MachineExtensions.kt`) in the same module provide trailing-lambda registration (`onAction`, `onGuard`, `onClock`), `Map<String, Any?>` event payload overloads, and `operator fun get(key: String): Value` / `operator fun get(index: Int): Value` indexing on `Value`:
       ```kotlin
       SimulatedClock(0).use { clock ->
           Machine(jsonStr, clock).use { machine ->
               machine.onAction("doTimerAction") { ctx, evt ->
                   val cycles = ctx["cycles"].asLongOrNull() ?: 0L
               }
               machine.onGuard("isConnected") { ctx, evt ->
                   ctx["connected"].asBooleanOrNull() ?: false
               }
               machine.start()
               machine.send("TIMER", mapOf("attempt" to 1))
           }
       }
       ```
- **Zero-`GlobalRef` Callback Architecture (Preventing JVM GC Leaks)**:
  - Callback maps (`Map<String, ActionCallback>`, `Map<String, GuardCallback>`, `ClockCallback`) are stored as normal Java fields on the Java `Machine` instance—**not** as JNI `GlobalRef` roots in Rust.
  - When a callback is registered for `"foo"`, Java notifies Rust that `"foo"` has a host handler. During `start()`, `send()`, or `tick()`, Rust calls back into a private dispatch method (`private void dispatchAction(String name, long ctxPtr, long evtPtr)`) on the active `Machine` reference.
  - Because Rust holds zero JNI `GlobalRef` roots to lambdas, lambdas that capture `machine` (`(ctx, evt) -> machine.send("NEXT")`) are collected normally by the JVM GC whenever `machine` becomes unreachable.
- **Robust Native Library Loading (Server `.jar` + Alpine `musl` + `noexec /tmp` + Android `.aar`)**:
  - `NativeLoader` in `io.lavi` resolves the native library in this order:
    1. If running on Android (`System.getProperty("java.vendor")` / `"java.vm.name"` check) or if `-Dlavi.library.bundled=false`, calls `System.loadLibrary("lavi_jvm")` directly.
    2. On Desktop/Server JVM, detects OS (`linux`, `macos`, `windows`), libc variant on Linux (`musl` vs `gnu` via `/lib/ld-musl-*` or `/proc/self/maps`), and CPU (`x86_64`, `aarch64`).
    3. Extracts `/natives/<os>-<arch>/liblavi_jvm.<ext>` into a deterministic content-hashed path (`<baseDir>/lavi-jni-<version>-<hash>/liblavi_jvm.<ext>`) using atomic temp-file rename (`Files.move`) if not already present. This avoids Windows locked-DLL temp file leaks across restarts and race conditions across concurrent JVM processes.
    4. Uses a fallback directory chain if `/tmp` is mounted `noexec` (`UnsatisfiedLinkError` / `EPERM`): `System.getProperty("lavi.tmpdir")` $\rightarrow$ `System.getProperty("java.io.tmpdir")` $\rightarrow$ user cache directory (`~/.cache/lavi` or `%LOCALAPPDATA%/lavi`).

### 3.4 Single-Host Build, Test, and Cross-Compilation
- **Local Development (`npm test`)**:
  - Compiles **only** for the local host machine (`cargo build -p lavi-py -p lavi-jvm`).
  - `npm run test:py`: Copies the compiled host library (`liblavi_py.dylib`/`.so` $\rightarrow$ `lavi.abi3.so` on Unix; `lavi_py.dll` $\rightarrow$ `lavi.pyd` on Windows) into `lavi-py/python/lavi/` and runs `pytest lavi-py/tests`—working out of the box without requiring global `maturin`.
  - `npm run test:jvm`: Runs `./gradlew test` in `lavi-jvm` (standard `java-library` + Kotlin JVM plugin; **zero Android SDK dependency**), executing both Java JUnit tests (`JavaMachineTest.java`) and Kotlin JUnit tests (`KotlinMachineTest.kt`) against the host `target/debug/liblavi_jvm.<ext>`.
- **Single-Host Release Cross-Compilation (`scripts/build-release-natives.mjs`)**:
  - Uses **macOS** as the canonical single-host release OS (providing the native Apple SDK for Darwin targets alongside `zig` and Android NDK).
  - Desktop targets via `cargo-zigbuild` / `maturin build --zig`:
    - `aarch64-apple-darwin`, `x86_64-apple-darwin`
    - `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`
    - `x86_64-unknown-linux-musl`, `aarch64-unknown-linux-musl` (for Alpine Linux Docker containers)
    - `x86_64-pc-windows-gnu`
  - Android targets via `cargo-ndk` (`arm64-v8a`, `armeabi-v7a`, `x86_64`):
    - Passes `-C link-arg=-Wl,-z,max-page-size=16384` on `aarch64-linux-android` and `x86_64-linux-android` and verifies 16 KB (`0x4000`) ELF `LOAD` segment alignment for Android 15+ / Google Play compliance.
    - Packages the `.aar` archive directly from the compiled `classes.jar`, `AndroidManifest.xml`, and `jni/<abi>/liblavi_jvm.so`.

---

## 4. Builder's Calls & Safety Invariants

1. **Re-Entrant Callback Execution Without Borrow Panics or Deadlocks**:
   - Action callbacks in `lavi` frequently call `machine.send(...)`, `machine.matches(...)`, `machine.can(...)`, or `machine.stop()` synchronously while inside `start()`, `send()`, `tick()`, or `SimulatedClock.increment()`.
   - Neither `lavi-py` (`#[pymethods]`) nor `lavi-jvm` may hold an exclusive PyO3 `&mut self` borrow or a non-reentrant `Mutex` lock across foreign callback invocations.
   - Instead, all methods take `&self` with interior state management:
     - While a transition step is actively executing callbacks (`in_callback == true`), re-entrant mutating calls (`send`, `stop`) enqueue onto a wrapper-level `PendingCommand` queue (`RefCell<VecDeque<PendingCommand>>`), which is drained immediately after the current `RuntimeMachine` step yields.
     - Read-only inspection calls (`matches`, `can`, `has_tag`, `is_done`, `get_next_deadline`) read from an interior state view updated around each callback invocation, preventing both PyO3 `PyBorrowMutError` and Rust aliasing UB.
2. **Callback Exception Handling & `SimulatedClock` Short-Circuiting**:
   - If a Python or Java/Kotlin callback throws an exception during `start()`, `send()`, `tick()`, or `SimulatedClock.set()`/`increment()`:
     - The FFI wrapper records `aborted = true` (retaining the pending Python/JVM exception).
     - Any remaining foreign guard or action callbacks in that step are skipped (faulted guards return `false`), while `lavi`'s built-in microstep finishes cleanly so `RuntimeMachine::is_processing` resets to `false`.
     - If inside `SimulatedClock.set()` or `increment()`, the clock loop breaks immediately at the current virtual timestamp rather than advancing to subsequent timer deadlines, and re-throws the exception to the caller.
3. **`SimulatedClock` Shared Ownership Safety**:
   - `SimulatedClock` and `Machine` share clock state via `Arc<Mutex<SimulatedClockInner>>` in Rust (and `Machine` retains a reference to the `SimulatedClock` wrapper), guaranteeing that closing or dropping `SimulatedClock` and `Machine` in any order can never produce a dangling pointer.

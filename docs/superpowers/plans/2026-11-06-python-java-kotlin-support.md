# Python, Java, and Kotlin Language Support Implementation Plan [flowos: 59f4159b-0f61-49df-b065-9ebd79e340fa]

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add idiomatic, memory-safe Python (`lavi-py`) and Java/Kotlin (`lavi-jvm`) bindings to the Cargo workspace with full runtime parity with `lavi.hpp` and single-host release packaging.

**Architecture:** Two new Rust `cdylib` workspace crates wrap the `#![no_std]` `lavi` core: `lavi-py` (`pyo3` with `abi3-py38`) for Python, and `lavi-jvm` (`jni-rs` + Gradle `java-library` and Kotlin extensions) for Server JVM and Android. Both bindings use owned `Arc<lavi::Value>` snapshots for callback safety, `&self` interior mutability with a `PendingCommand` queue for re-entrant callbacks, and cycle-safe GC integration (`__traverse__`/`__clear__` in Python; Java-side callback maps + `Cleaner` in JVM).

**Tech Stack:** Rust (`lavi`, `pyo3 >= 0.22`, `jni = "0.21"`), Python 3.8+ (`pytest`), Java 8+ & Kotlin (`Gradle`, `JUnit 5`), Node.js (`scripts/build-release-natives.mjs`).

**Spec:** [`docs/superpowers/specs/2026-11-06-python-java-kotlin-support-design.md`](../specs/2026-11-06-python-java-kotlin-support-design.md)

## Global Constraints

- Core `lavi` crate remains strictly `#![no_std]` + `alloc` with `indexmap` as its sole dependency and $O(1)$ bounded CPU call stack (iterative heap `Vec` work-stacks for all recursive Value/tree traversals).
- `lavi-py` uses `pyo3 = { version = "0.22", features = ["extension-module", "abi3-py38"] }` and `pyo3-build-config = "0.22"` in `build.rs`.
- `lavi-jvm` uses `jni = { version = "0.21", default-features = true }` (never enable `invocation` feature).
- Local `npm test` compiles only for the developer's host machine and must not require `ANDROID_HOME`, Docker, or global `maturin`.
- Every `Value` conversion or cloning helper in `lavi-py` and `lavi-jvm` must use an explicit heap `Vec` work-stack rather than CPU call-stack recursion.

## Review Focus

1. **Unstarted machine calls**: Calling `machine.send(...)`, `machine.matches(...)`, `machine.can(...)`, or `machine.has_tag(...)` before `machine.start()` drops the event and returns `False` without crashing or buffering pre-start events.
2. **Malformed JSON inputs**: Passing invalid JSON to `Machine(json_str)`, `restore_snapshot(json_str)`, or `send(event, payload_json)` raises `ValueError` in Python and `IllegalArgumentException` in Java/Kotlin immediately.
3. **Missing keys and out-of-bounds indices on `Value`**: Accessing `ctx["missing"]["nested"]` or `ctx["items"][99]` returns a `Null`-typed `Value` (`is_null() == True`, `bool(v) == False`) without throwing `KeyError` or `IndexOutOfBoundsException` so chained property lookups are safe.
4. **Post-stop and post-close operations**: Calling `send()` or `tick()` after `stop()` is a safe no-op; calling methods on a `Machine` or `SimulatedClock` after `close()` (or exiting `with` / `try-with-resources`) raises `RuntimeError` in Python or `IllegalStateException` in Java/Kotlin instead of dereferencing freed memory.
5. **Synchronous re-entrant callbacks**: Calling `machine.send("NEXT")`, `machine.matches(...)`, `machine.can(...)`, or `machine.stop()` from inside an `action` callback queues mutations and reads live state without triggering `PyBorrowMutError` or `Mutex` deadlocks.

---

## Phase 1: Python Bindings (`lavi-py`) [flowos: 187a6e7f-0210-4447-9568-fd39c4ea3125]

### Task 1: `lavi-py` Core Crate, `Value`, `SimulatedClock`, and `Machine` Lifecycle (~0.75d) [flowos: 5d989007-9e59-434d-a0dd-645260b395d2]

**Files:**
- Modify: `Cargo.toml`
- Modify: `package.json`
- Create: `lavi-py/Cargo.toml`
- Create: `lavi-py/build.rs`
- Create: `lavi-py/pyproject.toml`
- Create: `lavi-py/src/lib.rs`
- Create: `lavi-py/python/lavi/__init__.py`
- Create: `lavi-py/python/lavi/__init__.pyi`
- Create: `lavi-py/tests/test_lavi.py`
- Create: `scripts/stage-py-native.mjs`

**Interfaces:**
- Consumes: `lavi::{from_json_str, RuntimeMachine, SimulatedClock as CoreSimulatedClock, Value as CoreValue, ActionFunction, GuardFunction}`
- Produces (Python module `lavi`):
  - `class Value`: `is_null() -> bool`, `is_bool() -> bool`, `is_number() -> bool`, `is_str() -> bool`, `is_array() -> bool`, `is_object() -> bool`, `as_bool() -> Optional[bool]`, `as_float() -> Optional[float]`, `as_int() -> Optional[int]`, `as_str() -> Optional[str]`, `get(key: str, default: Any = None) -> Value | Any`, `__getitem__(key: str | int) -> Value`, `__contains__(key: str) -> bool`, `__len__() -> int`, `__bool__() -> bool`, `__eq__(other: Any) -> bool`, `to_python() -> Any`
  - `class SimulatedClock(initial_ms: int = 0)`: `now() -> int`, `set(target_ms: int) -> None`, `increment(dt_ms: int) -> None`, `close() -> None`, `__enter__() -> SimulatedClock`, `__exit__(...) -> None`
  - `class Machine(json_config: str, clock: Optional[SimulatedClock] = None)`: `start() -> Machine`, `stop() -> None`, `close() -> None`, `matches(state_path: str) -> bool`, `can(event: str, payload: Optional[dict[str, Any] | str] = None) -> bool`, `has_tag(tag: str) -> bool`, `is_done() -> bool`, `send(event: str | dict[str, Any], payload: Optional[dict[str, Any] | str] = None) -> None`, `tick() -> None`, `get_next_deadline() -> Optional[int]`, `get_snapshot() -> str`, `restore_snapshot(snapshot_json: str) -> None`, `__enter__() -> Machine`, `__exit__(...) -> None`

- [ ] **Step 1: Write the failing Python lifecycle, `Value`, and `SimulatedClock` tests in `lavi-py/tests/test_lavi.py`**

```python
import pytest
from lavi import Machine, SimulatedClock, Value

STOPLIGHT_JSON = """{
  "id": "trafficLight",
  "initial": "green",
  "context": {
    "cycles": 0,
    "ratio": 1.5,
    "label": "main",
    "active": true,
    "empty": null,
    "items": ["a", "b"],
    "nested": { "ok": true }
  },
  "states": {
    "green": {
      "tags": ["go"],
      "after": { "1000": "yellow" },
      "on": { "TIMER": "yellow" }
    },
    "yellow": { "on": { "TIMER": "red" } },
    "red": { "type": "final" }
  }
}"""

def test_invalid_json_raises_value_error():
    with pytest.raises(ValueError):
        Machine("{bad json")

def test_unstarted_machine_drops_events_and_matches_nothing():
    with Machine(STOPLIGHT_JSON) as m:
        assert not m.matches("green")
        assert not m.can("TIMER")
        assert not m.has_tag("go")
        assert not m.is_done()
        m.send("TIMER")
        m.start()
        assert m.matches("green")

def test_lifecycle_transitions_tags_done_and_snapshots():
    clock = SimulatedClock(100)
    with Machine(STOPLIGHT_JSON, clock=clock) as m:
        m.start()
        assert m.matches("green")
        assert m.can("TIMER")
        assert not m.can("UNKNOWN")
        assert m.has_tag("go")
        assert m.get_next_deadline() == 1100

        snap = m.get_snapshot()
        clock.increment(1000)
        assert m.matches("yellow")
        assert not m.has_tag("go")

        m.send("TIMER")
        assert m.matches("red")
        assert m.is_done()

        m.restore_snapshot(snap)
        assert m.matches("green")
        with pytest.raises(ValueError):
            m.restore_snapshot("{invalid_snapshot")

def test_stop_and_closed_machine_behavior():
    m = Machine(STOPLIGHT_JSON)
    m.start()
    m.stop()
    m.send("TIMER")
    assert m.matches("green")
    m.close()
    with pytest.raises(RuntimeError):
        m.send("TIMER")
```

- [ ] **Step 2: Create `lavi-py` scaffolding and `scripts/stage-py-native.mjs`, and verify `pytest` fails with `NotImplementedError` or missing symbols**

Configure `lavi-py/Cargo.toml` (`crate-type = ["cdylib"]`, `lavi = { workspace = true }`, `indexmap = "2.2"`, `pyo3 = { version = "0.22", features = ["extension-module", "abi3-py38"] }`), `lavi-py/build.rs` calling `pyo3_build_config::add_extension_module_link_args()`, and `scripts/stage-py-native.mjs` copying `target/debug/liblavi_py.dylib` (macOS) or `liblavi_py.so` (Linux) to `lavi-py/python/lavi/_lavi_py.abi3.so` (and `lavi_py.dll` to `_lavi_py.pyd` on Windows), re-exported from `lavi-py/python/lavi/__init__.py`.
Run: `cargo build -p lavi-py && node scripts/stage-py-native.mjs && PYTHONPATH=lavi-py/python python3 -m pytest lavi-py/tests/test_lavi.py`
Expected: FAIL (`ImportError: cannot import name 'Machine' from 'lavi'`).

- [ ] **Step 3: Implement `Value`, `SimulatedClock`, and `Machine` lifecycle methods in `lavi-py/src/lib.rs` and type stubs in `lavi-py/python/lavi/__init__.pyi`**

Implement `Value` wrapping `Arc<CoreValue>` with iterative heap `Vec` work-stack conversion in `to_python()` and Python object-to-`CoreValue` payload conversion for `send` and `can`. Implement `SimulatedClock` wrapping `Arc<Mutex<SimulatedClockInner>>` and `Machine` wrapping `RefCell<Option<MachineInner>>` (`None` when closed). Add `"build:py": "cargo build --package lavi-py && node scripts/stage-py-native.mjs"` and `"test:py": "npm run build:py && PYTHONPATH=lavi-py/python python3 -m pytest lavi-py/tests"` to `package.json`.

- [ ] **Step 4: Run `npm run test:py` to verify all Task 1 tests pass**

Run: `npm run test:py`
Expected: PASS (4 passed).

- [ ] **Step 5: Commit Task 1**

```bash
git add Cargo.toml Cargo.lock package.json scripts/stage-py-native.mjs lavi-py/
git commit -m "feat(py): add lavi-py core crate, Value, SimulatedClock, and Machine lifecycle"
```

---

### Task 2: `lavi-py` Callbacks, Re-Entrancy, Exception Short-Circuiting, and GC Traversal (~0.75d) [flowos: 2a253c8b-4e5d-4db9-92a2-1f9422153633]

**Files:**
- Modify: `lavi-py/src/lib.rs`
- Modify: `lavi-py/python/lavi/__init__.pyi`
- Modify: `lavi-py/tests/test_lavi.py`

**Interfaces:**
- Consumes: `Value`, `SimulatedClock`, `Machine` from Task 1.
- Produces on `lavi.Machine`:
  - `register_action(name: str, callback: Callable[[Value, Value], None]) -> Machine`
  - `register_guard(name: str, callback: Callable[[Value, Value], bool]) -> Machine`
  - `register_clock(callback: Callable[[], int] | SimulatedClock) -> Machine`
  - `action(name: str) -> Callable[[Callable[[Value, Value], None]], Callable[[Value, Value], None]]`
  - `guard(name: str) -> Callable[[Callable[[Value, Value], bool]], Callable[[Value, Value], bool]]`
  - `clock(callback: Callable[[], int]) -> Callable[[], int]`
  - PyO3 `__traverse__` and `__clear__` on `Machine` and `SimulatedClock`.

- [ ] **Step 1: Write failing tests for callbacks, `Value` inspection, re-entrant `send`/`matches`/`stop`, exception short-circuiting, and GC cycle collection in `lavi-py/tests/test_lavi.py`**

```python
import gc
import weakref
import pytest
from lavi import Machine, SimulatedClock

CALLBACK_MACHINE_JSON = """{
  "id": "cbMachine",
  "initial": "idle",
  "context": {
    "count": 2,
    "ratio": 1.5,
    "name": "sensor",
    "ready": true,
    "nothing": null,
    "items": [10, 20],
    "meta": { "tag": "v1" }
  },
  "states": {
    "idle": {
      "on": {
        "CHECK": [
          { "target": "active", "guard": "canActivate", "actions": ["onActivate", "secondAction"] },
          { "target": "fallback" }
        ]
      }
    },
    "active": {
      "after": {
        "100": { "target": "step2", "actions": ["onTick100"] }
      },
      "on": { " Chain": "done" }
    },
    "step2": {
      "after": {
        "100": { "target": "done", "actions": ["onTick200"] }
      }
    },
    "fallback": {},
    "done": { "type": "final" }
  }
}"""

def test_callbacks_value_ergonomics_and_retained_value_safety():
    retained = {}
    with Machine(CALLBACK_MACHINE_JSON) as m:
        @m.guard("canActivate")
        def check(ctx, evt):
            assert ctx["count"] == 2
            assert ctx["count"].as_int() == 2
            assert ctx["ratio"].as_float() == 1.5
            assert ctx["name"].as_str() == "sensor"
            assert bool(ctx["ready"]) is True
            assert bool(ctx["nothing"]) is False
            assert ctx["missing"]["deep"].is_null()
            assert bool(ctx["missing"]) is False
            assert ctx["items"][1].as_int() == 20
            assert ctx["items"][99].is_null()
            assert len(ctx["items"]) == 2
            assert "meta" in ctx
            assert ctx["meta"]["tag"] == "v1"
            assert evt["type"] == "CHECK"
            assert evt["attempt"].as_int() == 3
            retained["ctx"] = ctx
            retained["evt"] = evt
            return True

        m.register_action("onActivate", lambda ctx, evt: retained.setdefault("activated", ctx.to_python()))
        m.start()
        m.send("CHECK", payload={"attempt": 3})
        assert m.matches("active")

    # Retained Value objects remain safe after Machine is closed
    assert retained["ctx"]["name"].as_str() == "sensor"
    assert retained["evt"]["attempt"].as_int() == 3
    assert retained["activated"]["items"] == [10, 20]

def test_reentrant_send_matches_and_stop_inside_action_callback():
    with Machine(CALLBACK_MACHINE_JSON) as m:
        m.register_guard("canActivate", lambda ctx, evt: True)
        def reentrant_action(ctx, evt):
            assert m.matches("active")
            m.send({"type": " Chain"})
        m.register_action("onActivate", reentrant_action)
        m.start()
        m.send("CHECK")
        assert m.matches("done")
        assert m.is_done()

def test_callback_exception_short_circuits_step_and_simulated_clock():
    clock = SimulatedClock(0)
    second_called = False
    tick200_called = False
    with Machine(CALLBACK_MACHINE_JSON, clock=clock) as m:
        m.register_guard("canActivate", lambda ctx, evt: True)
        def fail_on_tick100(ctx, evt):
            raise RuntimeError("boom at 100ms")
        def on_tick200(ctx, evt):
            nonlocal tick200_called
            tick200_called = True
        m.register_action("onTick100", fail_on_tick100)
        m.register_action("onTick200", on_tick200)
        m.start()
        m.send("CHECK")
        with pytest.raises(RuntimeError, match="boom at 100ms"):
            clock.set(500)
        assert clock.now() == 100
        assert not tick200_called

def test_gc_collects_reference_cycle_between_machine_and_callback():
    def make_cycle():
        m = Machine(CALLBACK_MACHINE_JSON)
        m.register_action("onActivate", lambda ctx, evt: m.send(" Chain"))
        return weakref.ref(m)
    ref = make_cycle()
    gc.collect()
    assert ref() is None
```

- [ ] **Step 2: Run `npm run test:py` to verify the new tests fail**

Run: `npm run test:py`
Expected: FAIL (`AttributeError: 'lavi.Machine' object has no attribute 'guard'`).

- [ ] **Step 3: Implement callback registration, re-entrant `PendingCommand` queue, exception short-circuiting, and `__traverse__`/`__clear__` in `lavi-py/src/lib.rs`**

In `lavi-py/src/lib.rs`:
- Store registered Python callables (`Py<PyAny>`) in `MachineState` and bind synthetic `ActionFunction` / `GuardFunction` closures that acquire `Python::with_gil`, construct `Value(Arc::new(ctx.clone()))` and `Value(Arc::new(evt.clone()))`, and invoke the Python callable.
- Track `in_callback: Cell<bool>` and `aborted: Cell<bool>` plus `pending_commands: RefCell<VecDeque<PendingCommand>>` (`Send(String, Option<CoreValue>)`, `Stop`). While `in_callback` is true, `send()` and `stop()` push to `pending_commands` without borrowing `RuntimeMachine` mutably, and read queries (`matches`, `can`, `has_tag`, `is_done`, `get_next_deadline`) read from the active `RuntimeMachine` pointer safely. After each `RuntimeMachine` step yields, drain `pending_commands` while `!aborted.get()`.
- If a Python callback raises `PyErr`, store it via `PyErr::restore(py)` + `aborted.set(true)`, skip remaining foreign callbacks in the step, break any outer `SimulatedClock.set()`/`increment()` loop immediately, and return `Err(PyErr::fetch(py))`.
- Implement `__traverse__` and `__clear__` on `Machine` and `SimulatedClock`.

- [ ] **Step 4: Run `npm run test:py` and `cargo clippy` to verify all Python tests and lints pass**

Run: `npm run test:py && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS (8 passed, 0 clippy warnings).

- [ ] **Step 5: Commit Task 2**

```bash
git add lavi-py/
git commit -m "feat(py): add callbacks, re-entrant queue, exception short-circuiting, and GC traversal"
```

---

## Phase 2: Java and Kotlin Bindings (`lavi-jvm`) [flowos: 557b3e6f-60be-4cf4-a797-6c0b2932d51f]

### Task 3: `lavi-jvm` Rust JNI Bridge, `NativeLoader`, and Java API (~1d) [flowos: 94f6b527-442c-45c9-90a1-e8ed49a00c21]

**Files:**
- Modify: `Cargo.toml`
- Create: `lavi-jvm/Cargo.toml`
- Create: `lavi-jvm/src/lib.rs`
- Create: `lavi-jvm/settings.gradle.kts`
- Create: `lavi-jvm/build.gradle.kts`
- Create: `lavi-jvm/src/main/java/io/lavi/ActionCallback.java`
- Create: `lavi-jvm/src/main/java/io/lavi/GuardCallback.java`
- Create: `lavi-jvm/src/main/java/io/lavi/ClockCallback.java`
- Create: `lavi-jvm/src/main/java/io/lavi/NativeLoader.java`
- Create: `lavi-jvm/src/main/java/io/lavi/Value.java`
- Create: `lavi-jvm/src/main/java/io/lavi/SimulatedClock.java`
- Create: `lavi-jvm/src/main/java/io/lavi/Machine.java`
- Create: `lavi-jvm/src/test/java/io/lavi/JavaMachineTest.java`

**Interfaces:**
- Consumes: `lavi::{from_json_str, RuntimeMachine, SimulatedClock as CoreSimulatedClock, Value as CoreValue, ActionFunction, GuardFunction}`
- Produces (`io.lavi` Java package):
  - `@FunctionalInterface public interface ActionCallback { void execute(Value context, Value event); }`
  - `@FunctionalInterface public interface GuardCallback { boolean evaluate(Value context, Value event); }`
  - `@FunctionalInterface public interface ClockCallback { long nowMs(); }`
  - `public final class NativeLoader`: `public static synchronized void load()`, `static String resolveTargetArch(String osName, String osArch, boolean isMusl)`, `static Path extractOrReuse(InputStream stream, String versionHash, Path baseDir, String fileName)`
  - `public final class Value`: `isNull() -> boolean`, `isBoolean() -> boolean`, `isNumber() -> boolean`, `isString() -> boolean`, `isArray() -> boolean`, `isObject() -> boolean`, `asBoolean() -> Optional<Boolean>`, `asNumber() -> Optional<Double>`, `asLong() -> Optional<Long>`, `asString() -> Optional<String>`, `getProperty(String key) -> Value`, `getElement(int index) -> Value`, `size() -> int`, `hasProperty(String key) -> boolean`
  - `public final class SimulatedClock implements AutoCloseable`: `SimulatedClock()`, `SimulatedClock(long initialMs)`, `now() -> long`, `set(long targetMs) -> void`, `increment(long dtMs) -> void`, `close() -> void`
  - `public final class Machine implements AutoCloseable`: `Machine(String jsonConfig)`, `Machine(String jsonConfig, SimulatedClock clock)`, `registerAction(String name, ActionCallback cb) -> Machine`, `registerGuard(String name, GuardCallback cb) -> Machine`, `registerClock(ClockCallback cb) -> Machine`, `registerClock(SimulatedClock clock) -> Machine`, `start() -> Machine`, `stop() -> void`, `close() -> void`, `matches(String statePath) -> boolean`, `can(String event) -> boolean`, `can(String event, String payloadJson) -> boolean`, `hasTag(String tag) -> boolean`, `isDone() -> boolean`, `send(String event) -> void`, `send(String event, String payloadJson) -> void`, `tick() -> void`, `getNextDeadline() -> OptionalLong`, `getSnapshot() -> String`, `restoreSnapshot(String snapshotJson) -> void`

- [ ] **Step 1: Write failing Java JUnit 5 tests in `lavi-jvm/src/test/java/io/lavi/JavaMachineTest.java`**

```java
package io.lavi;

import org.junit.jupiter.api.Test;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.concurrent.atomic.AtomicReference;
import static org.junit.jupiter.api.Assertions.*;

class JavaMachineTest {
    private static final String MACHINE_JSON = "{\n" +
        "  \"id\": \"traffic\",\n" +
        "  \"initial\": \"green\",\n" +
        "  \"context\": { \"cycles\": 2, \"ratio\": 1.5, \"label\": \"main\", \"ok\": true, \"items\": [\"x\", \"y\"] },\n" +
        "  \"states\": {\n" +
        "    \"green\": {\n" +
        "      \"tags\": [\"go\"],\n" +
        "      \"after\": { \"500\": \"yellow\" },\n" +
        "      \"on\": {\n" +
        "        \"TIMER\": { \"target\": \"yellow\", \"guard\": \"canAdvance\", \"actions\": [\"onAdvance\"] }\n" +
        "      }\n" +
        "    },\n" +
        "    \"yellow\": { \"on\": { \"FINISH\": \"red\" } },\n" +
        "    \"red\": { \"type\": \"final\" }\n" +
        "  }\n" +
        "}";

    @Test
    void invalidJsonThrowsIllegalArgumentException() {
        assertThrows(IllegalArgumentException.class, () -> new Machine("{bad json"));
    }

    @Test
    void unstartedMachineDropsEventsAndMatchesNothing() {
        try (Machine m = new Machine(MACHINE_JSON)) {
            assertFalse(m.matches("green"));
            assertFalse(m.can("TIMER"));
            assertFalse(m.hasTag("go"));
            assertFalse(m.isDone());
            m.send("TIMER");
            m.start();
            assertTrue(m.matches("green"));
        }
    }

    @Test
    void callbacksValueInspectionReentrancyAndRetainedValueSafety() {
        AtomicReference<Value> savedCtx = new AtomicReference<>();
        AtomicReference<Value> savedEvt = new AtomicReference<>();
        try (SimulatedClock clock = new SimulatedClock(100);
             Machine m = new Machine(MACHINE_JSON, clock)) {
            m.registerGuard("canAdvance", (ctx, evt) -> {
                assertEquals(2L, ctx.getProperty("cycles").asLong().orElse(-1L));
                assertEquals(1.5, ctx.getProperty("ratio").asNumber().orElse(0.0));
                assertEquals("main", ctx.getProperty("label").asString().orElse(""));
                assertTrue(ctx.getProperty("ok").asBoolean().orElse(false));
                assertEquals("y", ctx.getProperty("items").getElement(1).asString().orElse(""));
                assertTrue(ctx.getProperty("items").getElement(99).isNull());
                assertTrue(ctx.getProperty("missing").getProperty("deep").isNull());
                assertEquals(2, ctx.getProperty("items").size());
                assertEquals(7L, evt.getProperty("step").asLong().orElse(-1L));
                savedCtx.set(ctx);
                savedEvt.set(evt);
                return true;
            });
            m.registerAction("onAdvance", (ctx, evt) -> {
                assertTrue(m.matches("yellow"));
                m.send("FINISH");
            });
            m.start();
            assertEquals(600L, m.getNextDeadline().orElse(-1L));
            m.send("TIMER", "{\"step\": 7}");
            assertTrue(m.matches("red"));
            assertTrue(m.isDone());
        }
        assertEquals("main", savedCtx.get().getProperty("label").asString().orElse(""));
        assertEquals(7L, savedEvt.get().getProperty("step").asLong().orElse(-1L));
    }

    @Test
    void callbackExceptionShortCircuitsSimulatedClockAndPropagates() {
        AtomicBoolean secondTickCalled = new AtomicBoolean(false);
        String timerJson = "{\n" +
            "  \"initial\": \"a\",\n" +
            "  \"states\": {\n" +
            "    \"a\": { \"after\": { \"100\": { \"target\": \"b\", \"actions\": [\"boom\"] } } },\n" +
            "    \"b\": { \"after\": { \"100\": { \"target\": \"c\", \"actions\": [\"second\"] } } },\n" +
            "    \"c\": {}\n" +
            "  }\n" +
            "}";
        try (SimulatedClock clock = new SimulatedClock(0);
             Machine m = new Machine(timerJson, clock)) {
            m.registerAction("boom", (ctx, evt) -> { throw new IllegalStateException("fail at 100"); });
            m.registerAction("second", (ctx, evt) -> secondTickCalled.set(true));
            m.start();
            IllegalStateException ex = assertThrows(IllegalStateException.class, () -> clock.set(500));
            assertEquals("fail at 100", ex.getMessage());
            assertEquals(100L, clock.now());
            assertFalse(secondTickCalled.get());
        }
    }

    @Test
    void closedMachineThrowsIllegalStateException() {
        Machine m = new Machine(MACHINE_JSON);
        m.close();
        assertThrows(IllegalStateException.class, m::start);
    }
}
```

- [ ] **Step 2: Create `lavi-jvm` Cargo and Gradle configuration and verify `./gradlew test` fails before implementation**

Create `lavi-jvm/Cargo.toml` (`crate-type = ["cdylib"]`, `jni = "0.21"`, `lavi = { workspace = true }`, `indexmap = "2.2"`) and `lavi-jvm/build.gradle.kts` (`java-library`, `org.jetbrains.kotlin.jvm`, JUnit Jupiter, passing `-Dlavi.library.path=${rootProject.projectDir}/../target/debug` to the `test` task).
Run: `cargo build -p lavi-jvm && gradle -p lavi-jvm test`
Expected: FAIL (compilation failure or `UnsatisfiedLinkError`).

- [ ] **Step 3: Implement `lavi-jvm/src/lib.rs` and the `io.lavi.*` Java classes**

Implement:
- `Value.java`: Pure Java immutable tree materializer (or JNI-backed `Arc<CoreValue>` wrapper materializing into Java `Value` nodes iteratively) so `Value` has zero dangling native pointers and works outside callbacks without native finalizer pressure.
- `Machine.java` & `SimulatedClock.java`: Hold callback `Map`s on the Java `Machine` instance; expose private `dispatchAction(String name, Value ctx, Value evt)`, `dispatchGuard(String name, Value ctx, Value evt)`, and `dispatchClock()` called from JNI using the active `JNIEnv` and `jobject` during transitions; register `java.lang.ref.Cleaner` to free native Rust peers if `.close()` is omitted; handle re-entrant `send`/`stop` via a `PendingCommand` queue in Rust (`lavi-jvm/src/lib.rs`) and check `env.exception_check()` after every callback to short-circuit transitions and `SimulatedClock` loops.
- `NativeLoader.java`: Support `-Dlavi.library.path`, Android `System.loadLibrary("lavi_jvm")`, `musl` vs `gnu` detection on Linux, content-addressed extraction `<baseDir>/lavi-jni-<hash>/liblavi_jvm.<ext>`, and fallback to user cache dir (`~/.cache/lavi`) if `/tmp` is `noexec`.

- [ ] **Step 4: Run `cargo build -p lavi-jvm && gradle -p lavi-jvm test` to verify all Java tests pass**

Run: `cargo build -p lavi-jvm && gradle -p lavi-jvm test`
Expected: PASS (5 passed).

- [ ] **Step 5: Commit Task 3**

```bash
git add Cargo.toml Cargo.lock lavi-jvm/
git commit -m "feat(jvm): add lavi-jvm JNI bridge, NativeLoader, and Java API"
```

---

### Task 4: `lavi-jvm` Kotlin Idiomatic Extensions and Kotlin Test Suite (~0.5d) [flowos: 5bb94d39-6b59-4011-abda-516bf25ab15f]

**Files:**
- Create: `lavi-jvm/src/main/kotlin/io/lavi/MachineExtensions.kt`
- Create: `lavi-jvm/src/test/kotlin/io/lavi/KotlinMachineTest.kt`
- Modify: `package.json`

**Interfaces:**
- Consumes: `io.lavi.{Machine, SimulatedClock, Value}` from Task 3.
- Produces (`io.lavi` Kotlin extensions in `MachineExtensions.kt`):
  - `fun Machine.onAction(name: String, block: (context: Value, event: Value) -> Unit): Machine`
  - `fun Machine.onGuard(name: String, block: (context: Value, event: Value) -> Boolean): Machine`
  - `fun Machine.onClock(block: () -> Long): Machine`
  - `fun Machine.send(event: String, payload: Map<String, Any?>)`
  - `fun Machine.can(event: String, payload: Map<String, Any?>): Boolean`
  - `operator fun Value.get(key: String): Value`
  - `operator fun Value.get(index: Int): Value`
  - `operator fun Value.contains(key: String): Boolean`
  - `fun Value.asStringOrNull(): String?`
  - `fun Value.asDoubleOrNull(): Double?`
  - `fun Value.asLongOrNull(): Long?`
  - `fun Value.asBooleanOrNull(): Boolean?`

- [ ] **Step 1: Write failing Kotlin tests in `lavi-jvm/src/test/kotlin/io/lavi/KotlinMachineTest.kt`**

```kotlin
package io.lavi

import org.junit.jupiter.api.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertNull
import kotlin.test.assertTrue

class KotlinMachineTest {
    private val machineJson = """
        {
          "id": "kotlinDemo",
          "initial": "idle",
          "context": {
            "retries": 2,
            "temperature": 21.5,
            "name": "rover",
            "armed": true,
            "tags": ["alpha", "beta"]
          },
          "states": {
            "idle": {
              "on": {
                "LAUNCH": {
                  "target": "running",
                  "guard": "canLaunch",
                  "actions": ["logLaunch"]
                }
              }
            },
            "running": {
              "after": { "250": "done" }
            },
            "done": { "type": "final" }
          }
        }
    """.trimIndent()

    @Test
    fun kotlinTrailingLambdasOperatorIndexingAndMapPayloadsWork() {
        var loggedBoost: Long? = null
        SimulatedClock(0).use { clock ->
            Machine(machineJson, clock).use { machine ->
                machine.onGuard("canLaunch") { ctx, evt ->
                    assertEquals(2L, ctx["retries"].asLongOrNull())
                    assertEquals(21.5, ctx["temperature"].asDoubleOrNull())
                    assertEquals("rover", ctx["name"].asStringOrNull())
                    assertEquals(true, ctx["armed"].asBooleanOrNull())
                    assertEquals("beta", ctx["tags"][1].asStringOrNull())
                    assertNull(ctx["missing"]["deep"].asStringOrNull())
                    assertTrue("tags" in ctx)
                    (evt["boost"].asLongOrNull() ?: 0L) > 5L
                }
                machine.onAction("logLaunch") { _, evt ->
                    loggedBoost = evt["boost"].asLongOrNull()
                }

                machine.start()
                assertFalse(machine.can("LAUNCH", mapOf("boost" to 3)))
                assertTrue(machine.can("LAUNCH", mapOf("boost" to 10)))

                machine.send("LAUNCH", mapOf("boost" to 10, "nested" to mapOf("ok" to true), "list" to listOf(1, 2)))
                assertTrue(machine.matches("running"))
                assertEquals(10L, loggedBoost)

                clock.increment(250)
                assertTrue(machine.isDone)
            }
        }
    }
}
```

- [ ] **Step 2: Run `gradle -p lavi-jvm test` to verify compilation fails before `MachineExtensions.kt` exists**

Run: `gradle -p lavi-jvm test`
Expected: FAIL (`Unresolved reference: onGuard`).

- [ ] **Step 3: Implement `lavi-jvm/src/main/kotlin/io/lavi/MachineExtensions.kt` and wire `build:jvm` and `test:jvm` into `package.json`**

Implement the extension functions and an iterative heap work-stack `Map<String, Any?>` $\rightarrow$ JSON serializer for `send` and `can` payloads in `MachineExtensions.kt`. Add `"build:jvm": "cargo build --package lavi-jvm && gradle -p lavi-jvm assemble"` and `"test:jvm": "cargo build --package lavi-jvm && gradle -p lavi-jvm test"` to `package.json`.

- [ ] **Step 4: Run `npm run test:jvm` to verify both Java and Kotlin test suites pass**

Run: `npm run test:jvm`
Expected: PASS (JavaMachineTest and KotlinMachineTest pass).

- [ ] **Step 5: Commit Task 4**

```bash
git add package.json lavi-jvm/
git commit -m "feat(jvm): add Kotlin idiomatic extensions and Kotlin test suite"
```

---

## Phase 3: Release Packaging and Cross-Target Documentation [flowos: 2d764551-9a48-41ce-af4a-a5b51dfb8d33]

### Task 5: Single-Host Cross-Compilation Script, Android AAR Packager, and Repo Docs (~0.5d) [flowos: cf685629-5342-4ed3-9b4c-3e1552a4879d]

**Files:**
- Create: `scripts/build-release-natives.mjs`
- Create: `tests/build-release-natives.test.ts`
- Modify: `package.json`
- Modify: `PROCESS.md`
- Modify: `README.md`

**Interfaces:**
- Consumes: `lavi-py` and `lavi-jvm` crates and Gradle outputs from Tasks 1–4.
- Produces (`scripts/build-release-natives.mjs` exports):
  - `DESKTOP_TARGETS`: `aarch64-apple-darwin`, `x86_64-apple-darwin`, `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`, `x86_64-unknown-linux-musl`, `aarch64-unknown-linux-musl`, `x86_64-pc-windows-gnu`
  - `ANDROID_TARGETS`: `arm64-v8a` (`aarch64-linux-android`), `armeabi-v7a` (`armv7-linux-androideabi`), `x86_64` (`x86_64-linux-android`) with `-C link-arg=-Wl,-z,max-page-size=16384`
  - `buildReleasePlan(options)` returning the exact command plan for `cargo zigbuild`, `maturin build --zig`, `cargo ndk`, ELF 16 KB (`0x4000`) alignment check, and `.aar` packaging (`AndroidManifest.xml`, `classes.jar`, `jni/<abi>/liblavi_jvm.so`).

- [ ] **Step 1: Write failing Vitest unit tests in `tests/build-release-natives.test.ts`**

```ts
import { describe, it, expect } from 'vitest';
import {
  DESKTOP_TARGETS,
  ANDROID_TARGETS,
  buildReleasePlan,
  verifyElf16KbAlignmentOutput,
} from '../scripts/build-release-natives.mjs';

describe('build-release-natives', () => {
  it('includes all required desktop gnu, musl, darwin, and windows targets', () => {
    const triples = DESKTOP_TARGETS.map((t: any) => t.triple);
    expect(triples).toContain('x86_64-unknown-linux-gnu');
    expect(triples).toContain('aarch64-unknown-linux-gnu');
    expect(triples).toContain('x86_64-unknown-linux-musl');
    expect(triples).toContain('aarch64-unknown-linux-musl');
    expect(triples).toContain('aarch64-apple-darwin');
    expect(triples).toContain('x86_64-apple-darwin');
    expect(triples).toContain('x86_64-pc-windows-gnu');
  });

  it('enforces 16 KB page alignment link flags on 64-bit Android targets', () => {
    const plan = buildReleasePlan({ dryRun: true });
    const arm64Cmd = plan.androidCommands.find((c: any) => c.abi === 'arm64-v8a');
    expect(arm64Cmd.rustflags).toContain('-C link-arg=-Wl,-z,max-page-size=16384');
    const x86Cmd = plan.androidCommands.find((c: any) => c.abi === 'x86_64');
    expect(x86Cmd.rustflags).toContain('-C link-arg=-Wl,-z,max-page-size=16384');
  });

  it('validates ELF LOAD segment alignment is at least 0x4000 (16 KB)', () => {
    const validReadelf = '  LOAD           0x000000 0x0000000000000000 0x0000000000000000 0x012340 0x012340 R E 0x4000';
    const invalidReadelf = '  LOAD           0x000000 0x0000000000000000 0x0000000000000000 0x012340 0x012340 R E 0x1000';
    expect(verifyElf16KbAlignmentOutput(validReadelf)).toBe(true);
    expect(verifyElf16KbAlignmentOutput(invalidReadelf)).toBe(false);
  });
});
```

- [ ] **Step 2: Run Vitest on `tests/build-release-natives.test.ts` and verify it fails**

Run: `npx vitest run tests/build-release-natives.test.ts`
Expected: FAIL (`Cannot find module '../scripts/build-release-natives.mjs'`).

- [ ] **Step 3: Implement `scripts/build-release-natives.mjs` and update `package.json`, `PROCESS.md`, and `README.md`**

Implement `scripts/build-release-natives.mjs`, wire `test:py` and `test:jvm` into `npm test` and `npm run build` in `package.json`, and document the Python (`lavi-py`) and Java/Kotlin (`lavi-jvm`) usage examples and workflow in `README.md` and `PROCESS.md`.

- [ ] **Step 4: Run the full verification suite (`npm run build && npm test && cargo clippy`)**

Run: `cargo clippy --workspace --all-targets -- -D warnings && npm run build && npm test`
Expected: PASS (all conformance checks, native Rust tests, WASM tests, E2E tests, C/C++ tests, Python pytest tests, JVM Java/Kotlin JUnit tests, and Vitest unit tests pass).

- [ ] **Step 5: Commit Task 5**

```bash
git add scripts/build-release-natives.mjs tests/build-release-natives.test.ts package.json PROCESS.md README.md
git commit -m "feat(release): add single-host cross-compilation script, AAR packager, and Python/JVM docs"
```

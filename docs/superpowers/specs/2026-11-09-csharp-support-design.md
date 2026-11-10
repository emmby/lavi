# Lavi C# / .NET Support (`lavi-csharp`) Design Specification

**Date:** 2026-11-09
**Status:** Approved

---

## 1. Intent and Why

Lavi provides a portable, `#![no_std]` Rust statechart and actor runtime compatible with XState v5 across WebAssembly/JS/TS/React/React Native (`lavi-wasm` / `lavi-fsm`), C/C++ for embedded microcontrollers (`lavi-c-cpp`), Python (`lavi-py`), Java/Kotlin/Android (`lavi-jvm`), Swift/Apple (`lavi-swift`), and Dart/Flutter (`lavi-dart`).

Adding C# / .NET support (`lavi-csharp`) extends Lavi to the .NET ecosystem—including .NET 8+ backend services, desktop applications, and C#-based game engines (such as Godot 4 .NET and Unity via standard P/Invoke)—using the existing C ABI exported by `lavi-c-cpp` (`include/lavi.h`) without modifying the `#![no_std]` Rust core.

The C# binding is designed around two core pillars:
1. **Idiomatic C# / .NET Developer Ergonomics**: Method overloading (`RegisterAction`, `RegisterGuard`, `RegisterDelay`, `RegisterClock`, `Send`), fluent builder chaining (`return this`), anonymous object / POCO / record serialization for `input` and event payloads, `System.Text.Json.JsonElement`-style `TryGet*` and typed accessors (`AsInt32`, `AsInt64`, `AsDouble`, `AsString`, `AsBool`) on `Value`, `TimeSpan` overloads alongside millisecond (`ulong`) overloads, C# `event` support (`Inspected`), and deterministic `IDisposable` (`using var`) resource management.
2. **XState v5 APIs & Spirit**: First-class `MachineSnapshot` returned by `GetSnapshot()` (exposing `Status`, `StateValue`, `Context`, `Output`, `Tags`, `Matches(...)`, `Can(...)`, `HasTag(...)`, `ToJson()`, and implicit conversion to `string`), `Subscribe(Action<MachineSnapshot>)` returning `IDisposable` for reactive state observation, and `Machine.CreateActor(...)` factory support alongside `new Machine(...)`.

---

## 2. Goals, Non-Goals, and Anti-Goals

### Goals
1. **Complete C ABI Feature Parity with Idiomatic C# & XState v5 Ergonomics**: Expose all statechart runtime capabilities in [`lavi-c-cpp/include/lavi.h`](../../../lavi-c-cpp/include/lavi.h) through the `Lavi` namespace:
   - **Creation & XState v5 Factory**:
     - `new Machine(string jsonConfig, object? input = null, SimulatedClock? clock = null)` (where `input` accepts raw JSON `string`, anonymous objects, records, POCOs, or `IDictionary<string, object?>`)
     - `Machine.CreateActor(string jsonConfig, object? input = null, SimulatedClock? clock = null)`
   - **Fluent Registration & Overloading** (returns `Machine` for chaining, supports pre-start and post-start registration and callback replacement):
     - `RegisterAction(string name, Action<Value, Value> callback)` (2-arg `(context, event)`)
     - `RegisterAction(string name, Action<Value, Value, Value> callback)` (3-arg `(context, event, params)`, plus `RegisterActionWithParams` alias)
     - `RegisterGuard(string name, Func<Value, Value, bool> callback)` (2-arg `(context, event)`)
     - `RegisterGuard(string name, Func<Value, Value, Value, bool> callback)` (3-arg `(context, event, params)`, plus `RegisterGuardWithParams` alias)
     - `RegisterDelay(string name, Func<Value, Value, ulong> callback)` and `RegisterDelay(string name, Func<Value, Value, TimeSpan> callback)`
     - `RegisterClock(SimulatedClock clock)` and `RegisterClock(Func<ulong> clockFn)`
     - `RegisterInspector(Action<string> callback)` and C# `event Action<string>? Inspected`
     - `Subscribe(Action<MachineSnapshot> observer)` returning `IDisposable`
   - **Lifecycle & Queries**:
     - `Start()` (returns `Machine` for fluent chaining; `TryStart()` returns `bool`), `Stop()`, `Dispose()` / `Close()`
     - `IsValid`, `IsDone`, `State`, `NextDeadline` (`ulong?`), `Tick()`
     - `Matches(string statePath)`, `Can(string eventName)`, `HasTag(string tag)`
   - **Event Dispatch**:
     - `Send(string eventName)`
     - `Send(string eventName, object? payload)` (merges `"type": eventName` with anonymous object, record, POCO, or dictionary properties)
     - `Send(object eventObject)` (XState v5 style `machine.Send(new { type = "PEDESTRIAN_WAIT", urgency = 42 })`)
     - `SendJson(string jsonEvent)`
   - **XState v5 `MachineSnapshot` & Persistence**:
     - `GetSnapshot()` returning `MachineSnapshot` (`Status`, `StateValue`, `Context` as `JsonElement`, `Output` as `JsonElement?`, `Tags` as `IReadOnlyList<string>`, `Matches(string)`, `Can(string)`, `HasTag(string)`, `ToJson()`, and `implicit operator string`)
     - `GetSnapshotJson()` returning `string`
     - `RestoreSnapshot(string snapshotJson)` and `RestoreSnapshot(MachineSnapshot snapshot)` returning `bool`
     - `GetOutput()`, `GetMeta()`, `GetStateMeta(string statePath)`
   - **Deterministic Virtual Time (`SimulatedClock : IDisposable`)**:
     - `Now` (`ulong`), `Set(ulong targetMs)`, `Set(TimeSpan target)`, `Increment(ulong dtMs)`, `Increment(TimeSpan dt)`, `Close()`, `Dispose()`
   - **Read-Only Callback Value Inspection (`readonly struct Value`)**:
     - Type checks: `IsNull`, `IsObject`, `IsString`, `IsNumber`, `IsBool`
     - Idiomatic nullable properties: `AsString` / `StringValue`, `AsDouble` / `NumberValue`, `AsInt32` (`int?`), `AsInt64` (`long?`), `AsBool` / `BoolValue`
     - Standard .NET `TryGet*` methods: `TryGetString(out string value)`, `TryGetDouble(out double value)`, `TryGetInt32(out int value)`, `TryGetInt64(out long value)`, `TryGetBoolean(out bool value)`
     - Navigation: `Property(string key)` and indexer `this[string key]`
2. **GC-Safe Unmanaged Callback Interop**: Pin a `GCHandle` per `Machine` instance for `void* user_data` and keep static `[UnmanagedFunctionPointer(CallingConvention.Cdecl)]` delegate instances alive in static readonly fields for the lifetime of the process so the .NET garbage collector never collects callback trampolines during native execution or leaks managed objects after `Dispose()`.
3. **Zero-Friction Native Library Loading**: Stage `liblavi_c_cpp.{dylib,so,dll}` into `lavi-csharp/native/` via `scripts/stage-csharp-native.mjs` (removing the transient shared library from `target/{debug,release}` afterwards so C/C++/Swift static linking tests remain unaffected, matching `scripts/stage-dart-native.mjs`) and resolve it automatically via `NativeLibrary.SetDllImportResolver`.
4. **First-Class Workspace, CI, Version Sync & Example Integration**:
   - xUnit test suite in `lavi-csharp/Lavi.Tests/LaviTests.cs`.
   - Self-contained stoplight example in `examples/csharp/` (`Program.cs` + `StoplightExample.csproj`) wired into `npm run build:examples` and `npm run test:examples`.
   - `build:csharp` and `test:csharp` scripts in `package.json`, included in `npm run build` and `npm test`.
   - Version synchronization for `lavi-csharp/Lavi/Lavi.csproj` and `examples/csharp/StoplightExample.csproj` in `scripts/sync-version.mjs` and its unit tests (`tests/sync-version.test.ts`).
   - `.NET 8` SDK installation step in `.github/workflows/ci.yml`.
   - Documentation updates across `README.md`, `RELEASING.md`, `AGENTS.md`, and `XSTATE_5_FEATURES.md`.

### Non-Goals
- Modifying the `#![no_std]` Rust core (`lavi`) or C ABI (`lavi-c-cpp`).
- Building a dynamic C# expression evaluator or runtime reflection engine (per `AGENTS.md` Level 3).
- Automated NuGet.org publishing in `.github/workflows/release.yml` in this initial track (local/workspace consumption and `dotnet pack` support are sufficient initially, matching `lavi-dart`).

### Anti-Goals
- Leaving `liblavi_c_cpp.dylib` / `.so` in `target/debug` after staging, which would cause `clang` / `swift test` to dynamically link instead of statically linking `liblavi_c_cpp.a`.
- Passing unpinned instance delegates directly to C without retaining static trampoline delegate references or freeing `GCHandle` on `Dispose()`.
- Throwing exceptions on unstarted or disposed `Machine` queries (must return safe defaults: `false`, `""`, `null` for `NextDeadline`, empty `MachineSnapshot`, matching `lavi-dart` and `lavi-swift`).

---

## 3. Constraints & Decisions

### Decisions Made
1. **Project Layout (`lavi-csharp/` & `examples/csharp/`)**:
   - `lavi-csharp/Lavi.sln`: Solution containing `Lavi/Lavi.csproj` and `Lavi.Tests/Lavi.Tests.csproj`.
   - `lavi-csharp/Lavi/Lavi.csproj`: Class library targeting `net8.0` with `<Version>0.2.0</Version>`, `<Nullable>enable</Nullable>`, and `<ImplicitUsings>enable</ImplicitUsings>`.
   - `lavi-csharp/Lavi/NativeLoader.cs`: Static initializer using `NativeLibrary.SetDllImportResolver` to resolve `"lavi_c_cpp"` from `LAVI_LIB_PATH`, `lavi-csharp/native/`, `native/`, `target/debug/`, `target/release/`, and OS default search paths.
   - `lavi-csharp/Lavi/NativeMethods.cs`: P/Invoke declarations (`[DllImport("lavi_c_cpp", CallingConvention = CallingConvention.Cdecl)]`) for all C ABI functions in `lavi.h` and `[UnmanagedFunctionPointer(CallingConvention.Cdecl)]` callback delegate types.
   - `lavi-csharp/Lavi/Value.cs`: `public readonly struct Value` wrapping `IntPtr` (`const fsm_value_t*`) with UTF-8 string decoding, typed properties (`AsString`, `AsDouble`, `AsInt32`, `AsInt64`, `AsBool`), `TryGet*` methods, `Property(string key)`, and `this[string key]` indexer.
   - `lavi-csharp/Lavi/SimulatedClock.cs`: `public sealed class SimulatedClock : IDisposable` wrapping `IntPtr` (`SimulatedClock*`) with `Now`, `Set(ulong)` / `Set(TimeSpan)`, `Increment(ulong)` / `Increment(TimeSpan)`, `Close()`, and `Dispose()`.
   - `lavi-csharp/Lavi/MachineSnapshot.cs`: `public sealed class MachineSnapshot` parsing the snapshot JSON (`status`, `value`, `context`, `tags`) + optional output JSON, delegating live `.Matches(string)`, `.Can(string)`, and `.HasTag(string)` checks when attached to an active `Machine` (and falling back to parsed snapshot data when detached), with `ToJson()` and `implicit operator string`.
   - `lavi-csharp/Lavi/Machine.cs`: `public sealed class Machine : IDisposable` with fluent registration overloads, `GCHandle` callback routing, `Subscribe(Action<MachineSnapshot>)`, `Inspected` event, anonymous-object/dictionary `input` and `Send` serialization, and two-call buffer sizing for native JSON string queries.
   - `lavi-csharp/Lavi.Tests/LaviTests.cs`: xUnit test suite covering:
     1. Unstarted and closed/disposed safe defaults
     2. Fluent registration, overloaded `RegisterAction` / `RegisterGuard` (2-arg and 3-arg parameterized), `TimeSpan` delays & `SimulatedClock`, and `Value` typed inspection (`AsInt32`, `TryGet*`, missing/deep property access)
     3. Structured `MachineSnapshot` (`GetSnapshot()`), `Subscribe(...)` reactive observer, snapshot restoration on an unstarted machine, and final state output serialization
     4. Callback replacement, custom clock callback (`RegisterClock(Func<ulong>)`), `Tick()`, and `Stop()`
     5. Inspector callback (`RegisterInspector` and `Inspected` C# event) receiving `@xstate.microstep` and `@xstate.snapshot` JSON events
     6. Anonymous object `input` (`new { cycles = 2 }`) and anonymous object / XState event object `Send` (`machine.Send("PEDESTRIAN_WAIT", new { urgency = 42 })` and `machine.Send(new { type = "PEDESTRIAN_WAIT", urgency = 42 })`)
   - `examples/csharp/StoplightExample.csproj` and `examples/csharp/Program.cs`: Self-contained executable referencing `../../lavi-csharp/Lavi/Lavi.csproj`, running the canonical 4-step stoplight verification using idiomatic C# (`using var`, fluent registration, `AsInt32`), and printing `C# example passed (transitions: 4)`.

### Left to the Builder
- Internal helper method organization within `NativeMethods.cs`, `MachineSnapshot.cs`, and `Machine.cs`.
- Exact xUnit assertion phrasing inside `LaviTests.cs` as long as all 6 test scenarios above are covered and pass via `dotnet test`.

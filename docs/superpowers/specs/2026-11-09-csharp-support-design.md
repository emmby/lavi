# Lavi C# / .NET Support (`lavi-csharp`) Design Specification

**Date:** 2026-11-09
**Status:** Approved

---

## 1. Intent and Why

Lavi provides a portable, `#![no_std]` Rust statechart and actor runtime compatible with XState v5 across WebAssembly/JS/TS/React/React Native (`lavi-wasm` / `lavi-fsm`), C/C++ for embedded microcontrollers (`lavi-c-cpp`), Python (`lavi-py`), Java/Kotlin/Android (`lavi-jvm`), Swift/Apple (`lavi-swift`), and Dart/Flutter (`lavi-dart`).

Adding C# / .NET support (`lavi-csharp`) extends Lavi to the .NET ecosystem—including .NET 8+ backend services, desktop applications, and C#-based game engines (such as Godot 4 .NET and Unity via standard P/Invoke)—using the existing C ABI exported by `lavi-c-cpp` (`include/lavi.h`) without modifying the `#![no_std]` Rust core.

---

## 2. Goals, Non-Goals, and Anti-Goals

### Goals
1. **Complete C ABI Feature Parity**: Expose all statechart runtime capabilities in [`lavi-c-cpp/include/lavi.h`](../../../lavi-c-cpp/include/lavi.h) through an idiomatic C# API in the `Lavi` namespace:
   - `Machine` lifecycle (`Start`, `Stop`, `Dispose` / `Close`, `IsValid`, `IsDone`, `State`, `NextDeadline`, `Tick`)
   - Statechart queries (`Matches`, `Can`, `HasTag`)
   - Event dispatch (`Send(string eventName, IDictionary<string, object?>? payload = null)`, `SendJson(string jsonEvent)`)
   - Named callback registration (`RegisterAction`, `RegisterActionWithParams`, `RegisterGuard`, `RegisterGuardWithParams`, `RegisterDelay`, `RegisterClock`, `RegisterInspector`) supporting both pre-start and post-start registration and callback replacement
   - Deterministic virtual time via `SimulatedClock` (`Now`, `Set`, `Increment`, `Dispose` / `Close`)
   - Read-only callback value inspection via `Value` (`IsNull`, `IsObject`, `IsString`, `IsNumber`, `IsBool`, `StringValue`, `NumberValue`, `BoolValue`, `Property(string key)`, and indexer `this[string key]`)
   - Snapshot & metadata serialization (`GetSnapshot`, `RestoreSnapshot`, `GetOutput`, `GetMeta`, `GetStateMeta`)
2. **GC-Safe Unmanaged Callback Interop**: Pin a `GCHandle` per `Machine` instance for `void* user_data` and keep static `[UnmanagedFunctionPointer(CallingConvention.Cdecl)]` delegate instances alive for the lifetime of the process so the .NET garbage collector never collects callback trampolines during native execution or leaks managed objects after `Dispose()`.
3. **Zero-Friction Native Library Loading**: Stage `liblavi_c_cpp.{dylib,so,dll}` into `lavi-csharp/native/` via `scripts/stage-csharp-native.mjs` (removing the transient shared library from `target/{debug,release}` afterwards so C/C++/Swift static linking tests remain unaffected, matching `scripts/stage-dart-native.mjs`) and resolve it automatically via `NativeLibrary.SetDllImportResolver`.
4. **First-Class Workspace, CI, Version Sync & Example Integration**:
   - xUnit test suite in `lavi-csharp/Lavi.Tests/LaviTests.cs` matching the behavioral coverage of `lavi-dart/test/lavi_test.dart` and `lavi-swift/Tests/LaviTests/LaviTests.swift`.
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
- Passing unpinned instance delegates directly to C without retaining delegate references or freeing `GCHandle` on `Dispose()`.
- Returning `null` or throwing exceptions on unstarted or disposed `Machine` queries (must return safe defaults: `false`, `""`, `null` for `NextDeadline`, matching `lavi-dart` and `lavi-swift`).

---

## 3. Constraints & Decisions

### Decisions Made (Designer's Call, Approved by Partner)
1. **Project Layout (`lavi-csharp/` & `examples/csharp/`)**:
   - `lavi-csharp/Lavi.sln`: Solution containing `Lavi/Lavi.csproj` and `Lavi.Tests/Lavi.Tests.csproj`.
   - `lavi-csharp/Lavi/Lavi.csproj`: Class library targeting `net8.0` with `<Version>0.2.0</Version>`, `<Nullable>enable</Nullable>`, `<ImplicitUsings>enable</ImplicitUsings>`, and `<AllowUnsafeBlocks>true</AllowUnsafeBlocks>`.
   - `lavi-csharp/Lavi/NativeLoader.cs`: Static initializer using `NativeLibrary.SetDllImportResolver` to resolve `"lavi_c_cpp"` from:
     1. `LAVI_LIB_PATH` environment variable (if set and file exists)
     2. Assembly directory & parent search paths (`lavi-csharp/native/`, `native/`, `target/debug/`, `target/release/` relative to `AppContext.BaseDirectory` and `Directory.GetCurrentDirectory()`)
     3. OS fallback via `NativeLibrary.TryLoad`
   - `lavi-csharp/Lavi/NativeMethods.cs`: P/Invoke declarations (`[DllImport("lavi_c_cpp", CallingConvention = CallingConvention.Cdecl)]`) for all C ABI functions in `lavi.h` and `[UnmanagedFunctionPointer(CallingConvention.Cdecl)]` callback delegate types.
   - `lavi-csharp/Lavi/Value.cs`: `public readonly struct Value` wrapping `IntPtr` (`const fsm_value_t*`) with UTF-8 string decoding (`fsm_value_get_string` two-call sizing pattern), `NumberValue`, `BoolValue`, `Property(string key)`, and `this[string key]` indexer.
   - `lavi-csharp/Lavi/SimulatedClock.cs`: `public sealed class SimulatedClock : IDisposable` wrapping `IntPtr` (`SimulatedClock*`) with `Now`, `Set(ulong targetMs)`, `Increment(ulong dtMs)`, `Close()`, and `Dispose()`.
   - `lavi-csharp/Lavi/Machine.cs`: `public sealed class Machine : IDisposable` wrapping `IntPtr` (`MachineContext*`) with `GCHandle` user-data routing to instance callback dictionaries, safe unstarted/disposed defaults, two-call buffer sizing for `State`, `GetSnapshot()`, `GetOutput()`, `GetMeta()`, and `GetStateMeta(string statePath)`, and `System.Text.Json` event payload serialization for `Send(string eventName, IDictionary<string, object?>? payload = null)`.
   - `lavi-csharp/Lavi.Tests/LaviTests.cs`: xUnit test suite covering:
     1. Unstarted and closed/disposed safe defaults
     2. Transitions, actions, parameterized actions, guards, parameterized guards, custom delays, `SimulatedClock`, and `Value` inspection (including missing/deep property access)
     3. Snapshot capture and restoration on an unstarted machine + final state output serialization
     4. Callback replacement (`RegisterAction` overwriting previous callback), custom clock callback (`RegisterClock(Func<ulong>)`), `Tick()`, and `Stop()`
     5. Inspector callback (`RegisterInspector`) receiving `@xstate.microstep` and `@xstate.snapshot` JSON events
     6. Input JSON (`inputJson`) initialization and dictionary payload `Send(eventName, payload)`
   - `examples/csharp/StoplightExample.csproj` and `examples/csharp/Program.cs`: Self-contained executable referencing `../../lavi-csharp/Lavi/Lavi.csproj`, running the canonical 4-step stoplight verification, and printing `C# example passed (transitions: 4)`.

### Left to the Builder
- Internal helper method organization within `NativeMethods.cs` and `Machine.cs`.
- Exact xUnit assertion phrasing inside `LaviTests.cs` as long as all 6 test scenarios above are covered and pass via `dotnet test`.

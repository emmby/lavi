# Lavi C# / .NET Support (`lavi-csharp`) Implementation Plan [flowos: 2704eacb-b859-4786-812a-46ad051578ac]

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add first-class C# / .NET support (`lavi-csharp`) wrapping the `lavi-c-cpp` C ABI with idiomatic C# developer ergonomics and XState v5 `MachineSnapshot` / `Subscribe` APIs.

**Architecture:** `lavi-csharp` provides a `.NET 8.0` class library (`Lavi`) that loads `liblavi_c_cpp.{dylib,so,dll}` via `NativeLibrary.SetDllImportResolver` and P/Invoke (`CallingConvention.Cdecl`), routing native callbacks to managed delegates via a per-instance `GCHandle` user-data pointer and static `[UnmanagedFunctionPointer]` trampolines. It exposes `Machine`, `MachineSnapshot`, `SimulatedClock`, and `Value` with method overloading, fluent chaining, `System.Text.Json` anonymous-object serialization, and `TimeSpan` support.

**Tech Stack:** C# 12 / .NET 8.0 (`net8.0`), P/Invoke (`System.Runtime.InteropServices`), `System.Text.Json`, xUnit (`dotnet test`), Node.js native staging (`scripts/stage-csharp-native.mjs`), Rust `lavi-c-cpp` (`cdylib`).

**Spec:** [`docs/superpowers/specs/2026-11-09-csharp-support-design.md`](../specs/2026-11-09-csharp-support-design.md)

## Global Constraints

- Do not modify the `#![no_std]` Rust core (`lavi`) or C ABI (`lavi-c-cpp`).
- `scripts/stage-csharp-native.mjs` must remove the transient `liblavi_c_cpp.{dylib,so,dll}` from `target/{debug,release}` after copying to `lavi-csharp/native/` so `clang` and `swift test` continue linking `liblavi_c_cpp.a` statically.
- Never pass unpinned instance delegates to C; keep static `[UnmanagedFunctionPointer(CallingConvention.Cdecl)]` delegate references alive in `static readonly` fields and free the per-instance `GCHandle` deterministically on `Dispose()`.
- Queries on an unstarted or disposed `Machine` must return safe defaults (`false`, `""`, `null` for `NextDeadline`, empty `MachineSnapshot`) without throwing exceptions.
- Synchronize `<Version>0.2.0</Version>` across `lavi-csharp/Lavi/Lavi.csproj` and `examples/csharp/StoplightExample.csproj` via `scripts/sync-version.mjs`.

## Review Focus

1. **Null or empty string inputs to `Machine`, `Send`, `Matches`, `Can`, `HasTag`, and `Value.Property`**: Passing `null` or `""` must not crash native code or throw `NullReferenceException`; return safe defaults (`false`, `""`, `Value.Null`).
2. **Repeated `Dispose()` / `Close()` calls on `Machine` and `SimulatedClock`**: Calling `Dispose()` or `Close()` multiple times must be idempotent and never double-free `MachineContext*`, `SimulatedClock*`, or `GCHandle`.
3. **Detaching `Machine` from `SimulatedClock` on `Dispose()`**: Disposing a `Machine` before `clock.Increment(...)` must call `fsm_free` so `SimulatedClock` detaches the machine and does not dereference a freed pointer.
4. **Non-integer floating-point numbers in `Value.AsInt32` / `TryGetInt32`**: `TryGetInt32` / `AsInt32` should truncate/convert finite numbers within `int` range and return `false` / `null` when `IsNumber` is false or the value is `NaN`/`Infinity`.
5. **`Send(object eventObject)` when the anonymous object lacks a `"type"` property**: Must safely no-op (matching unstarted/invalid JSON event behavior) rather than throwing an unhandled exception.

---

## Phase 1: Native Staging & P/Invoke Core (`lavi-csharp`) [flowos: 54b17ea6-4fb6-450a-8e9d-b3517744ff05]

### Task 1: Native Library Staging & P/Invoke Interop (`stage-csharp-native.mjs`, `NativeLoader.cs`, `NativeMethods.cs`, `Value.cs`, `SimulatedClock.cs`) [flowos: d08611f2-1c29-4955-8142-496acf1c3dd5]

- [ ] **Task 1: Native Library Staging & P/Invoke Interop (stage-csharp-native.mjs, NativeLoader.cs, NativeMethods.cs, Value.cs, SimulatedClock.cs) (~0.5d) [flowos: d08611f2-1c29-4955-8142-496acf1c3dd5]**

**Files:**
- Create: `scripts/stage-csharp-native.mjs`
- Create: `lavi-csharp/Lavi.sln`
- Create: `lavi-csharp/Lavi/Lavi.csproj`
- Create: `lavi-csharp/Lavi/NativeLoader.cs`
- Create: `lavi-csharp/Lavi/NativeMethods.cs`
- Create: `lavi-csharp/Lavi/Value.cs`
- Create: `lavi-csharp/Lavi/SimulatedClock.cs`
- Create: `lavi-csharp/Lavi.Tests/Lavi.Tests.csproj`
- Create: `lavi-csharp/Lavi.Tests/ClockAndValueTests.cs`

**Interfaces:**
- Consumes: C ABI in `lavi-c-cpp/include/lavi.h` (`fsm_*` symbols in `liblavi_c_cpp.{dylib,so,dll}`).
- Produces:
  - `Lavi.NativeLoader.EnsureLoaded()` (`internal static void`)
  - `Lavi.NativeMethods` (`internal static class` with P/Invoke signatures and `UnmanagedFunctionPointer(CallingConvention.Cdecl)` delegates)
  - `Lavi.Value` (`public readonly struct`):
    - `public static Value Null { get; }`
    - `public bool IsNull { get; }`, `IsObject`, `IsString`, `IsNumber`, `IsBool`
    - `public string? StringValue { get; }`, `public string? AsString { get; }`
    - `public double? NumberValue { get; }`, `public double? AsDouble { get; }`
    - `public int? AsInt32 { get; }`, `public long? AsInt64 { get; }`
    - `public bool? BoolValue { get; }`, `public bool? AsBool { get; }`
    - `public bool TryGetString(out string value)`, `TryGetDouble(out double value)`, `TryGetInt32(out int value)`, `TryGetInt64(out long value)`, `TryGetBoolean(out bool value)`
    - `public Value Property(string key)` and `public Value this[string key] { get; }`
  - `Lavi.SimulatedClock : IDisposable`:
    - `public SimulatedClock(ulong initialMs = 0)`
    - `internal IntPtr RawPointer { get; }`
    - `public ulong Now { get; }`
    - `public void Set(ulong targetMs)` and `public void Set(TimeSpan target)`
    - `public void Increment(ulong dtMs)` and `public void Increment(TimeSpan dt)`
    - `public void Close()` and `public void Dispose()`

- [ ] **Step 1: Ensure .NET 8 SDK is installed and stage the native library**

Run: `cargo rustc --package lavi-c-cpp --lib --crate-type cdylib && node scripts/stage-csharp-native.mjs`
Expected: `Staged C# P/Invoke native binary: ... -> .../lavi-csharp/native/liblavi_c_cpp.dylib` (and `target/debug/liblavi_c_cpp.dylib` is unlinked).

- [ ] **Step 2: Write the failing xUnit test in `lavi-csharp/Lavi.Tests/ClockAndValueTests.cs`**

```csharp
using System;
using Xunit;

namespace Lavi.Tests;

public class ClockAndValueTests
{
    [Fact]
    public void SimulatedClock_TracksTimeAndDisposesIdempotently()
    {
        using var clock = new SimulatedClock(100);
        Assert.Equal(100UL, clock.Now);

        clock.Increment(250);
        Assert.Equal(350UL, clock.Now);

        clock.Increment(TimeSpan.FromMilliseconds(150));
        Assert.Equal(500UL, clock.Now);

        clock.Set(TimeSpan.FromSeconds(2));
        Assert.Equal(2000UL, clock.Now);

        clock.Close();
        clock.Dispose();
        Assert.Equal(0UL, clock.Now);
    }

    [Fact]
    public void Value_Null_ReturnsSafeDefaults()
    {
        var v = Value.Null;
        Assert.True(v.IsNull);
        Assert.False(v.IsObject);
        Assert.False(v.IsString);
        Assert.False(v.IsNumber);
        Assert.False(v.IsBool);
        Assert.Null(v.AsString);
        Assert.Null(v.AsDouble);
        Assert.Null(v.AsInt32);
        Assert.Null(v.AsInt64);
        Assert.Null(v.AsBool);
        Assert.False(v.TryGetInt32(out _));
        Assert.True(v["missing"]["deep"].IsNull);
    }
}
```

- [ ] **Step 3: Implement `NativeLoader.cs`, `NativeMethods.cs`, `Value.cs`, and `SimulatedClock.cs`**

Use `ModuleInitializer` or static constructor in `NativeLoader` calling `NativeLibrary.SetDllImportResolver(typeof(NativeLoader).Assembly, Resolve)`. Read UTF-8 strings from `fsm_value_get_string` using the two-call length query (`fsm_value_get_string(ptr, IntPtr.Zero, UIntPtr.Zero)`).

- [ ] **Step 4: Run `dotnet test` to verify `ClockAndValueTests` passes**

Run: `dotnet test lavi-csharp/Lavi.sln`
Expected: PASS (2 tests passed)

- [ ] **Step 5: Commit Task 1**

```bash
git add scripts/stage-csharp-native.mjs lavi-csharp/ .gitignore
git commit -m "feat(csharp): add native staging and P/Invoke Value and SimulatedClock"
```

---

### Task 2: Idiomatic `Machine`, `MachineSnapshot`, and xUnit Test Suite (`MachineSnapshot.cs`, `Machine.cs`, `LaviTests.cs`) [flowos: d4dc3f79-c594-4672-bba3-84d0db61e907]

- [ ] **Task 2: Idiomatic Machine, MachineSnapshot, and xUnit Test Suite (MachineSnapshot.cs, Machine.cs, LaviTests.cs) (~0.5d) [flowos: d4dc3f79-c594-4672-bba3-84d0db61e907]**

**Files:**
- Create: `lavi-csharp/Lavi/MachineSnapshot.cs`
- Create: `lavi-csharp/Lavi/Machine.cs`
- Create: `lavi-csharp/Lavi.Tests/LaviTests.cs`

**Interfaces:**
- Consumes: `Lavi.NativeLoader`, `Lavi.NativeMethods`, `Lavi.Value`, `Lavi.SimulatedClock`.
- Produces:
  - `Lavi.MachineSnapshot` (`public sealed class`):
    - `public string Status { get; }`
    - `public string State { get; }`
    - `public JsonElement StateValue { get; }`
    - `public JsonElement Context { get; }`
    - `public JsonElement? Output { get; }`
    - `public IReadOnlyList<string> Tags { get; }`
    - `public bool Matches(string statePath)`
    - `public bool Can(string eventName)`
    - `public bool HasTag(string tag)`
    - `public string ToJson()`
    - `public static implicit operator string(MachineSnapshot snapshot)`
  - `Lavi.Machine : IDisposable` (`public sealed class`):
    - `public Machine(string jsonConfig, object? input = null, SimulatedClock? clock = null)`
    - `public static Machine CreateActor(string jsonConfig, object? input = null, SimulatedClock? clock = null)`
    - `public bool IsValid { get; }`, `public bool IsDone { get; }`, `public string State { get; }`, `public ulong? NextDeadline { get; }`
    - `public Machine Start()` and `public bool TryStart()`
    - `public void Stop()`, `public void Close()`, `public void Dispose()`
    - `public Machine RegisterAction(string name, Action<Value, Value> callback)`
    - `public Machine RegisterAction(string name, Action<Value, Value, Value> callback)` and `RegisterActionWithParams`
    - `public Machine RegisterGuard(string name, Func<Value, Value, bool> callback)`
    - `public Machine RegisterGuard(string name, Func<Value, Value, Value, bool> callback)` and `RegisterGuardWithParams`
    - `public Machine RegisterDelay(string name, Func<Value, Value, ulong> callback)` and `RegisterDelay(string name, Func<Value, Value, TimeSpan> callback)`
    - `public Machine RegisterClock(SimulatedClock clock)` and `RegisterClock(Func<ulong> callback)`
    - `public Machine RegisterInspector(Action<string> callback)` and `public event Action<string>? Inspected`
    - `public IDisposable Subscribe(Action<MachineSnapshot> observer)`
    - `public bool Matches(string statePath)`, `public bool Can(string eventName)`, `public bool HasTag(string tag)`
    - `public void Send(string eventName)`, `public void Send(string eventName, object? payload)`, `public void Send(object eventObject)`, `public void SendJson(string jsonEvent)`
    - `public void Tick()`
    - `public MachineSnapshot GetSnapshot()`, `public string GetSnapshotJson()`
    - `public bool RestoreSnapshot(string snapshotJson)`, `public bool RestoreSnapshot(MachineSnapshot snapshot)`
    - `public string GetOutput()`, `public string GetMeta()`, `public string GetStateMeta(string statePath)`

- [ ] **Step 1: Write the failing xUnit tests in `lavi-csharp/Lavi.Tests/LaviTests.cs`**

Cover all 6 spec scenarios plus the 5 Review Focus edge cases:
1. `UnstartedOrDisposed_ReturnsSafeDefaultsAndHandlesNullInputs`
2. `FluentRegistration_OverloadedActionsAndGuards_TimeSpanDelays_AndValueInspection`
3. `StructuredSnapshot_Subscribe_RestoreSnapshot_AndFinalOutput`
4. `CallbackReplacement_CustomClock_Tick_AndStop`
5. `InspectorCallback_AndInspectedEvent_ReceiveMicrostepAndSnapshotEvents`
6. `AnonymousObjectInput_AndAnonymousObjectSendPayloads`

- [ ] **Step 2: Run `dotnet test` to verify compilation failure before `Machine.cs` / `MachineSnapshot.cs` exist**

Run: `dotnet test lavi-csharp/Lavi.sln`
Expected: FAIL (`Machine` / `MachineSnapshot` not defined)

- [ ] **Step 3: Implement `MachineSnapshot.cs` and `Machine.cs`**

Allocate `GCHandle.Alloc(this, GCHandleType.Normal)` lazily when the native `MachineContext*` is created and free it in `Close()` / `Dispose()`. Keep static `readonly` fields for the 7 `[UnmanagedFunctionPointer(CallingConvention.Cdecl)]` trampoline delegates (`ActionThunk`, `ActionWithParamsThunk`, `GuardThunk`, `GuardWithParamsThunk`, `DelayThunk`, `ClockThunk`, `InspectThunk`) so the GC never collects them. When `Inspected`, `RegisterInspector`, or `Subscribe` is active, register the native inspector and dispatch `@xstate.snapshot` transitions to subscribers.

- [ ] **Step 4: Run `dotnet test` to verify all tests pass**

Run: `dotnet test lavi-csharp/Lavi.sln`
Expected: PASS (all tests in `ClockAndValueTests` and `LaviTests` pass)

- [ ] **Step 5: Commit Task 2**

```bash
git add lavi-csharp/
git commit -m "feat(csharp): implement Machine, MachineSnapshot, and xUnit test suite"
```

---

## Phase 2: Stoplight Example, Version Sync, CI & Documentation [flowos: e6e1ed13-b1a1-4c2b-b540-2c54b6665b24]

### Task 3: C# Stoplight Example, Version Sync & CI Wiring (`examples/csharp`, `package.json`, `sync-version.mjs`, `ci.yml`) [flowos: 70f28757-52f8-4f30-85ee-b7d6a5300be3]

- [ ] **Task 3: C# Stoplight Example, Version Sync & CI Wiring (examples/csharp, package.json, sync-version.mjs, ci.yml) (~0.25d) [flowos: 70f28757-52f8-4f30-85ee-b7d6a5300be3]**

**Files:**
- Create: `examples/csharp/StoplightExample.csproj`
- Create: `examples/csharp/Program.cs`
- Modify: `package.json`
- Modify: `scripts/sync-version.mjs`
- Modify: `tests/sync-version.test.ts`
- Modify: `.github/workflows/ci.yml`

**Interfaces:**
- Consumes: `Lavi` class library (`../../lavi-csharp/Lavi/Lavi.csproj`).
- Produces:
  - `npm run build:csharp`: `cargo rustc --package lavi-c-cpp --lib --crate-type cdylib && node scripts/stage-csharp-native.mjs && dotnet build lavi-csharp/Lavi.sln --nologo -v q`
  - `npm run test:csharp`: `npm run build:csharp && dotnet test lavi-csharp/Lavi.sln --no-build --nologo -v q`
  - `examples/csharp/Program.cs` printing `C# example passed (transitions: 4)`
  - `csproj-version` manifest support in `scripts/sync-version.mjs` covering `lavi-csharp/Lavi/Lavi.csproj` and `examples/csharp/StoplightExample.csproj`

- [ ] **Step 1: Add failing test for `csproj-version` in `tests/sync-version.test.ts`**

Verify `readAllManifestVersions` and `syncVersions` read and update `<Version>...</Version>` in `lavi-csharp/Lavi/Lavi.csproj` and `examples/csharp/StoplightExample.csproj`.

- [ ] **Step 2: Run `npx vitest run tests/sync-version.test.ts` to verify it fails**

Run: `npx vitest run tests/sync-version.test.ts`
Expected: FAIL

- [ ] **Step 3: Create `examples/csharp/StoplightExample.csproj` and `examples/csharp/Program.cs`, and update `scripts/sync-version.mjs`, `package.json`, and `.github/workflows/ci.yml`**

In `scripts/sync-version.mjs`, add `{ relPath: 'lavi-csharp/Lavi/Lavi.csproj', kind: 'csproj-version' }` and `{ relPath: 'examples/csharp/StoplightExample.csproj', kind: 'csproj-version' }`. Wire `build:csharp` and `test:csharp` into `package.json` (`build`, `test`, `build:examples`, `test:examples`) and add `actions/setup-dotnet@v4` (`dotnet-version: '8.0.x'`) to `.github/workflows/ci.yml`.

- [ ] **Step 4: Verify `sync-version`, `test:csharp`, and `dotnet run --project examples/csharp`**

Run: `npx vitest run tests/sync-version.test.ts && node scripts/sync-version.mjs --check && npm run test:csharp && dotnet run --project examples/csharp/StoplightExample.csproj`
Expected: All pass and `C# example passed (transitions: 4)` is printed.

- [ ] **Step 5: Commit Task 3**

```bash
git add examples/csharp/ package.json scripts/sync-version.mjs tests/sync-version.test.ts .github/workflows/ci.yml
git commit -m "feat(csharp): add stoplight example, version sync, and CI workflow integration"
```

---

### Task 4: Documentation Updates & Full Cross-Target Verification (`README.md`, `RELEASING.md`, `AGENTS.md`, `XSTATE_5_FEATURES.md`) [flowos: fb2238a6-f2db-4a67-b2af-d3a300ed1bbd]

- [ ] **Task 4: Documentation Updates & Full Cross-Target Verification (README.md, RELEASING.md, AGENTS.md, XSTATE_5_FEATURES.md) (~0.25d) [flowos: fb2238a6-f2db-4a67-b2af-d3a300ed1bbd]**

**Files:**
- Modify: `README.md`
- Modify: `RELEASING.md`
- Modify: `AGENTS.md`
- Modify: `PROCESS.md`
- Modify: `XSTATE_5_FEATURES.md`

**Interfaces:**
- Consumes: Completed `lavi-csharp` package, `examples/csharp`, and npm scripts.
- Produces: Updated architecture/feature tables, installation table, C# stoplight usage example in `README.md`, and full green `npm test` verification.

- [ ] **Step 1: Update `README.md`, `RELEASING.md`, `AGENTS.md`, `PROCESS.md`, and `XSTATE_5_FEATURES.md`**

Add `lavi-csharp` (`C# / .NET`) to the supported language lists, project structure, test commands (`npm run test:csharp`), installation table, usage examples section (showing fluent registration, `SimulatedClock`, `GetSnapshot()`, and `using var`), and comparison table (changing `(+C# planned)` to `C#`).

- [ ] **Step 2: Run `cargo clippy` and the full cross-target test suite (`npm test`)**

Run: `cargo clippy --workspace --all-targets -- -D warnings && npm test`
Expected: Zero clippy warnings and all suites (`test:conformance-check`, `test:native`, `test:wasm`, `test:e2e`, `test:c-cpp`, `test:py`, `test:jvm`, `test:swift`, `test:dart`, `test:csharp`, `test:examples`, `test:unit`) pass.

- [ ] **Step 3: Commit Task 4**

```bash
git add README.md RELEASING.md AGENTS.md PROCESS.md XSTATE_5_FEATURES.md
git commit -m "docs(csharp): document C# / .NET bindings across README, RELEASING, and AGENTS"
```

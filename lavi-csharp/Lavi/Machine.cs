using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text.Json;
using System.Text.Json.Nodes;

namespace Lavi;

/// <summary>
/// Idiomatic C# / .NET statechart actor runtime wrapping the native Lavi engine.
/// </summary>
public sealed class Machine : IDisposable
{
    private static readonly NativeMethods.NativeActionCallback ActionThunkDelegate = OnActionThunk;
    private static readonly NativeMethods.NativeActionWithParamsCallback ActionWithParamsThunkDelegate = OnActionWithParamsThunk;
    private static readonly NativeMethods.NativeGuardCallback GuardThunkDelegate = OnGuardThunk;
    private static readonly NativeMethods.NativeGuardWithParamsCallback GuardWithParamsThunkDelegate = OnGuardWithParamsThunk;
    private static readonly NativeMethods.NativeDelayCallback DelayThunkDelegate = OnDelayThunk;
    private static readonly NativeMethods.NativeClockWithDataCallback ClockThunkDelegate = OnClockThunk;
    private static readonly NativeMethods.NativeInspectCallback InspectThunkDelegate = OnInspectThunk;

    private readonly string _jsonConfig;
    private readonly string? _inputJson;
    private IntPtr _machinePtr;
    private GCHandle _selfHandle;
    private bool _closed;

    private readonly Dictionary<string, Action<Value, Value>> _actions = new(StringComparer.Ordinal);
    private readonly Dictionary<string, Action<Value, Value, Value>> _actionsWithParams = new(StringComparer.Ordinal);
    private readonly Dictionary<string, Func<Value, Value, bool>> _guards = new(StringComparer.Ordinal);
    private readonly Dictionary<string, Func<Value, Value, Value, bool>> _guardsWithParams = new(StringComparer.Ordinal);
    private readonly Dictionary<string, Func<Value, Value, ulong>> _delays = new(StringComparer.Ordinal);
    private Func<ulong>? _clockFn;
    private Action<string>? _inspectFn;
    private event Action<string>? _inspected;
    private readonly List<Action<MachineSnapshot>> _subscribers = new();
    private SimulatedClock? _simClock;

    public Machine(string jsonConfig, object? input = null, SimulatedClock? clock = null)
    {
        NativeLoader.EnsureLoaded();
        _jsonConfig = jsonConfig ?? string.Empty;
        _inputJson = SerializeInput(input);
        _simClock = clock;
    }

    ~Machine()
    {
        Close();
    }

    public static Machine CreateActor(string jsonConfig, object? input = null, SimulatedClock? clock = null) =>
        new(jsonConfig, input, clock);

    public bool IsValid => !_closed && _machinePtr != IntPtr.Zero;

    public bool IsDone => _machinePtr != IntPtr.Zero && NativeMethods.fsm_is_done(_machinePtr) == 1;

    public string State => _machinePtr == IntPtr.Zero
        ? string.Empty
        : NativeMethods.ReadBufferString((buf, len) => NativeMethods.fsm_get_state(_machinePtr, buf, len));

    public ulong? NextDeadline
    {
        get
        {
            if (_machinePtr == IntPtr.Zero)
            {
                return null;
            }

            return NativeMethods.fsm_get_next_deadline(_machinePtr, out var deadline)
                ? deadline
                : null;
        }
    }

    public event Action<string>? Inspected
    {
        add
        {
            if (_closed || value == null)
            {
                return;
            }
            _inspected += value;
            EnsureInspectorRegistered();
        }
        remove
        {
            _inspected -= value;
        }
    }

    private IntPtr SelfUserData
    {
        get
        {
            if (!_selfHandle.IsAllocated)
            {
                _selfHandle = GCHandle.Alloc(this, GCHandleType.Normal);
            }
            return GCHandle.ToIntPtr(_selfHandle);
        }
    }

    private static string? SerializeInput(object? input)
    {
        if (input is null)
        {
            return null;
        }

        if (input is string rawJson)
        {
            return rawJson;
        }

        return JsonSerializer.Serialize(input);
    }

    private IntPtr EnsureCreated()
    {
        if (_closed)
        {
            return IntPtr.Zero;
        }

        if (_machinePtr != IntPtr.Zero)
        {
            return _machinePtr;
        }

        if (string.IsNullOrWhiteSpace(_jsonConfig))
        {
            return IntPtr.Zero;
        }

        _machinePtr = _inputJson != null
            ? NativeMethods.fsm_create_with_input(_jsonConfig, _inputJson)
            : NativeMethods.fsm_create(_jsonConfig);

        if (_machinePtr != IntPtr.Zero)
        {
            BindRegisteredHandlers(_machinePtr);
        }

        return _machinePtr;
    }

    private void BindRegisteredHandlers(IntPtr ptr)
    {
        var userData = SelfUserData;
        foreach (var name in _actions.Keys)
        {
            NativeMethods.fsm_register_action(ptr, name, ActionThunkDelegate, userData);
        }

        foreach (var name in _actionsWithParams.Keys)
        {
            NativeMethods.fsm_register_action_with_params(ptr, name, ActionWithParamsThunkDelegate, userData);
        }

        foreach (var name in _guards.Keys)
        {
            NativeMethods.fsm_register_guard(ptr, name, GuardThunkDelegate, userData);
        }

        foreach (var name in _guardsWithParams.Keys)
        {
            NativeMethods.fsm_register_guard_with_params(ptr, name, GuardWithParamsThunkDelegate, userData);
        }

        foreach (var name in _delays.Keys)
        {
            NativeMethods.fsm_register_delay(ptr, name, DelayThunkDelegate, userData);
        }

        if (_simClock != null && _simClock.RawPointer != IntPtr.Zero)
        {
            NativeMethods.fsm_register_simulated_clock(ptr, _simClock.RawPointer);
        }
        else if (_clockFn != null)
        {
            NativeMethods.fsm_register_clock_with_data(ptr, ClockThunkDelegate, userData);
        }

        if (HasAnyInspector)
        {
            NativeMethods.fsm_register_inspector(ptr, InspectThunkDelegate, userData);
        }
    }

    private bool HasAnyInspector =>
        _inspectFn != null || _inspected != null || _subscribers.Count > 0;

    private void EnsureInspectorRegistered()
    {
        if (_machinePtr != IntPtr.Zero && HasAnyInspector)
        {
            NativeMethods.fsm_register_inspector(_machinePtr, InspectThunkDelegate, SelfUserData);
        }
    }

    public Machine Start()
    {
        TryStart();
        return this;
    }

    public bool TryStart()
    {
        if (_closed)
        {
            return false;
        }

        if (_machinePtr != IntPtr.Zero)
        {
            return true;
        }

        var ptr = EnsureCreated();
        if (ptr == IntPtr.Zero)
        {
            return false;
        }

        return NativeMethods.fsm_start(ptr) == 0;
    }

    public void Stop()
    {
        if (_machinePtr != IntPtr.Zero)
        {
            NativeMethods.fsm_stop(_machinePtr);
        }
    }

    public void Close()
    {
        if (_closed)
        {
            return;
        }

        _closed = true;
        var ptr = Interlocked.Exchange(ref _machinePtr, IntPtr.Zero);
        if (ptr != IntPtr.Zero)
        {
            NativeMethods.fsm_free(ptr);
        }

        if (_selfHandle.IsAllocated)
        {
            _selfHandle.Free();
        }

        _actions.Clear();
        _actionsWithParams.Clear();
        _guards.Clear();
        _guardsWithParams.Clear();
        _delays.Clear();
        _subscribers.Clear();
        _clockFn = null;
        _inspectFn = null;
        _inspected = null;
        _simClock = null;
        GC.SuppressFinalize(this);
    }

    public void Dispose() => Close();

    public Machine RegisterAction(string name, Action<Value, Value> callback)
    {
        if (_closed || string.IsNullOrEmpty(name) || callback == null)
        {
            return this;
        }

        _actions[name] = callback;
        _actionsWithParams.Remove(name);
        if (_machinePtr != IntPtr.Zero)
        {
            NativeMethods.fsm_register_action(_machinePtr, name, ActionThunkDelegate, SelfUserData);
        }

        return this;
    }

    public Machine RegisterAction(string name, Action<Value, Value, Value> callback)
    {
        if (_closed || string.IsNullOrEmpty(name) || callback == null)
        {
            return this;
        }

        _actionsWithParams[name] = callback;
        _actions.Remove(name);
        if (_machinePtr != IntPtr.Zero)
        {
            NativeMethods.fsm_register_action_with_params(_machinePtr, name, ActionWithParamsThunkDelegate, SelfUserData);
        }

        return this;
    }

    public Machine RegisterActionWithParams(string name, Action<Value, Value, Value> callback) =>
        RegisterAction(name, callback);

    public Machine RegisterGuard(string name, Func<Value, Value, bool> callback)
    {
        if (_closed || string.IsNullOrEmpty(name) || callback == null)
        {
            return this;
        }

        _guards[name] = callback;
        _guardsWithParams.Remove(name);
        if (_machinePtr != IntPtr.Zero)
        {
            NativeMethods.fsm_register_guard(_machinePtr, name, GuardThunkDelegate, SelfUserData);
        }

        return this;
    }

    public Machine RegisterGuard(string name, Func<Value, Value, Value, bool> callback)
    {
        if (_closed || string.IsNullOrEmpty(name) || callback == null)
        {
            return this;
        }

        _guardsWithParams[name] = callback;
        _guards.Remove(name);
        if (_machinePtr != IntPtr.Zero)
        {
            NativeMethods.fsm_register_guard_with_params(_machinePtr, name, GuardWithParamsThunkDelegate, SelfUserData);
        }

        return this;
    }

    public Machine RegisterGuardWithParams(string name, Func<Value, Value, Value, bool> callback) =>
        RegisterGuard(name, callback);

    public Machine RegisterDelay(string name, Func<Value, Value, ulong> callback)
    {
        if (_closed || string.IsNullOrEmpty(name) || callback == null)
        {
            return this;
        }

        _delays[name] = callback;
        if (_machinePtr != IntPtr.Zero)
        {
            NativeMethods.fsm_register_delay(_machinePtr, name, DelayThunkDelegate, SelfUserData);
        }

        return this;
    }

    public Machine RegisterDelay(string name, Func<Value, Value, TimeSpan> callback)
    {
        if (callback == null)
        {
            return this;
        }

        return RegisterDelay(name, (ctx, evt) =>
        {
            var span = callback(ctx, evt);
            var ms = span.TotalMilliseconds;
            return ms <= 0 ? 0UL : (ulong)ms;
        });
    }

    public Machine RegisterClock(Func<ulong> callback)
    {
        if (_closed || callback == null)
        {
            return this;
        }

        _clockFn = callback;
        _simClock = null;
        if (_machinePtr != IntPtr.Zero)
        {
            NativeMethods.fsm_register_clock_with_data(_machinePtr, ClockThunkDelegate, SelfUserData);
        }

        return this;
    }

    public Machine RegisterClock(SimulatedClock clock)
    {
        if (_closed || clock == null)
        {
            return this;
        }

        _simClock = clock;
        _clockFn = null;
        if (_machinePtr != IntPtr.Zero && clock.RawPointer != IntPtr.Zero)
        {
            NativeMethods.fsm_register_simulated_clock(_machinePtr, clock.RawPointer);
        }

        return this;
    }

    public Machine RegisterSimulatedClock(SimulatedClock clock) => RegisterClock(clock);

    public Machine RegisterInspector(Action<string> callback)
    {
        if (_closed || callback == null)
        {
            return this;
        }

        _inspectFn = callback;
        EnsureInspectorRegistered();
        return this;
    }

    public IDisposable Subscribe(Action<MachineSnapshot> observer)
    {
        if (_closed || observer == null)
        {
            return NoOpSubscription.Instance;
        }

        _subscribers.Add(observer);
        EnsureInspectorRegistered();
        return new Subscription(this, observer);
    }

    private void Unsubscribe(Action<MachineSnapshot> observer)
    {
        _subscribers.Remove(observer);
    }

    public bool Matches(string? statePath)
    {
        if (_machinePtr == IntPtr.Zero || string.IsNullOrEmpty(statePath))
        {
            return false;
        }

        return NativeMethods.fsm_matches(_machinePtr, statePath);
    }

    public bool Can(string? eventName)
    {
        if (_machinePtr == IntPtr.Zero || string.IsNullOrEmpty(eventName))
        {
            return false;
        }

        return NativeMethods.fsm_can(_machinePtr, eventName) == 1;
    }

    public bool HasTag(string? tag)
    {
        if (_machinePtr == IntPtr.Zero || string.IsNullOrEmpty(tag))
        {
            return false;
        }

        return NativeMethods.fsm_has_tag(_machinePtr, tag) == 1;
    }

    public void Send(string? eventName)
    {
        if (_machinePtr == IntPtr.Zero || string.IsNullOrEmpty(eventName))
        {
            return;
        }

        NativeMethods.fsm_send_event(_machinePtr, eventName);
    }

    public void Send(string? eventName, object? payload)
    {
        if (_machinePtr == IntPtr.Zero || string.IsNullOrEmpty(eventName))
        {
            return;
        }

        if (payload is null)
        {
            NativeMethods.fsm_send_event(_machinePtr, eventName);
            return;
        }

        var node = JsonSerializer.SerializeToNode(payload);
        if (node is JsonObject obj)
        {
            obj["type"] = eventName;
            SendJson(obj.ToJsonString());
        }
        else
        {
            NativeMethods.fsm_send_event(_machinePtr, eventName);
        }
    }

    public void Send(object? eventObject)
    {
        if (_machinePtr == IntPtr.Zero || eventObject is null)
        {
            return;
        }

        if (eventObject is string eventName)
        {
            Send(eventName);
            return;
        }

        var node = JsonSerializer.SerializeToNode(eventObject);
        if (node is JsonObject obj &&
            obj.TryGetPropertyValue("type", out var typeNode) &&
            typeNode is JsonValue typeVal &&
            typeVal.TryGetValue<string>(out var typeStr) &&
            !string.IsNullOrEmpty(typeStr))
        {
            SendJson(obj.ToJsonString());
        }
    }

    public void SendJson(string? jsonEvent)
    {
        if (_machinePtr == IntPtr.Zero || string.IsNullOrEmpty(jsonEvent))
        {
            return;
        }

        NativeMethods.fsm_send_json_event(_machinePtr, jsonEvent);
    }

    public void Tick()
    {
        if (_machinePtr != IntPtr.Zero)
        {
            NativeMethods.fsm_tick(_machinePtr);
        }
    }

    public string GetSnapshotJson() => _machinePtr == IntPtr.Zero
        ? string.Empty
        : NativeMethods.ReadBufferString((buf, len) => NativeMethods.fsm_get_snapshot_json(_machinePtr, buf, len));

    public MachineSnapshot GetSnapshot()
    {
        if (_machinePtr == IntPtr.Zero)
        {
            return MachineSnapshot.Unstarted;
        }

        var rawJson = GetSnapshotJson();
        if (string.IsNullOrEmpty(rawJson))
        {
            return MachineSnapshot.Unstarted;
        }

        return MachineSnapshot.FromJson(rawJson, State, GetOutput(), this);
    }

    public bool RestoreSnapshot(string? snapshotJson)
    {
        if (_closed || string.IsNullOrEmpty(snapshotJson))
        {
            return false;
        }

        var createdHere = _machinePtr == IntPtr.Zero;
        var ptr = EnsureCreated();
        if (ptr == IntPtr.Zero)
        {
            return false;
        }

        var rc = NativeMethods.fsm_restore_snapshot_json(ptr, snapshotJson);
        if (rc != 0)
        {
            if (createdHere)
            {
                NativeMethods.fsm_free(ptr);
                _machinePtr = IntPtr.Zero;
            }
            return false;
        }

        return true;
    }

    public bool RestoreSnapshot(MachineSnapshot? snapshot) =>
        snapshot != null && RestoreSnapshot(snapshot.ToJson());

    public string GetOutput() => _machinePtr == IntPtr.Zero
        ? string.Empty
        : NativeMethods.ReadBufferString((buf, len) => NativeMethods.fsm_get_output_json(_machinePtr, buf, len));

    public string GetMeta() => _machinePtr == IntPtr.Zero
        ? string.Empty
        : NativeMethods.ReadBufferString((buf, len) => NativeMethods.fsm_get_meta_json(_machinePtr, buf, len));

    public string GetStateMeta(string? statePath)
    {
        if (_machinePtr == IntPtr.Zero || statePath is null)
        {
            return string.Empty;
        }

        return NativeMethods.ReadBufferString(
            (buf, len) => NativeMethods.fsm_get_state_meta_json(_machinePtr, statePath, buf, len));
    }

    private static Machine? ResolveSelf(IntPtr userData)
    {
        if (userData == IntPtr.Zero)
        {
            return null;
        }

        var handle = GCHandle.FromIntPtr(userData);
        return handle.Target as Machine;
    }

    private static void OnActionThunk(IntPtr namePtr, IntPtr ctxPtr, IntPtr evtPtr, IntPtr userData)
    {
        var self = ResolveSelf(userData);
        var name = Marshal.PtrToStringUTF8(namePtr);
        if (self != null && name != null && self._actions.TryGetValue(name, out var cb))
        {
            cb(new Value(ctxPtr), new Value(evtPtr));
        }
    }

    private static void OnActionWithParamsThunk(
        IntPtr namePtr,
        IntPtr ctxPtr,
        IntPtr evtPtr,
        IntPtr paramsPtr,
        IntPtr userData)
    {
        var self = ResolveSelf(userData);
        var name = Marshal.PtrToStringUTF8(namePtr);
        if (self != null && name != null && self._actionsWithParams.TryGetValue(name, out var cb))
        {
            cb(new Value(ctxPtr), new Value(evtPtr), new Value(paramsPtr));
        }
    }

    private static bool OnGuardThunk(IntPtr namePtr, IntPtr ctxPtr, IntPtr evtPtr, IntPtr userData)
    {
        var self = ResolveSelf(userData);
        var name = Marshal.PtrToStringUTF8(namePtr);
        if (self != null && name != null && self._guards.TryGetValue(name, out var cb))
        {
            return cb(new Value(ctxPtr), new Value(evtPtr));
        }

        return false;
    }

    private static bool OnGuardWithParamsThunk(
        IntPtr namePtr,
        IntPtr ctxPtr,
        IntPtr evtPtr,
        IntPtr paramsPtr,
        IntPtr userData)
    {
        var self = ResolveSelf(userData);
        var name = Marshal.PtrToStringUTF8(namePtr);
        if (self != null && name != null && self._guardsWithParams.TryGetValue(name, out var cb))
        {
            return cb(new Value(ctxPtr), new Value(evtPtr), new Value(paramsPtr));
        }

        return false;
    }

    private static ulong OnDelayThunk(IntPtr namePtr, IntPtr ctxPtr, IntPtr evtPtr, IntPtr userData)
    {
        var self = ResolveSelf(userData);
        var name = Marshal.PtrToStringUTF8(namePtr);
        if (self != null && name != null && self._delays.TryGetValue(name, out var cb))
        {
            return cb(new Value(ctxPtr), new Value(evtPtr));
        }

        return 0UL;
    }

    private static ulong OnClockThunk(IntPtr userData)
    {
        var self = ResolveSelf(userData);
        return self?._clockFn?.Invoke() ?? 0UL;
    }

    private static void OnInspectThunk(IntPtr jsonPtr, IntPtr userData)
    {
        var self = ResolveSelf(userData);
        var json = Marshal.PtrToStringUTF8(jsonPtr);
        if (self == null || string.IsNullOrEmpty(json))
        {
            return;
        }

        self._inspectFn?.Invoke(json);
        self._inspected?.Invoke(json);

        if (self._subscribers.Count > 0 && json.Contains("\"@xstate.snapshot\"", StringComparison.Ordinal))
        {
            var snap = self.GetSnapshot();
            var snapshotListeners = self._subscribers.ToArray();
            foreach (var listener in snapshotListeners)
            {
                listener(snap);
            }
        }
    }

    private sealed class Subscription : IDisposable
    {
        private Machine? _machine;
        private Action<MachineSnapshot>? _observer;

        public Subscription(Machine machine, Action<MachineSnapshot> observer)
        {
            _machine = machine;
            _observer = observer;
        }

        public void Dispose()
        {
            var machine = Interlocked.Exchange(ref _machine, null);
            var observer = Interlocked.Exchange(ref _observer, null);
            if (machine != null && observer != null)
            {
                machine.Unsubscribe(observer);
            }
        }
    }

    private sealed class NoOpSubscription : IDisposable
    {
        public static readonly NoOpSubscription Instance = new();
        public void Dispose() { }
    }
}

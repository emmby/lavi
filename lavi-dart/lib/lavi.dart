library lavi;

import 'dart:convert';
import 'dart:ffi';
import 'package:ffi/ffi.dart';
import 'src/bindings.dart';
import 'src/loader.dart';

/// Read-only view of a Lavi JSON value (`context`, `event`, or `params`) passed into callbacks.
class Value {
  final Pointer<NativeFsmValue> _ptr;

  Value.fromRaw(this._ptr);

  bool get isNull => _ptr == nullptr;

  bool get isObject => !isNull && getBindings().fsmValueIsObject(_ptr);

  bool get isString => !isNull && getBindings().fsmValueIsString(_ptr);

  bool get isNumber => !isNull && getBindings().fsmValueIsNumber(_ptr);

  bool get isBool => !isNull && getBindings().fsmValueIsBool(_ptr);

  String? get stringValue {
    if (!isString) return null;
    final b = getBindings();
    final reqLen = b.fsmValueGetString(_ptr, nullptr, 0);
    if (reqLen == 0) return '';
    final buf = calloc<Uint8>(reqLen + 1);
    try {
      final copied = b.fsmValueGetString(_ptr, buf, reqLen + 1);
      if (copied <= 0) return null;
      return utf8.decode(buf.asTypedList(copied));
    } finally {
      calloc.free(buf);
    }
  }

  double? get numberValue {
    if (!isNumber) return null;
    return getBindings().fsmValueGetNumber(_ptr);
  }

  bool? get boolValue {
    if (!isBool) return null;
    return getBindings().fsmValueGetBool(_ptr);
  }

  Value property(String key) {
    if (isNull) return Value.fromRaw(nullptr);
    final cKey = key.toNativeUtf8();
    try {
      final propPtr = getBindings().fsmValueGetProperty(_ptr, cKey);
      return Value.fromRaw(propPtr);
    } finally {
      calloc.free(cKey);
    }
  }

  Value operator [](String key) => property(key);
}

typedef ActionCallback = void Function(Value context, Value event);
typedef ActionWithParamsCallback = void Function(
    Value context, Value event, Value params);
typedef GuardCallback = bool Function(Value context, Value event);
typedef GuardWithParamsCallback = bool Function(
    Value context, Value event, Value params);
typedef DelayCallback = int Function(Value context, Value event);
typedef ClockCallback = int Function();
typedef InspectCallback = void Function(String inspectJson);

/// Virtual clock modeled on XState 5's `SimulatedClock`.
class SimulatedClock {
  Pointer<NativeSimulatedClock> _ptr;

  SimulatedClock({int initialMs = 0})
      : _ptr = getBindings().fsmSimulatedClockCreate(initialMs);

  Pointer<NativeSimulatedClock> get rawPointer => _ptr;

  int get now {
    if (_ptr == nullptr) return 0;
    return getBindings().fsmSimulatedClockNow(_ptr);
  }

  void set(int targetMs) {
    if (_ptr == nullptr) return;
    getBindings().fsmSimulatedClockSet(_ptr, targetMs);
  }

  void increment(int dtMs) {
    if (_ptr == nullptr) return;
    getBindings().fsmSimulatedClockIncrement(_ptr, dtMs);
  }

  void close() {
    if (_ptr != nullptr) {
      getBindings().fsmSimulatedClockFree(_ptr);
      _ptr = nullptr;
    }
  }

  void dispose() => close();
}

/// Dart / Flutter wrapper for the Lavi statechart runtime.
class Machine {
  Pointer<NativeMachineContext> _machinePtr = nullptr;
  final String _jsonConfig;
  final String? _inputJson;
  bool _closed = false;

  final Map<String, ActionCallback> _actions = {};
  final Map<String, ActionWithParamsCallback> _actionsWithParams = {};
  final Map<String, GuardCallback> _guards = {};
  final Map<String, GuardWithParamsCallback> _guardsWithParams = {};
  final Map<String, DelayCallback> _delays = {};
  ClockCallback? _clockFn;
  InspectCallback? _inspectFn;
  SimulatedClock? _simClock;

  NativeCallable<NativeActionCallback>? _actionCallable;
  NativeCallable<NativeActionWithParamsCallback>? _actionWithParamsCallable;
  NativeCallable<NativeGuardCallback>? _guardCallable;
  NativeCallable<NativeGuardWithParamsCallback>? _guardWithParamsCallable;
  NativeCallable<NativeDelayCallback>? _delayCallable;
  NativeCallable<NativeClockWithDataCallback>? _clockCallable;
  NativeCallable<NativeInspectCallback>? _inspectCallable;

  Machine(this._jsonConfig, {String? inputJson, SimulatedClock? clock})
      : _inputJson = inputJson,
        _simClock = clock;

  bool get isValid => !_closed && _machinePtr != nullptr;

  NativeCallable<NativeActionCallback> _ensureActionCallable() {
    return _actionCallable ??=
        NativeCallable<NativeActionCallback>.isolateLocal(
      (Pointer<Utf8> namePtr, Pointer<NativeFsmValue> ctxPtr,
          Pointer<NativeFsmValue> evtPtr, Pointer<Void> _) {
        if (namePtr == nullptr) return;
        final name = namePtr.toDartString();
        final cb = _actions[name];
        if (cb != null) {
          cb(Value.fromRaw(ctxPtr), Value.fromRaw(evtPtr));
        }
      },
    );
  }

  NativeCallable<NativeActionWithParamsCallback>
      _ensureActionWithParamsCallable() {
    return _actionWithParamsCallable ??=
        NativeCallable<NativeActionWithParamsCallback>.isolateLocal(
      (Pointer<Utf8> namePtr,
          Pointer<NativeFsmValue> ctxPtr,
          Pointer<NativeFsmValue> evtPtr,
          Pointer<NativeFsmValue> paramsPtr,
          Pointer<Void> _) {
        if (namePtr == nullptr) return;
        final name = namePtr.toDartString();
        final cb = _actionsWithParams[name];
        if (cb != null) {
          cb(Value.fromRaw(ctxPtr), Value.fromRaw(evtPtr),
              Value.fromRaw(paramsPtr));
        }
      },
    );
  }

  NativeCallable<NativeGuardCallback> _ensureGuardCallable() {
    return _guardCallable ??= NativeCallable<NativeGuardCallback>.isolateLocal(
      (Pointer<Utf8> namePtr, Pointer<NativeFsmValue> ctxPtr,
          Pointer<NativeFsmValue> evtPtr, Pointer<Void> _) {
        if (namePtr == nullptr) return false;
        final name = namePtr.toDartString();
        final cb = _guards[name];
        if (cb != null) {
          return cb(Value.fromRaw(ctxPtr), Value.fromRaw(evtPtr));
        }
        return false;
      },
      exceptionalReturn: false,
    );
  }

  NativeCallable<NativeGuardWithParamsCallback>
      _ensureGuardWithParamsCallable() {
    return _guardWithParamsCallable ??=
        NativeCallable<NativeGuardWithParamsCallback>.isolateLocal(
      (Pointer<Utf8> namePtr,
          Pointer<NativeFsmValue> ctxPtr,
          Pointer<NativeFsmValue> evtPtr,
          Pointer<NativeFsmValue> paramsPtr,
          Pointer<Void> _) {
        if (namePtr == nullptr) return false;
        final name = namePtr.toDartString();
        final cb = _guardsWithParams[name];
        if (cb != null) {
          return cb(Value.fromRaw(ctxPtr), Value.fromRaw(evtPtr),
              Value.fromRaw(paramsPtr));
        }
        return false;
      },
      exceptionalReturn: false,
    );
  }

  NativeCallable<NativeDelayCallback> _ensureDelayCallable() {
    return _delayCallable ??= NativeCallable<NativeDelayCallback>.isolateLocal(
      (Pointer<Utf8> namePtr, Pointer<NativeFsmValue> ctxPtr,
          Pointer<NativeFsmValue> evtPtr, Pointer<Void> _) {
        if (namePtr == nullptr) return 0;
        final name = namePtr.toDartString();
        final cb = _delays[name];
        if (cb != null) {
          return cb(Value.fromRaw(ctxPtr), Value.fromRaw(evtPtr));
        }
        return 0;
      },
      exceptionalReturn: 0,
    );
  }

  NativeCallable<NativeClockWithDataCallback> _ensureClockCallable() {
    return _clockCallable ??=
        NativeCallable<NativeClockWithDataCallback>.isolateLocal(
      (Pointer<Void> _) {
        return _clockFn?.call() ?? 0;
      },
      exceptionalReturn: 0,
    );
  }

  NativeCallable<NativeInspectCallback> _ensureInspectCallable() {
    return _inspectCallable ??=
        NativeCallable<NativeInspectCallback>.isolateLocal(
      (Pointer<Utf8> jsonPtr, Pointer<Void> _) {
        if (jsonPtr == nullptr) return;
        _inspectFn?.call(jsonPtr.toDartString());
      },
    );
  }

  Pointer<NativeMachineContext> _ensureCreated() {
    if (_closed) return nullptr;
    if (_machinePtr != nullptr) return _machinePtr;

    final b = getBindings();
    final cJson = _jsonConfig.toNativeUtf8();
    Pointer<Utf8>? cInput;
    try {
      if (_inputJson != null) {
        cInput = _inputJson.toNativeUtf8();
        _machinePtr = b.fsmCreateWithInput(cJson, cInput);
      } else {
        _machinePtr = b.fsmCreate(cJson);
      }
    } finally {
      calloc.free(cJson);
      if (cInput != null) calloc.free(cInput);
    }

    if (_machinePtr != nullptr) {
      _bindRegisteredHandlers(_machinePtr);
    }
    return _machinePtr;
  }

  void _bindRegisteredHandlers(Pointer<NativeMachineContext> ptr) {
    final b = getBindings();
    for (final name in _actions.keys) {
      final cName = name.toNativeUtf8();
      try {
        b.fsmRegisterAction(
            ptr, cName, _ensureActionCallable().nativeFunction, nullptr);
      } finally {
        calloc.free(cName);
      }
    }
    for (final name in _actionsWithParams.keys) {
      final cName = name.toNativeUtf8();
      try {
        b.fsmRegisterActionWithParams(ptr, cName,
            _ensureActionWithParamsCallable().nativeFunction, nullptr);
      } finally {
        calloc.free(cName);
      }
    }
    for (final name in _guards.keys) {
      final cName = name.toNativeUtf8();
      try {
        b.fsmRegisterGuard(
            ptr, cName, _ensureGuardCallable().nativeFunction, nullptr);
      } finally {
        calloc.free(cName);
      }
    }
    for (final name in _guardsWithParams.keys) {
      final cName = name.toNativeUtf8();
      try {
        b.fsmRegisterGuardWithParams(ptr, cName,
            _ensureGuardWithParamsCallable().nativeFunction, nullptr);
      } finally {
        calloc.free(cName);
      }
    }
    for (final name in _delays.keys) {
      final cName = name.toNativeUtf8();
      try {
        b.fsmRegisterDelay(
            ptr, cName, _ensureDelayCallable().nativeFunction, nullptr);
      } finally {
        calloc.free(cName);
      }
    }
    final sim = _simClock;
    if (sim != null && sim.rawPointer != nullptr) {
      b.fsmRegisterSimulatedClock(ptr, sim.rawPointer);
    } else if (_clockFn != null) {
      b.fsmRegisterClockWithData(
          ptr, _ensureClockCallable().nativeFunction, nullptr);
    }
    if (_inspectFn != null) {
      b.fsmRegisterInspector(
          ptr, _ensureInspectCallable().nativeFunction, nullptr);
    }
  }

  bool start() {
    if (_closed) return false;
    if (_machinePtr != nullptr) return true;
    final ptr = _ensureCreated();
    if (ptr == nullptr) return false;
    return getBindings().fsmStart(ptr) == 0;
  }

  void stop() {
    if (_machinePtr != nullptr) {
      getBindings().fsmStop(_machinePtr);
    }
  }

  void close() {
    if (_closed) return;
    _closed = true;
    if (_machinePtr != nullptr) {
      getBindings().fsmFree(_machinePtr);
      _machinePtr = nullptr;
    }
    _actionCallable?.close();
    _actionCallable = null;
    _actionWithParamsCallable?.close();
    _actionWithParamsCallable = null;
    _guardCallable?.close();
    _guardCallable = null;
    _guardWithParamsCallable?.close();
    _guardWithParamsCallable = null;
    _delayCallable?.close();
    _delayCallable = null;
    _clockCallable?.close();
    _clockCallable = null;
    _inspectCallable?.close();
    _inspectCallable = null;

    _actions.clear();
    _actionsWithParams.clear();
    _guards.clear();
    _guardsWithParams.clear();
    _delays.clear();
    _clockFn = null;
    _inspectFn = null;
    _simClock = null;
  }

  void dispose() => close();

  void registerAction(String name, ActionCallback callback) {
    if (_closed) return;
    _actions[name] = callback;
    _actionsWithParams.remove(name);
    if (_machinePtr != nullptr) {
      final cName = name.toNativeUtf8();
      try {
        getBindings().fsmRegisterAction(_machinePtr, cName,
            _ensureActionCallable().nativeFunction, nullptr);
      } finally {
        calloc.free(cName);
      }
    }
  }

  void registerActionWithParams(
      String name, ActionWithParamsCallback callback) {
    if (_closed) return;
    _actionsWithParams[name] = callback;
    _actions.remove(name);
    if (_machinePtr != nullptr) {
      final cName = name.toNativeUtf8();
      try {
        getBindings().fsmRegisterActionWithParams(_machinePtr, cName,
            _ensureActionWithParamsCallable().nativeFunction, nullptr);
      } finally {
        calloc.free(cName);
      }
    }
  }

  void registerGuard(String name, GuardCallback callback) {
    if (_closed) return;
    _guards[name] = callback;
    _guardsWithParams.remove(name);
    if (_machinePtr != nullptr) {
      final cName = name.toNativeUtf8();
      try {
        getBindings().fsmRegisterGuard(
            _machinePtr, cName, _ensureGuardCallable().nativeFunction, nullptr);
      } finally {
        calloc.free(cName);
      }
    }
  }

  void registerGuardWithParams(String name, GuardWithParamsCallback callback) {
    if (_closed) return;
    _guardsWithParams[name] = callback;
    _guards.remove(name);
    if (_machinePtr != nullptr) {
      final cName = name.toNativeUtf8();
      try {
        getBindings().fsmRegisterGuardWithParams(_machinePtr, cName,
            _ensureGuardWithParamsCallable().nativeFunction, nullptr);
      } finally {
        calloc.free(cName);
      }
    }
  }

  void registerDelay(String name, DelayCallback callback) {
    if (_closed) return;
    _delays[name] = callback;
    if (_machinePtr != nullptr) {
      final cName = name.toNativeUtf8();
      try {
        getBindings().fsmRegisterDelay(
            _machinePtr, cName, _ensureDelayCallable().nativeFunction, nullptr);
      } finally {
        calloc.free(cName);
      }
    }
  }

  void registerClock(ClockCallback callback) {
    if (_closed) return;
    _clockFn = callback;
    _simClock = null;
    if (_machinePtr != nullptr) {
      getBindings().fsmRegisterClockWithData(
          _machinePtr, _ensureClockCallable().nativeFunction, nullptr);
    }
  }

  void registerSimulatedClock(SimulatedClock clock) {
    if (_closed) return;
    _simClock = clock;
    _clockFn = null;
    if (_machinePtr != nullptr && clock.rawPointer != nullptr) {
      getBindings().fsmRegisterSimulatedClock(_machinePtr, clock.rawPointer);
    }
  }

  void registerInspector(InspectCallback callback) {
    if (_closed) return;
    _inspectFn = callback;
    if (_machinePtr != nullptr) {
      getBindings().fsmRegisterInspector(
          _machinePtr, _ensureInspectCallable().nativeFunction, nullptr);
    }
  }

  bool matches(String statePath) {
    if (_machinePtr == nullptr) return false;
    final cPath = statePath.toNativeUtf8();
    try {
      return getBindings().fsmMatches(_machinePtr, cPath);
    } finally {
      calloc.free(cPath);
    }
  }

  bool can(String event) {
    if (_machinePtr == nullptr) return false;
    final cEvt = event.toNativeUtf8();
    try {
      return getBindings().fsmCan(_machinePtr, cEvt) == 1;
    } finally {
      calloc.free(cEvt);
    }
  }

  bool hasTag(String tag) {
    if (_machinePtr == nullptr) return false;
    final cTag = tag.toNativeUtf8();
    try {
      return getBindings().fsmHasTag(_machinePtr, cTag) == 1;
    } finally {
      calloc.free(cTag);
    }
  }

  bool get isDone {
    if (_machinePtr == nullptr) return false;
    return getBindings().fsmIsDone(_machinePtr) == 1;
  }

  void send(String event) {
    if (_machinePtr == nullptr) return;
    final cEvt = event.toNativeUtf8();
    try {
      getBindings().fsmSendEvent(_machinePtr, cEvt);
    } finally {
      calloc.free(cEvt);
    }
  }

  void sendJson(String jsonEvent) {
    if (_machinePtr == nullptr) return;
    final cJson = jsonEvent.toNativeUtf8();
    try {
      getBindings().fsmSendJsonEvent(_machinePtr, cJson);
    } finally {
      calloc.free(cJson);
    }
  }

  void tick() {
    if (_machinePtr == nullptr) return;
    getBindings().fsmTick(_machinePtr);
  }

  int? get nextDeadline {
    if (_machinePtr == nullptr) return null;
    final outPtr = calloc<Uint64>(1);
    try {
      if (getBindings().fsmGetNextDeadline(_machinePtr, outPtr)) {
        return outPtr.value;
      }
      return null;
    } finally {
      calloc.free(outPtr);
    }
  }

  String _readBufferString(
      int Function(Pointer<NativeMachineContext>, Pointer<Uint8>, int) query) {
    if (_machinePtr == nullptr) return '';
    final req = query(_machinePtr, nullptr, 0);
    if (req <= 1) return '';
    final buf = calloc<Uint8>(req);
    try {
      final written = query(_machinePtr, buf, req);
      if (written < 0) return '';
      return utf8.decode(buf.asTypedList(written));
    } finally {
      calloc.free(buf);
    }
  }

  String get state => _readBufferString(getBindings().fsmGetState);

  String getSnapshot() => _readBufferString(getBindings().fsmGetSnapshotJson);

  bool restoreSnapshot(String snapshotJson) {
    if (_closed) return false;
    final createdHere = (_machinePtr == nullptr);
    final ptr = _ensureCreated();
    if (ptr == nullptr) return false;

    final cSnap = snapshotJson.toNativeUtf8();
    final int rc;
    try {
      rc = getBindings().fsmRestoreSnapshotJson(ptr, cSnap);
    } finally {
      calloc.free(cSnap);
    }

    if (rc != 0) {
      if (createdHere) {
        getBindings().fsmFree(ptr);
        _machinePtr = nullptr;
      }
      return false;
    }
    return true;
  }

  String getOutput() => _readBufferString(getBindings().fsmGetOutputJson);

  String getMeta() => _readBufferString(getBindings().fsmGetMetaJson);

  String getStateMeta(String statePath) {
    if (_machinePtr == nullptr) return '';
    final cPath = statePath.toNativeUtf8();
    try {
      return _readBufferString(
        (ptr, buf, len) =>
            getBindings().fsmGetStateMetaJson(ptr, cPath, buf, len),
      );
    } finally {
      calloc.free(cPath);
    }
  }
}

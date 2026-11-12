import CLavi
import Foundation

/// Read-only view of a Lavi JSON value (`context`, `event`, or `params`) passed into callbacks.
public struct Value {
    internal let ptr: OpaquePointer?

    public init(raw: OpaquePointer?) {
        self.ptr = raw
    }

    public var isNull: Bool {
        return ptr == nil
    }

    public var isObject: Bool {
        guard let ptr = ptr else { return false }
        return fsm_value_is_object(ptr)
    }

    public var isString: Bool {
        guard let ptr = ptr else { return false }
        return fsm_value_is_string(ptr)
    }

    public var isNumber: Bool {
        guard let ptr = ptr else { return false }
        return fsm_value_is_number(ptr)
    }

    public var isBool: Bool {
        guard let ptr = ptr else { return false }
        return fsm_value_is_bool(ptr)
    }

    public var stringValue: String? {
        guard let ptr = ptr, fsm_value_is_string(ptr) else { return nil }
        let reqLen = fsm_value_get_string(ptr, nil, 0)
        if reqLen == 0 {
            return ""
        }
        var buffer = [CChar](repeating: 0, count: reqLen + 1)
        let copied = buffer.withUnsafeMutableBufferPointer { bufPtr in
            fsm_value_get_string(ptr, bufPtr.baseAddress, bufPtr.count)
        }
        guard copied > 0 else { return nil }
        return String(cString: buffer)
    }

    public var numberValue: Double? {
        guard let ptr = ptr, fsm_value_is_number(ptr) else { return nil }
        return fsm_value_get_number(ptr)
    }

    public var boolValue: Bool? {
        guard let ptr = ptr, fsm_value_is_bool(ptr) else { return nil }
        return fsm_value_get_bool(ptr)
    }

    public func property(_ key: String) -> Value {
        guard let ptr = ptr else { return Value(raw: nil) }
        let propPtr = key.withCString { cKey in
            fsm_value_get_property(ptr, cKey)
        }
        return Value(raw: propPtr)
    }

    public subscript(_ key: String) -> Value {
        return property(key)
    }
}

public typealias ActionCallback = (_ context: Value, _ event: Value) -> Void
public typealias ActionWithParamsCallback = (_ context: Value, _ event: Value, _ params: Value) -> Void
public typealias GuardCallback = (_ context: Value, _ event: Value) -> Bool
public typealias GuardWithParamsCallback = (_ context: Value, _ event: Value, _ params: Value) -> Bool
public typealias DelayCallback = (_ context: Value, _ event: Value) -> UInt64
public typealias OutputResolverCallback = (_ context: Value, _ event: Value) -> String
public typealias InvokeStartCallback = (_ id: String, _ src: String, _ input: Value, _ context: Value, _ event: Value) -> Void
public typealias InvokeStopCallback = (_ id: String) -> Void
public typealias ClockCallback = () -> UInt64
public typealias InspectCallback = (_ inspectJson: String) -> Void

/// Virtual clock modeled on XState 5's `SimulatedClock`.
public final class SimulatedClock {
    internal var ptr: OpaquePointer?

    public init(initialMs: UInt64 = 0) {
        self.ptr = fsm_simulated_clock_create(initialMs)
    }

    deinit {
        close()
    }

    public func close() {
        if let p = ptr {
            fsm_simulated_clock_free(p)
            ptr = nil
        }
    }

    public var now: UInt64 {
        guard let p = ptr else { return 0 }
        return fsm_simulated_clock_now(p)
    }

    public func set(_ targetMs: UInt64) {
        guard let p = ptr else { return }
        _ = fsm_simulated_clock_set(p, targetMs)
    }

    public func increment(_ dtMs: UInt64) {
        guard let p = ptr else { return }
        _ = fsm_simulated_clock_increment(p, dtMs)
    }
}

private let swiftActionThunk: @convention(c) (
    UnsafePointer<CChar>?,
    OpaquePointer?,
    OpaquePointer?,
    UnsafeMutableRawPointer?
) -> Void = { namePtr, ctxPtr, evtPtr, userData in
    guard let userData = userData, let namePtr = namePtr else { return }
    let machine = Unmanaged<Machine>.fromOpaque(userData).takeUnretainedValue()
    let name = String(cString: namePtr)
    if let cb = machine.actions[name] {
        cb(Value(raw: ctxPtr), Value(raw: evtPtr))
    }
}

private let swiftActionWithParamsThunk: @convention(c) (
    UnsafePointer<CChar>?,
    OpaquePointer?,
    OpaquePointer?,
    OpaquePointer?,
    UnsafeMutableRawPointer?
) -> Void = { namePtr, ctxPtr, evtPtr, paramsPtr, userData in
    guard let userData = userData, let namePtr = namePtr else { return }
    let machine = Unmanaged<Machine>.fromOpaque(userData).takeUnretainedValue()
    let name = String(cString: namePtr)
    if let cb = machine.actionsWithParams[name] {
        cb(Value(raw: ctxPtr), Value(raw: evtPtr), Value(raw: paramsPtr))
    }
}

private let swiftGuardThunk: @convention(c) (
    UnsafePointer<CChar>?,
    OpaquePointer?,
    OpaquePointer?,
    UnsafeMutableRawPointer?
) -> Bool = { namePtr, ctxPtr, evtPtr, userData in
    guard let userData = userData, let namePtr = namePtr else { return false }
    let machine = Unmanaged<Machine>.fromOpaque(userData).takeUnretainedValue()
    let name = String(cString: namePtr)
    if let cb = machine.guards[name] {
        return cb(Value(raw: ctxPtr), Value(raw: evtPtr))
    }
    return false
}

private let swiftGuardWithParamsThunk: @convention(c) (
    UnsafePointer<CChar>?,
    OpaquePointer?,
    OpaquePointer?,
    OpaquePointer?,
    UnsafeMutableRawPointer?
) -> Bool = { namePtr, ctxPtr, evtPtr, paramsPtr, userData in
    guard let userData = userData, let namePtr = namePtr else { return false }
    let machine = Unmanaged<Machine>.fromOpaque(userData).takeUnretainedValue()
    let name = String(cString: namePtr)
    if let cb = machine.guardsWithParams[name] {
        return cb(Value(raw: ctxPtr), Value(raw: evtPtr), Value(raw: paramsPtr))
    }
    return false
}

private let swiftDelayThunk: @convention(c) (
    UnsafePointer<CChar>?,
    OpaquePointer?,
    OpaquePointer?,
    UnsafeMutableRawPointer?
) -> UInt64 = { namePtr, ctxPtr, evtPtr, userData in
    guard let userData = userData, let namePtr = namePtr else { return 0 }
    let machine = Unmanaged<Machine>.fromOpaque(userData).takeUnretainedValue()
    let name = String(cString: namePtr)
    if let cb = machine.delays[name] {
        return cb(Value(raw: ctxPtr), Value(raw: evtPtr))
    }
    return 0
}

private let swiftOutputResolverThunk: @convention(c) (
    UnsafePointer<CChar>?,
    OpaquePointer?,
    OpaquePointer?,
    UnsafeMutablePointer<CChar>?,
    Int,
    UnsafeMutableRawPointer?
) -> Int32 = { statePathPtr, ctxPtr, evtPtr, outBuf, outBufLen, userData in
    guard let userData = userData, let statePathPtr = statePathPtr else { return 0 }
    let machine = Unmanaged<Machine>.fromOpaque(userData).takeUnretainedValue()
    let path = String(cString: statePathPtr)
    guard let cb = machine.outputResolvers[path],
          let outBuf = outBuf,
          outBufLen > 0 else {
        return 0
    }
    let json = cb(Value(raw: ctxPtr), Value(raw: evtPtr))
    let utf8 = Array(json.utf8)
    let copyLen = min(utf8.count, outBufLen - 1)
    for i in 0..<copyLen {
        outBuf[i] = CChar(bitPattern: utf8[i])
    }
    outBuf[copyLen] = 0
    return Int32(copyLen)
}

private let swiftInvokeStartThunk: @convention(c) (
    UnsafePointer<CChar>?,
    UnsafePointer<CChar>?,
    OpaquePointer?,
    OpaquePointer?,
    OpaquePointer?,
    UnsafeMutableRawPointer?
) -> Void = { idPtr, srcPtr, inputPtr, ctxPtr, evtPtr, userData in
    guard let userData = userData, let idPtr = idPtr, let srcPtr = srcPtr else { return }
    let machine = Unmanaged<Machine>.fromOpaque(userData).takeUnretainedValue()
    machine.invokeStartFn?(
        String(cString: idPtr),
        String(cString: srcPtr),
        Value(raw: inputPtr),
        Value(raw: ctxPtr),
        Value(raw: evtPtr)
    )
}

private let swiftInvokeStopThunk: @convention(c) (
    UnsafePointer<CChar>?,
    UnsafeMutableRawPointer?
) -> Void = { idPtr, userData in
    guard let userData = userData, let idPtr = idPtr else { return }
    let machine = Unmanaged<Machine>.fromOpaque(userData).takeUnretainedValue()
    machine.invokeStopFn?(String(cString: idPtr))
}

private let swiftClockThunk: @convention(c) (
    UnsafeMutableRawPointer?
) -> UInt64 = { userData in
    guard let userData = userData else { return 0 }
    let machine = Unmanaged<Machine>.fromOpaque(userData).takeUnretainedValue()
    return machine.clockFn?() ?? 0
}

private let swiftInspectThunk: @convention(c) (
    UnsafePointer<CChar>?,
    UnsafeMutableRawPointer?
) -> Void = { jsonPtr, userData in
    guard let userData = userData, let jsonPtr = jsonPtr else { return }
    let machine = Unmanaged<Machine>.fromOpaque(userData).takeUnretainedValue()
    machine.inspectFn?(String(cString: jsonPtr))
}

/// Swift wrapper for the Lavi statechart runtime.
public final class Machine {
    private var machinePtr: OpaquePointer?
    private let jsonConfig: String
    private let inputJson: String?
    private var started: Bool = false
    private var closed: Bool = false

    internal var actions: [String: ActionCallback] = [:]
    internal var actionsWithParams: [String: ActionWithParamsCallback] = [:]
    internal var guards: [String: GuardCallback] = [:]
    internal var guardsWithParams: [String: GuardWithParamsCallback] = [:]
    internal var delays: [String: DelayCallback] = [:]
    internal var outputResolvers: [String: OutputResolverCallback] = [:]
    internal var invokeStartFn: InvokeStartCallback?
    internal var invokeStopFn: InvokeStopCallback?
    internal var clockFn: ClockCallback?
    internal var inspectFn: InspectCallback?
    private var simClock: SimulatedClock?

    public init(jsonConfig: String, inputJson: String? = nil, clock: SimulatedClock? = nil) {
        self.jsonConfig = jsonConfig
        self.inputJson = inputJson
        self.simClock = clock
    }

    deinit {
        close()
    }

    public func close() {
        closed = true
        started = false
        if let ptr = machinePtr {
            fsm_free(ptr)
            machinePtr = nil
        }
        actions.removeAll()
        actionsWithParams.removeAll()
        guards.removeAll()
        guardsWithParams.removeAll()
        delays.removeAll()
        outputResolvers.removeAll()
        invokeStartFn = nil
        invokeStopFn = nil
        clockFn = nil
        inspectFn = nil
        simClock = nil
    }

    public var isValid: Bool {
        return !closed && started && machinePtr != nil
    }

    private var selfOpaque: UnsafeMutableRawPointer {
        return Unmanaged.passUnretained(self).toOpaque()
    }

    private func ensureCreated() -> OpaquePointer? {
        if closed { return nil }
        if let existing = machinePtr { return existing }

        let created: OpaquePointer?
        if let inp = inputJson {
            created = jsonConfig.withCString { cJson in
                inp.withCString { cInput in
                    fsm_create_with_input(cJson, cInput)
                }
            }
        } else {
            created = jsonConfig.withCString { cJson in
                fsm_create(cJson)
            }
        }

        guard let ptr = created else { return nil }
        machinePtr = ptr
        bindRegisteredHandlers(ptr)
        return ptr
    }

    private func bindRegisteredHandlers(_ ptr: OpaquePointer) {
        let userData = selfOpaque
        for name in actions.keys {
            name.withCString { cName in
                _ = fsm_register_action(ptr, cName, swiftActionThunk, userData)
            }
        }
        for name in actionsWithParams.keys {
            name.withCString { cName in
                _ = fsm_register_action_with_params(ptr, cName, swiftActionWithParamsThunk, userData)
            }
        }
        for name in guards.keys {
            name.withCString { cName in
                _ = fsm_register_guard(ptr, cName, swiftGuardThunk, userData)
            }
        }
        for name in guardsWithParams.keys {
            name.withCString { cName in
                _ = fsm_register_guard_with_params(ptr, cName, swiftGuardWithParamsThunk, userData)
            }
        }
        for name in delays.keys {
            name.withCString { cName in
                _ = fsm_register_delay(ptr, cName, swiftDelayThunk, userData)
            }
        }
        for path in outputResolvers.keys {
            path.withCString { cPath in
                _ = fsm_register_output_resolver(ptr, cPath, swiftOutputResolverThunk, userData)
            }
        }
        if invokeStartFn != nil || invokeStopFn != nil {
            _ = fsm_register_invoke_callbacks(
                ptr,
                invokeStartFn != nil ? swiftInvokeStartThunk : nil,
                invokeStopFn != nil ? swiftInvokeStopThunk : nil,
                userData
            )
        }
        if let simPtr = simClock?.ptr {
            _ = fsm_register_simulated_clock(ptr, simPtr)
        } else if clockFn != nil {
            _ = fsm_register_clock_with_data(ptr, swiftClockThunk, userData)
        }
        if inspectFn != nil {
            _ = fsm_register_inspector(ptr, swiftInspectThunk, userData)
        }
    }

    @discardableResult
    public func start() -> Bool {
        if closed { return false }
        if started && machinePtr != nil { return true }
        guard let ptr = ensureCreated() else { return false }
        if fsm_start(ptr) == 0 {
            started = true
            return true
        }
        return false
    }

    public func stop() {
        guard let ptr = machinePtr else { return }
        _ = fsm_stop(ptr)
    }

    public func registerAction(_ name: String, callback: @escaping ActionCallback) {
        if closed { return }
        actions[name] = callback
        actionsWithParams.removeValue(forKey: name)
        if let ptr = machinePtr {
            name.withCString { cName in
                _ = fsm_register_action(ptr, cName, swiftActionThunk, selfOpaque)
            }
        }
    }

    public func registerActionWithParams(_ name: String, callback: @escaping ActionWithParamsCallback) {
        if closed { return }
        actionsWithParams[name] = callback
        actions.removeValue(forKey: name)
        if let ptr = machinePtr {
            name.withCString { cName in
                _ = fsm_register_action_with_params(ptr, cName, swiftActionWithParamsThunk, selfOpaque)
            }
        }
    }

    public func registerGuard(_ name: String, callback: @escaping GuardCallback) {
        if closed { return }
        guards[name] = callback
        guardsWithParams.removeValue(forKey: name)
        if let ptr = machinePtr {
            name.withCString { cName in
                _ = fsm_register_guard(ptr, cName, swiftGuardThunk, selfOpaque)
            }
        }
    }

    public func registerGuardWithParams(_ name: String, callback: @escaping GuardWithParamsCallback) {
        if closed { return }
        guardsWithParams[name] = callback
        guards.removeValue(forKey: name)
        if let ptr = machinePtr {
            name.withCString { cName in
                _ = fsm_register_guard_with_params(ptr, cName, swiftGuardWithParamsThunk, selfOpaque)
            }
        }
    }

    public func registerDelay(_ name: String, callback: @escaping DelayCallback) {
        if closed { return }
        delays[name] = callback
        if let ptr = machinePtr {
            name.withCString { cName in
                _ = fsm_register_delay(ptr, cName, swiftDelayThunk, selfOpaque)
            }
        }
    }

    public func registerOutputResolver(_ statePath: String = "", callback: @escaping OutputResolverCallback) {
        if closed { return }
        outputResolvers[statePath] = callback
        if let ptr = machinePtr {
            statePath.withCString { cPath in
                _ = fsm_register_output_resolver(ptr, cPath, swiftOutputResolverThunk, selfOpaque)
            }
        }
    }

    public func registerInvokeCallbacks(onStart: InvokeStartCallback? = nil, onStop: InvokeStopCallback? = nil) {
        if closed { return }
        invokeStartFn = onStart
        invokeStopFn = onStop
        if let ptr = machinePtr {
            _ = fsm_register_invoke_callbacks(
                ptr,
                onStart != nil ? swiftInvokeStartThunk : nil,
                onStop != nil ? swiftInvokeStopThunk : nil,
                selfOpaque
            )
        }
    }

    public func registerClock(_ callback: @escaping ClockCallback) {
        if closed { return }
        clockFn = callback
        simClock = nil
        if let ptr = machinePtr {
            _ = fsm_register_clock_with_data(ptr, swiftClockThunk, selfOpaque)
        }
    }

    public func registerClock(_ clock: SimulatedClock) {
        if closed { return }
        simClock = clock
        clockFn = nil
        if let ptr = machinePtr, let simPtr = clock.ptr {
            _ = fsm_register_simulated_clock(ptr, simPtr)
        }
    }

    public func registerInspector(_ callback: @escaping InspectCallback) {
        if closed { return }
        inspectFn = callback
        if let ptr = machinePtr {
            _ = fsm_register_inspector(ptr, swiftInspectThunk, selfOpaque)
        }
    }

    public func matches(_ statePath: String) -> Bool {
        guard let ptr = machinePtr else { return false }
        return statePath.withCString { cPath in
            fsm_matches(ptr, cPath)
        }
    }

    public func can(_ event: String) -> Bool {
        guard let ptr = machinePtr else { return false }
        return event.withCString { cEvt in
            fsm_can(ptr, cEvt) == 1
        }
    }

    public func hasTag(_ tag: String) -> Bool {
        guard let ptr = machinePtr else { return false }
        return tag.withCString { cTag in
            fsm_has_tag(ptr, cTag) == 1
        }
    }

    public var isDone: Bool {
        guard let ptr = machinePtr else { return false }
        return fsm_is_done(ptr) == 1
    }

    public func send(_ event: String) {
        guard let ptr = machinePtr else { return }
        event.withCString { cEvt in
            _ = fsm_send_event(ptr, cEvt)
        }
    }

    public func sendJson(_ jsonEvent: String) {
        guard let ptr = machinePtr else { return }
        jsonEvent.withCString { cJson in
            _ = fsm_send_json_event(ptr, cJson)
        }
    }

    public func tick() {
        guard let ptr = machinePtr else { return }
        _ = fsm_tick(ptr)
    }

    public var nextDeadline: UInt64? {
        guard let ptr = machinePtr else { return nil }
        var out: UInt64 = 0
        if fsm_get_next_deadline(ptr, &out) {
            return out
        }
        return nil
    }

    private func readBufferString(_ query: (OpaquePointer, UnsafeMutablePointer<CChar>?, Int) -> Int32) -> String {
        guard let ptr = machinePtr else { return "" }
        let req = query(ptr, nil, 0)
        if req <= 1 { return "" }
        var buffer = [CChar](repeating: 0, count: Int(req))
        let written = buffer.withUnsafeMutableBufferPointer { bufPtr in
            query(ptr, bufPtr.baseAddress, bufPtr.count)
        }
        guard written >= 0 else { return "" }
        return String(cString: buffer)
    }

    public var state: String {
        return readBufferString { ptr, buf, len in
            fsm_get_state(ptr, buf, len)
        }
    }

    public func getSnapshot() -> String {
        return readBufferString { ptr, buf, len in
            fsm_get_snapshot_json(ptr, buf, len)
        }
    }

    @discardableResult
    public func restoreSnapshot(_ snapshotJson: String) -> Bool {
        if closed { return false }
        let createdHere = (machinePtr == nil)
        guard let ptr = ensureCreated() else { return false }
        let rc = snapshotJson.withCString { cSnap in
            fsm_restore_snapshot_json(ptr, cSnap)
        }
        if rc != 0 {
            if createdHere {
                fsm_free(ptr)
                machinePtr = nil
            }
            return false
        }
        started = true
        return true
    }

    public func getOutput() -> String {
        return readBufferString { ptr, buf, len in
            fsm_get_output_json(ptr, buf, len)
        }
    }

    public func getFinalLeafOutput() -> String {
        return readBufferString { ptr, buf, len in
            fsm_get_final_leaf_output_json(ptr, buf, len)
        }
    }

    public func initialTransitionJson(inputJson: String? = nil) -> String {
        guard ensureCreated() != nil else { return "" }
        if let inp = inputJson {
            return inp.withCString { cInp in
                readBufferString { p, buf, len in
                    fsm_initial_transition_json(p, cInp, buf, len)
                }
            }
        } else {
            return readBufferString { p, buf, len in
                fsm_initial_transition_json(p, nil, buf, len)
            }
        }
    }

    public func transitionPureJson(snapshotJson: String, eventJson: String) -> String {
        guard ensureCreated() != nil else { return "" }
        return snapshotJson.withCString { cSnap in
            eventJson.withCString { cEvt in
                readBufferString { p, buf, len in
                    fsm_transition_pure_json(p, cSnap, cEvt, buf, len)
                }
            }
        }
    }

    public func getMeta() -> String {
        return readBufferString { ptr, buf, len in
            fsm_get_meta_json(ptr, buf, len)
        }
    }

    public func getStateMeta(_ statePath: String) -> String {
        return statePath.withCString { cPath in
            readBufferString { ptr, buf, len in
                fsm_get_state_meta_json(ptr, cPath, buf, len)
            }
        }
    }
}

package io.lavi;

import java.util.HashMap;
import java.util.Map;
import java.util.OptionalLong;

public final class Machine implements AutoCloseable {
    private static class NativeState implements Runnable {
        private long nativeHandle;

        NativeState(long nativeHandle) {
            this.nativeHandle = nativeHandle;
        }

        @Override
        public void run() {
            if (nativeHandle != 0) {
                nativeFree(nativeHandle);
                nativeHandle = 0;
            }
        }
    }

    private long nativeHandle;
    private final NativeState state;
    private final NativeCleaner.Cleanable cleanable;
    private final Map<String, ActionCallback> actions = new HashMap<String, ActionCallback>();
    private final Map<String, ParameterizedActionCallback> parameterizedActions = new HashMap<String, ParameterizedActionCallback>();
    private final Map<String, GuardCallback> guards = new HashMap<String, GuardCallback>();
    private final Map<String, ParameterizedGuardCallback> parameterizedGuards = new HashMap<String, ParameterizedGuardCallback>();
    private final Map<String, OutputResolverCallback> outputResolvers = new HashMap<String, OutputResolverCallback>();
    private InvokeStartCallback invokeStartCallback;
    private InvokeStopCallback invokeStopCallback;
    private ClockCallback clockCallback;
    private InspectCallback inspectCallback;
    private SimulatedClock simulatedClock;
    private volatile boolean closed = false;

    public boolean isClosed() {
        return closed;
    }

    public Machine(String jsonConfig) {
        this(jsonConfig, null);
    }

    public Machine(String jsonConfig, SimulatedClock clock) {
        if (jsonConfig == null) {
            throw new IllegalArgumentException("Machine jsonConfig cannot be null");
        }
        NativeLoader.load();
        long clockHandle = clock != null ? clock.getNativeHandle() : 0L;
        this.nativeHandle = nativeCreate(jsonConfig, clockHandle);
        this.state = new NativeState(nativeHandle);
        this.cleanable = NativeCleaner.register(this, state);

        if (clock != null) {
            registerClock(clock);
        }
    }

    public synchronized Machine registerAction(String name, ActionCallback cb) {
        checkClosed();
        if (name == null || cb == null) {
            throw new IllegalArgumentException("Action name and callback cannot be null");
        }
        parameterizedActions.remove(name);
        actions.put(name, cb);
        nativeRegisterAction(nativeHandle, name);
        return this;
    }

    public synchronized Machine registerAction(String name, ParameterizedActionCallback cb) {
        checkClosed();
        if (name == null || cb == null) {
            throw new IllegalArgumentException("Action name and callback cannot be null");
        }
        actions.remove(name);
        parameterizedActions.put(name, cb);
        nativeRegisterAction(nativeHandle, name);
        return this;
    }

    public synchronized Machine registerGuard(String name, GuardCallback cb) {
        checkClosed();
        if (name == null || cb == null) {
            throw new IllegalArgumentException("Guard name and callback cannot be null");
        }
        parameterizedGuards.remove(name);
        guards.put(name, cb);
        nativeRegisterGuard(nativeHandle, name);
        return this;
    }

    public synchronized Machine registerGuard(String name, ParameterizedGuardCallback cb) {
        checkClosed();
        if (name == null || cb == null) {
            throw new IllegalArgumentException("Guard name and callback cannot be null");
        }
        guards.remove(name);
        parameterizedGuards.put(name, cb);
        nativeRegisterGuard(nativeHandle, name);
        return this;
    }

    public synchronized Machine registerOutputResolver(String statePath, OutputResolverCallback cb) {
        checkClosed();
        if (statePath == null || cb == null) {
            throw new IllegalArgumentException("statePath and callback cannot be null");
        }
        outputResolvers.put(statePath, cb);
        nativeRegisterOutputResolver(nativeHandle, statePath);
        return this;
    }

    public synchronized Machine registerInvokeCallbacks(InvokeStartCallback onStart, InvokeStopCallback onStop) {
        checkClosed();
        this.invokeStartCallback = onStart;
        this.invokeStopCallback = onStop;
        nativeRegisterInvokeCallbacks(nativeHandle, onStart != null, onStop != null);
        return this;
    }

    public synchronized Machine registerClock(ClockCallback cb) {
        checkClosed();
        if (cb == null) {
            throw new IllegalArgumentException("ClockCallback cannot be null");
        }
        if (this.simulatedClock != null) {
            this.simulatedClock.unregisterMachine(this);
            this.simulatedClock = null;
        }
        this.clockCallback = cb;
        nativeRegisterClockCallback(nativeHandle);
        return this;
    }

    public synchronized Machine registerClock(SimulatedClock clock) {
        checkClosed();
        if (clock == null) {
            throw new IllegalArgumentException("SimulatedClock cannot be null");
        }
        if (this.simulatedClock == clock) {
            return this; // Deduplicate registration
        }
        if (this.simulatedClock != null) {
            this.simulatedClock.unregisterMachine(this);
        }
        this.simulatedClock = clock;
        this.clockCallback = null;
        clock.registerMachine(this);
        nativeRegisterSimulatedClock(nativeHandle, clock.getNativeHandle());
        return this;
    }

    public synchronized Machine registerInspector(InspectCallback cb) {
        checkClosed();
        if (cb == null) {
            throw new IllegalArgumentException("InspectCallback cannot be null");
        }
        this.inspectCallback = cb;
        nativeRegisterInspector(nativeHandle);
        return this;
    }

    public synchronized Machine start() {
        checkClosed();
        nativeStart(nativeHandle);
        return this;
    }

    public synchronized void stop() {
        checkClosed();
        nativeStop(nativeHandle);
    }

    @Override
    public synchronized void close() {
        if (closed) return;
        closed = true;
        cleanable.clean();
        nativeHandle = 0L;
        if (simulatedClock != null) {
            simulatedClock.unregisterMachine(this);
            simulatedClock = null;
        }
        actions.clear();
        parameterizedActions.clear();
        guards.clear();
        parameterizedGuards.clear();
        outputResolvers.clear();
        invokeStartCallback = null;
        invokeStopCallback = null;
        clockCallback = null;
        inspectCallback = null;
    }

    public synchronized Value getContext() {
        checkClosed();
        long handle = nativeGetContext(nativeHandle);
        return handle != 0L ? Value.fromNativeHandle(handle) : Value.NULL;
    }

    public synchronized Value getOutput() {
        checkClosed();
        long handle = nativeGetOutput(nativeHandle);
        return handle != 0L ? Value.fromNativeHandle(handle) : Value.NULL;
    }

    public synchronized String getOutputJson() {
        Value out = getOutput();
        return out.isNull() ? null : out.toJson();
    }

    public synchronized Value getFinalLeafOutput() {
        checkClosed();
        long handle = nativeGetFinalLeafOutput(nativeHandle);
        return handle != 0L ? Value.fromNativeHandle(handle) : Value.NULL;
    }

    public synchronized String getFinalLeafOutputJson() {
        Value out = getFinalLeafOutput();
        return out.isNull() ? null : out.toJson();
    }

    public synchronized String initialTransitionJson() {
        return initialTransitionJson(null);
    }

    public synchronized String initialTransitionJson(String inputJson) {
        checkClosed();
        return nativeInitialTransitionJson(nativeHandle, inputJson);
    }

    public synchronized Value initialTransition() {
        return Value.fromJson(initialTransitionJson(null));
    }

    public synchronized Value initialTransition(Value input) {
        String inpJson = (input == null || input.isNull()) ? null : input.toJson();
        return Value.fromJson(initialTransitionJson(inpJson));
    }

    public synchronized String transitionPureJson(String snapshotOrStateJson, String eventJson) {
        checkClosed();
        if (snapshotOrStateJson == null || eventJson == null) {
            throw new IllegalArgumentException("snapshotOrStateJson and eventJson cannot be null");
        }
        return nativeTransitionPureJson(nativeHandle, snapshotOrStateJson, eventJson);
    }

    public synchronized Value transitionPure(String snapshotOrStateJson, String eventJson) {
        return Value.fromJson(transitionPureJson(snapshotOrStateJson, eventJson));
    }

    public synchronized boolean matches(String statePath) {
        checkClosed();
        if (statePath == null) return false;
        return nativeMatches(nativeHandle, statePath);
    }

    public synchronized boolean can(String event) {
        return can(event, null);
    }

    public synchronized boolean can(String event, String payloadJson) {
        checkClosed();
        if (event == null) return false;
        return nativeCan(nativeHandle, event, payloadJson);
    }

    public synchronized boolean hasTag(String tag) {
        checkClosed();
        if (tag == null) return false;
        return nativeHasTag(nativeHandle, tag);
    }

    public synchronized boolean isDone() {
        checkClosed();
        return nativeIsDone(nativeHandle);
    }

    public synchronized void send(String event) {
        send(event, null);
    }

    public synchronized void send(String event, String payloadJson) {
        checkClosed();
        if (event == null) return;
        nativeSend(nativeHandle, event, payloadJson);
    }

    public synchronized void tick() {
        checkClosed();
        nativeTick(nativeHandle);
    }

    public synchronized OptionalLong getNextDeadline() {
        checkClosed();
        long d = nativeGetNextDeadline(nativeHandle);
        return d >= 0 ? OptionalLong.of(d) : OptionalLong.empty();
    }

    public synchronized String getSnapshot() {
        checkClosed();
        return nativeGetSnapshot(nativeHandle);
    }

    public synchronized void restoreSnapshot(String snapshotJson) {
        checkClosed();
        if (snapshotJson == null) {
            throw new IllegalArgumentException("snapshotJson cannot be null");
        }
        nativeRestoreSnapshot(nativeHandle, snapshotJson);
    }

    long getNativeHandle() {
        checkClosed();
        return nativeHandle;
    }

    private void checkClosed() {
        if (closed) {
            throw new IllegalStateException("Machine is closed");
        }
    }

    // Called from JNI during transitions/ticks
    private void dispatchAction(String name, long ctxHandle, long evtHandle, long paramsHandle) {
        ActionCallback cb = actions.get(name);
        ParameterizedActionCallback pcb = parameterizedActions.get(name);
        if (cb == null && pcb == null) {
            if (ctxHandle != 0L) Value.nativeFree(ctxHandle);
            if (evtHandle != 0L) Value.nativeFree(evtHandle);
            if (paramsHandle != 0L) Value.nativeFree(paramsHandle);
            return;
        }
        Value ctx = Value.fromNativeHandle(ctxHandle);
        Value evt = Value.fromNativeHandle(evtHandle);
        Value params = paramsHandle != 0L ? Value.fromNativeHandle(paramsHandle) : Value.NULL;
        if (pcb != null) {
            pcb.execute(ctx, evt, params);
        } else if (cb != null) {
            cb.execute(ctx, evt);
        }
    }

    // Called from JNI during transitions/can
    private boolean dispatchGuard(String name, long ctxHandle, long evtHandle, long paramsHandle) {
        GuardCallback cb = guards.get(name);
        ParameterizedGuardCallback pcb = parameterizedGuards.get(name);
        if (cb == null && pcb == null) {
            if (ctxHandle != 0L) Value.nativeFree(ctxHandle);
            if (evtHandle != 0L) Value.nativeFree(evtHandle);
            if (paramsHandle != 0L) Value.nativeFree(paramsHandle);
            return false;
        }
        Value ctx = Value.fromNativeHandle(ctxHandle);
        Value evt = Value.fromNativeHandle(evtHandle);
        Value params = paramsHandle != 0L ? Value.fromNativeHandle(paramsHandle) : Value.NULL;
        if (pcb != null) {
            return pcb.evaluate(ctx, evt, params);
        } else if (cb != null) {
            return cb.evaluate(ctx, evt);
        }
        return false;
    }

    // Called from JNI when computing dynamic final state or machine output
    private String dispatchOutputResolver(String statePath, long ctxHandle, long evtHandle) {
        OutputResolverCallback cb = outputResolvers.get(statePath);
        if (cb == null) {
            if (ctxHandle != 0L) Value.nativeFree(ctxHandle);
            if (evtHandle != 0L) Value.nativeFree(evtHandle);
            return "null";
        }
        Value ctx = Value.fromNativeHandle(ctxHandle);
        Value evt = Value.fromNativeHandle(evtHandle);
        Value out = cb.resolve(ctx, evt);
        return out != null ? out.toJson() : "null";
    }

    // Called from JNI when an invoked child service starts
    private void dispatchInvokeStart(String id, long srcHandle, long inputHandle, long ctxHandle, long evtHandle) {
        InvokeStartCallback cb = invokeStartCallback;
        if (cb == null) {
            if (srcHandle != 0L) Value.nativeFree(srcHandle);
            if (inputHandle != 0L) Value.nativeFree(inputHandle);
            if (ctxHandle != 0L) Value.nativeFree(ctxHandle);
            if (evtHandle != 0L) Value.nativeFree(evtHandle);
            return;
        }
        Value src = srcHandle != 0L ? Value.fromNativeHandle(srcHandle) : Value.NULL;
        Value inp = inputHandle != 0L ? Value.fromNativeHandle(inputHandle) : Value.NULL;
        Value ctx = ctxHandle != 0L ? Value.fromNativeHandle(ctxHandle) : Value.NULL;
        Value evt = evtHandle != 0L ? Value.fromNativeHandle(evtHandle) : Value.NULL;
        cb.onStart(id, src, inp, ctx, evt);
    }

    // Called from JNI when an invoked child service stops
    private void dispatchInvokeStop(String id) {
        InvokeStopCallback cb = invokeStopCallback;
        if (cb != null) {
            cb.onStop(id);
        }
    }

    // Called from JNI during transitions
    private long dispatchClock() {
        if (clockCallback != null) {
            return clockCallback.nowMs();
        }
        return 0L;
    }

    // Called from JNI during inspection events
    private void dispatchInspect(long evtHandle) {
        InspectCallback cb = inspectCallback;
        if (cb == null) {
            if (evtHandle != 0L) Value.nativeFree(evtHandle);
            return;
        }
        Value evt = Value.fromNativeHandle(evtHandle);
        cb.onInspect(evt);
    }

    private static native long nativeCreate(String jsonConfig, long clockHandle);
    private static native void nativeRegisterAction(long handle, String name);
    private static native void nativeRegisterGuard(long handle, String name);
    private static native void nativeRegisterOutputResolver(long handle, String statePath);
    private static native void nativeRegisterInvokeCallbacks(long handle, boolean hasStart, boolean hasStop);
    private static native void nativeRegisterClockCallback(long handle);
    private static native void nativeRegisterSimulatedClock(long handle, long clockHandle);
    private static native void nativeRegisterInspector(long handle);
    private native void nativeStart(long handle);
    private native void nativeStop(long handle);
    private static native void nativeFree(long handle);
    private native long nativeGetContext(long handle);
    private native long nativeGetOutput(long handle);
    private native long nativeGetFinalLeafOutput(long handle);
    private native String nativeInitialTransitionJson(long handle, String inputJson);
    private native String nativeTransitionPureJson(long handle, String snapshotJson, String eventJson);
    private native boolean nativeMatches(long handle, String statePath);
    private native boolean nativeCan(long handle, String event, String payloadJson);
    private native boolean nativeHasTag(long handle, String tag);
    private native boolean nativeIsDone(long handle);
    private native void nativeSend(long handle, String event, String payloadJson);
    private native void nativeTick(long handle);
    private native long nativeGetNextDeadline(long handle);
    private native String nativeGetSnapshot(long handle);
    private native void nativeRestoreSnapshot(long handle, String snapshotJson);
}

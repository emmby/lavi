package io.lavi;

import java.lang.ref.Cleaner;
import java.util.HashMap;
import java.util.Map;
import java.util.OptionalLong;

public final class Machine implements AutoCloseable {
    private static final Cleaner CLEANER = Cleaner.create();

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
    private final Cleaner.Cleanable cleanable;
    private final Map<String, ActionCallback> actions = new HashMap<>();
    private final Map<String, GuardCallback> guards = new HashMap<>();
    private ClockCallback clockCallback;
    private SimulatedClock simulatedClock;
    private boolean closed = false;

    public synchronized boolean isClosed() {
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
        this.cleanable = CLEANER.register(this, state);

        if (clock != null) {
            registerClock(clock);
        }
    }

    public synchronized Machine registerAction(String name, ActionCallback cb) {
        checkClosed();
        if (name == null || cb == null) {
            throw new IllegalArgumentException("Action name and callback cannot be null");
        }
        actions.put(name, cb);
        nativeRegisterAction(nativeHandle, name);
        return this;
    }

    public synchronized Machine registerGuard(String name, GuardCallback cb) {
        checkClosed();
        if (name == null || cb == null) {
            throw new IllegalArgumentException("Guard name and callback cannot be null");
        }
        guards.put(name, cb);
        nativeRegisterGuard(nativeHandle, name);
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
        guards.clear();
        clockCallback = null;
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
    private void dispatchAction(String name, Value ctx, Value evt) {
        ActionCallback cb = actions.get(name);
        if (cb != null) {
            cb.execute(ctx, evt);
        }
    }

    // Called from JNI during transitions/can
    private boolean dispatchGuard(String name, Value ctx, Value evt) {
        GuardCallback cb = guards.get(name);
        if (cb != null) {
            return cb.evaluate(ctx, evt);
        }
        return false;
    }

    // Called from JNI during transitions
    private long dispatchClock() {
        if (clockCallback != null) {
            return clockCallback.nowMs();
        }
        return 0L;
    }

    private static native long nativeCreate(String jsonConfig, long clockHandle);
    private static native void nativeRegisterAction(long handle, String name);
    private static native void nativeRegisterGuard(long handle, String name);
    private static native void nativeRegisterClockCallback(long handle);
    private static native void nativeRegisterSimulatedClock(long handle, long clockHandle);
    private native void nativeStart(long handle);
    private native void nativeStop(long handle);
    private static native void nativeFree(long handle);
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

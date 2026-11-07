package io.lavi;

import java.lang.ref.Cleaner;
import java.util.Collections;
import java.util.Set;
import java.util.WeakHashMap;

public final class SimulatedClock implements AutoCloseable {
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

    private final long nativeHandle;
    private final NativeState state;
    private final Cleaner.Cleanable cleanable;
    private final Set<Machine> machines = Collections.synchronizedSet(Collections.newSetFromMap(new WeakHashMap<>()));
    private boolean closed = false;

    public SimulatedClock() {
        this(0L);
    }

    public SimulatedClock(long initialMs) {
        NativeLoader.load();
        this.nativeHandle = nativeCreate(initialMs);
        this.state = new NativeState(nativeHandle);
        this.cleanable = CLEANER.register(this, state);
    }

    public synchronized long now() {
        checkClosed();
        return nativeNow(nativeHandle);
    }

    public synchronized void set(long targetMs) {
        checkClosed();
        Machine[] active = machines.toArray(new Machine[0]);
        nativeSet(nativeHandle, active, targetMs);
    }

    public synchronized void increment(long dtMs) {
        checkClosed();
        long current = now();
        set(Math.addExact(current, dtMs));
    }

    @Override
    public synchronized void close() {
        if (closed) return;
        closed = true;
        cleanable.clean();
        machines.clear();
    }

    synchronized void registerMachine(Machine m) {
        checkClosed();
        machines.add(m);
    }

    long getNativeHandle() {
        checkClosed();
        return nativeHandle;
    }

    private void checkClosed() {
        if (closed) {
            throw new IllegalStateException("SimulatedClock is closed");
        }
    }

    private static native long nativeCreate(long initialMs);
    private static native long nativeNow(long handle);
    private native void nativeSet(long handle, Machine[] machines, long targetMs);
    private static native void nativeFree(long handle);
}

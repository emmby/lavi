package io.lavi;

import java.lang.ref.PhantomReference;
import java.lang.ref.ReferenceQueue;
import java.util.Collections;
import java.util.Set;
import java.util.concurrent.ConcurrentHashMap;

final class NativeCleaner {
    private static final ReferenceQueue<Object> QUEUE = new ReferenceQueue<Object>();
    private static final Set<CleanableReference> REFS =
        Collections.newSetFromMap(new ConcurrentHashMap<CleanableReference, Boolean>());

    static {
        Thread daemonThread = new Thread(new Runnable() {
            @Override
            public void run() {
                while (true) {
                    try {
                        CleanableReference ref = (CleanableReference) QUEUE.remove();
                        if (ref != null) {
                            ref.clean();
                        }
                    } catch (InterruptedException e) {
                        break;
                    } catch (Throwable ignored) {
                    }
                }
            }
        }, "lavi-native-cleaner");
        daemonThread.setDaemon(true);
        daemonThread.start();
    }

    interface Cleanable {
        void clean();
    }

    private static final class CleanableReference extends PhantomReference<Object> implements Cleanable {
        private final Runnable action;
        private volatile boolean cleaned = false;

        CleanableReference(Object referent, Runnable action) {
            super(referent, QUEUE);
            this.action = action;
            REFS.add(this);
        }

        @Override
        public void clean() {
            REFS.remove(this);
            clear();
            if (!cleaned) {
                synchronized (this) {
                    if (!cleaned) {
                        cleaned = true;
                        try {
                            action.run();
                        } catch (Throwable ignored) {
                        }
                    }
                }
            }
        }
    }

    public static Cleanable register(Object obj, Runnable action) {
        return new CleanableReference(obj, action);
    }
}

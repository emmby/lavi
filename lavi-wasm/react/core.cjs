'use strict';

const React = require('react');

function defaultSelector(snapshot) {
  return snapshot;
}

function defaultCompare(a, b) {
  return Object.is(a, b);
}

function stableSerialize(val) {
  try {
    return JSON.stringify(val);
  } catch (_) {
    return String(val);
  }
}

function snapshotsEquivalent(a, b) {
  if (Object.is(a, b)) {
    return true;
  }
  if (!a || !b) {
    return false;
  }
  return (
    a.status === b.status &&
    Object.is(a.error, b.error) &&
    Object.is(a.output, b.output) &&
    stableSerialize(a.value) === stableSerialize(b.value) &&
    stableSerialize(a.context) === stableSerialize(b.context)
  );
}

function unsubscribeHandle(sub) {
  if (!sub) {
    return;
  }
  if (typeof sub.unsubscribe === 'function') {
    sub.unsubscribe();
  } else if (typeof sub === 'function') {
    sub();
  }
}

function createReactHooks(defaultCreateActor) {
  function instantiateActor(machineOrLogic, options) {
    const resolved =
      typeof machineOrLogic === 'function' ? machineOrLogic() : machineOrLogic;
    if (resolved && typeof resolved.createActor === 'function') {
      return resolved.createActor(options);
    }
    if (typeof defaultCreateActor === 'function') {
      return defaultCreateActor(resolved, options);
    }
    throw new Error(
      'Unable to create actor: pass a machine created with createMachine() or ensure createActor is available.'
    );
  }

  function createManagedActorRef(machineOrLogic, options) {
    const state = {
      current: instantiateActor(machineOrLogic, options),
      listeners: new Map(),
      started: false,
      stoppedByCleanup: false,
      explicitlyStopped: false,
      __laviReactStarting: false,
      latestLogic: machineOrLogic,
      latestOptions: options,
      needsSync: false,
    };

    function flushImplementationSync() {
      if (!state.needsSync) {
        return;
      }
      state.needsSync = false;
      const resolved =
        typeof state.latestLogic === 'function'
          ? state.latestLogic()
          : state.latestLogic;
      if (
        state.current &&
        typeof state.current.__syncImplementations === 'function'
      ) {
        state.current.__syncImplementations(resolved, state.latestOptions);
      }
    }

    const proxyRef = {
      get __laviReactStarting() {
        return state.__laviReactStarting;
      },
      set __laviReactStarting(val) {
        state.__laviReactStarting = Boolean(val);
      },
      get is_stopped() {
        return Boolean(state.current && state.current.is_stopped);
      },
      __queueImplementationSync(nextLogic, nextOptions) {
        if (
          state.latestLogic !== nextLogic ||
          state.latestOptions !== nextOptions
        ) {
          state.latestLogic = nextLogic;
          state.latestOptions = nextOptions;
          state.needsSync = true;
        }
      },
      __syncImplementations(nextLogic, nextOptions) {
        state.latestLogic = nextLogic;
        state.latestOptions = nextOptions;
        state.needsSync = true;
        flushImplementationSync();
      },
      getSnapshot() {
        flushImplementationSync();
        return state.current.getSnapshot();
      },
      getPersistedSnapshot() {
        flushImplementationSync();
        return state.current.getPersistedSnapshot();
      },
      getNextDeadline() {
        return state.current.getNextDeadline();
      },
      send(event) {
        flushImplementationSync();
        if (!state.started && !state.explicitlyStopped && !state.stoppedByCleanup) {
          proxyRef.start();
        }
        return state.current.send(event);
      },
      tick() {
        flushImplementationSync();
        return state.current.tick();
      },
      start() {
        if (state.explicitlyStopped) {
          return proxyRef;
        }
        flushImplementationSync();
        if (state.stoppedByCleanup) {
          state.stoppedByCleanup = false;
          state.started = true;
          if (
            state.current &&
            typeof state.current.__resumeAfterCleanup === 'function'
          ) {
            state.current.__resumeAfterCleanup();
          } else {
            state.current = instantiateActor(
              state.latestLogic,
              state.latestOptions
            );
            for (const [key, entry] of state.listeners.entries()) {
              unsubscribeHandle(entry.sub);
              entry.sub = state.current.subscribe(entry.observer);
              state.listeners.set(key, entry);
            }
            state.current.start();
          }
          return proxyRef;
        }
        if (!state.started) {
          state.started = true;
          state.current.start();
        }
        return proxyRef;
      },
      stop() {
        state.stoppedByCleanup = false;
        state.explicitlyStopped = true;
        state.current.stop();
        return proxyRef;
      },
      __stopFromEffectCleanup() {
        state.stoppedByCleanup = true;
        state.current.stop();
      },
      subscribe(observerOrFn) {
        const innerSub = state.current.subscribe(observerOrFn);
        const key = {};
        state.listeners.set(key, { observer: observerOrFn, sub: innerSub });
        return {
          unsubscribe() {
            const entry = state.listeners.get(key);
            state.listeners.delete(key);
            unsubscribeHandle(entry ? entry.sub : innerSub);
          },
        };
      },
    };

    return proxyRef;
  }

  function useSelector(
    actorRef,
    selector = defaultSelector,
    compare = defaultCompare
  ) {
    const selectorRef = React.useRef(selector);
    const compareRef = React.useRef(compare);
    selectorRef.current = selector;
    compareRef.current = compare;

    const cacheRef = React.useRef({
      actorRef: null,
      snapshot: undefined,
      selected: undefined,
      selector: null,
      compare: null,
      hasValue: false,
    });

    const getSelection = React.useCallback(() => {
      const cache = cacheRef.current;
      const sel = selectorRef.current;
      const cmp = compareRef.current;

      if (!actorRef || typeof actorRef.getSnapshot !== 'function') {
        const nextSelected = sel(undefined);
        if (
          !cache.hasValue ||
          cache.actorRef !== actorRef ||
          !cmp(cache.selected, nextSelected)
        ) {
          cache.actorRef = actorRef;
          cache.snapshot = undefined;
          cache.selected = nextSelected;
          cache.selector = sel;
          cache.compare = cmp;
          cache.hasValue = true;
        } else {
          cache.selector = sel;
          cache.compare = cmp;
        }
        return cache.selected;
      }

      if (cache.actorRef !== actorRef || !cache.hasValue) {
        const snap = actorRef.getSnapshot();
        const nextSelected = sel(snap);
        cache.actorRef = actorRef;
        cache.snapshot = snap;
        cache.selected = nextSelected;
        cache.selector = sel;
        cache.compare = cmp;
        cache.hasValue = true;
        return nextSelected;
      }

      if (cache.selector !== sel || cache.compare !== cmp) {
        const nextSelected = sel(cache.snapshot);
        cache.selector = sel;
        cache.compare = cmp;
        if (!cmp(cache.selected, nextSelected)) {
          cache.selected = nextSelected;
        }
      }

      return cache.selected;
    }, [actorRef]);

    const subscribe = React.useCallback(
      (onStoreChange) => {
        if (!actorRef || typeof actorRef.subscribe !== 'function') {
          return () => {};
        }

        const handleSnapshot = (nextSnapshot) => {
          const cache = cacheRef.current;
          const sel = selectorRef.current;
          const cmp = compareRef.current;
          const nextSelected = sel(nextSnapshot);

          if (
            actorRef.__laviReactStarting === true &&
            sel === defaultSelector &&
            cmp === defaultCompare &&
            cache.hasValue &&
            cache.actorRef === actorRef &&
            snapshotsEquivalent(cache.snapshot, nextSnapshot)
          ) {
            cache.snapshot = nextSnapshot;
            return;
          }

          if (
            !cache.hasValue ||
            cache.actorRef !== actorRef ||
            !cmp(cache.selected, nextSelected)
          ) {
            cache.actorRef = actorRef;
            cache.snapshot = nextSnapshot;
            cache.selected = nextSelected;
            cache.selector = sel;
            cache.compare = cmp;
            cache.hasValue = true;
            onStoreChange();
          } else {
            cache.snapshot = nextSnapshot;
            cache.selector = sel;
            cache.compare = cmp;
          }
        };

        const sub = actorRef.subscribe(handleSnapshot);

        if (typeof actorRef.getSnapshot === 'function') {
          const currentSnap = actorRef.getSnapshot();
          if (
            !cacheRef.current.hasValue ||
            cacheRef.current.actorRef !== actorRef ||
            !snapshotsEquivalent(cacheRef.current.snapshot, currentSnap)
          ) {
            handleSnapshot(currentSnap);
          }
        }

        return () => {
          unsubscribeHandle(sub);
        };
      },
      [actorRef]
    );

    return React.useSyncExternalStore(subscribe, getSelection, getSelection);
  }

  function useActorRef(machineOrLogic, options, observerOrListener) {
    const actorRefHolder = React.useRef(null);
    if (actorRefHolder.current === null) {
      actorRefHolder.current = createManagedActorRef(machineOrLogic, options);
    } else if (
      typeof actorRefHolder.current.__queueImplementationSync === 'function'
    ) {
      actorRefHolder.current.__queueImplementationSync(machineOrLogic, options);
    }
    const actorRef = actorRefHolder.current;

    const observerRef = React.useRef(observerOrListener);
    observerRef.current = observerOrListener;
    const hasObserver = Boolean(observerOrListener);

    React.useEffect(() => {
      if (
        actorRef &&
        typeof actorRef.__syncImplementations === 'function'
      ) {
        actorRef.__syncImplementations(machineOrLogic, options);
      }
    }, [actorRef, machineOrLogic, options]);

    React.useEffect(() => {
      if (!hasObserver || !actorRef || typeof actorRef.subscribe !== 'function') {
        return undefined;
      }
      const sub = actorRef.subscribe({
        next(snapshot) {
          const current = observerRef.current;
          if (typeof current === 'function') {
            current(snapshot);
          } else if (current && typeof current.next === 'function') {
            current.next(snapshot);
          }
        },
        error(err) {
          const current = observerRef.current;
          if (current && typeof current.error === 'function') {
            current.error(err);
          }
        },
        complete() {
          const current = observerRef.current;
          if (current && typeof current.complete === 'function') {
            current.complete();
          }
        },
      });
      return () => {
        unsubscribeHandle(sub);
      };
    }, [actorRef, hasObserver]);

    React.useEffect(() => {
      if (!actorRef) {
        return undefined;
      }
      actorRef.__laviReactStarting = true;
      try {
        actorRef.start();
      } finally {
        actorRef.__laviReactStarting = false;
      }
      return () => {
        if (typeof actorRef.__stopFromEffectCleanup === 'function') {
          actorRef.__stopFromEffectCleanup();
        } else if (typeof actorRef.stop === 'function') {
          actorRef.stop();
        }
      };
    }, [actorRef]);

    return actorRef;
  }

  function useActor(machineOrLogic, options) {
    const actorRef = useActorRef(machineOrLogic, options);
    const snapshot = useSelector(actorRef, defaultSelector, defaultCompare);
    const send = React.useCallback(
      (event) => actorRef.send(event),
      [actorRef]
    );
    return [snapshot, send, actorRef];
  }

  function useMachine(machineOrLogic, options) {
    return useActor(machineOrLogic, options);
  }

  return {
    useSelector,
    useActorRef,
    useActor,
    useMachine,
  };
}

module.exports = {
  createReactHooks,
  defaultSelector,
  defaultCompare,
  snapshotsEquivalent,
};

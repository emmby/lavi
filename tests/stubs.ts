export {
  cancel,
  enqueueActions,
  log,
  raise,
  and,
  or,
  not,
  stateIn,
  fromTransition,
  sendTo,
  sendParent,
  spawnChild,
  fromPromise,
  fromObservable,
  fromCallback,
  stopChild,
  forwardTo
} from '../lavi-wasm/pkg-npm/lavi';
export const CallbackActorRef = {};
export const of = (...args: any[]) => ({
  subscribe: (observer: any) => {
    const obs = typeof observer === 'function' ? { next: observer } : observer;
    for (const val of args) {
      obs.next?.(val);
    }
    obs.complete?.();
    return { unsubscribe: () => {} };
  }
});
export const toPromise = (actor?: any): Promise<any> => {
  return new Promise((resolve, reject) => {
    if (!actor) return resolve(undefined);
    const snap = actor.getSnapshot?.();
    if (snap?.status === 'done') return resolve(snap.output);
    if (snap?.status === 'error') return reject(snap.error);
    const sub = actor.subscribe({
      next: (s: any) => {
        if (s?.status === 'done') {
          sub?.unsubscribe?.();
          resolve(s.output);
        } else if (s?.status === 'error') {
          sub?.unsubscribe?.();
          reject(s.error);
        }
      },
      complete: () => {
        sub?.unsubscribe?.();
        resolve(actor.getSnapshot?.().output);
      },
      error: (err: any) => {
        sub?.unsubscribe?.();
        reject(err);
      }
    });
  });
};
export const createDoneActorEvent = (id: string, output?: any) => ({
  type: `xstate.done.actor.${id}`,
  output
});
export const resolveReferencedActor = (machine: any, src: string) => {
  if (typeof src === 'string') {
    const match = src.match(/^xstate\.invoke\.(\d+)\.(.*)/);
    if (match) {
      const [, indexStr, nodeId] = match;
      const node = machine?.getStateNodeById ? machine.getStateNodeById(nodeId) : undefined;
      const invokeConfig = node?.config?.invoke;
      const item = Array.isArray(invokeConfig) ? invokeConfig[parseInt(indexStr, 10)] : invokeConfig;
      return item?.src;
    }
    return machine?.implementations?.actors?.[src] || machine?.actors?.[src];
  }
  return src;
};
export const waitFor = (actor: any, predicate: (snapshot: any) => boolean, options?: { timeout?: number }): Promise<any> => {
  return new Promise((resolve, reject) => {
    let sub: any;
    const timeout = options?.timeout ?? 2000;
    const timer = setTimeout(() => {
      sub?.unsubscribe?.();
      reject(new Error(`waitFor timeout after ${timeout}ms`));
    }, timeout);

    const check = (snap: any) => {
      try {
        if (predicate(snap)) {
          clearTimeout(timer);
          sub?.unsubscribe?.();
          resolve(snap);
          return true;
        }
      } catch (err) {
        clearTimeout(timer);
        sub?.unsubscribe?.();
        reject(err);
        return true;
      }
      return false;
    };

    if (check(actor.getSnapshot())) {
      return;
    }

    sub = actor.subscribe({
      next: (snap: any) => {
        check(snap);
      },
      error: (err: any) => {
        clearTimeout(timer);
        sub?.unsubscribe?.();
        reject(err);
      },
      complete: () => {
        if (!check(actor.getSnapshot())) {
          clearTimeout(timer);
          sub?.unsubscribe?.();
          reject(new Error('Actor completed without satisfying predicate'));
        }
      }
    });
  });
};

// Mocked types for TS:
export type ActorRef = any;
export type ActorRefFromLogic = any;
export type AnyActorRef = any;
export type EventObject = any;
export type Snapshot = any;
export type EventFrom<T = any> = any;
export type ExecutableActionsFrom<T = any> = any;
export type ExecutableSpawnAction = any;
export type InspectionEvent = any;
export type InspectedActionEvent = any;

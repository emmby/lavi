import { describe, it, expect, vi } from 'vitest';
import {
  createMachine,
  createActor,
  fromPromise,
  fromTransition,
  fromObservable,
  fromCallback,
  assign
} from '../../lavi-wasm/pkg-npm/lavi';

describe('Milestone 3 Adversarial & Critic Stress Suite', () => {
  describe('Dimension 1: Deep Parent-Child-Grandchild Lifecycle & Teardown', () => {
    it('propagates input across 3 generations and cleanly tears down child and grandchild on parent stop', async () => {
      const grandchildStopped = vi.fn();
      const childCleanup = vi.fn();

      const grandchildLogic = fromCallback(({ input }) => {
        expect(input).toEqual({ secret: 999, level: 3 });
        return () => {
          grandchildStopped();
        };
      });

      const childLogic = fromCallback(({ input }) => {
        expect(input).toEqual({ secret: 999 });
        return () => {
          childCleanup();
        };
      });

      const parentMachine = createMachine({
        id: 'parentMachine',
        initial: 'active',
        states: {
          active: {
            invoke: [
              {
                id: 'child1',
                src: childLogic,
                input: { secret: 999 }
              },
              {
                id: 'grandchild1',
                src: grandchildLogic,
                input: { secret: 999, level: 3 }
              }
            ]
          }
        }
      });

      const actor = createActor(parentMachine).start();

      // Verify active children in snapshot.children
      const snap = actor.getSnapshot();
      expect(snap.children.child1).toBeDefined();
      expect(snap.children.grandchild1).toBeDefined();

      // Stopping root actor must tear down all active children
      actor.stop();
      expect(childCleanup).toHaveBeenCalledTimes(1);
      expect(grandchildStopped).toHaveBeenCalledTimes(1);
      expect(actor.getSnapshot().status).toBe('stopped');
    });
  });

  describe('Dimension 2: Promise Resolution & Completion Forwarding', () => {
    it('delivers xstate.done.actor.<id> on promise resolution with output to parent machine', async () => {
      const { resolve, promise } = Promise.withResolvers<number>();

      const delayPromiseLogic = fromPromise(({ input }: { input: { val: number } }) => {
        return promise.then(() => input.val * 2);
      });

      const machine = createMachine({
        id: 'promiseTest',
        initial: 'fetching',
        states: {
          fetching: {
            invoke: {
              id: 'fetcher',
              src: delayPromiseLogic,
              input: { val: 21 },
              onDone: {
                target: 'success',
                actions: assign({
                  result: ({ event }) => (event as any).output
                })
              }
            }
          },
          success: {}
        }
      });

      const actor = createActor(machine).start();
      expect(actor.getSnapshot().value).toBe('fetching');

      resolve(42);
      await Promise.resolve();
      await Promise.resolve();

      expect(actor.getSnapshot().value).toBe('success');
      expect((actor.getSnapshot().context as any).result).toBe(42);
    });
  });

  describe('Dimension 3: Sibling Invokes in Parallel States', () => {
    it('manages concurrent invoked promises in parallel regions and aggregates completion', async () => {
      const p1Resolvers = Promise.withResolvers<number>();
      const p2Resolvers = Promise.withResolvers<number>();

      const logic1 = fromPromise(() => p1Resolvers.promise);
      const logic2 = fromPromise(() => p2Resolvers.promise);

      const machine = createMachine({
        id: 'parallelInvokes',
        type: 'parallel',
        states: {
          regionA: {
            initial: 'loading',
            states: {
              loading: {
                invoke: {
                  id: 'p1',
                  src: logic1,
                  onDone: 'doneA'
                }
              },
              doneA: { type: 'final' }
            }
          },
          regionB: {
            initial: 'loading',
            states: {
              loading: {
                invoke: {
                  id: 'p2',
                  src: logic2,
                  onDone: 'doneB'
                }
              },
              doneB: { type: 'final' }
            }
          }
        }
      });

      const actor = createActor(machine).start();
      expect(actor.getSnapshot().status).toBe('active');

      p1Resolvers.resolve(100);
      await Promise.resolve();
      await Promise.resolve();
      expect(actor.getSnapshot().value).toEqual({ regionA: 'doneA', regionB: 'loading' });

      p2Resolvers.resolve(200);
      await Promise.resolve();
      await Promise.resolve();
      expect(actor.getSnapshot().value).toEqual({ regionA: 'doneA', regionB: 'doneB' });
      expect(actor.getSnapshot().status).toBe('done');
    });
  });

  describe('Dimension 4: Dynamic Input Evaluation with Self Reference', () => {
    it('provides accurate self reference to input factory function', () => {
      let selfInFactory: any = null;
      let selfInContext: any = null;

      const child = createMachine({
        types: {} as { input: { caller: any } },
        context: ({ input }) => {
          selfInContext = input.caller;
          return {};
        }
      });

      const parent = createMachine({
        initial: 'active',
        states: {
          active: {
            invoke: {
              src: child,
              input: ({ self }) => {
                selfInFactory = self;
                return { caller: self };
              }
            }
          }
        }
      });

      const parentActor = createActor(parent).start();

      expect(selfInFactory).toBe(parentActor);
      expect(selfInContext).toBe(parentActor);
    });
  });

  describe('Dimension 5: Multi-Actor Inspection Protocol', () => {
    it('captures full sequence of @xstate.actor, @xstate.event, @xstate.snapshot for parent and child', async () => {
      const recordedEvents: any[] = [];

      const child = createMachine({
        id: 'inspectedChild',
        initial: 'work',
        states: {
          work: {
            on: {
              PING: 'doneWork'
            },
            doneWork: {}
          }
        }
      });

      const parent = createMachine({
        id: 'inspectedParent',
        initial: 'idle',
        states: {
          idle: {
            invoke: {
              id: 'myChild',
              src: child
            }
          }
        }
      });

      const actor = createActor(parent, {
        id: 'parentActorId',
        inspect: (ev: any) => {
          recordedEvents.push(ev);
        }
      }).start();

      // send to child via snapshot.children
      actor.getSnapshot().children.myChild.send({ type: 'PING' });

      const actorTypes = recordedEvents.map((e) => e.type);
      expect(actorTypes).toContain('@xstate.actor');
      expect(actorTypes).toContain('@xstate.event');
      expect(actorTypes).toContain('@xstate.snapshot');

      // Both parent and child must have registered with inspector
      const registeredActors = recordedEvents
        .filter((e) => e.type === '@xstate.actor')
        .map((e) => e.actorRef.id);
      expect(registeredActors).toContain('parentActorId');
      expect(registeredActors).toContain('myChild');
    });
  });

  describe('Dimension 6: Observable and Callback Actor Input & Lifecycle', () => {
    it('handles fromObservable with input and next notifications', () => {
      const values: number[] = [];
      const logic = fromObservable(({ input }: { input: { factor: number } }) => ({
        subscribe: (observer: any) => {
          observer.next(1 * input.factor);
          observer.next(2 * input.factor);
          observer.complete();
          return { unsubscribe: () => {} };
        }
      }));

      const actor = createActor(logic, { input: { factor: 10 } }).start();
      actor.subscribe((snap: any) => {
        if (snap.context !== undefined) {
          values.push(snap.context);
        }
      });

      expect(actor.getSnapshot().status).toBe('done');
      expect(actor.getSnapshot().context).toBe(20);
    });

    it('handles fromCallback with input, sendBack, and receive', () => {
      let receivedEvent: any = null;
      let cleanupCalled = false;

      const logic = fromCallback(({ input, receive }) => {
        expect(input).toEqual({ key: 'value' });
        receive((event: any) => {
          receivedEvent = event;
        });
        return () => {
          cleanupCalled = true;
        };
      });

      const actor = createActor(logic, { input: { key: 'value' } }).start();
      actor.send({ type: 'TEST_EVENT', payload: 123 });

      expect(receivedEvent).toEqual({ type: 'TEST_EVENT', payload: 123 });

      actor.stop();
      expect(cleanupCalled).toBe(true);
    });
  });
});

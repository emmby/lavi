import { describe, it, expect, vi } from 'vitest';
import {
  createMachine,
  createActor,
  assign,
  sendParent,
  sendTo,
  fromPromise
} from '../../lavi-wasm/pkg-npm/lavi';
import { waitFor } from '../stubs';

describe('Milestone 3 Iteration 2 Adversarial Defect Verification', () => {
  it('Defect 1: preserves CURRENT_ACTOR across multiple sendParent actions and assigns self', () => {
    const parentEvents: any[] = [];
    let capturedSelf: any = null;

    const child = createMachine({
      initial: 'active',
      states: {
        active: {
          on: {
            TRIGGER: {
              actions: [
                sendParent({ type: 'P_MSG', val: 1 }),
                sendParent({ type: 'P_MSG', val: 2 }),
                sendParent({ type: 'P_MSG', val: 3 }),
                assign(({ self }: any) => {
                  capturedSelf = self;
                  return {};
                })
              ]
            }
          }
        }
      }
    });

    const parent = createMachine({
      initial: 'running',
      states: {
        running: {
          invoke: {
            id: 'childActor',
            src: child
          },
          on: {
            P_MSG: {
              actions: ({ event }) => {
                parentEvents.push((event as any).val);
              }
            }
          }
        }
      }
    });

    const actor = createActor(parent).start();
    const childRef = actor.getSnapshot().children.childActor;
    expect(childRef).toBeDefined();

    childRef.send({ type: 'TRIGGER' });

    expect(parentEvents).toEqual([1, 2, 3]);
    expect(capturedSelf).toBeDefined();
    expect(capturedSelf.sessionId).toBe(childRef.sessionId);
  });

  // Defect 2 is currently UNRESOLVED: child completes immediately during start_internal,
  // but parent drops the xstate.done.actor event because parent.machine.is_started is false during initialization.
  it('Defect 2: immediately dispatches xstate.done.actor on Actor::start_internal() when child starts in final state', () => {
    const child = createMachine({
      initial: 'doneState',
      output: { completedVal: 99 },
      states: {
        doneState: {
          type: 'final'
        }
      }
    });

    const parent = createMachine({
      initial: 'running',
      states: {
        running: {
          invoke: {
            id: 'immediateChild',
            src: child,
            onDone: {
              target: 'finished',
              actions: assign({
                donePayload: ({ event }: any) => event.output
              })
            }
          }
        },
        finished: {}
      }
    });

    const actor = createActor(parent).start();
    console.log('Defect 2 snapshot value:', actor.getSnapshot().value);
    console.log('Defect 2 child status:', actor.getSnapshot().children?.immediateChild?.getSnapshot()?.status);
    expect(actor.getSnapshot().value).toBe('finished');
    expect((actor.getSnapshot().context as any).donePayload).toEqual({ completedVal: 99 });
  });

  it('Defect 3: dynamic input factory can inspect self (sessionId, id, getSnapshot, children, system) without BorrowMutError panics', () => {
    let capturedInfo: any = null;

    const childMachine = createMachine({
      types: {} as { input: any },
      context: ({ input }) => {
        capturedInfo = input;
        return {};
      }
    });

    const parentMachine = createMachine({
      id: 'rootMachine',
      initial: 'active',
      states: {
        active: {
          invoke: {
            id: 'testChild',
            src: childMachine,
            input: ({ self }: any) => {
              return {
                sid: self.sessionId,
                id: self.id,
                snapStatus: self.getSnapshot().status,
                hasChildren: typeof self.getSnapshot().children === 'object',
                hasSystem: typeof self.system === 'object'
              };
            }
          }
        }
      }
    });

    const actor = createActor(parentMachine, { id: 'rootMachine' }).start();
    expect(capturedInfo).toBeDefined();
    expect(capturedInfo.sid).toBe(actor.sessionId);
    expect(capturedInfo.id).toBe('rootMachine');
    expect(capturedInfo.snapStatus).toBe('active');
    expect(capturedInfo.hasChildren).toBe(true);
    expect(capturedInfo.hasSystem).toBe(true);
  });

  it('Defect 4: passes post-transition context to dynamic input factory', () => {
    let receivedContextInInput: any = null;

    const child = createMachine({
      types: {} as { input: any },
      context: ({ input }) => ({ fromInput: input })
    });

    const machine = createMachine({
      initial: 's0',
      context: { count: 0 },
      states: {
        s0: {
          on: {
            INC: {
              target: 's1',
              actions: assign({
                count: ({ context }) => context.count + 50
              })
            }
          }
        },
        s1: {
          invoke: {
            id: 'childInS1',
            src: child,
            input: ({ context }: any) => {
              receivedContextInInput = context.count;
              return context.count;
            }
          }
        }
      }
    });

    const actor = createActor(machine).start();
    expect(actor.getSnapshot().value).toBe('s0');

    actor.send({ type: 'INC' });

    expect(actor.getSnapshot().value).toBe('s1');
    expect((actor.getSnapshot().context as any).count).toBe(50);
    expect(receivedContextInInput).toBe(50);
  });

  // Defect 5 is currently UNRESOLVED: StateNode::create_actor in state_node.rs does not evaluate
  // functional inputs (only Actor::create_actor for raw POJOs did).
  it('Defect 5: evaluates functional input options in standalone createActor(machine, { input: fn })', () => {
    let selfInInputFn: any = null;

    const m = createMachine({
      types: {} as { input: { initialNum: number } },
      context: ({ input }) => ({ num: input.initialNum * 2 })
    });

    const actor = createActor(m, {
      input: ({ self }: any) => {
        selfInInputFn = self;
        return { initialNum: 21 };
      }
    });

    actor.start();

    expect(selfInInputFn).toBe(actor);
    expect((actor.getSnapshot().context as any).num).toBe(42);
  });

  it('Defect 6: cleans up and restarts invokes across microstep loops (always/eventless transitions)', () => {
    let workerSpawnCount = 0;
    let workerCleanupCount = 0;

    const workerLogic = fromPromise(() => {
      workerSpawnCount++;
      return Promise.resolve(workerSpawnCount);
    });

    const machine = createMachine({
      initial: 'stepA',
      context: { loopCount: 0 },
      states: {
        stepA: {
          invoke: {
            id: 'worker',
            src: workerLogic
          },
          on: {
            NEXT: 'stepB'
          }
        },
        stepB: {
          always: [
            {
              guard: ({ context }) => context.loopCount < 2,
              target: 'stepA',
              actions: assign({
                loopCount: ({ context }) => context.loopCount + 1
              })
            },
            {
              target: 'stepDone'
            }
          ]
        },
        stepDone: {}
      }
    });

    const actor = createActor(machine).start();
    expect(actor.getSnapshot().value).toBe('stepA');
    expect(workerSpawnCount).toBe(1);

    // Transition stepA -> stepB -> always -> stepA (loopCount 1)
    actor.send({ type: 'NEXT' });
    expect(actor.getSnapshot().value).toBe('stepA');
    expect((actor.getSnapshot().context as any).loopCount).toBe(1);
    expect(workerSpawnCount).toBe(2);

    // Transition stepA -> stepB -> always -> stepA (loopCount 2)
    actor.send({ type: 'NEXT' });
    expect(actor.getSnapshot().value).toBe('stepA');
    expect((actor.getSnapshot().context as any).loopCount).toBe(2);
    expect(workerSpawnCount).toBe(3);

    // Transition stepA -> stepB -> always -> stepDone
    actor.send({ type: 'NEXT' });
    expect(actor.getSnapshot().value).toBe('stepDone');
  });

  it('Defect 7: system._register indexes actors by sessionId and custom id', () => {
    const parent = createMachine({
      id: 'parentMachine',
      initial: 'active',
      states: {
        active: {
          invoke: [
            {
              id: 'slotA',
              src: createMachine({ id: 'childA' })
            },
            {
              id: 'slotB',
              src: createMachine({ id: 'childB' })
            }
          ]
        }
      }
    });

    const actor = createActor(parent).start();
    const childA = actor.getSnapshot().children.slotA;
    const childB = actor.getSnapshot().children.slotB;

    expect(actor.system.get(childA.sessionId)).toBe(childA);
    expect(actor.system.get('slotA')).toBe(childA);

    expect(actor.system.get(childB.sessionId)).toBe(childB);
    expect(actor.system.get('slotB')).toBe(childB);
  });

  it('Defect 8: suppresses duplicate @xstate.event on reentrant relayed queue messages', () => {
    const recordedEvents: any[] = [];

    const child = createMachine({
      initial: 'ready',
      states: {
        ready: {
          on: {
            PING: {
              actions: sendParent({ type: 'PONG' })
            }
          }
        }
      }
    });

    const parent = createMachine({
      initial: 'active',
      states: {
        active: {
          invoke: {
            id: 'c',
            src: child
          },
          on: {
            TRIGGER: {
              actions: sendTo('c', { type: 'PING' })
            },
            PONG: 'finished'
          }
        },
        finished: {}
      }
    });

    const actor = createActor(parent, {
      inspect: (ev: any) => {
        if (ev.type === '@xstate.event') {
          recordedEvents.push(ev);
        }
      }
    }).start();

    actor.send({ type: 'TRIGGER' });

    expect(actor.getSnapshot().value).toBe('finished');

    // Count occurrences of event PONG
    const pongEvents = recordedEvents.filter((e) => e.event?.type === 'PONG');
    // Exactly 1 @xstate.event with sourceRef: child sessionId
    expect(pongEvents.length).toBe(1);
    expect(pongEvents[0].sourceRef).toBeDefined();
    expect(pongEvents[0].actorRef.sessionId).toBe(actor.sessionId);
  });

  it('Defect 9: waitFor handles immediate synchronous predicate satisfaction without TDZ error', async () => {
    const machine = createMachine({
      initial: 'ready',
      states: {
        ready: {}
      }
    });

    const actor = createActor(machine).start();

    // Snapshot is immediately in 'ready'
    const snap = await waitFor(actor, (s) => s.value === 'ready');
    expect(snap.value).toBe('ready');
  });
});

import { describe, it, expect, vi } from 'vitest';
import {
  createMachine,
  initialTransition,
  transition,
  fromTransition,
  sendTo,
  setup,
  assign,
  createActor
} from '../../lavi-wasm/pkg-npm/lavi';
import { fromPromise, toPromise, createDoneActorEvent, resolveReferencedActor } from '../stubs';

describe('Milestone 2 Challenger Adversarial Suite: fromTransition & Pure Action Descriptors', () => {
  // =========================================================================
  // Dimension 1: fromTransition Actor Logic Creator
  // =========================================================================
  describe('Dimension 1: fromTransition Multi-Round & Invariant Stress', () => {
    it('1.1 stress-tests 100 consecutive transitions with varying events and context mutations', () => {
      interface State {
        count: number;
        history: string[];
        nested: { val: number };
      }

      const logic = fromTransition(
        (state: State, event: any) => {
          switch (event.type) {
            case 'INC':
              return {
                ...state,
                count: state.count + (event.step ?? 1),
                history: [...state.history, `+${event.step ?? 1}`],
                nested: { val: state.nested.val + 10 }
              };
            case 'DEC':
              return {
                ...state,
                count: state.count - (event.step ?? 1),
                history: [...state.history, `-${event.step ?? 1}`],
                nested: { val: state.nested.val - 10 }
              };
            case 'MUL':
              return {
                ...state,
                count: state.count * event.factor,
                history: [...state.history, `*${event.factor}`],
                nested: { val: state.nested.val * event.factor }
              };
            case 'RESET':
              return {
                count: 0,
                history: [...state.history, 'RESET'],
                nested: { val: 0 }
              };
            default:
              return state;
          }
        },
        { count: 10, history: [], nested: { val: 100 } }
      );

      const [init] = initialTransition(logic);
      expect(init.status).toBe('active');
      expect(init.context.count).toBe(10);
      expect(init.context.nested.val).toBe(100);

      let snap = init;
      let expectedCount = 10;
      let expectedNested = 100;

      for (let i = 0; i < 25; i++) {
        const [next1] = transition(logic, snap, { type: 'INC', step: 2 });
        expectedCount += 2;
        expectedNested += 10;
        expect(next1.context.count).toBe(expectedCount);
        expect(next1.context.nested.val).toBe(expectedNested);

        const [next2] = transition(logic, next1, { type: 'DEC', step: 1 });
        expectedCount -= 1;
        expectedNested -= 10;
        expect(next2.context.count).toBe(expectedCount);
        expect(next2.context.nested.val).toBe(expectedNested);

        const [next3] = transition(logic, next2, { type: 'MUL', factor: 2 });
        expectedCount *= 2;
        expectedNested *= 2;
        expect(next3.context.count).toBe(expectedCount);
        expect(next3.context.nested.val).toBe(expectedNested);

        const [next4] = transition(logic, next3, { type: 'UNKNOWN_NOOP' });
        expect(next4.context.count).toBe(expectedCount);

        snap = next4;
      }

      expect(snap.context.history.length).toBe(75); // 25 * 3 mutations
      expect(snap.context.count).toBe(expectedCount);
      expect(snap.context.nested.val).toBe(expectedNested);
    });

    it('1.2 verifies snapshot immutability across multiple branching transitions from the same root', () => {
      const logic = fromTransition(
        (state: { count: number; items: string[] }, event: any) => {
          if (event.type === 'ADD') {
            return {
              count: state.count + 1,
              items: [...state.items, event.item]
            };
          }
          return state;
        },
        { count: 0, items: [] }
      );

      const [init] = initialTransition(logic);
      const [s1] = transition(logic, init, { type: 'ADD', item: 'first' });

      // Branch A
      const [branchA1] = transition(logic, s1, { type: 'ADD', item: 'A1' });
      const [branchA2] = transition(logic, branchA1, { type: 'ADD', item: 'A2' });

      // Branch B (from s1)
      const [branchB1] = transition(logic, s1, { type: 'ADD', item: 'B1' });
      const [branchB2] = transition(logic, branchB1, { type: 'ADD', item: 'B2' });

      // Invariants:
      expect(init.context.items).toEqual([]);
      expect(init.context.count).toBe(0);

      expect(s1.context.items).toEqual(['first']);
      expect(s1.context.count).toBe(1);

      expect(branchA1.context.items).toEqual(['first', 'A1']);
      expect(branchA2.context.items).toEqual(['first', 'A1', 'A2']);

      expect(branchB1.context.items).toEqual(['first', 'B1']);
      expect(branchB2.context.items).toEqual(['first', 'B1', 'B2']);

      // Distinct references
      expect(init).not.toBe(s1);
      expect(s1).not.toBe(branchA1);
      expect(branchA1).not.toBe(branchB1);
      expect(branchA1.context).not.toBe(branchB1.context);
    });

    it('1.3 tests getInitialSnapshot with dynamic context function and input parameters', () => {
      const initialFn = vi.fn(({ input }: any) => ({
        multiplied: (input?.base ?? 1) * (input?.factor ?? 1),
        configured: true
      }));

      const logic = fromTransition((s) => s, initialFn);

      // Call getInitialSnapshot with input
      const snap1 = logic.getInitialSnapshot({}, { base: 10, factor: 5 });
      expect(snap1.status).toBe('active');
      expect(snap1.context).toEqual({ multiplied: 50, configured: true });

      // Call initialTransition with options { input }
      const [snap2] = initialTransition(logic, { input: { base: 7, factor: 3 } } as any);
      expect(snap2.status).toBe('active');
      expect(snap2.context).toEqual({ multiplied: 21, configured: true });

      // Call with undefined input
      const snap3 = logic.getInitialSnapshot({}, undefined);
      expect(snap3.context).toEqual({ multiplied: 1, configured: true });

      // Call with null input
      const snap4 = logic.getInitialSnapshot({}, null);
      expect(snap4.context).toEqual({ multiplied: 1, configured: true });
    });

    it('1.4 tests getPersistedSnapshot and restoreSnapshot roundtrip fidelity', () => {
      const logic = fromTransition(
        (s: number, evt: any) => (evt.type === 'INC' ? s + 1 : s),
        100
      );

      const [init] = initialTransition(logic);
      const [s1] = transition(logic, init, { type: 'INC' });
      expect(s1.context).toBe(101);

      const persisted = logic.getPersistedSnapshot(s1);
      expect(persisted).toBeDefined();

      const restored = logic.restoreSnapshot(persisted);
      expect(restored.status).toBe('active');
      expect(restored.context).toBe(101);

      const [s2] = transition(logic, restored, { type: 'INC' });
      expect(s2.context).toBe(102);
    });

    it('1.5 tests createActor with fromTransition: state stepping and subscription notifications', () => {
      const logic = fromTransition(
        (s: number, evt: any) => (evt.type === 'ADD' ? s + evt.amount : s),
        10
      );

      const actor = createActor(logic).start();
      expect(actor.getSnapshot().context).toBe(10);

      const callsA: number[] = [];
      const callsB: number[] = [];

      const subA = actor.subscribe((snap: any) => {
        callsA.push(snap.context);
      });

      const subB = actor.subscribe((snap: any) => {
        callsB.push(snap.context);
      });

      actor.send({ type: 'ADD', amount: 5 });
      expect(actor.getSnapshot().context).toBe(15);
      expect(callsA).toEqual([15]);
      expect(callsB).toEqual([15]);

      actor.send({ type: 'ADD', amount: 10 });
      expect(actor.getSnapshot().context).toBe(25);
      expect(callsA).toEqual([15, 25]);
      expect(callsB).toEqual([15, 25]);
    });

    it('1.6 [DEFECT PROBE] verifies subscription unsubscribe removes observer from notifications', () => {
      const logic = fromTransition(
        (s: number, evt: any) => (evt.type === 'ADD' ? s + evt.amount : s),
        10
      );

      const actor = createActor(logic).start();
      const calls: number[] = [];

      const sub = actor.subscribe((snap: any) => {
        calls.push(snap.context);
      });

      actor.send({ type: 'ADD', amount: 5 });
      expect(calls).toEqual([15]);

      // Unsubscribe
      sub.unsubscribe();

      actor.send({ type: 'ADD', amount: 20 });
      expect(actor.getSnapshot().context).toBe(35);

      // In standard Observable semantics, after unsubscribe(), calls should NOT receive 35.
      // This assertion proves the bug in lavi-wasm/src/logic.rs line 141 (no-op unsub_closure).
      expect(calls).toEqual([15]);
    });

    it('1.7 handles primitive and null contexts in fromTransition', () => {
      // Number
      const numLogic = fromTransition((s: number, e: any) => s + e.val, 10);
      const [numInit] = initialTransition(numLogic);
      expect(numInit.context).toBe(10);
      const [numNext] = transition(numLogic, numInit, { type: 'add', val: 5 });
      expect(numNext.context).toBe(15);

      // String
      const strLogic = fromTransition((s: string, e: any) => s + '-' + e.word, 'hello');
      const [strInit] = initialTransition(strLogic);
      expect(strInit.context).toBe('hello');
      const [strNext] = transition(strLogic, strInit, { type: 'append', word: 'world' });
      expect(strNext.context).toBe('hello-world');

      // Null
      const nullLogic = fromTransition((_s: any, e: any) => e.data, null);
      const [nullInit] = initialTransition(nullLogic);
      expect(nullInit.context).toBeNull();
      const [nullNext] = transition(nullLogic, nullInit, { type: 'set', data: { a: 1 } });
      expect(nullNext.context).toEqual({ a: 1 });
    });

    it('1.8 preserves extra snapshot metadata fields across transition calls', () => {
      const logic = fromTransition((s: number) => s + 1, 0);
      const customSnapshot = {
        status: 'active',
        context: 5,
        customMeta: { traceId: 'abc-123' },
        tags: ['counter']
      };

      const [nextSnap] = transition(logic, customSnapshot, { type: 'tick' });
      expect(nextSnap.context).toBe(6);
      expect(nextSnap.customMeta).toEqual({ traceId: 'abc-123' });
      expect(nextSnap.tags).toEqual(['counter']);
    });
  });

  // =========================================================================
  // Dimension 2: Pure Action Descriptors (spawnChild, sendTo)
  // =========================================================================
  describe('Dimension 2: Pure Action Descriptors & Invoke Resolution', () => {
    it('2.1 multi-level hierarchical machine with root, compound, and leaf invokes in initialTransition', () => {
      const child1 = createMachine({ id: 'child1' });
      const child2 = createMachine({ id: 'child2' });
      const child3 = createMachine({ id: 'child3' });

      const machine = createMachine({
        id: 'rootMachine',
        invoke: {
          id: 'rootChild',
          src: child1,
          input: { role: 'root' }
        },
        initial: 'parentState',
        states: {
          parentState: {
            invoke: {
              id: 'parentChild',
              src: child2,
              input: { role: 'parent' }
            },
            initial: 'leafState',
            states: {
              leafState: {
                invoke: {
                  id: 'leafChild',
                  src: child3,
                  input: { role: 'leaf' }
                }
              }
            }
          }
        }
      });

      const [snapshot, actions] = initialTransition(machine);

      // Verify actions contains xstate.spawnChild for all 3 levels
      const spawnActions = actions.filter((a: any) => a.type === 'xstate.spawnChild');
      expect(spawnActions.length).toBe(3);

      const rootSpawn = spawnActions.find((a: any) => a.params?.id === 'rootChild');
      expect(rootSpawn).toBeDefined();
      expect(rootSpawn?.params?.input).toEqual({ role: 'root' });
      expect(rootSpawn?.params?.actorRef?.id).toBe('rootChild');

      const parentSpawn = spawnActions.find((a: any) => a.params?.id === 'parentChild');
      expect(parentSpawn).toBeDefined();
      expect(parentSpawn?.params?.input).toEqual({ role: 'parent' });
      expect(parentSpawn?.params?.actorRef?.id).toBe('parentChild');

      const leafSpawn = spawnActions.find((a: any) => a.params?.id === 'leafChild');
      expect(leafSpawn).toBeDefined();
      expect(leafSpawn?.params?.input).toEqual({ role: 'leaf' });
      expect(leafSpawn?.params?.actorRef?.id).toBe('leafChild');

      // Verify snapshot.children contains all 3 actors
      expect(snapshot.children).toBeDefined();
      expect(snapshot.children.rootChild).toBeDefined();
      expect(snapshot.children.parentChild).toBeDefined();
      expect(snapshot.children.leafChild).toBeDefined();
    });

    it('2.2 parallel states with simultaneous invokes in orthogonal regions', () => {
      const workerA = createMachine({ id: 'workerA' });
      const workerB = createMachine({ id: 'workerB' });

      const machine = createMachine({
        id: 'parallelMachine',
        type: 'parallel',
        states: {
          regionA: {
            invoke: {
              id: 'actorA',
              src: workerA,
              input: { region: 'A' }
            }
          },
          regionB: {
            invoke: {
              id: 'actorB',
              src: workerB,
              input: { region: 'B' }
            }
          }
        }
      });

      const [snapshot, actions] = initialTransition(machine);

      const spawnActions = actions.filter((a: any) => a.type === 'xstate.spawnChild');
      expect(spawnActions.length).toBe(2);

      const spawnA = spawnActions.find((a: any) => a.params?.id === 'actorA');
      const spawnB = spawnActions.find((a: any) => a.params?.id === 'actorB');

      expect(spawnA).toBeDefined();
      expect(spawnA?.params?.input).toEqual({ region: 'A' });

      expect(spawnB).toBeDefined();
      expect(spawnB?.params?.input).toEqual({ region: 'B' });

      expect(snapshot.children.actorA).toBeDefined();
      expect(snapshot.children.actorB).toBeDefined();
    });

    it('2.3 state transition triggering invoke emits xstate.spawnChild and updates snapshot.children', () => {
      const workerMachine = createMachine({ id: 'worker' });

      const machine = createMachine({
        id: 'flowMachine',
        initial: 'idle',
        states: {
          idle: {
            on: {
              START: 'working'
            }
          },
          working: {
            invoke: {
              id: 'workerChild',
              src: workerMachine,
              input: { task: 'compute' }
            },
            on: {
              FINISH: 'done'
            }
          },
          done: {}
        }
      });

      const [initSnap, initActions] = initialTransition(machine);
      expect(initSnap.value).toBe('idle');
      expect(initActions.filter((a: any) => a.type === 'xstate.spawnChild').length).toBe(0);
      expect(initSnap.children?.workerChild).toBeUndefined();

      // Transition to 'working'
      const [workSnap, workActions] = transition(machine, initSnap, { type: 'START' });
      expect(workSnap.value).toBe('working');

      const spawnAction = workActions.find((a: any) => a.type === 'xstate.spawnChild');
      expect(spawnAction).toBeDefined();
      expect(spawnAction?.params?.id).toBe('workerChild');
      expect(spawnAction?.params?.input).toEqual({ task: 'compute' });

      expect(workSnap.children?.workerChild).toBeDefined();
      expect(workSnap.children.workerChild.id).toBe('workerChild');
    });

    it('2.4 dynamic input functions evaluated with context and event', () => {
      const child = createMachine({ id: 'dynamicChild' });

      const machine = createMachine({
        id: 'dynMachine',
        initial: 'start',
        context: { multiplier: 3, base: 10 },
        states: {
          start: {
            on: {
              GO: {
                target: 'running'
              }
            }
          },
          running: {
            invoke: {
              id: 'calcActor',
              src: child,
              input: ({ context, event }: any) => ({
                result: context.base * context.multiplier + (event.extra ?? 0),
                triggeredBy: event.type
              })
            }
          }
        }
      });

      const [initSnap] = initialTransition(machine);
      const [, actions] = transition(machine, initSnap, { type: 'GO', extra: 5 });

      const spawn = actions.find((a: any) => a.type === 'xstate.spawnChild');
      expect(spawn).toBeDefined();
      expect(spawn?.params?.id).toBe('calcActor');
      expect(spawn?.params?.input).toEqual({
        result: 35, // 10 * 3 + 5
        triggeredBy: 'GO'
      });
    });

    it('2.5 sendTo with dynamic target functions and dynamic event functions', () => {
      const machine = createMachine({
        id: 'sendMachine',
        initial: 'ready',
        context: {
          activeTargetId: 'worker_42',
          sequence: 1
        },
        states: {
          ready: {
            on: {
              DISPATCH: {
                actions: sendTo(
                  ({ context }: any) => context.activeTargetId,
                  ({ context, event }: any) => ({
                    type: 'WORK_ORDER',
                    seq: context.sequence,
                    payload: event.data
                  }),
                  { delay: 100, id: 'workOrderTimer' }
                )
              }
            }
          }
        }
      });

      const [init] = initialTransition(machine);
      const [, actions] = transition(machine, init, { type: 'DISPATCH', data: 'process_batch' });

      const sendAction = actions.find((a: any) => a.type === 'xstate.sendTo');
      expect(sendAction).toBeDefined();
      expect(sendAction?.params).toMatchObject({
        targetId: 'worker_42',
        to: 'worker_42',
        event: {
          type: 'WORK_ORDER',
          seq: 1,
          payload: 'process_batch'
        },
        delay: 100,
        id: 'workOrderTimer'
      });
    });

    it('2.6 sendTo targeting actor object reference instead of string ID', () => {
      const machine = createMachine({
        id: 'sendObjMachine',
        initial: 'ready',
        context: {
          recipientRef: { id: 'special_actor', sessionId: 'sess_123' }
        },
        states: {
          ready: {
            on: {
              PING: {
                actions: sendTo(
                  ({ context }: any) => context.recipientRef,
                  { type: 'HELLO' }
                )
              }
            }
          }
        }
      });

      const [init] = initialTransition(machine);
      const [, actions] = transition(machine, init, { type: 'PING' });

      const sendAction = actions.find((a: any) => a.type === 'xstate.sendTo');
      expect(sendAction).toBeDefined();
      expect(sendAction?.params?.targetId).toBe('special_actor');
      expect(sendAction?.params?.to).toEqual({ id: 'special_actor', sessionId: 'sess_123' });
      expect(sendAction?.params?.event).toEqual({ type: 'HELLO' });
    });
  });

  // =========================================================================
  // Dimension 3: Edge Cases, Hostile Inputs & Error Boundaries
  // =========================================================================
  describe('Dimension 3: Edge Cases & Hostile Inputs', () => {
    it('3.1 sendTo with invalid string event produces xstate.sendTo.invalid descriptor', () => {
      const invalidAction = sendTo('recipient', 'INVALID_STRING_EVENT' as any);
      expect((invalidAction as any).type).toBe('xstate.sendTo.invalid');
      expect((invalidAction as any).error).toContain('Only event objects may be used with sendTo');
    });

    it('3.2 invoke with array of multiple invoke definitions in single state', () => {
      const childA = createMachine({ id: 'cA' });
      const childB = createMachine({ id: 'cB' });

      const machine = createMachine({
        id: 'multiInvoke',
        initial: 'running',
        states: {
          running: {
            invoke: [
              { id: 'actorA', src: childA, input: { tag: 'A' } },
              { id: 'actorB', src: childB, input: { tag: 'B' } }
            ]
          }
        }
      });

      const [snap, actions] = initialTransition(machine);
      const spawns = actions.filter((a: any) => a.type === 'xstate.spawnChild');
      expect(spawns.length).toBe(2);

      expect(spawns.find((s: any) => s.params?.id === 'actorA')).toBeDefined();
      expect(spawns.find((s: any) => s.params?.id === 'actorB')).toBeDefined();
      expect(snap.children?.actorA).toBeDefined();
      expect(snap.children?.actorB).toBeDefined();
    });

    it('3.3 invoke with default/generated ID when id is omitted', () => {
      const child = createMachine({ id: 'anonymous' });

      const machine = createMachine({
        id: 'noIdMachine',
        initial: 'start',
        states: {
          start: {
            invoke: {
              src: child
            }
          }
        }
      });

      const [snap, actions] = initialTransition(machine);
      const spawn = actions.find((a: any) => a.type === 'xstate.spawnChild');
      expect(spawn).toBeDefined();
      expect(spawn?.params?.id).toBeDefined();
      expect(typeof spawn?.params?.id).toBe('string');
      expect(spawn?.params?.id.length).toBeGreaterThan(0);
      expect(spawn?.params?.actorRef?.id).toBe(spawn?.params?.id);
    });

    it('3.4 invoke with static null/undefined input', () => {
      const child = createMachine({ id: 'nullInputChild' });

      const machine = createMachine({
        id: 'nullInpMachine',
        initial: 'start',
        states: {
          start: {
            invoke: {
              id: 'child1',
              src: child,
              input: null
            }
          }
        }
      });

      const [, actions] = initialTransition(machine);
      const spawn = actions.find((a: any) => a.type === 'xstate.spawnChild');
      expect(spawn).toBeDefined();
      expect(spawn?.params?.id).toBe('child1');
      expect(spawn?.params?.input).toBeNull();
    });

    it('3.5 sendTo edge cases: undefined target, empty string target, empty ID object', () => {
      const machine = createMachine({
        id: 'edgeTargets',
        initial: 'start',
        states: {
          start: {
            on: {
              SEND_UNDEF: {
                actions: sendTo(() => undefined, { type: 'EVT' })
              },
              SEND_EMPTY_STR: {
                actions: sendTo('', { type: 'EVT' })
              },
              SEND_NO_ID_OBJ: {
                actions: sendTo(() => ({ otherProp: 123 }), { type: 'EVT' })
              }
            }
          }
        }
      });

      const [init] = initialTransition(machine);

      // Undefined target
      const [, a1] = transition(machine, init, { type: 'SEND_UNDEF' });
      const send1 = a1.find((a: any) => a.type === 'xstate.sendTo');
      expect(send1).toBeDefined();
      expect(send1?.params?.targetId).toBeUndefined();

      // Empty string target
      const [, a2] = transition(machine, init, { type: 'SEND_EMPTY_STR' });
      const send2 = a2.find((a: any) => a.type === 'xstate.sendTo');
      expect(send2).toBeDefined();
      expect(send2?.params?.targetId).toBe('');

      // Object without id property
      const [, a3] = transition(machine, init, { type: 'SEND_NO_ID_OBJ' });
      const send3 = a3.find((a: any) => a.type === 'xstate.sendTo');
      expect(send3).toBeDefined();
      expect(send3?.params?.targetId).toBeUndefined();
    });

    it('3.6 setup({ actors }) reference actor resolution in multi-step serverless workflow', async () => {
      const results: string[] = [];

      const machine = setup({
        actors: {
          step1: fromPromise(async ({ input }: any) => {
            results.push(`step1:${input?.val}`);
            return { val: (input?.val ?? 0) + 10 };
          }),
          step2: fromPromise(async ({ input }: any) => {
            results.push(`step2:${input?.val}`);
            return { val: (input?.val ?? 0) * 2 };
          })
        }
      }).createMachine({
        id: 'pipeline',
        initial: 'first',
        context: { value: 5 },
        states: {
          first: {
            invoke: {
              id: 'actorStep1',
              src: 'step1',
              input: ({ context }: any) => ({ val: context.value }),
              onDone: 'second'
            }
          },
          second: {
            invoke: {
              id: 'actorStep2',
              src: 'step2',
              input: ({ context }: any) => ({ val: context.value }),
              onDone: 'done'
            }
          },
          done: {}
        }
      });

      // Step 1: Initial transition
      const [s0, a0] = initialTransition(machine);
      expect(s0.value).toBe('first');
      const spawn1 = a0.find((a: any) => a.type === 'xstate.spawnChild');
      expect(spawn1?.params?.id).toBe('actorStep1');
      expect(spawn1?.params?.src).toBe('step1');
      expect(spawn1?.params?.input).toEqual({ val: 5 });

      // Simulate async completion of step 1
      const logic1 = resolveReferencedActor(machine, spawn1?.params?.src);
      expect(logic1).toBeDefined();
      const output1 = await logic1.config({ input: spawn1?.params?.input });
      expect(output1).toEqual({ val: 15 });

      // Step 2: Transition on done event
      const [s1, a1] = transition(machine, s0, {
        type: 'xstate.done.actor.actorStep1',
        output: output1
      });
      expect(s1.value).toBe('second');

      const spawn2 = a1.find((a: any) => a.type === 'xstate.spawnChild');
      expect(spawn2?.params?.id).toBe('actorStep2');
      expect(spawn2?.params?.src).toBe('step2');

      // Simulate async completion of step 2
      const logic2 = resolveReferencedActor(machine, spawn2?.params?.src);
      const output2 = await logic2.config({ input: spawn2?.params?.input });
      expect(output2).toEqual({ val: 10 });

      // Step 3: Transition on done event to finish
      const [s2] = transition(machine, s1, {
        type: 'xstate.done.actor.actorStep2',
        output: output2
      });
      expect(s2.value).toBe('done');

      expect(results).toEqual(['step1:5', 'step2:5']);
    });

    it('3.7 [DEFECT PROBE] inline actions on invoke.onDone are dropped by lavi-wasm process_config', () => {
      let actionExecuted = false;

      const machine = createMachine({
        id: 'invokeOnDoneActions',
        initial: 'step1',
        context: { count: 0 },
        states: {
          step1: {
            invoke: {
              id: 'childWorker',
              src: 'dummy',
              onDone: {
                target: 'step2',
                actions: [
                  () => { actionExecuted = true; },
                  assign({ count: ({ event }: any) => event.output })
                ]
              }
            }
          },
          step2: {}
        }
      });

      const [s0] = initialTransition(machine);
      expect(s0.value).toBe('step1');

      const [s1] = transition(machine, s0, {
        type: 'xstate.done.actor.childWorker',
        output: 42
      });

      // Target transition succeeds:
      expect(s1.value).toBe('step2');

      // But actions on invoke.onDone were dropped!
      // In a compliant XState 5 runtime, actionExecuted should be true and s1.context.count should be 42.
      // This assertion confirms the defect:
      expect(actionExecuted).toBe(true);
      expect(s1.context.count).toBe(42);
    });
  });
});

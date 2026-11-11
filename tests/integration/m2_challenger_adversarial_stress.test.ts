import { describe, it, expect, vi } from 'vitest';
import {
  createMachine,
  initialTransition,
  transition,
  fromTransition,
  setup,
  assign,
  raise,
  createActor
} from '../../lavi-wasm/pkg-npm/lavi';

describe('Milestone 2 Iteration 2 Adversarial Stress Suite', () => {
  // =========================================================================
  // Dimension 1: Defect 1 - fromTransition High-Load & Multi-Observer Stress
  // =========================================================================
  describe('Dimension 1: fromTransition 50+ Observers Rapid Subscribe & Unsubscribe', () => {
    it('1.1 handles 100 simultaneous observers with rapid interleaved sends and selective unsubscriptions', () => {
      const logic = fromTransition(
        (state: { count: number }, event: { type: string; step?: number }) => {
          if (event.type === 'INC') {
            return { count: state.count + (event.step ?? 1) };
          }
          return state;
        },
        { count: 0 }
      );

      const actor = createActor(logic).start();
      const numObservers = 100;
      const notifications: number[][] = Array.from({ length: numObservers }, () => []);
      const subscriptions: { unsubscribe: () => void }[] = [];

      // Step 1: Subscribe 100 observers
      for (let i = 0; i < numObservers; i++) {
        const index = i;
        const sub = actor.subscribe((snap: any) => {
          notifications[index].push(snap.context.count);
        });
        subscriptions.push(sub);
      }

      // Step 2: Send first batch of events
      actor.send({ type: 'INC', step: 1 });
      actor.send({ type: 'INC', step: 2 });

      // All 100 observers should have received [1, 3]
      for (let i = 0; i < numObservers; i++) {
        expect(notifications[i]).toEqual([1, 3]);
      }

      // Step 3: Unsubscribe every even index (0, 2, 4, ... 98) - exactly 50 observers
      for (let i = 0; i < numObservers; i += 2) {
        subscriptions[i].unsubscribe();
      }

      // Step 4: Send second batch of events
      actor.send({ type: 'INC', step: 5 });
      actor.send({ type: 'INC', step: 10 });

      expect(actor.getSnapshot().context.count).toBe(18);

      // Even observers should remain at [1, 3]
      for (let i = 0; i < numObservers; i += 2) {
        expect(notifications[i]).toEqual([1, 3]);
      }

      // Odd observers should have received [1, 3, 8, 18]
      for (let i = 1; i < numObservers; i += 2) {
        expect(notifications[i]).toEqual([1, 3, 8, 18]);
      }

      // Step 5: Rapidly unsubscribe the remaining 50 observers
      for (let i = 1; i < numObservers; i += 2) {
        subscriptions[i].unsubscribe();
      }

      // Step 6: Send event after all unsubscribed
      actor.send({ type: 'INC', step: 100 });
      expect(actor.getSnapshot().context.count).toBe(118);

      // No observers receive the 118 update
      for (let i = 0; i < numObservers; i++) {
        if (i % 2 === 0) {
          expect(notifications[i]).toEqual([1, 3]);
        } else {
          expect(notifications[i]).toEqual([1, 3, 8, 18]);
        }
      }
    });

    it('1.2 multiple subscribe and unsubscribe churn across 50 cycles with fresh observers', () => {
      const logic = fromTransition(
        (s: number, evt: any) => (evt.type === 'TICK' ? s + 1 : s),
        0
      );
      const actor = createActor(logic).start();

      let activeCalls = 0;
      for (let cycle = 0; cycle < 50; cycle++) {
        // Subscribe 5 observers
        const subs = [0, 1, 2, 3, 4].map(() =>
          actor.subscribe(() => {
            activeCalls++;
          })
        );

        // Send 1 event -> 5 calls
        actor.send({ type: 'TICK' });

        // Unsubscribe 3 of them
        subs[0].unsubscribe();
        subs[2].unsubscribe();
        subs[4].unsubscribe();

        // Send another event -> 2 calls
        actor.send({ type: 'TICK' });

        // Unsubscribe remaining 2
        subs[1].unsubscribe();
        subs[3].unsubscribe();

        // Send another event -> 0 calls
        actor.send({ type: 'TICK' });
      }

      // Each cycle: 5 + 2 = 7 calls * 50 cycles = 350 calls
      expect(activeCalls).toBe(350);
      expect(actor.getSnapshot().context).toBe(150); // 3 TICKs per cycle * 50 cycles
    });

    it('1.3 stop() cleans up active observers and triggers complete() on all 50+ subscribers', () => {
      const logic = fromTransition(
        (s: number, evt: any) => (evt.type === 'ADD' ? s + evt.val : s),
        10
      );
      const actor = createActor(logic).start();

      const completed: number[] = [];
      const nextCalls: number[] = [];
      const numObservers = 60;

      for (let i = 0; i < numObservers; i++) {
        const id = i;
        actor.subscribe({
          next: (snap: any) => {
            nextCalls.push(snap.context);
          },
          complete: () => {
            completed.push(id);
          }
        });
      }

      actor.send({ type: 'ADD', val: 5 });
      expect(nextCalls.length).toBe(60);
      expect(completed.length).toBe(0);

      // Stop actor
      actor.stop();
      expect(actor.getSnapshot().status).toBe('stopped');
      expect(completed.length).toBe(60);

      // Subsequent send is gated / dropped
      actor.send({ type: 'ADD', val: 100 });
      expect(actor.getSnapshot().context).toBe(15);
      expect(nextCalls.length).toBe(60);

      // Subscribing to an already-stopped actor immediately invokes complete() and returns an unsubscribe no-op
      let lateCompleted = false;
      const lateSub = actor.subscribe({
        next: () => {},
        complete: () => {
          lateCompleted = true;
        }
      });
      expect(lateCompleted).toBe(true);
      expect(typeof lateSub.unsubscribe).toBe('function');
      expect(() => lateSub.unsubscribe()).not.toThrow();
    });

    it('1.4 verifies pre-start subscription behavior and zero pre-start event buffering', () => {
      const logic = fromTransition(
        (s: number, evt: any) => (evt.type === 'ADD' ? s + evt.val : s),
        100
      );

      const actor = createActor(logic);
      const preStartCalls: number[] = [];

      // Subscribe before start
      actor.subscribe((snap: any) => {
        preStartCalls.push(snap.context);
      });

      // Send event before start - Level 1 hard invariant: zero pre-start buffering, events dropped
      actor.send({ type: 'ADD', val: 50 });
      expect(preStartCalls.length).toBe(0);
      expect(actor.getSnapshot().context).toBe(100);

      // Now start the actor
      actor.start();

      // Post-start send should be delivered to the pre-start subscriber
      actor.send({ type: 'ADD', val: 25 });
      expect(actor.getSnapshot().context).toBe(125);
      expect(preStartCalls).toEqual([125]);
    });

    it('1.5 stress tests 500 simultaneous observers with registration, notifications, and cleanup', () => {
      const logic = fromTransition(
        (s: number, evt: any) => (evt.type === 'INC' ? s + 1 : s),
        0
      );
      const actor = createActor(logic).start();

      const numSubs = 500;
      let totalCalls = 0;
      const subs: { unsubscribe: () => void }[] = [];

      for (let i = 0; i < numSubs; i++) {
        subs.push(
          actor.subscribe(() => {
            totalCalls++;
          })
        );
      }

      actor.send({ type: 'INC' });
      expect(totalCalls).toBe(500);

      // Unsubscribe all 500
      for (const sub of subs) {
        sub.unsubscribe();
      }

      // Next send receives 0 notifications
      actor.send({ type: 'INC' });
      expect(totalCalls).toBe(500);
      expect(actor.getSnapshot().context).toBe(2);
    });
  });

  // =========================================================================
  // Dimension 2: Re-entrant Unsubscribe and Re-entrant Send
  // =========================================================================
  describe('Dimension 2: Re-entrancy Stress in fromTransition Observers', () => {
    it('2.1 observer self-unsubscribes during its own next() callback', () => {
      const logic = fromTransition(
        (s: number, evt: any) => (evt.type === 'INC' ? s + 1 : s),
        0
      );
      const actor = createActor(logic).start();

      const observerACalls: number[] = [];
      const observerBCalls: number[] = [];

      let subA: { unsubscribe: () => void };

      subA = actor.subscribe((snap: any) => {
        observerACalls.push(snap.context);
        // Self-unsubscribe on context === 2
        if (snap.context === 2) {
          subA.unsubscribe();
        }
      });

      actor.subscribe((snap: any) => {
        observerBCalls.push(snap.context);
      });

      actor.send({ type: 'INC' }); // 1
      actor.send({ type: 'INC' }); // 2 -> subA self-unsubscribes
      actor.send({ type: 'INC' }); // 3
      actor.send({ type: 'INC' }); // 4

      expect(observerACalls).toEqual([1, 2]);
      expect(observerBCalls).toEqual([1, 2, 3, 4]);
      expect(actor.getSnapshot().context).toBe(4);
    });

    it('2.2 forward sibling unsubscription: Observer A unregisters Observer B during A callback', () => {
      const logic = fromTransition(
        (s: string[], evt: any) => [...s, evt.item],
        []
      );
      const actor = createActor(logic).start();

      const aLogs: string[] = [];
      const bLogs: string[] = [];

      let subB: { unsubscribe: () => void };

      actor.subscribe((snap: any) => {
        aLogs.push(`A:${snap.context.length}`);
        if (snap.context.length === 1 && subB) {
          // A unregisters B during processing of the first event
          subB.unsubscribe();
        }
      });

      subB = actor.subscribe((snap: any) => {
        bLogs.push(`B:${snap.context.length}`);
      });

      // Event 1
      actor.send({ item: 'first' });

      // Event 2
      actor.send({ item: 'second' });

      // In the first send, depending on whether observers are snapshotted or checked live,
      // B was already snapshotted in the dispatch loop of event 1, so it may receive event 1.
      // But for event 2, B is definitively removed!
      expect(aLogs).toEqual(['A:1', 'A:2']);
      // For event 2, B MUST NOT be called:
      expect(bLogs.filter((x) => x === 'B:2').length).toBe(0);
    });

    it('2.3 backward sibling unsubscription: Observer B unregisters Observer A during B callback', () => {
      const logic = fromTransition(
        (s: number, evt: any) => s + evt.val,
        0
      );
      const actor = createActor(logic).start();

      const aLogs: number[] = [];
      const bLogs: number[] = [];

      let subA: { unsubscribe: () => void };

      subA = actor.subscribe((snap: any) => {
        aLogs.push(snap.context);
      });

      actor.subscribe((snap: any) => {
        bLogs.push(snap.context);
        if (snap.context === 10) {
          subA.unsubscribe();
        }
      });

      actor.send({ val: 10 }); // Both receive 10, then B unsubs A
      actor.send({ val: 20 }); // Only B receives 30

      expect(aLogs).toEqual([10]);
      expect(bLogs).toEqual([10, 30]);
    });

    it('2.4 re-entrant send: observer sends event during next() notification', () => {
      const logic = fromTransition(
        (s: { count: number; trace: string[] }, evt: any) => {
          if (evt.type === 'CASCADE') {
            return {
              count: s.count + 1,
              trace: [...s.trace, `CASCADE_${s.count + 1}`]
            };
          }
          if (evt.type === 'START') {
            return {
              count: 1,
              trace: [...s.trace, 'START']
            };
          }
          return s;
        },
        { count: 0, trace: [] }
      );

      const actor = createActor(logic).start();
      const receivedCounts: number[] = [];

      actor.subscribe((snap: any) => {
        receivedCounts.push(snap.context.count);
        // Cascades if count < 4
        if (snap.context.count < 4) {
          actor.send({ type: 'CASCADE' });
        }
      });

      actor.send({ type: 'START' });

      // The cascade should step from 1 -> 2 -> 3 -> 4 safely without RefCell panic
      expect(actor.getSnapshot().context.count).toBe(4);
      expect(receivedCounts.includes(4)).toBe(true);
      expect(actor.getSnapshot().context.trace).toEqual([
        'START',
        'CASCADE_2',
        'CASCADE_3',
        'CASCADE_4'
      ]);
    });

    it('2.5 re-entrant stop: observer calls stop() inside next() callback', () => {
      const logic = fromTransition(
        (s: number, evt: any) => s + 1,
        0
      );
      const actor = createActor(logic).start();

      let stoppedDuringNext = false;
      let completedCalled = false;

      actor.subscribe({
        next: (snap: any) => {
          if (snap.context === 1) {
            stoppedDuringNext = true;
            actor.stop();
          }
        },
        complete: () => {
          completedCalled = true;
        }
      });

      actor.send({ type: 'INC' });

      expect(stoppedDuringNext).toBe(true);
      expect(completedCalled).toBe(true);
      expect(actor.getSnapshot().status).toBe('stopped');
      expect(actor.getSnapshot().context).toBe(1);

      // Send after stop is ignored
      actor.send({ type: 'INC' });
      expect(actor.getSnapshot().context).toBe(1);
    });

    it('2.6 observer throwing error in next() does not abort notification of sibling observers', () => {
      const logic = fromTransition(
        (s: number, evt: any) => s + evt.val,
        0
      );
      const actor = createActor(logic).start();

      const siblingLogs: number[] = [];

      // Observer 1: throws error in next
      actor.subscribe({
        next: () => {
          throw new Error('Hostile observer fault');
        }
      });

      // Observer 2: robust sibling
      actor.subscribe({
        next: (snap: any) => {
          siblingLogs.push(snap.context);
        }
      });

      // Send event - actor should not throw unhandled exception that crashes execution
      expect(() => {
        actor.send({ val: 42 });
      }).not.toThrow();

      expect(actor.getSnapshot().context).toBe(42);
      expect(siblingLogs).toEqual([42]);
    });

    it('2.7 mutually recursive ping-pong cascade between two observers terminates safely', () => {
      const logic = fromTransition(
        (s: { ping: number; pong: number }, evt: any) => {
          if (evt.type === 'PING') return { ...s, ping: s.ping + 1 };
          if (evt.type === 'PONG') return { ...s, pong: s.pong + 1 };
          return s;
        },
        { ping: 0, pong: 0 }
      );
      const actor = createActor(logic).start();

      // Observer 1: responds to PING by sending PONG (up to 3 times)
      actor.subscribe((snap: any) => {
        if (snap.context.ping > snap.context.pong && snap.context.ping <= 3) {
          actor.send({ type: 'PONG' });
        }
      });

      // Observer 2: responds to PONG by sending PING (up to 3 times)
      actor.subscribe((snap: any) => {
        if (snap.context.pong >= snap.context.ping && snap.context.pong < 3) {
          actor.send({ type: 'PING' });
        }
      });

      actor.send({ type: 'PING' });

      expect(actor.getSnapshot().context).toEqual({ ping: 3, pong: 3 });
    });
  });

  // =========================================================================
  // Dimension 3: Defect 2 - Invoke onDone & onError Sequential Actions & Guards
  // =========================================================================
  describe('Dimension 3: Invoke onDone/onError Multiple Sequential Actions and Guards', () => {
    it('3.1 invoke.onDone executes multiple sequential inline actions in strict order and updates context', () => {
      const executionOrder: string[] = [];

      const machine = createMachine({
        id: 'invokeDoneMultiActions',
        initial: 'fetching',
        context: {
          step1Result: '',
          step2Result: '',
          counter: 0
        },
        states: {
          fetching: {
            invoke: {
              id: 'worker',
              src: 'fetchData',
              onDone: {
                target: 'completed',
                actions: [
                  () => {
                    executionOrder.push('action_1_side_effect');
                  },
                  assign({
                    step1Result: ({ event }: any) => `processed_${event.output.id}`,
                    counter: ({ context }: any) => context.counter + 1
                  }),
                  ({ context, event }: any) => {
                    executionOrder.push(`action_2_see_c=${context.counter}_e=${event.output.id}`);
                  },
                  assign({
                    step2Result: ({ context }: any) => `${context.step1Result}_finalized`,
                    counter: ({ context }: any) => context.counter + 10
                  }),
                  ({ context }: any) => {
                    executionOrder.push(`action_3_final_c=${context.counter}`);
                  }
                ]
              }
            }
          },
          completed: {}
        }
      });

      const [s0] = initialTransition(machine);
      expect(s0.value).toBe('fetching');

      const [s1] = transition(machine, s0, {
        type: 'xstate.done.actor.worker',
        output: { id: 'payload_42' }
      });

      expect(s1.value).toBe('completed');
      expect(s1.context.step1Result).toBe('processed_payload_42');
      expect(s1.context.step2Result).toBe('processed_payload_42_finalized');
      expect(s1.context.counter).toBe(11);
      expect(executionOrder).toEqual([
        'action_1_side_effect',
        'action_2_see_c=1_e=payload_42',
        'action_3_final_c=11'
      ]);
    });

    it('3.2 invoke.onDone evaluates multiple guarded branches and selects matching branch with its actions', () => {
      const trace: string[] = [];

      const machine = createMachine({
        id: 'guardedOnDone',
        initial: 'running',
        context: {
          status: 'init',
          result: null as any
        },
        states: {
          running: {
            invoke: {
              id: 'evalWorker',
              src: 'evalLogic',
              onDone: [
                {
                  guard: ({ event }: any) => event.output?.status === 'retry',
                  target: 'retrying',
                  actions: [
                    assign({ status: 'needs_retry' }),
                    () => {
                      trace.push('selected_retry_branch');
                    }
                  ]
                },
                {
                  guard: ({ event }: any) => event.output?.status === 'success',
                  target: 'succeeded',
                  actions: [
                    assign({
                      status: 'ok',
                      result: ({ event }: any) => event.output.data
                    }),
                    () => {
                      trace.push('selected_success_branch');
                    }
                  ]
                },
                {
                  target: 'failed',
                  actions: [
                    assign({ status: 'unknown_failure' }),
                    () => {
                      trace.push('selected_fallback_branch');
                    }
                  ]
                }
              ]
            }
          },
          retrying: {},
          succeeded: {},
          failed: {}
        }
      });

      const [s0] = initialTransition(machine);

      // Branch 1: Retry match
      const [sRetry] = transition(machine, s0, {
        type: 'xstate.done.actor.evalWorker',
        output: { status: 'retry' }
      });
      expect(sRetry.value).toBe('retrying');
      expect(sRetry.context.status).toBe('needs_retry');
      expect(trace).toEqual(['selected_retry_branch']);

      // Branch 2: Success match
      trace.length = 0;
      const [sSuccess] = transition(machine, s0, {
        type: 'xstate.done.actor.evalWorker',
        output: { status: 'success', data: { token: 'xyz' } }
      });
      expect(sSuccess.value).toBe('succeeded');
      expect(sSuccess.context.status).toBe('ok');
      expect(sSuccess.context.result).toEqual({ token: 'xyz' });
      expect(trace).toEqual(['selected_success_branch']);

      // Branch 3: Fallback match
      trace.length = 0;
      const [sFallback] = transition(machine, s0, {
        type: 'xstate.done.actor.evalWorker',
        output: { status: 'other' }
      });
      expect(sFallback.value).toBe('failed');
      expect(sFallback.context.status).toBe('unknown_failure');
      expect(trace).toEqual(['selected_fallback_branch']);
    });

    it('3.3 invoke.onError executes multiple sequential actions and assigns error context', () => {
      const errorLogs: string[] = [];

      const machine = createMachine({
        id: 'invokeErrorMultiActions',
        initial: 'processing',
        context: {
          hasError: false,
          errorMessage: '',
          errorCode: 0
        },
        states: {
          processing: {
            invoke: {
              id: 'failingWorker',
              src: 'faulty',
              onError: {
                target: 'handledError',
                actions: [
                  () => {
                    errorLogs.push('log_error_start');
                  },
                  assign({
                    hasError: true,
                    errorMessage: ({ event }: any) => event.error?.message ?? 'unknown',
                    errorCode: ({ event }: any) => event.error?.code ?? -1
                  }),
                  ({ context }: any) => {
                    errorLogs.push(`logged_code_${context.errorCode}`);
                  }
                ]
              }
            }
          },
          handledError: {}
        }
      });

      const [s0] = initialTransition(machine);
      expect(s0.value).toBe('processing');

      const [sErr] = transition(machine, s0, {
        type: 'xstate.error.actor.failingWorker',
        error: { message: 'Network timeout', code: 504 }
      });

      expect(sErr.value).toBe('handledError');
      expect(sErr.context.hasError).toBe(true);
      expect(sErr.context.errorMessage).toBe('Network timeout');
      expect(sErr.context.errorCode).toBe(504);
      expect(errorLogs).toEqual(['log_error_start', 'logged_code_504']);
    });

    it('3.4 invoke.onError evaluates multiple guarded branches and selects appropriate error handler', () => {
      const trace: string[] = [];

      const machine = createMachine({
        id: 'guardedOnError',
        initial: 'executing',
        context: {
          handledStrategy: ''
        },
        states: {
          executing: {
            invoke: {
              id: 'paymentWorker',
              src: 'pay',
              onError: [
                {
                  guard: ({ event }: any) => event.error?.code === 402,
                  target: 'insufficientFunds',
                  actions: [
                    assign({ handledStrategy: 'funds_declined' }),
                    () => {
                      trace.push('handled_insufficient_funds');
                    }
                  ]
                },
                {
                  guard: ({ event }: any) => event.error?.code === 401,
                  target: 'unauthorized',
                  actions: [
                    assign({ handledStrategy: 're_authenticate' }),
                    () => {
                      trace.push('handled_unauthorized');
                    }
                  ]
                },
                {
                  target: 'genericError',
                  actions: [
                    assign({ handledStrategy: 'crash' }),
                    () => {
                      trace.push('handled_generic');
                    }
                  ]
                }
              ]
            }
          },
          insufficientFunds: {},
          unauthorized: {},
          genericError: {}
        }
      });

      const [s0] = initialTransition(machine);

      // Guard 402
      trace.length = 0;
      const [s402] = transition(machine, s0, {
        type: 'xstate.error.actor.paymentWorker',
        error: { code: 402, message: 'Card declined' }
      });
      expect(s402.value).toBe('insufficientFunds');
      expect(s402.context.handledStrategy).toBe('funds_declined');
      expect(trace).toEqual(['handled_insufficient_funds']);

      // Guard 401
      trace.length = 0;
      const [s401] = transition(machine, s0, {
        type: 'xstate.error.actor.paymentWorker',
        error: { code: 401, message: 'Expired token' }
      });
      expect(s401.value).toBe('unauthorized');
      expect(s401.context.handledStrategy).toBe('re_authenticate');
      expect(trace).toEqual(['handled_unauthorized']);

      // Fallback 500
      trace.length = 0;
      const [s500] = transition(machine, s0, {
        type: 'xstate.error.actor.paymentWorker',
        error: { code: 500, message: 'Internal Server Error' }
      });
      expect(s500.value).toBe('genericError');
      expect(s500.context.handledStrategy).toBe('crash');
      expect(trace).toEqual(['handled_generic']);
    });

    it('3.5 multiple concurrent invokes in a state each have independent onDone/onError action descriptors', () => {
      const trace: string[] = [];

      const machine = createMachine({
        id: 'multiInvokesInState',
        initial: 'waiting',
        context: {
          task1Done: false,
          task2Done: false
        },
        states: {
          waiting: {
            invoke: [
              {
                id: 'task1',
                src: 'task1Src',
                onDone: {
                  actions: [
                    assign({ task1Done: true }),
                    () => {
                      trace.push('task1_completed');
                    }
                  ]
                }
              },
              {
                id: 'task2',
                src: 'task2Src',
                onDone: {
                  actions: [
                    assign({ task2Done: true }),
                    () => {
                      trace.push('task2_completed');
                    }
                  ]
                }
              }
            ]
          }
        }
      });

      const [s0] = initialTransition(machine);
      expect(s0.context).toEqual({ task1Done: false, task2Done: false });

      // Task 1 finishes first
      const [s1] = transition(machine, s0, {
        type: 'xstate.done.actor.task1',
        output: 'res1'
      });
      expect(s1.context.task1Done).toBe(true);
      expect(s1.context.task2Done).toBe(false);
      expect(trace).toEqual(['task1_completed']);

      // Task 2 finishes second
      const [s2] = transition(machine, s1, {
        type: 'xstate.done.actor.task2',
        output: 'res2'
      });
      expect(s2.context.task1Done).toBe(true);
      expect(s2.context.task2Done).toBe(true);
      expect(trace).toEqual(['task1_completed', 'task2_completed']);
    });

    it('3.6 invoke.onDone supports target string and transitions to target state correctly', () => {
      const machine = createMachine({
        id: 'invokeDoneStringTarget',
        initial: 'loading',
        states: {
          loading: {
            invoke: {
              id: 'simpleWorker',
              src: 'fetcher',
              onDone: 'loaded',
              onError: 'failed'
            }
          },
          loaded: {},
          failed: {}
        }
      });

      const [s0] = initialTransition(machine);
      expect(s0.value).toBe('loading');

      const [sDone] = transition(machine, s0, {
        type: 'xstate.done.actor.simpleWorker',
        output: 'ok'
      });
      expect(sDone.value).toBe('loaded');

      const [sErr] = transition(machine, s0, {
        type: 'xstate.error.actor.simpleWorker',
        error: new Error('fail')
      });
      expect(sErr.value).toBe('failed');
    });

    it('3.7 invoke.onDone action triggers raise() to immediately trigger an internal transition', () => {
      const trace: string[] = [];

      const machine = createMachine({
        id: 'invokeDoneRaise',
        initial: 'step1',
        context: { flag: false },
        states: {
          step1: {
            invoke: {
              id: 'calc',
              src: 'calculate',
              onDone: {
                target: 'step2',
                actions: [
                  () => { trace.push('onDoneAction'); },
                  raise({ type: 'INTERNAL_ADVANCE' })
                ]
              }
            }
          },
          step2: {
            on: {
              INTERNAL_ADVANCE: {
                target: 'step3',
                actions: [
                  assign({ flag: true }),
                  () => { trace.push('internalAdvanceAction'); }
                ]
              }
            }
          },
          step3: {}
        }
      });

      const [s0] = initialTransition(machine);
      expect(s0.value).toBe('step1');

      const [s1] = transition(machine, s0, {
        type: 'xstate.done.actor.calc',
        output: 100
      });

      // Because raise({ type: 'INTERNAL_ADVANCE' }) was raised during the transition to step2,
      // the event loop processes the internal event and advances to step3!
      expect(s1.value).toBe('step3');
      expect(s1.context.flag).toBe(true);
      expect(trace).toEqual(['onDoneAction', 'internalAdvanceAction']);
    });

    it('3.8 hierarchical invoke in nested compound state with actions onDone and parent delegation', () => {
      const trace: string[] = [];

      const machine = createMachine({
        id: 'hierarchicalInvoke',
        initial: 'parentState',
        context: {
          data: null as any
        },
        states: {
          parentState: {
            initial: 'childState',
            states: {
              childState: {
                invoke: {
                  id: 'nestedWorker',
                  src: 'nestedWorkerSrc',
                  onDone: {
                    target: '#hierarchicalInvoke.finalState',
                    actions: [
                      assign({ data: ({ event }: any) => event.output }),
                      () => { trace.push('nested_done_action'); }
                    ]
                  }
                }
              }
            }
          },
          finalState: {}
        }
      });

      const [s0] = initialTransition(machine);
      expect(s0.value).toEqual({ parentState: 'childState' });

      const [s1] = transition(machine, s0, {
        type: 'xstate.done.actor.nestedWorker',
        output: { nestedKey: 'nestedVal' }
      });

      expect(s1.value).toBe('finalState');
      expect(s1.context.data).toEqual({ nestedKey: 'nestedVal' });
      expect(trace).toEqual(['nested_done_action']);
    });
  });
});

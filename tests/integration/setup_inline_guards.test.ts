import { describe, it, expect } from 'vitest';
import {
  setup,
  createActor,
  initialTransition,
  transition,
  assign,
  and,
  or,
  not,
} from '../../lavi-wasm/pkg-npm/lavi';

describe('setup().createMachine inline guards bridging', () => {
  describe('1. Actor execution (createActor)', () => {
    it('evaluates standalone inline guards with context in setup().createMachine', () => {
      const machine = setup({
        types: {} as {
          context: { count: number };
          events: { type: 'INCREMENT' } | { type: 'CHECK' };
        },
      }).createMachine({
        id: 'setupInlineGuardActor',
        initial: 'idle',
        context: { count: 0 },
        states: {
          idle: {
            on: {
              INCREMENT: {
                actions: assign({
                  count: ({ context }: any) => context.count + 5,
                }),
              },
              CHECK: [
                {
                  target: 'unlocked',
                  guard: ({ context }: any) => context.count >= 10,
                },
                {
                  target: 'locked',
                },
              ],
            },
          },
          locked: {
            on: {
              RETRY: 'idle',
            },
          },
          unlocked: {},
        },
      });

      // Attempt CHECK at count = 0 (guard returns false -> goes to locked)
      const actor1 = createActor(machine).start();
      actor1.send({ type: 'CHECK' });
      expect(actor1.getSnapshot().value).toBe('locked');

      // Increment twice to count = 10, then CHECK (guard returns true -> goes to unlocked)
      const actor2 = createActor(machine).start();
      actor2.send({ type: 'INCREMENT' });
      actor2.send({ type: 'INCREMENT' });
      expect(actor2.getSnapshot().context.count).toBe(10);
      actor2.send({ type: 'CHECK' });
      expect(actor2.getSnapshot().value).toBe('unlocked');
    });

    it('evaluates inline guards with event payloads in setup().createMachine', () => {
      const machine = setup({}).createMachine({
        id: 'eventPayloadInlineGuard',
        initial: 'idle',
        context: { threshold: 50 },
        states: {
          idle: {
            on: {
              SUBMIT: {
                target: 'accepted',
                guard: ({ context, event }: any) => event.score > context.threshold,
              },
            },
          },
          accepted: {},
        },
      });

      const actor = createActor(machine).start();

      // score <= threshold -> guard fails, stays in idle
      actor.send({ type: 'SUBMIT', score: 30 });
      expect(actor.getSnapshot().value).toBe('idle');

      // score > threshold -> guard passes, transitions to accepted
      actor.send({ type: 'SUBMIT', score: 75 });
      expect(actor.getSnapshot().value).toBe('accepted');
    });

    it('evaluates inline guards nested in combinators (and, or, not) with named guards', () => {
      const machine = setup({
        guards: {
          isSystemReady: ({ context }: any) => context.ready === true,
        },
      }).createMachine({
        id: 'combinatorInlineGuards',
        initial: 'evaluating',
        context: { ready: true, energy: 80, debugMode: false },
        states: {
          evaluating: {
            on: {
              TEST_AND: {
                target: 'andPassed',
                guard: and([
                  'isSystemReady',
                  ({ context }: any) => context.energy >= 50,
                ]),
              },
              TEST_OR: {
                target: 'orPassed',
                guard: or([
                  ({ context }: any) => context.debugMode === true,
                  ({ context }: any) => context.energy > 90,
                ]),
              },
              TEST_NOT: {
                target: 'notPassed',
                guard: not(({ context }: any) => context.energy < 20),
              },
            },
          },
          andPassed: {},
          orPassed: {},
          notPassed: {},
        },
      });

      // and(['isSystemReady', inline energy >= 50]) -> true
      const actor1 = createActor(machine).start();
      actor1.send({ type: 'TEST_AND' });
      expect(actor1.getSnapshot().value).toBe('andPassed');

      // or([debugMode === true, energy > 90]) -> false for energy = 80, debugMode = false
      const actor2 = createActor(machine).start();
      actor2.send({ type: 'TEST_OR' });
      expect(actor2.getSnapshot().value).toBe('evaluating');

      // not(energy < 20) -> not(false) -> true
      const actor3 = createActor(machine).start();
      actor3.send({ type: 'TEST_NOT' });
      expect(actor3.getSnapshot().value).toBe('notPassed');
    });
  });

  describe('2. Pure Stepping Execution (initialTransition & transition)', () => {
    it('evaluates inline guards in pure transitions on setup().createMachine', () => {
      const machine = setup({}).createMachine({
        id: 'pureSteppingInlineGuard',
        initial: 'idle',
        context: { val: 100 },
        states: {
          idle: {
            on: {
              STEP: [
                {
                  target: 'high',
                  guard: ({ context }: any) => context.val >= 50,
                },
                {
                  target: 'low',
                },
              ],
            },
          },
          high: {},
          low: {},
        },
      });

      const [s0] = initialTransition(machine);
      expect(s0.value).toBe('idle');

      // Step with context.val = 100 -> matches high
      const [s1] = transition(machine, s0, { type: 'STEP' });
      expect(s1.value).toBe('high');

      // Step from modified state with context.val = 20 -> matches low
      const customState = { ...s0, context: { val: 20 } };
      const [s2] = transition(machine, customState, { type: 'STEP' });
      expect(s2.value).toBe('low');
    });

    it('evaluates event-dependent inline guards in pure transitions', () => {
      const machine = setup({}).createMachine({
        id: 'pureSteppingEventGuard',
        initial: 'pending',
        context: { allowedCode: 'SECRET' },
        states: {
          pending: {
            on: {
              AUTH: {
                target: 'authenticated',
                guard: ({ context, event }: any) => event.code === context.allowedCode,
              },
            },
          },
          authenticated: {},
        },
      });

      const [s0] = initialTransition(machine);

      // Wrong code -> stays in pending
      const [sFail] = transition(machine, s0, { type: 'AUTH', code: 'WRONG' });
      expect(sFail.value).toBe('pending');

      // Correct code -> transitions to authenticated
      const [sPass] = transition(machine, s0, { type: 'AUTH', code: 'SECRET' });
      expect(sPass.value).toBe('authenticated');
    });
  });

  describe('3. Machine .provide() preservation', () => {
    it('preserves inline guards when machine implementations are provided', () => {
      const baseMachine = setup({
        actions: {
          logAction: () => {},
        },
      }).createMachine({
        id: 'provideMachine',
        initial: 'step1',
        context: { flag: true },
        states: {
          step1: {
            on: {
              NEXT: {
                target: 'step2',
                guard: ({ context }: any) => context.flag === true,
                actions: 'logAction',
              },
            },
          },
          step2: {},
        },
      });

      let actionCalled = false;
      const providedMachine = baseMachine.provide({
        actions: {
          logAction: () => {
            actionCalled = true;
          },
        },
      });

      const actor = createActor(providedMachine).start();
      actor.send({ type: 'NEXT' });
      expect(actor.getSnapshot().value).toBe('step2');
      expect(actionCalled).toBe(true);
    });
  });
});

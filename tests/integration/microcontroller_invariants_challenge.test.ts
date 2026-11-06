import { describe, it, expect, vi } from 'vitest';
import { createMachine, createActor } from '../../portable-xstate-wasm/pkg-npm/portable_xstate';

describe('Level 1 Microcontroller Invariants & Safety Empirical Challenge', () => {
  describe('Zero Pre-Start Buffering Invariant', () => {
    it('strictly drops events sent before actor.start() and does not execute actions or transition upon start', () => {
      const entrySpy = vi.fn();
      const stepActionSpy = vi.fn();

      const machine = createMachine({
        id: 'preStartDropMachine',
        initial: 'initialState',
        states: {
          initialState: {
            entry: entrySpy,
            on: {
              STEP: {
                target: 'secondState',
                actions: stepActionSpy,
              },
            },
          },
          secondState: {},
        },
      });

      const actor = createActor(machine);

      // 1. Send events BEFORE start()
      actor.send({ type: 'STEP' });
      actor.send({ type: 'STEP' });
      actor.send({ type: 'RANDOM_EVENT' });

      // Action spy must not be called
      expect(stepActionSpy).not.toHaveBeenCalled();

      // 2. Start the actor
      actor.start();

      // Entry of initial state must be called on start
      expect(entrySpy).toHaveBeenCalledTimes(1);

      // Pre-start STEP events must NOT have been buffered or replayed
      expect(stepActionSpy).not.toHaveBeenCalled();
      expect(actor.getSnapshot().matches('initialState')).toBe(true);
      expect(actor.getSnapshot().matches('secondState')).toBe(false);

      // 3. Normal event processing works once started
      actor.send({ type: 'STEP' });
      expect(stepActionSpy).toHaveBeenCalledTimes(1);
      expect(actor.getSnapshot().matches('secondState')).toBe(true);
    });

    it('strictly drops pre-start events even when multiple transitions exist', () => {
      let count = 0;
      const machine = createMachine({
        id: 'counterPreStart',
        initial: 'zero',
        states: {
          zero: {
            on: {
              INC: {
                actions: () => {
                  count++;
                },
              },
            },
          },
        },
      });

      const actor = createActor(machine);
      for (let i = 0; i < 50; i++) {
        actor.send({ type: 'INC' });
      }
      expect(count).toBe(0);

      actor.start();
      expect(count).toBe(0);

      actor.send({ type: 'INC' });
      expect(count).toBe(1);
    });
  });

  describe('Post-Stop & Final Actor Event Dropping', () => {
    it('strictly drops events after actor.stop() without mutating state or context', () => {
      let executed = 0;
      const machine = createMachine({
        id: 'stopDropMachine',
        initial: 'step1',
        states: {
          step1: {
            on: {
              ADVANCE: {
                target: 'step2',
                actions: () => {
                  executed++;
                },
              },
            },
          },
          step2: {
            on: {
              ADVANCE: {
                actions: () => {
                  executed++;
                },
              },
            },
          },
        },
      });

      const actor = createActor(machine).start();
      actor.send({ type: 'ADVANCE' });
      expect(actor.getSnapshot().matches('step2')).toBe(true);
      expect(executed).toBe(1);

      // Stop the actor
      actor.stop();
      expect(actor.getSnapshot().status).toBe('stopped');

      // Subsequent events must be dropped
      actor.send({ type: 'ADVANCE' });
      actor.send({ type: 'ADVANCE' });
      expect(executed).toBe(1);
      expect(actor.getSnapshot().matches('step2')).toBe(true);
      expect(actor.getSnapshot().status).toBe('stopped');
    });

    it('strictly drops events after reaching a final state', () => {
      let finalActions = 0;
      const machine = createMachine({
        id: 'finalDropMachine',
        initial: 'working',
        states: {
          working: {
            on: {
              FINISH: 'complete',
            },
          },
          complete: {
            type: 'final',
            on: {
              MORE_WORK: {
                actions: () => {
                  finalActions++;
                },
              },
            },
          },
        },
      });

      const actor = createActor(machine).start();
      actor.send({ type: 'FINISH' });
      expect(actor.getSnapshot().matches('complete')).toBe(true);
      expect(actor.getSnapshot().status).toBe('done');

      actor.send({ type: 'MORE_WORK' });
      expect(finalActions).toBe(0);
      expect(actor.getSnapshot().matches('complete')).toBe(true);
      expect(actor.getSnapshot().status).toBe('done');
    });
  });

  describe('Deep Hierarchy & Bounded Call-Stack Traversal', () => {
    it('executes clean transitions through a 30-level deep nested compound machine without stack overflow', () => {
      const depth = 30;
      // Build 30 nested states: lvl0 -> lvl1 -> ... -> lvl29
      let statesConfig: any = {
        [`lvl_${depth}`]: {
          on: {
            RESET: 'lvl_0',
          },
        },
      };

      for (let i = depth - 1; i >= 0; i--) {
        const nextState = `lvl_${i + 1}`;
        statesConfig[`lvl_${i}`] = {
          on: {
            NEXT: nextState,
          },
        };
      }

      const machine = createMachine({
        id: 'deepCompoundFsm',
        initial: 'lvl_0',
        states: statesConfig,
      });

      const actor = createActor(machine).start();
      expect(actor.getSnapshot().matches('lvl_0')).toBe(true);

      for (let i = 0; i < depth; i++) {
        expect(actor.getSnapshot().matches(`lvl_${i}`)).toBe(true);
        actor.send({ type: 'NEXT' });
      }

      expect(actor.getSnapshot().matches(`lvl_${depth}`)).toBe(true);

      // Snapshot roundtrip at max depth
      const snapshot = actor.getPersistedSnapshot();
      expect(snapshot.status).toBe('active');

      const restored = createActor(machine, { snapshot }).start();
      expect(restored.getSnapshot().matches(`lvl_${depth}`)).toBe(true);

      restored.send({ type: 'RESET' });
      expect(restored.getSnapshot().matches('lvl_0')).toBe(true);
    });

    it('correctly inspects 10 parallel regions without stack issues', () => {
      const regionsCount = 10;
      const statesObj: any = {};
      for (let r = 0; r < regionsCount; r++) {
        statesObj[`region_${r}`] = {
          initial: 'off',
          states: {
            off: { on: { TOGGLE: 'on' } },
            on: { on: { TOGGLE: 'off' } },
          },
        };
      }

      const machine = createMachine({
        id: 'parallelChallenge',
        type: 'parallel',
        states: statesObj,
      });

      const actor = createActor(machine).start();

      for (let r = 0; r < regionsCount; r++) {
        expect(actor.getSnapshot().matches(`region_${r}.off`)).toBe(true);
      }

      actor.send({ type: 'TOGGLE' });

      for (let r = 0; r < regionsCount; r++) {
        expect(actor.getSnapshot().matches(`region_${r}.on`)).toBe(true);
      }

      const snap = actor.getPersistedSnapshot();
      const restored = createActor(machine, { snapshot: snap }).start();

      for (let r = 0; r < regionsCount; r++) {
        expect(restored.getSnapshot().matches(`region_${r}.on`)).toBe(true);
      }
    });
  });

  describe('Pathological Event Payloads', () => {
    it('handles non-standard event objects gracefully without crashing', () => {
      const machine = createMachine({
        id: 'pathologicalEvents',
        initial: 'idle',
        states: {
          idle: {
            on: {
              NORMAL: 'active',
            },
          },
          active: {},
        },
      });

      const actor = createActor(machine).start();

      // String event
      actor.send('IRRELEVANT' as any);
      expect(actor.getSnapshot().matches('idle')).toBe(true);

      // Empty object
      actor.send({} as any);
      expect(actor.getSnapshot().matches('idle')).toBe(true);

      // Event with missing type
      actor.send({ foo: 'bar' } as any);
      expect(actor.getSnapshot().matches('idle')).toBe(true);

      // Valid transition
      actor.send({ type: 'NORMAL' });
      expect(actor.getSnapshot().matches('active')).toBe(true);
    });
  });
});

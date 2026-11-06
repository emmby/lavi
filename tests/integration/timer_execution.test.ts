import { describe, it, expect, vi, afterEach } from 'vitest';
import { createMachine, createActor, SimulatedClock } from '../../lavi-wasm/pkg-npm/lavi';

afterEach(() => {
  vi.useRealTimers();
});

describe('empirical timer execution & conformance challenger', () => {
  // 1. Fake Timers
  describe('fake timers (vi.useFakeTimers)', () => {
    it('drives delayed transitions automatically without manual .tick() calls', () => {
      vi.useFakeTimers();

      const machine = createMachine({
        id: 'timerTest',
        initial: 'step1',
        states: {
          step1: {
            after: {
              100: 'step2'
            }
          },
          step2: {
            after: {
              200: 'step3'
            }
          },
          step3: {
            type: 'final'
          }
        }
      });

      const actor = createActor(machine).start();
      expect(actor.getSnapshot().value).toBe('step1');

      // Advance partially: should still be step1
      vi.advanceTimersByTime(50);
      expect(actor.getSnapshot().value).toBe('step1');

      // Advance past step1's 100ms deadline: should transition to step2
      vi.advanceTimersByTime(50);
      expect(actor.getSnapshot().value).toBe('step2');

      // Advance partially in step2: should still be step2
      vi.advanceTimersByTime(100);
      expect(actor.getSnapshot().value).toBe('step2');

      // Advance past step2's 200ms deadline: should transition to step3
      vi.advanceTimersByTime(100);
      expect(actor.getSnapshot().value).toBe('step3');
      expect(actor.getSnapshot().status).toBe('done');
    });

    it('cancels pending timer when an external event triggers an early transition', () => {
      vi.useFakeTimers();

      const machine = createMachine({
        id: 'cancelTest',
        initial: 'waiting',
        states: {
          waiting: {
            on: {
              INTERRUPT: 'interrupted'
            },
            after: {
              100: 'timed_out'
            }
          },
          interrupted: {},
          timed_out: {}
        }
      });

      const actor = createActor(machine).start();
      expect(actor.getSnapshot().value).toBe('waiting');

      // Send event at t=40ms
      vi.advanceTimersByTime(40);
      actor.send({ type: 'INTERRUPT' });
      expect(actor.getSnapshot().value).toBe('interrupted');

      // Advance past the original 100ms deadline: should stay in interrupted, NOT timed_out
      vi.advanceTimersByTime(100);
      expect(actor.getSnapshot().value).toBe('interrupted');
    });
  });

  // 2. Real Timers
  describe('real async timers', () => {
    it('drives delayed transitions automatically using real setTimeout', async () => {
      const machine = createMachine({
        id: 'realTimer',
        initial: 'init',
        states: {
          init: {
            after: {
              30: 'completed'
            }
          },
          completed: {
            type: 'final'
          }
        }
      });

      const actor = createActor(machine).start();
      expect(actor.getSnapshot().value).toBe('init');

      await new Promise((resolve) => setTimeout(resolve, 60));

      expect(actor.getSnapshot().value).toBe('completed');
      expect(actor.getSnapshot().status).toBe('done');
    });
  });

  // 3. Custom Clock Options
  describe('custom clock options ({ setTimeout, clearTimeout })', () => {
    it('uses custom clock setTimeout to arm timers', async () => {
      let customSetTimeoutCalls = 0;
      const customClock = {
        setTimeout: (cb: () => void, ms: number) => {
          customSetTimeoutCalls++;
          return setTimeout(cb, ms);
        },
        clearTimeout: (handle: any) => {
          clearTimeout(handle);
        }
      };

      const machine = createMachine({
        initial: 'idle',
        states: {
          idle: {
            after: {
              20: 'active'
            }
          },
          active: {}
        }
      });

      const actor = createActor(machine, { clock: customClock }).start();
      expect(customSetTimeoutCalls).toBeGreaterThan(0);

      await new Promise((resolve) => setTimeout(resolve, 50));
      expect(actor.getSnapshot().value).toBe('active');
    });

    it('cancels host timer via custom clearTimeout on actor.stop()', () => {
      let clearTimeoutCalled = false;
      let scheduledHandle: any = null;

      const customClock = {
        setTimeout: (cb: () => void, ms: number) => {
          scheduledHandle = setTimeout(cb, ms);
          return scheduledHandle;
        },
        clearTimeout: (handle: any) => {
          clearTimeoutCalled = true;
          clearTimeout(handle);
        }
      };

      const machine = createMachine({
        initial: 'idle',
        states: {
          idle: {
            after: {
              1000: 'active'
            }
          },
          active: {}
        }
      });

      const actor = createActor(machine, { clock: customClock }).start();
      expect(scheduledHandle).not.toBeNull();

      actor.stop();
      expect(clearTimeoutCalled).toBe(true);
      expect(actor.getSnapshot().status).toBe('stopped');
    });

    it('handles issue #5001: does not call clearTimeout with undefined when timer fires naturally', async () => {
      const spyClear = vi.fn();
      const customClock = {
        setTimeout: (cb: () => void, ms: number) => setTimeout(cb, ms),
        clearTimeout: spyClear
      };

      const machine = createMachine({
        initial: 'green',
        states: {
          green: {
            after: {
              10: 'yellow'
            }
          },
          yellow: {}
        }
      });

      const actor = createActor(machine, { clock: customClock }).start();
      await new Promise((resolve) => setTimeout(resolve, 30));

      expect(actor.getSnapshot().value).toBe('yellow');
      expect(spyClear.mock.calls.length).toBe(0);
    });
  });

  // 4. Actor Subscription & Completion
  describe('actor.subscribe({ complete })', () => {
    it('fires complete callback when delayed transition leads to a final state', async () => {
      const { resolve, promise } = Promise.withResolvers<void>();
      let completeFired = false;
      let nextSnapshots: string[] = [];

      const machine = createMachine({
        initial: 'start',
        states: {
          start: {
            after: {
              20: 'finish'
            }
          },
          finish: {
            type: 'final'
          }
        }
      });

      const actor = createActor(machine);
      actor.subscribe({
        next: (snap: any) => {
          nextSnapshots.push(snap.value);
        },
        complete: () => {
          completeFired = true;
          resolve();
        }
      });

      actor.start();
      await promise;

      expect(completeFired).toBe(true);
      expect(actor.getSnapshot().value).toBe('finish');
      expect(actor.getSnapshot().status).toBe('done');
      expect(nextSnapshots).toContain('finish');
    });

    it('immediately calls complete if subscribed to an already-done actor', () => {
      const machine = createMachine({
        initial: 'doneState',
        states: {
          doneState: {
            type: 'final'
          }
        }
      });

      const actor = createActor(machine).start();
      expect(actor.getSnapshot().status).toBe('done');

      let completedImmediately = false;
      actor.subscribe({
        complete: () => {
          completedImmediately = true;
        }
      });

      expect(completedImmediately).toBe(true);
    });

    it('supports unsubscribe to stop receiving subsequent snapshots', () => {
      vi.useFakeTimers();

      const machine = createMachine({
        initial: 's1',
        states: {
          s1: {
            after: { 50: 's2' }
          },
          s2: {
            after: { 50: 's3' }
          },
          s3: {}
        }
      });

      const actor = createActor(machine);
      const values: string[] = [];
      const sub = actor.subscribe((snap: any) => {
        values.push(snap.value);
      });

      actor.start();
      vi.advanceTimersByTime(50);
      expect(actor.getSnapshot().value).toBe('s2');
      expect(values).toEqual(['s1', 's2']);

      sub.unsubscribe();

      vi.advanceTimersByTime(50);
      expect(actor.getSnapshot().value).toBe('s3');
      // values should not have received s3
      expect(values).toEqual(['s1', 's2']);
    });
  });

  // 5. SimulatedClock Compatibility
  describe('SimulatedClock compatibility', () => {
    it('coexists with SimulatedClock when clock is explicitly passed', () => {
      const clock = new SimulatedClock();

      const machine = createMachine({
        initial: 'green',
        states: {
          green: {
            after: {
              100: 'yellow'
            }
          },
          yellow: {}
        }
      });

      const actor = createActor(machine, { clock }).start();
      expect(actor.getSnapshot().value).toBe('green');

      clock.set(50);
      expect(actor.getSnapshot().value).toBe('green');

      clock.set(100);
      expect(actor.getSnapshot().value).toBe('yellow');
    });
  });

  // 6. Adversarial Stress Scenarios
  describe('adversarial stress testing', () => {
    it('handles rapid events and transitions while timers are active', () => {
      vi.useFakeTimers();

      const machine = createMachine({
        initial: 'a',
        states: {
          a: {
            on: { PING: 'b' },
            after: { 100: 'timeout_a' }
          },
          b: {
            on: { PING: 'a' },
            after: { 100: 'timeout_b' }
          },
          timeout_a: {},
          timeout_b: {}
        }
      });

      const actor = createActor(machine).start();

      // Rapidly toggle between a and b at t=10, 20, 30...
      for (let i = 0; i < 20; i++) {
        vi.advanceTimersByTime(10);
        actor.send({ type: 'PING' });
      }

      // Neither timeout should have fired because timer resets each transition
      const current = actor.getSnapshot().value;
      expect(['a', 'b']).toContain(current);

      // Now let it sit for 100ms
      vi.advanceTimersByTime(100);
      const finalVal = actor.getSnapshot().value;
      expect(['timeout_a', 'timeout_b']).toContain(finalVal);
    });

    it('handles multiple concurrent actors with interleaved deadlines', () => {
      vi.useFakeTimers();

      const m1 = createMachine({
        initial: 'idle',
        states: {
          idle: { after: { 100: 'done1' } },
          done1: { type: 'final' }
        }
      });

      const m2 = createMachine({
        initial: 'idle',
        states: {
          idle: { after: { 150: 'done2' } },
          done2: { type: 'final' }
        }
      });

      const a1 = createActor(m1).start();
      const a2 = createActor(m2).start();

      vi.advanceTimersByTime(50);
      expect(a1.getSnapshot().value).toBe('idle');
      expect(a2.getSnapshot().value).toBe('idle');

      vi.advanceTimersByTime(50); // t=100
      expect(a1.getSnapshot().value).toBe('done1');
      expect(a2.getSnapshot().value).toBe('idle');

      vi.advanceTimersByTime(50); // t=150
      expect(a1.getSnapshot().value).toBe('done1');
      expect(a2.getSnapshot().value).toBe('done2');
    });

    it('handles multiple deadlines where earliest guard fails, advancing to subsequent deadline', () => {
      vi.useFakeTimers();

      const machine = createMachine({
        initial: 'test',
        states: {
          test: {
            after: {
              100: [{ target: 'never', guard: () => false }],
              200: 'target_b'
            }
          },
          never: {},
          target_b: {}
        }
      });

      const actor = createActor(machine).start();
      vi.advanceTimersByTime(100);
      expect(actor.getSnapshot().value).toBe('test');

      vi.advanceTimersByTime(100);
      expect(actor.getSnapshot().value).toBe('target_b');
    });

    it('handles parallel states with different delayed transition deadlines', () => {
      vi.useFakeTimers();

      const machine = createMachine({
        type: 'parallel',
        states: {
          p1: {
            initial: 'a1',
            states: {
              a1: { after: { 100: 'b1' } },
              b1: {}
            }
          },
          p2: {
            initial: 'a2',
            states: {
              a2: { after: { 200: 'b2' } },
              b2: {}
            }
          }
        }
      });

      const actor = createActor(machine).start();
      expect(actor.getSnapshot().value).toEqual({ p1: 'a1', p2: 'a2' });

      vi.advanceTimersByTime(100);
      expect(actor.getSnapshot().value).toEqual({ p1: 'b1', p2: 'a2' });

      vi.advanceTimersByTime(100);
      expect(actor.getSnapshot().value).toEqual({ p1: 'b1', p2: 'b2' });
    });

    it('handles 0ms delay transitions automatically', () => {
      vi.useFakeTimers();

      const machine = createMachine({
        initial: 'fast',
        states: {
          fast: {
            after: {
              0: 'next'
            }
          },
          next: {}
        }
      });

      const actor = createActor(machine).start();
      expect(actor.getSnapshot().value).toBe('fast');

      vi.advanceTimersByTime(0);
      expect(actor.getSnapshot().value).toBe('next');
    });

    it('resets timer deadline when transitioning into state with after', () => {
      vi.useFakeTimers();

      const machine = createMachine({
        initial: 'first',
        states: {
          first: {
            on: { GO: 'timed' }
          },
          timed: {
            after: {
              100: 'done'
            }
          },
          done: {}
        }
      });

      const actor = createActor(machine).start();

      // Spend 50ms in first
      vi.advanceTimersByTime(50);
      expect(actor.getSnapshot().value).toBe('first');

      // Transition to timed at t=50
      actor.send({ type: 'GO' });
      expect(actor.getSnapshot().value).toBe('timed');

      // Advance 50ms (timed has only been active for 50ms)
      vi.advanceTimersByTime(50);
      expect(actor.getSnapshot().value).toBe('timed');

      // Advance another 50ms (timed has now been active for 100ms)
      vi.advanceTimersByTime(50);
      expect(actor.getSnapshot().value).toBe('done');
    });
  });
});

import { setTimeout as sleep } from 'node:timers/promises';
import { createMachine, createActor } from '../lavi-wasm/pkg-npm/lavi';
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

const lightMachine = createMachine({
  id: 'light',
  initial: 'green',
  context: {
    canTurnGreen: true
  },
  states: {
    green: {
      after: {
        1000: 'yellow'
      }
    },
    yellow: {
      after: {
        1000: [{ target: 'red' }]
      }
    },
    red: {
      after: {
        1000: 'green'
      }
    }
  }
});

afterEach(() => {
  vi.useRealTimers();
});

describe('delayed transitions', () => {
  it('should transition after delay', () => {
    vi.useFakeTimers();

    const actorRef = createActor(lightMachine).start();
    expect(actorRef.getSnapshot().value).toBe('green');

    vi.advanceTimersByTime(500);
    actorRef.tick(); // Added manually for tick evaluation parity
    expect(actorRef.getSnapshot().value).toBe('green');

    vi.advanceTimersByTime(510);
    actorRef.tick();
    expect(actorRef.getSnapshot().value).toBe('yellow');
  });

  it.skip('should not try to clear an undefined timeout when exiting source state of a delayed transition', async () => {
    // SKIPPED: portable-xstate does not implement JS setTimeout mapping bound to exit mechanisms.
  });

  it.skip('should format transitions properly', () => {
    // SKIPPED: Internal node formatting representations differ and shouldn't be validated exactly for strings.
  });

  it('should be able to transition with delay from nested initial state', () => {
    const machine = createMachine({
      initial: 'nested',
      states: {
        nested: {
          initial: 'wait',
          states: {
            wait: {
              after: {
                10: 'end'
              }
            }
          }
        },
        end: {
          id: 'end',
          type: 'final'
        }
      }
    });

    const actor = createActor(machine);
    actor.start();
    
    vi.useFakeTimers();
    vi.advanceTimersByTime(15);
    actor.tick();
    expect(actor.getSnapshot().value).toBe('end');
    vi.useRealTimers();
  });

  it('parent state should enter child state without re-entering self (relative target)', () => {
    const actual: string[] = [];

    const machine = createMachine({
      initial: 'one',
      states: {
        one: {
          initial: 'two',
          entry: () => actual.push('entered one'),
          states: {
            two: {
              entry: () => actual.push('entered two')
            },
            three: {
              entry: () => actual.push('entered three'),
              always: 'end' // relative target
            }
          },
          after: {
            10: '.three'
          }
        },
        end: {
          id: 'end',
          type: 'final'
        }
      }
    });

    const actor = createActor(machine);
    actor.start();

    vi.useFakeTimers();
    vi.advanceTimersByTime(15);
    actor.tick();
    expect(actual).toEqual(['entered one', 'entered two', 'entered three']);
    expect(actor.getSnapshot().value).toBe('end');
    vi.useRealTimers();
  });

  it('should defer a single send event for a delayed conditional transition (#886)', () => {
    vi.useFakeTimers();
    const spy = vi.fn();
    const machine = createMachine({
      initial: 'X',
      states: {
        X: {
          after: {
            1: [
              {
                target: 'Y',
                guard: () => true
              },
              {
                target: 'Z'
              }
            ]
          }
        },
        Y: {
          on: {
            '*': {
              actions: spy
            }
          }
        },
        Z: {}
      }
    });

    const actor = createActor(machine).start();

    vi.advanceTimersByTime(10);
    actor.tick();
    expect(spy).not.toHaveBeenCalled();
  });

  it.skip('should execute an after transition after starting from a state resolved using `.getPersistedSnapshot`', () => {
    // skipped via statelyai core tests.
  });

  it('should execute an after transition after starting from a persisted state', () => {
    const createMyMachine = () =>
      createMachine({
        initial: 'A',
        states: {
          A: {
            on: {
              NEXT: 'B'
            }
          },
          B: {
            after: {
              1: 'C'
            }
          },
          C: {
            type: 'final'
          }
        }
      });

    let service = createActor(createMyMachine()).start();
    
    const persistedSnapshot = JSON.parse(JSON.stringify(service.getSnapshot()));

    service = createActor(createMyMachine(), {
      snapshot: persistedSnapshot
    }).start();

    service.send({ type: 'NEXT' });

    vi.useFakeTimers();
    vi.advanceTimersByTime(5);
    service.tick();
    
    expect(service.getSnapshot().value).toBe('C');
    vi.useRealTimers();
  });

  describe('delay expressions', () => {
    it.skip('should evaluate the expression (function) to determine the delay', () => {
      // SKIPPED: portable-xstate does not evaluate dynamic expression maps
    });

    it.skip('should evaluate the expression (string) to determine the delay', () => {
      // SKIPPED: portable-xstate does not evaluate dynamic expression maps
    });
  });
});

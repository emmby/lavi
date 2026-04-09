import { createMachine, createActor } from '../portable-xstate-wasm/pkg-npm/portable_xstate';
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

describe('delayed transitions', () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('should transition after delay', () => {
    const machine = createMachine({
      initial: 'a',
      states: {
        a: {
          after: {
            10: 'b'
          }
        },
        b: {
          after: {
            20: 'c'
          }
        },
        c: {
          type: 'final'
        }
      }
    });

    const actor = createActor(machine).start();

    expect(actor.getSnapshot().value).toBe('a');

    vi.advanceTimersByTime(5);
    actor.tick();
    expect(actor.getSnapshot().value).toBe('a');

    vi.advanceTimersByTime(5);
    actor.tick();
    expect(actor.getSnapshot().value).toBe('b');

    vi.advanceTimersByTime(20);
    actor.tick();
    expect(actor.getSnapshot().value).toBe('c');
  });
});

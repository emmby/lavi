import { describe, it, expect } from 'vitest';
import { createMachine, createActor } from '../../portable-xstate-wasm/pkg-npm/portable_xstate';

describe('snapshot status round-trip parity across actor states', () => {
  const machine = createMachine({
    id: 'statusParityMachine',
    initial: 'idle',
    states: {
      idle: {
        on: {
          START: 'active_step1',
          FINISH_IMMEDIATELY: 'done_state'
        }
      },
      active_step1: {
        on: {
          NEXT: 'active_step2',
          FINISH: 'done_state'
        }
      },
      active_step2: {
        on: {
          FINISH: 'done_state'
        }
      },
      done_state: {
        type: 'final'
      }
    }
  });

  it('restores snapshot of an active machine: preserves status "active" and responds to events', () => {
    const actor = createActor(machine).start();
    actor.send({ type: 'START' });
    actor.send({ type: 'NEXT' });
    expect(actor.getSnapshot().matches('active_step2')).toBe(true);
    expect(actor.getSnapshot().status).toBe('active');

    const snap = actor.getPersistedSnapshot();
    expect(snap.status).toBe('active');

    // Restore snapshot into fresh actor
    const restored = createActor(machine, { snapshot: snap }).start();
    expect(restored.getSnapshot().status).toBe('active');
    expect(restored.getPersistedSnapshot().status).toBe('active');
    expect(restored.getSnapshot().matches('active_step2')).toBe(true);

    // Verify it responds to events and transitions to final
    restored.send({ type: 'FINISH' });
    expect(restored.getSnapshot().matches('done_state')).toBe(true);
    expect(restored.getSnapshot().status).toBe('done');
    expect(restored.getPersistedSnapshot().status).toBe('done');
  });

  it('restores snapshot of a done/final machine: preserves status "done" and ignores subsequent events', () => {
    const actor = createActor(machine).start();
    actor.send({ type: 'FINISH_IMMEDIATELY' });
    expect(actor.getSnapshot().matches('done_state')).toBe(true);
    expect(actor.getSnapshot().status).toBe('done');

    const doneSnap = actor.getPersistedSnapshot();
    expect(doneSnap.status).toBe('done');

    // Calling stop() on a done actor must preserve done
    actor.stop();
    expect(actor.getSnapshot().status).toBe('done');
    expect(actor.getPersistedSnapshot().status).toBe('done');

    // Restore snapshot into fresh actor
    const restored = createActor(machine, { snapshot: doneSnap }).start();
    expect(restored.getSnapshot().status).toBe('done');
    expect(restored.getPersistedSnapshot().status).toBe('done');
    expect(restored.getSnapshot().matches('done_state')).toBe(true);

    // Verify subsequent events are ignored
    restored.send({ type: 'START' });
    expect(restored.getSnapshot().matches('done_state')).toBe(true);
    expect(restored.getSnapshot().status).toBe('done');
    expect(restored.getPersistedSnapshot().status).toBe('done');
  });

  it('restores snapshot of a stopped machine: preserves status "stopped" and drops events', () => {
    const actor = createActor(machine).start();
    actor.send({ type: 'START' });
    actor.stop();
    expect(actor.getSnapshot().matches('active_step1')).toBe(true);
    expect(actor.getSnapshot().status).toBe('stopped');

    const stoppedSnap = actor.getPersistedSnapshot();
    expect(stoppedSnap.status).toBe('stopped');

    // Restore snapshot into fresh actor
    const restored = createActor(machine, { snapshot: stoppedSnap }).start();
    expect(restored.getSnapshot().status).toBe('stopped');
    expect(restored.getPersistedSnapshot().status).toBe('stopped');
    expect(restored.getSnapshot().matches('active_step1')).toBe(true);

    // Verify subsequent events are dropped
    restored.send({ type: 'NEXT' });
    expect(restored.getSnapshot().matches('active_step1')).toBe(true);
    expect(restored.getSnapshot().status).toBe('stopped');
    expect(restored.getPersistedSnapshot().status).toBe('stopped');
  });

  it('maintains round-trip snapshot idempotence across multiple persistence cycles', () => {
    // Done state idempotence
    const actor = createActor(machine).start();
    actor.send({ type: 'FINISH_IMMEDIATELY' });
    const snap1 = actor.getPersistedSnapshot();

    const restored1 = createActor(machine, { snapshot: snap1 });
    const snap2 = restored1.getPersistedSnapshot();
    expect(snap2.status).toBe('done');
    expect(snap2.value).toEqual(snap1.value);

    const restored2 = createActor(machine, { snapshot: snap2 }).start();
    const snap3 = restored2.getPersistedSnapshot();
    expect(snap3.status).toBe('done');
    expect(snap3.value).toEqual(snap1.value);
  });
});

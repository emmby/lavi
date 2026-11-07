import assert from 'node:assert/strict';
// In a standalone project using the published package:
// import { createMachine, createActor, SimulatedClock } from 'lavi-fsm';
import { createMachine, createActor, SimulatedClock } from '../../lavi-wasm/pkg-npm/lavi.js';

let transitions = 0;

const machine = createMachine(
  {
    id: 'stoplight',
    initial: 'red',
    context: { cycles: 1 },
    states: {
      red: {
        entry: ['onRed'],
        after: { 1000: 'green' }
      },
      green: {
        entry: ['onGreen'],
        on: {
          PEDESTRIAN_WAIT: {
            target: 'yellow',
            guard: 'canCross'
          }
        },
        after: { 1000: 'yellow' }
      },
      yellow: {
        entry: ['onYellow'],
        after: { 1000: 'red' }
      }
    }
  },
  {
    actions: {
      onRed: () => { transitions += 1; },
      onGreen: () => { transitions += 1; },
      onYellow: () => { transitions += 1; }
    },
    guards: {
      canCross: ({ context }) => context.cycles >= 1
    }
  }
);

const clock = new SimulatedClock();
const actor = createActor(machine, { clock });

// 1. Start in "red"
actor.start();
assert.equal(actor.getSnapshot().matches('red'), true);
assert.equal(transitions, 1);

// 2. Advance 1000ms -> "green" (via `after: 1000`)
clock.increment(1000);
assert.equal(actor.getSnapshot().matches('green'), true);
assert.equal(transitions, 2);

// 3. Send guarded "PEDESTRIAN_WAIT" -> "yellow"
actor.send({ type: 'PEDESTRIAN_WAIT' });
assert.equal(actor.getSnapshot().matches('yellow'), true);
assert.equal(transitions, 3);

// 4. Advance 1000ms -> "red" (via `after: 1000`)
clock.increment(1000);
assert.equal(actor.getSnapshot().matches('red'), true);
assert.equal(transitions, 4);

console.log(`JS/WASM example passed (transitions: ${transitions})`);

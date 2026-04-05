import { createMachine } from '../portable-xstate-wasm/pkg-npm/portable_xstate_wasm.js';
import { trackEntries } from './utils.ts';
import { createActor } from '../portable-xstate-wasm/pkg-npm/portable_xstate_wasm.js';

function viFn() { return function() {} }
const entrySpy = viFn();
const exitSpy = viFn();
const transitionSpy = viFn();

const machine = createMachine({
initial: 'a',
states: {
    a: {
    initial: 'a1',
    states: {
        a1: {
        on: {
            NEXT_FN: 'a3'
        }
        },
        a2: {},
        a3: {
        on: {
            NEXT: {
            target: 'a2',
            actions: [transitionSpy]
            }
        },
        entry: entrySpy,
        exit: exitSpy
        }
    }
    }
}
});

const flushTracked = trackEntries(machine);

const actor = createActor(machine).start();
flushTracked();

console.log("SENDING NEXT_FN");
actor.send({ type: 'NEXT_FN' });

console.log("FLUSHED:", flushTracked());


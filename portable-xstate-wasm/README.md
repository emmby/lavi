# Portable XState WASM

WebAssembly bindings for Portable XState, enabling XState-compatible state machines to run in the browser or Node.js via WASM.

## Features

- **XState Compatibility**: Exposes an API similar to XState (`createMachine`, `createActor`, `assign`).
- **Performance**: Runs logic in compiled Rust/WASM.
- **Interoperability**: seamlessly converts between JavaScript Objects and interior Rust `Value` types.

## Usage

```javascript
import { createMachine, createActor, assign } from 'portable-xstate-wasm';

const machine = createMachine({
  id: 'toggle',
  initial: 'inactive',
  states: {
    inactive: { on: { TOGGLE: 'active' } },
    active: { on: { TOGGLE: 'inactive' } }
  }
});

const actor = createActor(machine);
actor.start();
actor.send({ type: 'TOGGLE' });
console.log(actor.getSnapshot().value); // "active"
```

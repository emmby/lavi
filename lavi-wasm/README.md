# Lavi WASM (`lavi-wasm`)

WebAssembly bindings for **Lavi**, enabling XState v5-compatible statecharts to run in the browser or Node.js via WebAssembly.

## Features

- **XState v5 API Compatibility**: Exposes `createMachine`, `createActor`, `setup`, `assign`, `assertEvent`, and `SimulatedClock`.
- **Inline Callback Bridging**: Seamlessly bridges inline JavaScript actions, guards, and `assign` functions (`js_sys::Function`) to named runtime callbacks on the Rust core.

## Usage

```javascript
// TODO(flowos: e9505536-8f94-4c57-8de4-944e1257ce7c): Publish `lavi-wasm` to npm
import { createMachine, createActor, assign } from 'lavi-wasm';
// (Or import from './lavi-wasm/pkg-npm/lavi.js' when running from a local repo build)

const machine = createMachine({
  id: 'toggle',
  initial: 'inactive',
  states: {
    inactive: { on: { TOGGLE: 'active' } },
    active: { on: { TOGGLE: 'inactive' } }
  }
});

const actor = createActor(machine).start();
actor.send({ type: 'TOGGLE' });
console.log(actor.getSnapshot().value); // "active"
```

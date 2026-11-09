# Lavi WASM (`lavi-fsm`)

WebAssembly bindings for **Lavi**, enabling XState v5-compatible statecharts to run in Node.js, browser bundlers (Vite, Webpack, Rollup, esbuild), and direct browser ES modules via WebAssembly.

## Installation

```bash
npm install lavi-fsm
```

## Features

- **XState v5 API Compatibility**: Exposes `createMachine`, `createActor`, `setup`, `assign`, `raise`, `cancel`, `enqueueActions`, `isMachineSnapshot`, `assertEvent`, `inspect` / `actor.system.inspect`, and `SimulatedClock`.
- **Inline Callback Bridging**: Seamlessly bridges inline JavaScript actions, guards, and `assign` functions (`js_sys::Function`) to named runtime callbacks on the Rust core.
- **Universal Module Exports**:
  - `import { createMachine, createActor } from 'lavi-fsm'` works out-of-the-box in Node.js and browser bundlers.
  - `import init, { createMachine, createActor } from 'lavi-fsm/web'` supports direct `<script type="module">` usage in browsers.

## Usage

```javascript
import { createMachine, createActor, assign } from 'lavi-fsm';

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

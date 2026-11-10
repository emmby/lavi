# Lavi WASM (`lavi-fsm`)

JavaScript, TypeScript, React, and React Native bindings for **Lavi**, enabling XState v5-compatible statecharts to run in Node.js, browser bundlers (Vite, Webpack, Rollup, esbuild), direct browser ES modules, and **React Native / Expo (Hermes & JavaScriptCore)**.

## Installation

```bash
npm install lavi-fsm
```

## Features

- **XState v5 API Compatibility**: Exposes `createMachine`, `createActor`, `setup`, `assign`, `raise`, `cancel`, `enqueueActions`, `isMachineSnapshot`, `assertEvent`, `inspect` / `actor.system.inspect`, and `SimulatedClock`.
- **Inline Callback Bridging**: Seamlessly bridges inline JavaScript actions, guards, and `assign` functions (`js_sys::Function`) to named runtime callbacks on the Rust core.
- **React & React Native Hooks (`lavi-fsm/react`)**: Built-in `useMachine`, `useActor`, `useActorRef`, and `useSelector` hooks powered by React 18+'s `useSyncExternalStore`.
- **Universal Module Exports**:
  - `import { createMachine, createActor } from 'lavi-fsm'` works out-of-the-box in Node.js, browser bundlers, and **React Native / Expo (Metro)** (via a precompiled `wasm2js` entrypoint requiring zero `WebAssembly` global or native linking).
  - `import { useMachine, useActor, useActorRef, useSelector } from 'lavi-fsm/react'` provides idiomatic statechart hooks for React Web and React Native components.
  - `import init, { createMachine, createActor } from 'lavi-fsm/web'` supports direct `<script type="module">` usage in browsers.

## Usage

### Core (`lavi-fsm`)

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

### React & React Native (`lavi-fsm/react`)

```tsx
import { createMachine, assign } from 'lavi-fsm';
import { useMachine, useSelector } from 'lavi-fsm/react';

const counterMachine = createMachine({
  id: 'counter',
  initial: 'active',
  context: { count: 0 },
  states: {
    active: {
      on: {
        INCREMENT: {
          actions: assign({ count: ({ context }) => context.count + 1 })
        }
      }
    }
  }
});

export function Counter() {
  const [snapshot, send, actorRef] = useMachine(counterMachine);
  const count = useSelector(actorRef, (s) => s.context.count);

  return (
    <button onClick={() => send({ type: 'INCREMENT' })}>
      State: {String(snapshot.value)} | Count: {count}
    </button>
  );
}
```

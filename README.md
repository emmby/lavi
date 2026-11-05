# Portable XState

A high-performance, dynamic Finite State Machine (FSM) runtime for the web, powered by Rust and WebAssembly.

Portable XState (formerly FSM WASM) allows you to load **XState-compatible JSON definitions** at runtime and execute them within a compiled WebAssembly module.

## Features

*   **Dynamic Loading**: Load state machine definitions (JSON) on the fly without recompiling the Wasm.
*   **High Performance**: Core logic runs in native Rust/Wasm.
*   **XState Compatibility**: Accepts a subset of the XState JSON format (States, Transitions, Context).
*   **Web Ready**: bindings for direct use in JavaScript/TypeScript.

## Project Structure

*   **`portable-xstate-rust`**: The pure Rust library containing the `RuntimeMachine` logic and state storage.
*   **`portable-xstate-wasm`**: The WebAssembly adapter that exposes the runtime to JavaScript/TypeScript.
*   **`portable-xstate-c-cpp`**: The C/C++ static library and headers for embedded and native platforms.
*   **`index.html`**: A demo web page visualizing a Traffic Light state machine.

## Getting Started

### Prerequisites

*   **VS Code** with **Dev Containers** extension (Recommended).
*   Or manually: Rust, Cargo, and `wasm-pack`.

### Building

1.  Open the project in the Dev Container.
2.  Build the Wasm module:
    ```bash
    wasm-pack build portable-xstate-wasm --target web --out-name portable_xstate
    ```

### Running the Demo

1.  Build the project (see above).
2.  Start a local HTTP server in the root directory:
    ```bash
    python3 -m http.server 8080
    ```
3.  Open [http://localhost:8080](http://localhost:8080) in your browser.
4.  Open the browser console to see state transitions logs, or interact with the UI.

## Testing

This project includes both native core tests and browser-based verification tests.

### 1. Run Native Tests (portable-xstate-rust)
These run on your machine and verify the core Rust logic.

```bash
cargo test
```

### 2. Run Browser Tests (portable-xstate-wasm)
These run in a headless Chrome browser to verify the WASM integration against XState.

```bash
cd portable-xstate-wasm
wasm-pack test --headless --chrome
```

### Run All Tests
To run all test suites (native, WASM, E2E, C/C++, and unit) in sequence:

```bash
npm test
```

Or run them individually:
- `npm run test:native` (Rust)
- `npm run test:wasm` (WASM/XState)
- `npm run test:e2e` (HTML Examples via Playwright)
- `npm run test:c-cpp` (C/C++ Static Library)
- `npm run test:unit` (XState v5 Conformance via Vitest)

### Running tests via Devcontainer CLI

If you are not using VS Code but still want an isolated, reproducible environment to run tests, you can use the `@devcontainers/cli`:

```bash
# Start the devcontainer
npx -y @devcontainers/cli up --workspace-folder .

# Run the test suite headlessly inside the container
npx -y @devcontainers/cli exec --workspace-folder . npm run build
npx -y @devcontainers/cli exec --workspace-folder . npm test
```

## Usage in JavaScript

```javascript
import init, { Actor } from './path/to/pkg/portable_xstate.js';

async function main() {
    await init();

    const machineDef = JSON.stringify({
        initial: "idle",
        states: {
            idle: { on: { START: "running" } },
            running: { on: { STOP: "idle" } }
        }
    });

    // Create and start the machine
    const machine = new Actor(machineDef).start();
    console.log(machine.getSnapshot().value); // "idle"
    
    // Send an event
    machine.send("START");
    console.log(machine.getSnapshot().value); // "running"
}
main();
```

## Known Discrepancy: XState 5 Parity vs Memory Environments

To safely cater to severely memory-constrained embedded environments (e.g., Arduinos, ESP32s), `portable-xstate` consciously deviates from XState 5 regarding pre-initialization event queues.

In **XState 5**, an unstarted Actor implements a **Mailbox Buffer**: any events sent via `actor.send()` before `actor.start()` is called are pushed to a dynamic array allocated on the heap, which is later flushed when the machine starts.

In **portable-xstate**, an unstarted machine explicitly **Drops** events. If `machine.send()` is called before `machine.start()`, the event is safely discarded. This guarantees that unbound hardware interrupts or rapid polling sensors cannot flood the heap with a massive queue of deferred events and crash the microcontroller before it boots.

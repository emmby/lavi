# Portable XState

A high-performance, dynamic Finite State Machine (FSM) runtime for the web, powered by Rust and WebAssembly.

Portable XState (formerly FSM WASM) allows you to load **XState-compatible JSON definitions** at runtime and execute them within a compiled WebAssembly module.

## Features

*   **Dynamic Loading**: Load state machine definitions (JSON) on the fly without recompiling the Wasm.
*   **High Performance**: Core logic runs in native Rust/Wasm.
*   **XState Compatibility**: Accepts a subset of the XState JSON format (States, Transitions, Context).
*   **Web Ready**: bindings for direct use in JavaScript/TypeScript.

## Project Structure

*   **`portable-xstate-core`**: The pure Rust library containing the `RuntimeMachine` logic and state storage.
*   **`portable-xstate`**: The generic Wasm adapter that exposes the runtime to the browser.
*   **`fsm-codegen`**: (Experimental) Macros for compile-time state machine generation.
*   **`index.html`**: A demo web page visualizing a Traffic Light state machine.

## Getting Started

### Prerequisites

*   **VS Code** with **Dev Containers** extension (Recommended).
*   Or manually: Rust, Cargo, and `wasm-pack`.

### Building

1.  Open the project in the Dev Container.
2.  Build the Wasm module:
    ```bash
    wasm-pack build fsm-wasm --target web --out-name portable_xstate
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

### 1. Run Native Tests (fsm-core)
These run on your machine and verify the core Rust logic.

```bash
cargo test
```

### 2. Run Browser Tests (portable-xstate)
These run in a headless Chrome browser to verify the WASM integration against XState.

```bash
cd fsm-wasm
wasm-pack test --headless --chrome
```

### Run All Tests
To run all test suites (native, WASM, and E2E) in sequence:

```bash
npm test
```

Or run them individually:
- `npm run test:native` (Rust Core)
- `npm run test:wasm` (WASM/XState)
- `npm run test:e2e` (HTML Examples via Playwright)

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

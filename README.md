# Wasm XState Runtime

A high-performance, dynamic Finite State Machine (FSM) runtime for the web, powered by Rust and WebAssembly.

This library allows you to load **XState-compatible JSON definitions** at runtime and execute them within a compiled WebAssembly module.

## Features

*   **Dynamic Loading**: Load state machine definitions (JSON) on the fly without recompiling the Wasm.
*   **High Performance**: Core logic runs in native Rust/Wasm.
*   **XState Compatibility**: Accepts a subset of the XState JSON format (States, Transitions, Context).
*   **Web Ready**: bindings for direct use in JavaScript/TypeScript.

## Project Structure

*   **`fsm-core`**: The pure Rust library containing the `RuntimeMachine` logic and state storage.
*   **`fsm-wasm`**: The generic Wasm adapter that exposes the runtime to the browser.
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
    wasm-pack build fsm-wasm --target web
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

### 2. Run Browser Tests (fsm-wasm)
These run in a headless Chrome browser to verify the WASM integration against XState.

```bash
cd fsm-wasm
wasm-pack test --headless --chrome
```

### Run All Tests
To run both suites in sequence:

```bash
cargo test && cd fsm-wasm && wasm-pack test --headless --chrome
```

## Usage in JavaScript

```javascript
import init, { FsmRunner } from './path/to/pkg/fsm_wasm.js';

async function main() {
    await init();

    const machineDef = JSON.stringify({
        initial: "idle",
        states: {
            idle: { on: { START: "running" } },
            running: { on: { STOP: "idle" } }
        }
    });

    const machine = new FsmRunner(machineDef);
    console.log(machine.current_state()); // "idle"
    
    machine.transition("START");
    console.log(machine.current_state()); // "running"
}
main();
```

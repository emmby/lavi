# Portable XState Examples

This directory contains HTML examples demonstrating the capabilities of the Portable XState library.

## Prerequisite: Local Server Required

Due to browser security policies (CORS) regarding ES Modules (`type="module"`) and WebAssembly loading, **these examples will not work if you simply open the `.html` files directly in your browser** (i.e., using the `file://` protocol).

You must serve these files using a local HTTP server.

## How to Run

### Option 0: View Online (No Setup Required)

All browser WASM examples are hosted on GitHub Pages:
-   **All Examples**: [https://emmby.github.io/portable-xstate/examples/](https://emmby.github.io/portable-xstate/examples/)
-   **Autonomous Robot Vacuum Demo**: [https://emmby.github.io/portable-xstate/examples/robot_vacuum.html](https://emmby.github.io/portable-xstate/examples/robot_vacuum.html)

### Option 1: Python (Local Server)

If you have Python 3 installed (which acts as a simple HTTP server):

1.  Open your terminal at the root of the repository (`/path/to/fsm-wasm`).
2.  Run the following command:
    ```bash
    python3 -m http.server 8000
    ```
3.  Open your browser and navigate to:
    [http://localhost:8000/examples/](http://localhost:8000/examples/)

### Option 2: Node.js (http-server)

If you have Node.js installed:

```bash
npx http-server . -p 8000
```
Then visit [http://localhost:8000/examples/](http://localhost:8000/examples/).

### Option 3: VS Code (Live Server)

If you use VS Code:
1.  Install the **Live Server** extension.
2.  Right-click `examples/index.html` and select **"Open with Live Server"**.

## Available Examples

-   **Autonomous Robot Vacuum** (`robot_vacuum.html`): Interactive open-floor robot vacuum simulator powered by `portable-xstate-wasm`, featuring nested states, `exit` safety guarantees, `always` low-battery routing, `after` timers, shallow history (`H`), and a live Mermaid SVG / JSON toggle.
-   **Simple Light Switch** (`simple.html`): Basic state toggling.
-   **Traffic Light** (`traffic_light.html`): Timed transitions and consolidated configuration.
-   **Guards** (`guards.html`): Conditional transitions based on context.
-   **Actions** (`actions.html`): Executing side-effects and updating functionality.
-   **Dynamic Assignment** (`dynamic_assignment.html`): Modifying extended state (context) with math.
-   **ESP32 Traffic Light (PlatformIO)** (`stoplight-esp32/`): Arduino/ESP32 firmware project using the C++ wrapper (`xstate::PortableXState`) via PlatformIO.

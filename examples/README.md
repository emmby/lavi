# Lavi Examples

This directory contains HTML and embedded examples demonstrating the capabilities of the Lavi statechart library.

## Prerequisite: Local Server Required

Due to browser security policies (CORS) regarding ES Modules (`type="module"`) and WebAssembly loading, **these examples will not work if you simply open the `.html` files directly in your browser** (i.e., using the `file://` protocol).

You must serve these files using a local HTTP server.

## How to Run

### Option 1: Python (Recommended)

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

-   **Simple Light Switch** (`simple.html`): Basic state toggling.
-   **Traffic Light** (`traffic_light.html`): Timed transitions and consolidated configuration.
-   **Guards** (`guards.html`): Conditional transitions based on context.
-   **Actions** (`actions.html`): Executing side-effects and updating functionality.
-   **Dynamic Assignment** (`dynamic_assignment.html`): Modifying extended state (context) with math.
-   **ESP32 Traffic Light (PlatformIO)** (`stoplight-esp32/`): Arduino/ESP32 firmware project using the C++ wrapper (`lavi::Machine`) via PlatformIO.

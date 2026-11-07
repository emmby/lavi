# Lavi Examples

This directory contains self-contained per-language usage examples across all 7 supported languages, an ESP32 PlatformIO firmware example, and interactive browser WebAssembly demos.

---

## 1. Self-Contained Language Examples

Each subdirectory is a standalone project with its own idiomatic build configuration implementing the shared `stoplight` statechart (`red` $\rightarrow$ `green` $\rightarrow$ `yellow` $\rightarrow$ `red` with `entry` actions, `after` delayed transitions via `SimulatedClock`, and a guarded `PEDESTRIAN_WAIT` event).

All host examples are built automatically by `npm run build` (`npm run build:examples`) and verified on every CI run by `npm test` (`npm run test:examples`).

| Language | Directory | Local Build / Project Config | Run Command (from repo root) |
| :--- | :--- | :--- | :--- |
| **Rust** | [`rust/`](./rust/) | `Cargo.toml` | `cargo run --manifest-path examples/rust/Cargo.toml` |
| **JavaScript / TypeScript (WASM)** | [`js-wasm/`](./js-wasm/) | `package.json` | `npm run build:npm && node examples/js-wasm/index.mjs` |
| **C (C11)** | [`c/`](./c/) | `Makefile` | `npm run build:c-cpp && make -C examples/c run` |
| **C++ (C++11)** | [`cpp/`](./cpp/) | `Makefile` | `npm run build:c-cpp && make -C examples/cpp run` |
| **Python** | [`python/`](./python/) | `pyproject.toml` | `npm run build:py && PYTHONPATH=lavi-py/python python3 examples/python/main.py` |
| **Java** | [`java/`](./java/) | `build.gradle.kts` + `settings.gradle.kts` | `cargo build -p lavi-jvm && ./lavi-jvm/gradlew -p examples/java run` |
| **Kotlin** | [`kotlin/`](./kotlin/) | `build.gradle.kts` + `settings.gradle.kts` | `cargo build -p lavi-jvm && ./lavi-jvm/gradlew -p examples/kotlin run` |
| **ESP32 (PlatformIO)** | [`esp32/`](./esp32/) | `platformio.ini` | `pio run -d examples/esp32` *(or host syntax check via `npm run test:examples`)* |

---

## 2. Interactive Browser WASM Demos

Due to browser security policies (CORS) regarding ES Modules (`type="module"`) and WebAssembly loading, **these `.html` examples must be served over HTTP** rather than opened directly via `file://`.

### How to Run the Browser Demos

#### Option 0: View Online (No Setup Required)

Browse and interact with all WASM examples live on GitHub Pages (opens with the **Autonomous Robot Vacuum** demo selected):
- [https://emmby.github.io/lavi/examples/](https://emmby.github.io/lavi/examples/)

#### Option 1: Python (Local Server)

1. Build the web WASM bundle from the root of the repository:
   ```bash
   npm run build:web
   ```
2. Start a local HTTP server:
   ```bash
   python3 -m http.server 8000
   ```
3. Open [http://localhost:8000/examples/](http://localhost:8000/examples/).

#### Option 2: Node.js (`http-server`)

```bash
npx http-server . -p 8000
```
Then visit [http://localhost:8000/examples/](http://localhost:8000/examples/).

### Available Browser Demos

- **Autonomous Robot Vacuum** (`robot_vacuum.html`): Interactive open-floor robot vacuum simulator powered by `lavi-wasm`, featuring nested states, `exit` safety guarantees, `always` low-battery routing, `after` timers, shallow history (`H`), and a live Mermaid SVG / JSON toggle.
- **Simple Light Switch** (`simple.html`): Basic state toggling.
- **Traffic Light** (`traffic_light.html`): Timed transitions and consolidated configuration.
- **Guards** (`guards.html`): Conditional transitions based on context.
- **Parameterized Guards** (`parameterized_guards.html`): Reusable guards with declarative parameters.
- **Actions** (`actions.html`): Executing side-effects and updating functionality.
- **Dynamic Assignment** (`dynamic_assignment.html`): Modifying extended state (context) with math.

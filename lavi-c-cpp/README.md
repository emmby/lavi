# Lavi C/C++ (`lavi-c-cpp`)

This crate provides a **C/C++ compatible Static Library** for the **Lavi** statechart runtime (compatible with XState v5 JSON), allowing it to be used in embedded firmware (such as ESP32), iOS, or desktop C/C++ systems.

## Architecture

This library compiles to `liblavi_c_cpp.a`. It exposes:
1. **Raw C API**: `include/lavi.h`
2. **C++11 Wrapper**: `include/lavi.hpp` (RAII-enabled `lavi::Machine`, `lavi::SimulatedClock`, and `lavi::Value`)

## PlatformIO Integration

`lavi` includes a root [`library.json`](../library.json) and [`build_rust.py`](../build_rust.py) script so it can be added directly to any PlatformIO project (`espressif32` or `native`).

### 1. Add to `platformio.ini`
```ini
[env:featheresp32]
platform = espressif32
board = featheresp32
framework = arduino
lib_deps =
    https://github.com/emmby/lavi.git
```

For ESP32 Xtensa targets (`esp32`, `esp32s2`, `esp32s3`), install [`espup`](https://github.com/esp-rs/espup) (`cargo install espup && espup install`) so `build_rust.py` can cross-compile `liblavi_c_cpp.a` automatically during `pio run`. For `platform = native`, the standard host Rust toolchain is used automatically.

See [`examples/stoplight-esp32`](../examples/stoplight-esp32) for a complete Arduino/ESP32 PlatformIO example.

## Building Manually for ESP32 (Xtensa)

**Requirement:** The **Espressif Rust Toolchain** is required **only** for Xtensa-based chips (ESP32, S2, S3).

1. **Install Toolchain**:
   ```bash
   cargo install espup
   espup install
   . $HOME/export-esp.sh
   ```

2. **Build Static Lib**:
   ```bash
   cargo +esp build -Z build-std=std,panic_abort --release --target xtensa-esp32-espidf
   ```

## Building for iOS, Android, or Desktop

For standard targets (including ESP32-C3 which is RISC-V), you can use the **Stable Rust** toolchain:

1. **Add Target** (example for iOS):
   ```bash
   rustup target add aarch64-apple-ios
   ```

2. **Build**:
   ```bash
   cargo build --release --target aarch64-apple-ios
   ```

## Usage

### 1. Include Headers
Add `lavi-c-cpp/include` to your project's include path.

### 2. Link Library
Link against the generated `.a` file in `target/xtensa-esp32-espidf/release/liblavi_c_cpp.a`.

### 3. C++11 Example
```cpp
#include "lavi.hpp"

void setup() {
    // lavi::Machine handles memory automatically (RAII)
    lavi::Machine machine(JSON_CONFIG);
    machine.start();
    
    // Check active state
    bool isGreen = machine.matches("green");
    
    // Send Event
    machine.send("TIMER");
}
```

### 4. C Example
```c
#include "lavi.h"

void setup() {
    MachineContext* machine = fsm_create(JSON_CONFIG);
    fsm_start(machine);

    bool isGreen = fsm_matches(machine, "green");

    fsm_send_event(machine, "TIMER");

    // Manual memory management required in C
    fsm_free(machine);
}
```

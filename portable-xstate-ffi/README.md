# Portable XState FFI

This crate provides a **C/C++ compatible Static Library** for the Portable XState runtime, allowing it to be used in embedded firmware (like ESP32), iOS, or other systems.

## Architecture

This library compiles to `libportable_xstate_ffi.a`. It exposes:
1.  **Raw C API**: `include/portable_xstate.h`
2.  **C++ Wrapper**: `include/portable_xstate.hpp` (RAII-enabled, easy to use)

## Building for ESP32 (Xtensa)

**Requirement:** The **Espressif Rust Toolchain** is required **only** for Xtensa-based chips (ESP32, S2, S3).

1.  **Install Toolchain**:
    ```bash
    cargo install espup
    espup install
    . $HOME/export-esp.sh
    ```

2.  **Build Static Lib**:
    ```bash
    cargo +esp build -Z build-std=std,panic_abort --release --target xtensa-esp32-espidf
    ```

## Building for iOS, Android, or Desktop

For standard targets (including ESP32-C3 which is RISC-V), you can use the **Stable Rust** toolchain.

1.  **Add Target** (example for iOS):
    ```bash
    rustup target add aarch64-apple-ios
    ```

2.  **Build**:
    ```bash
    cargo build --release --target aarch64-apple-ios
    ```

## Usage

### 1. Include Headers
Add `portable-xstate-ffi/include` to your project's include path.

### 2. Link Library
Link against the generated `.a` file in `target/xtensa-esp32-espidf/release/libportable_xstate_ffi.a`.

### 3. C++ Example
```cpp
#include "portable_xstate.hpp"

void setup() {
    // PortableXState handles memory automatically (RAII)
    xstate::PortableXState machine(JSON_CONFIG);
    machine.start();
    
    // Check active state
    bool isGreen = machine.matches("green");
    
    // Send Event
    machine.send("TIMER");
}
```

### 4. C Example
```c
#include "portable_xstate.h"

void setup() {
    MachineContext* machine = fsm_create(JSON_CONFIG);
    
    fsm_send_event(machine, "TIMER");
    
    // Manual memory management required in C
    fsm_free(machine);
}
```

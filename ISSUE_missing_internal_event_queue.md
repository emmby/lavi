# ISSUE: Reactive Timing Hazards in Consumers due to Missing Internal Event Queue (`raise` support)

## Status
- **Type**: Feature Request / Architecture Gap
- **Priority**: High (Alignment with XState v5 Roadmap)
- **Reporter**: `perlerspiral` team
- **Related to**: [XSTATE_5_FEATURES.md:L67](file:///Users/mike/Gemini/perlerspiral-complete/portable-xstate/XSTATE_5_FEATURES.md#L67)

## Problem Description
Consumer applications (specifically the `perlerspiral` bead sorter) are currently forced to implement manual **Event Deferral** logic in their main application loops. This is because `portable-xstate` does not yet support an internal event queue for "raised" events.

When an action (like `captureTrainingBead`) completes, it often needs to signal the machine to transition to a "next step." However, calling `machine->send()` directly from within an action callback triggers a **Reactive Timing Hazard**:
1. The machine is still in the middle of a transition.
2. The state is not yet "settled."
3. Triggering a nested transition can lead to stack overflow (on embedded targets like ESP32) or lost events.

## Current Workaround (in `perlerspiral`)
Developers must use global flags and manual polling in `main.cpp`:

```cpp
// In main.cpp
void loop() {
  machine->tick();
  
  if (emitNextBead) {
      emitNextBead = false;
      machine->send("NEXT_BEAD"); // Manual deferral
  }
}
```

## Proposed Solution (XState v5 Parity)
Implement the **Internal Event Queue** model from XState v5:
1. **Internal Queue**: Add a FIFO queue to the `MachineContext` (Rust) or `PortableXState` (C++).
2. **`raise(EVENT)` Action**: Implement the `raise` action creator which adds an event to the *internal* queue rather than the external mailbox.
3. **Priority Processing**: Update the interpreter logic to process the internal queue **immediately** after a transition finishes and **before** checking the external event queue.

## Benefits
- **Determinism**: Guaranteed processing of "micro-steps" before external hardware interrupts (buttons, sensors) are handled.
- **Architectural Purity**: Remove the need for consumption-layer polling and global "emit" flags.
- **Spec Alignment**: Fulfills a key missing feature in the [XState 5 checklist](file:///Users/mike/Gemini/perlerspiral-complete/portable-xstate/XSTATE_5_FEATURES.md).

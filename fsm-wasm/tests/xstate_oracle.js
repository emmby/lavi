import { createMachine, createActor, assign, setup } from 'https://esm.sh/xstate@5.18.2';

export function getDoorScenario() {
    const machineDef = {
        id: "door",
        initial: "closed",
        states: {
            closed: {
                on: {
                    OPEN: [
                        { target: "open", guard: "can_open" },
                        { target: "closed" }
                    ],
                    UNLOCK: [{ target: "closed", actions: ["unlock"] }],
                    LOCK: [{ target: "closed", actions: ["lock"] }]
                }
            },
            open: {
                on: {
                    CLOSE: [ { target: "closed" } ],
                    LOCK: [{ target: "open", actions: ["lock"] }] 
                }
            }
        },
        context: { locked: true }
    };

    const config = {
        guards: {
            can_open: ({ context }) => !context.locked
        },
        actions: {
            unlock: assign({ locked: false }),
            lock: assign({ locked: true }),
        }
    };
    
    // Scenario from verification.html: ["OPEN", "UNLOCK", "OPEN", "CLOSE"]
    const events = ["OPEN", "UNLOCK", "OPEN", "CLOSE"];

    return { machineDef, config, events };
}

export function runXState(machineDef, config, events) {
    const snapshots = [];
    
    const machine = setup(config).createMachine(machineDef);
    const actor = createActor(machine, {
        inspect: (evt) => {
            if (evt.type === '@xstate.snapshot') {
                // Sanitize via JSON to remove functions/internal helpers and match what we get from WASM/JSON serialization
                snapshots.push(JSON.parse(JSON.stringify(evt.snapshot)));
            }
        }
    });
    
    actor.start();
    
    // Initial snapshot is captured by start()
    
    for (const eventName of events) {
        actor.send({ type: eventName });
    }
    
    return snapshots;
}

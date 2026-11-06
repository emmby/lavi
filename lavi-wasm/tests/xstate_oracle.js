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

// Helper to convert XState assign functions to JSON-serializable objects for lavi-wasm
function sanitizeForJson(obj) {
    if (Array.isArray(obj)) {
        return obj.map(sanitizeForJson);
    } else if (obj !== null && typeof obj === 'object') {
        const newObj = {};
        for (const k in obj) {
            newObj[k] = sanitizeForJson(obj[k]);
        }
        return newObj;
    } else if (typeof obj === 'function' && obj.type === 'xstate.assign') {
        return { type: 'xstate.assign', assignment: obj.assignment };
    }
    return obj;
}

export function getAssignScenario() {
    const machineDef = {
        id: "assign_test",
        initial: "a",
        context: { count: 0, test: "A" },
        states: {
            a: {
                on: {
                    NEXT: [{ target: "b", actions: [assign({ count: 10 })] }]
                }
            },
            b: {
                 entry: [assign({ test: "B" })],
                 on: {
                     RESET: [{ target: "a", actions: [assign({ count: 0, test: "A" })] }]
                 }
            }
        }
    };

    const config = {
        guards: {},
        actions: {}
    };
    
    // Scenario: A(count:0, test:A) -> NEXT -> B(count:10, test:A->B) -> RESET -> A(count:0, test:A)
    const events = ["NEXT", "RESET", "NEXT"];

    return { machineDef, machineDefJson: sanitizeForJson(machineDef), config, events };
}

export function getDynamicAssignScenario() {
    const machineDef = {
        id: "dynamic_assign_test",
        initial: "a",
        context: { count: 0 },
        states: {
            a: {
                on: {
                    INC: [{ target: "a", actions: ["increment"] }]
                }
            }
        }
    };

    const config = {
        actions: {
            increment: assign((ctx) => ({ count: ctx.context.count + 1 }))
        }
    };
    
    // Scenario: A(count:0) -> INC -> A(count:1) -> INC -> A(count:2)
    const events = ["INC", "INC"];

    // Note: dynamic assignments in 'config' don't need sanitization for the machineDefJson
    // BUT lavi-wasm setup_wasm_machine(config) handles the config object.
    
    return { machineDef, machineDefJson: machineDef, config, events };
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

export function getXStateAssign(assignment) {
    return assign(assignment);
}


import { createMachine, createActor, assign } from 'xstate';
import fs from 'fs';

const [machinePath, scenarioPath] = process.argv.slice(2);

if (!machinePath || !scenarioPath) {
    console.error("Usage: node generate_trace.js <machine_json_path> <scenario_json_path>");
    process.exit(1);
}

try {
    const machineDef = JSON.parse(fs.readFileSync(machinePath, 'utf8'));
    const scenario = JSON.parse(fs.readFileSync(scenarioPath, 'utf8'));

    // Implementation Maps
    const guards = {
        can_open: ({ context }) => {
            return !context.locked;
        },
        always_true: () => true,
        always_false: () => false,
    };

    const actions = {
        unlock: assign({ locked: false }),
        lock: assign({ locked: true }),
    };

    // Build XState Config
    const xstateConfig = {
        id: machineDef.id,
        initial: machineDef.initial,
        context: machineDef.context || {},
        states: {},
        // Inject global event handlers for context manipulation (Simulating external environment changes)
        on: {
            UNLOCK: { actions: "unlock" },
            LOCK: { actions: "lock" }
        }
    };

    // Transform states
    for (const [stateName, stateDef] of Object.entries(machineDef.states)) {
        xstateConfig.states[stateName] = {
            on: {}
        };
        
        if (stateDef.on) {
            for (const [eventName, transitions] of Object.entries(stateDef.on)) {
                // definitions.transitions is array of { target, cond }
                xstateConfig.states[stateName].on[eventName] = transitions.map(t => ({
                    target: t.target,
                    guard: t.cond 
                }));
            }
        }
    }

    const machine = createMachine(xstateConfig, {
        guards,
        actions
    });

    const actor = createActor(machine);
    
    const trace = [];

    // Capture initial state
    // trace.push({ event: "init", value: machine.initialState.value, context: machine.initialState.context });
    // Actually subscription will catch initial state if started? 
    // V5: subscribe emits current state immediately upon subscription? Let's verify.
    // Usually yes.

        trace.push({
            event: snapshot.event?.type || "init", 
            value: snapshot.value,
            context: snapshot.context
        });

    actor.start();

    // Run scenario
    for (const event of scenario) {
        const eventObj = typeof event === 'string' ? { type: event } : event;
        actor.send(eventObj);
    }

    console.log(JSON.stringify(trace, null, 2));

} catch (err) {
    console.error("Error generating trace:", err);
    process.exit(1);
}

import init, { FsmRunner, setup, assign } from '../fsm-wasm/pkg/fsm_wasm.js';

/**
 * Common setup for FSM examples.
 * @param {Object} options
 * @param {Object} options.machineDef - The state machine definition.
 * @param {Object} [options.config] - The configuration object (guards, actions).
 * @param {Function} [options.updateUI] - Callback to update the UI. Receives (state, context, machine).
 */
export async function runFsmExample({ machineDef, config }) {
    await init();

    let machine;

    // Handle string definition (for JSON loading) or object definition
    const def = typeof machineDef === 'string' ? JSON.parse(machineDef) : machineDef;

    if (config) {
        // Use setup() pattern if config (guards/actions) is provided
        const wasmMachine = setup(config).createMachine(def);
        machine = wasmMachine.spawn_runner({});
    } else {
        // Direct instantiation for simple machines
        machine = new FsmRunner(def);
    }

    // Display Machine Definition if element exists
    const defDisplay = document.getElementById('defDisplay');
    if (defDisplay) {
        let displayContent = "// Machine Definition\nconst machine = " + JSON.stringify(def, null, 2);
        
        if (config) {
            // Replacer to show function placeholders
            const replacer = (key, value) => {
                if (typeof value === 'function') {
                    return value.toString();
                }
                return value;
            };
            
            displayContent += "\n\n// Configuration (Actions/Guards)\nconst config = " + JSON.stringify(config, replacer, 2);
        }
        
        defDisplay.innerText = displayContent;
    }

    return {
        machine
    };
}

// Export common WASM functions for convenience
export { assign };

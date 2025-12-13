import init, { FsmRunner, setup, assign } from '../fsm-wasm/pkg/fsm_wasm.js';

/**
 * Common setup for FSM examples.
 * @param {Object} options
 * @param {Object} options.machineDef - The state machine definition.
 * @param {Object} [options.config] - The configuration object (guards, actions).
 * @param {Function} [options.updateUI] - Callback to update the UI. Receives (state, context, machine).
 */
export async function runFsmExample({ machineDef, config, updateUI }) {
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

    // Wrap updateUI to provide easier access to state and context
    const wrappedUpdateUI = (event) => {
        const state = machine.current_state();
        let context;
        try {
            context = JSON.parse(machine.context_json());
        } catch (e) {
            context = {};
        }

        // Auto-update common status elements if they exist
        const lblState = document.getElementById('lblState');
        if (lblState) lblState.innerText = state.toUpperCase();

        const lblContext = document.getElementById('lblContext');
        if (lblContext) lblContext.innerText = JSON.stringify(context, null, 2);

        // Call custom UI update
        if (updateUI) {
            updateUI(state, context, machine, event);
        }
    };

    // Initial update
    wrappedUpdateUI("INIT");

    // Display Machine Definition if element exists
    const defDisplay = document.getElementById('defDisplay');
    if (defDisplay) {
        let displayContent = "// Machine Definition\nconst machine = " + JSON.stringify(def, null, 2);
        
        if (config) {
            // Note: Functions in config will not stringify well by default, 
            // but we can try to show keys at least.
            // For better display, we might want to manually format it or expect the caller to do it.
            // Here we just do a basic stringify which omits functions.
            // A better approach for specific examples is to let them handle the display text
            // or pass the raw source string.
            // For now, we show the structure.
            
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
        machine,
        updateUI: wrappedUpdateUI,
        transition: (event) => {
            machine.transition(event);
            wrappedUpdateUI(event);
        }
    };
}

// Export common WASM functions for convenience
export { assign };

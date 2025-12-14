import { assign } from '../portable-xstate-wasm/pkg/portable_xstate.js';

/**
 * Displays the machine definition and configuration in the UI.
 * @param {Object} machineDef - The state machine definition.
 * @param {Object} [config] - The configuration object (guards, actions).
 * @param {string} [elementId='defDisplay'] - The ID of the element to display the code in.
 */
export function displayMachine(machineDef, config, elementId = 'defDisplay') {
    const defDisplay = document.getElementById(elementId);
    if (defDisplay) {
        // Handle definition parsing if it's a string
        const def = typeof machineDef === 'string' ? JSON.parse(machineDef) : machineDef;

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
}



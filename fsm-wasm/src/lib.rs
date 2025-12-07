use wasm_bindgen::prelude::*;
use fsm_core::RuntimeMachine;

#[wasm_bindgen]
pub struct FsmRunner {
    machine: RuntimeMachine,
}

#[wasm_bindgen]
impl FsmRunner {
    #[wasm_bindgen(constructor)]
    pub fn new(json_definition: &str) -> Result<FsmRunner, JsValue> {
        let machine = RuntimeMachine::from_json(json_definition)
            .map_err(|e| JsValue::from_str(&format!("Failed to parse machine: {}", e)))?;
            
        Ok(FsmRunner { machine })
    }

    pub fn transition(&mut self, event: &str) -> Option<String> {
        self.machine.transition(event)
    }

    pub fn current_state(&self) -> String {
        self.machine.state().to_string()
    }
    
    pub fn context_json(&self) -> String {
        self.machine.context.to_string()
    }
}

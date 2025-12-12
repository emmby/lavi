use wasm_bindgen::prelude::*;
use wasm_bindgen_test::*;
use web_sys::console;

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen(module = "/tests/xstate_oracle.js")]
extern "C" {
    fn getDoorScenario() -> JsValue;
    fn runXState(machine_def: &JsValue, config: &JsValue, events: &JsValue) -> JsValue;
}

#[wasm_bindgen_test]
fn test_verification_door_machine() {
    console::log_1(&"Starting verification test...".into());

    let scenario = getDoorScenario();
    let machine_def = js_sys::Reflect::get(&scenario, &"machineDef".into()).unwrap();
    let config = js_sys::Reflect::get(&scenario, &"config".into()).unwrap();
    let events = js_sys::Reflect::get(&scenario, &"events".into()).unwrap();

    // 1. Run XState Oracle
    console::log_1(&"Running XState oracle...".into());
    let xstate_snapshots_val = runXState(&machine_def, &config, &events);
    let xstate_snapshots: js_sys::Array = xstate_snapshots_val.into();

    // 2. Setup WASM Machine
    console::log_1(&"Setting up WASM machine...".into());
    // Use the public API from the crate
    let setup_obj = fsm_wasm::setup_wasm_machine(config.clone());
    let wasm_machine = setup_obj.create_wasm_machine(machine_def.clone()).expect("Failed to create wasm machine");

    // Capture WASM snapshots
    let wasm_snapshots = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let wasm_snapshots_clone = wasm_snapshots.clone();

    let inspector = Closure::wrap(Box::new(move |evt: JsValue| {
        let evt_type = js_sys::Reflect::get(&evt, &"type".into()).unwrap();
        if evt_type.as_string().unwrap_or_default() == "@xstate.snapshot" {
            // In fsm-wasm implementation, the event IS the snapshot with an added type field
            wasm_snapshots_clone.borrow_mut().push(js_sys::JSON::stringify(&evt).unwrap().as_string().unwrap());
        }
    }) as Box<dyn FnMut(JsValue)>);

    let inspect_opts = js_sys::Object::new();
    js_sys::Reflect::set(&inspect_opts, &"inspect".into(), &inspector.as_ref().unchecked_ref()).unwrap();

    let mut runner = wasm_machine.spawn_runner(inspect_opts.into()).expect("Failed to spawn runner");
    
    // 3. Start WASM
    console::log_1(&"Starting WASM runner...".into());
    runner.start();

    // 4. Run Events
    let events_arr = js_sys::Array::from(&events);
    for i in 0..events_arr.length() {
        let event = events_arr.get(i);
        runner.send(event);
    }
    
    inspector.forget(); // Leak memory to keep handler alive if needed, or just drop. here synchronous so ok.

    // 5. Compare
    let wasm_snaps = wasm_snapshots.borrow();
    
    console::log_1(&format!("WASM Snapshots: {}", wasm_snaps.len()).into());
    console::log_1(&format!("XState Snapshots: {}", xstate_snapshots.length()).into());

    assert_eq!(wasm_snaps.len(), xstate_snapshots.length() as usize, "Snapshot count mismatch");

    for i in 0..wasm_snaps.len() {
        let w_json = &wasm_snaps[i];
        let x_obj = xstate_snapshots.get(i as u32);
        let x_json = js_sys::JSON::stringify(&x_obj).unwrap().as_string().unwrap();
        
        // Parse both to flexible generic JSON objects to ignore ordering and specific keys
        let w_val: serde_json::Value = serde_json::from_str(w_json).unwrap();
        let x_val: serde_json::Value = serde_json::from_str(&x_json).unwrap();
        
        let w_obj = w_val.as_object().unwrap();
        let x_obj_map = x_val.as_object().unwrap();
        
        // Filter keys
        let mut w_filtered = w_obj.clone();
        w_filtered.remove("type"); // Remove WASM internal event type
        
        let mut x_filtered = x_obj_map.clone();
        x_filtered.remove("machine"); // Remove XState internal ref
        
        // Also remove 'output' and 'error' from XState if WASM doesn't have them yet? 
        // verification.html logic:
        // "Filter out 'machine' (XState internal ref) and 'type' (WASM event type)"
        // It strictly compares everything else.
        
        // Note: XState snapshots might have 'status': 'active'
        // WASM sets 'status': 'active'
        
        // Let's assert equality of filtered objects
        if w_filtered != x_filtered {
             console::log_1(&format!("Mismatch at step {}:\nWASM: {:?}\nXState: {:?}", i, w_filtered, x_filtered).into());
             panic!("Snapshot mismatch at step {}", i);
        }
    }
    
    console::log_1(&"Verification Passed!".into());
}

use wasm_bindgen::prelude::*;
use wasm_bindgen_test::*;
use web_sys::console;

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen(module = "/tests/xstate_oracle.js")]
extern "C" {
    fn getDoorScenario() -> JsValue;
    fn getAssignScenario() -> JsValue;
    fn getDynamicAssignScenario() -> JsValue;
    fn runXState(machine_def: &JsValue, config: &JsValue, events: &JsValue) -> JsValue;
    fn getXStateAssign(assignment: &JsValue) -> JsValue;
}

fn verify_scenario(scenario: JsValue) {
    let machine_def = js_sys::Reflect::get(&scenario, &"machineDef".into()).unwrap();
    // Prefer machineDefJson if available (for scenarios with functions like assign)
    let machine_def_json = js_sys::Reflect::get(&scenario, &"machineDefJson".into()).unwrap_or(JsValue::UNDEFINED);
    let wasm_def = if !machine_def_json.is_undefined() { machine_def_json } else { machine_def.clone() };

    let config = js_sys::Reflect::get(&scenario, &"config".into()).unwrap();
    let events = js_sys::Reflect::get(&scenario, &"events".into()).unwrap();

    // 1. Run XState Oracle
    console::log_1(&"Running XState oracle...".into());
    let xstate_snapshots_val = runXState(&machine_def, &config, &events);
    let xstate_snapshots: js_sys::Array = xstate_snapshots_val.into();

    // 2. Setup WASM Machine
    console::log_1(&"Setting up WASM machine...".into());
    let setup_obj = fsm_wasm::setup_wasm_machine(config.clone());
    let wasm_machine = setup_obj.create_wasm_machine(wasm_def).expect("Failed to create wasm machine");

    // Capture WASM snapshots
    let wasm_snapshots = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let wasm_snapshots_clone = wasm_snapshots.clone();

    let inspector = Closure::wrap(Box::new(move |evt: JsValue| {
        let evt_type = js_sys::Reflect::get(&evt, &"type".into()).unwrap();
        if evt_type.as_string().unwrap_or_default() == "@xstate.snapshot" {
            wasm_snapshots_clone.borrow_mut().push(js_sys::JSON::stringify(&evt).unwrap().as_string().unwrap());
        }
    }) as Box<dyn FnMut(JsValue)>);

    let inspect_opts = js_sys::Object::new();
    js_sys::Reflect::set(&inspect_opts, &"inspect".into(), &inspector.as_ref().unchecked_ref()).unwrap();

    let mut runner = wasm_machine.create_actor_method(inspect_opts.into()).expect("Failed to spawn runner");
    
    // 3. Start WASM
    console::log_1(&"Starting WASM runner...".into());
    runner.start();

    // 4. Run Events
    let events_arr = js_sys::Array::from(&events);
    for i in 0..events_arr.length() {
        let event = events_arr.get(i);
        runner.send(event);
    }
    
    inspector.forget(); 

    // 5. Compare
    let wasm_snaps = wasm_snapshots.borrow();
    
    console::log_1(&format!("WASM Snapshots: {}", wasm_snaps.len()).into());
    console::log_1(&format!("XState Snapshots: {}", xstate_snapshots.length()).into());

    assert_eq!(wasm_snaps.len(), xstate_snapshots.length() as usize, "Snapshot count mismatch");

    for i in 0..wasm_snaps.len() {
        let w_json = &wasm_snaps[i];
        let x_obj = xstate_snapshots.get(i as u32);
        let x_json = js_sys::JSON::stringify(&x_obj).unwrap().as_string().unwrap();
        
        let w_val: serde_json::Value = serde_json::from_str(w_json).unwrap();
        let x_val: serde_json::Value = serde_json::from_str(&x_json).unwrap();
        
        let w_obj = w_val.as_object().unwrap();
        let x_obj_map = x_val.as_object().unwrap();
        
        let mut w_filtered = w_obj.clone();
        w_filtered.remove("type"); 
        
        let mut x_filtered = x_obj_map.clone();
        x_filtered.remove("machine"); 
        
        if w_filtered != x_filtered {
             console::log_1(&format!("Mismatch at step {}:\nWASM: {:?}\nXState: {:?}", i, w_filtered, x_filtered).into());
             panic!("Snapshot mismatch at step {}", i);
        }
    }
}

#[wasm_bindgen_test]
fn test_verification_door_machine() {
    console::log_1(&"Starting verification test: Door...".into());
    let scenario = getDoorScenario();
    verify_scenario(scenario);
    console::log_1(&"Verification Passed: Door!".into());
}

#[wasm_bindgen_test]
fn test_verification_assign_machine() {
    console::log_1(&"Starting verification test: Assign...".into());
    let scenario = getAssignScenario();
    verify_scenario(scenario);
    console::log_1(&"Verification Passed: Assign!".into());
}

#[wasm_bindgen_test]
fn test_verification_dynamic_assign_machine() {
    console::log_1(&"Starting verification test: Dynamic Assign...".into());
    let scenario = getDynamicAssignScenario();
    verify_scenario(scenario);
    console::log_1(&"Verification Passed: Dynamic Assign!".into());
}

#[wasm_bindgen_test]
fn test_assign_identity() {
    console::log_1(&"Testing assign() identity...".into());
    
    // Case 1: Object assignment
    let obj_assign = serde_wasm_bindgen::to_value(&serde_json::json!({ "count": 10 })).unwrap();
    let xstate_res = getXStateAssign(&obj_assign);
    let wasm_res = fsm_wasm::assign(obj_assign);
    
    // Compare type
    let type_key = JsValue::from_str("type");
    let x_type = js_sys::Reflect::get(&xstate_res, &type_key).unwrap();
    let w_type = js_sys::Reflect::get(&wasm_res, &type_key).unwrap();
    assert_eq!(x_type.as_string(), w_type.as_string(), "Type should match");
    assert_eq!(x_type.as_string().unwrap(), "xstate.assign", "Type should be xstate.assign");
    
    // Compare function type
    assert!(xstate_res.is_function(), "XState assign returns function");
    assert!(wasm_res.is_function(), "Wasm assign returns function");

    // Compare assignment payload
    let assign_key = JsValue::from_str("assignment");
    let x_val = js_sys::Reflect::get(&xstate_res, &assign_key).unwrap();
    let w_val = js_sys::Reflect::get(&wasm_res, &assign_key).unwrap();
    
    let x_json = js_sys::JSON::stringify(&x_val).unwrap();
    let w_json = js_sys::JSON::stringify(&w_val).unwrap();
    assert_eq!(x_json, w_json, "Assignment content should match");

    console::log_1(&"Assign identity verified!".into());
}

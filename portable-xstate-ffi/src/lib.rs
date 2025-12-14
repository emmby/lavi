use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int};
use portable_xstate_core::{RuntimeMachine, Value};
use portable_xstate_json::from_json_str;
use std::ptr;
use std::collections::HashMap;

pub struct MachineContext {
    machine: RuntimeMachine,
}

#[no_mangle]
pub extern "C" fn fsm_create(json_ptr: *const c_char) -> *mut MachineContext {
    if json_ptr.is_null() { return ptr::null_mut(); }
    let c_str = unsafe { CStr::from_ptr(json_ptr) };
    let json_str = match c_str.to_str() {
        Ok(s) => s,
        Err(_) => return ptr::null_mut(),
    };

    let def = match from_json_str(json_str) {
        Ok(d) => d,
        Err(_) => return ptr::null_mut(),
    };

    let machine = RuntimeMachine::new(def);
    let ctx = Box::new(MachineContext {
        machine,
    });
    Box::into_raw(ctx)
}

#[no_mangle]
pub extern "C" fn fsm_free(ptr: *mut MachineContext) {
    if !ptr.is_null() {
        unsafe {
            drop(Box::from_raw(ptr));
        }
    }
}

#[no_mangle]
pub extern "C" fn fsm_get_state(ptr: *mut MachineContext, buf: *mut c_char, len: usize) -> c_int {
    let ctx = unsafe {
        if ptr.is_null() { return -1; }
        &mut *ptr
    };
    let state = ctx.machine.state();
    let c_str_state = match CString::new(state) {
        Ok(s) => s,
        Err(_) => return -2,
    };
    let bytes = c_str_state.as_bytes_with_nul();
    
    if bytes.len() > len {
        return -3;
    }
    
    unsafe {
        ptr::copy_nonoverlapping(bytes.as_ptr(), buf as *mut u8, bytes.len());
    }
    0
}

#[no_mangle]
pub extern "C" fn fsm_send_event(ptr: *mut MachineContext, event_ptr: *const c_char) -> c_int {
    let ctx = unsafe {
        if ptr.is_null() { return -1; }
        &mut *ptr
    };
    
    let event_c_str = unsafe { CStr::from_ptr(event_ptr) };
    let event = match event_c_str.to_str() {
        Ok(s) => s,
        Err(_) => return -2,
    };
    
    let guards = HashMap::new(); 
    let actions = HashMap::new(); 
    let payload = Value::Null;

    ctx.machine.transition(event, &payload, &guards, &actions);
    0
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CStr;

    #[test]
    fn test_traffic_light_ffi() {
        let json = CString::new(r#"{
          "id": "trafficLight",
          "initial": "green",
          "states": {
            "green": { "on": { "TIMER": [ { "target": "yellow" } ] } },
            "yellow": { "on": { "TIMER": [ { "target": "red" } ] } },
            "red": { "on": { "TIMER": [ { "target": "green" } ] } }
          },
          "context": { "cycles": 0 }
        }"#).unwrap();

        let machine_ptr = fsm_create(json.as_ptr());
        assert!(!machine_ptr.is_null());

        let mut buf = vec![0u8; 64];
        
        // Check initial state
        assert_eq!(fsm_get_state(machine_ptr, buf.as_mut_ptr() as *mut c_char, buf.len()), 0);
        let state = unsafe { CStr::from_ptr(buf.as_ptr() as *const c_char).to_str().unwrap() };
        assert_eq!(state, "green");

        // Transition
        let event = CString::new("TIMER").unwrap();
        fsm_send_event(machine_ptr, event.as_ptr());

        // Check new state
        assert_eq!(fsm_get_state(machine_ptr, buf.as_mut_ptr() as *mut c_char, buf.len()), 0);
        let state = unsafe { CStr::from_ptr(buf.as_ptr() as *const c_char).to_str().unwrap() };
        assert_eq!(state, "yellow");

        fsm_free(machine_ptr);
    }
}

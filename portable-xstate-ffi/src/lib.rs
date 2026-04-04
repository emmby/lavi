#![no_std]
extern crate alloc;

use alloc::boxed::Box;
use alloc::ffi::CString;
use core::ffi::{CStr, c_char, c_int}; 
use core::ptr;
use alloc::collections::BTreeMap;
use portable_xstate_core::{RuntimeMachine, Value};
use portable_xstate_json::from_json_str;
use indexmap::IndexMap;

#[repr(C)]
pub struct fsm_value_t {
    _private: [u8; 0],
}

pub type fsm_action_callback_t = Option<extern "C" fn(
    action_name: *const c_char,
    context: *const fsm_value_t,
    event: *const fsm_value_t,
    user_data: *mut core::ffi::c_void,
)>;

pub struct RegisteredAction {
    pub callback: fsm_action_callback_t,
    pub user_data: *mut core::ffi::c_void,
}

pub struct MachineContext {
    machine: RuntimeMachine,
    callbacks: BTreeMap<alloc::string::String, RegisteredAction>,
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

    let machine = match RuntimeMachine::new(&def) {
        Ok(m) => m,
        Err(_) => return ptr::null_mut(),
    };
    let ctx = Box::new(MachineContext {
        machine,
        callbacks: BTreeMap::new(),
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
    let state_value = &ctx.machine.current_state;
    // Extract first active path from StateValue
    let active_paths = state_value.get_active_paths();
    let state_str = if !active_paths.is_empty() {
        &active_paths[0]
    } else {
        ""
    };
    
    let c_str_state = match CString::new(state_str) {
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
    
    let guards = IndexMap::new(); 
    let mut actions = IndexMap::new(); 
    
    for (name, reg) in &ctx.callbacks {
        let action_name_cstr = alloc::ffi::CString::new(name.clone()).unwrap();
        let callback = reg.callback;
        let user_data = reg.user_data;
        
        let action_fn: portable_xstate_core::ActionFunction = alloc::rc::Rc::new(move |context: &mut Value, event: &Value, _params: &Option<Value>| {
            if let Some(cb) = callback {
                cb(
                    action_name_cstr.as_ptr(),
                    context as *const Value as *const fsm_value_t,
                    event as *const Value as *const fsm_value_t,
                    user_data
                );
            }
        });
        actions.insert(name.clone(), action_fn);
    }
    
    let payload = Value::Null;

    ctx.machine.transition(event, &payload, &guards, &actions);
    0
}

#[no_mangle]
pub extern "C" fn fsm_value_is_object(val: *const fsm_value_t) -> bool {
    if val.is_null() { return false; }
    let v = unsafe { &*(val as *const Value) };
    v.as_object().is_some()
}

#[no_mangle]
pub extern "C" fn fsm_value_is_string(val: *const fsm_value_t) -> bool {
    if val.is_null() { return false; }
    let v = unsafe { &*(val as *const Value) };
    v.as_string().is_some()
}

#[no_mangle]
pub extern "C" fn fsm_value_is_number(val: *const fsm_value_t) -> bool {
    if val.is_null() { return false; }
    let v = unsafe { &*(val as *const Value) };
    v.as_f64().is_some()
}

#[no_mangle]
pub extern "C" fn fsm_value_is_bool(val: *const fsm_value_t) -> bool {
    if val.is_null() { return false; }
    let v = unsafe { &*(val as *const Value) };
    v.as_bool().is_some()
}

#[no_mangle]
pub extern "C" fn fsm_value_get_string(val: *const fsm_value_t) -> *const c_char {
    if val.is_null() { return core::ptr::null(); }
    let v = unsafe { &*(val as *const Value) };
    if let Some(s) = v.as_string() {
        let c_str = alloc::ffi::CString::new(s.clone()).unwrap();
        return c_str.into_raw() as *const c_char;
    }
    core::ptr::null()
}

#[no_mangle]
pub extern "C" fn fsm_value_get_number(val: *const fsm_value_t) -> f64 {
    if val.is_null() { return 0.0; }
    let v = unsafe { &*(val as *const Value) };
    v.as_f64().unwrap_or(0.0)
}

#[no_mangle]
pub extern "C" fn fsm_value_get_bool(val: *const fsm_value_t) -> bool {
    if val.is_null() { return false; }
    let v = unsafe { &*(val as *const Value) };
    v.as_bool().unwrap_or(false)
}

#[no_mangle]
pub extern "C" fn fsm_value_get_property(val: *const fsm_value_t, key: *const c_char) -> *const fsm_value_t {
    if val.is_null() || key.is_null() { return core::ptr::null(); }
    let v = unsafe { &*(val as *const Value) };
    if let Some(obj) = v.as_object() {
        let key_cstr = unsafe { CStr::from_ptr(key) };
        if let Ok(key_str) = key_cstr.to_str() {
            if let Some(prop) = obj.get(key_str) {
                return prop as *const Value as *const fsm_value_t;
            }
        }
    }
    core::ptr::null()
}

#[no_mangle]
pub extern "C" fn fsm_register_action(
    ptr: *mut MachineContext,
    action_name: *const c_char,
    callback: fsm_action_callback_t,
    user_data: *mut core::ffi::c_void,
) -> c_int {
    let ctx = unsafe {
        if ptr.is_null() { return -1; }
        &mut *ptr
    };
    if action_name.is_null() { return -2; }
    
    let name_c_str = unsafe { CStr::from_ptr(action_name) };
    let name = match name_c_str.to_str() {
        Ok(s) => s,
        Err(_) => return -3,
    };
    
    ctx.callbacks.insert(alloc::string::String::from(name), RegisteredAction {
        callback,
        user_data,
    });
    
    0
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::ffi::CString;
    use core::ffi::{CStr, c_char};
    use alloc::vec;

    // We can use std for testing if we want, but since we are checking no_std compat, let's stick to alloc/core as much as possible.
    // However, cargo test runs with std available by default unless restricted.
    // The issue is `use std::...` inside a `#![no_std]` crate failing if std isn't reachable.
    // We can just use alloc/core to be safe and consistent.

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

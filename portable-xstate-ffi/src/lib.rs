#![no_std]
#![allow(clippy::not_unsafe_ptr_arg_deref)]
extern crate alloc;

use alloc::boxed::Box;
use alloc::ffi::CString;
use core::ffi::{CStr, c_char, c_int}; 
use core::ptr;
use alloc::collections::BTreeMap;
use portable_xstate_core::{from_json_str, RuntimeMachine, Value};
use indexmap::IndexMap;

#[repr(C)]
pub struct fsm_value_t {
    _private: [u8; 0],
}

#[allow(non_camel_case_types)]
pub type fsm_action_callback_t = Option<extern "C" fn(
    action_name: *const c_char,
    context: *const fsm_value_t,
    event: *const fsm_value_t,
    user_data: *mut core::ffi::c_void,
)>;

#[allow(non_camel_case_types)]
pub type fsm_clock_callback_t = Option<extern "C" fn() -> u64>;

#[allow(non_camel_case_types)]
pub type fsm_clock_with_data_callback_t = Option<extern "C" fn(user_data: *mut core::ffi::c_void) -> u64>;

#[allow(non_camel_case_types)]
pub type fsm_guard_callback_t = Option<extern "C" fn(
    guard_name: *const c_char,
    context: *const fsm_value_t,
    event: *const fsm_value_t,
    user_data: *mut core::ffi::c_void,
) -> bool>;

pub struct RegisteredAction {
    pub callback: fsm_action_callback_t,
    pub user_data: *mut core::ffi::c_void,
}

pub struct RegisteredGuard {
    pub callback: fsm_guard_callback_t,
    pub user_data: *mut core::ffi::c_void,
}

type SharedMachineList = alloc::rc::Rc<core::cell::RefCell<alloc::vec::Vec<*mut MachineContext>>>;

pub struct SimulatedClock {
    clock: portable_xstate_core::SimulatedClock,
    machines: SharedMachineList,
}

pub struct MachineContext {
    machine: RuntimeMachine,
    callbacks: BTreeMap<alloc::string::String, RegisteredAction>,
    guard_callbacks: BTreeMap<alloc::string::String, RegisteredGuard>,
    actions: IndexMap<alloc::string::String, portable_xstate_core::ActionFunction>,
    guards: IndexMap<alloc::string::String, portable_xstate_core::GuardFunction>,
    bound_clocks: alloc::vec::Vec<SharedMachineList>,
}

impl Drop for MachineContext {
    fn drop(&mut self) {
        let self_ptr = self as *mut MachineContext;
        for list in &self.bound_clocks {
            list.borrow_mut().retain(|&ptr| ptr != self_ptr);
        }
    }
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

    let machine = match RuntimeMachine::from_definition(def) {
        Ok(m) => m,
        Err(_) => return ptr::null_mut(),
    };
    let ctx = Box::new(MachineContext {
        machine,
        callbacks: BTreeMap::new(),
        guard_callbacks: BTreeMap::new(),
        actions: IndexMap::new(),
        guards: IndexMap::new(),
        bound_clocks: alloc::vec::Vec::new(),
    });
    Box::into_raw(ctx)
}

#[no_mangle]
pub extern "C" fn fsm_start(ptr: *mut MachineContext) -> c_int {
    let ctx = unsafe {
        if ptr.is_null() { return -1; }
        &mut *ptr
    };
    ctx.machine.start(&ctx.guards, &ctx.actions);
    0
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
    
    if event_ptr.is_null() {
        return -2;
    }
    let event_c_str = unsafe { CStr::from_ptr(event_ptr) };
    let event = match event_c_str.to_str() {
        Ok(s) => s,
        Err(_) => return -2,
    };
    
    if !ctx.machine.is_started { return 0; }
    let mut event_map = IndexMap::new();
    event_map.insert(
        alloc::string::String::from("type"),
        Value::String(alloc::string::String::from(event)),
    );
    let payload = Value::Object(event_map);
    ctx.machine.transition(event, &payload, &ctx.guards, &ctx.actions);
    0
}

/// Registers a custom clock mechanism for resolving `after` transitions.
/// The `callback` must return a u64 reflecting an absolute monotonically increasing timestamp in milliseconds.
/// Returns 0 on success, or -1 if the context pointer is null.
#[no_mangle]
pub extern "C" fn fsm_register_clock(
    ptr: *mut MachineContext,
    callback: fsm_clock_callback_t,
) -> c_int {
    let ctx = unsafe {
        if ptr.is_null() { return -1; }
        &mut *ptr
    };
    if let Some(cb) = callback {
        let clock_millis_fn: alloc::rc::Rc<dyn Fn() -> u64> = alloc::rc::Rc::new(move || cb());
        ctx.machine.register_clock(clock_millis_fn);
    }
    0
}

#[no_mangle]
pub extern "C" fn fsm_register_clock_with_data(
    ptr: *mut MachineContext,
    callback: fsm_clock_with_data_callback_t,
    user_data: *mut core::ffi::c_void,
) -> c_int {
    let ctx = unsafe {
        if ptr.is_null() { return -1; }
        &mut *ptr
    };
    if let Some(cb) = callback {
        let clock_millis_fn: alloc::rc::Rc<dyn Fn() -> u64> =
            alloc::rc::Rc::new(move || cb(user_data));
        ctx.machine.register_clock(clock_millis_fn);
    }
    0
}

#[no_mangle]
pub extern "C" fn fsm_get_next_deadline(
    ptr: *const MachineContext,
    out_deadline: *mut u64,
) -> bool {
    if ptr.is_null() || out_deadline.is_null() {
        return false;
    }
    let ctx = unsafe { &*ptr };
    if let Some(deadline) = ctx.machine.next_timer_deadline() {
        unsafe {
            *out_deadline = deadline;
        }
        true
    } else {
        false
    }
}

#[no_mangle]
pub extern "C" fn fsm_simulated_clock_create(initial_ms: u64) -> *mut SimulatedClock {
    let sim = Box::new(SimulatedClock {
        clock: portable_xstate_core::SimulatedClock::with_initial(initial_ms),
        machines: alloc::rc::Rc::new(core::cell::RefCell::new(alloc::vec::Vec::new())),
    });
    Box::into_raw(sim)
}

#[no_mangle]
pub extern "C" fn fsm_simulated_clock_free(clock_ptr: *mut SimulatedClock) {
    if !clock_ptr.is_null() {
        unsafe {
            drop(Box::from_raw(clock_ptr));
        }
    }
}

#[no_mangle]
pub extern "C" fn fsm_register_simulated_clock(
    ptr: *mut MachineContext,
    clock_ptr: *mut SimulatedClock,
) -> c_int {
    if ptr.is_null() || clock_ptr.is_null() {
        return -1;
    }
    let ctx = unsafe { &mut *ptr };
    let sim = unsafe { &mut *clock_ptr };

    ctx.machine.register_clock(&sim.clock);
    {
        let mut list = sim.machines.borrow_mut();
        if !list.contains(&ptr) {
            list.push(ptr);
        }
    }
    ctx.bound_clocks.push(sim.machines.clone());
    0
}

#[no_mangle]
pub extern "C" fn fsm_simulated_clock_now(clock_ptr: *const SimulatedClock) -> u64 {
    if clock_ptr.is_null() {
        return 0;
    }
    let sim = unsafe { &*clock_ptr };
    sim.clock.now()
}

#[no_mangle]
pub extern "C" fn fsm_simulated_clock_set(
    clock_ptr: *mut SimulatedClock,
    target_ms: u64,
) -> c_int {
    if clock_ptr.is_null() {
        return -1;
    }
    let sim = unsafe { &mut *clock_ptr };

    let mut last_deadline: Option<u64> = None;
    let mut same_deadline_steps = 0usize;
    let mut total_steps = 0usize;

    loop {
        let current_machines: alloc::vec::Vec<*mut MachineContext> =
            sim.machines.borrow().clone();
        let next_deadline_opt = current_machines
            .iter()
            .filter_map(|&m_ptr| {
                let ctx = unsafe { &*m_ptr };
                ctx.machine.next_timer_deadline()
            })
            .min();

        let Some(next_deadline) = next_deadline_opt else {
            break;
        };
        if next_deadline > target_ms {
            break;
        }
        if last_deadline == Some(next_deadline) {
            same_deadline_steps += 1;
            if same_deadline_steps >= 100 {
                break;
            }
        } else {
            last_deadline = Some(next_deadline);
            same_deadline_steps = 0;
        }
        total_steps += 1;
        if total_steps > 1_000_000 {
            break;
        }

        sim.clock.set_now_raw(next_deadline);

        for m_ptr in current_machines {
            if sim.machines.borrow().contains(&m_ptr) {
                let ctx = unsafe { &mut *m_ptr };
                let _ = ctx.machine.tick(&ctx.guards, &ctx.actions);
            }
        }
    }

    if target_ms > sim.clock.now() {
        sim.clock.set_now_raw(target_ms);
    }

    0
}

#[no_mangle]
pub extern "C" fn fsm_simulated_clock_increment(
    clock_ptr: *mut SimulatedClock,
    dt_ms: u64,
) -> c_int {
    if clock_ptr.is_null() {
        return -1;
    }
    let now = fsm_simulated_clock_now(clock_ptr);
    fsm_simulated_clock_set(clock_ptr, now.saturating_add(dt_ms))
}

/// Evaluates queued `after` transition timers.
/// Call this periodically or when waking from an event loop to advance delayed transitions.
/// Returns 0 on success, or -1 if the context pointer is null.
#[no_mangle]
pub extern "C" fn fsm_tick(ptr: *mut MachineContext) -> c_int {
    let ctx = unsafe {
        if ptr.is_null() { return -1; }
        &mut *ptr
    };
    
    if !ctx.machine.is_started { return 0; }
    
    let _ = ctx.machine.tick(&ctx.guards, &ctx.actions);
    
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
pub extern "C" fn fsm_value_get_string(val: *const fsm_value_t, buf: *mut c_char, buf_len: usize) -> usize {
    if val.is_null() { return 0; }
    let v = unsafe { &*(val as *const Value) };
    if let Some(s) = v.as_string() {
        let len = s.len();
        if !buf.is_null() && buf_len > 0 {
            let bytes_to_copy = core::cmp::min(len, buf_len - 1);
            unsafe {
                core::ptr::copy_nonoverlapping(s.as_ptr(), buf as *mut u8, bytes_to_copy);
                *buf.add(bytes_to_copy) = 0;
            }
        }
        return len;
    }
    0
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
    
    let name_string = alloc::string::String::from(name);
    
    ctx.callbacks.insert(name_string.clone(), RegisteredAction {
        callback,
        user_data,
    });
    
    // Process the ActionFunction immediately
    let action_name_cstr = alloc::ffi::CString::new(name_string.clone()).unwrap();
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
    
    ctx.actions.insert(name_string, action_fn);
    
    0
}

#[no_mangle]
pub extern "C" fn fsm_register_guard(
    ptr: *mut MachineContext,
    guard_name: *const c_char,
    callback: fsm_guard_callback_t,
    user_data: *mut core::ffi::c_void,
) -> c_int {
    let ctx = unsafe {
        if ptr.is_null() { return -1; }
        &mut *ptr
    };
    if guard_name.is_null() { return -2; }
    
    let name_c_str = unsafe { CStr::from_ptr(guard_name) };
    let name = match name_c_str.to_str() {
        Ok(s) => s,
        Err(_) => return -3,
    };
    
    let name_string = alloc::string::String::from(name);
    
    ctx.guard_callbacks.insert(name_string.clone(), RegisteredGuard {
        callback,
        user_data,
    });
    
    let guard_name_cstr = alloc::ffi::CString::new(name_string.clone()).unwrap();
    let guard_fn: portable_xstate_core::GuardFunction = alloc::rc::Rc::new(move |context: &Value, event: &Value, _params: &Option<Value>| -> bool {
        if let Some(cb) = callback {
            cb(
                guard_name_cstr.as_ptr(),
                context as *const Value as *const fsm_value_t,
                event as *const Value as *const fsm_value_t,
                user_data
            )
        } else {
            false
        }
    });
    
    ctx.guards.insert(name_string, guard_fn);
    
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

        fsm_start(machine_ptr);

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

    extern "C" fn test_action_callback(
        _action_name: *const c_char,
        _context: *const fsm_value_t,
        _event: *const fsm_value_t,
        user_data: *mut core::ffi::c_void,
    ) {
        let machine_ptr = user_data as *mut MachineContext;
        let event = CString::new("NEXT").unwrap();
        fsm_send_event(machine_ptr, event.as_ptr());
    }

    #[test]
    fn test_reentrant_send_ffi() {
        let json = CString::new(r#"{
          "id": "reentrant",
          "initial": "step1",
          "states": {
            "step1": {
              "on": {
                "START": { "target": "step2" }
              }
            },
            "step2": {
              "entry": ["triggerNext"],
              "on": {
                "NEXT": { "target": "step3" }
              }
            },
            "step3": { }
          }
        }"#).unwrap();

        let machine_ptr = fsm_create(json.as_ptr());
        assert!(!machine_ptr.is_null());

        let action_name = CString::new("triggerNext").unwrap();
        fsm_register_action(machine_ptr, action_name.as_ptr(), Some(test_action_callback), machine_ptr as *mut core::ffi::c_void);

        fsm_start(machine_ptr);

        let event = CString::new("START").unwrap();
        fsm_send_event(machine_ptr, event.as_ptr());

        let mut buf = vec![0u8; 64];
        assert_eq!(fsm_get_state(machine_ptr, buf.as_mut_ptr() as *mut c_char, buf.len()), 0);
        let state = unsafe { CStr::from_ptr(buf.as_ptr() as *const c_char).to_str().unwrap() };
        
        // Due to the external queue deferral, the "NEXT" event generated in the "step2" entry action
        // will be processed sequentially immediately after "START" completes.
        assert_eq!(state, "step3");

        fsm_free(machine_ptr);
    }

    struct BootTestState {
        machine_ptr: *mut MachineContext,
        boot_entry_calls: usize,
        last_event_type: alloc::string::String,
    }

    extern "C" fn on_boot_entry(
        _action_name: *const c_char,
        _context: *const fsm_value_t,
        event: *const fsm_value_t,
        user_data: *mut core::ffi::c_void,
    ) {
        let state = unsafe { &mut *(user_data as *mut BootTestState) };
        state.boot_entry_calls += 1;
        let type_key = CString::new("type").unwrap();
        let ev_type_val = fsm_value_get_property(event, type_key.as_ptr());
        assert!(!ev_type_val.is_null());
        let mut buf = vec![0u8; 32];
        fsm_value_get_string(ev_type_val, buf.as_mut_ptr() as *mut c_char, buf.len());
        let ev_str = unsafe { CStr::from_ptr(buf.as_ptr() as *const c_char).to_str().unwrap() };
        assert_eq!(ev_str, "xstate.init");

        let next_ev = CString::new("EXTERNAL_FROM_BOOT").unwrap();
        fsm_send_event(state.machine_ptr, next_ev.as_ptr());
    }

    extern "C" fn on_running_entry(
        _action_name: *const c_char,
        _context: *const fsm_value_t,
        event: *const fsm_value_t,
        user_data: *mut core::ffi::c_void,
    ) {
        let state = unsafe { &mut *(user_data as *mut BootTestState) };
        let type_key = CString::new("type").unwrap();
        let ev_type_val = fsm_value_get_property(event, type_key.as_ptr());
        assert!(!ev_type_val.is_null());
        let mut buf = vec![0u8; 64];
        fsm_value_get_string(ev_type_val, buf.as_mut_ptr() as *mut c_char, buf.len());
        let ev_str = unsafe { CStr::from_ptr(buf.as_ptr() as *const c_char).to_str().unwrap() };
        state.last_event_type = alloc::string::String::from(ev_str);
    }

    extern "C" fn guard_is_allowed(
        _guard_name: *const c_char,
        _context: *const fsm_value_t,
        event: *const fsm_value_t,
        _user_data: *mut core::ffi::c_void,
    ) -> bool {
        let type_key = CString::new("type").unwrap();
        let ev_type_val = fsm_value_get_property(event, type_key.as_ptr());
        if ev_type_val.is_null() {
            return false;
        }
        let mut buf = vec![0u8; 32];
        fsm_value_get_string(ev_type_val, buf.as_mut_ptr() as *mut c_char, buf.len());
        let ev_str = unsafe { CStr::from_ptr(buf.as_ptr() as *const c_char).to_str().unwrap() };
        ev_str == "xstate.init"
    }

    #[test]
    fn test_initial_always_and_reentrant_send_on_start_ffi() {
        let json = CString::new(r#"{
          "id": "bootMachine",
          "initial": "boot",
          "states": {
            "boot": {
              "entry": ["onBootEntry"],
              "always": [
                { "target": "ready", "guard": "isAllowed" }
              ],
              "on": {
                "EXTERNAL_FROM_BOOT": { "target": "wrong_premature" }
              }
            },
            "ready": {
              "on": {
                "EXTERNAL_FROM_BOOT": { "target": "running" }
              }
            },
            "running": {
              "entry": ["onRunningEntry"]
            },
            "wrong_premature": {}
          }
        }"#).unwrap();

        let machine_ptr = fsm_create(json.as_ptr());
        assert!(!machine_ptr.is_null());

        let mut test_state = BootTestState {
            machine_ptr,
            boot_entry_calls: 0,
            last_event_type: alloc::string::String::new(),
        };
        let state_ptr = &mut test_state as *mut BootTestState as *mut core::ffi::c_void;

        let act_boot = CString::new("onBootEntry").unwrap();
        let act_run = CString::new("onRunningEntry").unwrap();
        let grd_allow = CString::new("isAllowed").unwrap();

        fsm_register_action(machine_ptr, act_boot.as_ptr(), Some(on_boot_entry), state_ptr);
        fsm_register_action(machine_ptr, act_run.as_ptr(), Some(on_running_entry), state_ptr);
        fsm_register_guard(machine_ptr, grd_allow.as_ptr(), Some(guard_is_allowed), ptr::null_mut());

        // Creating the machine must not execute initial actions before fsm_start
        assert_eq!(test_state.boot_entry_calls, 0);

        fsm_start(machine_ptr);

        let mut buf = vec![0u8; 64];
        assert_eq!(fsm_get_state(machine_ptr, buf.as_mut_ptr() as *mut c_char, buf.len()), 0);
        let state = unsafe { CStr::from_ptr(buf.as_ptr() as *const c_char).to_str().unwrap() };
        assert_eq!(state, "running");
        assert_eq!(test_state.boot_entry_calls, 1);
        assert_eq!(test_state.last_event_type, "EXTERNAL_FROM_BOOT");

        fsm_free(machine_ptr);
    }

    struct SimClockTestState {
        machine_ptr: *mut MachineContext,
        clock_ptr: *mut SimulatedClock,
        polls: u32,
        connected: bool,
        reentrant_fired_at: u64,
    }

    extern "C" fn sim_poll_action(
        _action_name: *const c_char,
        _context: *const fsm_value_t,
        _event: *const fsm_value_t,
        user_data: *mut core::ffi::c_void,
    ) {
        let st = unsafe { &mut *(user_data as *mut SimClockTestState) };
        st.polls += 1;
        if st.polls == 5 {
            st.connected = true;
        }
    }

    extern "C" fn sim_is_connected_guard(
        _guard_name: *const c_char,
        _context: *const fsm_value_t,
        _event: *const fsm_value_t,
        user_data: *mut core::ffi::c_void,
    ) -> bool {
        let st = unsafe { &*(user_data as *const SimClockTestState) };
        st.connected
    }

    extern "C" fn sim_reentrant_action(
        _action_name: *const c_char,
        _context: *const fsm_value_t,
        _event: *const fsm_value_t,
        user_data: *mut core::ffi::c_void,
    ) {
        let st = unsafe { &mut *(user_data as *mut SimClockTestState) };
        st.reentrant_fired_at = fsm_simulated_clock_now(st.clock_ptr);
        let ev = CString::new("SYNC_NOW").unwrap();
        fsm_send_event(st.machine_ptr, ev.as_ptr());
    }

    #[test]
    fn test_simulated_clock_hierarchical_and_reentrant_ffi() {
        let json = CString::new(r##"{
            "id": "april_ffi",
            "initial": "ConnectToNetwork",
            "states": {
                "ConnectToNetwork": {
                    "initial": "Polling",
                    "after": {
                        "300000": "Timeout"
                    },
                    "states": {
                        "Polling": {
                            "entry": ["poll"],
                            "after": {
                                "500": [
                                    { "target": "#Connected", "guard": "isConnected", "actions": ["triggerSync"] },
                                    { "target": "Polling", "reenter": true }
                                ]
                            }
                        }
                    }
                },
                "Connected": {
                    "id": "Connected",
                    "on": {
                        "SYNC_NOW": "Wait"
                    }
                },
                "Wait": {
                    "after": {
                        "60000": "Sleep"
                    }
                },
                "Sleep": {},
                "Timeout": {}
            }
        }"##).unwrap();

        let clock_ptr = fsm_simulated_clock_create(1000);
        assert!(!clock_ptr.is_null());
        assert_eq!(fsm_simulated_clock_now(clock_ptr), 1000);

        let machine_ptr = fsm_create(json.as_ptr());
        assert!(!machine_ptr.is_null());
        assert_eq!(fsm_register_simulated_clock(machine_ptr, clock_ptr), 0);

        let mut st = SimClockTestState {
            machine_ptr,
            clock_ptr,
            polls: 0,
            connected: false,
            reentrant_fired_at: 0,
        };
        let st_ptr = &mut st as *mut SimClockTestState as *mut core::ffi::c_void;

        let poll_name = CString::new("poll").unwrap();
        let sync_name = CString::new("triggerSync").unwrap();
        let guard_name = CString::new("isConnected").unwrap();

        fsm_register_action(machine_ptr, poll_name.as_ptr(), Some(sim_poll_action), st_ptr);
        fsm_register_action(machine_ptr, sync_name.as_ptr(), Some(sim_reentrant_action), st_ptr);
        fsm_register_guard(machine_ptr, guard_name.as_ptr(), Some(sim_is_connected_guard), st_ptr);

        fsm_start(machine_ptr);

        let mut next_dl = 0u64;
        assert!(fsm_get_next_deadline(machine_ptr, &mut next_dl));
        assert_eq!(next_dl, 1500);

        // Advance 100,000 ms (from 1,000 to 101,000):
        // - Poll 1 at t=1000, Poll 2 at 1500, Poll 3 at 2000, Poll 4 at 2500, Poll 5 at 3000 (sets connected = true)
        // - At t=3500: guard is true -> transitions to Connected, triggerSync sends SYNC_NOW -> transitions to Wait at t=3500
        // - Wait's 60,000ms timer is armed at 3500 + 60000 = 63,500
        // - At t=63,500: Wait -> Sleep
        // - Clock settles at 101,000
        assert_eq!(fsm_simulated_clock_increment(clock_ptr, 100_000), 0);
        assert_eq!(fsm_simulated_clock_now(clock_ptr), 101_000);
        assert_eq!(st.polls, 5);
        assert_eq!(st.reentrant_fired_at, 3500);

        let mut buf = vec![0u8; 64];
        assert_eq!(fsm_get_state(machine_ptr, buf.as_mut_ptr() as *mut c_char, buf.len()), 0);
        let state = unsafe { CStr::from_ptr(buf.as_ptr() as *const c_char).to_str().unwrap() };
        assert_eq!(state, "Sleep");
        assert!(!fsm_get_next_deadline(machine_ptr, &mut next_dl));

        // Verify destruction-order safety: freeing machine before clock, then calling set on clock
        fsm_free(machine_ptr);
        assert_eq!(fsm_simulated_clock_set(clock_ptr, 200_000), 0);
        assert_eq!(fsm_simulated_clock_now(clock_ptr), 200_000);
        fsm_simulated_clock_free(clock_ptr);
    }
}

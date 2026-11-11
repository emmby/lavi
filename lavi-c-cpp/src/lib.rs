#![no_std]
#![allow(clippy::not_unsafe_ptr_arg_deref)]
extern crate alloc;
#[cfg(not(target_os = "none"))]
extern crate std;

use alloc::boxed::Box;
use core::ffi::{CStr, c_char, c_int}; 
use core::ptr;
use lavi::{from_json_str, RuntimeMachine, Value};
use lavi::{IndexMap, IndexMapExt};

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
pub type fsm_action_with_params_callback_t = Option<extern "C" fn(
    action_name: *const c_char,
    context: *const fsm_value_t,
    event: *const fsm_value_t,
    params: *const fsm_value_t,
    user_data: *mut core::ffi::c_void,
)>;


#[allow(non_camel_case_types)]
pub type fsm_clock_callback_t = Option<extern "C" fn() -> u64>;

#[allow(non_camel_case_types)]
pub type fsm_clock_with_data_callback_t = Option<extern "C" fn(user_data: *mut core::ffi::c_void) -> u64>;

#[allow(non_camel_case_types)]
pub type fsm_inspect_callback_t = Option<extern "C" fn(
    inspect_json: *const c_char,
    user_data: *mut core::ffi::c_void,
)>;

#[allow(non_camel_case_types)]
pub type fsm_output_resolver_callback_t = Option<extern "C" fn(
    state_path: *const c_char,
    context: *const fsm_value_t,
    event: *const fsm_value_t,
    out_json_buffer: *mut c_char,
    out_buffer_len: usize,
    user_data: *mut core::ffi::c_void,
) -> i32>;

#[allow(non_camel_case_types)]
pub type fsm_invoke_start_callback_t = Option<extern "C" fn(
    invoke_id: *const c_char,
    src_json: *const c_char,
    input: *const fsm_value_t,
    context: *const fsm_value_t,
    event: *const fsm_value_t,
    user_data: *mut core::ffi::c_void,
)>;

#[allow(non_camel_case_types)]
pub type fsm_invoke_stop_callback_t = Option<extern "C" fn(
    invoke_id: *const c_char,
    user_data: *mut core::ffi::c_void,
)>;

#[allow(non_camel_case_types)]
pub type fsm_guard_callback_t = Option<extern "C" fn(
    guard_name: *const c_char,
    context: *const fsm_value_t,
    event: *const fsm_value_t,
    user_data: *mut core::ffi::c_void,
) -> bool>;

#[allow(non_camel_case_types)]
pub type fsm_delay_callback_t = Option<extern "C" fn(
    delay_name: *const c_char,
    context: *const fsm_value_t,
    event: *const fsm_value_t,
    user_data: *mut core::ffi::c_void,
) -> u64>;

#[allow(non_camel_case_types)]
pub type fsm_guard_with_params_callback_t = Option<extern "C" fn(
    guard_name: *const c_char,
    context: *const fsm_value_t,
    event: *const fsm_value_t,
    params: *const fsm_value_t,
    user_data: *mut core::ffi::c_void,
) -> bool>;

type SharedMachineList = alloc::rc::Rc<core::cell::RefCell<alloc::vec::Vec<*mut MachineContext>>>;

pub struct SimulatedClock {
    clock: lavi::SimulatedClock,
    machines: SharedMachineList,
}

pub struct MachineContext {
    machine: RuntimeMachine,
    actions: IndexMap<alloc::string::String, lavi::ActionFunction>,
    guards: IndexMap<alloc::string::String, lavi::GuardFunction>,
    bound_clocks: alloc::vec::Vec<SharedMachineList>,
}

impl MachineContext {
    fn detach_from_clocks(&mut self) {
        let self_ptr = self as *mut MachineContext;
        for list in self.bound_clocks.drain(..) {
            list.borrow_mut().retain(|&ptr| ptr != self_ptr);
        }
    }
}

impl Drop for MachineContext {
    fn drop(&mut self) {
        self.detach_from_clocks();
    }
}

fn copy_string_to_buffer(s: &str, buffer: *mut c_char, buffer_len: usize) -> i32 {
    let bytes = s.as_bytes();
    let needed_len = bytes.len() + 1;

    if needed_len > i32::MAX as usize {
        return -3;
    }

    if buffer.is_null() || buffer_len == 0 {
        return needed_len as i32;
    }

    if buffer_len < needed_len {
        return -2;
    }

    unsafe {
        core::ptr::copy_nonoverlapping(bytes.as_ptr(), buffer as *mut u8, bytes.len());
        *buffer.add(bytes.len()) = 0;
    }

    bytes.len() as i32
}

#[no_mangle]
pub extern "C" fn fsm_create_with_input(
    json_ptr: *const c_char,
    input_ptr: *const c_char,
) -> *mut MachineContext {
    if json_ptr.is_null() {
        return ptr::null_mut();
    }
    let c_str = unsafe { CStr::from_ptr(json_ptr) };
    let json_str = match c_str.to_str() {
        Ok(s) => s,
        Err(_) => return ptr::null_mut(),
    };

    let def = match from_json_str(json_str) {
        Ok(d) => d,
        Err(_) => return ptr::null_mut(),
    };

    let mut machine = match RuntimeMachine::from_definition(def) {
        Ok(m) => m,
        Err(_) => return ptr::null_mut(),
    };

    if !input_ptr.is_null() {
        let input_c_str = unsafe { CStr::from_ptr(input_ptr) };
        let input_str = match input_c_str.to_str() {
            Ok(s) => s,
            Err(_) => return ptr::null_mut(),
        };
        let parsed_input = match Value::from_json(input_str) {
            Ok(v) => v,
            Err(_) => return ptr::null_mut(),
        };
        machine.input = Some(parsed_input);
    }

    let ctx = Box::new(MachineContext {
        machine,
        actions: IndexMap::new(),
        guards: IndexMap::new(),
        bound_clocks: alloc::vec::Vec::new(),
    });
    Box::into_raw(ctx)
}

#[no_mangle]
pub extern "C" fn fsm_create(json_ptr: *const c_char) -> *mut MachineContext {
    fsm_create_with_input(json_ptr, ptr::null())
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
pub extern "C" fn fsm_destroy(ptr: *mut MachineContext) {
    fsm_free(ptr);
}

#[no_mangle]
pub extern "C" fn fsm_matches(ptr: *const MachineContext, state_path: *const c_char) -> bool {
    if ptr.is_null() || state_path.is_null() {
        return false;
    }
    let ctx = unsafe { &*ptr };
    let path_c_str = unsafe { CStr::from_ptr(state_path) };
    let path = match path_c_str.to_str() {
        Ok(s) => s,
        Err(_) => return false,
    };
    ctx.machine.matches(path)
}

#[no_mangle]
pub extern "C" fn fsm_is_in_state(ptr: *const MachineContext, state_path: *const c_char) -> bool {
    fsm_matches(ptr, state_path)
}


#[no_mangle]
pub extern "C" fn fsm_can(ctx: *const MachineContext, event: *const c_char) -> c_int {
    if ctx.is_null() {
        return -1;
    }
    if event.is_null() {
        return -2;
    }
    let ctx_ref = unsafe { &*ctx };
    let event_c_str = unsafe { CStr::from_ptr(event) };
    let event_str = match event_c_str.to_str() {
        Ok(s) => s,
        Err(_) => return -3,
    };
    let payload = Value::event(event_str);
    if ctx_ref
        .machine
        .can_with_guards(event_str, &payload, &ctx_ref.guards)
    {
        1
    } else {
        0
    }
}

#[no_mangle]
pub extern "C" fn fsm_has_tag(ctx: *const MachineContext, tag: *const c_char) -> c_int {
    if ctx.is_null() {
        return -1;
    }
    if tag.is_null() {
        return -2;
    }
    let ctx_ref = unsafe { &*ctx };
    let tag_c_str = unsafe { CStr::from_ptr(tag) };
    let tag_str = match tag_c_str.to_str() {
        Ok(s) => s,
        Err(_) => return -3,
    };
    if ctx_ref.machine.has_tag(tag_str) {
        1
    } else {
        0
    }
}

#[no_mangle]
pub extern "C" fn fsm_is_done(ctx: *const MachineContext) -> c_int {
    if ctx.is_null() {
        return -1;
    }
    let ctx_ref = unsafe { &*ctx };
    if ctx_ref.machine.is_done() {
        1
    } else {
        0
    }
}

#[no_mangle]
pub extern "C" fn fsm_stop(ctx: *mut MachineContext) -> c_int {
    if ctx.is_null() {
        return -1;
    }
    let ctx_ref = unsafe { &mut *ctx };
    ctx_ref.machine.stop();
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

#[no_mangle]
pub extern "C" fn fsm_send_json_event(
    ptr: *mut MachineContext,
    json_event_ptr: *const c_char,
) -> c_int {
    let ctx = unsafe {
        if ptr.is_null() {
            return -1;
        }
        &mut *ptr
    };
    if json_event_ptr.is_null() {
        return -2;
    }
    let c_str = unsafe { CStr::from_ptr(json_event_ptr) };
    let json_str = match c_str.to_str() {
        Ok(s) => s,
        Err(_) => return -2,
    };

    let val = match Value::from_json(json_str) {
        Ok(v) => v,
        Err(_) => return -3,
    };

    if !ctx.machine.is_started {
        return 0; // Level 1 memory invariant: drop pre-start events without buffering
    }

    let (event_type, payload) = match val {
        Value::String(ref s) => (s.clone(), Value::event(s)),
        Value::Object(ref map) => {
            let ev_type = match map.get("type").and_then(|v| v.as_string()) {
                Some(s) => alloc::string::String::from(s),
                None => return -4,
            };
            (ev_type, val)
        }
        _ => return -4,
    };

    ctx.machine.transition(&event_type, &payload, &ctx.guards, &ctx.actions);
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
    ctx.detach_from_clocks();
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
    ctx.detach_from_clocks();
    if let Some(cb) = callback {
        let clock_millis_fn: alloc::rc::Rc<dyn Fn() -> u64> =
            alloc::rc::Rc::new(move || cb(user_data));
        ctx.machine.register_clock(clock_millis_fn);
    }
    0
}

#[no_mangle]
pub extern "C" fn fsm_register_inspector(
    ptr: *mut MachineContext,
    callback: fsm_inspect_callback_t,
    user_data: *mut core::ffi::c_void,
) -> c_int {
    let ctx = unsafe {
        if ptr.is_null() {
            return -1;
        }
        &mut *ptr
    };
    if let Some(cb) = callback {
        let inspect_fn: lavi::InspectFunction = alloc::rc::Rc::new(move |evt: &lavi::InspectEvent| {
            let json = evt.to_json_string();
            if let Ok(c_str) = alloc::ffi::CString::new(json) {
                cb(c_str.as_ptr(), user_data);
            }
        });
        ctx.machine.register_inspector(inspect_fn);
    } else {
        ctx.machine.inspector_fn = None;
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
        clock: lavi::SimulatedClock::with_initial(initial_ms),
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

    ctx.detach_from_clocks();
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

    sim.clock.set_now_raw(target_ms);

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

    // Process the ActionFunction immediately
    let action_name_cstr = alloc::ffi::CString::new(name_string.clone()).unwrap();
    let action_fn: lavi::ActionFunction = alloc::rc::Rc::new(move |context: &mut Value, event: &Value, _params: &Option<Value>| {
        if let Some(cb) = callback {
            cb(
                action_name_cstr.as_ptr(),
                context as *const Value as *const fsm_value_t,
                event as *const Value as *const fsm_value_t,
                user_data
            );
        }
    });
    
    ctx.machine.actions.insert(name_string.clone(), action_fn.clone());
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

    let guard_name_cstr = alloc::ffi::CString::new(name_string.clone()).unwrap();
    let guard_fn: lavi::GuardFunction = alloc::rc::Rc::new(move |context: &Value, event: &Value, _params: &Option<Value>| -> bool {
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
    
    ctx.machine.guards.insert(name_string.clone(), guard_fn.clone());
    ctx.guards.insert(name_string, guard_fn);
    
    0
}

#[no_mangle]
pub extern "C" fn fsm_register_delay(
    ptr: *mut MachineContext,
    delay_name: *const c_char,
    callback: fsm_delay_callback_t,
    user_data: *mut core::ffi::c_void,
) -> c_int {
    let ctx = unsafe {
        if ptr.is_null() { return -1; }
        &mut *ptr
    };
    if delay_name.is_null() { return -2; }
    
    let name_c_str = unsafe { CStr::from_ptr(delay_name) };
    let name = match name_c_str.to_str() {
        Ok(s) => s,
        Err(_) => return -3,
    };
    
    let name_string = alloc::string::String::from(name);
    let delay_name_cstr = alloc::ffi::CString::new(name_string.clone()).unwrap();
    let delay_fn: lavi::DelayFunction = alloc::rc::Rc::new(move |context: &Value, event: &Value| -> u64 {
        if let Some(cb) = callback {
            cb(
                delay_name_cstr.as_ptr(),
                context as *const Value as *const fsm_value_t,
                event as *const Value as *const fsm_value_t,
                user_data,
            )
        } else {
            0
        }
    });
    
    ctx.machine.register_delay(&name_string, delay_fn);
    0
}

#[no_mangle]
pub extern "C" fn fsm_register_guard_with_params(
    ptr: *mut MachineContext,
    guard_name: *const c_char,
    callback: fsm_guard_with_params_callback_t,
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
    let guard_name_cstr = alloc::ffi::CString::new(name_string.clone()).unwrap();
    let guard_fn: lavi::GuardFunction = alloc::rc::Rc::new(move |context: &Value, event: &Value, params: &Option<Value>| -> bool {
        if let Some(cb) = callback {
            let params_ptr = match params {
                Some(p) => p as *const Value as *const fsm_value_t,
                None => ptr::null(),
            };
            cb(
                guard_name_cstr.as_ptr(),
                context as *const Value as *const fsm_value_t,
                event as *const Value as *const fsm_value_t,
                params_ptr,
                user_data,
            )
        } else {
            false
        }
    });
    
    ctx.machine.guards.insert(name_string.clone(), guard_fn.clone());
    ctx.guards.insert(name_string, guard_fn);
    0
}

#[no_mangle]
pub extern "C" fn fsm_register_action_with_params(
    ptr: *mut MachineContext,
    action_name: *const c_char,
    callback: fsm_action_with_params_callback_t,
    user_data: *mut core::ffi::c_void,
) -> c_int {
    let ctx = unsafe {
        if ptr.is_null() {
            return -1;
        }
        &mut *ptr
    };
    if action_name.is_null() {
        return -2;
    }

    let name_c_str = unsafe { CStr::from_ptr(action_name) };
    let name = match name_c_str.to_str() {
        Ok(s) => s,
        Err(_) => return -3,
    };

    let name_string = alloc::string::String::from(name);
    let action_name_cstr = match alloc::ffi::CString::new(name_string.clone()) {
        Ok(cs) => cs,
        Err(_) => return -3,
    };
    let action_fn: lavi::ActionFunction = alloc::rc::Rc::new(
        move |context: &mut Value, event: &Value, params: &Option<Value>| {
            if let Some(cb) = callback {
                let params_ptr = match params {
                    Some(p) => p as *const Value as *const fsm_value_t,
                    None => ptr::null(),
                };
                cb(
                    action_name_cstr.as_ptr(),
                    context as *const Value as *const fsm_value_t,
                    event as *const Value as *const fsm_value_t,
                    params_ptr,
                    user_data,
                );
            }
        },
    );

    ctx.machine.actions.insert(name_string.clone(), action_fn.clone());
    ctx.actions.insert(name_string, action_fn);
    0
}

#[no_mangle]
pub extern "C" fn fsm_register_output_resolver(
    ptr: *mut MachineContext,
    state_path: *const c_char,
    callback: fsm_output_resolver_callback_t,
    user_data: *mut core::ffi::c_void,
) -> c_int {
    let ctx = unsafe {
        if ptr.is_null() {
            return -1;
        }
        &mut *ptr
    };
    if state_path.is_null() {
        return -2;
    }
    let path_c_str = unsafe { CStr::from_ptr(state_path) };
    let path_str = match path_c_str.to_str() {
        Ok(s) => s,
        Err(_) => return -3,
    };
    let path_string = alloc::string::String::from(path_str);
    let path_cstr = match alloc::ffi::CString::new(path_string.clone()) {
        Ok(cs) => cs,
        Err(_) => return -3,
    };

    if let Some(cb) = callback {
        let resolver_fn: lavi::OutputResolver =
            alloc::rc::Rc::new(move |context: &Value, event: &Value| -> Value {
                let mut buf = alloc::vec![0u8; 4096];
                let written = cb(
                    path_cstr.as_ptr(),
                    context as *const Value as *const fsm_value_t,
                    event as *const Value as *const fsm_value_t,
                    buf.as_mut_ptr() as *mut c_char,
                    buf.len(),
                    user_data,
                );
                if written <= 0 {
                    return Value::Null;
                }
                let needed = written as usize;
                if needed >= buf.len() {
                    buf.resize(needed + 1, 0);
                    let written2 = cb(
                        path_cstr.as_ptr(),
                        context as *const Value as *const fsm_value_t,
                        event as *const Value as *const fsm_value_t,
                        buf.as_mut_ptr() as *mut c_char,
                        buf.len(),
                        user_data,
                    );
                    if written2 <= 0 {
                        return Value::Null;
                    }
                    let len = (written2 as usize).min(buf.len() - 1);
                    if let Ok(json_str) = core::str::from_utf8(&buf[..len]) {
                        return Value::from_json(json_str).unwrap_or(Value::Null);
                    }
                    return Value::Null;
                }
                if let Ok(json_str) = core::str::from_utf8(&buf[..needed]) {
                    Value::from_json(json_str).unwrap_or(Value::Null)
                } else {
                    Value::Null
                }
            });
        ctx.machine.register_output_resolver(&path_string, resolver_fn);
    } else {
        ctx.machine.output_resolvers.swap_remove(&path_string);
    }
    0
}

#[no_mangle]
pub extern "C" fn fsm_register_invoke_callbacks(
    ptr: *mut MachineContext,
    on_start: fsm_invoke_start_callback_t,
    on_stop: fsm_invoke_stop_callback_t,
    user_data: *mut core::ffi::c_void,
) -> c_int {
    let ctx = unsafe {
        if ptr.is_null() {
            return -1;
        }
        &mut *ptr
    };

    let start_fn: Option<lavi::InvokeStartFunction> = on_start.map(|cb| {
        alloc::rc::Rc::new(
            move |invoke_id: &str,
                  src: &Value,
                  input: &Option<Value>,
                  context: &Value,
                  event: &Value| {
                let Ok(id_cstr) = alloc::ffi::CString::new(invoke_id) else {
                    return;
                };
                let src_str = match src {
                    Value::String(s) => s.clone(),
                    other => other.to_json_string(),
                };
                let Ok(src_cstr) = alloc::ffi::CString::new(src_str) else {
                    return;
                };
                let input_ptr = match input {
                    Some(v) => v as *const Value as *const fsm_value_t,
                    None => ptr::null(),
                };
                cb(
                    id_cstr.as_ptr(),
                    src_cstr.as_ptr(),
                    input_ptr,
                    context as *const Value as *const fsm_value_t,
                    event as *const Value as *const fsm_value_t,
                    user_data,
                );
            },
        ) as lavi::InvokeStartFunction
    });

    let stop_fn: Option<lavi::InvokeStopFunction> = on_stop.map(|cb| {
        alloc::rc::Rc::new(move |invoke_id: &str| {
            if let Ok(id_cstr) = alloc::ffi::CString::new(invoke_id) {
                cb(id_cstr.as_ptr(), user_data);
            }
        }) as lavi::InvokeStopFunction
    });

    ctx.machine.register_invoke_callbacks(start_fn, stop_fn);
    0
}

#[no_mangle]
pub extern "C" fn fsm_get_snapshot_json(
    ptr: *const MachineContext,
    buffer: *mut c_char,
    buffer_len: usize,
) -> i32 {
    if ptr.is_null() {
        return -1;
    }
    let ctx = unsafe { &*ptr };
    let json = ctx.machine.get_persisted_snapshot_json();
    copy_string_to_buffer(&json, buffer, buffer_len)
}

#[no_mangle]
pub extern "C" fn fsm_restore_snapshot_json(
    ptr: *mut MachineContext,
    json_ptr: *const c_char,
) -> i32 {
    if ptr.is_null() {
        return -1;
    }
    if json_ptr.is_null() {
        return -2;
    }
    let c_str = unsafe { CStr::from_ptr(json_ptr) };
    let json_str = match c_str.to_str() {
        Ok(s) => s,
        Err(_) => return -3,
    };
    let ctx = unsafe { &mut *ptr };
    match ctx.machine.restore_from_snapshot_json(json_str) {
        Ok(()) => {
            if ctx.machine.status != lavi::MachineStatus::Stopped {
                ctx.machine.is_started = true;
            }
            ctx.machine.revive_timers();
            0
        }
        Err(_) => -4,
    }
}

#[no_mangle]
pub extern "C" fn fsm_get_output_json(
    ptr: *const MachineContext,
    buffer: *mut c_char,
    buffer_len: usize,
) -> i32 {
    if ptr.is_null() {
        return -1;
    }
    let ctx = unsafe { &*ptr };
    let output_val = match ctx.machine.get_output() {
        Some(v) => v,
        None => return -4,
    };
    let json = output_val.to_json_string();
    copy_string_to_buffer(&json, buffer, buffer_len)
}

#[no_mangle]
pub extern "C" fn fsm_get_final_leaf_output_json(
    ptr: *const MachineContext,
    buffer: *mut c_char,
    buffer_len: usize,
) -> i32 {
    if ptr.is_null() {
        return -1;
    }
    let ctx = unsafe { &*ptr };
    let output_val = match ctx.machine.get_final_leaf_output() {
        Some(v) => v,
        None => return -4,
    };
    let json = output_val.to_json_string();
    copy_string_to_buffer(&json, buffer, buffer_len)
}

#[no_mangle]
pub extern "C" fn fsm_initial_transition_json(
    ptr: *const MachineContext,
    input_json: *const c_char,
    buffer: *mut c_char,
    buffer_len: usize,
) -> i32 {
    if ptr.is_null() {
        return -1;
    }
    let ctx = unsafe { &*ptr };
    let input_opt = if input_json.is_null() {
        None
    } else {
        let c_str = unsafe { CStr::from_ptr(input_json) };
        match c_str.to_str() {
            Ok(s) => Some(s),
            Err(_) => return -4,
        }
    };
    let json = match ctx.machine.initial_transition_pure_json(input_opt) {
        Ok(s) => s,
        Err(_) => return -4,
    };
    copy_string_to_buffer(&json, buffer, buffer_len)
}

#[no_mangle]
pub extern "C" fn fsm_transition_pure_json(
    ptr: *const MachineContext,
    state_or_snapshot_json: *const c_char,
    event_json: *const c_char,
    buffer: *mut c_char,
    buffer_len: usize,
) -> i32 {
    if ptr.is_null() || state_or_snapshot_json.is_null() || event_json.is_null() {
        return -1;
    }
    let ctx = unsafe { &*ptr };
    let state_c_str = unsafe { CStr::from_ptr(state_or_snapshot_json) };
    let state_str = match state_c_str.to_str() {
        Ok(s) => s,
        Err(_) => return -4,
    };
    let event_c_str = unsafe { CStr::from_ptr(event_json) };
    let event_str = match event_c_str.to_str() {
        Ok(s) => s,
        Err(_) => return -4,
    };
    let json = match ctx.machine.transition_pure_json(state_str, event_str) {
        Ok(s) => s,
        Err(_) => return -4,
    };
    copy_string_to_buffer(&json, buffer, buffer_len)
}

#[no_mangle]
pub extern "C" fn fsm_get_meta_json(
    ptr: *const MachineContext,
    buffer: *mut c_char,
    buffer_len: usize,
) -> i32 {
    if ptr.is_null() {
        return -1;
    }
    let ctx = unsafe { &*ptr };
    let meta_val = ctx.machine.get_meta();
    let json = meta_val.to_json_string();
    copy_string_to_buffer(&json, buffer, buffer_len)
}

#[no_mangle]
pub extern "C" fn fsm_get_state_meta_json(
    ptr: *const MachineContext,
    state_path: *const c_char,
    buffer: *mut c_char,
    buffer_len: usize,
) -> i32 {
    if ptr.is_null() {
        return -1;
    }
    if state_path.is_null() {
        return -2;
    }
    let ctx = unsafe { &*ptr };
    let path_c_str = unsafe { CStr::from_ptr(state_path) };
    let path_str = match path_c_str.to_str() {
        Ok(s) => s,
        Err(_) => return -2,
    };

    let meta_opt = if path_str.is_empty()
        || path_str == "(machine)"
        || (!ctx.machine.definition.id.is_empty() && path_str == ctx.machine.definition.id)
    {
        ctx.machine.definition.meta.as_ref()
    } else {
        let resolved_path = if let Some(id) = path_str.strip_prefix('#') {
            ctx.machine.definition.id_map.get(id).map(|s| s.as_str()).unwrap_or(path_str)
        } else {
            path_str
        };
        match ctx.machine.get_state_definition(resolved_path) {
            Some(def) => def.meta.as_ref(),
            None => return -4, // State not found
        }
    };


    let meta = match meta_opt {
        Some(m) => m,
        None => return -5, // State exists but has no meta defined
    };

    let json = meta.to_json_string();
    match copy_string_to_buffer(&json, buffer, buffer_len) {
        -2 => -3,
        res => res,
    }
}

#[no_mangle]
pub extern "C" fn fsm_get_state(
    ptr: *const MachineContext,
    buffer: *mut c_char,
    buffer_len: usize,
) -> i32 {
    if ptr.is_null() {
        return -1;
    }
    let ctx = unsafe { &*ptr };
    let state_val = ctx.machine.current_state.to_value();
    let json = state_val.to_json_string();
    copy_string_to_buffer(&json, buffer, buffer_len)
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

        let green = CString::new("green").unwrap();
        let yellow = CString::new("yellow").unwrap();

        // Check initial state
        assert!(fsm_matches(machine_ptr, green.as_ptr()));
        assert!(!fsm_matches(machine_ptr, yellow.as_ptr()));

        // Transition
        let event = CString::new("TIMER").unwrap();
        fsm_send_event(machine_ptr, event.as_ptr());

        // Check new state
        assert!(fsm_matches(machine_ptr, yellow.as_ptr()));
        assert!(!fsm_matches(machine_ptr, green.as_ptr()));

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

        // Due to the external queue deferral, the "NEXT" event generated in the "step2" entry action
        // will be processed sequentially immediately after "START" completes.
        let step3 = CString::new("step3").unwrap();
        assert!(fsm_matches(machine_ptr, step3.as_ptr()));

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

        let running = CString::new("running").unwrap();
        assert!(fsm_matches(machine_ptr, running.as_ptr()));
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

        let sleep_state = CString::new("Sleep").unwrap();
        assert!(fsm_matches(machine_ptr, sleep_state.as_ptr()));
        assert!(!fsm_get_next_deadline(machine_ptr, &mut next_dl));

        // Verify destruction-order safety: freeing machine before clock, then calling set on clock
        fsm_free(machine_ptr);
        assert_eq!(fsm_simulated_clock_set(clock_ptr, 200_000), 0);
        assert_eq!(fsm_simulated_clock_now(clock_ptr), 200_000);
        fsm_simulated_clock_free(clock_ptr);
    }

    #[test]
    fn test_matches_parallel_and_compound_ffi() {
        let json = CString::new(r#"{
          "id": "displayMachine",
          "initial": "Display",
          "states": {
            "Display": {
              "type": "parallel",
              "states": {
                "Theme": {
                  "initial": "Light",
                  "states": {
                    "Light": { "on": { "TOGGLE_THEME": "Dark" } },
                    "Dark": {}
                  }
                },
                "Power": {
                  "initial": "On",
                  "states": {
                    "On": { "on": { "SLEEP": "Off" } },
                    "Off": {}
                  }
                }
              }
            }
          }
        }"#).unwrap();

        let machine_ptr = fsm_create(json.as_ptr());
        assert!(!machine_ptr.is_null());
        fsm_start(machine_ptr);

        let display = CString::new("Display").unwrap();
        let theme = CString::new("Display.Theme").unwrap();
        let light = CString::new("Display.Theme.Light").unwrap();
        let dark = CString::new("Display.Theme.Dark").unwrap();
        let power_on = CString::new("Display.Power.On").unwrap();
        let power_off = CString::new("Display.Power.Off").unwrap();

        assert!(fsm_matches(machine_ptr, display.as_ptr()));
        assert!(fsm_matches(machine_ptr, theme.as_ptr()));
        assert!(fsm_matches(machine_ptr, light.as_ptr()));
        assert!(fsm_matches(machine_ptr, power_on.as_ptr()));
        assert!(!fsm_matches(machine_ptr, dark.as_ptr()));
        assert!(!fsm_matches(machine_ptr, power_off.as_ptr()));
        assert!(!fsm_matches(ptr::null_mut(), display.as_ptr()));
        assert!(!fsm_matches(machine_ptr, ptr::null()));

        fsm_free(machine_ptr);
    }

    extern "C" fn guard_always_false(
        _guard_name: *const c_char,
        _context: *const fsm_value_t,
        _event: *const fsm_value_t,
        _user_data: *mut core::ffi::c_void,
    ) -> bool {
        false
    }

    #[test]
    fn test_wildcard_and_partial_wildcard_ffi() {
        let json = CString::new(r#"{
          "id": "wildcardFfi",
          "initial": "idle",
          "states": {
            "idle": {
              "on": {
                "*": "catchAll",
                "models.*": "anyModel",
                "models.user.*": {
                  "target": "userModelBlocked",
                  "guard": "never"
                },
                "models.user.created": "exactUserCreated"
              }
            },
            "catchAll": {},
            "anyModel": {},
            "userModelBlocked": {},
            "exactUserCreated": {}
          }
        }"#).unwrap();

        let exact_ev = CString::new("models.user.created").unwrap();
        let fallback_ev = CString::new("models.user.updated").unwrap();
        let prefix_zero_ev = CString::new("models").unwrap();
        let wild_ev = CString::new("UNHANDLED").unwrap();
        let grd_never = CString::new("never").unwrap();

        let exact_state = CString::new("exactUserCreated").unwrap();
        let any_model_state = CString::new("anyModel").unwrap();
        let catch_all_state = CString::new("catchAll").unwrap();

        // 1. Exact descriptor beats partial and full wildcard
        let m1 = fsm_create(json.as_ptr());
        fsm_register_guard(m1, grd_never.as_ptr(), Some(guard_always_false), ptr::null_mut());
        fsm_start(m1);
        fsm_send_event(m1, exact_ev.as_ptr());
        assert!(fsm_matches(m1, exact_state.as_ptr()));
        fsm_free(m1);

        // 2. Guarded longer partial descriptor falls back to shorter partial descriptor
        let m2 = fsm_create(json.as_ptr());
        fsm_register_guard(m2, grd_never.as_ptr(), Some(guard_always_false), ptr::null_mut());
        fsm_start(m2);
        fsm_send_event(m2, fallback_ev.as_ptr());
        assert!(fsm_matches(m2, any_model_state.as_ptr()));
        fsm_free(m2);

        // 3. Prefix matching (+0) matches "models.*" on "models"
        let m3 = fsm_create(json.as_ptr());
        fsm_register_guard(m3, grd_never.as_ptr(), Some(guard_always_false), ptr::null_mut());
        fsm_start(m3);
        fsm_send_event(m3, prefix_zero_ev.as_ptr());
        assert!(fsm_matches(m3, any_model_state.as_ptr()));
        fsm_free(m3);

        // 4. Unmatched event falls back to "*"
        let m4 = fsm_create(json.as_ptr());
        fsm_register_guard(m4, grd_never.as_ptr(), Some(guard_always_false), ptr::null_mut());
        fsm_start(m4);
        fsm_send_event(m4, wild_ev.as_ptr());
        assert!(fsm_matches(m4, catch_all_state.as_ptr()));
        fsm_free(m4);
    }

    #[test]
    fn test_can_has_tag_is_done_stop_ffi() {
        let json = CString::new(r##"{
          "id": "queriesFfi",
          "initial": "idle",
          "states": {
            "idle": {
              "tags": ["ready", "standby"],
              "on": {
                "START": "working",
                "BLOCKED": {
                  "target": "working",
                  "guard": "never"
                }
              }
            },
            "working": {
              "initial": "step1",
              "tags": "busy",
              "states": {
                "step1": {
                  "tags": ["phase1"],
                  "on": { "FINISH": "#queriesFfi.done" }
                }
              }
            },
            "done": {
              "type": "final",
              "tags": ["completed"]
            }
          }
        }"##).unwrap();

        let grd_never = CString::new("never").unwrap();
        let ev_start = CString::new("START").unwrap();
        let ev_blocked = CString::new("BLOCKED").unwrap();
        let ev_finish = CString::new("FINISH").unwrap();
        let ev_unknown = CString::new("UNKNOWN").unwrap();

        let tag_ready = CString::new("ready").unwrap();
        let tag_busy = CString::new("busy").unwrap();
        let tag_phase1 = CString::new("phase1").unwrap();
        let tag_completed = CString::new("completed").unwrap();

        let state_idle = CString::new("idle").unwrap();
        let state_working = CString::new("working.step1").unwrap();
        let state_done = CString::new("done").unwrap();

        let m = fsm_create(json.as_ptr());
        assert!(!m.is_null());
        fsm_register_guard(m, grd_never.as_ptr(), Some(guard_always_false), ptr::null_mut());
        fsm_start(m);

        // Null pointer error checks
        assert_eq!(fsm_can(ptr::null(), ev_start.as_ptr()), -1);
        assert_eq!(fsm_can(m, ptr::null()), -2);
        assert_eq!(fsm_has_tag(ptr::null(), tag_ready.as_ptr()), -1);
        assert_eq!(fsm_has_tag(m, ptr::null()), -2);
        assert_eq!(fsm_is_done(ptr::null()), -1);
        assert_eq!(fsm_stop(ptr::null_mut()), -1);

        // In "idle"
        assert_eq!(fsm_is_done(m), 0);
        assert_eq!(fsm_has_tag(m, tag_ready.as_ptr()), 1);
        assert_eq!(fsm_has_tag(m, tag_busy.as_ptr()), 0);
        assert_eq!(fsm_can(m, ev_start.as_ptr()), 1);
        assert_eq!(fsm_can(m, ev_blocked.as_ptr()), 0);
        assert_eq!(fsm_can(m, ev_unknown.as_ptr()), 0);
        assert!(fsm_matches(m, state_idle.as_ptr()));

        // Transition to "working.step1"
        fsm_send_event(m, ev_start.as_ptr());
        assert!(fsm_matches(m, state_working.as_ptr()));
        assert_eq!(fsm_is_done(m), 0);
        assert_eq!(fsm_has_tag(m, tag_ready.as_ptr()), 0);
        assert_eq!(fsm_has_tag(m, tag_busy.as_ptr()), 1);
        assert_eq!(fsm_has_tag(m, tag_phase1.as_ptr()), 1);
        assert_eq!(fsm_can(m, ev_finish.as_ptr()), 1);

        // Transition to "done"
        fsm_send_event(m, ev_finish.as_ptr());
        assert!(fsm_matches(m, state_done.as_ptr()));
        assert_eq!(fsm_is_done(m), 1);
        assert_eq!(fsm_has_tag(m, tag_completed.as_ptr()), 1);
        assert_eq!(fsm_can(m, ev_start.as_ptr()), 0);
        fsm_free(m);

        // Stop test
        let m2 = fsm_create(json.as_ptr());
        fsm_start(m2);
        assert_eq!(fsm_can(m2, ev_start.as_ptr()), 1);
        assert_eq!(fsm_stop(m2), 0);
        assert_eq!(fsm_can(m2, ev_start.as_ptr()), 0);
        assert_eq!(fsm_is_done(m2), 0);
        fsm_send_event(m2, ev_start.as_ptr());
        assert!(fsm_matches(m2, state_idle.as_ptr()));
        fsm_free(m2);
    }

    #[test]
    fn test_snapshot_persistence_ffi() {
        let json = CString::new(r#"{
          "id": "snapFfi",
          "initial": "a",
          "context": { "count": 0 },
          "states": {
            "a": {
              "on": {
                "NEXT": {
                  "target": "b",
                  "actions": [
                    { "type": "xstate.assign", "assignment": { "count": 42 } }
                  ]
                }
              }
            },
            "b": {
              "on": { "FINISH": "c" }
            },
            "c": { "type": "final" }
          }
        }"#).unwrap();

        let m1 = fsm_create(json.as_ptr());
        assert!(!m1.is_null());
        fsm_start(m1);

        let ev_next = CString::new("NEXT").unwrap();
        fsm_send_event(m1, ev_next.as_ptr());
        let state_b = CString::new("b").unwrap();
        assert!(fsm_matches(m1, state_b.as_ptr()));

        // 1. Probe length
        let req = fsm_get_snapshot_json(m1, ptr::null_mut(), 0);
        assert!(req > 0);

        // 2. Buffer too small
        let mut small_buf = [0 as c_char; 4];
        assert_eq!(fsm_get_snapshot_json(m1, small_buf.as_mut_ptr(), 4), -2);

        // 3. Serialize
        let mut snap_buf = alloc::vec![0 as c_char; req as usize];
        let written = fsm_get_snapshot_json(m1, snap_buf.as_mut_ptr(), req as usize);
        assert!(written > 0);
        assert_eq!(written as usize, req as usize - 1);

        // 4. Null checks
        assert_eq!(fsm_get_snapshot_json(ptr::null(), snap_buf.as_mut_ptr(), req as usize), -1);
        assert_eq!(fsm_restore_snapshot_json(ptr::null_mut(), snap_buf.as_ptr()), -1);
        assert_eq!(fsm_restore_snapshot_json(m1, ptr::null()), -2);

        // 5. Restore into new machine
        let m2 = fsm_create(json.as_ptr());
        assert!(!m2.is_null());
        assert_eq!(fsm_restore_snapshot_json(m2, snap_buf.as_ptr()), 0);
        assert!(fsm_matches(m2, state_b.as_ptr()));

        // 6. Transition restored machine to final state
        let ev_finish = CString::new("FINISH").unwrap();
        fsm_send_event(m2, ev_finish.as_ptr());
        let state_c = CString::new("c").unwrap();
        assert!(fsm_matches(m2, state_c.as_ptr()));
        assert_eq!(fsm_is_done(m2), 1);

        fsm_free(m1);
        fsm_free(m2);
    }

    #[test]
    fn test_m5_c_ffi_parity() {
        let json = CString::new(r#"{
          "id": "m5Fsm",
          "initial": "init",
          "meta": { "version": "1.0" },
          "output": { "result": "done", "val": 42 },
          "states": {
            "init": {
              "meta": { "label": "Initial State" },
              "on": {
                "FINISH": "final"
              }
            },
            "final": {
              "type": "final"
            }
          }
        }"#).unwrap();

        let input_json = CString::new(r#"{"seed": 100}"#).unwrap();

        // 1. Creation with input
        let m = fsm_create_with_input(json.as_ptr(), input_json.as_ptr());
        assert!(!m.is_null());

        // Pre-start event is dropped
        let ev_finish = CString::new(r#"{"type":"FINISH"}"#).unwrap();
        assert_eq!(fsm_send_json_event(m, ev_finish.as_ptr()), 0);

        let state_init = CString::new("init").unwrap();
        let state_final = CString::new("final").unwrap();
        assert!(!fsm_matches(m, state_final.as_ptr())); // dropped pre-start event did not transition to final

        fsm_start(m);
        assert!(fsm_is_in_state(m, state_init.as_ptr()));



        // 2. State inspection
        let mut state_buf = [0 as c_char; 32];
        let state_len = fsm_get_state(m, state_buf.as_mut_ptr(), state_buf.len());
        assert!(state_len > 0);
        let state_cstr = unsafe { CStr::from_ptr(state_buf.as_ptr()) };
        assert_eq!(state_cstr.to_str().unwrap(), "\"init\"");

        // 3. Meta inspection
        let meta_req = fsm_get_meta_json(m, ptr::null_mut(), 0);
        assert!(meta_req > 0);
        let mut meta_buf = alloc::vec![0 as c_char; meta_req as usize];
        let meta_written = fsm_get_meta_json(m, meta_buf.as_mut_ptr(), meta_req as usize);
        assert_eq!(meta_written, meta_req - 1);
        let meta_str = unsafe { CStr::from_ptr(meta_buf.as_ptr()) }.to_str().unwrap();
        assert!(meta_str.contains("\"version\":\"1.0\""));
        assert!(meta_str.contains("\"label\":\"Initial State\""));

        // State-specific meta
        let mut sm_buf = [0 as c_char; 64];
        let sm_len = fsm_get_state_meta_json(m, state_init.as_ptr(), sm_buf.as_mut_ptr(), sm_buf.len());
        assert!(sm_len > 0);
        let sm_str = unsafe { CStr::from_ptr(sm_buf.as_ptr()) }.to_str().unwrap();
        assert!(sm_str.contains("\"label\":\"Initial State\""));

        // Non-existent state
        let missing_state = CString::new("nonexistent").unwrap();
        assert_eq!(fsm_get_state_meta_json(m, missing_state.as_ptr(), sm_buf.as_mut_ptr(), sm_buf.len()), -4);

        // State with no meta
        let state_final = CString::new("final").unwrap();
        assert_eq!(fsm_get_state_meta_json(m, state_final.as_ptr(), sm_buf.as_mut_ptr(), sm_buf.len()), -5);

        // 4. Output before final is -4
        let mut out_buf = [0 as c_char; 64];
        assert_eq!(fsm_get_output_json(m, out_buf.as_mut_ptr(), out_buf.len()), -4);

        // 5. Send JSON event
        assert_eq!(fsm_send_json_event(m, ev_finish.as_ptr()), 0);
        assert!(fsm_matches(m, state_final.as_ptr()));
        assert_eq!(fsm_is_done(m), 1);

        // Output after final
        let out_req = fsm_get_output_json(m, ptr::null_mut(), 0);
        assert!(out_req > 0);
        assert_eq!(fsm_get_output_json(m, out_buf.as_mut_ptr(), 4), -2); // buffer too small
        let out_written = fsm_get_output_json(m, out_buf.as_mut_ptr(), out_buf.len());
        assert!(out_written > 0);
        let out_str = unsafe { CStr::from_ptr(out_buf.as_ptr()) }.to_str().unwrap();
        assert!(out_str.contains("\"result\":\"done\""));
        assert!(out_str.contains("\"val\":42"));

        fsm_destroy(m);
    }
}

#[cfg(all(not(test), target_os = "none"))]
struct CMalloc;

#[cfg(all(not(test), target_os = "none"))]
unsafe impl core::alloc::GlobalAlloc for CMalloc {
    unsafe fn alloc(&self, layout: core::alloc::Layout) -> *mut u8 {
        extern "C" {
            fn malloc(size: usize) -> *mut u8;
        }
        malloc(layout.size())
    }
    unsafe fn dealloc(&self, ptr: *mut u8, _layout: core::alloc::Layout) {
        extern "C" {
            fn free(ptr: *mut u8);
        }
        free(ptr);
    }
}

#[cfg(all(not(test), target_os = "none"))]
#[global_allocator]
static GLOBAL_ALLOCATOR: CMalloc = CMalloc;

#[cfg(all(not(test), target_os = "none"))]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}

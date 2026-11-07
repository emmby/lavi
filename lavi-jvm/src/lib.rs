extern crate alloc;

use indexmap::IndexMap;
use jni::objects::{JClass, JObject, JObjectArray, JString};
use jni::sys::{jboolean, jlong, jobject, jstring, JNI_FALSE, JNI_TRUE};
use jni::JNIEnv;
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;

pub enum PendingCommand {
    Send {
        event_name: String,
        payload_val: lavi::Value,
    },
    Stop,
}

pub struct SimulatedClockPeer {
    pub clock: lavi::SimulatedClock,
    pub closed: Cell<bool>,
}

pub struct MachinePeer {
    pub machine: RefCell<Option<lavi::RuntimeMachine>>,
    pub active_machine: Cell<*const lavi::RuntimeMachine>,
    pub callback_depth: Cell<usize>,
    pub aborted: Cell<bool>,
    pub pending_commands: RefCell<VecDeque<PendingCommand>>,
    pub guards: RefCell<IndexMap<String, lavi::GuardFunction>>,
    pub actions: RefCell<IndexMap<String, lavi::ActionFunction>>,
    pub is_closed: Cell<bool>,
    pub active_env: Cell<*mut ()>,
    pub active_obj: Cell<jobject>,
}

struct ActiveScope<'a> {
    peer: &'a MachinePeer,
    prev_env: *mut (),
    prev_obj: jobject,
}

impl<'a> ActiveScope<'a> {
    fn new(peer: &'a MachinePeer, env: &mut JNIEnv<'_>, obj: jobject) -> Self {
        let prev_env = peer.active_env.get();
        let prev_obj = peer.active_obj.get();
        peer.active_env.set(env as *mut JNIEnv as *mut ());
        peer.active_obj.set(obj);
        Self {
            peer,
            prev_env,
            prev_obj,
        }
    }
}

impl<'a> Drop for ActiveScope<'a> {
    fn drop(&mut self) {
        self.peer.active_env.set(self.prev_env);
        self.peer.active_obj.set(self.prev_obj);
    }
}

struct CallbackGuard<'a> {
    depth: &'a Cell<usize>,
}

impl<'a> CallbackGuard<'a> {
    fn new(depth: &'a Cell<usize>) -> Self {
        depth.set(depth.get().saturating_add(1));
        Self { depth }
    }
}

impl<'a> Drop for CallbackGuard<'a> {
    fn drop(&mut self) {
        let current = self.depth.get();
        if current > 0 {
            self.depth.set(current - 1);
        }
    }
}

impl MachinePeer {
    fn with_machine<R>(&self, f: impl FnOnce(&lavi::RuntimeMachine) -> R) -> Result<R, &'static str> {
        if self.is_closed.get() {
            return Err("Machine is closed");
        }
        if self.callback_depth.get() > 0 {
            let ptr = self.active_machine.get();
            if !ptr.is_null() {
                let m = unsafe { &*ptr };
                return Ok(f(m));
            }
        }
        let guard = self.machine.borrow();
        match guard.as_ref() {
            Some(m) => Ok(f(m)),
            None => Err("Machine is closed"),
        }
    }

    fn step_transition(
        &self,
        env: &mut JNIEnv<'_>,
        obj: jobject,
        event_name: &str,
        payload_val: &lavi::Value,
    ) -> Result<(), jni::errors::Error> {
        if self.is_closed.get() {
            let _ = env.throw_new("java/lang/IllegalStateException", "Machine is closed");
            return Ok(());
        }
        if self.aborted.get() {
            return Ok(());
        }

        let _scope = ActiveScope::new(self, env, obj);
        let guards = self.guards.borrow().clone();
        let actions = self.actions.borrow().clone();

        {
            let mut guard = self.machine.borrow_mut();
            let Some(ref mut machine_ref) = *guard else {
                let _ = env.throw_new("java/lang/IllegalStateException", "Machine is closed");
                return Ok(());
            };
            if !machine_ref.is_started || machine_ref.status == lavi::MachineStatus::Stopped {
                return Ok(());
            }
            self.active_machine
                .set(machine_ref as *const lavi::RuntimeMachine);
            machine_ref.transition(event_name, payload_val, &guards, &actions);
            self.active_machine.set(std::ptr::null());
        }

        if self.aborted.get() || env.exception_check()? {
            self.aborted.set(false);
            return Ok(());
        }

        self.drain_pending_commands(env, obj)
    }

    fn step_tick(&self, env: &mut JNIEnv<'_>, obj: jobject) -> Result<(), jni::errors::Error> {
        if self.is_closed.get() {
            let _ = env.throw_new("java/lang/IllegalStateException", "Machine is closed");
            return Ok(());
        }
        if self.aborted.get() {
            return Ok(());
        }

        let _scope = ActiveScope::new(self, env, obj);
        let guards = self.guards.borrow().clone();
        let actions = self.actions.borrow().clone();

        {
            let mut guard = self.machine.borrow_mut();
            let Some(ref mut machine_ref) = *guard else {
                let _ = env.throw_new("java/lang/IllegalStateException", "Machine is closed");
                return Ok(());
            };
            if !machine_ref.is_started
                || machine_ref.status == lavi::MachineStatus::Stopped
                || machine_ref.is_done()
                || machine_ref.status == lavi::MachineStatus::Done
            {
                return Ok(());
            }
            self.active_machine
                .set(machine_ref as *const lavi::RuntimeMachine);
            let _ = machine_ref.tick(&guards, &actions);
            self.active_machine.set(std::ptr::null());
        }

        if self.aborted.get() || env.exception_check()? {
            self.aborted.set(false);
            return Ok(());
        }

        self.drain_pending_commands(env, obj)
    }

    fn step_start(&self, env: &mut JNIEnv<'_>, obj: jobject) -> Result<(), jni::errors::Error> {
        if self.is_closed.get() {
            let _ = env.throw_new("java/lang/IllegalStateException", "Machine is closed");
            return Ok(());
        }
        if self.aborted.get() {
            return Ok(());
        }

        let _scope = ActiveScope::new(self, env, obj);
        let guards = self.guards.borrow().clone();
        let actions = self.actions.borrow().clone();

        {
            let mut guard = self.machine.borrow_mut();
            let Some(ref mut machine_ref) = *guard else {
                let _ = env.throw_new("java/lang/IllegalStateException", "Machine is closed");
                return Ok(());
            };
            if !machine_ref.is_started {
                self.active_machine
                    .set(machine_ref as *const lavi::RuntimeMachine);
                machine_ref.start(&guards, &actions);
                self.active_machine.set(std::ptr::null());
            }
        }

        if self.aborted.get() || env.exception_check()? {
            self.aborted.set(false);
            return Ok(());
        }

        self.drain_pending_commands(env, obj)
    }

    fn step_stop(&self) {
        let mut guard = self.machine.borrow_mut();
        if let Some(ref mut m) = *guard {
            m.stop();
        }
    }

    fn drain_pending_commands(
        &self,
        env: &mut JNIEnv<'_>,
        obj: jobject,
    ) -> Result<(), jni::errors::Error> {
        loop {
            if self.aborted.get() || env.exception_check()? {
                self.aborted.set(false);
                break;
            }
            let next_cmd = self.pending_commands.borrow_mut().pop_front();
            let Some(cmd) = next_cmd else {
                break;
            };

            match cmd {
                PendingCommand::Send {
                    event_name,
                    payload_val,
                } => {
                    self.step_transition(env, obj, &event_name, &payload_val)?;
                }
                PendingCommand::Stop => {
                    self.step_stop();
                }
            }
        }
        Ok(())
    }

    fn get_next_deadline(&self) -> Result<Option<u64>, &'static str> {
        self.with_machine(|m| {
            if m.is_started
                && m.status != lavi::MachineStatus::Stopped
                && !m.is_done()
                && m.status != lavi::MachineStatus::Done
            {
                m.next_timer_deadline()
            } else {
                None
            }
        })
    }
}

// ---------------------------------------------------------------------------
// JNI Exports for SimulatedClock
// ---------------------------------------------------------------------------

#[no_mangle]
pub extern "system" fn Java_io_lavi_SimulatedClock_nativeCreate(
    _env: JNIEnv,
    _class: JClass,
    initial_ms: jlong,
) -> jlong {
    let peer = SimulatedClockPeer {
        clock: lavi::SimulatedClock::with_initial(initial_ms as u64),
        closed: Cell::new(false),
    };
    Box::into_raw(Box::new(peer)) as jlong
}

#[no_mangle]
pub extern "system" fn Java_io_lavi_SimulatedClock_nativeNow(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
) -> jlong {
    let peer = unsafe { &*(handle as *const SimulatedClockPeer) };
    if peer.closed.get() {
        let _ = env.throw_new("java/lang/IllegalStateException", "SimulatedClock is closed");
        return 0;
    }
    peer.clock.now() as jlong
}

#[no_mangle]
pub extern "system" fn Java_io_lavi_SimulatedClock_nativeSet(
    mut env: JNIEnv,
    _this_obj: JObject,
    handle: jlong,
    machines: JObjectArray,
    target_ms: jlong,
) {
    let clock_peer = unsafe { &*(handle as *const SimulatedClockPeer) };
    if clock_peer.closed.get() {
        let _ = env.throw_new("java/lang/IllegalStateException", "SimulatedClock is closed");
        return;
    }

    let target_ms = target_ms as u64;

    let len = match env.get_array_length(&machines) {
        Ok(l) => l,
        Err(_) => return,
    };

    let mut machine_entries: Vec<(jobject, *const MachinePeer)> = Vec::with_capacity(len as usize);
    for i in 0..len {
        let elem = match env.get_object_array_element(&machines, i) {
            Ok(o) => o,
            Err(_) => return,
        };
        if elem.is_null() {
            continue;
        }
        let handle_val = match env.get_field(&elem, "nativeHandle", "J") {
            Ok(v) => match v.j() {
                Ok(h) => h,
                Err(_) => continue,
            },
            Err(_) => continue,
        };
        if handle_val != 0 {
            let m_peer = handle_val as *const MachinePeer;
            machine_entries.push((elem.into_raw(), m_peer));
        }
    }

    let mut last_deadline: Option<u64> = None;
    let mut same_deadline_steps = 0usize;
    let mut total_steps = 0usize;

    loop {
        if env.exception_check().unwrap_or(false) {
            return;
        }

        let mut min_deadline: Option<u64> = None;
        for (_, peer_ptr) in &machine_entries {
            let peer = unsafe { &**peer_ptr };
            if let Ok(Some(d)) = peer.get_next_deadline() {
                min_deadline = Some(min_deadline.map_or(d, |curr| curr.min(d)));
            }
        }

        let next_deadline = match min_deadline {
            Some(d) => d,
            None => break,
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

        clock_peer.clock.set_now_raw(next_deadline);

        for (m_obj, peer_ptr) in &machine_entries {
            let peer = unsafe { &**peer_ptr };
            let _ = peer.step_tick(&mut env, *m_obj);
            if env.exception_check().unwrap_or(false) {
                return;
            }
        }
    }

    if !env.exception_check().unwrap_or(false) {
        clock_peer.clock.set_now_raw(target_ms);
    }
}

#[no_mangle]
pub extern "system" fn Java_io_lavi_SimulatedClock_nativeFree(
    _env: JNIEnv,
    _class: JClass,
    handle: jlong,
) {
    if handle != 0 {
        let peer = unsafe { Box::from_raw(handle as *mut SimulatedClockPeer) };
        peer.closed.set(true);
        drop(peer);
    }
}

// ---------------------------------------------------------------------------
// JNI Exports for Machine
// ---------------------------------------------------------------------------

#[no_mangle]
pub extern "system" fn Java_io_lavi_Machine_nativeCreate(
    mut env: JNIEnv,
    _class: JClass,
    json_config: JString,
    clock_handle: jlong,
) -> jlong {
    let json_str: String = match env.get_string(&json_config) {
        Ok(s) => s.into(),
        Err(_) => return 0,
    };

    let def = match lavi::from_json_str(&json_str) {
        Ok(d) => d,
        Err(e) => {
            let _ = env.throw_new(
                "java/lang/IllegalArgumentException",
                format!("Invalid machine JSON: {e}"),
            );
            return 0;
        }
    };

    let mut runtime_machine = match lavi::RuntimeMachine::from_definition(def) {
        Ok(m) => m,
        Err(e) => {
            let _ = env.throw_new(
                "java/lang/IllegalArgumentException",
                format!("Invalid machine definition: {e}"),
            );
            return 0;
        }
    };

    if clock_handle != 0 {
        let clock_peer = unsafe { &*(clock_handle as *const SimulatedClockPeer) };
        if clock_peer.closed.get() {
            let _ = env.throw_new(
                "java/lang/IllegalStateException",
                "SimulatedClock is closed",
            );
            return 0;
        }
        runtime_machine.register_clock(clock_peer.clock.as_clock_fn());
    }

    let peer = MachinePeer {
        machine: RefCell::new(Some(runtime_machine)),
        active_machine: Cell::new(std::ptr::null()),
        callback_depth: Cell::new(0),
        aborted: Cell::new(false),
        pending_commands: RefCell::new(VecDeque::new()),
        guards: RefCell::new(IndexMap::new()),
        actions: RefCell::new(IndexMap::new()),
        is_closed: Cell::new(false),
        active_env: Cell::new(std::ptr::null_mut()),
        active_obj: Cell::new(std::ptr::null_mut()),
    };

    Box::into_raw(Box::new(peer)) as jlong
}

#[no_mangle]
pub extern "system" fn Java_io_lavi_Machine_nativeRegisterAction(
    mut env: JNIEnv,
    _class: JClass,
    machine_handle: jlong,
    name: JString,
) {
    let m_peer = unsafe { &*(machine_handle as *const MachinePeer) };
    if m_peer.is_closed.get() {
        let _ = env.throw_new("java/lang/IllegalStateException", "Machine is closed");
        return;
    }

    let action_name: String = match env.get_string(&name) {
        Ok(s) => s.into(),
        Err(_) => return,
    };

    let peer_ptr = machine_handle as *const MachinePeer;
    let name_clone = action_name.clone();

    let action_fn: lavi::ActionFunction = alloc::rc::Rc::new(move |ctx, evt, _params| {
        let peer = unsafe { &*peer_ptr };
        if peer.is_closed.get() || peer.aborted.get() {
            return;
        }

        let env_ptr = peer.active_env.get() as *mut JNIEnv;
        let obj = peer.active_obj.get();
        if env_ptr.is_null() || obj.is_null() {
            return;
        }

        let env = unsafe { &mut *env_ptr };
        let _guard = CallbackGuard::new(&peer.callback_depth);

        let res = (|| -> Result<(), jni::errors::Error> {
            let ctx_json = ctx.to_json_string();
            let evt_json = evt.to_json_string();

            let j_ctx_str = env.new_string(ctx_json)?;
            let j_evt_str = env.new_string(evt_json)?;

            let j_ctx = env
                .call_static_method(
                    "io/lavi/Value",
                    "fromJson",
                    "(Ljava/lang/String;)Lio/lavi/Value;",
                    &[(&j_ctx_str).into()],
                )?
                .l()?;

            let j_evt = env
                .call_static_method(
                    "io/lavi/Value",
                    "fromJson",
                    "(Ljava/lang/String;)Lio/lavi/Value;",
                    &[(&j_evt_str).into()],
                )?
                .l()?;

            let j_name = env.new_string(&name_clone)?;
            let target_obj = unsafe { JObject::from_raw(obj) };

            env.call_method(
                &target_obj,
                "dispatchAction",
                "(Ljava/lang/String;Lio/lavi/Value;Lio/lavi/Value;)V",
                &[(&j_name).into(), (&j_ctx).into(), (&j_evt).into()],
            )?;

            Ok(())
        })();

        if res.is_err() || env.exception_check().unwrap_or(false) {
            peer.aborted.set(true);
        }
    });

    m_peer.actions.borrow_mut().insert(action_name, action_fn);
}

#[no_mangle]
pub extern "system" fn Java_io_lavi_Machine_nativeRegisterGuard(
    mut env: JNIEnv,
    _class: JClass,
    machine_handle: jlong,
    name: JString,
) {
    let m_peer = unsafe { &*(machine_handle as *const MachinePeer) };
    if m_peer.is_closed.get() {
        let _ = env.throw_new("java/lang/IllegalStateException", "Machine is closed");
        return;
    }

    let guard_name: String = match env.get_string(&name) {
        Ok(s) => s.into(),
        Err(_) => return,
    };

    let peer_ptr = machine_handle as *const MachinePeer;
    let name_clone = guard_name.clone();

    let guard_fn: lavi::GuardFunction = alloc::rc::Rc::new(move |ctx, evt, _params| {
        let peer = unsafe { &*peer_ptr };
        if peer.is_closed.get() || peer.aborted.get() {
            return false;
        }

        let env_ptr = peer.active_env.get() as *mut JNIEnv;
        let obj = peer.active_obj.get();
        if env_ptr.is_null() || obj.is_null() {
            return false;
        }

        let env = unsafe { &mut *env_ptr };
        let _guard = CallbackGuard::new(&peer.callback_depth);

        let res = (|| -> Result<bool, jni::errors::Error> {
            let ctx_json = ctx.to_json_string();
            let evt_json = evt.to_json_string();

            let j_ctx_str = env.new_string(ctx_json)?;
            let j_evt_str = env.new_string(evt_json)?;

            let j_ctx = env
                .call_static_method(
                    "io/lavi/Value",
                    "fromJson",
                    "(Ljava/lang/String;)Lio/lavi/Value;",
                    &[(&j_ctx_str).into()],
                )?
                .l()?;

            let j_evt = env
                .call_static_method(
                    "io/lavi/Value",
                    "fromJson",
                    "(Ljava/lang/String;)Lio/lavi/Value;",
                    &[(&j_evt_str).into()],
                )?
                .l()?;

            let j_name = env.new_string(&name_clone)?;
            let target_obj = unsafe { JObject::from_raw(obj) };

            let ret = env.call_method(
                &target_obj,
                "dispatchGuard",
                "(Ljava/lang/String;Lio/lavi/Value;Lio/lavi/Value;)Z",
                &[(&j_name).into(), (&j_ctx).into(), (&j_evt).into()],
            )?;

            ret.z()
        })();

        match res {
            Ok(b) => {
                if env.exception_check().unwrap_or(false) {
                    peer.aborted.set(true);
                    false
                } else {
                    b
                }
            }
            Err(_) => {
                peer.aborted.set(true);
                false
            }
        }
    });

    m_peer.guards.borrow_mut().insert(guard_name, guard_fn);
}

#[no_mangle]
pub extern "system" fn Java_io_lavi_Machine_nativeRegisterClockCallback(
    mut env: JNIEnv,
    _class: JClass,
    machine_handle: jlong,
) {
    let m_peer = unsafe { &*(machine_handle as *const MachinePeer) };
    if m_peer.is_closed.get() {
        let _ = env.throw_new("java/lang/IllegalStateException", "Machine is closed");
        return;
    }

    let peer_ptr = machine_handle as *const MachinePeer;
    let clock_fn: alloc::rc::Rc<dyn Fn() -> u64> = alloc::rc::Rc::new(move || {
        let peer = unsafe { &*peer_ptr };
        if peer.is_closed.get() || peer.aborted.get() {
            return 0;
        }

        let env_ptr = peer.active_env.get() as *mut JNIEnv;
        let obj = peer.active_obj.get();
        if env_ptr.is_null() || obj.is_null() {
            return 0;
        }

        let env = unsafe { &mut *env_ptr };
        let target_obj = unsafe { JObject::from_raw(obj) };

        match env.call_method(&target_obj, "dispatchClock", "()J", &[]) {
            Ok(ret) => match ret.j() {
                Ok(v) => v as u64,
                Err(_) => {
                    peer.aborted.set(true);
                    0
                }
            },
            Err(_) => {
                peer.aborted.set(true);
                0
            }
        }
    });

    let mut guard = m_peer.machine.borrow_mut();
    if let Some(ref mut m) = *guard {
        m.register_clock(clock_fn);
    }
}

#[no_mangle]
pub extern "system" fn Java_io_lavi_Machine_nativeRegisterSimulatedClock(
    mut env: JNIEnv,
    _class: JClass,
    machine_handle: jlong,
    clock_handle: jlong,
) {
    let m_peer = unsafe { &*(machine_handle as *const MachinePeer) };
    if m_peer.is_closed.get() {
        let _ = env.throw_new("java/lang/IllegalStateException", "Machine is closed");
        return;
    }

    let clock_peer = unsafe { &*(clock_handle as *const SimulatedClockPeer) };
    if clock_peer.closed.get() {
        let _ = env.throw_new(
            "java/lang/IllegalStateException",
            "SimulatedClock is closed",
        );
        return;
    }

    let mut guard = m_peer.machine.borrow_mut();
    if let Some(ref mut m) = *guard {
        m.register_clock(clock_peer.clock.as_clock_fn());
    }
}

#[no_mangle]
pub extern "system" fn Java_io_lavi_Machine_nativeStart(
    mut env: JNIEnv,
    this_obj: JObject,
    handle: jlong,
) {
    let peer = unsafe { &*(handle as *const MachinePeer) };
    if peer.is_closed.get() {
        let _ = env.throw_new("java/lang/IllegalStateException", "Machine is closed");
        return;
    }

    let _ = peer.step_start(&mut env, this_obj.as_raw());
}

#[no_mangle]
pub extern "system" fn Java_io_lavi_Machine_nativeStop(
    mut env: JNIEnv,
    _this_obj: JObject,
    handle: jlong,
) {
    let peer = unsafe { &*(handle as *const MachinePeer) };
    if peer.is_closed.get() {
        let _ = env.throw_new("java/lang/IllegalStateException", "Machine is closed");
        return;
    }

    if peer.callback_depth.get() > 0 {
        peer.pending_commands
            .borrow_mut()
            .push_back(PendingCommand::Stop);
        return;
    }

    peer.step_stop();
}

#[no_mangle]
pub extern "system" fn Java_io_lavi_Machine_nativeFree(
    _env: JNIEnv,
    _class: JClass,
    handle: jlong,
) {
    if handle != 0 {
        let peer = unsafe { Box::from_raw(handle as *mut MachinePeer) };
        peer.is_closed.set(true);
        *peer.machine.borrow_mut() = None;
        peer.actions.borrow_mut().clear();
        peer.guards.borrow_mut().clear();
        peer.pending_commands.borrow_mut().clear();
        drop(peer);
    }
}

#[no_mangle]
pub extern "system" fn Java_io_lavi_Machine_nativeMatches(
    mut env: JNIEnv,
    _this_obj: JObject,
    handle: jlong,
    state_path: JString,
) -> jboolean {
    let peer = unsafe { &*(handle as *const MachinePeer) };
    if peer.is_closed.get() {
        let _ = env.throw_new("java/lang/IllegalStateException", "Machine is closed");
        return JNI_FALSE;
    }

    let path_str: String = match env.get_string(&state_path) {
        Ok(s) => s.into(),
        Err(_) => return JNI_FALSE,
    };

    let res = peer.with_machine(|m| {
        if !m.is_started {
            false
        } else {
            m.matches(&path_str)
        }
    });

    match res {
        Ok(true) => JNI_TRUE,
        _ => JNI_FALSE,
    }
}

#[no_mangle]
pub extern "system" fn Java_io_lavi_Machine_nativeCan(
    mut env: JNIEnv,
    this_obj: JObject,
    handle: jlong,
    event: JString,
    payload_json: JString,
) -> jboolean {
    let peer = unsafe { &*(handle as *const MachinePeer) };
    if peer.is_closed.get() {
        let _ = env.throw_new("java/lang/IllegalStateException", "Machine is closed");
        return JNI_FALSE;
    }

    let _scope = ActiveScope::new(peer, &mut env, this_obj.as_raw());

    let event_str: String = match env.get_string(&event) {
        Ok(s) => s.into(),
        Err(_) => return JNI_FALSE,
    };

    let payload_val = if !payload_json.is_null() {
        let p_str: String = match env.get_string(&payload_json) {
            Ok(s) => s.into(),
            Err(_) => return JNI_FALSE,
        };
        match lavi::Value::from_json(&p_str) {
            Ok(parsed) => {
                let mut obj = parsed.into_object().unwrap_or_default();
                if !obj.contains_key("type") {
                    obj.insert(
                        "type".to_string(),
                        lavi::Value::String(event_str.clone()),
                    );
                }
                lavi::Value::Object(obj)
            }
            Err(e) => {
                let _ = env.throw_new(
                    "java/lang/IllegalArgumentException",
                    format!("Invalid payload JSON: {e:?}"),
                );
                return JNI_FALSE;
            }
        }
    } else {
        lavi::Value::event(&event_str)
    };

    let guards = peer.guards.borrow().clone();
    let res = peer.with_machine(|m| {
        if !m.is_started || m.status == lavi::MachineStatus::Stopped {
            false
        } else {
            let prev_active = peer.active_machine.get();
            if prev_active.is_null() {
                peer.active_machine.set(m as *const lavi::RuntimeMachine);
            }
            let can_res = m.can_with_guards(&event_str, &payload_val, &guards);
            if prev_active.is_null() {
                peer.active_machine.set(std::ptr::null());
            }
            can_res
        }
    });

    match res {
        Ok(true) => JNI_TRUE,
        _ => JNI_FALSE,
    }
}

#[no_mangle]
pub extern "system" fn Java_io_lavi_Machine_nativeHasTag(
    mut env: JNIEnv,
    _this_obj: JObject,
    handle: jlong,
    tag: JString,
) -> jboolean {
    let peer = unsafe { &*(handle as *const MachinePeer) };
    if peer.is_closed.get() {
        let _ = env.throw_new("java/lang/IllegalStateException", "Machine is closed");
        return JNI_FALSE;
    }

    let tag_str: String = match env.get_string(&tag) {
        Ok(s) => s.into(),
        Err(_) => return JNI_FALSE,
    };

    let res = peer.with_machine(|m| {
        if !m.is_started {
            false
        } else {
            m.has_tag(&tag_str)
        }
    });

    match res {
        Ok(true) => JNI_TRUE,
        _ => JNI_FALSE,
    }
}

#[no_mangle]
pub extern "system" fn Java_io_lavi_Machine_nativeIsDone(
    mut env: JNIEnv,
    _this_obj: JObject,
    handle: jlong,
) -> jboolean {
    let peer = unsafe { &*(handle as *const MachinePeer) };
    if peer.is_closed.get() {
        let _ = env.throw_new("java/lang/IllegalStateException", "Machine is closed");
        return JNI_FALSE;
    }

    let res = peer.with_machine(|m| {
        if !m.is_started {
            false
        } else {
            m.is_done()
        }
    });

    match res {
        Ok(true) => JNI_TRUE,
        _ => JNI_FALSE,
    }
}

#[no_mangle]
pub extern "system" fn Java_io_lavi_Machine_nativeSend(
    mut env: JNIEnv,
    this_obj: JObject,
    handle: jlong,
    event: JString,
    payload_json: JString,
) {
    let peer = unsafe { &*(handle as *const MachinePeer) };
    if peer.is_closed.get() {
        let _ = env.throw_new("java/lang/IllegalStateException", "Machine is closed");
        return;
    }

    let event_str: String = match env.get_string(&event) {
        Ok(s) => s.into(),
        Err(_) => return,
    };

    let payload_val = if !payload_json.is_null() {
        let p_str: String = match env.get_string(&payload_json) {
            Ok(s) => s.into(),
            Err(_) => return,
        };
        match lavi::Value::from_json(&p_str) {
            Ok(parsed) => {
                let mut obj = parsed.into_object().unwrap_or_default();
                if !obj.contains_key("type") {
                    obj.insert(
                        "type".to_string(),
                        lavi::Value::String(event_str.clone()),
                    );
                }
                lavi::Value::Object(obj)
            }
            Err(e) => {
                let _ = env.throw_new(
                    "java/lang/IllegalArgumentException",
                    format!("Invalid payload JSON: {e:?}"),
                );
                return;
            }
        }
    } else {
        lavi::Value::event(&event_str)
    };

    let is_running = peer.with_machine(|m| {
        m.is_started && m.status != lavi::MachineStatus::Stopped
    });

    match is_running {
        Ok(true) => {}
        _ => return,
    }

    if peer.callback_depth.get() > 0 {
        peer.pending_commands
            .borrow_mut()
            .push_back(PendingCommand::Send {
                event_name: event_str,
                payload_val,
            });
        return;
    }

    let _ = peer.step_transition(&mut env, this_obj.as_raw(), &event_str, &payload_val);
}

#[no_mangle]
pub extern "system" fn Java_io_lavi_Machine_nativeTick(
    mut env: JNIEnv,
    this_obj: JObject,
    handle: jlong,
) {
    let peer = unsafe { &*(handle as *const MachinePeer) };
    if peer.is_closed.get() {
        let _ = env.throw_new("java/lang/IllegalStateException", "Machine is closed");
        return;
    }

    let _ = peer.step_tick(&mut env, this_obj.as_raw());
}

#[no_mangle]
pub extern "system" fn Java_io_lavi_Machine_nativeGetNextDeadline(
    mut env: JNIEnv,
    _this_obj: JObject,
    handle: jlong,
) -> jlong {
    let peer = unsafe { &*(handle as *const MachinePeer) };
    if peer.is_closed.get() {
        let _ = env.throw_new("java/lang/IllegalStateException", "Machine is closed");
        return -1;
    }

    let res = peer.get_next_deadline();
    match res {
        Ok(Some(d)) => d as jlong,
        _ => -1,
    }
}

#[no_mangle]
pub extern "system" fn Java_io_lavi_Machine_nativeGetSnapshot(
    mut env: JNIEnv,
    _this_obj: JObject,
    handle: jlong,
) -> jstring {
    let peer = unsafe { &*(handle as *const MachinePeer) };
    if peer.is_closed.get() {
        let _ = env.throw_new("java/lang/IllegalStateException", "Machine is closed");
        return std::ptr::null_mut();
    }

    let snap = peer.with_machine(|m| m.get_persisted_snapshot_json());
    match snap {
        Ok(s) => match env.new_string(s) {
            Ok(js) => js.into_raw(),
            Err(_) => std::ptr::null_mut(),
        },
        Err(e) => {
            let _ = env.throw_new("java/lang/IllegalStateException", e);
            std::ptr::null_mut()
        }
    }
}

#[no_mangle]
pub extern "system" fn Java_io_lavi_Machine_nativeRestoreSnapshot(
    mut env: JNIEnv,
    _this_obj: JObject,
    handle: jlong,
    snapshot_json: JString,
) {
    let peer = unsafe { &*(handle as *const MachinePeer) };
    if peer.is_closed.get() {
        let _ = env.throw_new("java/lang/IllegalStateException", "Machine is closed");
        return;
    }

    let snap_str: String = match env.get_string(&snapshot_json) {
        Ok(s) => s.into(),
        Err(_) => return,
    };

    let mut guard = peer.machine.borrow_mut();
    let Some(ref mut m) = *guard else {
        let _ = env.throw_new("java/lang/IllegalStateException", "Machine is closed");
        return;
    };

    if let Err(e) = m.restore_from_snapshot_json(&snap_str) {
        let _ = env.throw_new(
            "java/lang/IllegalArgumentException",
            format!("Invalid snapshot: {e}"),
        );
    }
}

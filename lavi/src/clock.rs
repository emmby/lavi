use alloc::string::String;

use crate::runtime::RuntimeMachine;
use crate::types::{ActionFunction, GuardFunction, IndexMap, StateValue};

pub trait IntoClockFn {
    fn into_clock_fn(self) -> alloc::rc::Rc<dyn Fn() -> u64>;
}

impl IntoClockFn for alloc::rc::Rc<dyn Fn() -> u64> {
    fn into_clock_fn(self) -> alloc::rc::Rc<dyn Fn() -> u64> {
        self
    }
}

impl<F: Fn() -> u64 + 'static> IntoClockFn for alloc::rc::Rc<F> {
    fn into_clock_fn(self) -> alloc::rc::Rc<dyn Fn() -> u64> {
        self
    }
}

impl<F: Fn() -> u64 + 'static> IntoClockFn for F {
    fn into_clock_fn(self) -> alloc::rc::Rc<dyn Fn() -> u64> {
        alloc::rc::Rc::new(self)
    }
}

#[derive(Clone, Default)]
pub struct SimulatedClock {
    now_ms: alloc::rc::Rc<core::cell::Cell<u64>>,
}

impl SimulatedClock {
    pub fn new() -> Self {
        Self {
            now_ms: alloc::rc::Rc::new(core::cell::Cell::new(0)),
        }
    }

    pub fn with_initial(initial_ms: u64) -> Self {
        Self {
            now_ms: alloc::rc::Rc::new(core::cell::Cell::new(initial_ms)),
        }
    }

    pub fn now(&self) -> u64 {
        self.now_ms.get()
    }

    pub fn set_now_raw(&self, now_ms: u64) {
        self.now_ms.set(now_ms);
    }

    pub fn as_clock_fn(&self) -> alloc::rc::Rc<dyn Fn() -> u64> {
        let cell = self.now_ms.clone();
        alloc::rc::Rc::new(move || cell.get())
    }

    pub fn set(
        &self,
        target_ms: u64,
        machine: &mut RuntimeMachine,
        guards: &IndexMap<String, GuardFunction>,
        actions: &IndexMap<String, ActionFunction>,
    ) -> Option<StateValue> {
        let mut last_ret = None;
        let mut last_deadline: Option<u64> = None;
        let mut same_deadline_steps = 0usize;
        let mut total_steps = 0usize;

        while let Some(next_deadline) = machine.next_timer_deadline() {
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

            self.now_ms.set(next_deadline);
            if let Some(st) = machine.tick(guards, actions) {
                last_ret = Some(st);
            }
        }

        self.now_ms.set(target_ms);

        last_ret
    }

    pub fn increment(
        &self,
        dt_ms: u64,
        machine: &mut RuntimeMachine,
        guards: &IndexMap<String, GuardFunction>,
        actions: &IndexMap<String, ActionFunction>,
    ) -> Option<StateValue> {
        let target = self.now_ms.get().saturating_add(dt_ms);
        self.set(target, machine, guards, actions)
    }
}

impl IntoClockFn for &SimulatedClock {
    fn into_clock_fn(self) -> alloc::rc::Rc<dyn Fn() -> u64> {
        self.as_clock_fn()
    }
}

impl IntoClockFn for SimulatedClock {
    fn into_clock_fn(self) -> alloc::rc::Rc<dyn Fn() -> u64> {
        self.as_clock_fn()
    }
}

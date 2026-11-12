#![no_std]

extern crate alloc;

pub mod clock;
pub mod json;
pub mod parser;
pub mod runtime;
pub mod snapshot;
pub mod types;

pub use clock::{IntoClockFn, SimulatedClock};
pub use json::JsonError;
pub use parser::{
    assign, create_machine, from_json_str, setup, value_to_state_value, SetupObject,
};
pub use runtime::RuntimeMachine;
pub use types::*;

#[macro_export]
macro_rules! indexmap {
    () => {
        $crate::IndexMap::new()
    };
    ($($key:expr => $value:expr),* $(,)?) => {{
        let mut map = $crate::IndexMap::new();
        $( map.insert($key, $value); )*
        map
    }};
}

#[cfg(test)]
use alloc::boxed::Box;
#[cfg(test)]
use alloc::format;
#[cfg(test)]
use alloc::rc::Rc;
#[cfg(test)]
use alloc::string::String;
#[cfg(test)]
use alloc::vec::Vec;

#[cfg(test)]
mod repro_test;

#[cfg(test)]
mod test_m2_parallel_history_on_done {
    extern crate std;
    use super::*;

    #[test]
    fn test_parallel_history_on_done_fires() {
        let json = r#"{
            "initial": "parallelSteps",
            "states": {
                "parallelSteps": {
                    "type": "parallel",
                    "states": {
                        "hist": {
                            "type": "history"
                        },
                        "one": {
                            "initial": "wait_one",
                            "states": {
                                "wait_one": {
                                    "on": { "finish_one": "done" }
                                },
                                "done": { "type": "final" }
                            }
                        },
                        "two": {
                            "initial": "wait_two",
                            "states": {
                                "wait_two": {
                                    "on": { "finish_two": "done" }
                                },
                                "done": { "type": "final" }
                            }
                        }
                    },
                    "onDone": "finished"
                },
                "finished": {}
            }
        }"#;
        let mut machine = RuntimeMachine::from_json(json).expect("valid machine");
        machine.start(&IndexMap::new(), &IndexMap::new());
        assert!(machine.current_state.matches("parallelSteps.one.wait_one"));
        assert!(machine.current_state.matches("parallelSteps.two.wait_two"));
        machine.transition("finish_one", &Value::Null, &IndexMap::new(), &IndexMap::new());
        assert!(machine.current_state.matches("parallelSteps.one.done"));
        machine.transition("finish_two", &Value::Null, &IndexMap::new(), &IndexMap::new());
        assert!(machine.current_state.matches("finished"));
    }

    #[test]
    fn test_final_output_secret() {
        let json = r#"{
            "initial": "secret",
            "context": { "revealedSecret": null },
            "states": {
                "secret": {
                    "initial": "wait",
                    "states": {
                        "wait": {
                            "on": { "REQUEST_SECRET": "reveal" }
                        },
                        "reveal": {
                            "type": "final"
                        }
                    },
                    "onDone": {
                        "target": "success"
                    }
                },
                "success": {
                    "type": "final"
                }
            }
        }"#;
        let mut machine = RuntimeMachine::from_json(json).expect("valid machine");
        machine.start(&IndexMap::new(), &IndexMap::new());
        assert!(machine.current_state.matches("secret.wait"));
        machine.transition("REQUEST_SECRET", &Value::Null, &IndexMap::new(), &IndexMap::new());
        assert!(machine.current_state.matches("success"));
        assert!(machine.is_done());
    }

    #[test]
    fn test_td21_pure_transitions_output_and_invoke_callbacks() {
        use alloc::string::ToString;
        use core::cell::RefCell;

        let json = r#"{
            "id": "td21Rust",
            "initial": "active",
            "context": { "total": 11 },
            "states": {
                "active": {
                    "initial": "working",
                    "invoke": { "id": "rustWorker", "src": "rustTask", "input": { "rate": 2 } },
                    "states": {
                        "working": {
                            "entry": ["logWorking"],
                            "on": { "DONE_LEAF": "leafFinal" }
                        },
                        "leafFinal": {
                            "type": "final",
                            "output": { "subStatus": "rustLeafDone" }
                        }
                    },
                    "on": { "FINISH_ALL": "completed" }
                },
                "completed": { "type": "final" }
            }
        }"#;

        let mut machine = RuntimeMachine::from_json(json).expect("valid machine");
        let started = Rc::new(RefCell::new(Vec::<String>::new()));
        let stopped = Rc::new(RefCell::new(Vec::<String>::new()));

        machine.register_output_resolver("", Rc::new(|ctx, _evt| {
            let total = ctx.as_object()
                .and_then(|o| o.get("total"))
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            let mut out = IndexMap::new();
            out.insert("doubled".to_string(), Value::Number(total * 2.0));
            Value::Object(out)
        }));

        let started_clone = Rc::clone(&started);
        let stopped_clone = Rc::clone(&stopped);
        machine.register_invoke_callbacks(
            Some(Rc::new(move |id, src, inp, _ctx, _evt| {
                let src_str = src.as_string().map(|s| s.as_str()).unwrap_or("");
                let rate = inp.as_ref().and_then(|v| v.as_object())
                    .and_then(|o| o.get("rate"))
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0) as i64;
                started_clone.borrow_mut().push(format!("{}:{}:{}", id, src_str, rate));
            })),
            Some(Rc::new(move |id| {
                stopped_clone.borrow_mut().push(id.to_string());
            })),
        );

        let init_pure = machine.initial_transition_pure_json(None).expect("pure initial");
        assert!(init_pure.contains(r#""value":{"active":"working"}"#));
        assert!(started.borrow().is_empty());
        assert!(!machine.is_started);

        machine.start(&IndexMap::new(), &IndexMap::new());
        assert_eq!(*started.borrow(), alloc::vec!["rustWorker:rustTask:2".to_string()]);

        let live_snap = machine.get_persisted_snapshot_json();
        let next_pure = machine
            .transition_pure_json(&live_snap, r#"{"type":"FINISH_ALL"}"#)
            .expect("pure transition");
        assert!(next_pure.contains(r#""value":"completed""#));
        assert!(machine.matches("active.working"));
        assert!(stopped.borrow().is_empty());

        assert!(machine.get_final_leaf_output().is_none());
        machine.transition("DONE_LEAF", &Value::event("DONE_LEAF"), &IndexMap::new(), &IndexMap::new());
        assert!(machine.matches("active.leafFinal"));
        let leaf_out = machine.get_final_leaf_output().expect("leaf output");
        assert_eq!(leaf_out.to_json_string(), r#"{"subStatus":"rustLeafDone"}"#);

        machine.transition("FINISH_ALL", &Value::event("FINISH_ALL"), &IndexMap::new(), &IndexMap::new());
        assert!(machine.is_done());
        assert_eq!(*stopped.borrow(), alloc::vec!["rustWorker".to_string()]);
        let root_out = machine.get_output().expect("root output");
        assert_eq!(root_out.to_json_string(), r#"{"doubled":22}"#);
    }
}


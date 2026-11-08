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
}

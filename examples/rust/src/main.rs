use core::cell::Cell;
use lavi::{ActionFunction, GuardFunction, IndexMap, IndexMapExt, RuntimeMachine, SimulatedClock, Value};
use std::rc::Rc;

const MACHINE_JSON: &str = r#"{
  "id": "stoplight",
  "initial": "red",
  "context": { "cycles": 1 },
  "states": {
    "red": {
      "entry": ["onRed"],
      "after": { "1000": "green" }
    },
    "green": {
      "entry": ["onGreen"],
      "on": {
        "PEDESTRIAN_WAIT": {
          "target": "yellow",
          "guard": "canCross"
        }
      },
      "after": { "1000": "yellow" }
    },
    "yellow": {
      "entry": ["onYellow"],
      "after": { "1000": "red" }
    }
  }
}"#;

fn main() {
    let clock = SimulatedClock::with_initial(0);
    let mut machine = RuntimeMachine::from_json(MACHINE_JSON).expect("valid statechart JSON");
    machine.register_clock(&clock);

    let mut guards: IndexMap<String, GuardFunction> = IndexMap::new();
    guards.insert(
        "canCross".to_string(),
        Rc::new(|ctx, _evt, _params| {
            ctx.as_object()
                .and_then(|o| o.get("cycles"))
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0)
                >= 1.0
        }),
    );

    let transitions = Rc::new(Cell::new(0usize));
    let mut actions: IndexMap<String, ActionFunction> = IndexMap::new();
    for name in ["onRed", "onGreen", "onYellow"] {
        let counter = Rc::clone(&transitions);
        actions.insert(
            name.to_string(),
            Rc::new(move |_ctx, _evt, _params| {
                counter.set(counter.get() + 1);
            }),
        );
    }

    // 1. Start in "red"
    machine.start(&guards, &actions);
    assert!(machine.matches("red"));
    assert_eq!(transitions.get(), 1);

    // 2. Advance 1000ms -> "green" (via `after: 1000`)
    clock.increment(1000, &mut machine, &guards, &actions);
    assert!(machine.matches("green"));
    assert_eq!(transitions.get(), 2);

    // 3. Send guarded "PEDESTRIAN_WAIT" -> "yellow"
    machine.transition(
        "PEDESTRIAN_WAIT",
        &Value::event("PEDESTRIAN_WAIT"),
        &guards,
        &actions,
    );
    assert!(machine.matches("yellow"));
    assert_eq!(transitions.get(), 3);

    // 4. Advance 1000ms -> "red" (via `after: 1000`)
    clock.increment(1000, &mut machine, &guards, &actions);
    assert!(machine.matches("red"));
    assert_eq!(transitions.get(), 4);

    println!("Rust example passed (transitions: {})", transitions.get());
}

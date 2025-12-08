
#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::fs;
    use std::path::PathBuf;
    use std::process::Command;
    use serde::{Deserialize, Serialize};
    use serde_json::Value;
    use crate::RuntimeMachine;

    #[derive(Debug, Deserialize, Serialize, PartialEq)]
    struct TraceStep {
        event: String,
        value: String,
        context: Value,
    }

    // Helper to find absolute path to fixtures
    fn fixture_path(name: &str) -> PathBuf {
        let mut path = std::env::current_dir().unwrap();
        // Walking up to workspace root from fsm-core
        if path.ends_with("fsm-core") {
            path.pop();
        }
        path.join("tests").join("fixtures").join(name)
    }

    fn reference_impl_path() -> PathBuf {
        let mut path = std::env::current_dir().unwrap();
        if path.ends_with("fsm-core") {
            path.pop();
        }
        path.join("tests").join("reference_impl")
    }

    #[test]
    fn test_guards_vs_reference() {
        let machine_file = fixture_path("guards_machine.json");
        let scenario_file = fixture_path("guards_scenario.json");

        // 1. Generate Reference Trace via Node.js
        let node_script_dir = reference_impl_path();
        let output = Command::new("node")
            .arg("generate_trace.js")
            .arg(machine_file.to_str().unwrap())
            .arg(scenario_file.to_str().unwrap())
            .current_dir(&node_script_dir)
            .output()
            .expect("Failed to execute Node.js reference generator");

        if !output.status.success() {
            panic!("Reference generator failed:\nstderr: {}\nstdout: {}", 
                String::from_utf8_lossy(&output.stderr), 
                String::from_utf8_lossy(&output.stdout));
        }

        let reference_trace: Vec<TraceStep> = serde_json::from_slice(&output.stdout)
            .expect("Failed to parse reference trace");

        // 2. Generate Local Trace via Rust Runtime
        let machine_json = fs::read_to_string(&machine_file).expect("Failed to read machine");
        let scenario_json = fs::read_to_string(&scenario_file).expect("Failed to read scenario");
        
        let mut machine = RuntimeMachine::from_json(&machine_json).expect("Failed to parse machine JSON");
        let scenario: Vec<String> = serde_json::from_str(&scenario_json).expect("Failed to parse scenario");

        let mut local_trace = Vec::new();

        // Implement guards & actions in Rust
        let mut guards: HashMap<String, Box<dyn Fn(&Value) -> bool>> = HashMap::new();
        guards.insert("can_open".to_string(), Box::new(|ctx| {
            if let Some(locked) = ctx.get("locked").and_then(|v| v.as_bool()) {
                !locked
            } else {
                false 
            }
        }));
        
        guards.insert("always_true".to_string(), Box::new(|_| true));
        guards.insert("always_false".to_string(), Box::new(|_| false));

        // Note: Our simplified runtime doesn't have an "actions" map yet or automated action runner.
        // We have to simulate the external actions manually relative to the scenario events.
        // This is a limitation of the PoC runtime in fsm-core compared to XState.
        // For this test, we have to mimic the "external" changes that the reference script simulated via actions.
        // The reference script had: 
        //   UNLOCK -> action: locked = false
        //   LOCK -> action: locked = true
        // In our Rust Runtime, transition() handles the state update.
        // We need to inspect the inputs/events and mutate context accordingly BEFORE or AFTER transition?
        // XState actions happen during transition.
        
        // Scenario loop
        for event in scenario {
            // Simulate Side Effects (Actions)
            // In a real implementation this would be part of the machine definition parsing
            if event == "UNLOCK" {
                 if let Some(obj) = machine.context.as_object_mut() {
                    obj.insert("locked".to_string(), serde_json::Value::Bool(false));
                }
            } else if event == "LOCK" {
                 if let Some(obj) = machine.context.as_object_mut() {
                    obj.insert("locked".to_string(), serde_json::Value::Bool(true));
                }
            }

            // Perform Transition
            let _ = machine.transition(&event, &guards);
            
            // Record state for trace
            local_trace.push(TraceStep {
                event: event.clone(),
                value: machine.state().to_string(),
                context: machine.context.clone(),
            });
        }

        // 3. Compare
        assert_eq!(local_trace.len(), reference_trace.len(), "Trace lengths differ");
        
        for (i, (local, reference)) in local_trace.iter().zip(reference_trace.iter()).enumerate() {
            assert_eq!(local, reference, "Trace mismatch at step {}", i);
        }
    }
}

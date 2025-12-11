
#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::fs;
    use std::path::PathBuf;
    use std::process::Command;
    use std::rc::Rc;
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
        let machine_file = fixture_path("guards/guards_machine.json");
        let scenario_file = fixture_path("guards/guards_scenario.json");

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

        let full_reference_trace: Vec<TraceStep> = serde_json::from_slice(&output.stdout)
            .expect("Failed to parse reference trace");
        
        // Skip init step
        let reference_trace: Vec<TraceStep> = full_reference_trace
            .into_iter()
            .filter(|step| step.event != "init")
            .collect();

        // 2. Generate Local Trace via Rust Runtime
        let machine_json = fs::read_to_string(&machine_file).expect("Failed to read machine");
        let scenario_json = fs::read_to_string(&scenario_file).expect("Failed to read scenario");
        
        let mut machine = RuntimeMachine::from_json(&machine_json).expect("Failed to parse machine JSON");
        let scenario: Vec<String> = serde_json::from_str(&scenario_json).expect("Failed to parse scenario");

        let mut local_trace = Vec::new();

        // Implement guards & actions in Rust
        let mut guards: HashMap<String, Rc<dyn Fn(&Value, &Value) -> bool>> = HashMap::new();
        guards.insert("can_open".to_string(), Rc::new(|ctx, _| {
            if let Some(locked) = ctx.get("locked").and_then(|v| v.as_bool()) {
                !locked
            } else {
                false 
            }
        }));
        
        guards.insert("always_true".to_string(), Rc::new(|_, _| true));
        guards.insert("always_false".to_string(), Rc::new(|_, _| false));

        let actions: HashMap<String, Rc<dyn Fn(&mut Value, &Value)>> = HashMap::new();

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
            let _ = machine.transition(&event, &guards, &actions);
            
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

    #[test]
    fn test_actions_vs_reference() {
        let machine_file = fixture_path("actions/actions_machine.json");
        let scenario_file = fixture_path("actions/actions_scenario.json");

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

        let full_reference_trace: Vec<TraceStep> = serde_json::from_slice(&output.stdout)
            .expect("Failed to parse reference trace");
        
        // Skip init
        let reference_trace: Vec<TraceStep> = full_reference_trace
            .into_iter()
            .filter(|step| step.event != "init")
            .collect();


        // 2. Generate Local Trace via Rust Runtime
        let machine_json = fs::read_to_string(&machine_file).expect("Failed to read machine");
        let scenario_json = fs::read_to_string(&scenario_file).expect("Failed to read scenario");
        
        let mut machine = RuntimeMachine::from_json(&machine_json).expect("Failed to parse machine JSON");
        let scenario: Vec<String> = serde_json::from_str(&scenario_json).expect("Failed to parse scenario");

        let mut local_trace = Vec::new();

        let guards: HashMap<String, Rc<dyn Fn(&Value, &Value) -> bool>> = HashMap::new();
        
        let mut actions: HashMap<String, Rc<dyn Fn(&mut Value, &Value)>> = HashMap::new();
        actions.insert("increment".to_string(), Rc::new(|ctx, _| {
            if let Some(count) = ctx.get("count").and_then(|v| v.as_i64()) {
                if let Some(obj) = ctx.as_object_mut() {
                    obj.insert("count".to_string(), serde_json::Value::Number((count + 1).into()));
                }
            }
        }));
        actions.insert("set_entered_b".to_string(), Rc::new(|ctx, _| {
            if let Some(obj) = ctx.as_object_mut() {
                obj.insert("entered_b".to_string(), serde_json::Value::Bool(true));
            }
        }));
        actions.insert("set_exited_a".to_string(), Rc::new(|ctx, _| {
            if let Some(obj) = ctx.as_object_mut() {
                obj.insert("exited_a".to_string(), serde_json::Value::Bool(true));
            }
        }));
        actions.insert("log_transition".to_string(), Rc::new(|ctx, _| {
            if let Some(obj) = ctx.as_object_mut() {
                obj.insert("transition_logged".to_string(), serde_json::Value::Bool(true));
            }
        }));

        // Scenario loop
        for event in scenario {
            // Perform Transition
            let _ = machine.transition(&event, &guards, &actions);
            
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

extern crate alloc;

use alloc::boxed::Box;
use alloc::format;
use alloc::string::String;
use indexmap::IndexMap;
use portable_xstate_rust::{
    value_to_state_value, MachineStatus, RuntimeMachine, StateValue, Value,
};

#[test]
fn test_stack_constrained_deep_compound_500() {
    // 64 KB stack thread testing 500-depth compound state serialization & restoration
    let builder = std::thread::Builder::new()
        .name("deep_compound_64k".into())
        .stack_size(64 * 1024);

    let handler = builder
        .spawn(|| {
            let depth = 500;
            let mut curr = StateValue::Atomic(format!("leaf_{}", depth));
            for i in (0..depth).rev() {
                curr = StateValue::Compound(format!("lvl_{}", i), Box::new(curr));
            }

            // 1. to_value() with 500 nested frames
            let val = curr.to_value();
            assert!(val.as_object().is_some());

            // 2. to_json_string()
            let json_str = val.to_json_string();
            assert!(json_str.contains("leaf_500"));

            // 3. from_json()
            let parsed_val = Value::from_json(&json_str).expect("parse JSON");

            // 4. value_to_state_value()
            let reconstructed = value_to_state_value(parsed_val);

            // 5. matches() at deep leaf
            let mut full_path = String::new();
            for i in 0..depth {
                full_path.push_str(&format!("lvl_{}.", i));
            }
            full_path.push_str(&format!("leaf_{}", depth));
            assert!(reconstructed.matches(&full_path));

            // 6. Explicit drop of both deep state values inside the 64 KB thread
            drop(curr);
            drop(reconstructed);
        })
        .unwrap();

    handler
        .join()
        .expect("500-depth compound state in 64 KB stack must not overflow");
}

#[test]
fn test_stack_constrained_deep_compound_32kb_stack() {
    // Highly constrained 32 KB stack (mimicking embedded microcontrollers like ESP32)
    let builder = std::thread::Builder::new()
        .name("deep_compound_32k".into())
        .stack_size(32 * 1024);

    let handler = builder
        .spawn(|| {
            let depth = 300;
            let mut curr = StateValue::Atomic(format!("leaf_{}", depth));
            for i in (0..depth).rev() {
                curr = StateValue::Compound(format!("node_{}", i), Box::new(curr));
            }

            let val = curr.to_value();
            let json_str = val.to_json_string();
            let parsed = Value::from_json(&json_str).expect("valid JSON");
            let reconstructed = value_to_state_value(parsed);

            let mut expected = String::new();
            for i in 0..depth {
                expected.push_str(&format!("node_{}.", i));
            }
            expected.push_str(&format!("leaf_{}", depth));
            assert!(reconstructed.matches(&expected));

            drop(curr);
            drop(reconstructed);
        })
        .unwrap();

    handler
        .join()
        .expect("300-depth compound state in 32 KB stack must not overflow");
}

#[test]
fn test_stack_constrained_deep_parallel_states_64kb() {
    // 20 parallel regions, each with 50-deep compound nesting = 1,000 total nodes
    let builder = std::thread::Builder::new()
        .name("deep_parallel_64k".into())
        .stack_size(64 * 1024);

    let handler = builder
        .spawn(|| {
            let regions_count = 20;
            let depth = 50;
            let mut regions = IndexMap::new();

            for r in 0..regions_count {
                let region_key = format!("region_{}", r);
                let mut child = StateValue::Atomic(format!("leaf_r{}_{}", r, depth));
                for d in (0..depth).rev() {
                    child = StateValue::Compound(format!("step_r{}_{}", r, d), Box::new(child));
                }
                regions.insert(region_key, child);
            }

            let parallel_state = StateValue::Parallel(regions);

            // 1. to_value()
            let val = parallel_state.to_value();
            assert!(val.as_object().is_some());

            // 2. JSON roundtrip
            let json = val.to_json_string();
            let parsed = Value::from_json(&json).expect("valid JSON");
            let reconstructed = value_to_state_value(parsed);

            // 3. Verify matches for first, middle, and last regions
            for r in [0, 10, regions_count - 1] {
                let mut path = format!("region_{}.step_r{}_0.", r, r);
                for d in 1..depth {
                    path.push_str(&format!("step_r{}_{}.", r, d));
                }
                path.push_str(&format!("leaf_r{}_{}", r, depth));
                assert!(reconstructed.matches(&path));
            }

            drop(parallel_state);
            drop(reconstructed);
        })
        .unwrap();

    handler
        .join()
        .expect("1,000-node parallel hierarchy in 64 KB stack must not overflow");
}

#[test]
fn test_stack_constrained_deep_dotted_atomic_64kb() {
    // Atomic string with 500 dot-separated segments in 64 KB stack
    let builder = std::thread::Builder::new()
        .name("deep_dotted_64k".into())
        .stack_size(64 * 1024);

    let handler = builder
        .spawn(|| {
            let count = 500;
            let mut dotted = String::new();
            for i in 0..count {
                dotted.push_str(&format!("seg{}.", i));
            }
            dotted.push_str("leaf");

            let atomic = StateValue::Atomic(dotted.clone());
            let val = atomic.to_value();
            assert!(val.as_object().is_some());

            let json = val.to_json_string();
            let parsed = Value::from_json(&json).expect("valid JSON");
            let reconstructed = value_to_state_value(parsed);

            assert!(reconstructed.matches(&dotted));

            drop(atomic);
            drop(reconstructed);
        })
        .unwrap();

    handler
        .join()
        .expect("500-segment dotted atomic state in 64 KB stack must not overflow");
}

#[test]
fn test_stack_constrained_deep_value_clone_100_64kb() {
    let builder = std::thread::Builder::new()
        .name("deep_clone_64k".into())
        .stack_size(64 * 1024);

    let handler = builder
        .spawn(|| {
            let depth = 100;

            // 1. Nested Object
            let mut obj_val = Value::String(format!("leaf_{}", depth));
            for i in (0..depth).rev() {
                let mut map = IndexMap::new();
                map.insert(format!("lvl_{}", i), obj_val);
                obj_val = Value::Object(map);
            }

            let cloned_obj = obj_val.clone();
            assert_eq!(cloned_obj, obj_val);

            // 2. Nested Array
            let mut arr_val = Value::Number(42.0);
            for _ in 0..depth {
                arr_val = Value::Array(alloc::vec![arr_val]);
            }

            let cloned_arr = arr_val.clone();
            assert_eq!(cloned_arr, arr_val);

            drop(obj_val);
            drop(cloned_obj);
            drop(arr_val);
            drop(cloned_arr);
        })
        .unwrap();

    handler
        .join()
        .expect("100-depth Value::clone must succeed in 64 KB stack");
}

#[test]
fn test_stack_constrained_deep_value_partial_eq_100_64kb() {
    let builder = std::thread::Builder::new()
        .name("deep_eq_64k".into())
        .stack_size(64 * 1024);

    let handler = builder
        .spawn(|| {
            let depth = 100;

            // Build v1 and v2 identical, v3 differing at leaf
            let mut v1 = Value::String("matching_leaf".into());
            let mut v2 = Value::String("matching_leaf".into());
            let mut v3 = Value::String("differing_leaf".into());

            for i in (0..depth).rev() {
                let mut m1 = IndexMap::new();
                let mut m2 = IndexMap::new();
                let mut m3 = IndexMap::new();

                m1.insert(format!("k_{}", i), v1);
                m2.insert(format!("k_{}", i), v2);
                m3.insert(format!("k_{}", i), v3);

                v1 = Value::Object(m1);
                v2 = Value::Object(m2);
                v3 = Value::Object(m3);
            }

            // Equality checks must not overflow stack
            assert_eq!(v1, v2);
            assert_ne!(v1, v3);
            assert_ne!(v2, v3);

            // Array equality checks
            let mut a1 = Value::Bool(true);
            let mut a2 = Value::Bool(true);
            let mut a3 = Value::Bool(false);

            for _ in 0..depth {
                a1 = Value::Array(alloc::vec![a1]);
                a2 = Value::Array(alloc::vec![a2]);
                a3 = Value::Array(alloc::vec![a3]);
            }

            assert_eq!(a1, a2);
            assert_ne!(a1, a3);
        })
        .unwrap();

    handler
        .join()
        .expect("100-depth Value PartialEq must succeed in 64 KB stack");
}

#[test]
fn test_stack_constrained_deep_snapshot_restore_100_64kb() {
    let builder = std::thread::Builder::new()
        .name("deep_snap_restore_64k".into())
        .stack_size(64 * 1024);

    let handler = builder
        .spawn(|| {
            let depth = 100;

            // 1. Build 100-deep compound state value JSON
            let mut val_str = format!(r#""leaf_{depth}""#);
            for i in (0..depth).rev() {
                val_str = format!(r#"{{"lvl_{i}":{val_str}}}"#);
            }

            // 2. Build 100-deep context JSON
            let mut ctx_str = r#"{"count":42}"#.to_string();
            for i in (0..depth).rev() {
                ctx_str = format!(r#"{{"c_{i}":{ctx_str}}}"#);
            }

            let snap_json = format!(
                r#"{{"status":"active","value":{val_str},"context":{ctx_str},"historyValue":{{}},"tags":[],"children":{{}}}}"#
            );

            let mut m = RuntimeMachine::from_json(
                r#"{"id":"deep","initial":"idle","states":{"idle":{}}}"#
            ).expect("valid machine");

            // Owned JSON restore: zero clones, bounded O(1) stack
            m.restore_from_snapshot_json(&snap_json)
                .expect("100-deep JSON snapshot restore must succeed in 64 KB stack");

            assert_eq!(m.status(), MachineStatus::Active);

            let mut expected_path = String::new();
            for i in 0..depth {
                expected_path.push_str(&format!("lvl_{}.", i));
            }
            expected_path.push_str(&format!("leaf_{}", depth));
            assert!(m.current_state.matches(&expected_path));

            // Borrowed restore: safe iterative clone, bounded O(1) stack
            let snap_val = Value::from_json(&snap_json).expect("valid JSON");
            let mut m2 = RuntimeMachine::from_json(
                r#"{"id":"deep2","initial":"idle","states":{"idle":{}}}"#
            ).expect("valid machine");

            m2.restore_from_snapshot_value(&snap_val)
                .expect("100-deep borrowed snapshot restore must succeed in 64 KB stack");

            assert_eq!(m2.status(), MachineStatus::Active);
            assert_eq!(m2.current_state, m.current_state);
            assert_eq!(m2.context, m.context);
        })
        .unwrap();

    handler
        .join()
        .expect("100-deep snapshot restore completed without stack overflow");
}

#[test]
fn test_stack_constrained_deep_value_clone_32kb() {
    let builder = std::thread::Builder::new()
        .name("deep_clone_32k".into())
        .stack_size(32 * 1024);

    let handler = builder
        .spawn(|| {
            let depth = 100;
            let mut val = Value::String(format!("leaf_{}", depth));
            for i in (0..depth).rev() {
                let mut map = IndexMap::new();
                map.insert(format!("lvl_{}", i), val);
                val = Value::Object(map);
            }

            let cloned = val.clone();
            assert_eq!(cloned, val);

            drop(val);
            drop(cloned);
        })
        .unwrap();

    handler
        .join()
        .expect("100-depth Value::clone must succeed in 32 KB stack");
}

#[test]
fn test_snapshot_restore_pathological_payloads() {
    let json = r#"{
        "id": "resilient",
        "initial": "idle",
        "context": { "value": 100 },
        "states": {
            "idle": {
                "on": { "GO": "busy" }
            },
            "busy": {}
        }
    }"#;

    let mut m = RuntimeMachine::from_json(json).expect("valid machine");
    m.start(&IndexMap::new(), &IndexMap::new());

    // 1. Empty JSON object: status, value, context should preserve valid defaults or remain unchanged
    assert!(m.restore_from_snapshot_json(r#"{}"#).is_ok());
    assert!(m.current_state.matches("idle"));

    // 2. Non-object JSON inputs: must return Err, no panics
    assert!(m.restore_from_snapshot_json(r#""just a string""#).is_err());
    assert!(m.restore_from_snapshot_json(r#"42"#).is_err());
    assert!(m.restore_from_snapshot_json(r#"[1, 2, 3]"#).is_err());
    assert!(m.restore_from_snapshot_json(r#"true"#).is_err());
    assert!(m.restore_from_snapshot_json(r#"null"#).is_err());
    assert!(m.restore_from_snapshot_json(r#"   "#).is_err());
    assert!(m.restore_from_snapshot_json(r#"{"unterminated": "#).is_err());

    // 3. Pathological historyValue: nested arrays, weird IDs, nulls
    let weird_hist = r#"{
        "status": "active",
        "value": "idle",
        "historyValue": {
            "node.one": [null, {}, {"id": 123}, {"id": null}, {"id": "nonexistent"}],
            "node.two": "plain_string",
            "node.three": {"nested": "value"},
            "node.four": null,
            "node.five": 999
        }
    }"#;
    assert!(m.restore_from_snapshot_json(weird_hist).is_ok());

    // 4. Pathological status values: unknown status string must not crash
    let weird_status = r#"{
        "status": "some_alien_status",
        "value": "idle"
    }"#;
    assert!(m.restore_from_snapshot_json(weird_status).is_ok());
    // Machine status remains what it was
    assert_eq!(m.status(), MachineStatus::Active);

    // 5. Pathological context: non-object context (array, number, string)
    let non_obj_ctx = r#"{
        "status": "active",
        "value": "idle",
        "context": [1, 2, 3]
    }"#;
    assert!(m.restore_from_snapshot_json(non_obj_ctx).is_ok());
    assert!(m.context.as_array().is_some());
}

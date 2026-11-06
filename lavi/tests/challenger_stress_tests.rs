extern crate alloc;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use indexmap::IndexMap;
use lavi::{
    MachineStatus, RuntimeMachine, Value,
};

#[test]
fn test_challenger_depth_scaling_clone_64kb() {
    let builder = std::thread::Builder::new()
        .name("deep_clone_scaling_64k".into())
        .stack_size(64 * 1024);

    let handler = builder
        .spawn(|| {
            for depth in [100, 200, 500] {
                // 1. Deep Object
                let mut obj = Value::String(format!("leaf_{}", depth));
                for i in (0..depth).rev() {
                    let mut m = IndexMap::new();
                    m.insert(format!("key_{}", i), obj);
                    obj = Value::Object(m);
                }
                let cloned_obj = obj.clone();
                assert_eq!(cloned_obj, obj, "Cloned deep object must equal original at depth {}", depth);

                // 2. Deep Array
                let mut arr = Value::Number(depth as f64);
                for _ in 0..depth {
                    arr = Value::Array(alloc::vec![arr]);
                }
                let cloned_arr = arr.clone();
                assert_eq!(cloned_arr, arr, "Cloned deep array must equal original at depth {}", depth);

                // 3. Alternating Object and Array
                let mut alt = Value::Bool(true);
                for i in (0..depth).rev() {
                    if i % 2 == 0 {
                        let mut m = IndexMap::new();
                        m.insert(format!("lvl_{}", i), alt);
                        alt = Value::Object(m);
                    } else {
                        alt = Value::Array(alloc::vec![alt]);
                    }
                }
                let cloned_alt = alt.clone();
                assert_eq!(cloned_alt, alt, "Cloned alternating structure must equal original at depth {}", depth);
            }
        })
        .unwrap();

    handler.join().expect("Depth scaling Value::clone (up to 500) in 64 KB stack must succeed");
}

#[test]
fn test_challenger_depth_scaling_clone_32kb() {
    let builder = std::thread::Builder::new()
        .name("deep_clone_scaling_32k".into())
        .stack_size(32 * 1024);

    let handler = builder
        .spawn(|| {
            for depth in [100, 200, 300] {
                let mut obj = Value::String(format!("leaf_{}", depth));
                for i in (0..depth).rev() {
                    let mut m = IndexMap::new();
                    m.insert(format!("key_{}", i), obj);
                    obj = Value::Object(m);
                }
                let cloned = obj.clone();
                assert_eq!(cloned, obj, "Cloned deep object must equal original at depth {} in 32 KB", depth);

                let mut arr = Value::Number(123.0);
                for _ in 0..depth {
                    arr = Value::Array(alloc::vec![arr]);
                }
                let cloned_arr = arr.clone();
                assert_eq!(cloned_arr, arr, "Cloned deep array must equal original at depth {} in 32 KB", depth);
            }
        })
        .unwrap();

    handler.join().expect("Depth scaling Value::clone (up to 300) in 32 KB stack must succeed");
}

#[test]
fn test_challenger_key_order_preservation() {
    // Verify that Value::clone preserves the exact insertion order of IndexMap keys
    let mut map = IndexMap::new();
    map.insert("alpha".to_string(), Value::Number(1.0));
    map.insert("bravo".to_string(), Value::String("two".into()));
    map.insert("charlie".to_string(), Value::Array(alloc::vec![
        Value::Null,
        Value::Object({
            let mut sub = IndexMap::new();
            sub.insert("nested_1".to_string(), Value::Bool(true));
            sub.insert("nested_2".to_string(), Value::Array(Vec::new()));
            sub.insert("nested_3".to_string(), Value::Object(IndexMap::new()));
            sub
        }),
        Value::Number(42.5),
    ]));
    map.insert("delta".to_string(), Value::Object(IndexMap::new()));
    map.insert("echo".to_string(), Value::Array(Vec::new()));
    map.insert("foxtrot".to_string(), Value::Bool(false));
    map.insert("golf".to_string(), Value::Object({
        let mut deep = IndexMap::new();
        deep.insert("first".to_string(), Value::String("deep_first".into()));
        deep.insert("second".to_string(), Value::String("deep_second".into()));
        deep.insert("third".to_string(), Value::String("deep_third".into()));
        deep
    }));
    map.insert("hotel".to_string(), Value::Null);

    let original = Value::Object(map);
    let cloned = original.clone();

    // Verify pointer difference
    assert_ne!(
        &original as *const _,
        &cloned as *const _,
        "Clone must be a distinct allocation"
    );

    // Verify exact equality
    assert_eq!(original, cloned);

    // Inspect insertion order explicitly
    let orig_keys: Vec<&String> = original.as_object().unwrap().keys().collect();
    let cloned_keys: Vec<&String> = cloned.as_object().unwrap().keys().collect();
    assert_eq!(orig_keys, cloned_keys, "Top-level keys must match exact insertion order");

    // Inspect nested sub-object keys
    let orig_golf = original.as_object().unwrap().get("golf").unwrap().as_object().unwrap();
    let cloned_golf = cloned.as_object().unwrap().get("golf").unwrap().as_object().unwrap();
    let orig_golf_keys: Vec<&String> = orig_golf.keys().collect();
    let cloned_golf_keys: Vec<&String> = cloned_golf.keys().collect();
    assert_eq!(orig_golf_keys, cloned_golf_keys, "Nested object keys must match exact insertion order");
}

#[test]
fn test_challenger_partial_eq_adversarial_64kb() {
    let builder = std::thread::Builder::new()
        .name("deep_eq_adversarial_64k".into())
        .stack_size(64 * 1024);

    let handler = builder
        .spawn(|| {
            let depth = 300;

            // 1. Identical deep trees
            let mut v1 = Value::String("shared_leaf".into());
            let mut v2 = Value::String("shared_leaf".into());
            for i in (0..depth).rev() {
                let mut m1 = IndexMap::new();
                let mut m2 = IndexMap::new();
                m1.insert(format!("k_{}", i), v1);
                m2.insert(format!("k_{}", i), v2);
                v1 = Value::Object(m1);
                v2 = Value::Object(m2);
            }
            assert_eq!(v1, v2);

            // 2. Differing only at the innermost leaf
            let mut v3 = Value::String("differing_leaf".into());
            for i in (0..depth).rev() {
                let mut m3 = IndexMap::new();
                m3.insert(format!("k_{}", i), v3);
                v3 = Value::Object(m3);
            }
            assert_ne!(v1, v3);

            // 3. Differing in key name at midpoint
            let mut v4 = Value::String("shared_leaf".into());
            for i in (0..depth).rev() {
                let mut m4 = IndexMap::new();
                let k = if i == depth / 2 {
                    format!("DIFFERENT_k_{}", i)
                } else {
                    format!("k_{}", i)
                };
                m4.insert(k, v4);
                v4 = Value::Object(m4);
            }
            assert_ne!(v1, v4);

            // 4. Differing in map length at midpoint
            let mut v5 = Value::String("shared_leaf".into());
            for i in (0..depth).rev() {
                let mut m5 = IndexMap::new();
                m5.insert(format!("k_{}", i), v5);
                if i == depth / 2 {
                    m5.insert("extra_key".into(), Value::Bool(false));
                }
                v5 = Value::Object(m5);
            }
            assert_ne!(v1, v5);

            // 5. Differing in array length at innermost level
            let mut a1 = Value::Array(alloc::vec![Value::Number(1.0)]);
            let mut a2 = Value::Array(alloc::vec![Value::Number(1.0), Value::Number(2.0)]);
            for _ in 0..depth {
                a1 = Value::Array(alloc::vec![a1]);
                a2 = Value::Array(alloc::vec![a2]);
            }
            assert_ne!(a1, a2);
        })
        .unwrap();

    handler.join().expect("Adversarial PartialEq in 64 KB stack must succeed");
}

#[test]
fn test_challenger_deep_snapshot_restore_scaling_64kb_and_32kb() {
    // Test depth 150 snapshot restoration in 64 KB stack
    let builder_64k = std::thread::Builder::new()
        .name("deep_snap_150_64k".into())
        .stack_size(64 * 1024);

    let handler_64k = builder_64k
        .spawn(|| {
            let depth = 150;

            let mut val_str = format!(r#""leaf_{depth}""#);
            for i in (0..depth).rev() {
                val_str = format!(r#"{{"lvl_{i}":{val_str}}}"#);
            }

            let mut ctx_str = r#"{"counter":999}"#.to_string();
            for i in (0..depth).rev() {
                ctx_str = format!(r#"{{"c_{i}":{ctx_str}}}"#);
            }

            let snap_json = format!(
                r#"{{"status":"active","value":{val_str},"context":{ctx_str},"historyValue":{{}}}}"#
            );

            // Test Owned Restore
            let mut m = RuntimeMachine::from_json(
                r#"{"id":"m_deep","initial":"idle","states":{"idle":{}}}"#
            ).unwrap();
            m.restore_from_snapshot_json(&snap_json)
                .expect("150-deep owned snapshot restore must succeed in 64 KB");

            assert_eq!(m.status(), MachineStatus::Active);
            let mut expected_path = String::new();
            for i in 0..depth {
                expected_path.push_str(&format!("lvl_{}.", i));
            }
            expected_path.push_str(&format!("leaf_{}", depth));
            assert!(m.current_state.matches(&expected_path));

            // Test Borrowed Restore
            let snap_val = Value::from_json(&snap_json).unwrap();
            let mut m2 = RuntimeMachine::from_json(
                r#"{"id":"m_deep2","initial":"idle","states":{"idle":{}}}"#
            ).unwrap();
            m2.restore_from_snapshot_value(&snap_val)
                .expect("150-deep borrowed snapshot restore must succeed in 64 KB");
            assert_eq!(m2.status(), MachineStatus::Active);
            assert_eq!(m2.current_state, m.current_state);
            assert_eq!(m2.context, m.context);
        })
        .unwrap();

    handler_64k.join().expect("150-deep snapshot restore in 64 KB stack must succeed");

    // Test depth 80 snapshot restoration in 32 KB stack
    let builder_32k = std::thread::Builder::new()
        .name("deep_snap_80_32k".into())
        .stack_size(32 * 1024);

    let handler_32k = builder_32k
        .spawn(|| {
            let depth = 80;

            let mut val_str = format!(r#""leaf_{depth}""#);
            for i in (0..depth).rev() {
                val_str = format!(r#"{{"node_{i}":{val_str}}}"#);
            }

            let mut ctx_str = r#"{"val":1}"#.to_string();
            for i in (0..depth).rev() {
                ctx_str = format!(r#"{{"ctx_{i}":{ctx_str}}}"#);
            }

            let snap_json = format!(
                r#"{{"status":"active","value":{val_str},"context":{ctx_str}}}"#
            );

            // Owned Restore
            let mut m = RuntimeMachine::from_json(
                r#"{"id":"m32","initial":"idle","states":{"idle":{}}}"#
            ).unwrap();
            m.restore_from_snapshot_json(&snap_json)
                .expect("80-deep owned snapshot restore in 32 KB stack");
            assert_eq!(m.status(), MachineStatus::Active);

            // Borrowed Restore
            let snap_val = Value::from_json(&snap_json).unwrap();
            let mut m2 = RuntimeMachine::from_json(
                r#"{"id":"m32_2","initial":"idle","states":{"idle":{}}}"#
            ).unwrap();
            m2.restore_from_snapshot_value(&snap_val)
                .expect("80-deep borrowed snapshot restore in 32 KB stack");
            assert_eq!(m2.status(), MachineStatus::Active);
        })
        .unwrap();

    handler_32k.join().expect("80-deep snapshot restore in 32 KB stack must succeed");
}

#[test]
fn test_challenger_deep_history_value_restore_64kb() {
    let builder = std::thread::Builder::new()
        .name("deep_history_64k".into())
        .stack_size(64 * 1024);

    let handler = builder
        .spawn(|| {
            let depth = 100;
            // Build 100-deep nested object inside historyValue
            let mut inner_val = format!(r#""hist_leaf_{depth}""#);
            for i in (0..depth).rev() {
                inner_val = format!(r#"{{"h_lvl_{i}":{inner_val}}}"#);
            }

            let snap_json = format!(
                r#"{{"status":"active","value":"idle","historyValue":{{"parent.hist":{inner_val}}}}}"#
            );

            let mut m = RuntimeMachine::from_json(
                r#"{"id":"m_hist","initial":"idle","states":{"idle":{}}}"#
            ).unwrap();

            // Owned JSON restore with 100-deep historyValue
            m.restore_from_snapshot_json(&snap_json)
                .expect("100-deep historyValue owned restore must succeed in 64 KB");

            assert_eq!(m.history_value.len(), 1);
            let hist_sv = m.history_value.get("parent").unwrap();
            assert!(hist_sv.matches("h_lvl_0.h_lvl_1"));

            // Borrowed restore
            let val = Value::from_json(&snap_json).unwrap();
            let mut m2 = RuntimeMachine::from_json(
                r#"{"id":"m_hist2","initial":"idle","states":{"idle":{}}}"#
            ).unwrap();
            m2.restore_from_snapshot_value(&val)
                .expect("100-deep historyValue borrowed restore must succeed in 64 KB");
            assert_eq!(m2.history_value.len(), 1);
        })
        .unwrap();

    handler.join().expect("100-deep historyValue restore in 64 KB stack must succeed");
}

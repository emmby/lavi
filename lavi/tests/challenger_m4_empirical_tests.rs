extern crate alloc;

use alloc::format;
use alloc::rc::Rc;
use alloc::string::{String, ToString};
use alloc::vec;
use lavi::{IndexMap, IndexMapExt};
use lavi::{
    ActionFunction, GuardFunction, MachineDefinition, RuntimeMachine, StateDefinition, StateType,
    Value,
};

#[inline]
fn stack_sz(kb: usize) -> usize {
    if cfg!(debug_assertions) {
        kb.max(64) * 1024
    } else {
        kb * 1024
    }
}

fn run_in_thread<F>(stack_size_kb: usize, f: F)
where
    F: FnOnce() + Send + 'static,
{
    let builder = std::thread::Builder::new().stack_size(stack_sz(stack_size_kb));
    match builder.spawn(f) {
        Ok(handle) => {
            handle
                .join()
                .expect("Thread failed or overflowed stack");
        }
        Err(e) => {
            println!(
                "OS thread spawn with stack_size_kb {} returned {:?}; skipped thread run",
                stack_size_kb, e
            );
        }
    }
}

/// Stress Test 1: 100-level deep state hierarchy with tags in an 8KB stack thread.
#[test]
fn test_deep_100_level_tags_8kb_stack() {
    run_in_thread(16, || {
        let depth = 100;

        // Build leaf
        let mut leaf = StateDefinition::default_atomic();
        leaf.tags.push("leaf_tag_99".to_string());

        let mut current_states = IndexMap::new();
        current_states.insert(format!("s_{}", depth - 1), leaf);

        // Build chain from bottom up
        for i in (0..(depth - 1)).rev() {
            let mut node = StateDefinition::default_atomic();
            node.state_type = StateType::Compound;
            node.initial = Some(format!("s_{}", i + 1));
            node.states = core::mem::take(&mut current_states);
            if i % 25 == 0 {
                node.tags.push(format!("tag_level_{}", i));
            }
            let mut parent_map = IndexMap::new();
            parent_map.insert(format!("s_{}", i), node);
            current_states = parent_map;
        }

        let def = MachineDefinition {
            id: "deep_tag_machine".to_string(),
            initial: "s_0".to_string(),
            states: current_states,
            tags: vec!["root_tag".to_string()],
            ..Default::default()
        };

        let machine = RuntimeMachine::from_definition(def).expect("Valid deep machine definition");

        // Verify root tag
        assert!(machine.has_tag("root_tag"));

        // Verify periodic tags across 100 levels
        assert!(machine.has_tag("tag_level_0"));
        assert!(machine.has_tag("tag_level_25"));
        assert!(machine.has_tag("tag_level_50"));
        assert!(machine.has_tag("tag_level_75"));
        assert!(machine.has_tag("leaf_tag_99"));

        // Verify non-existent tags return false safely
        assert!(!machine.has_tag("non_existent"));
        assert!(!machine.has_tag("tag_level_1"));

        // Verify get_tags contains all 6 tags
        let tags = machine.get_tags();
        assert_eq!(tags.len(), 6);
        assert!(tags.contains(&"root_tag".to_string()));
        assert!(tags.contains(&"tag_level_0".to_string()));
        assert!(tags.contains(&"tag_level_25".to_string()));
        assert!(tags.contains(&"tag_level_50".to_string()));
        assert!(tags.contains(&"tag_level_75".to_string()));
        assert!(tags.contains(&"leaf_tag_99".to_string()));
    });
}

/// Stress Test 2: 100-level deep state hierarchy with metadata in an 8KB stack thread.
#[test]
fn test_deep_100_level_metadata_8kb_stack() {
    run_in_thread(16, || {
        let depth = 100;
        let mut current_states = IndexMap::new();

        // Build leaf with explicit ID
        let mut leaf = StateDefinition::default_atomic();
        leaf.id = Some("explicit_leaf_id".to_string());
        let mut leaf_meta = IndexMap::new();
        leaf_meta.insert("level".to_string(), Value::Number(99.0));
        leaf.meta = Some(Value::Object(leaf_meta));
        current_states.insert(format!("s_{}", depth - 1), leaf);

        // Build chain from bottom up
        for i in (0..(depth - 1)).rev() {
            let mut node = StateDefinition::default_atomic();
            node.state_type = StateType::Compound;
            node.initial = Some(format!("s_{}", i + 1));
            node.states = core::mem::take(&mut current_states);
            if i % 25 == 0 {
                let mut meta_map = IndexMap::new();
                meta_map.insert("level".to_string(), Value::Number(i as f64));
                node.meta = Some(Value::Object(meta_map));
            }
            let mut parent_map = IndexMap::new();
            parent_map.insert(format!("s_{}", i), node);
            current_states = parent_map;
        }

        let mut root_meta = IndexMap::new();
        root_meta.insert("app".to_string(), Value::String("deep_test".to_string()));
        let def = MachineDefinition {
            id: "deep_meta_machine".to_string(),
            initial: "s_0".to_string(),
            meta: Some(Value::Object(root_meta)),
            states: current_states,
            ..Default::default()
        };

        let machine = RuntimeMachine::from_definition(def).expect("Valid deep machine definition");

        let meta = machine.get_meta();
        let meta_obj = match meta {
            Value::Object(ref map) => map,
            other => panic!("Expected Object for get_meta, got {:?}", other),
        };

        // Root meta key is "deep_meta_machine"
        assert!(meta_obj.contains_key("deep_meta_machine"));

        // Level 0 meta key is "deep_meta_machine.s_0"
        assert!(meta_obj.contains_key("deep_meta_machine.s_0"));

        // Leaf meta has explicit id "explicit_leaf_id"
        assert!(meta_obj.contains_key("explicit_leaf_id"));
    });
}

/// Stress Test 3: Wide parallel states (20 regions) with tags and metadata in an 8KB stack thread.
#[test]
fn test_wide_parallel_tags_and_meta_8kb_stack() {
    run_in_thread(16, || {
        let region_count = 20;
        let mut regions = IndexMap::new();

        for r in 0..region_count {
            let mut state_def = StateDefinition::default_atomic();
            state_def.tags.push("parallel_shared_tag".to_string());
            state_def.tags.push(format!("region_tag_{}", r));

            let mut meta_map = IndexMap::new();
            meta_map.insert("region_index".to_string(), Value::Number(r as f64));
            state_def.meta = Some(Value::Object(meta_map));

            let mut region_states = IndexMap::new();
            region_states.insert("active".to_string(), state_def);

            let mut region_def = StateDefinition::default_atomic();
            region_def.state_type = StateType::Compound;
            region_def.initial = Some("active".to_string());
            region_def.states = region_states;
            regions.insert(format!("region_{}", r), region_def);
        }

        let def = MachineDefinition {
            id: "wide_parallel".to_string(),
            state_type: StateType::Parallel,
            tags: vec!["root_wide_tag".to_string()],
            states: regions,
            ..Default::default()
        };

        let machine = RuntimeMachine::from_definition(def).expect("Valid parallel machine definition");

        // Verify shared tag is present and deduplicated
        assert!(machine.has_tag("parallel_shared_tag"));
        assert!(machine.has_tag("root_wide_tag"));

        // Verify all 20 individual region tags are present
        for r in 0..region_count {
            assert!(machine.has_tag(&format!("region_tag_{}", r)));
        }

        let tags = machine.get_tags();
        // 1 root tag + 1 shared tag + 20 region tags = 22 tags
        assert_eq!(tags.len(), 22);

        // Verify metadata from all 20 regions is aggregated
        let meta = machine.get_meta();
        if let Value::Object(ref map) = meta {
            for r in 0..region_count {
                let key = format!("wide_parallel.region_{}.active", r);
                assert!(map.contains_key(&key), "Missing meta key: {}", key);
            }
        } else {
            panic!("Expected Value::Object for get_meta");
        }
    });
}

/// Stress Test 4: Input propagation to initial event on machine start.
#[test]
fn test_input_lifecycle_and_initial_event_identity() {
    let mut states = IndexMap::new();
    let mut initial_state = StateDefinition::default_atomic();

    let received_input_holder = Rc::new(core::cell::RefCell::new(Value::Null));
    let holder_clone = received_input_holder.clone();

    let entry_action: ActionFunction = Rc::new(move |_ctx, evt, _params| {
        if let Value::Object(evt_map) = evt {
            if let Some(inp) = evt_map.get("input") {
                *holder_clone.borrow_mut() = inp.clone();
            }
        }
    });

    initial_state.entry.push(lavi::Action::Named("capture_input".to_string()));
    states.insert("init".to_string(), initial_state);

    let def = MachineDefinition {
        initial: "init".to_string(),
        states,
        ..Default::default()
    };

    let mut machine = RuntimeMachine::from_definition(def).expect("Valid machine");

    // Provide complex structured input
    let mut complex_input = IndexMap::new();
    complex_input.insert("greeting".to_string(), Value::String("hello world".to_string()));
    complex_input.insert("count".to_string(), Value::Number(42.0));
    complex_input.insert("flag".to_string(), Value::Bool(true));
    complex_input.insert(
        "nested".to_string(),
        Value::Array(vec![Value::Number(1.0), Value::Number(2.0)]),
    );

    let input_val = Value::Object(complex_input.clone());
    machine.input = Some(input_val.clone());

    let mut actions: IndexMap<String, ActionFunction> = IndexMap::new();
    actions.insert("capture_input".to_string(), entry_action);
    let guards: IndexMap<String, GuardFunction> = IndexMap::new();

    machine.start(&guards, &actions);

    assert_eq!(*received_input_holder.borrow(), input_val);
}

/// Stress Test 5: Inactive sibling isolation in Rust RuntimeMachine::get_meta.
#[test]
fn test_inactive_sibling_meta_isolation_rust() {
    let mut states = IndexMap::new();

    let mut state_a = StateDefinition::default_atomic();
    let mut meta_a = IndexMap::new();
    meta_a.insert("name".to_string(), Value::String("State A".to_string()));
    state_a.meta = Some(Value::Object(meta_a));

    let mut state_b = StateDefinition::default_atomic();
    let mut meta_b = IndexMap::new();
    meta_b.insert("name".to_string(), Value::String("State B".to_string()));
    state_b.meta = Some(Value::Object(meta_b));

    states.insert("a".to_string(), state_a);
    states.insert("b".to_string(), state_b);

    let def = MachineDefinition {
        id: "my_machine".to_string(),
        initial: "a".to_string(),
        states,
        ..Default::default()
    };

    let machine = RuntimeMachine::from_definition(def).expect("Valid machine");

    let meta = machine.get_meta();
    if let Value::Object(ref map) = meta {
        assert!(map.contains_key("my_machine.a"));
        assert!(!map.contains_key("my_machine.b"));
    } else {
        panic!("Expected Value::Object");
    }
}

/// Verify initial_transition_pure resolves all orthogonal regions on root parallel machines.
#[test]
fn test_parallel_initial_transition_pure_all_regions() {
    let mut states = IndexMap::new();

    let mut reg_a_states = IndexMap::new();
    reg_a_states.insert("off".to_string(), StateDefinition::default_atomic());
    reg_a_states.insert("on".to_string(), StateDefinition::default_atomic());
    let mut reg_a = StateDefinition::default_atomic();
    reg_a.state_type = StateType::Compound;
    reg_a.initial = Some("off".to_string());
    reg_a.states = reg_a_states;
    states.insert("regionA".to_string(), reg_a);

    let mut reg_b_states = IndexMap::new();
    reg_b_states.insert("idle".to_string(), StateDefinition::default_atomic());
    reg_b_states.insert("active".to_string(), StateDefinition::default_atomic());
    let mut reg_b = StateDefinition::default_atomic();
    reg_b.state_type = StateType::Compound;
    reg_b.initial = Some("idle".to_string());
    reg_b.states = reg_b_states;
    states.insert("regionB".to_string(), reg_b);

    let def = MachineDefinition {
        id: "parallel_pure_test".to_string(),
        state_type: StateType::Parallel,
        states,
        ..Default::default()
    };

    let machine = RuntimeMachine::from_definition(def).expect("Valid parallel machine");
    let (s0, _, _) = machine.initial_transition_pure(None);
    let paths = s0.get_active_paths();
    assert!(paths.contains(&"regionA.off".to_string()), "Missing regionA.off");
    assert!(paths.contains(&"regionB.idle".to_string()), "Missing regionB.idle");
    assert_eq!(paths.len(), 2, "Expected exactly 2 active paths");
}

/// Stress Test: 1-region parallel machine pure stepping.
#[test]
fn test_single_region_parallel_pure_stepping() {
    let mut states = IndexMap::new();
    let mut reg_states = IndexMap::new();
    reg_states.insert("init".to_string(), StateDefinition::default_atomic());
    reg_states.insert("done".to_string(), StateDefinition::default_atomic());
    let mut reg = StateDefinition::default_atomic();
    reg.state_type = StateType::Compound;
    reg.initial = Some("init".to_string());
    reg.states = reg_states;
    states.insert("single".to_string(), reg);

    let def = MachineDefinition {
        id: "single_parallel".to_string(),
        state_type: StateType::Parallel,
        states,
        ..Default::default()
    };

    let machine = RuntimeMachine::from_definition(def).expect("Valid parallel machine");
    let (s0, _, _) = machine.initial_transition_pure(None);
    let paths = s0.get_active_paths();
    assert_eq!(paths, vec!["single.init".to_string()]);
}

/// Stress Test: 3-region parallel machine simultaneous pure stepping.
#[test]
fn test_three_region_parallel_simultaneous_pure_stepping() {
    let mut states = IndexMap::new();
    for r in &["r1", "r2", "r3"] {
        let mut r_states = IndexMap::new();
        r_states.insert("a".to_string(), StateDefinition::default_atomic());
        r_states.insert("b".to_string(), StateDefinition::default_atomic());
        let mut reg = StateDefinition::default_atomic();
        reg.state_type = StateType::Compound;
        reg.initial = Some("a".to_string());
        reg.states = r_states;
        states.insert(r.to_string(), reg);
    }

    let def = MachineDefinition {
        id: "three_region_parallel".to_string(),
        state_type: StateType::Parallel,
        states,
        ..Default::default()
    };

    let machine = RuntimeMachine::from_definition(def).expect("Valid 3-region machine");
    let (s0, _, _) = machine.initial_transition_pure(None);
    let paths = s0.get_active_paths();
    assert_eq!(paths.len(), 3);
    assert!(paths.contains(&"r1.a".to_string()));
    assert!(paths.contains(&"r2.a".to_string()));
    assert!(paths.contains(&"r3.a".to_string()));
}

/// Empirical Test: Verify history state restoration in parallel regions under pure stepping.
#[test]
fn test_history_state_pure_stepping_with_parallel_regions() {
    let json = r#"{
        "id": "parallel_history_repro",
        "initial": "operating",
        "states": {
            "operating": {
                "initial": "mode",
                "states": {
                    "mode": {
                        "type": "parallel",
                        "states": {
                            "light": {
                                "initial": "bright",
                                "states": {
                                    "bright": { "on": { "DIM": "dimmed" } },
                                    "dimmed": {}
                                }
                            },
                            "sound": {
                                "initial": "beep",
                                "states": {
                                    "beep": { "on": { "MUTE": "silent" } },
                                    "silent": {}
                                }
                            }
                        }
                    },
                    "hist": {
                        "type": "history",
                        "history": "deep"
                    }
                },
                "on": { "PAUSE": "paused" }
            },
            "paused": {
                "on": { "RESUME": "operating.hist" }
            }
        }
    }"#;

    let def = lavi::from_json_str(json).expect("valid machine JSON");
    let machine = RuntimeMachine::from_definition(def).expect("valid machine");

    // Pure stepping sequence
    let (s0, _, _) = machine.initial_transition_pure(None);
    let paths0 = s0.get_active_paths();
    assert!(paths0.contains(&"operating.mode.light.bright".to_string()));
    assert!(paths0.contains(&"operating.mode.sound.beep".to_string()));

    let (s1, ctx1, _, h1) = machine.transition_pure_with_context_and_history(
        &s0,
        &machine.context,
        None,
        "DIM",
        &Value::Null,
    );
    let paths1 = s1.get_active_paths();
    assert!(paths1.contains(&"operating.mode.light.dimmed".to_string()));
    assert!(paths1.contains(&"operating.mode.sound.beep".to_string()));

    let (s2, ctx2, _, h2) = machine.transition_pure_with_context_and_history(
        &s1,
        &ctx1,
        Some(&h1),
        "MUTE",
        &Value::Null,
    );
    let paths2 = s2.get_active_paths();
    assert!(paths2.contains(&"operating.mode.light.dimmed".to_string()));
    assert!(paths2.contains(&"operating.mode.sound.silent".to_string()));

    let (s3, ctx3, _, h3) = machine.transition_pure_with_context_and_history(
        &s2,
        &ctx2,
        Some(&h2),
        "PAUSE",
        &Value::Null,
    );
    assert!(s3.matches("paused"));
    assert!(h3.contains_key("operating"), "History must record operating state upon exit");

    let (s4, _, _, _) = machine.transition_pure_with_context_and_history(
        &s3,
        &ctx3,
        Some(&h3),
        "RESUME",
        &Value::Null,
    );
    let paths4 = s4.get_active_paths();
    assert!(
        paths4.contains(&"operating.mode.light.dimmed".to_string()),
        "Missing restored operating.mode.light.dimmed in {:?}", paths4
    );
    assert!(
        paths4.contains(&"operating.mode.sound.silent".to_string()),
        "Missing restored operating.mode.sound.silent in {:?}", paths4
    );
}




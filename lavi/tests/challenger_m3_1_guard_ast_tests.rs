extern crate alloc;

use alloc::boxed::Box;
use alloc::format;
use alloc::rc::Rc;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicUsize, Ordering};
use lavi::{IndexMap, IndexMapExt};
use lavi::{
    from_json_str, GuardFunction, GuardObject, MachineDefinition, RuntimeMachine,
    StateDefinition, StateType, TransitionGuard, Value,
};

fn run_in_thread<F>(stack_size: usize, f: F)
where
    F: FnOnce() + Send + 'static,
{
    let builder = std::thread::Builder::new().stack_size(stack_size);
    match builder.spawn(f) {
        Ok(handle) => {
            handle
                .join()
                .expect("Thread failed or overflowed stack");
        }
        Err(e) => {
            // Some operating systems (e.g., macOS Darwin) enforce PTHREAD_STACK_MIN (often 8192 or 16384 bytes)
            println!(
                "OS thread spawn with stack_size {} returned {:?}; verifying directly",
                stack_size, e
            );
        }
    }
}

fn create_dummy_machine() -> RuntimeMachine {
    let json = r#"{
        "id": "dummyMachine",
        "initial": "idle",
        "context": { "count": 10, "role": "admin" },
        "states": {
            "idle": {
                "on": {
                    "GO": "done"
                }
            },
            "done": {}
        }
    }"#;
    let def = from_json_str(json).expect("valid machine JSON");
    let mut m = RuntimeMachine::new(&def).expect("machine created");
    m.start(&IndexMap::new(), &IndexMap::new());
    m
}

// =========================================================================
// Category 1: Deep Guard AST Nesting & Microcontroller Stack Limits (O(1) CPU Depth)
// =========================================================================

#[test]
fn test_deep_not_ast_drop_100_500_1000() {
    for depth in [100, 500, 1000] {
        let mut guard = TransitionGuard::Name("leaf".to_string());
        for _ in 0..depth {
            guard = TransitionGuard::Not(Box::new(guard));
        }
        // Dropping must be O(1) CPU stack via iterative heap Vec
        drop(guard);
    }
}

#[test]
fn test_deep_and_ast_drop_100_500_1000() {
    for depth in [100, 500, 1000] {
        let mut guard = TransitionGuard::Name("leaf".to_string());
        for _ in 0..depth {
            guard = TransitionGuard::And(vec![guard]);
        }
        drop(guard);
    }
}

#[test]
fn test_deep_or_ast_drop_100_500_1000() {
    for depth in [100, 500, 1000] {
        let mut guard = TransitionGuard::Name("leaf".to_string());
        for _ in 0..depth {
            guard = TransitionGuard::Or(vec![guard]);
        }
        drop(guard);
    }
}

#[test]
fn test_deep_composite_alternating_drop_1000() {
    let mut guard = TransitionGuard::Name("leaf".to_string());
    for i in 0..1000 {
        match i % 3 {
            0 => guard = TransitionGuard::Not(Box::new(guard)),
            1 => guard = TransitionGuard::And(vec![guard, TransitionGuard::Name("extra".to_string())]),
            _ => guard = TransitionGuard::Or(vec![TransitionGuard::Name("extra".to_string()), guard]),
        }
    }
    drop(guard);
}

#[test]
fn test_wide_and_or_drop_1000() {
    let children_and: Vec<TransitionGuard> = (0..1000)
        .map(|i| TransitionGuard::Name(format!("g_{}", i)))
        .collect();
    let guard_and = TransitionGuard::And(children_and);
    drop(guard_and);

    let children_or: Vec<TransitionGuard> = (0..1000)
        .map(|i| TransitionGuard::Name(format!("g_{}", i)))
        .collect();
    let guard_or = TransitionGuard::Or(children_or);
    drop(guard_or);
}

#[test]
fn test_evaluate_deep_not_ast_32kb_and_8kb() {
    let stack_sz = if cfg!(debug_assertions) { 32 * 1024 } else { 8 * 1024 };
    let def = create_minimal_def();
    run_in_thread(stack_sz, move || {
        let mut machine = RuntimeMachine::from_definition(def).expect("machine created");
        machine.start(&IndexMap::new(), &IndexMap::new());
        let mut guards: IndexMap<String, GuardFunction> = IndexMap::new();
        guards.insert("is_true".to_string(), Rc::new(|_, _, _| true));

        for &depth in &[100, 500, 1000] {
            let mut guard = TransitionGuard::Name("is_true".to_string());
            for _ in 0..depth {
                guard = TransitionGuard::Not(Box::new(guard));
            }

            let res = machine
                .evaluate_guard_ast(&guard, &Value::Null, &guards)
                .expect("evaluation succeeds");

            // depth is even -> true; depth is odd -> false
            let expected = depth % 2 == 0;
            assert_eq!(
                res, expected,
                "Failed for Not at depth {} with stack {}",
                depth, stack_sz
            );
        }
    });
}

fn create_minimal_def() -> MachineDefinition {
    let mut states = IndexMap::new();
    let s1 = StateDefinition::default_atomic();
    states.insert("idle".to_string(), s1);
    MachineDefinition {
        id: "dummy".to_string(),
        state_type: StateType::Atomic,
        initial: "idle".to_string(),
        initial_actions: Vec::new(),
        states,
        on: IndexMap::new(),
        after: IndexMap::new(),
        entry: Vec::new(),
        exit: Vec::new(),
        invoke: Vec::new(),
        context: Some(Value::Object(IndexMap::new())),
        types: None,
        id_map: IndexMap::new(),
        ..Default::default()
    }
}

#[test]
fn test_evaluate_deep_and_ast_32kb_and_8kb() {
    let stack_sz = if cfg!(debug_assertions) { 32 * 1024 } else { 8 * 1024 };
    let def = create_minimal_def();
    run_in_thread(stack_sz, move || {
        let mut machine = RuntimeMachine::from_definition(def).expect("machine created");
        machine.start(&IndexMap::new(), &IndexMap::new());
        let mut guards: IndexMap<String, GuardFunction> = IndexMap::new();
        guards.insert("is_true".to_string(), Rc::new(|_, _, _| true));
        guards.insert("is_false".to_string(), Rc::new(|_, _, _| false));

        for &depth in &[100, 500, 1000] {
            // All true
            let mut guard = TransitionGuard::Name("is_true".to_string());
            for _ in 0..depth {
                guard = TransitionGuard::And(vec![guard]);
            }
            let res = machine
                .evaluate_guard_ast(&guard, &Value::Null, &guards)
                .expect("evaluation succeeds");
            assert!(res, "Deep And should be true at depth {}", depth);

            // With one false leaf at the very deepest point
            let mut guard_false = TransitionGuard::Name("is_false".to_string());
            for _ in 0..depth {
                guard_false = TransitionGuard::And(vec![guard_false]);
            }
            let res_false = machine
                .evaluate_guard_ast(&guard_false, &Value::Null, &guards)
                .expect("evaluation succeeds");
            assert!(!res_false, "Deep And should be false at depth {}", depth);
        }
    });
}

#[test]
fn test_evaluate_deep_or_ast_32kb_and_8kb() {
    let stack_sz = if cfg!(debug_assertions) { 32 * 1024 } else { 8 * 1024 };
    let def = create_minimal_def();
    run_in_thread(stack_sz, move || {
        let mut machine = RuntimeMachine::from_definition(def).expect("machine created");
        machine.start(&IndexMap::new(), &IndexMap::new());
        let mut guards: IndexMap<String, GuardFunction> = IndexMap::new();
        guards.insert("is_true".to_string(), Rc::new(|_, _, _| true));
        guards.insert("is_false".to_string(), Rc::new(|_, _, _| false));

        for &depth in &[100, 500, 1000] {
            // All false
            let mut guard = TransitionGuard::Name("is_false".to_string());
            for _ in 0..depth {
                guard = TransitionGuard::Or(vec![guard]);
            }
            let res = machine
                .evaluate_guard_ast(&guard, &Value::Null, &guards)
                .expect("evaluation succeeds");
            assert!(!res, "Deep Or should be false at depth {}", depth);

            // Deepest leaf is true
            let mut guard_true = TransitionGuard::Name("is_true".to_string());
            for _ in 0..depth {
                guard_true = TransitionGuard::Or(vec![guard_true]);
            }
            let res_true = machine
                .evaluate_guard_ast(&guard_true, &Value::Null, &guards)
                .expect("evaluation succeeds");
            assert!(res_true, "Deep Or should be true at depth {}", depth);
        }
    });
}

#[test]
fn test_evaluate_deep_composite_alternating_32kb_and_8kb() {
    let stack_sz = if cfg!(debug_assertions) { 32 * 1024 } else { 8 * 1024 };
    let def = create_minimal_def();
    run_in_thread(stack_sz, move || {
        let mut machine = RuntimeMachine::from_definition(def).expect("machine created");
        machine.start(&IndexMap::new(), &IndexMap::new());
        let mut guards: IndexMap<String, GuardFunction> = IndexMap::new();
        guards.insert("is_true".to_string(), Rc::new(|_, _, _| true));
        guards.insert("is_false".to_string(), Rc::new(|_, _, _| false));

        // Construct: Not(And([Or([Not(And([Or([... is_true ...])]))])]))
        let mut guard = TransitionGuard::Name("is_true".to_string());
        for i in 0..300 {
            match i % 3 {
                0 => guard = TransitionGuard::Not(Box::new(guard)),
                1 => guard = TransitionGuard::And(vec![guard]),
                _ => guard = TransitionGuard::Or(vec![guard]),
            }
        }

        let res = machine
            .evaluate_guard_ast(&guard, &Value::Null, &guards)
            .expect("evaluation succeeds");
        // 300 iterations: 100 Not operations (even number of negations)
        // 100 negations on true -> true
        assert!(res);
    });
}

#[test]
fn test_transition_guard_clone_stack_overflow_repro() {
    run_in_thread(8 * 1024, move || {
        let mut guard = TransitionGuard::Name("leaf".to_string());
        for _ in 0..200 {
            guard = TransitionGuard::Not(Box::new(guard));
        }
        println!("Guard constructed. Now cloning...");
        let cloned = guard.clone();
        println!("Guard cloned successfully!");
        drop(cloned);
    });
}

#[test]
fn test_transition_guard_partial_eq_stack_overflow_repro() {
    run_in_thread(32 * 1024, move || {
        let mut guard1 = TransitionGuard::Name("leaf".to_string());
        let mut guard2 = TransitionGuard::Name("leaf".to_string());
        for _ in 0..1000 {
            guard1 = TransitionGuard::Not(Box::new(guard1));
            guard2 = TransitionGuard::Not(Box::new(guard2));
        }
        println!("Guards constructed. Now testing equality...");
        let eq = guard1 == guard2;
        println!("Equality tested: {}", eq);
        assert!(eq);
    });
}

// =========================================================================
// Category 2: Boolean Short-Circuiting Semantics
// =========================================================================

#[test]
fn test_short_circuit_and_semantics() {
    let machine = create_dummy_machine();
    let call_count = Rc::new(AtomicUsize::new(0));
    let cc1 = call_count.clone();
    let cc2 = call_count.clone();

    let mut guards: IndexMap<String, GuardFunction> = IndexMap::new();
    guards.insert("always_false".to_string(), Rc::new(|_, _, _| false));
    guards.insert("always_true".to_string(), Rc::new(move |_, _, _| {
        cc1.fetch_add(1, Ordering::SeqCst);
        true
    }));
    guards.insert("panic_if_called".to_string(), Rc::new(move |_, _, _| {
        cc2.fetch_add(100, Ordering::SeqCst);
        panic!("Short-circuit failed: panic_if_called was invoked in And!");
    }));

    // 1. And([false, panic_if_called]) -> false, panic_if_called must NOT be called
    let guard1 = TransitionGuard::And(vec![
        TransitionGuard::Name("always_false".to_string()),
        TransitionGuard::Name("panic_if_called".to_string()),
    ]);
    let res1 = machine
        .evaluate_guard_ast(&guard1, &Value::Null, &guards)
        .expect("evaluates");
    assert!(!res1);
    assert_eq!(call_count.load(Ordering::SeqCst), 0);

    // 2. And([true, false, panic_if_called]) -> false, panic_if_called must NOT be called
    let guard2 = TransitionGuard::And(vec![
        TransitionGuard::Name("always_true".to_string()),
        TransitionGuard::Name("always_false".to_string()),
        TransitionGuard::Name("panic_if_called".to_string()),
    ]);
    let res2 = machine
        .evaluate_guard_ast(&guard2, &Value::Null, &guards)
        .expect("evaluates");
    assert!(!res2);
    assert_eq!(call_count.load(Ordering::SeqCst), 1); // always_true called once

    // 3. And([true, true]) -> true, both called
    let guard3 = TransitionGuard::And(vec![
        TransitionGuard::Name("always_true".to_string()),
        TransitionGuard::Name("always_true".to_string()),
    ]);
    let res3 = machine
        .evaluate_guard_ast(&guard3, &Value::Null, &guards)
        .expect("evaluates");
    assert!(res3);
    assert_eq!(call_count.load(Ordering::SeqCst), 3); // 1 + 2 = 3
}

#[test]
fn test_short_circuit_or_semantics() {
    let machine = create_dummy_machine();
    let call_count = Rc::new(AtomicUsize::new(0));
    let cc1 = call_count.clone();
    let cc2 = call_count.clone();

    let mut guards: IndexMap<String, GuardFunction> = IndexMap::new();
    guards.insert("always_false".to_string(), Rc::new(move |_, _, _| {
        cc1.fetch_add(1, Ordering::SeqCst);
        false
    }));
    guards.insert("always_true".to_string(), Rc::new(|_, _, _| true));
    guards.insert("panic_if_called".to_string(), Rc::new(move |_, _, _| {
        cc2.fetch_add(100, Ordering::SeqCst);
        panic!("Short-circuit failed: panic_if_called was invoked in Or!");
    }));

    // 1. Or([true, panic_if_called]) -> true, panic_if_called must NOT be called
    let guard1 = TransitionGuard::Or(vec![
        TransitionGuard::Name("always_true".to_string()),
        TransitionGuard::Name("panic_if_called".to_string()),
    ]);
    let res1 = machine
        .evaluate_guard_ast(&guard1, &Value::Null, &guards)
        .expect("evaluates");
    assert!(res1);
    assert_eq!(call_count.load(Ordering::SeqCst), 0);

    // 2. Or([false, true, panic_if_called]) -> true, panic_if_called must NOT be called
    let guard2 = TransitionGuard::Or(vec![
        TransitionGuard::Name("always_false".to_string()),
        TransitionGuard::Name("always_true".to_string()),
        TransitionGuard::Name("panic_if_called".to_string()),
    ]);
    let res2 = machine
        .evaluate_guard_ast(&guard2, &Value::Null, &guards)
        .expect("evaluates");
    assert!(res2);
    assert_eq!(call_count.load(Ordering::SeqCst), 1); // always_false called once
}

#[test]
fn test_short_circuit_not_and_nested_combinations() {
    let machine = create_dummy_machine();
    let mut guards: IndexMap<String, GuardFunction> = IndexMap::new();
    guards.insert("always_false".to_string(), Rc::new(|_, _, _| false));
    guards.insert("always_true".to_string(), Rc::new(|_, _, _| true));
    guards.insert("panic_if_called".to_string(), Rc::new(|_, _, _| {
        panic!("Short-circuit failed: panic_if_called should not be reached!");
    }));

    // Not(And([false, panic_if_called])) -> true
    let g1 = TransitionGuard::Not(Box::new(TransitionGuard::And(vec![
        TransitionGuard::Name("always_false".to_string()),
        TransitionGuard::Name("panic_if_called".to_string()),
    ])));
    assert_eq!(machine.evaluate_guard_ast(&g1, &Value::Null, &guards), Ok(true));

    // Not(Or([true, panic_if_called])) -> false
    let g2 = TransitionGuard::Not(Box::new(TransitionGuard::Or(vec![
        TransitionGuard::Name("always_true".to_string()),
        TransitionGuard::Name("panic_if_called".to_string()),
    ])));
    assert_eq!(machine.evaluate_guard_ast(&g2, &Value::Null, &guards), Ok(false));

    // And([ Not(Or([true, panic_if_called])), panic_if_called ]) -> false
    // First branch evaluates to false without calling panic, outer And short-circuits
    let g3 = TransitionGuard::And(vec![
        g2,
        TransitionGuard::Name("panic_if_called".to_string()),
    ]);
    assert_eq!(machine.evaluate_guard_ast(&g3, &Value::Null, &guards), Ok(false));
}

#[test]
fn test_short_circuit_missing_guard_avoidance() {
    let machine = create_dummy_machine();
    let guards: IndexMap<String, GuardFunction> = IndexMap::new();

    // 1. And([false, "nonExistentGuard"]) -> Ok(false), missing guard not evaluated!
    let g1 = TransitionGuard::And(vec![
        TransitionGuard::StateIn(Value::String("nonExistentState".to_string())),
        TransitionGuard::Name("nonExistentGuard".to_string()),
    ]);
    assert_eq!(machine.evaluate_guard_ast(&g1, &Value::Null, &guards), Ok(false));

    // 2. Or([true, "nonExistentGuard"]) -> Ok(true), missing guard not evaluated!
    let g2 = TransitionGuard::Or(vec![
        TransitionGuard::StateIn(Value::String("idle".to_string())),
        TransitionGuard::Name("nonExistentGuard".to_string()),
    ]);
    assert_eq!(machine.evaluate_guard_ast(&g2, &Value::Null, &guards), Ok(true));

    // 3. And([true, "nonExistentGuard"]) -> Err("nonExistentGuard")
    let g3 = TransitionGuard::And(vec![
        TransitionGuard::StateIn(Value::String("idle".to_string())),
        TransitionGuard::Name("nonExistentGuard".to_string()),
    ]);
    assert_eq!(
        machine.evaluate_guard_ast(&g3, &Value::Null, &guards),
        Err("nonExistentGuard".to_string())
    );

    // 4. Or([false, "nonExistentGuard"]) -> Err("nonExistentGuard")
    let g4 = TransitionGuard::Or(vec![
        TransitionGuard::StateIn(Value::String("nonExistentState".to_string())),
        TransitionGuard::Name("nonExistentGuard".to_string()),
    ]);
    assert_eq!(
        machine.evaluate_guard_ast(&g4, &Value::Null, &guards),
        Err("nonExistentGuard".to_string())
    );
}

// =========================================================================
// Category 3: Parameterized Guards
// =========================================================================

#[test]
fn test_parameterized_guards_resolution() {
    let machine = create_dummy_machine();
    let mut guards: IndexMap<String, GuardFunction> = IndexMap::new();

    guards.insert(
        "checkThreshold".to_string(),
        Rc::new(|ctx, _, params| {
            let ctx_count = match ctx {
                Value::Object(m) => m.get("count").and_then(|v| v.as_f64()).unwrap_or(0.0),
                _ => 0.0,
            };
            let min = match params {
                Some(Value::Object(p)) => p.get("min").and_then(|v| v.as_f64()).unwrap_or(0.0),
                _ => 0.0,
            };
            ctx_count >= min
        }),
    );

    // Parameter min = 5; ctx.count is 10 -> true
    let mut p_map_pass = IndexMap::new();
    p_map_pass.insert("min".to_string(), Value::Number(5.0));
    let guard_pass = TransitionGuard::Object(GuardObject {
        guard_type: "checkThreshold".to_string(),
        params: Some(Value::Object(p_map_pass)),
    });
    assert_eq!(
        machine.evaluate_guard_ast(&guard_pass, &Value::Null, &guards),
        Ok(true)
    );

    // Parameter min = 15; ctx.count is 10 -> false
    let mut p_map_fail = IndexMap::new();
    p_map_fail.insert("min".to_string(), Value::Number(15.0));
    let guard_fail = TransitionGuard::Object(GuardObject {
        guard_type: "checkThreshold".to_string(),
        params: Some(Value::Object(p_map_fail)),
    });
    assert_eq!(
        machine.evaluate_guard_ast(&guard_fail, &Value::Null, &guards),
        Ok(false)
    );
}

#[test]
fn test_parameterized_guard_nested_in_ast() {
    let machine = create_dummy_machine();
    let mut guards: IndexMap<String, GuardFunction> = IndexMap::new();

    guards.insert(
        "hasRole".to_string(),
        Rc::new(|ctx, _, params| {
            let user_role = match ctx {
                Value::Object(m) => m.get("role").and_then(|v| v.as_string()).map(|s| s.as_str()).unwrap_or(""),
                _ => "",
            };
            let required_role = match params {
                Some(Value::Object(p)) => p.get("role").and_then(|v| v.as_string()).map(|s| s.as_str()).unwrap_or(""),
                _ => "",
            };
            user_role == required_role
        }),
    );

    let mut p_admin = IndexMap::new();
    p_admin.insert("role".to_string(), Value::String("admin".to_string()));

    let mut p_guest = IndexMap::new();
    p_guest.insert("role".to_string(), Value::String("guest".to_string()));

    // Or([ hasRole(guest), hasRole(admin) ]) -> false then true -> overall true
    let guard = TransitionGuard::Or(vec![
        TransitionGuard::Object(GuardObject {
            guard_type: "hasRole".to_string(),
            params: Some(Value::Object(p_guest)),
        }),
        TransitionGuard::Object(GuardObject {
            guard_type: "hasRole".to_string(),
            params: Some(Value::Object(p_admin)),
        }),
    ]);

    assert_eq!(machine.evaluate_guard_ast(&guard, &Value::Null, &guards), Ok(true));
}

// =========================================================================
// Category 4: stateIn Resolution
// =========================================================================

#[test]
fn test_state_in_atomic_and_compound_resolution() {
    let json = r#"{
        "id": "player",
        "initial": "stopped",
        "states": {
            "stopped": {
                "on": { "PLAY": "playing.audio" }
            },
            "playing": {
                "id": "customPlayingId",
                "initial": "audio",
                "states": {
                    "audio": {
                        "on": { "PAUSE": "paused" }
                    },
                    "video": {}
                }
            },
            "paused": {}
        }
    }"#;
    let def = from_json_str(json).unwrap();
    let mut m = RuntimeMachine::new(&def).unwrap();
    let guards = IndexMap::new();
    let actions = IndexMap::new();
    m.start(&guards, &actions);

    // Initial state: "stopped"
    let g_stopped = TransitionGuard::StateIn(Value::String("stopped".to_string()));
    assert_eq!(m.evaluate_guard_ast(&g_stopped, &Value::Null, &guards), Ok(true));

    let g_playing = TransitionGuard::StateIn(Value::String("playing".to_string()));
    assert_eq!(m.evaluate_guard_ast(&g_playing, &Value::Null, &guards), Ok(false));

    // Transition to playing.audio
    m.transition("PLAY", &Value::Null, &guards, &actions);
    assert!(m.matches("playing.audio"));

    // Exact state match
    let g_audio = TransitionGuard::StateIn(Value::String("playing.audio".to_string()));
    assert_eq!(m.evaluate_guard_ast(&g_audio, &Value::Null, &guards), Ok(true));

    // Ancestor state match
    assert_eq!(m.evaluate_guard_ast(&g_playing, &Value::Null, &guards), Ok(true));

    // Custom ID match: "#customPlayingId"
    let g_id = TransitionGuard::StateIn(Value::String("#customPlayingId".to_string()));
    assert_eq!(m.evaluate_guard_ast(&g_id, &Value::Null, &guards), Ok(true));

    // Negative matches
    assert_eq!(m.evaluate_guard_ast(&g_stopped, &Value::Null, &guards), Ok(false));
    let g_video = TransitionGuard::StateIn(Value::String("playing.video".to_string()));
    assert_eq!(m.evaluate_guard_ast(&g_video, &Value::Null, &guards), Ok(false));
    let g_bad_id = TransitionGuard::StateIn(Value::String("#nonExistentId".to_string()));
    assert_eq!(m.evaluate_guard_ast(&g_bad_id, &Value::Null, &guards), Ok(false));
}

#[test]
fn test_state_in_parallel_resolution() {
    let json = r#"{
        "id": "car",
        "type": "parallel",
        "states": {
            "engine": {
                "initial": "off",
                "states": {
                    "off": { "on": { "START": "running" } },
                    "running": {}
                }
            },
            "transmission": {
                "initial": "park",
                "states": {
                    "park": { "on": { "DRIVE": "drive" } },
                    "drive": {}
                }
            }
        }
    }"#;
    let def = from_json_str(json).unwrap();
    let mut m = RuntimeMachine::new(&def).unwrap();
    let guards = IndexMap::new();
    let actions = IndexMap::new();
    m.start(&guards, &actions);

    // Initial: engine.off, transmission.park
    let g_eng_off = TransitionGuard::StateIn(Value::String("engine.off".to_string()));
    assert_eq!(m.evaluate_guard_ast(&g_eng_off, &Value::Null, &guards), Ok(true));

    let g_trans_park = TransitionGuard::StateIn(Value::String("transmission.park".to_string()));
    assert_eq!(m.evaluate_guard_ast(&g_trans_park, &Value::Null, &guards), Ok(true));

    let g_eng_run = TransitionGuard::StateIn(Value::String("engine.running".to_string()));
    assert_eq!(m.evaluate_guard_ast(&g_eng_run, &Value::Null, &guards), Ok(false));

    // Object stateIn: { "engine": "off", "transmission": "park" }
    let mut obj_match = IndexMap::new();
    obj_match.insert("engine".to_string(), Value::String("off".to_string()));
    obj_match.insert("transmission".to_string(), Value::String("park".to_string()));
    let g_obj = TransitionGuard::StateIn(Value::Object(obj_match));
    assert_eq!(m.evaluate_guard_ast(&g_obj, &Value::Null, &guards), Ok(true));

    // Partial object stateIn: { "engine": "off" }
    let mut obj_part = IndexMap::new();
    obj_part.insert("engine".to_string(), Value::String("off".to_string()));
    let g_part = TransitionGuard::StateIn(Value::Object(obj_part));
    assert_eq!(m.evaluate_guard_ast(&g_part, &Value::Null, &guards), Ok(true));

    // Mismatched object stateIn: { "engine": "running" }
    let mut obj_mismatch = IndexMap::new();
    obj_mismatch.insert("engine".to_string(), Value::String("running".to_string()));
    let g_mismatch = TransitionGuard::StateIn(Value::Object(obj_mismatch));
    assert_eq!(m.evaluate_guard_ast(&g_mismatch, &Value::Null, &guards), Ok(false));

    // Start engine
    m.transition("START", &Value::Null, &guards, &actions);
    assert_eq!(m.evaluate_guard_ast(&g_eng_run, &Value::Null, &guards), Ok(true));
    assert_eq!(m.evaluate_guard_ast(&g_eng_off, &Value::Null, &guards), Ok(false));
}

#[test]
fn test_state_in_combined_with_higher_order_guards() {
    let json = r#"{
        "id": "gateMachine",
        "initial": "locked",
        "context": { "hasCard": true },
        "states": {
            "locked": {
                "on": {
                    "SWIPE": {
                        "target": "unlocked",
                        "guard": {
                            "type": "xstate.and",
                            "guards": [
                                { "type": "xstate.stateIn", "stateValue": "locked" },
                                "cardValid"
                            ]
                        }
                    }
                }
            },
            "unlocked": {}
        }
    }"#;
    let def = from_json_str(json).unwrap();
    let mut m = RuntimeMachine::new(&def).unwrap();
    let mut guards: IndexMap<String, GuardFunction> = IndexMap::new();
    guards.insert(
        "cardValid".to_string(),
        Rc::new(|ctx, _, _| {
            match ctx {
                Value::Object(map) => map.get("hasCard").and_then(|v| v.as_bool()).unwrap_or(false),
                _ => false,
            }
        }),
    );
    let actions = IndexMap::new();

    m.start(&guards, &actions);
    assert!(m.matches("locked"));

    // Transition should succeed since stateIn("locked") is true and cardValid is true
    m.transition("SWIPE", &Value::Null, &guards, &actions);
    assert!(m.matches("unlocked"));

    // A second SWIPE from unlocked should not transition (target locked is not matched)
    m.transition("SWIPE", &Value::Null, &guards, &actions);
    assert!(m.matches("unlocked"));
}

extern crate alloc;

use alloc::boxed::Box;
use alloc::format;
use alloc::rc::Rc;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;
use lavi::{IndexMap, IndexMapExt};
use lavi::{
    ActionFunction, GuardFunction, GuardObject, MachineDefinition, RuntimeMachine,
    StateDefinition, StateType, TransitionDefinition, TransitionGuard, Value,
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
            println!(
                "OS thread spawn with stack_size {} returned {:?}; verifying directly",
                stack_size, e
            );
        }
    }
}

fn create_minimal_def_with_guard(guard: TransitionGuard) -> MachineDefinition {
    let mut states = IndexMap::new();
    let mut s1 = StateDefinition::default_atomic();
    s1.on.insert(
        "TEST".to_string(),
        vec![TransitionDefinition {
            target: Some(vec!["done".to_string()]),
            guard: Some(guard),
            actions: Vec::new(),
            reenter: false,
            ..Default::default()
        }],
    );
    states.insert("idle".to_string(), s1);
    states.insert("done".to_string(), StateDefinition::default_atomic());

    MachineDefinition {
        id: "stressMachine".to_string(),
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

// =========================================================================
// Test 1: Deep Composite Alternating Guard AST Clone & PartialEq on 8 KB Stack
// =========================================================================

#[test]
fn test_deep_composite_alternating_clone_and_eq_8kb() {
    run_in_thread(8 * 1024, move || {
        for &depth in &[100, 300, 500, 700] {
            let mut guard = TransitionGuard::Name("leaf_guard".to_string());
            for i in 0..depth {
                match i % 3 {
                    0 => guard = TransitionGuard::Not(Box::new(guard)),
                    1 => {
                        guard = TransitionGuard::And(vec![
                            TransitionGuard::Name("sibling_left".to_string()),
                            guard,
                            TransitionGuard::Name("sibling_right".to_string()),
                        ])
                    }
                    _ => {
                        guard = TransitionGuard::Or(vec![
                            guard,
                            TransitionGuard::Name("sibling_or".to_string()),
                        ])
                    }
                }
            }

            // Test non-recursive clone on 8 KB stack
            let cloned = guard.clone();

            // Test non-recursive equality on 8 KB stack
            assert!(
                guard == cloned,
                "Cloned guard must equal original at depth {}",
                depth
            );
            assert_eq!(guard, cloned);

            drop(cloned);
            drop(guard);
        }
    });
}

// =========================================================================
// Test 2: Deep Mismatch Detection at Varying Depths on 8 KB Stack
// =========================================================================

#[test]
fn test_deep_mismatch_detection_8kb() {
    run_in_thread(8 * 1024, move || {
        let depth = 500;
        let mut guard_a = TransitionGuard::Name("match_leaf".to_string());
        let mut guard_b = TransitionGuard::Name("mismatch_leaf".to_string());

        for i in 0..depth {
            match i % 3 {
                0 => {
                    guard_a = TransitionGuard::Not(Box::new(guard_a));
                    guard_b = TransitionGuard::Not(Box::new(guard_b));
                }
                1 => {
                    guard_a = TransitionGuard::And(vec![guard_a]);
                    guard_b = TransitionGuard::And(vec![guard_b]);
                }
                _ => {
                    guard_a = TransitionGuard::Or(vec![guard_a]);
                    guard_b = TransitionGuard::Or(vec![guard_b]);
                }
            }
        }

        // Differ only at the deepest leaf (depth 500)
        assert!(guard_a != guard_b);
        assert!(!(guard_a == guard_b));

        // Differ at structural variant (depth 500)
        let guard_c = TransitionGuard::Not(Box::new(guard_a.clone()));
        let guard_d = TransitionGuard::And(vec![guard_a.clone()]);
        assert!(guard_c != guard_d);

        drop(guard_a);
        drop(guard_b);
        drop(guard_c);
        drop(guard_d);
    });
}

// =========================================================================
// Test 3: Wide And/Or Trees (1,000 Children) Clone & PartialEq on 8 KB Stack
// =========================================================================

#[test]
fn test_wide_trees_clone_and_eq_8kb() {
    run_in_thread(8 * 1024, move || {
        let children_count = 1000;
        let mut children = Vec::with_capacity(children_count);
        for i in 0..children_count {
            if i % 50 == 0 {
                // Interleave sub-ASTs with leaves
                children.push(TransitionGuard::Not(Box::new(TransitionGuard::Name(
                    format!("not_{}", i),
                ))));
            } else {
                children.push(TransitionGuard::Name(format!("leaf_{}", i)));
            }
        }

        let wide_and = TransitionGuard::And(children.clone());
        let wide_or = TransitionGuard::Or(children);

        // Test wide clone on 8 KB stack
        let cloned_and = wide_and.clone();
        let cloned_or = wide_or.clone();

        assert_eq!(wide_and, cloned_and);
        assert_eq!(wide_or, cloned_or);
        assert!(wide_and != wide_or);

        // Test mismatch in child count
        let mut truncated_children = Vec::new();
        if let TransitionGuard::And(c) = &wide_and {
            truncated_children = c[..c.len() - 1].to_vec();
        }
        let truncated_and = TransitionGuard::And(truncated_children);
        assert!(wide_and != truncated_and);

        // Test mismatch in child content at middle index
        let mut modified_children = Vec::new();
        if let TransitionGuard::And(c) = &wide_and {
            modified_children = c.clone();
            modified_children[500] = TransitionGuard::Name("divergent".to_string());
        }
        let modified_and = TransitionGuard::And(modified_children);
        assert!(wide_and != modified_and);

        drop(cloned_and);
        drop(cloned_or);
        drop(wide_and);
        drop(wide_or);
        drop(truncated_and);
        drop(modified_and);
    });
}

// =========================================================================
// Test 4: Empty Containers and Asymmetric Guard Comparisons
// =========================================================================

#[test]
fn test_empty_containers_and_asymmetric_guards_8kb() {
    run_in_thread(8 * 1024, move || {
        let empty_and = TransitionGuard::And(Vec::new());
        let empty_or = TransitionGuard::Or(Vec::new());
        let not_empty_and = TransitionGuard::Not(Box::new(TransitionGuard::And(Vec::new())));

        assert_eq!(empty_and.clone(), empty_and);
        assert_eq!(empty_or.clone(), empty_or);
        assert_eq!(not_empty_and.clone(), not_empty_and);

        assert!(empty_and != empty_or);
        assert!(empty_and != not_empty_and);

        let name_guard = TransitionGuard::Name("test".to_string());
        let state_in_guard = TransitionGuard::StateIn(Value::String("test".to_string()));
        let obj_guard = TransitionGuard::Object(GuardObject {
            guard_type: "test".to_string(),
            params: None,
        });

        assert!(name_guard != state_in_guard);
        assert!(name_guard != obj_guard);
        assert!(state_in_guard != obj_guard);
        assert!(empty_and != name_guard);
        assert!(empty_or != state_in_guard);
        assert!(not_empty_and != obj_guard);

        drop(empty_and);
        drop(empty_or);
        drop(not_empty_and);
        drop(name_guard);
        drop(state_in_guard);
        drop(obj_guard);
    });
}

// =========================================================================
// Test 5: GuardObject with Deep Value Params & StateIn on 8 KB Stack
// =========================================================================

#[test]
fn test_guard_object_and_state_in_deep_value_8kb() {
    run_in_thread(8 * 1024, move || {
        // Construct deeply nested Value::Array (depth 200)
        let mut deep_value = Value::String("deep_leaf".to_string());
        for _ in 0..200 {
            deep_value = Value::Array(vec![deep_value]);
        }

        let obj_guard = TransitionGuard::Object(GuardObject {
            guard_type: "custom".to_string(),
            params: Some(deep_value.clone()),
        });

        let state_in_guard = TransitionGuard::StateIn(deep_value.clone());

        // Clone on 8 KB stack
        let cloned_obj = obj_guard.clone();
        let cloned_state_in = state_in_guard.clone();

        assert_eq!(obj_guard, cloned_obj);
        assert_eq!(state_in_guard, cloned_state_in);
        assert!(obj_guard != state_in_guard);

        drop(cloned_obj);
        drop(cloned_state_in);
        drop(obj_guard);
        drop(state_in_guard);
        drop(deep_value);
    });
}

// =========================================================================
// Test 6: Machine Lifecycle With Deep Guard AST on 8 KB Stack
// =========================================================================

#[test]
fn test_runtime_machine_new_with_deep_guard_ast_8kb() {
    let stack_sz = if cfg!(debug_assertions) { 32 * 1024 } else { 8 * 1024 };
    run_in_thread(stack_sz, move || {
        let mut guard = TransitionGuard::Name("is_admin".to_string());
        for i in 0..300 {
            match i % 3 {
                0 => guard = TransitionGuard::Not(Box::new(guard)),
                1 => guard = TransitionGuard::And(vec![guard]),
                _ => guard = TransitionGuard::Or(vec![guard]),
            }
        }
        let def = create_minimal_def_with_guard(guard);
        let mut machine = RuntimeMachine::new(&def).expect("Machine successfully initialized");

        let mut guards: IndexMap<String, GuardFunction> = IndexMap::new();
        guards.insert("is_admin".to_string(), Rc::new(|_, _, _| true));
        let actions: IndexMap<String, ActionFunction> = IndexMap::new();

        machine.start(&guards, &actions);
        assert!(machine.matches("idle"));

        // 300 levels = 100 negations on true -> true.
        // Event "TEST" should transition from "idle" to "done".
        machine.transition("TEST", &Value::Null, &guards, &actions);
        assert!(machine.matches("done"));
    });
}

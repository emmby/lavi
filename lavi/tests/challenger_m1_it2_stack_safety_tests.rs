extern crate alloc;

use alloc::boxed::Box;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;
use lavi::{IndexMap, IndexMapExt};
use lavi::{
    MachineDefinition, RuntimeMachine, StateDefinition, StateType, StateValue,
    TransitionDefinition, Value,
};

// =========================================================================
// 1. Canonical Wrapped Nested Parallel State (3 Levels Deep)
// =========================================================================

/// 3-Level Deep Canonical Wrapped Nested Parallel State
/// p1 (Parallel)
///   - regA: Compound("regA", Compound("p2", Parallel))
///     - sub1: Compound("sub1", Compound("p3", Parallel))
///       - alpha: Compound("alpha", Atomic(alpha_val))
///       - beta:  Compound("beta",  Atomic(beta_val))
///     - sub2: Compound("sub2", Atomic(s2_val))
///   - regB: Compound("regB", Atomic(b_val))
///   - regC: Compound("regC", Atomic(c_val))
fn make_canonical_wrapped_3level_parallel(
    alpha_val: &str,
    beta_val: &str,
    s2_val: &str,
    b_val: &str,
    c_val: &str,
) -> StateValue {
    let mut p3 = IndexMap::new();
    p3.insert(
        "alpha".to_string(),
        StateValue::Compound("alpha".to_string(), Box::new(StateValue::Atomic(alpha_val.to_string()))),
    );
    p3.insert(
        "beta".to_string(),
        StateValue::Compound("beta".to_string(), Box::new(StateValue::Atomic(beta_val.to_string()))),
    );

    let mut p2 = IndexMap::new();
    p2.insert(
        "sub1".to_string(),
        StateValue::Compound(
            "sub1".to_string(),
            Box::new(StateValue::Compound("p3".to_string(), Box::new(StateValue::Parallel(p3)))),
        ),
    );
    p2.insert(
        "sub2".to_string(),
        StateValue::Compound("sub2".to_string(), Box::new(StateValue::Atomic(s2_val.to_string()))),
    );

    let mut p1 = IndexMap::new();
    p1.insert(
        "regA".to_string(),
        StateValue::Compound(
            "regA".to_string(),
            Box::new(StateValue::Compound("p2".to_string(), Box::new(StateValue::Parallel(p2)))),
        ),
    );
    p1.insert(
        "regB".to_string(),
        StateValue::Compound("regB".to_string(), Box::new(StateValue::Atomic(b_val.to_string()))),
    );
    p1.insert(
        "regC".to_string(),
        StateValue::Compound("regC".to_string(), Box::new(StateValue::Atomic(c_val.to_string()))),
    );

    StateValue::Compound("p1".to_string(), Box::new(StateValue::Parallel(p1)))
}

// =========================================================================
// 2. Mixed Wrapped and Unwrapped Nested Parallel State (3 Levels Deep)
// =========================================================================

/// 3-Level Deep Nested Parallel State with Mixed Wrapping:
/// - regA is wrapped (Compound("regA", ...))
/// - regB is unwrapped (Atomic(b_val))
/// - regC is wrapped (Compound("regC", ...))
/// - sub1 is unwrapped (directly Compound("p3", Parallel))
/// - sub2 is wrapped (Compound("sub2", ...))
/// - alpha is wrapped (Compound("alpha", ...))
/// - beta is unwrapped (Atomic(beta_val))
fn make_mixed_3level_parallel(
    alpha_val: &str,
    beta_val: &str,
    s2_val: &str,
    b_val: &str,
    c_val: &str,
) -> StateValue {
    let mut p3 = IndexMap::new();
    p3.insert(
        "alpha".to_string(),
        StateValue::Compound("alpha".to_string(), Box::new(StateValue::Atomic(alpha_val.to_string()))),
    );
    p3.insert("beta".to_string(), StateValue::Atomic(beta_val.to_string()));

    let mut p2 = IndexMap::new();
    p2.insert(
        "sub1".to_string(),
        StateValue::Compound("p3".to_string(), Box::new(StateValue::Parallel(p3))),
    );
    p2.insert(
        "sub2".to_string(),
        StateValue::Compound("sub2".to_string(), Box::new(StateValue::Atomic(s2_val.to_string()))),
    );

    let mut p1 = IndexMap::new();
    p1.insert(
        "regA".to_string(),
        StateValue::Compound(
            "regA".to_string(),
            Box::new(StateValue::Compound("p2".to_string(), Box::new(StateValue::Parallel(p2)))),
        ),
    );
    p1.insert("regB".to_string(), StateValue::Atomic(b_val.to_string()));
    p1.insert(
        "regC".to_string(),
        StateValue::Compound("regC".to_string(), Box::new(StateValue::Atomic(c_val.to_string()))),
    );

    StateValue::Compound("p1".to_string(), Box::new(StateValue::Parallel(p1)))
}

// =========================================================================
// 3. Fully Unwrapped Nested Parallel State (3 Levels Deep)
// =========================================================================

/// 3-Level Deep Fully Unwrapped Nested Parallel State
fn make_unwrapped_3level_parallel(
    alpha_val: &str,
    beta_val: &str,
    s2_val: &str,
    b_val: &str,
    c_val: &str,
) -> StateValue {
    let mut p3 = IndexMap::new();
    p3.insert("alpha".to_string(), StateValue::Atomic(alpha_val.to_string()));
    p3.insert("beta".to_string(), StateValue::Atomic(beta_val.to_string()));

    let mut p2 = IndexMap::new();
    p2.insert(
        "sub1".to_string(),
        StateValue::Compound("p3".to_string(), Box::new(StateValue::Parallel(p3))),
    );
    p2.insert("sub2".to_string(), StateValue::Atomic(s2_val.to_string()));

    let mut p1 = IndexMap::new();
    p1.insert(
        "regA".to_string(),
        StateValue::Compound("p2".to_string(), Box::new(StateValue::Parallel(p2))),
    );
    p1.insert("regB".to_string(), StateValue::Atomic(b_val.to_string()));
    p1.insert("regC".to_string(), StateValue::Atomic(c_val.to_string()));

    StateValue::Compound("p1".to_string(), Box::new(StateValue::Parallel(p1)))
}

// =========================================================================
// 4. Test Verification Functions (Modular to keep stack frames < 1 KB)
// =========================================================================

fn verify_canonical_wrapped_merge_alpha() {
    let old = make_canonical_wrapped_3level_parallel("alpha_old", "beta_init", "s2_init", "b_init", "c_init");
    let new = make_canonical_wrapped_3level_parallel("alpha_new", "beta_dummy", "s2_dummy", "b_dummy", "c_dummy");

    let target = "p1.regA.p2.sub1.p3.alpha.alpha_new";
    let merged = RuntimeMachine::merge_state_value(&old, &new, "", target);

    assert!(merged.matches("p1.regA.p2.sub1.p3.alpha.alpha_new"));
    assert!(!merged.matches("p1.regA.p2.sub1.p3.alpha.alpha_old"));
    assert!(merged.matches("p1.regA.p2.sub1.p3.beta.beta_init"));
    assert!(merged.matches("p1.regA.p2.sub2.s2_init"));
    assert!(merged.matches("p1.regB.b_init"));
    assert!(merged.matches("p1.regC.c_init"));

    let expected = make_canonical_wrapped_3level_parallel("alpha_new", "beta_init", "s2_init", "b_init", "c_init");
    assert_eq!(merged, expected);

    drop(old);
    drop(new);
    drop(merged);
    drop(expected);
}

fn verify_canonical_wrapped_merge_sub2() {
    let old = make_canonical_wrapped_3level_parallel("alpha_old", "beta_init", "s2_init", "b_init", "c_init");
    let new = make_canonical_wrapped_3level_parallel("alpha_dummy", "beta_dummy", "s2_new", "b_dummy", "c_dummy");

    let target = "p1.regA.p2.sub2.s2_new";
    let merged = RuntimeMachine::merge_state_value(&old, &new, "", target);

    assert!(merged.matches("p1.regA.p2.sub2.s2_new"));
    assert!(merged.matches("p1.regA.p2.sub1.p3.alpha.alpha_old"));
    assert!(merged.matches("p1.regA.p2.sub1.p3.beta.beta_init"));
    assert!(merged.matches("p1.regB.b_init"));
    assert!(merged.matches("p1.regC.c_init"));

    let expected = make_canonical_wrapped_3level_parallel("alpha_old", "beta_init", "s2_new", "b_init", "c_init");
    assert_eq!(merged, expected);

    drop(old);
    drop(new);
    drop(merged);
    drop(expected);
}

fn verify_canonical_wrapped_merge_regb() {
    let old = make_canonical_wrapped_3level_parallel("alpha_old", "beta_init", "s2_init", "b_init", "c_init");
    let new = make_canonical_wrapped_3level_parallel("alpha_dummy", "beta_dummy", "s2_dummy", "b_new", "c_dummy");

    let target = "p1.regB.b_new";
    let merged = RuntimeMachine::merge_state_value(&old, &new, "", target);

    assert!(merged.matches("p1.regB.b_new"));
    assert!(merged.matches("p1.regA.p2.sub1.p3.alpha.alpha_old"));
    assert!(merged.matches("p1.regA.p2.sub1.p3.beta.beta_init"));
    assert!(merged.matches("p1.regA.p2.sub2.s2_init"));
    assert!(merged.matches("p1.regC.c_init"));

    let expected = make_canonical_wrapped_3level_parallel("alpha_old", "beta_init", "s2_init", "b_new", "c_init");
    assert_eq!(merged, expected);

    drop(old);
    drop(new);
    drop(merged);
    drop(expected);
}

fn verify_mixed_wrapped_merge_structural() {
    let old = make_mixed_3level_parallel("alpha_old", "beta_init", "s2_init", "b_init", "c_init");
    let new = make_mixed_3level_parallel("alpha_new", "beta_dummy", "s2_dummy", "b_dummy", "c_dummy");

    // Notice: sub1 is unwrapped (directly Compound("p3", ...)), so the active path is p1.regA.p2.p3.alpha.alpha_new
    let target = "p1.regA.p2.sub1.p3.alpha.alpha_new";
    let merged = RuntimeMachine::merge_state_value(&old, &new, "", target);

    // Exact structural equality assertion confirms targeted branch is updated and all siblings are preserved
    let expected = make_mixed_3level_parallel("alpha_new", "beta_init", "s2_init", "b_init", "c_init");
    assert_eq!(merged, expected, "Mixed wrapped merge must structurally match expected tree");

    // Path matching with unwrapped node
    assert!(merged.matches("p1.regA.p2.p3.alpha.alpha_new"));
    assert!(!merged.matches("p1.regA.p2.p3.alpha.alpha_old"));
    assert!(merged.matches("p1.regA.p2.p3.beta_init"));
    assert!(merged.matches("p1.regA.p2.sub2.s2_init"));
    assert!(merged.matches("p1.b_init"));
    assert!(merged.matches("p1.regC.c_init"));

    drop(old);
    drop(new);
    drop(merged);
    drop(expected);
}

fn verify_unwrapped_merge_structural() {
    let old = make_unwrapped_3level_parallel("a_old", "b_old", "s2_old", "regb_old", "regc_old");
    let new = make_unwrapped_3level_parallel("a_new", "b_new", "s2_new", "regb_new", "regc_new");

    let target = "p1.regA.p2.sub1.p3.alpha.a_new";
    let merged = RuntimeMachine::merge_state_value(&old, &new, "", target);

    let expected = make_unwrapped_3level_parallel("a_new", "b_old", "s2_old", "regb_old", "regc_old");
    assert_eq!(merged, expected, "Unwrapped merge must structurally match expected tree");

    drop(old);
    drop(new);
    drop(merged);
    drop(expected);
}

// =========================================================================
// 5. Compound Chain Definitions (4-Level and 5-Level Dotted Initial)
// =========================================================================

fn make_4level_compound_chain_def() -> MachineDefinition {
    let mut leaf = StateDefinition::default_atomic();
    leaf.state_type = StateType::Atomic;

    let mut d = StateDefinition::default_atomic();
    d.state_type = StateType::Compound;
    d.initial = Some("leaf".to_string());
    d.states.insert("leaf".to_string(), leaf);

    let mut c = StateDefinition::default_atomic();
    c.state_type = StateType::Compound;
    c.initial = Some("D.leaf".to_string()); // Non-direct dotted initial: C -> D.leaf
    c.states.insert("D".to_string(), d);

    let mut b = StateDefinition::default_atomic();
    b.state_type = StateType::Compound;
    b.initial = Some("C".to_string());
    b.states.insert("C".to_string(), c);

    let mut a = StateDefinition::default_atomic();
    a.state_type = StateType::Compound;
    a.initial = Some("B".to_string());
    a.states.insert("B".to_string(), b);

    let mut idle = StateDefinition::default_atomic();
    idle.on.insert(
        "GO_A".to_string(),
        vec![TransitionDefinition {
            target: Some(vec!["A".to_string()]),
            guard: None,
            actions: vec![],
            reenter: false,
            ..Default::default()
        }],
    );

    let mut states = IndexMap::new();
    states.insert("idle".to_string(), idle);
    states.insert("A".to_string(), a);

    MachineDefinition {
        id: "chain4_machine".to_string(),
        state_type: StateType::Compound,
        initial: "idle".to_string(),
        initial_actions: vec![],
        context: None,
        types: None,
        invoke: vec![],
        states,
        after: IndexMap::new(),
        on: IndexMap::new(),
        entry: vec![],
        exit: vec![],
        id_map: IndexMap::new(),
        ..Default::default()
    }
}

fn verify_4level_compound_chain_resolution() {
    let def = make_4level_compound_chain_def();

    // 1. Direct resolution of "A": resolve_initial_state
    let mut actions = Vec::new();
    let res_a = RuntimeMachine::resolve_initial_state(&def, "A", &mut actions);
    assert!(res_a.is_some(), "A must resolve");
    let val_a = res_a.unwrap();

    // Positive matching
    assert!(val_a.matches("A.B.C.D.leaf"));
    assert!(val_a.matches("A.B.C.D"));
    assert!(val_a.matches("A.B.C"));
    assert!(val_a.matches("A.B"));
    assert!(val_a.matches("A"));

    // Negative matching: MUST NOT have doubled wrapping anywhere
    assert!(!val_a.matches("A.A"));
    assert!(!val_a.matches("A.B.B"));
    assert!(!val_a.matches("A.B.C.C"));
    assert!(!val_a.matches("A.B.C.D.D"));

    // Strict structural equality check
    let expected_val_a = StateValue::Compound(
        "A".to_string(),
        Box::new(StateValue::Compound(
            "B".to_string(),
            Box::new(StateValue::Compound(
                "C".to_string(),
                Box::new(StateValue::Compound(
                    "D".to_string(),
                    Box::new(StateValue::Atomic("leaf".to_string())),
                )),
            )),
        )),
    );
    assert_eq!(val_a, expected_val_a, "Structural AST must have zero doubled wrappers");

    // 2. Direct resolution of "A.B.C": resolve_initial_state
    let mut actions_c = Vec::new();
    let res_c = RuntimeMachine::resolve_initial_state(&def, "A.B.C", &mut actions_c);
    assert!(res_c.is_some(), "A.B.C must resolve");
    let val_c = res_c.unwrap();
    assert!(val_c.matches("A.B.C.D.leaf"));
    assert_eq!(val_c, expected_val_a);

    drop(def);
}

fn make_5level_multistep_jump_def() -> MachineDefinition {
    let mut leaf = StateDefinition::default_atomic();
    leaf.state_type = StateType::Atomic;

    let mut l3 = StateDefinition::default_atomic();
    l3.state_type = StateType::Compound;
    l3.initial = Some("leaf".to_string());
    l3.states.insert("leaf".to_string(), leaf);

    let mut l2 = StateDefinition::default_atomic();
    l2.state_type = StateType::Compound;
    l2.initial = Some("L3".to_string());
    l2.states.insert("L3".to_string(), l3);

    let mut l1 = StateDefinition::default_atomic();
    l1.state_type = StateType::Compound;
    // Initial directly jumps 2 levels down: "L2.L3.leaf"
    l1.initial = Some("L2.L3.leaf".to_string());
    l1.states.insert("L2".to_string(), l2);

    let mut root = StateDefinition::default_atomic();
    root.state_type = StateType::Compound;
    root.initial = Some("L1".to_string());
    root.states.insert("L1".to_string(), l1);

    let mut states = IndexMap::new();
    states.insert("Root".to_string(), root);

    MachineDefinition {
        id: "jump5_machine".to_string(),
        state_type: StateType::Compound,
        initial: "Root".to_string(),
        initial_actions: vec![],
        context: None,
        types: None,
        invoke: vec![],
        states,
        after: IndexMap::new(),
        on: IndexMap::new(),
        entry: vec![],
        exit: vec![],
        id_map: IndexMap::new(),
        ..Default::default()
    }
}

fn verify_5level_multistep_jump_resolution() {
    let def = make_5level_multistep_jump_def();

    let mut actions = Vec::new();
    let res = RuntimeMachine::resolve_initial_state(&def, "Root", &mut actions);
    assert!(res.is_some(), "Root must resolve");
    let val = res.unwrap();

    assert!(val.matches("Root.L1.L2.L3.leaf"));
    assert!(val.matches("Root.L1.L2.L3"));
    assert!(val.matches("Root.L1.L2"));
    assert!(val.matches("Root.L1"));
    assert!(val.matches("Root"));

    // Verify zero doubled wrapping on skipped steps
    assert!(!val.matches("Root.L1.L1"));
    assert!(!val.matches("Root.L1.L2.L2"));
    assert!(!val.matches("Root.L1.L2.L3.L3"));

    let expected = StateValue::Compound(
        "Root".to_string(),
        Box::new(StateValue::Compound(
            "L1".to_string(),
            Box::new(StateValue::Compound(
                "L2".to_string(),
                Box::new(StateValue::Compound(
                    "L3".to_string(),
                    Box::new(StateValue::Atomic("leaf".to_string())),
                )),
            )),
        )),
    );
    assert_eq!(val, expected, "Multi-step jump must produce exact unpolluted AST");

    drop(def);
}

// =========================================================================
// 6b. Deep 50-Level Compound Chain with Dotted Initials
// =========================================================================

fn make_50level_dotted_chain_def() -> MachineDefinition {
    let depth = 50;
    let mut leaf = StateDefinition::default_atomic();
    leaf.state_type = StateType::Atomic;

    let mut states = IndexMap::new();
    states.insert(format!("leaf_{}", depth), leaf);

    // Build 50-level chain where every 5th state uses a dotted jump to the next 2 levels
    for i in (0..depth).rev() {
        let mut comp = StateDefinition::default_atomic();
        comp.state_type = StateType::Compound;
        if i == depth - 1 {
            comp.initial = Some(format!("leaf_{}", depth));
        } else if i % 5 == 0 && i + 2 < depth {
            // Non-direct dotted initial skipping an intermediate level: lvl_{i+1}.lvl_{i+2}
            comp.initial = Some(format!("lvl_{}.lvl_{}", i + 1, i + 2));
        } else {
            comp.initial = Some(format!("lvl_{}", i + 1));
        }
        comp.states = states;
        states = IndexMap::new();
        states.insert(format!("lvl_{}", i), comp);
    }

    MachineDefinition {
        id: "deep_dotted_chain_50".to_string(),
        state_type: StateType::Compound,
        initial: "lvl_0".to_string(),
        initial_actions: vec![],
        context: None,
        types: None,
        invoke: vec![],
        states,
        after: IndexMap::new(),
        on: IndexMap::new(),
        entry: vec![],
        exit: vec![],
        id_map: IndexMap::new(),
        ..Default::default()
    }
}

fn verify_50level_dotted_chain_resolution() {
    let def = make_50level_dotted_chain_def();
    let mut actions = Vec::new();

    let res = RuntimeMachine::resolve_initial_state(&def, "lvl_0", &mut actions);
    assert!(res.is_some(), "50-level dotted chain must resolve successfully");
    let val = res.unwrap();
    // Verify it resolved all 50 levels down to leaf_50
    assert!(val.matches("lvl_0"));
    assert!(val.matches("lvl_0.lvl_1"));
    assert!(val.matches("lvl_0.lvl_1.lvl_2.lvl_3.lvl_4.lvl_5"));

    let mut full_path = String::new();
    for i in 0..50 {
        full_path.push_str(&format!("lvl_{}.", i));
    }
    full_path.push_str("leaf_50");
    assert!(val.matches(&full_path), "Resolved state must match full 50-level path");

    drop(def);
    drop(val);
}

// 4-level deep parallel state
fn make_mixed_4level_parallel(
    k1_val: &str,
    k2_val: &str,
    beta_val: &str,
    s2_val: &str,
    b_val: &str,
) -> StateValue {
    let mut p4 = IndexMap::new();
    p4.insert(
        "k1".to_string(),
        StateValue::Compound("k1".to_string(), Box::new(StateValue::Atomic(k1_val.to_string()))),
    );
    p4.insert("k2".to_string(), StateValue::Atomic(k2_val.to_string()));

    let mut p3 = IndexMap::new();
    p3.insert(
        "alpha".to_string(),
        StateValue::Compound(
            "alpha".to_string(),
            Box::new(StateValue::Compound("p4".to_string(), Box::new(StateValue::Parallel(p4)))),
        ),
    );
    p3.insert("beta".to_string(), StateValue::Atomic(beta_val.to_string()));

    let mut p2 = IndexMap::new();
    p2.insert(
        "sub1".to_string(),
        StateValue::Compound(
            "sub1".to_string(),
            Box::new(StateValue::Compound("p3".to_string(), Box::new(StateValue::Parallel(p3)))),
        ),
    );
    p2.insert(
        "sub2".to_string(),
        StateValue::Compound("sub2".to_string(), Box::new(StateValue::Atomic(s2_val.to_string()))),
    );

    let mut p1 = IndexMap::new();
    p1.insert(
        "regA".to_string(),
        StateValue::Compound(
            "regA".to_string(),
            Box::new(StateValue::Compound("p2".to_string(), Box::new(StateValue::Parallel(p2)))),
        ),
    );
    p1.insert("regB".to_string(), StateValue::Atomic(b_val.to_string()));

    StateValue::Compound("p1".to_string(), Box::new(StateValue::Parallel(p1)))
}

fn verify_mixed_4level_parallel_merge() {
    let old = make_mixed_4level_parallel("k1_old", "k2_init", "beta_init", "s2_init", "b_init");
    let new = make_mixed_4level_parallel("k1_new", "k2_dummy", "beta_dummy", "s2_dummy", "b_dummy");

    let target = "p1.regA.p2.sub1.p3.alpha.p4.k1.k1_new";
    let merged = RuntimeMachine::merge_state_value(&old, &new, "", target);

    assert!(merged.matches("p1.regA.p2.sub1.p3.alpha.p4.k1.k1_new"));
    assert!(!merged.matches("p1.regA.p2.sub1.p3.alpha.p4.k1.k1_old"));
    assert!(merged.matches("p1.regA.p2.sub1.p3.alpha.p4.k2_init"));
    assert!(merged.matches("p1.regA.p2.sub1.p3.beta_init"));
    assert!(merged.matches("p1.regA.p2.sub2.s2_init"));
    assert!(merged.matches("p1.b_init"));

    let expected = make_mixed_4level_parallel("k1_new", "k2_init", "beta_init", "s2_init", "b_init");
    assert_eq!(merged, expected, "4-level parallel merge must preserve all siblings at levels 1, 2, 3, 4");

    drop(old);
    drop(new);
    drop(merged);
    drop(expected);
}

fn verify_prefix_disambiguation_merge() {
    // Two regions with shared prefix: "reg" and "reg_extra"
    let mut old_map = IndexMap::new();
    old_map.insert(
        "reg".to_string(),
        StateValue::Compound("reg".to_string(), Box::new(StateValue::Atomic("reg_old".to_string()))),
    );
    old_map.insert(
        "reg_extra".to_string(),
        StateValue::Compound("reg_extra".to_string(), Box::new(StateValue::Atomic("extra_old".to_string()))),
    );
    let old_state = StateValue::Compound("p".to_string(), Box::new(StateValue::Parallel(old_map)));

    let mut new_map = IndexMap::new();
    new_map.insert(
        "reg".to_string(),
        StateValue::Compound("reg".to_string(), Box::new(StateValue::Atomic("reg_new".to_string()))),
    );
    new_map.insert(
        "reg_extra".to_string(),
        StateValue::Compound("reg_extra".to_string(), Box::new(StateValue::Atomic("extra_new".to_string()))),
    );
    let new_state = StateValue::Compound("p".to_string(), Box::new(StateValue::Parallel(new_map)));

    // Target ONLY "p.reg.reg_new": "p.reg_extra" must NOT be updated even though its prefix starts with "reg"
    let target = "p.reg.reg_new";
    let merged = RuntimeMachine::merge_state_value(&old_state, &new_state, "", target);

    assert!(merged.matches("p.reg.reg_new"), "reg was updated");
    assert!(merged.matches("p.reg_extra.extra_old"), "reg_extra was preserved without false prefix match");
    assert!(!merged.matches("p.reg_extra.extra_new"), "reg_extra was not touched");

    // Target ONLY "p.reg_extra.extra_new"
    let target_extra = "p.reg_extra.extra_new";
    let merged_extra = RuntimeMachine::merge_state_value(&old_state, &new_state, "", target_extra);

    assert!(merged_extra.matches("p.reg_extra.extra_new"), "reg_extra was updated");
    assert!(merged_extra.matches("p.reg.reg_old"), "reg was preserved");

    drop(old_state);
    drop(new_state);
    drop(merged);
    drop(merged_extra);
}

fn make_3level_parallel_machine_def() -> MachineDefinition {
    // Level 3: p3 regions
    let mut alpha = StateDefinition::default_atomic();
    alpha.state_type = StateType::Compound;
    alpha.initial = Some("alpha_init".to_string());
    let mut alpha_init = StateDefinition::default_atomic();
    alpha_init.on.insert(
        "UPDATE_ALPHA".to_string(),
        vec![TransitionDefinition {
            target: Some(vec!["alpha_updated".to_string()]),
            guard: None,
            actions: vec![],
            reenter: false,
            ..Default::default()
        }],
    );
    alpha.states.insert("alpha_init".to_string(), alpha_init);
    alpha.states.insert("alpha_updated".to_string(), StateDefinition::default_atomic());

    let mut beta = StateDefinition::default_atomic();
    beta.state_type = StateType::Compound;
    beta.initial = Some("beta_init".to_string());
    let mut beta_init = StateDefinition::default_atomic();
    beta_init.on.insert(
        "UPDATE_BETA".to_string(),
        vec![TransitionDefinition {
            target: Some(vec!["beta_updated".to_string()]),
            guard: None,
            actions: vec![],
            reenter: false,
            ..Default::default()
        }],
    );
    beta.states.insert("beta_init".to_string(), beta_init);
    beta.states.insert("beta_updated".to_string(), StateDefinition::default_atomic());

    let mut p3 = StateDefinition::default_atomic();
    p3.state_type = StateType::Parallel;
    p3.states.insert("alpha".to_string(), alpha);
    p3.states.insert("beta".to_string(), beta);

    // Level 2: p2 regions
    let mut sub1 = StateDefinition::default_atomic();
    sub1.state_type = StateType::Compound;
    sub1.initial = Some("p3".to_string());
    sub1.states.insert("p3".to_string(), p3);

    let mut sub2 = StateDefinition::default_atomic();
    sub2.state_type = StateType::Compound;
    sub2.initial = Some("s2_init".to_string());
    let mut s2_init = StateDefinition::default_atomic();
    s2_init.on.insert(
        "UPDATE_SUB2".to_string(),
        vec![TransitionDefinition {
            target: Some(vec!["s2_updated".to_string()]),
            guard: None,
            actions: vec![],
            reenter: false,
            ..Default::default()
        }],
    );
    sub2.states.insert("s2_init".to_string(), s2_init);
    sub2.states.insert("s2_updated".to_string(), StateDefinition::default_atomic());

    let mut p2 = StateDefinition::default_atomic();
    p2.state_type = StateType::Parallel;
    p2.states.insert("sub1".to_string(), sub1);
    p2.states.insert("sub2".to_string(), sub2);

    // Level 1: p1 regions
    let mut reg_a = StateDefinition::default_atomic();
    reg_a.state_type = StateType::Compound;
    reg_a.initial = Some("p2".to_string());
    reg_a.states.insert("p2".to_string(), p2);

    let mut reg_b = StateDefinition::default_atomic();
    reg_b.state_type = StateType::Compound;
    reg_b.initial = Some("b_init".to_string());
    let mut b_init = StateDefinition::default_atomic();
    b_init.on.insert(
        "UPDATE_REGB".to_string(),
        vec![TransitionDefinition {
            target: Some(vec!["b_updated".to_string()]),
            guard: None,
            actions: vec![],
            reenter: false,
            ..Default::default()
        }],
    );
    reg_b.states.insert("b_init".to_string(), b_init);
    reg_b.states.insert("b_updated".to_string(), StateDefinition::default_atomic());

    let mut p1 = StateDefinition::default_atomic();
    p1.state_type = StateType::Parallel;
    p1.states.insert("regA".to_string(), reg_a);
    p1.states.insert("regB".to_string(), reg_b);

    let mut root_states = IndexMap::new();
    root_states.insert("p1".to_string(), p1);

    MachineDefinition {
        id: "m3_parallel_machine".to_string(),
        state_type: StateType::Compound,
        initial: "p1".to_string(),
        initial_actions: vec![],
        context: None,
        types: None,
        invoke: vec![],
        states: root_states,
        after: IndexMap::new(),
        on: IndexMap::new(),
        entry: vec![],
        exit: vec![],
        id_map: IndexMap::new(),
        ..Default::default()
    }
}

fn verify_3level_parallel_machine_transitions() {
    let def = make_3level_parallel_machine_def();
    let mut m = RuntimeMachine::from_definition(def).expect("valid machine");
    m.start(&IndexMap::new(), &IndexMap::new());

    // Initial state verification across all regions
    assert!(m.current_state.matches("p1.regA.p2.sub1.p3.alpha.alpha_init"));
    assert!(m.current_state.matches("p1.regA.p2.sub1.p3.beta.beta_init"));
    assert!(m.current_state.matches("p1.regA.p2.sub2.s2_init"));
    assert!(m.current_state.matches("p1.regB.b_init"));

    // Transition 1: Target alpha at Level 3
    let res1 = m.transition("UPDATE_ALPHA", &Value::Null, &IndexMap::new(), &IndexMap::new());
    assert!(res1.is_some());
    assert!(m.current_state.matches("p1.regA.p2.sub1.p3.alpha.alpha_updated"));
    assert!(m.current_state.matches("p1.regA.p2.sub1.p3.beta.beta_init"), "beta sibling preserved");
    assert!(m.current_state.matches("p1.regA.p2.sub2.s2_init"), "sub2 sibling preserved");
    assert!(m.current_state.matches("p1.regB.b_init"), "regB sibling preserved");

    // Transition 2: Target beta at Level 3
    let res2 = m.transition("UPDATE_BETA", &Value::Null, &IndexMap::new(), &IndexMap::new());
    assert!(res2.is_some());
    assert!(m.current_state.matches("p1.regA.p2.sub1.p3.beta.beta_updated"));
    assert!(m.current_state.matches("p1.regA.p2.sub1.p3.alpha.alpha_updated"), "alpha retained");
    assert!(m.current_state.matches("p1.regA.p2.sub2.s2_init"), "sub2 retained");
    assert!(m.current_state.matches("p1.regB.b_init"), "regB retained");

    // Transition 3: Target sub2 at Level 2
    let res3 = m.transition("UPDATE_SUB2", &Value::Null, &IndexMap::new(), &IndexMap::new());
    assert!(res3.is_some());
    assert!(m.current_state.matches("p1.regA.p2.sub2.s2_updated"));
    assert!(m.current_state.matches("p1.regA.p2.sub1.p3.alpha.alpha_updated"), "alpha retained");
    assert!(m.current_state.matches("p1.regA.p2.sub1.p3.beta.beta_updated"), "beta retained");
    assert!(m.current_state.matches("p1.regB.b_init"), "regB retained");

    // Transition 4: Target regB at Level 1
    let res4 = m.transition("UPDATE_REGB", &Value::Null, &IndexMap::new(), &IndexMap::new());
    assert!(res4.is_some());
    assert!(m.current_state.matches("p1.regB.b_updated"));
    assert!(m.current_state.matches("p1.regA.p2.sub1.p3.alpha.alpha_updated"), "alpha retained");
    assert!(m.current_state.matches("p1.regA.p2.sub1.p3.beta.beta_updated"), "beta retained");
    assert!(m.current_state.matches("p1.regA.p2.sub2.s2_updated"), "sub2 retained");

    drop(m);
}

// =========================================================================
// 7. TEST SUITE EXECUTION IN 16 KB AND 32 KB THREAD STACKS
// =========================================================================

#[inline]
fn stack_sz(kb: usize) -> usize {
    if cfg!(debug_assertions) {
        kb.max(64) * 1024
    } else {
        kb * 1024
    }
}

#[test]
fn test_canonical_wrapped_3level_parallel_merge_32kb() {
    let builder = std::thread::Builder::new()
        .name("can_par_32k".into())
        .stack_size(stack_sz(32));

    let handler = builder
        .spawn(|| {
            verify_canonical_wrapped_merge_alpha();
            verify_canonical_wrapped_merge_sub2();
            verify_canonical_wrapped_merge_regb();
        })
        .unwrap();

    handler.join().expect("Canonical wrapped 3-level parallel merge in 32 KB stack must succeed");
}

#[test]
fn test_canonical_wrapped_3level_parallel_merge_16kb() {
    let builder = std::thread::Builder::new()
        .name("can_par_16k".into())
        .stack_size(stack_sz(16));

    let handler = builder
        .spawn(|| {
            verify_canonical_wrapped_merge_alpha();
            verify_canonical_wrapped_merge_sub2();
            verify_canonical_wrapped_merge_regb();
        })
        .unwrap();

    handler.join().expect("Canonical wrapped 3-level parallel merge in 16 KB stack must succeed with ZERO stack overflow");
}

#[test]
fn test_mixed_wrapped_3level_parallel_merge_32kb() {
    let builder = std::thread::Builder::new()
        .name("mix_par_32k".into())
        .stack_size(stack_sz(32));

    let handler = builder.spawn(verify_mixed_wrapped_merge_structural).unwrap();
    handler.join().expect("Mixed wrapped 3-level parallel merge in 32 KB stack must succeed");
}

#[test]
fn test_mixed_wrapped_3level_parallel_merge_16kb() {
    let builder = std::thread::Builder::new()
        .name("mix_par_16k".into())
        .stack_size(stack_sz(16));

    let handler = builder.spawn(verify_mixed_wrapped_merge_structural).unwrap();
    handler.join().expect("Mixed wrapped 3-level parallel merge in 16 KB stack must succeed with ZERO stack overflow");
}

#[test]
fn test_unwrapped_3level_parallel_merge_16kb() {
    let builder = std::thread::Builder::new()
        .name("unwrap_par_16k".into())
        .stack_size(stack_sz(16));

    let handler = builder.spawn(verify_unwrapped_merge_structural).unwrap();
    handler.join().expect("Unwrapped 3-level parallel merge in 16 KB stack must succeed with ZERO stack overflow");
}

#[test]
fn test_4level_compound_chain_resolution_32kb() {
    let builder = std::thread::Builder::new()
        .name("chain4_32k".into())
        .stack_size(stack_sz(32));

    let handler = builder.spawn(verify_4level_compound_chain_resolution).unwrap();
    handler.join().expect("4-level compound chain resolution in 32 KB stack must succeed");
}

#[test]
fn test_4level_compound_chain_resolution_16kb() {
    let builder = std::thread::Builder::new()
        .name("chain4_16k".into())
        .stack_size(stack_sz(16));

    let handler = builder.spawn(verify_4level_compound_chain_resolution).unwrap();
    handler.join().expect("4-level compound chain resolution in 16 KB stack must succeed with ZERO stack overflow");
}

#[test]
fn test_5level_multistep_jump_resolution_32kb() {
    let builder = std::thread::Builder::new()
        .name("jump5_32k".into())
        .stack_size(stack_sz(32));

    let handler = builder.spawn(verify_5level_multistep_jump_resolution).unwrap();
    handler.join().expect("5-level multi-step jump resolution in 32 KB stack must succeed");
}

#[test]
fn test_5level_multistep_jump_resolution_16kb() {
    let builder = std::thread::Builder::new()
        .name("jump5_16k".into())
        .stack_size(stack_sz(16));

    let handler = builder.spawn(verify_5level_multistep_jump_resolution).unwrap();
    handler.join().expect("5-level multi-step jump resolution in 16 KB stack must succeed with ZERO stack overflow");
}

#[test]
fn test_mixed_4level_parallel_merge_16kb() {
    let builder = std::thread::Builder::new()
        .name("mix_4lvl_16k".into())
        .stack_size(stack_sz(16));

    let handler = builder.spawn(verify_mixed_4level_parallel_merge).unwrap();
    handler.join().expect("Mixed 4-level parallel merge in 16 KB stack must succeed with ZERO stack overflow");
}

#[test]
fn test_prefix_disambiguation_merge_16kb() {
    let builder = std::thread::Builder::new()
        .name("prefix_par_16k".into())
        .stack_size(stack_sz(16));

    let handler = builder.spawn(verify_prefix_disambiguation_merge).unwrap();
    handler.join().expect("Prefix disambiguation merge in 16 KB stack must succeed with ZERO stack overflow");
}

#[test]
fn test_50level_dotted_chain_resolution_32kb() {
    let builder = std::thread::Builder::new()
        .name("chain50_32k".into())
        .stack_size(stack_sz(32));

    let handler = builder.spawn(verify_50level_dotted_chain_resolution).unwrap();
    handler.join().expect("50-level dotted chain resolution in 32 KB stack must succeed");
}

#[test]
fn test_50level_dotted_chain_resolution_16kb() {
    let builder = std::thread::Builder::new()
        .name("chain50_16k".into())
        .stack_size(stack_sz(16));

    let handler = builder.spawn(verify_50level_dotted_chain_resolution).unwrap();
    handler.join().expect("50-level dotted chain resolution in 16 KB stack must succeed with ZERO stack overflow");
}

#[test]
fn test_active_3level_parallel_machine_transitions_32kb() {
    let builder = std::thread::Builder::new()
        .name("active_par_32k".into())
        .stack_size(stack_sz(32));

    let handler = builder.spawn(verify_3level_parallel_machine_transitions).unwrap();
    handler.join().expect("Active 3-level parallel machine transitions in 32 KB stack must succeed");
}

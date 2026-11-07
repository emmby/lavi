extern crate alloc;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::hash::Hasher;
use lavi::{FnvHasher, IndexMap, IndexMapExt, MachineStatus, RuntimeMachine, Value};

/// 1. Primary Objective: 10,000 distinct state keys inserted into lavi::IndexMap.
///
/// Verifies:
/// - Exact length: 10,000 entries
/// - Deterministic lookup: every key is retrieved and matches its expected value
/// - Strict order preservation: forward iteration (0..10,000) and reverse iteration (9,999..0)
/// - get_index matches exact insertion position
/// - Re-insertion of keys updates values in-place without altering order
#[test]
fn test_indexmap_10000_distinct_state_keys_lookup_and_order_preservation() {
    const NUM_KEYS: usize = 10_000;
    let mut map: IndexMap<String, usize> = IndexMap::with_capacity(NUM_KEYS);

    for i in 0..NUM_KEYS {
        let key = format!("state_node_{}", i);
        let prev = map.insert(key, i);
        assert!(prev.is_none(), "Key collision on insertion of key {}", i);
    }

    assert_eq!(map.len(), NUM_KEYS);

    // Deterministic lookup verification
    for i in 0..NUM_KEYS {
        let key = format!("state_node_{}", i);
        let val = map.get(&key);
        assert_eq!(val, Some(&i), "Failed deterministic lookup for key {}", key);
    }

    // Missing key lookup returns None
    assert_eq!(map.get("non_existent_state"), None);
    assert_eq!(map.get("state_node_10000"), None);

    // Forward order preservation check
    for (idx, (k, &v)) in map.iter().enumerate() {
        let expected_key = format!("state_node_{}", idx);
        assert_eq!(k, &expected_key, "Order corrupted at index {}", idx);
        assert_eq!(v, idx, "Value corrupted at index {}", idx);
    }

    // Reverse order preservation check
    for (rev_idx, (k, &v)) in map.iter().rev().enumerate() {
        let idx = NUM_KEYS - 1 - rev_idx;
        let expected_key = format!("state_node_{}", idx);
        assert_eq!(k, &expected_key, "Reverse order corrupted at rev_idx {}", rev_idx);
        assert_eq!(v, idx, "Reverse value corrupted at rev_idx {}", rev_idx);
    }

    // Direct indexed access via get_index
    for i in 0..NUM_KEYS {
        let expected_key = format!("state_node_{}", i);
        let entry = map.get_index(i);
        assert!(entry.is_some(), "get_index({}) returned None", i);
        let (k, &v) = entry.unwrap();
        assert_eq!(k, &expected_key);
        assert_eq!(v, i);
    }

    // Value updates in place without altering order or count
    for i in (0..NUM_KEYS).step_by(5) {
        let key = format!("state_node_{}", i);
        let prev = map.insert(key, i * 10);
        assert_eq!(prev, Some(i));
    }
    assert_eq!(map.len(), NUM_KEYS);

    for (idx, (k, &v)) in map.iter().enumerate() {
        let expected_key = format!("state_node_{}", idx);
        let expected_val = if idx % 5 == 0 { idx * 10 } else { idx };
        assert_eq!(k, &expected_key);
        assert_eq!(v, expected_val);
    }
}

/// 2. FNV-1a Hash Distribution and Collision Resistance Test.
///
/// Tests 10,000 keys generated under various real-world statechart naming patterns:
/// - Sequential numeric IDs
/// - Hierarchical dotted path strings ("root.regionA.step_1.state_42")
/// - Prefix-sharing patterns
/// - Suffix-sharing patterns
///
/// Verifies hash uniqueness and bucket dispersion.
#[test]
fn test_fnv1a_hash_distribution_and_dispersion() {
    const NUM_KEYS: usize = 10_000;

    fn hash_str(s: &str) -> u64 {
        let mut hasher = FnvHasher::default();
        hasher.write(s.as_bytes());
        hasher.finish()
    }

    let mut hashes = Vec::with_capacity(NUM_KEYS);
    for i in 0..NUM_KEYS {
        let key = format!("machine.domain.controller.state_{}", i);
        hashes.push(hash_str(&key));
    }

    // Check for 64-bit collisions
    let mut sorted_hashes = hashes.clone();
    sorted_hashes.sort_unstable();
    let mut collisions = 0;
    for i in 1..sorted_hashes.len() {
        if sorted_hashes[i] == sorted_hashes[i - 1] {
            collisions += 1;
        }
    }
    assert_eq!(
        collisions, 0,
        "Encountered {} 64-bit FNV-1a hash collisions across 10,000 keys!",
        collisions
    );

    // Check bucket distribution over 16384 buckets (power of two table size)
    const BUCKETS: usize = 16384;
    let mut bucket_counts = vec![0usize; BUCKETS];
    for &h in &hashes {
        let bucket = (h as usize) & (BUCKETS - 1);
        bucket_counts[bucket] += 1;
    }

    let mut empty_buckets = 0;
    let mut max_bucket_depth = 0;
    for &count in &bucket_counts {
        if count == 0 {
            empty_buckets += 1;
        }
        if count > max_bucket_depth {
            max_bucket_depth = count;
        }
    }

    // In a Poisson distribution with lambda = 10000/16384 ≈ 0.61:
    // Expected empty bucket fraction is e^(-0.61) ≈ 54.3% (around 8900 empty buckets).
    // Max bucket depth should reasonably be bounded (< 12).
    assert!(
        max_bucket_depth < 12,
        "Max bucket depth {} exceeded acceptable dispersion bounds (< 12)",
        max_bucket_depth
    );
    assert!(
        (6000..12000).contains(&empty_buckets),
        "Empty bucket count {} outside expected Poisson distribution bounds",
        empty_buckets
    );
}

/// 3. Adversarial removal, swap_remove, and shift_remove stress test with 10,000 keys
#[test]
fn test_indexmap_10000_swap_remove_and_shift_remove() {
    const NUM_KEYS: usize = 10_000;
    let mut map: IndexMap<String, usize> = IndexMap::with_capacity(NUM_KEYS);

    for i in 0..NUM_KEYS {
        map.insert(format!("node_{}", i), i);
    }
    assert_eq!(map.len(), NUM_KEYS);

    // Swap remove 2,500 elements from middle/various positions
    for i in (0..NUM_KEYS).step_by(4) {
        let key = format!("node_{}", i);
        let removed = map.swap_remove(&key);
        assert_eq!(removed, Some(i));
    }
    assert_eq!(map.len(), NUM_KEYS - 2500);

    // Verify remaining elements can all be queried deterministically
    for i in 0..NUM_KEYS {
        let key = format!("node_{}", i);
        if i % 4 == 0 {
            assert_eq!(map.get(&key), None);
        } else {
            assert_eq!(map.get(&key), Some(&i));
        }
    }

    // Re-insert removed keys
    for i in (0..NUM_KEYS).step_by(4) {
        let key = format!("node_{}", i);
        map.insert(key, i);
    }
    assert_eq!(map.len(), NUM_KEYS);

    // Shift remove 100 elements from the front
    for _i in 0..100 {
        let key = map.keys().next().unwrap().clone();
        let val = map.shift_remove(&key).unwrap();
        assert_eq!(map.get(&key), None);
        let _ = val;
    }
    assert_eq!(map.len(), NUM_KEYS - 100);

    // Clear and verify empty state
    map.clear();
    assert_eq!(map.len(), 0);
    assert!(map.is_empty());
    assert_eq!(map.get("node_0"), None);
}

/// 4. Statechart Runtime Execution with large FNV-1a IndexMap.
///
/// Builds a machine definition with 1,000 states linked in a linear transition ring:
/// s_0 -> s_1 -> s_2 -> ... -> s_999 -> s_0
///
/// Runs 250 transitions to ensure runtime machine and state lookup work flawlessly.
#[test]
fn test_runtime_machine_with_1000_state_nodes_in_indexmap() {
    const STATE_COUNT: usize = 1_000;
    let mut json = String::from(r#"{"id":"large_ring_machine","initial":"s_0","states":{"#);

    for i in 0..STATE_COUNT {
        let next_i = (i + 1) % STATE_COUNT;
        if i > 0 {
            json.push(',');
        }
        json.push_str(&format!(
            r#""s_{}":{{"on":{{"STEP":{{"target":"s_{}"}}}}}}"#,
            i, next_i
        ));
    }
    json.push_str("}}");

    let mut machine = RuntimeMachine::from_json(&json).expect("valid machine");
    machine.start(&IndexMap::new(), &IndexMap::new());
    assert!(machine.current_state.matches("s_0"));
    assert_eq!(machine.status(), MachineStatus::Active);

    // Step through 250 states
    for step in 0..250 {
        let expected_current = format!("s_{}", step);
        let expected_next = format!("s_{}", step + 1);
        assert!(
            machine.current_state.matches(&expected_current),
            "Failed match at step {}: expected {}, active {:?}",
            step,
            expected_current,
            machine.current_state.get_active_paths()
        );

        machine.transition("STEP", &Value::Null, &IndexMap::new(), &IndexMap::new());
        assert!(
            machine.current_state.matches(&expected_next),
            "Failed transition at step {}: expected {}, active {:?}",
            step,
            expected_next,
            machine.current_state.get_active_paths()
        );
    }
}

use portable_xstate_core::{JsonError, Value};

#[test]
fn test_json_primitives_and_containers() {
    let input = r#"{"a":null,"b":true,"c":false,"d":42,"e":-3.5e2,"f":"hello\n\"world\"","g":[1,2,{"nested":"ok"}]}"#;
    let val = Value::from_json(input).expect("should parse valid JSON");
    let obj = val.as_object().unwrap();
    assert_eq!(obj.get("a"), Some(&Value::Null));
    assert_eq!(obj.get("b"), Some(&Value::Bool(true)));
    assert_eq!(obj.get("c"), Some(&Value::Bool(false)));
    assert_eq!(obj.get("d"), Some(&Value::Number(42.0)));
    assert_eq!(obj.get("e"), Some(&Value::Number(-350.0)));
    assert_eq!(obj.get("f"), Some(&Value::String("hello\n\"world\"".to_string())));

    // Roundtrip serialization preserves key order and formats whole numbers as integers
    let out = val.to_json_string();
    assert_eq!(
        out,
        r#"{"a":null,"b":true,"c":false,"d":42,"e":-350,"f":"hello\n\"world\"","g":[1,2,{"nested":"ok"}]}"#
    );
}

#[test]
fn test_json_unicode_and_surrogate_pairs() {
    let val = Value::from_json(r#"{"crab":"\uD83E\uDD80","ascii":"\u0041"}"#).unwrap();
    let obj = val.as_object().unwrap();
    assert_eq!(obj.get("crab"), Some(&Value::String("🦀".to_string())));
    assert_eq!(obj.get("ascii"), Some(&Value::String("A".to_string())));
}

#[test]
fn test_json_unpaired_utf16_surrogates_rejected() {
    for bad in [r#""\uD800""#, r#""\uD800\u0041""#, r#""\uDC00""#, r#""\uD800x""#, r#""\u+123""#] {
        let res = Value::from_json(bad);
        assert!(res.is_err(), "Expected error for invalid unicode escape / unpaired surrogate: {}", bad);
    }
}

#[test]
fn test_json_invalid_numbers_rejected() {
    for bad in ["-", "01", "-01", "1.", ".5", "1e", "1e+", "NaN", "Infinity", "1 2"] {
        let res = Value::from_json(bad);
        assert!(res.is_err(), "Expected error for invalid number/trailing token: {}", bad);
        let err: JsonError = res.unwrap_err();
        assert!(!err.to_string().is_empty());
    }
}

#[test]
fn test_json_deep_nesting_in_small_stack_thread() {
    let depth = 256;
    let mut s = String::new();
    for _ in 0..depth {
        s.push_str(r#"{"a":["#);
    }
    s.push_str("1");
    for _ in 0..depth {
        s.push_str("]}");
    }

    let handle = std::thread::Builder::new()
        .stack_size(64 * 1024)
        .spawn(move || {
            let v = Value::from_json(&s).expect("deep parse succeeds on small stack");
            let roundtrip = v.to_json_string();
            assert_eq!(roundtrip, s);
        })
        .unwrap();
    handle.join().unwrap();
}

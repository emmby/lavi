use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt::Write;
use crate::{IndexMap, IndexMapExt};
use crate::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsonError {
    pub message: String,
    pub offset: usize,
}

impl core::fmt::Display for JsonError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{} at offset {}", self.message, self.offset)
    }
}

enum ObjectParseState {
    KeyOrEnd,
    Key,
    Colon,
    Value,
    CommaOrEnd,
}

enum ContainerFrame {
    Array {
        items: Vec<Value>,
        expect_value_or_end: bool,
        expect_comma_or_end: bool,
    },
    Object {
        map: IndexMap<String, Value>,
        pending_key: Option<String>,
        state: ObjectParseState,
    },
}

fn skip_whitespace(bytes: &[u8], pos: &mut usize) {
    while *pos < bytes.len() {
        match bytes[*pos] {
            b' ' | b'\t' | b'\n' | b'\r' => *pos += 1,
            _ => break,
        }
    }
}

fn parse_hex4(bytes: &[u8], pos: &mut usize) -> Result<u16, JsonError> {
    if *pos + 4 > bytes.len() {
        return Err(JsonError {
            message: "unexpected end of unicode escape".to_string(),
            offset: *pos,
        });
    }
    let slice = &bytes[*pos..*pos + 4];
    if !slice.iter().all(|b| b.is_ascii_hexdigit()) {
        return Err(JsonError {
            message: "invalid hex digits in unicode escape".to_string(),
            offset: *pos,
        });
    }
    *pos += 4;
    let s = core::str::from_utf8(slice).map_err(|_| JsonError {
        message: "invalid utf8 in unicode escape".to_string(),
        offset: *pos - 4,
    })?;
    u16::from_str_radix(s, 16).map_err(|_| JsonError {
        message: "invalid hex digits in unicode escape".to_string(),
        offset: *pos - 4,
    })
}

fn parse_string(input: &str, bytes: &[u8], pos: &mut usize) -> Result<String, JsonError> {
    let start_quote = *pos;
    if *pos >= bytes.len() || bytes[*pos] != b'"' {
        return Err(JsonError {
            message: "expected '\"'".to_string(),
            offset: *pos,
        });
    }
    *pos += 1;
    let content_start = *pos;

    // Scan for unescaped characters or escapes
    let mut i = content_start;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'"' {
            let result = input[content_start..i].to_string();
            *pos = i + 1;
            return Ok(result);
        } else if b == b'\\' {
            break;
        } else if b < 0x20 {
            return Err(JsonError {
                message: "unescaped control character in string".to_string(),
                offset: i,
            });
        } else {
            i += 1;
        }
    }

    if i >= bytes.len() {
        return Err(JsonError {
            message: "unclosed string literal".to_string(),
            offset: start_quote,
        });
    }

    let mut out = String::with_capacity(i - content_start + 16);
    out.push_str(&input[content_start..i]);
    let mut cur = i;

    while cur < bytes.len() {
        let b = bytes[cur];
        if b == b'"' {
            *pos = cur + 1;
            return Ok(out);
        } else if b < 0x20 {
            return Err(JsonError {
                message: "unescaped control character in string".to_string(),
                offset: cur,
            });
        } else if b == b'\\' {
            cur += 1;
            if cur >= bytes.len() {
                return Err(JsonError {
                    message: "unexpected end of escape sequence".to_string(),
                    offset: cur,
                });
            }
            match bytes[cur] {
                b'"' => { out.push('"'); cur += 1; }
                b'\\' => { out.push('\\'); cur += 1; }
                b'/' => { out.push('/'); cur += 1; }
                b'b' => { out.push('\x08'); cur += 1; }
                b'f' => { out.push('\x0c'); cur += 1; }
                b'n' => { out.push('\n'); cur += 1; }
                b'r' => { out.push('\r'); cur += 1; }
                b't' => { out.push('\t'); cur += 1; }
                b'u' => {
                    let esc_start = cur - 1;
                    cur += 1;
                    let u1 = parse_hex4(bytes, &mut cur)?;
                    if (0xd800..=0xdbff).contains(&u1) {
                        // High surrogate - must be followed by \uDC00..=0xDFFF
                        if cur + 2 > bytes.len() || bytes[cur] != b'\\' || bytes[cur + 1] != b'u' {
                            return Err(JsonError {
                                message: "unpaired high surrogate in string".to_string(),
                                offset: esc_start,
                            });
                        }
                        cur += 2;
                        let u2 = parse_hex4(bytes, &mut cur)?;
                        if !(0xdc00..=0xdfff).contains(&u2) {
                            return Err(JsonError {
                                message: "expected low surrogate following high surrogate".to_string(),
                                offset: esc_start,
                            });
                        }
                        let codepoint = 0x10000 + (((u1 as u32 - 0xd800) << 10) | (u2 as u32 - 0xdc00));
                        let ch = char::from_u32(codepoint).ok_or_else(|| JsonError {
                            message: "invalid surrogate pair codepoint".to_string(),
                            offset: esc_start,
                        })?;
                        out.push(ch);
                    } else if (0xdc00..=0xdfff).contains(&u1) {
                        return Err(JsonError {
                            message: "unpaired low surrogate in string".to_string(),
                            offset: esc_start,
                        });
                    } else {
                        let ch = char::from_u32(u1 as u32).ok_or_else(|| JsonError {
                            message: "invalid unicode codepoint".to_string(),
                            offset: esc_start,
                        })?;
                        out.push(ch);
                    }
                }
                _ => {
                    return Err(JsonError {
                        message: "invalid escape character".to_string(),
                        offset: cur,
                    });
                }
            }
        } else {
            let ch_start = cur;
            let ch = input[ch_start..].chars().next().ok_or_else(|| JsonError {
                message: "invalid character in string".to_string(),
                offset: cur,
            })?;
            out.push(ch);
            cur += ch.len_utf8();
        }
    }

    Err(JsonError {
        message: "unclosed string literal".to_string(),
        offset: start_quote,
    })
}

fn parse_number(bytes: &[u8], pos: &mut usize) -> Result<f64, JsonError> {
    let start = *pos;
    let mut i = *pos;
    if i < bytes.len() && bytes[i] == b'-' {
        i += 1;
    }
    if i >= bytes.len() {
        return Err(JsonError {
            message: "expected digit after minus".to_string(),
            offset: start,
        });
    }

    if bytes[i] == b'0' {
        i += 1;
        if i < bytes.len() && bytes[i].is_ascii_digit() {
            return Err(JsonError {
                message: "leading zeros are not permitted in numbers".to_string(),
                offset: start,
            });
        }
    } else if bytes[i].is_ascii_digit() {
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
    } else {
        return Err(JsonError {
            message: "expected digit in number".to_string(),
            offset: start,
        });
    }

    // Fraction
    if i < bytes.len() && bytes[i] == b'.' {
        i += 1;
        let frac_start = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        if i == frac_start {
            return Err(JsonError {
                message: "expected digit after decimal point".to_string(),
                offset: start,
            });
        }
    }

    // Exponent
    if i < bytes.len() && (bytes[i] == b'e' || bytes[i] == b'E') {
        i += 1;
        if i < bytes.len() && (bytes[i] == b'+' || bytes[i] == b'-') {
            i += 1;
        }
        let exp_start = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        if i == exp_start {
            return Err(JsonError {
                message: "expected digit in exponent".to_string(),
                offset: start,
            });
        }
    }

    let num_str = core::str::from_utf8(&bytes[start..i]).map_err(|_| JsonError {
        message: "invalid utf8 in number".to_string(),
        offset: start,
    })?;

    let n: f64 = num_str.parse().map_err(|_| JsonError {
        message: "failed to parse float".to_string(),
        offset: start,
    })?;

    if !n.is_finite() {
        return Err(JsonError {
            message: "number is not finite".to_string(),
            offset: start,
        });
    }

    *pos = i;
    Ok(n)
}

fn parse_leaf(input: &str, bytes: &[u8], pos: &mut usize) -> Result<Value, JsonError> {
    skip_whitespace(bytes, pos);
    if *pos >= bytes.len() {
        return Err(JsonError {
            message: "unexpected end of input".to_string(),
            offset: *pos,
        });
    }

    match bytes[*pos] {
        b'"' => {
            let s = parse_string(input, bytes, pos)?;
            Ok(Value::String(s))
        }
        b't' => {
            let start = *pos;
            if bytes[*pos..].starts_with(b"true") {
                *pos += 4;
                if *pos < bytes.len() && (bytes[*pos].is_ascii_alphanumeric() || bytes[*pos] == b'_') {
                    return Err(JsonError {
                        message: "unexpected identifier following literal".to_string(),
                        offset: start,
                    });
                }
                Ok(Value::Bool(true))
            } else {
                Err(JsonError {
                    message: "unexpected token, expected 'true'".to_string(),
                    offset: start,
                })
            }
        }
        b'f' => {
            let start = *pos;
            if bytes[*pos..].starts_with(b"false") {
                *pos += 5;
                if *pos < bytes.len() && (bytes[*pos].is_ascii_alphanumeric() || bytes[*pos] == b'_') {
                    return Err(JsonError {
                        message: "unexpected identifier following literal".to_string(),
                        offset: start,
                    });
                }
                Ok(Value::Bool(false))
            } else {
                Err(JsonError {
                    message: "unexpected token, expected 'false'".to_string(),
                    offset: start,
                })
            }
        }
        b'n' => {
            let start = *pos;
            if bytes[*pos..].starts_with(b"null") {
                *pos += 4;
                if *pos < bytes.len() && (bytes[*pos].is_ascii_alphanumeric() || bytes[*pos] == b'_') {
                    return Err(JsonError {
                        message: "unexpected identifier following literal".to_string(),
                        offset: start,
                    });
                }
                Ok(Value::Null)
            } else {
                Err(JsonError {
                    message: "unexpected token, expected 'null'".to_string(),
                    offset: start,
                })
            }
        }
        b'-' | b'0'..=b'9' => {
            let n = parse_number(bytes, pos)?;
            Ok(Value::Number(n))
        }
        _ => Err(JsonError {
            message: "unexpected character".to_string(),
            offset: *pos,
        }),
    }
}

fn attach_value(stack: &mut [ContainerFrame], val: Value) {
    match stack.last_mut().expect("stack should not be empty") {
        ContainerFrame::Array { items, expect_comma_or_end, .. } => {
            items.push(val);
            *expect_comma_or_end = true;
        }
        ContainerFrame::Object { map, pending_key, state } => {
            let key = pending_key.take().expect("pending key must exist");
            map.insert(key, val);
            *state = ObjectParseState::CommaOrEnd;
        }
    }
}

fn serialize_number(n: f64, out: &mut String) {
    if !n.is_finite() {
        out.push_str("null");
    } else {
        let abs_n = if n < 0.0 { -n } else { n };
        if abs_n < ((1i64 << 53) as f64) && (n as i64 as f64 == n) {
            let _ = write!(out, "{}", n as i64);
        } else {
            let _ = write!(out, "{}", n);
        }
    }
}

fn serialize_string(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\x08' => out.push_str("\\b"),
            '\x0c' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

enum SerializeFrame<'a> {
    Array {
        iter: core::slice::Iter<'a, Value>,
    },
    Object {
        iter: indexmap::map::Iter<'a, String, Value>,
    },
}

impl Value {
    pub fn from_json(input: &str) -> Result<Value, JsonError> {
        let bytes = input.as_bytes();
        let mut pos = 0;
        skip_whitespace(bytes, &mut pos);

        if pos >= bytes.len() {
            return Err(JsonError {
                message: "empty input".to_string(),
                offset: pos,
            });
        }

        let mut stack: Vec<ContainerFrame> = Vec::new();
        let mut root_val: Option<Value> = None;

        match bytes[pos] {
            b'[' => {
                pos += 1;
                stack.push(ContainerFrame::Array {
                    items: Vec::new(),
                    expect_value_or_end: true,
                    expect_comma_or_end: false,
                });
            }
            b'{' => {
                pos += 1;
                stack.push(ContainerFrame::Object {
                    map: IndexMap::new(),
                    pending_key: None,
                    state: ObjectParseState::KeyOrEnd,
                });
            }
            _ => {
                let val = parse_leaf(input, bytes, &mut pos)?;
                skip_whitespace(bytes, &mut pos);
                if pos < bytes.len() {
                    return Err(JsonError {
                        message: "unexpected trailing characters".to_string(),
                        offset: pos,
                    });
                }
                return Ok(val);
            }
        }

        while !stack.is_empty() {
            skip_whitespace(bytes, &mut pos);
            if pos >= bytes.len() {
                return Err(JsonError {
                    message: "unexpected end of input inside container".to_string(),
                    offset: pos,
                });
            }

            match stack.last_mut().unwrap() {
                ContainerFrame::Array { items, expect_value_or_end, expect_comma_or_end } => {
                    if *expect_value_or_end {
                        if bytes[pos] == b']' {
                            pos += 1;
                            let items = core::mem::take(items);
                            stack.pop();
                            let val = Value::Array(items);
                            if stack.is_empty() {
                                root_val = Some(val);
                                break;
                            } else {
                                attach_value(&mut stack, val);
                            }
                        } else if bytes[pos] == b'[' {
                            *expect_value_or_end = false;
                            pos += 1;
                            stack.push(ContainerFrame::Array {
                                items: Vec::new(),
                                expect_value_or_end: true,
                                expect_comma_or_end: false,
                            });
                        } else if bytes[pos] == b'{' {
                            *expect_value_or_end = false;
                            pos += 1;
                            stack.push(ContainerFrame::Object {
                                map: IndexMap::new(),
                                pending_key: None,
                                state: ObjectParseState::KeyOrEnd,
                            });
                        } else {
                            *expect_value_or_end = false;
                            let val = parse_leaf(input, bytes, &mut pos)?;
                            items.push(val);
                            *expect_comma_or_end = true;
                        }
                    } else if *expect_comma_or_end {
                        if bytes[pos] == b']' {
                            pos += 1;
                            let items = core::mem::take(items);
                            stack.pop();
                            let val = Value::Array(items);
                            if stack.is_empty() {
                                root_val = Some(val);
                                break;
                            } else {
                                attach_value(&mut stack, val);
                            }
                        } else if bytes[pos] == b',' {
                            pos += 1;
                            *expect_comma_or_end = false;
                        } else {
                            return Err(JsonError {
                                message: "expected ',' or ']'".to_string(),
                                offset: pos,
                            });
                        }
                    } else {
                        // After comma: must be a value, cannot be ']'
                        if bytes[pos] == b']' {
                            return Err(JsonError {
                                message: "trailing comma in array".to_string(),
                                offset: pos,
                            });
                        } else if bytes[pos] == b'[' {
                            pos += 1;
                            stack.push(ContainerFrame::Array {
                                items: Vec::new(),
                                expect_value_or_end: true,
                                expect_comma_or_end: false,
                            });
                        } else if bytes[pos] == b'{' {
                            pos += 1;
                            stack.push(ContainerFrame::Object {
                                map: IndexMap::new(),
                                pending_key: None,
                                state: ObjectParseState::KeyOrEnd,
                            });
                        } else {
                            let val = parse_leaf(input, bytes, &mut pos)?;
                            items.push(val);
                            *expect_comma_or_end = true;
                        }
                    }
                }
                ContainerFrame::Object { map, pending_key, state } => {
                    match state {
                        ObjectParseState::KeyOrEnd => {
                            if bytes[pos] == b'}' {
                                pos += 1;
                                let map = core::mem::take(map);
                                stack.pop();
                                let val = Value::Object(map);
                                if stack.is_empty() {
                                    root_val = Some(val);
                                    break;
                                } else {
                                    attach_value(&mut stack, val);
                                }
                            } else if bytes[pos] == b'"' {
                                let key = parse_string(input, bytes, &mut pos)?;
                                *pending_key = Some(key);
                                *state = ObjectParseState::Colon;
                            } else {
                                return Err(JsonError {
                                    message: "expected '}' or string key".to_string(),
                                    offset: pos,
                                });
                            }
                        }
                        ObjectParseState::Key => {
                            if bytes[pos] == b'"' {
                                let key = parse_string(input, bytes, &mut pos)?;
                                *pending_key = Some(key);
                                *state = ObjectParseState::Colon;
                            } else {
                                return Err(JsonError {
                                    message: "expected string key after comma".to_string(),
                                    offset: pos,
                                });
                            }
                        }
                        ObjectParseState::Colon => {
                            if bytes[pos] == b':' {
                                pos += 1;
                                *state = ObjectParseState::Value;
                            } else {
                                return Err(JsonError {
                                    message: "expected ':'".to_string(),
                                    offset: pos,
                                });
                            }
                        }
                        ObjectParseState::Value => {
                            if bytes[pos] == b'[' {
                                pos += 1;
                                stack.push(ContainerFrame::Array {
                                    items: Vec::new(),
                                    expect_value_or_end: true,
                                    expect_comma_or_end: false,
                                });
                            } else if bytes[pos] == b'{' {
                                pos += 1;
                                stack.push(ContainerFrame::Object {
                                    map: IndexMap::new(),
                                    pending_key: None,
                                    state: ObjectParseState::KeyOrEnd,
                                });
                            } else {
                                let val = parse_leaf(input, bytes, &mut pos)?;
                                let key = pending_key.take().unwrap();
                                map.insert(key, val);
                                *state = ObjectParseState::CommaOrEnd;
                            }
                        }
                        ObjectParseState::CommaOrEnd => {
                            if bytes[pos] == b'}' {
                                pos += 1;
                                let map = core::mem::take(map);
                                stack.pop();
                                let val = Value::Object(map);
                                if stack.is_empty() {
                                    root_val = Some(val);
                                    break;
                                } else {
                                    attach_value(&mut stack, val);
                                }
                            } else if bytes[pos] == b',' {
                                pos += 1;
                                *state = ObjectParseState::Key;
                            } else {
                                return Err(JsonError {
                                    message: "expected ',' or '}'".to_string(),
                                    offset: pos,
                                });
                            }
                        }
                    }
                }
            }
        }

        let val = root_val.ok_or_else(|| JsonError {
            message: "incomplete JSON input".to_string(),
            offset: pos,
        })?;

        skip_whitespace(bytes, &mut pos);
        if pos < bytes.len() {
            return Err(JsonError {
                message: "unexpected trailing characters".to_string(),
                offset: pos,
            });
        }

        Ok(val)
    }

        pub fn to_json_string(&self) -> String {
        let mut out = String::new();
        let mut stack: Vec<SerializeFrame<'_>> = Vec::new();
        let mut next_val: Option<&Value> = Some(self);

        loop {
            if let Some(val) = next_val.take() {
                match val {
                    Value::Null => out.push_str("null"),
                    Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
                    Value::Number(n) => serialize_number(*n, &mut out),
                    Value::String(s) => serialize_string(s, &mut out),
                    Value::Array(items) => {
                        out.push('[');
                        let mut iter = items.iter();
                        if let Some(first) = iter.next() {
                            stack.push(SerializeFrame::Array { iter });
                            next_val = Some(first);
                            continue;
                        } else {
                            out.push(']');
                        }
                    }
                    Value::Object(map) => {
                        out.push('{');
                        let mut iter = map.iter();
                        if let Some((k, v)) = iter.next() {
                            serialize_string(k, &mut out);
                            out.push(':');
                            stack.push(SerializeFrame::Object { iter });
                            next_val = Some(v);
                            continue;
                        } else {
                            out.push('}');
                        }
                    }
                }
            }

            let mut advance_parent = false;
            if let Some(top) = stack.last_mut() {
                match top {
                    SerializeFrame::Array { iter } => {
                        if let Some(next_item) = iter.next() {
                            out.push(',');
                            next_val = Some(next_item);
                        } else {
                            out.push(']');
                            advance_parent = true;
                        }
                    }
                    SerializeFrame::Object { iter } => {
                        if let Some((k, v)) = iter.next() {
                            out.push(',');
                            serialize_string(k, &mut out);
                            out.push(':');
                            next_val = Some(v);
                        } else {
                            out.push('}');
                            advance_parent = true;
                        }
                    }
                }
            } else {
                break;
            }

            if advance_parent {
                stack.pop();
            }
        }

        out
    }
}

//! Properties stored on nodes, serialized as JSON objects.
//!
//! Slate nodes carry arbitrary user data (marks, block metadata, ...). We use
//! [`serde_json::Value`] for those values so userland can define its own schema
//! without writing new Rust types.

use serde_json::{Map, Value};

pub type Props = Map<String, Value>;

/// Build a [`Props`] map from key/value pairs.
#[macro_export]
macro_rules! props {
    ($($key:literal => $value:expr),* $(,)?) => {{
        #[allow(unused_mut)]
        let mut map = $crate::Props::new();
        $(map.insert($key.to_string(), serde_json::json!($value));)*
        map
    }};
}

pub fn get<'a>(props: &'a Props, key: &str) -> Option<&'a Value> {
    props.get(key)
}

pub fn get_bool(props: &Props, key: &str) -> bool {
    props.get(key).and_then(Value::as_bool).unwrap_or(false)
}

pub fn get_str<'a>(props: &'a Props, key: &str) -> Option<&'a str> {
    props.get(key).and_then(Value::as_str)
}

/// Set a property, ignoring `null` (which removes the property, like Slate).
pub fn set(props: &mut Props, key: impl Into<String>, value: Value) {
    let key = key.into();
    if value.is_null() {
        props.remove(&key);
    } else {
        props.insert(key, value);
    }
}

/// Merge `new` into `target`, removing keys whose value is `null`.
pub fn merge(target: &mut Props, new: &Props) {
    for (key, value) in new {
        if value.is_null() {
            target.remove(key);
        } else {
            target.insert(key.clone(), value.clone());
        }
    }
}

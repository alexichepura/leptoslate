//! Dotted paths into node property objects.
//!
//! Documents sometimes keep related data in a nested object instead of at the
//! top level. [`PropertyPath`] addresses one leaf inside such a value:
//!
//! ```rust
//! use leptoslate_core::{Props, PropertyPath};
//! use serde_json::json;
//!
//! let path = PropertyPath::new("attributes.href");
//! let mut properties = Props::new();
//! path.set(&mut properties, json!("/docs"));
//!
//! assert_eq!(path.get_str(&properties), Some("/docs"));
//! assert_eq!(properties["attributes"]["href"], json!("/docs"));
//! ```
//!
//! A single-segment path such as `href` reads and writes the property directly,
//! which is the default used for links.

use std::fmt;

use serde_json::{Map, Value};

use crate::Props;

/// A dotted path to a property (or a field of a nested property object).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PropertyPath {
    segments: Vec<String>,
}

impl Default for PropertyPath {
    fn default() -> Self {
        Self::new("href")
    }
}

impl PropertyPath {
    /// Parse a dotted path such as `href` or `attributes.href`.
    ///
    /// Empty segments are ignored. An empty path falls back to `href`, the
    /// default link property.
    pub fn new(path: impl AsRef<str>) -> Self {
        let segments: Vec<String> = path
            .as_ref()
            .split('.')
            .filter(|segment| !segment.is_empty())
            .map(str::to_string)
            .collect();
        Self {
            segments: if segments.is_empty() {
                vec!["href".to_string()]
            } else {
                segments
            },
        }
    }

    /// The path segments, outermost first.
    pub fn segments(&self) -> &[String] {
        &self.segments
    }

    /// Whether the path addresses a top-level property.
    pub fn is_flat(&self) -> bool {
        self.segments.len() == 1
    }

    /// Read the value at this path.
    pub fn get<'a>(&self, properties: &'a Props) -> Option<&'a Value> {
        let (first, rest) = self.segments.split_first()?;
        let mut value = properties.get(first)?;
        for segment in rest {
            value = value.as_object()?.get(segment)?;
        }
        Some(value)
    }

    /// Read the value at this path as a string.
    pub fn get_str<'a>(&self, properties: &'a Props) -> Option<&'a str> {
        self.get(properties).and_then(Value::as_str)
    }

    /// Write a value, creating intermediate objects as needed.
    ///
    /// Non-object intermediate values are replaced with objects.
    pub fn set(&self, properties: &mut Props, value: Value) {
        set_in_map(properties, &self.segments, value);
    }

    /// Remove the value at this path.
    ///
    /// Intermediate objects that become empty are removed as well.
    pub fn remove(&self, properties: &mut Props) -> bool {
        remove_in_map(properties, &self.segments)
    }

    /// Build a top-level property patch that writes `value` at this path.
    ///
    /// Existing fields of an intermediate object are preserved, which keeps
    /// unrelated data (for example other `attributes`) intact through
    /// `Editor::set_node_properties` and undo.
    pub fn with_value(&self, properties: &Props, value: Value) -> Props {
        let mut patch = Props::new();
        let Some((first, rest)) = self.segments.split_first() else {
            return patch;
        };
        if rest.is_empty() {
            patch.insert(first.clone(), value);
            return patch;
        }
        let mut nested = properties
            .get(first)
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        set_in_map(&mut nested, rest, value);
        patch.insert(first.clone(), Value::Object(nested));
        patch
    }
}

impl fmt::Display for PropertyPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.segments.join("."))
    }
}

impl From<&str> for PropertyPath {
    fn from(path: &str) -> Self {
        Self::new(path)
    }
}

impl From<String> for PropertyPath {
    fn from(path: String) -> Self {
        Self::new(path)
    }
}

fn set_in_map(map: &mut Map<String, Value>, segments: &[String], value: Value) {
    let Some((first, rest)) = segments.split_first() else {
        return;
    };
    if rest.is_empty() {
        map.insert(first.clone(), value);
        return;
    }
    let entry = map
        .entry(first.clone())
        .or_insert_with(|| Value::Object(Map::new()));
    if !entry.is_object() {
        *entry = Value::Object(Map::new());
    }
    if let Some(child) = entry.as_object_mut() {
        set_in_map(child, rest, value);
    }
}

fn remove_in_map(map: &mut Map<String, Value>, segments: &[String]) -> bool {
    let Some((first, rest)) = segments.split_first() else {
        return false;
    };
    if rest.is_empty() {
        return map.remove(first).is_some();
    }
    let Some(child) = map.get_mut(first).and_then(Value::as_object_mut) else {
        return false;
    };
    let removed = remove_in_map(child, rest);
    if removed && child.is_empty() {
        map.remove(first);
    }
    removed
}

#[cfg(test)]
mod tests {
    use super::PropertyPath;
    use crate::Props;
    use serde_json::json;

    #[test]
    fn flat_paths_read_and_write_top_level_properties() {
        let path = PropertyPath::new("href");
        assert!(path.is_flat());

        let mut properties = Props::new();
        path.set(&mut properties, json!("https://example.com"));
        assert_eq!(path.get_str(&properties), Some("https://example.com"));
        assert_eq!(properties.get("href"), Some(&json!("https://example.com")));
    }

    #[test]
    fn nested_paths_create_and_remove_objects() {
        let path = PropertyPath::new("attributes.href");
        let mut properties = Props::new();
        path.set(&mut properties, json!("/docs"));
        assert_eq!(properties["attributes"]["href"], json!("/docs"));

        assert!(path.remove(&mut properties));
        assert!(properties.get("attributes").is_none());
    }

    #[test]
    fn paths_preserve_sibling_fields_in_the_same_object() {
        let path = PropertyPath::new("attributes.href");
        let mut properties = Props::new();
        properties.insert("attributes".into(), json!({ "target": "_blank" }));
        properties.insert("label".into(), json!("docs"));

        let patch = path.with_value(&properties, json!("/docs"));
        path.set(&mut properties, json!("/docs"));

        assert_eq!(
            patch.get("attributes"),
            Some(&json!({ "target": "_blank", "href": "/docs" }))
        );
        assert_eq!(properties["attributes"]["target"], json!("_blank"));
        assert_eq!(properties["attributes"]["href"], json!("/docs"));
    }

    #[test]
    fn empty_paths_fall_back_to_the_link_default() {
        assert_eq!(PropertyPath::new("").to_string(), "href");
        assert_eq!(
            PropertyPath::new("attributes..href").to_string(),
            "attributes.href"
        );
    }
}

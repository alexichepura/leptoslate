//! The document tree: elements and text nodes, mirroring Slate's JSON format.

use std::fmt;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::props::{self, Props};

/// A text node. Always a leaf.
///
/// Serializes as `{ "text": "...", ...otherProperties }`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Text {
    pub text: String,
    #[serde(flatten)]
    pub properties: Props,
}

impl Text {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            properties: Props::new(),
        }
    }

    pub fn empty() -> Self {
        Self::new("")
    }

    pub fn with_properties(mut self, properties: Props) -> Self {
        self.properties = properties;
        self
    }

    pub fn with_property(mut self, key: impl Into<String>, value: Value) -> Self {
        self.properties.insert(key.into(), value);
        self
    }

    pub fn len(&self) -> usize {
        self.text.chars().count()
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// Marks are just text node properties.
    pub fn marks(&self) -> &Props {
        &self.properties
    }
}

/// An element node with an `element_type` and arbitrary properties.
///
/// Serializes as `{ "type": "paragraph", "children": [...], ...properties }`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Element {
    #[serde(rename = "type")]
    pub element_type: String,
    #[serde(flatten)]
    pub properties: Props,
    pub children: Vec<Node>,
}

impl Element {
    pub fn new(element_type: impl Into<String>, children: Vec<Node>) -> Self {
        Self {
            element_type: element_type.into(),
            properties: Props::new(),
            children,
        }
    }

    pub fn paragraph(children: Vec<Node>) -> Self {
        Self::new("p", children)
    }

    pub fn empty(element_type: impl Into<String>) -> Self {
        Self::new(element_type, vec![Node::Text(Text::empty())])
    }

    pub fn with_properties(mut self, properties: Props) -> Self {
        self.properties = properties;
        self
    }

    pub fn with_property(mut self, key: impl Into<String>, value: Value) -> Self {
        self.properties.insert(key.into(), value);
        self
    }

    pub fn type_name(&self) -> &str {
        &self.element_type
    }

    pub fn has_children(&self) -> bool {
        !self.children.is_empty()
    }

    pub fn text_len(&self) -> usize {
        self.children.iter().map(Node::text_len).sum()
    }

    /// True if every child is a text node.
    pub fn is_text_only(&self) -> bool {
        self.children.iter().all(Node::is_text)
    }
}

/// A node in the document: either an [`Element`] or a [`Text`].
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Node {
    Element(Element),
    Text(Text),
}

impl Node {
    /// A deterministic hash of this node's type, properties and text content.
    ///
    /// Used by the view layer to key block rendering without adding persistent
    /// IDs to the document model. Hash collisions are possible in principle but
    /// vanishingly unlikely at 64 bits.
    pub fn content_hash(&self) -> u64 {
        use std::hash::Hasher;

        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.hash_content(&mut hasher);
        hasher.finish()
    }

    fn hash_content<H: std::hash::Hasher>(&self, hasher: &mut H) {
        use std::hash::Hash;

        match self {
            Node::Element(element) => {
                0u8.hash(hasher);
                element.element_type.hash(hasher);
                for (key, value) in &element.properties {
                    key.hash(hasher);
                    value.to_string().hash(hasher);
                }
                for child in &element.children {
                    child.hash_content(hasher);
                }
            }
            Node::Text(text) => {
                1u8.hash(hasher);
                text.text.hash(hasher);
                for (key, value) in &text.properties {
                    key.hash(hasher);
                    value.to_string().hash(hasher);
                }
            }
        }
    }

    pub fn text(text: impl Into<String>) -> Self {
        Node::Text(Text::new(text))
    }

    pub fn empty_text() -> Self {
        Node::Text(Text::empty())
    }

    pub fn is_text(&self) -> bool {
        matches!(self, Node::Text(_))
    }

    pub fn is_element(&self) -> bool {
        matches!(self, Node::Element(_))
    }

    pub fn as_text(&self) -> Option<&Text> {
        match self {
            Node::Text(text) => Some(text),
            Node::Element(_) => None,
        }
    }

    pub fn as_text_mut(&mut self) -> Option<&mut Text> {
        match self {
            Node::Text(text) => Some(text),
            Node::Element(_) => None,
        }
    }

    pub fn as_element(&self) -> Option<&Element> {
        match self {
            Node::Element(element) => Some(element),
            Node::Text(_) => None,
        }
    }

    pub fn as_element_mut(&mut self) -> Option<&mut Element> {
        match self {
            Node::Element(element) => Some(element),
            Node::Text(_) => None,
        }
    }

    pub fn text_string(&self) -> Option<&str> {
        self.as_text().map(|t| t.text.as_str())
    }

    pub fn text_len(&self) -> usize {
        match self {
            Node::Text(text) => text.len(),
            Node::Element(element) => element.text_len(),
        }
    }

    pub fn children(&self) -> &[Node] {
        match self {
            Node::Element(element) => &element.children,
            Node::Text(_) => &[],
        }
    }

    pub fn children_mut(&mut self) -> &mut [Node] {
        match self {
            Node::Element(element) => &mut element.children,
            Node::Text(_) => &mut [],
        }
    }

    pub fn element_type(&self) -> Option<&str> {
        self.as_element().map(|e| e.element_type.as_str())
    }

    pub fn properties(&self) -> &Props {
        match self {
            Node::Element(element) => &element.properties,
            Node::Text(text) => &text.properties,
        }
    }

    pub fn properties_mut(&mut self) -> &mut Props {
        match self {
            Node::Element(element) => &mut element.properties,
            Node::Text(text) => &mut text.properties,
        }
    }

    pub fn get_prop(&self, key: &str) -> Option<&Value> {
        self.properties().get(key)
    }

    pub fn set_prop(&mut self, key: impl Into<String>, value: Value) {
        props::set(self.properties_mut(), key, value);
    }

    /// The concatenated text of this node.
    pub fn string(&self) -> String {
        let mut out = String::new();
        self.append_string(&mut out);
        out
    }

    fn append_string(&self, out: &mut String) {
        match self {
            Node::Text(text) => out.push_str(&text.text),
            Node::Element(element) => {
                for child in &element.children {
                    child.append_string(out);
                }
            }
        }
    }

    /// Descend through the first child until a text node is reached.
    pub fn first_text(&self) -> Option<&Text> {
        match self {
            Node::Text(text) => Some(text),
            Node::Element(element) => element.children.first().and_then(Node::first_text),
        }
    }

    /// Descend through the last child until a text node is reached.
    pub fn last_text(&self) -> Option<&Text> {
        match self {
            Node::Text(text) => Some(text),
            Node::Element(element) => element.children.last().and_then(Node::last_text),
        }
    }
}

impl From<Text> for Node {
    fn from(text: Text) -> Self {
        Node::Text(text)
    }
}

impl From<Element> for Node {
    fn from(element: Element) -> Self {
        Node::Element(element)
    }
}

impl fmt::Display for Node {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Node::Text(text) => f.write_str(&text.text),
            Node::Element(element) => {
                write!(
                    f,
                    "<{}>{}</{}>",
                    element.element_type,
                    element
                        .children
                        .iter()
                        .map(Node::string)
                        .collect::<String>(),
                    element.element_type
                )
            }
        }
    }
}

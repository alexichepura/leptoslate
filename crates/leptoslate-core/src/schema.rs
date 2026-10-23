//! A small, serializable schema that describes how element types render and
//! how the normalizer treats them.
//!
//! Slate's schema lives in plugins; this is a pragmatic subset that covers the
//! common rich-text building blocks without tying the core to the DOM.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ElementRule {
    /// HTML tag used by the Leptos renderer.
    pub tag: String,
    /// Inline elements participate in the surrounding text flow.
    pub inline: bool,
    /// Void elements have no editable content (images, dividers, ...).
    pub void: bool,
}

impl ElementRule {
    pub fn block(tag: impl Into<String>) -> Self {
        Self {
            tag: tag.into(),
            inline: false,
            void: false,
        }
    }

    pub fn inline(tag: impl Into<String>) -> Self {
        Self {
            tag: tag.into(),
            inline: true,
            void: false,
        }
    }

    pub fn void(tag: impl Into<String>) -> Self {
        Self {
            tag: tag.into(),
            inline: false,
            void: true,
        }
    }

    pub fn inline_void(tag: impl Into<String>) -> Self {
        Self {
            tag: tag.into(),
            inline: true,
            void: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Schema {
    rules: HashMap<String, ElementRule>,
    /// Element type used when wrapping stray text in a block container.
    pub default_block: String,
    /// Element type used when surrounding inline nodes with text.
    pub default_inline: String,
    /// True when the tag fallback should be `<div>`.
    pub unknown_tag: String,
}

impl Default for Schema {
    fn default() -> Self {
        let schema = Self {
            rules: HashMap::new(),
            default_block: "p".into(),
            default_inline: "span".into(),
            unknown_tag: "div".into(),
        };
        schema
            .with_rule("p", ElementRule::block("p"))
            .with_rule("h1", ElementRule::block("h1"))
            .with_rule("h2", ElementRule::block("h2"))
            .with_rule("h3", ElementRule::block("h3"))
            .with_rule("h4", ElementRule::block("h4"))
            .with_rule("blockquote", ElementRule::block("blockquote"))
            .with_rule("code", ElementRule::block("pre"))
            .with_rule("hr", ElementRule::void("hr"))
            .with_rule("img", ElementRule::void("img"))
            .with_rule("inline-image", ElementRule::inline_void("img"))
            .with_rule("a", ElementRule::inline("a"))
            .with_rule("span", ElementRule::inline("span"))
            .with_rule("mention", ElementRule::inline_void("span"))
    }
}

impl Schema {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_rule(mut self, element_type: impl Into<String>, rule: ElementRule) -> Self {
        self.rules.insert(element_type.into(), rule);
        self
    }

    pub fn set_rule(&mut self, element_type: impl Into<String>, rule: ElementRule) {
        self.rules.insert(element_type.into(), rule);
    }

    pub fn rule(&self, element_type: &str) -> Option<&ElementRule> {
        self.rules.get(element_type)
    }

    pub fn tag_for(&self, element_type: &str) -> &str {
        self.rule(element_type)
            .map(|rule| rule.tag.as_str())
            .unwrap_or(self.unknown_tag.as_str())
    }

    pub fn is_inline(&self, element_type: &str) -> bool {
        self.rule(element_type).map(|r| r.inline).unwrap_or(false)
    }

    pub fn is_void(&self, element_type: &str) -> bool {
        self.rule(element_type).map(|r| r.void).unwrap_or(false)
    }

    pub fn is_list(&self, element_type: &str) -> bool {
        matches!(element_type, "ul" | "ol")
    }

    pub fn is_table(&self, element_type: &str) -> bool {
        element_type == "table"
    }

    pub fn is_table_section(&self, element_type: &str) -> bool {
        matches!(element_type, "thead" | "tbody" | "tfoot")
    }

    pub fn is_table_row(&self, element_type: &str) -> bool {
        element_type == "tr"
    }

    pub fn is_table_cell(&self, element_type: &str) -> bool {
        matches!(element_type, "th" | "td")
    }

    pub fn is_table_element(&self, element_type: &str) -> bool {
        self.is_table(element_type)
            || self.is_table_section(element_type)
            || self.is_table_row(element_type)
            || self.is_table_cell(element_type)
    }

    pub fn default_block_rule(&self) -> ElementRule {
        self.rule(&self.default_block)
            .cloned()
            .unwrap_or_else(|| ElementRule::block("p"))
    }
}

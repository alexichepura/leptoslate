//! Configurable property keys for the built-in text marks.
//!
//! Marks are ordinary text-node properties, so an application can store bold
//! text under `strong`, italic under `em`, and so on. [`MarkSet`] maps the five
//! built-in mark kinds to their property keys and is used by paste, keyboard
//! shortcuts, `beforeinput` formatting, the selection bubble and the default
//! mark styles.
//!
//! Defaults match the conventional property names:
//!
//! ```rust
//! use leptoslate_core::{MarkKind, MarkSet};
//!
//! let marks = MarkSet::new();
//! assert_eq!(marks.key(MarkKind::Bold), "bold");
//! assert_eq!(marks.resolve("bold"), Some("bold"));
//!
//! let custom = MarkSet::new()
//!     .bold("strong")
//!     .italic("em")
//!     .underline("u");
//! assert_eq!(custom.key(MarkKind::Bold), "strong");
//! // Conventional names remain accepted on input and resolve to the
//! // configured key, so documents from other sources import cleanly.
//! assert_eq!(custom.resolve("bold"), Some("strong"));
//! ```

use std::collections::BTreeMap;

/// One of the built-in text marks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MarkKind {
    Bold,
    Italic,
    Underline,
    Strikethrough,
    Code,
}

impl MarkKind {
    /// Every built-in mark kind, in presentation order.
    pub const ALL: [Self; 5] = [
        Self::Bold,
        Self::Italic,
        Self::Underline,
        Self::Strikethrough,
        Self::Code,
    ];

    /// The default property key for this mark.
    pub const fn default_key(self) -> &'static str {
        match self {
            Self::Bold => "bold",
            Self::Italic => "italic",
            Self::Underline => "underline",
            Self::Strikethrough => "strikethrough",
            Self::Code => "code",
        }
    }

    /// Resolve a conventional property name to its mark kind.
    pub fn from_default_name(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|kind| kind.default_key() == name)
    }
}

/// Property keys for the five built-in marks, plus optional input aliases.
///
/// The set is configuration only: the document model keeps using arbitrary
/// text properties, so `toggle_mark` and custom renderers are unaffected.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MarkSet {
    bold: String,
    italic: String,
    underline: String,
    strikethrough: String,
    code: String,
    aliases: BTreeMap<String, MarkKind>,
}

impl Default for MarkSet {
    fn default() -> Self {
        Self {
            bold: MarkKind::Bold.default_key().to_string(),
            italic: MarkKind::Italic.default_key().to_string(),
            underline: MarkKind::Underline.default_key().to_string(),
            strikethrough: MarkKind::Strikethrough.default_key().to_string(),
            code: MarkKind::Code.default_key().to_string(),
            aliases: BTreeMap::new(),
        }
    }
}

impl MarkSet {
    /// The conventional `bold` / `italic` / `underline` / `strikethrough` /
    /// `code` keys.
    pub fn new() -> Self {
        Self::default()
    }

    /// Use a different property key for bold.
    pub fn bold(mut self, key: impl Into<String>) -> Self {
        set_key(&mut self.bold, key.into());
        self
    }

    /// Use a different property key for italic.
    pub fn italic(mut self, key: impl Into<String>) -> Self {
        set_key(&mut self.italic, key.into());
        self
    }

    /// Use a different property key for underline.
    pub fn underline(mut self, key: impl Into<String>) -> Self {
        set_key(&mut self.underline, key.into());
        self
    }

    /// Use a different property key for strikethrough.
    pub fn strikethrough(mut self, key: impl Into<String>) -> Self {
        set_key(&mut self.strikethrough, key.into());
        self
    }

    /// Use a different property key for inline code.
    pub fn code(mut self, key: impl Into<String>) -> Self {
        set_key(&mut self.code, key.into());
        self
    }

    /// Accept an additional input name for a mark kind.
    ///
    /// Conventional names are always accepted. Aliases are useful when pasted
    /// HTML or stored documents use another convention, such as `strong` for
    /// bold.
    pub fn alias(mut self, name: impl Into<String>, kind: MarkKind) -> Self {
        let name = name.into();
        if !name.is_empty() {
            self.aliases.insert(name, kind);
        }
        self
    }

    /// The configured property key for a mark kind.
    pub fn key(&self, kind: MarkKind) -> &str {
        match kind {
            MarkKind::Bold => &self.bold,
            MarkKind::Italic => &self.italic,
            MarkKind::Underline => &self.underline,
            MarkKind::Strikethrough => &self.strikethrough,
            MarkKind::Code => &self.code,
        }
    }

    /// The configured bold key.
    pub fn bold_key(&self) -> &str {
        &self.bold
    }

    /// The configured italic key.
    pub fn italic_key(&self) -> &str {
        &self.italic
    }

    /// The configured underline key.
    pub fn underline_key(&self) -> &str {
        &self.underline
    }

    /// The configured strikethrough key.
    pub fn strikethrough_key(&self) -> &str {
        &self.strikethrough
    }

    /// The configured inline-code key.
    pub fn code_key(&self) -> &str {
        &self.code
    }

    /// All configured keys, in presentation order.
    pub fn keys(&self) -> [&str; 5] {
        [
            &self.bold,
            &self.italic,
            &self.underline,
            &self.strikethrough,
            &self.code,
        ]
    }

    /// Map an input property name to the configured key for its mark kind.
    ///
    /// Explicit aliases win, followed by the configured keys and then the
    /// conventional default names.
    pub fn resolve(&self, name: &str) -> Option<&str> {
        if let Some(kind) = self.aliases.get(name) {
            return Some(self.key(*kind));
        }
        for kind in MarkKind::ALL {
            if self.key(kind) == name {
                return Some(self.key(kind));
            }
        }
        MarkKind::from_default_name(name).map(|kind| self.key(kind))
    }

    /// Whether `name` is an accepted input name for `kind`.
    pub fn accepts(&self, name: &str, kind: MarkKind) -> bool {
        self.resolve(name) == Some(self.key(kind))
    }
}

fn set_key(target: &mut String, key: String) {
    if !key.is_empty() {
        *target = key;
    }
}

#[cfg(test)]
mod tests {
    use super::{MarkKind, MarkSet};

    #[test]
    fn defaults_match_the_conventional_keys() {
        let marks = MarkSet::new();
        assert_eq!(
            marks.keys(),
            ["bold", "italic", "underline", "strikethrough", "code"]
        );
        for kind in MarkKind::ALL {
            assert_eq!(marks.resolve(kind.default_key()), Some(marks.key(kind)));
        }
    }

    #[test]
    fn custom_keys_are_used_for_output_and_defaults_remain_input_aliases() {
        let marks = MarkSet::new()
            .bold("strong")
            .italic("em")
            .underline("u")
            .strikethrough("del")
            .code("mono");

        assert_eq!(marks.bold_key(), "strong");
        assert_eq!(marks.code_key(), "mono");
        assert_eq!(marks.resolve("bold"), Some("strong"));
        assert_eq!(marks.resolve("italic"), Some("em"));
        assert_eq!(marks.resolve("code"), Some("mono"));
        assert_eq!(marks.resolve("strong"), Some("strong"));
        assert!(marks.accepts("bold", MarkKind::Bold));
        assert!(!marks.accepts("italic", MarkKind::Bold));
    }

    #[test]
    fn explicit_aliases_extend_input_names() {
        let marks = MarkSet::new()
            .bold("strong")
            .alias("heavy", MarkKind::Bold)
            .alias("slanted", MarkKind::Italic);

        assert_eq!(marks.resolve("heavy"), Some("strong"));
        assert_eq!(marks.resolve("slanted"), Some("italic"));
        assert_eq!(marks.resolve("unknown"), None);
    }

    #[test]
    fn empty_keys_and_aliases_keep_the_defaults() {
        let marks = MarkSet::new().bold("").alias("", MarkKind::Italic);
        assert_eq!(marks.bold_key(), "bold");
        assert_eq!(marks.italic_key(), "italic");
    }
}

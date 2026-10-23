//! HTML/Markdown parsing and insertion helpers for the rich preset.

use leptoslate::{Editor, EditorHandle};

/// Parse and insert rich fragments through the rich preset parser.
///
/// The core editor only inserts already-parsed [`leptoslate::Node`] fragments;
/// HTML and Markdown parsing is owned by this crate.
pub trait PasteTransforms {
    /// Parse and insert an HTML fragment.
    fn insert_html(&mut self, html: &str);

    /// Parse and insert a Markdown fragment.
    fn insert_markdown(&mut self, markdown: &str);
}

impl PasteTransforms for Editor {
    fn insert_html(&mut self, html: &str) {
        let nodes = crate::fragment::parse_html(html);
        self.insert_fragment(nodes);
    }

    fn insert_markdown(&mut self, markdown: &str) {
        let nodes = crate::fragment::parse_markdown(markdown);
        self.insert_fragment(nodes);
    }
}

/// Parsing/insertion helpers on the reactive editor handle.
pub trait PasteHandle {
    /// Parse and insert an HTML fragment.
    fn insert_html(&self, html: &str);

    /// Parse and insert a Markdown fragment.
    fn insert_markdown(&self, markdown: &str);
}

impl PasteHandle for EditorHandle {
    fn insert_html(&self, html: &str) {
        self.update(|editor| editor.insert_html(html));
    }

    fn insert_markdown(&self, markdown: &str) {
        self.update(|editor| editor.insert_markdown(markdown));
    }
}

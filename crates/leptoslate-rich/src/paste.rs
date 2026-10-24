//! HTML/Markdown parsing and insertion helpers for the rich preset.

use leptoslate::{Editor, EditorHandle, MarkSet};

use crate::fragment::FragmentConfig;

/// Parse and insert rich fragments through the rich preset parser.
///
/// The core editor only inserts already-parsed [`leptoslate::Node`] fragments;
/// HTML and Markdown parsing is owned by this crate.
pub trait PasteTransforms {
    /// Parse and insert an HTML fragment using the default mark keys.
    fn insert_html(&mut self, html: &str);

    /// Parse and insert a Markdown fragment using the default mark keys.
    fn insert_markdown(&mut self, markdown: &str);

    /// Parse and insert an HTML fragment with configured mark keys.
    fn insert_html_with_marks(&mut self, html: &str, marks: &MarkSet);

    /// Parse and insert a Markdown fragment with configured mark keys.
    fn insert_markdown_with_marks(&mut self, markdown: &str, marks: &MarkSet);

    /// Parse and insert an HTML fragment with explicit parser configuration.
    fn insert_html_with_config(&mut self, html: &str, config: &FragmentConfig);

    /// Parse and insert a Markdown fragment with explicit parser configuration.
    fn insert_markdown_with_config(&mut self, markdown: &str, config: &FragmentConfig);
}

impl PasteTransforms for Editor {
    fn insert_html(&mut self, html: &str) {
        self.insert_html_with_config(html, &FragmentConfig::default());
    }

    fn insert_markdown(&mut self, markdown: &str) {
        self.insert_markdown_with_config(markdown, &FragmentConfig::default());
    }

    fn insert_html_with_marks(&mut self, html: &str, marks: &MarkSet) {
        self.insert_html_with_config(
            html,
            &FragmentConfig {
                marks: marks.clone(),
                ..FragmentConfig::default()
            },
        );
    }

    fn insert_markdown_with_marks(&mut self, markdown: &str, marks: &MarkSet) {
        self.insert_markdown_with_config(
            markdown,
            &FragmentConfig {
                marks: marks.clone(),
                ..FragmentConfig::default()
            },
        );
    }

    fn insert_html_with_config(&mut self, html: &str, config: &FragmentConfig) {
        let nodes = crate::fragment::parse_html_with_config(html, config);
        self.insert_fragment(nodes);
    }

    fn insert_markdown_with_config(&mut self, markdown: &str, config: &FragmentConfig) {
        let nodes = crate::fragment::parse_markdown_with_config(markdown, config);
        self.insert_fragment(nodes);
    }
}

/// Parsing/insertion helpers on the reactive editor handle.
pub trait PasteHandle {
    /// Parse and insert an HTML fragment using the default mark keys.
    fn insert_html(&self, html: &str);

    /// Parse and insert a Markdown fragment using the default mark keys.
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

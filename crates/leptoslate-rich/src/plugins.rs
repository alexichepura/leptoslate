//! Rich paste and clipboard behavior for the `leptoslate` binding.

use leptos::prelude::WithUntracked;
use leptoslate::{EditablePlugin, EditorHandle, MarkSet, PropertyPath};
use web_sys::{ClipboardEvent, DragEvent, Element};

const MODEL_CLIPBOARD_MIME: &str = "application/x-leptoslate+json";

/// Parses rich clipboard HTML and Markdown before falling back to plain text.
pub struct RichPastePlugin {
    pub html: bool,
    pub markdown: bool,
    /// Property keys written for bold, italic and the other built-in marks.
    pub marks: MarkSet,
    /// Dotted property path used to store link targets (`href` by default).
    pub link_property: PropertyPath,
}

impl RichPastePlugin {
    pub fn new(html: bool, markdown: bool) -> Self {
        Self {
            html,
            markdown,
            marks: MarkSet::default(),
            link_property: PropertyPath::default(),
        }
    }

    /// Write parsed marks using the configured property keys.
    pub fn with_marks(mut self, marks: MarkSet) -> Self {
        self.marks = marks;
        self
    }

    /// Store parsed links at the configured dotted property path.
    pub fn with_link_property(mut self, link_property: PropertyPath) -> Self {
        self.link_property = link_property;
        self
    }

    fn config(&self) -> crate::fragment::FragmentConfig {
        crate::fragment::FragmentConfig {
            marks: self.marks.clone(),
            link_property: self.link_property.clone(),
        }
    }
}

impl EditablePlugin for RichPastePlugin {
    fn on_paste(&self, editor: &EditorHandle, _root: &Element, event: &ClipboardEvent) -> bool {
        let Some(data) = event.clipboard_data() else {
            return false;
        };
        if data
            .files()
            .map(|files| files.length() > 0)
            .unwrap_or(false)
        {
            return false;
        }

        if self.html {
            let html = data.get_data("text/html").unwrap_or_default();
            if !html.trim().is_empty() {
                if let Some(model) = extract_model_marker(&html) {
                    if let Ok(nodes) = serde_json::from_str::<Vec<leptoslate::Node>>(&model) {
                        if !nodes.is_empty() {
                            editor.insert_fragment(nodes);
                            return true;
                        }
                    }
                }
                let nodes = crate::fragment::parse_html_with_config(&html, &self.config());
                if !nodes.is_empty() {
                    editor.insert_fragment(nodes);
                    return true;
                }
            }
        }

        if self.markdown {
            let text = data.get_data("text/plain").unwrap_or_default();
            if !text.is_empty() {
                let nodes = crate::fragment::parse_markdown_with_config(&text, &self.config());
                if !nodes.is_empty() {
                    editor.insert_fragment(nodes);
                    return true;
                }
            }
        }
        false
    }

    fn on_drop(&self, editor: &EditorHandle, _root: &Element, event: &DragEvent) -> bool {
        if !self.html {
            return false;
        }
        let Some(data) = event.data_transfer() else {
            return false;
        };
        let html = data.get_data("text/html").unwrap_or_default();
        if html.trim().is_empty() {
            return false;
        }
        let nodes = crate::fragment::parse_html_with_config(&html, &self.config());
        if nodes.is_empty() {
            return false;
        }
        editor.insert_fragment(nodes);
        true
    }
}

/// Copies rich block HTML and a lossless model payload for node selections.
pub struct RichClipboardPlugin;

impl EditablePlugin for RichClipboardPlugin {
    fn on_copy(&self, editor: &EditorHandle, root: &Element, event: &ClipboardEvent) -> bool {
        write_clipboard(editor, root, event)
    }

    fn on_cut(&self, editor: &EditorHandle, root: &Element, event: &ClipboardEvent) -> bool {
        if !editor.has_node_selection() {
            return false;
        }
        if !write_clipboard(editor, root, event) {
            return false;
        }
        editor.delete_node_selection();
        true
    }
}

fn write_clipboard(editor: &EditorHandle, root: &Element, event: &ClipboardEvent) -> bool {
    if !editor.has_node_selection() {
        return false;
    }
    let text = editor.node_selection_string().unwrap_or_default();
    let html = selection_html(editor, root);
    let model = selection_model_json(editor);
    let html = match (&html, &model) {
        (Some(html), Some(model)) => Some(model_marker_html(model, html)),
        (Some(html), None) => Some(html.clone()),
        (None, Some(model)) => Some(model_marker_html(model, "")),
        (None, None) => None,
    };
    let Some(data) = event.clipboard_data() else {
        return false;
    };
    let _ = data.set_data("text/plain", &text);
    if let Some(html) = &html {
        let _ = data.set_data("text/html", html);
    }
    if let Some(model) = &model {
        let _ = data.set_data(MODEL_CLIPBOARD_MIME, model);
    }
    true
}

fn selection_html(editor: &EditorHandle, root: &Element) -> Option<String> {
    let paths = editor.state.with_untracked(|state| state.node_selections());
    if paths.is_empty() {
        return None;
    }
    let html = paths
        .iter()
        .filter_map(|path| {
            root.query_selector(&format!("[data-node-path=\"{}\"]", path_string(path)))
                .ok()
                .flatten()
        })
        .map(|element| element.outer_html())
        .collect::<Vec<_>>()
        .join("");
    (!html.trim().is_empty()).then_some(html)
}

fn selection_model_json(editor: &EditorHandle) -> Option<String> {
    let nodes = editor.state.with_untracked(|state| {
        state
            .node_selections()
            .iter()
            .filter_map(|path| state.node(path).cloned())
            .collect::<Vec<_>>()
    });
    (!nodes.is_empty())
        .then(|| serde_json::to_string(&nodes).ok())
        .flatten()
}

fn model_marker_html(model: &str, html: &str) -> String {
    format!(
        "<div data-leptoslate-model=\"{}\">{}</div>",
        hex_encode(model.as_bytes()),
        html
    )
}

fn extract_model_marker(html: &str) -> Option<String> {
    let marker = "data-leptoslate-model=\"";
    let start = html.find(marker)? + marker.len();
    let end = html[start..].find('"')? + start;
    let bytes = hex_decode(&html[start..end])?;
    String::from_utf8(bytes).ok()
}

fn path_string(path: &[usize]) -> String {
    path.iter()
        .map(usize::to_string)
        .collect::<Vec<_>>()
        .join(".")
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0xf) as usize] as char);
    }
    out
}

fn hex_decode(value: &str) -> Option<Vec<u8>> {
    if !value.len().is_multiple_of(2) {
        return None;
    }
    value
        .as_bytes()
        .chunks(2)
        .map(|pair| {
            let high = hex_digit(pair[0])?;
            let low = hex_digit(pair[1])?;
            Some((high << 4) | low)
        })
        .collect()
}

fn hex_digit(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_marker_round_trips_non_ascii_payloads() {
        let model = r#"[{"type":"callout","tone":"warning","children":[{"text":"Привет"}]}]"#;
        let html = model_marker_html(model, "<aside>Привет</aside>");
        assert!(html.contains("data-leptoslate-model"));
        assert_eq!(extract_model_marker(&html).as_deref(), Some(model));
    }
}

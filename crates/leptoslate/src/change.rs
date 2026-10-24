//! Document change notifications for [`Editable`](crate::Editable).
//!
//! The [`Editable`](crate::Editable) component accepts an optional
//! `on_change` callback that receives the document as Slate-shaped JSON. It is
//! only invoked when the document content changes: selection-only, focus-only
//! and active-mark updates are ignored.
//!
//! ```rust,ignore
//! use std::time::Duration;
//!
//! let on_change = Callback::new(move |json: String| {
//!     // Persist, validate or preview the current document.
//! });
//!
//! view! {
//!     <Editable
//!         editor=editor
//!         on_change=on_change
//!         change_debounce=Duration::from_millis(300)
//!     />
//! }
//! ```
//!
//! The callback is not invoked for the initial document. When a debounce is
//! configured, changes reset the timer and only the latest document is
//! delivered. A pending update is flushed when the editor is unmounted.

use std::{
    hash::{Hash, Hasher},
    time::Duration,
};

use leptos::prelude::*;
use leptoslate_core::Editor;
use send_wrapper::SendWrapper;
use wasm_bindgen::{closure::Closure, JsCast};

/// A stable fingerprint of a document's content.
///
/// Selection, node selection, active marks, history and other editor state are
/// excluded. Two states with the same rendered document have the same
/// fingerprint, which is what change notifications use to ignore
/// selection-only updates.
pub fn document_fingerprint(editor: &Editor) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    editor.children.len().hash(&mut hasher);
    for child in &editor.children {
        child.content_hash().hash(&mut hasher);
    }
    hasher.finish()
}

type ChangeTimer = StoredValue<Option<SendWrapper<Closure<dyn FnMut()>>>>;

/// Emits the document JSON to a callback, optionally after a debounce delay.
///
/// Timers are stored through [`StoredValue`] so the emitter can live inside a
/// component closure and be shared with the timer callback.
#[derive(Clone)]
pub(crate) struct ChangeEmitter {
    editor: crate::EditorHandle,
    callback: Callback<String>,
    debounce: Option<Duration>,
    timeout: StoredValue<Option<i32>>,
    timer: ChangeTimer,
}

impl ChangeEmitter {
    pub(crate) fn new(
        editor: crate::EditorHandle,
        callback: Callback<String>,
        debounce: Option<Duration>,
    ) -> Self {
        Self {
            editor,
            callback,
            debounce,
            timeout: StoredValue::new(None),
            timer: StoredValue::new(None),
        }
    }

    /// Report a content change from the editor.
    ///
    /// With no debounce (or a zero debounce) the document is serialized and
    /// delivered immediately on the next microtask-free call. Otherwise the
    /// timer is reset and the latest document is delivered once it fires.
    pub(crate) fn notify(&self) {
        match self.debounce {
            Some(delay) if !delay.is_zero() => self.schedule(delay),
            _ => self.emit(),
        }
    }

    /// Deliver a pending debounced update immediately, if there is one.
    pub(crate) fn flush(&self) {
        if self.timeout.get_value().is_some() {
            self.emit();
        }
    }

    fn emit(&self) {
        self.cancel();
        let json = self.editor.state.with_untracked(Editor::to_json);
        self.callback.run(json);
    }

    fn schedule(&self, delay: Duration) {
        self.cancel();

        let editor = self.editor.clone();
        let callback = self.callback;
        let timeout = self.timeout;
        let closure = Closure::<dyn FnMut()>::new(move || {
            timeout.set_value(None);
            let json = editor.state.with_untracked(Editor::to_json);
            callback.run(json);
        });

        let Some(window) = web_sys::window() else {
            let json = self.editor.state.with_untracked(Editor::to_json);
            self.callback.run(json);
            return;
        };
        let milliseconds = delay.as_millis().min(i32::MAX as u128) as i32;
        match window.set_timeout_with_callback_and_timeout_and_arguments_0(
            closure.as_ref().unchecked_ref(),
            milliseconds,
        ) {
            Ok(handle) => {
                self.timeout.set_value(Some(handle));
                // Keep the closure alive until the timer fires, is reset or the
                // editor is unmounted.
                self.timer.set_value(Some(SendWrapper::new(closure)));
            }
            Err(_) => {
                let json = self.editor.state.with_untracked(Editor::to_json);
                self.callback.run(json);
            }
        }
    }

    pub(crate) fn cancel(&self) {
        if let Some(handle) = self.timeout.get_value() {
            if let Some(window) = web_sys::window() {
                window.clear_timeout_with_handle(handle);
            }
        }
        self.timeout.set_value(None);
        self.timer.set_value(None);
    }
}

#[cfg(test)]
mod tests {
    use super::document_fingerprint;
    use leptoslate_core::{Editor, Element, Node, Point, Range, Text};

    fn paragraph(text: &str) -> Node {
        Node::Element(Element::paragraph(vec![Node::Text(Text::new(text))]))
    }

    #[test]
    fn fingerprint_ignores_selection_and_active_marks() {
        let mut editor = Editor::new(vec![paragraph("hello world")]);
        let fingerprint = document_fingerprint(&editor);

        // A collapsed caret and pending active marks are selection state, not
        // document content.
        editor.toggle_mark("bold", serde_json::json!(true));
        assert_eq!(document_fingerprint(&editor), fingerprint);

        editor.select(Range::new(
            Point::new(vec![0, 0], 0),
            Point::new(vec![0, 0], 5),
        ));
        assert_eq!(document_fingerprint(&editor), fingerprint);

        editor.deselect();
        assert_eq!(document_fingerprint(&editor), fingerprint);
    }

    #[test]
    fn fingerprint_changes_for_text_and_formatting() {
        let mut editor = Editor::new(vec![paragraph("hello")]);
        let fingerprint = document_fingerprint(&editor);

        editor.insert_text("!");
        let after_text = document_fingerprint(&editor);
        assert_ne!(after_text, fingerprint);

        editor.select_all();
        editor.toggle_mark("italic", serde_json::json!(true));
        assert_ne!(document_fingerprint(&editor), after_text);
    }

    #[test]
    fn fingerprint_sees_mark_changes_on_text_leaves() {
        let mut editor = Editor::new(vec![paragraph("hello")]);
        let fingerprint = document_fingerprint(&editor);

        if let Some(text) = editor.node_mut(&[0, 0]).and_then(Node::as_text_mut) {
            text.properties
                .insert("code".into(), serde_json::json!(true));
        }
        assert_ne!(document_fingerprint(&editor), fingerprint);
    }
}

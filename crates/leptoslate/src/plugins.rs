//! Composable editor behavior hooks.
//!
//! Plugins run before the built-in `Editable` handlers. Returning `true` means
//! the event was handled and the built-in path is skipped. Plugins can call the
//! [`EditorHandle`] like any other toolbar or application code.
//!
//! Slate plugins are JavaScript functions that override editor methods; they
//! are not binary-compatible with this API and must be ported to Rust.

use std::sync::Arc;

use crate::handle::EditorHandle;
use web_sys::{ClipboardEvent, DragEvent, InputEvent, KeyboardEvent};

/// Optional event handlers for the editable surface.
///
/// All methods default to `false`, so implement only the events you need.
pub trait EditablePlugin: Send + Sync {
    fn on_keydown(&self, _editor: &EditorHandle, _event: &KeyboardEvent) -> bool {
        false
    }

    fn on_beforeinput(&self, _editor: &EditorHandle, _event: &InputEvent) -> bool {
        false
    }

    fn on_paste(&self, _editor: &EditorHandle, _event: &ClipboardEvent) -> bool {
        false
    }

    fn on_copy(&self, _editor: &EditorHandle, _event: &ClipboardEvent) -> bool {
        false
    }

    fn on_cut(&self, _editor: &EditorHandle, _event: &ClipboardEvent) -> bool {
        false
    }

    fn on_drop(&self, _editor: &EditorHandle, _event: &DragEvent) -> bool {
        false
    }
}

/// An ordered collection of [`EditablePlugin`] values.
#[derive(Clone, Default)]
pub struct EditablePlugins {
    plugins: Arc<Vec<Arc<dyn EditablePlugin>>>,
}

impl EditablePlugins {
    pub fn new() -> Self {
        Self::default()
    }

    /// Append a plugin. Earlier plugins have higher priority.
    pub fn with<P>(mut self, plugin: P) -> Self
    where
        P: EditablePlugin + 'static,
    {
        Arc::make_mut(&mut self.plugins).push(Arc::new(plugin));
        self
    }

    /// Append an already shared plugin.
    pub fn with_arc(mut self, plugin: Arc<dyn EditablePlugin>) -> Self {
        Arc::make_mut(&mut self.plugins).push(plugin);
        self
    }

    pub fn push<P>(&mut self, plugin: P)
    where
        P: EditablePlugin + 'static,
    {
        Arc::make_mut(&mut self.plugins).push(Arc::new(plugin));
    }

    pub fn is_empty(&self) -> bool {
        self.plugins.is_empty()
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = &Arc<dyn EditablePlugin>> {
        self.plugins.iter()
    }
}

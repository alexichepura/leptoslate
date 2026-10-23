//! List keyboard behavior for the rich preset.

use leptoslate::{EditablePlugin, EditorHandle};
use web_sys::KeyboardEvent;

/// Tab/Shift+Tab list indentation.
pub struct RichListPlugin;

impl EditablePlugin for RichListPlugin {
    fn on_keydown(&self, editor: &EditorHandle, event: &KeyboardEvent) -> bool {
        if event.key() != "Tab" || event.meta_key() || event.ctrl_key() || event.alt_key() {
            return false;
        }
        if event.shift_key() {
            editor.outdent_list()
        } else {
            editor.indent_list()
        }
    }
}

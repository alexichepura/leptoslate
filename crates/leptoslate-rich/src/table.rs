//! Table keyboard behavior for the rich preset.

use leptoslate::{EditablePlugin, EditorHandle};
use web_sys::{InputEvent, KeyboardEvent};

/// Prevents structural editing inside table cells and supports cell navigation.
pub struct RichTablePlugin;

impl EditablePlugin for RichTablePlugin {
    fn on_keydown(&self, editor: &EditorHandle, event: &KeyboardEvent) -> bool {
        if !editor.in_table_cell() {
            return false;
        }
        match event.key().as_str() {
            "Enter" => true,
            "Tab" if !event.meta_key() && !event.ctrl_key() && !event.alt_key() => {
                editor.move_table_cell(if event.shift_key() { -1 } else { 1 });
                true
            }
            "Backspace" => editor.at_table_cell_start(),
            "Delete" => editor.at_table_cell_end(),
            _ => false,
        }
    }

    fn on_beforeinput(&self, editor: &EditorHandle, event: &InputEvent) -> bool {
        editor.in_table_cell()
            && matches!(
                event.input_type().as_str(),
                "insertParagraph" | "insertLineBreak"
            )
    }
}

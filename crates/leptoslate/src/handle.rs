//! The reactive editor handle passed to [`crate::Editable`].

use std::sync::atomic::{AtomicU64, Ordering};

use leptos::prelude::*;
use leptoslate_core::{Editor, Node, Path, Point, Props};
use serde_json::Value;

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

/// A cheap, cloneable handle to an editor value.
///
/// The document lives in a `RwSignal`, so reading and mutating it integrates
/// with Leptos reactivity. All mutation helpers run one undoable transaction.
#[derive(Clone)]
pub struct EditorHandle {
    pub state: RwSignal<Editor>,
    pub id: String,
}

impl EditorHandle {
    /// Create a handle from a Slate-shaped document.
    pub fn new(children: Vec<Node>) -> Self {
        Self::from_editor(Editor::new(children))
    }

    /// Create a handle from an existing editor (including its schema/history).
    pub fn from_editor(editor: Editor) -> Self {
        let id = format!("ls-editor-{}", NEXT_ID.fetch_add(1, Ordering::Relaxed));
        Self {
            state: RwSignal::new(editor),
            id,
        }
    }

    /// Create an empty document with a single paragraph.
    pub fn empty() -> Self {
        Self::from_editor(Editor::empty())
    }

    pub fn read<R>(&self, f: impl FnOnce(&Editor) -> R) -> R {
        self.state.with(f)
    }

    pub fn update<R>(&self, f: impl FnOnce(&mut Editor) -> R) -> R {
        self.state
            .try_update(f)
            .unwrap_or_else(|| panic!("editor signal has been disposed"))
    }

    /// A snapshot of the current editor state.
    pub fn snapshot(&self) -> Editor {
        self.state.get_untracked()
    }

    /// Replace the document. History is reset and the caret moves to the start.
    pub fn set_value(&self, children: Vec<Node>) {
        let schema = self.state.with_untracked(|editor| editor.schema.clone());
        let mut editor = Editor::new(children);
        editor.set_schema(schema);
        self.state.set(editor);
    }

    pub fn set_json(&self, json: &str) -> Result<(), serde_json::Error> {
        let children = serde_json::from_str::<Vec<Node>>(json)?;
        self.set_value(children);
        Ok(())
    }

    pub fn to_json(&self) -> String {
        self.state.with_untracked(|editor| editor.to_json())
    }

    pub fn to_json_pretty(&self) -> String {
        self.state.with_untracked(|editor| editor.to_json_pretty())
    }

    pub fn document_string(&self) -> String {
        self.state.with_untracked(|editor| editor.document_string())
    }

    pub fn is_empty(&self) -> bool {
        self.state.with_untracked(|editor| editor.is_empty())
    }

    pub fn word_count(&self) -> usize {
        self.state.with_untracked(|editor| {
            editor
                .document_string()
                .split_whitespace()
                .filter(|word| !word.is_empty())
                .count()
        })
    }

    pub fn selection_string(&self) -> Option<String> {
        self.state
            .with_untracked(|editor| editor.selection_string())
    }

    // -----------------------------------------------------------------
    // Editing API
    // -----------------------------------------------------------------

    pub fn insert_text(&self, text: &str) {
        self.update(|editor| editor.insert_text(text));
    }

    pub fn insert_plain_text(&self, text: &str) {
        self.update(|editor| editor.insert_plain_text(text));
    }

    pub fn insert_break(&self) {
        self.update(Editor::insert_break);
    }

    pub fn insert_soft_break(&self) {
        self.update(Editor::insert_soft_break);
    }

    pub fn delete_selection(&self) {
        self.update(Editor::delete_selection);
    }

    pub fn delete_backward(&self) {
        self.update(Editor::delete_backward);
    }

    pub fn delete_word_backward(&self) {
        self.update(Editor::delete_word_backward);
    }

    pub fn delete_forward(&self) {
        self.update(Editor::delete_forward);
    }

    pub fn delete_word_forward(&self) {
        self.update(Editor::delete_word_forward);
    }

    pub fn toggle_mark(&self, key: &str, value: Value) {
        self.update(|editor| editor.toggle_mark(key, value));
    }

    pub fn add_mark(&self, key: &str, value: Value) {
        self.update(|editor| editor.add_mark(key, value));
    }

    pub fn remove_mark(&self, key: &str) {
        self.update(|editor| editor.remove_mark(key));
    }

    pub fn set_block(&self, element_type: &str) {
        self.update(|editor| editor.set_block(element_type));
    }

    /// Wrap the current selection in a link, or update the link at the caret.
    pub fn set_link(&self, href: &str) -> bool {
        self.update(|editor| editor.set_link(href))
    }

    /// Remove links intersecting the current selection.
    pub fn remove_link(&self) -> bool {
        self.update(Editor::remove_link)
    }

    /// The href of the link at the current selection, if any.
    pub fn link_at_selection(&self) -> Option<String> {
        self.state
            .with_untracked(|editor| editor.link_at_selection().map(|(_, href)| href))
    }

    /// The node path and href of the link at the current selection.
    pub fn link_node_at_selection(&self) -> Option<(Path, String)> {
        self.state.with_untracked(Editor::link_at_selection)
    }

    /// Update the href and text of the link at the current selection.
    pub fn update_link_at_selection(&self, href: &str, text: &str) -> bool {
        self.update(|editor| editor.update_link_at_selection(href, text))
    }

    /// Whether the caret is inside a table cell.
    pub fn in_table_cell(&self) -> bool {
        self.state
            .with_untracked(Editor::table_cell_at_selection)
            .is_some()
    }

    /// Move to the previous/next table cell.
    pub fn move_table_cell(&self, direction: isize) -> bool {
        self.update(|editor| editor.move_table_cell(direction))
    }

    /// Whether a collapsed caret is at the start of its table cell.
    pub fn at_table_cell_start(&self) -> bool {
        self.state.with_untracked(|editor| {
            let Some(range) = editor.selection.as_ref() else {
                return false;
            };
            range.is_collapsed() && editor.at_table_cell_start(range.start())
        })
    }

    /// Whether a collapsed caret is at the end of its table cell.
    pub fn at_table_cell_end(&self) -> bool {
        self.state.with_untracked(|editor| {
            let Some(range) = editor.selection.as_ref() else {
                return false;
            };
            range.is_collapsed() && editor.at_table_cell_end(range.start())
        })
    }

    /// Insert a table after the current selection.
    pub fn insert_table(&self, rows: usize, cols: usize, header: bool) -> Option<Path> {
        self.update(|editor| editor.insert_table(rows, cols, header))
    }

    pub fn insert_table_row_after(&self) -> bool {
        self.update(Editor::insert_table_row_after)
    }

    pub fn delete_table_row(&self) -> bool {
        self.update(Editor::delete_table_row)
    }

    pub fn insert_table_column_after(&self) -> bool {
        self.update(Editor::insert_table_column_after)
    }

    pub fn delete_table_column(&self) -> bool {
        self.update(Editor::delete_table_column)
    }

    /// Insert an inline node (mention, embed, …) at the caret.
    pub fn insert_inline_node(&self, node: Node) -> Option<Path> {
        self.update(|editor| editor.insert_inline_node(node))
    }

    /// Replace a text range with an inline node (for mention triggers).
    pub fn replace_range_with_inline_node(
        &self,
        range: leptoslate_core::Range,
        node: Node,
    ) -> Option<Path> {
        self.update(|editor| editor.replace_range_with_inline_node(range, node))
    }

    /// Move a top-level block to a final index.
    pub fn move_block(&self, from: usize, to: usize) -> bool {
        self.update(|editor| editor.move_block(from, to))
    }

    /// Insert a parsed fragment at the current selection.
    pub fn insert_fragment(&self, nodes: Vec<Node>) {
        self.update(|editor| editor.insert_fragment(nodes));
    }

    /// Parse and insert an HTML fragment.
    pub fn insert_html(&self, html: &str) {
        self.update(|editor| editor.insert_html(html));
    }

    /// Parse and insert a Markdown fragment.
    pub fn insert_markdown(&self, markdown: &str) {
        self.update(|editor| editor.insert_markdown(markdown));
    }

    /// Place the caret at the start of the text following a node.
    pub fn select_after_node(&self, path: &[usize]) -> Option<Point> {
        self.update(|editor| editor.select_after_node(path))
    }

    pub fn toggle_list(&self, list_type: &str) {
        self.update(|editor| editor.toggle_list(list_type));
    }

    /// Indent the current list item under its previous sibling.
    pub fn indent_list(&self) -> bool {
        self.update(Editor::indent_list)
    }

    /// Outdent the current list item.
    pub fn outdent_list(&self) -> bool {
        self.update(Editor::outdent_list)
    }

    pub fn select_all(&self) {
        self.update(Editor::select_all);
    }

    /// Select a block/node, clearing any text selection.
    pub fn select_node(&self, path: &[usize]) {
        self.update(|editor| editor.select_node(path));
    }

    /// Extend the node selection to another block.
    pub fn extend_node_selection_to(&self, path: &[usize]) {
        self.update(|editor| editor.extend_node_selection_to(path));
    }

    /// Extend the node selection up or down by one block.
    pub fn extend_node_selection(&self, direction: isize) -> bool {
        self.update(|editor| editor.extend_node_selection(direction))
    }

    /// Move a single block selection or collapse a multi-block range.
    pub fn move_node_selection(&self, direction: isize) -> bool {
        self.update(|editor| editor.move_node_selection(direction))
    }

    pub fn clear_node_selection(&self) {
        self.update(Editor::clear_node_selection);
    }

    pub fn has_node_selection(&self) -> bool {
        self.state
            .with_untracked(|editor| editor.has_node_selection())
    }

    /// Reactive check used by custom renderers to style the selected block.
    pub fn is_node_selected(&self, path: &[usize]) -> bool {
        self.state.with(|editor| editor.is_node_selected(path))
    }

    pub fn delete_node_selection(&self) -> bool {
        self.update(Editor::delete_node_selection)
    }

    pub fn duplicate_node_selection(&self) -> bool {
        self.update(Editor::duplicate_node_selection)
    }

    pub fn node_selection_string(&self) -> Option<String> {
        self.state
            .with_untracked(|editor| editor.node_selection_string())
    }

    pub fn move_selection_off_node(&self, reverse: bool) -> Option<Point> {
        self.update(|editor| editor.move_selection_off_node(reverse))
    }

    pub fn undo(&self) -> bool {
        self.update(Editor::undo)
    }

    pub fn redo(&self) -> bool {
        self.update(Editor::redo)
    }

    pub fn can_undo(&self) -> bool {
        self.state.with_untracked(|editor| editor.can_undo())
    }

    pub fn can_redo(&self) -> bool {
        self.state.with_untracked(|editor| editor.can_redo())
    }

    /// Marks shared by the current selection.
    pub fn marks(&self) -> Props {
        self.state
            .with_untracked(|editor| editor.marks_at_selection())
    }

    /// Whether the current selection fully carries `key = value`.
    pub fn mark_active(&self, key: &str, value: &Value) -> bool {
        self.state
            .with_untracked(|editor| editor.has_mark(key, value))
    }

    /// The block type at the caret, useful for toolbar state.
    pub fn active_block(&self) -> Option<String> {
        self.state.with_untracked(|editor| {
            let range = editor.selection.as_ref()?;
            let block = editor.leaf_block_path(range.start())?;
            editor
                .element(&block)
                .map(|element| element.element_type.clone())
        })
    }
}

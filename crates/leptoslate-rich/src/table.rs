//! Table transforms, schema and keyboard behavior for the rich preset.
//!
//! The core editor knows nothing about tables. This module owns the table
//! schema rules, cell/row/column transforms, empty-table normalization and the
//! structural hooks that keep block transforms out of table cells.

use leptos::prelude::WithUntracked;
use leptoslate::core::{path, Feature};
use leptoslate::{
    Editor, EditorHandle, Element, ElementRule, Node, Operation, Path, Point, Range, Schema,
};
use web_sys::{InputEvent, KeyboardEvent};

/// Table transforms provided by the rich preset.
///
/// Import this trait to call the methods on [`Editor`]:
///
/// ```rust,ignore
/// use leptoslate_rich::TableTransforms;
/// editor.insert_table(3, 3, true);
/// ```
pub trait TableTransforms {
    /// The nearest table cell containing `point`.
    fn table_cell_at(&self, point: &Point) -> Option<Path>;

    /// The nearest table containing `point`.
    fn table_at(&self, point: &Point) -> Option<Path>;

    /// The cell under the current selection, if any.
    fn table_cell_at_selection(&self) -> Option<Path>;

    /// The table containing the current selection.
    fn table_at_selection(&self) -> Option<Path>;

    /// Cells belonging to a table, in document order.
    fn table_cells_in(&self, table: &[usize]) -> Vec<Path>;

    /// Move the caret to the previous or next cell in the current table.
    fn move_table_cell(&mut self, direction: isize) -> bool;

    /// Whether the caret is at the first text position of its table cell.
    fn at_table_cell_start(&self, point: &Point) -> bool;

    /// Whether the caret is at the last text position of its table cell.
    fn at_table_cell_end(&self, point: &Point) -> bool;

    /// Insert a table after the current selection.
    fn insert_table(&mut self, rows: usize, cols: usize, header: bool) -> Option<Path>;

    /// Insert an empty row after the current row.
    fn insert_table_row_after(&mut self) -> bool;

    /// Delete the current table row when the table has more than one row.
    fn delete_table_row(&mut self) -> bool;

    /// Insert an empty cell after the current column in every row.
    fn insert_table_column_after(&mut self) -> bool;

    /// Delete the current column when every row has more than one cell.
    fn delete_table_column(&mut self) -> bool;
}

/// Table transforms on the reactive editor handle.
pub trait TableHandle {
    /// Whether the caret is inside a table cell.
    fn in_table_cell(&self) -> bool;

    /// Move to the previous/next table cell.
    fn move_table_cell(&self, direction: isize) -> bool;

    /// Whether a collapsed caret is at the start of its table cell.
    fn at_table_cell_start(&self) -> bool;

    /// Whether a collapsed caret is at the end of its table cell.
    fn at_table_cell_end(&self) -> bool;

    /// Insert a table after the current selection.
    fn insert_table(&self, rows: usize, cols: usize, header: bool) -> Option<Path>;

    /// Insert an empty row after the current row.
    fn insert_table_row_after(&self) -> bool;

    /// Delete the current table row.
    fn delete_table_row(&self) -> bool;

    /// Insert an empty cell after the current column.
    fn insert_table_column_after(&self) -> bool;

    /// Delete the current column.
    fn delete_table_column(&self) -> bool;
}

impl TableHandle for EditorHandle {
    fn in_table_cell(&self) -> bool {
        self.state
            .with_untracked(Editor::table_cell_at_selection)
            .is_some()
    }

    fn move_table_cell(&self, direction: isize) -> bool {
        self.update(|editor| editor.move_table_cell(direction))
    }

    fn at_table_cell_start(&self) -> bool {
        self.state.with_untracked(|editor| {
            let Some(range) = editor.selection.as_ref() else {
                return false;
            };
            range.is_collapsed() && editor.at_table_cell_start(range.start())
        })
    }

    fn at_table_cell_end(&self) -> bool {
        self.state.with_untracked(|editor| {
            let Some(range) = editor.selection.as_ref() else {
                return false;
            };
            range.is_collapsed() && editor.at_table_cell_end(range.start())
        })
    }

    fn insert_table(&self, rows: usize, cols: usize, header: bool) -> Option<Path> {
        self.update(|editor| editor.insert_table(rows, cols, header))
    }

    fn insert_table_row_after(&self) -> bool {
        self.update(Editor::insert_table_row_after)
    }

    fn delete_table_row(&self) -> bool {
        self.update(Editor::delete_table_row)
    }

    fn insert_table_column_after(&self) -> bool {
        self.update(Editor::insert_table_column_after)
    }

    fn delete_table_column(&self) -> bool {
        self.update(Editor::delete_table_column)
    }
}

/// Table schema rules, empty-container normalization and structural hooks.
#[derive(Clone, Copy, Debug, Default)]
pub struct RichTableFeature;

impl Feature for RichTableFeature {
    fn install_schema(&self, schema: &mut Schema) {
        schema.set_rule("table", ElementRule::block("table"));
        schema.set_rule("thead", ElementRule::block("thead"));
        schema.set_rule("tbody", ElementRule::block("tbody"));
        schema.set_rule("tfoot", ElementRule::block("tfoot"));
        schema.set_rule("tr", ElementRule::block("tr"));
        schema.set_rule("th", ElementRule::block("th"));
        schema.set_rule("td", ElementRule::block("td"));
    }

    fn normalize(&self, editor: &Editor) -> Option<Operation> {
        fn scan(editor: &Editor, children: &[Node], parent: &[usize]) -> Option<Operation> {
            for (index, node) in children.iter().enumerate() {
                let mut at = parent.to_vec();
                at.push(index);
                let Some(element) = node.as_element() else {
                    continue;
                };
                let is_container = editor.schema.is_table(&element.element_type)
                    || editor.schema.is_table_section(&element.element_type)
                    || editor.schema.is_table_row(&element.element_type);
                if is_container && element.children.is_empty() {
                    return Some(Operation::RemoveNode { path: at });
                }
                if editor.schema.is_table_element(&element.element_type) {
                    if let Some(operation) = scan(editor, &element.children, &at) {
                        return Some(operation);
                    }
                }
            }
            None
        }
        scan(editor, &editor.children, &[])
    }

    fn inline_container(&self, editor: &Editor, element_type: &str) -> bool {
        editor.schema.is_table_cell(element_type)
    }

    fn before_set_block(&self, editor: &mut Editor, _element_type: &str) -> bool {
        let Some(range) = editor.selection.as_ref() else {
            return false;
        };
        editor.table_cell_at(range.start()).is_some()
    }
}

impl TableTransforms for Editor {
    fn table_cell_at(&self, point: &Point) -> Option<Path> {
        for depth in (1..point.path.len()).rev() {
            let path = &point.path[..depth];
            if self
                .element(path)
                .map(|element| self.schema.is_table_cell(&element.element_type))
                .unwrap_or(false)
            {
                return Some(path.to_vec());
            }
        }
        None
    }

    fn table_at(&self, point: &Point) -> Option<Path> {
        for depth in (1..point.path.len()).rev() {
            let path = &point.path[..depth];
            if self
                .element(path)
                .map(|element| self.schema.is_table(&element.element_type))
                .unwrap_or(false)
            {
                return Some(path.to_vec());
            }
        }
        None
    }

    fn table_cell_at_selection(&self) -> Option<Path> {
        let range = self.selection.as_ref()?;
        self.table_cell_at(range.start())
    }

    fn table_at_selection(&self) -> Option<Path> {
        let range = self.selection.as_ref()?;
        self.table_at(range.start())
    }

    fn table_cells_in(&self, table: &[usize]) -> Vec<Path> {
        let mut cells = Vec::new();
        if let Some(element) = self.element(table) {
            collect_table_cells(&element.children, table, &self.schema, &mut cells);
        }
        cells
    }

    fn move_table_cell(&mut self, direction: isize) -> bool {
        let Some(range) = self.selection.clone() else {
            return false;
        };
        let Some(cell) = self.table_cell_at(range.start()) else {
            return false;
        };
        let Some(table) = self.table_at(range.start()) else {
            return false;
        };
        let cells = self.table_cells_in(&table);
        let Some(position) = cells.iter().position(|path| path == &cell) else {
            return false;
        };
        let next = position as isize + direction;
        if next < 0 || next as usize >= cells.len() {
            return false;
        }
        let Some(point) = self.first_text_point_in(&cells[next as usize]) else {
            return false;
        };
        self.selection = Some(Range::collapsed(point));
        self.marks = None;
        self.history.break_coalescing();
        true
    }

    fn at_table_cell_start(&self, point: &Point) -> bool {
        let Some(cell) = self.table_cell_at(point) else {
            return false;
        };
        self.first_text_point_in(&cell)
            .map(|start| start == *point)
            .unwrap_or(false)
    }

    fn at_table_cell_end(&self, point: &Point) -> bool {
        let Some(cell) = self.table_cell_at(point) else {
            return false;
        };
        self.last_text_point_in(&cell)
            .map(|end| end == *point)
            .unwrap_or(false)
    }

    fn insert_table(&mut self, rows: usize, cols: usize, header: bool) -> Option<Path> {
        let rows = rows.max(1);
        let cols = cols.max(1);
        let row = |cell_type: &str| {
            Node::Element(Element::new(
                "tr",
                (0..cols)
                    .map(|_| Node::Element(Element::new(cell_type, vec![Node::empty_text()])))
                    .collect(),
            ))
        };

        let mut children = Vec::new();
        if header {
            children.push(Node::Element(Element::new("thead", vec![row("th")])));
            if rows > 1 {
                children.push(Node::Element(Element::new(
                    "tbody",
                    (1..rows).map(|_| row("td")).collect(),
                )));
            }
        } else {
            children.push(Node::Element(Element::new(
                "tbody",
                (0..rows).map(|_| row("td")).collect(),
            )));
        }

        let table = Node::Element(Element::new("table", children));
        self.insert_block_after_selection(table)
    }

    fn insert_table_row_after(&mut self) -> bool {
        let Some(cell) = self.table_cell_at_selection() else {
            return false;
        };
        if cell.len() < 2 {
            return false;
        }
        let row = cell[..cell.len() - 1].to_vec();
        let Some(row_element) = self.element(&row).cloned() else {
            return false;
        };
        let mut children = Vec::new();
        for child in &row_element.children {
            let Some(element) = child.as_element() else {
                continue;
            };
            if self.schema.is_table_cell(&element.element_type) {
                children.push(Node::Element(Element::new(
                    element.element_type.clone(),
                    vec![Node::empty_text()],
                )));
            }
        }
        if children.is_empty() {
            return false;
        }
        let new_row = Node::Element(Element::new(row_element.element_type, children));
        let insert_at = child_path(&row[..row.len() - 1], *row.last().unwrap() + 1);
        let mut inserted = false;
        self.transaction(|editor| {
            if editor
                .apply_operation(Operation::InsertNode {
                    path: insert_at.clone(),
                    node: new_row,
                })
                .is_ok()
            {
                inserted = true;
                if let Some(point) = editor.first_text_point_in(&insert_at) {
                    editor.selection = Some(Range::collapsed(point));
                }
            }
        });
        inserted
    }

    fn delete_table_row(&mut self) -> bool {
        let Some(cell) = self.table_cell_at_selection() else {
            return false;
        };
        if cell.len() < 2 {
            return false;
        }
        let row = cell[..cell.len() - 1].to_vec();
        let Some(table) = self.table_at_selection() else {
            return false;
        };
        let rows = table_rows_in(self, &table);
        if rows.len() < 2 {
            return false;
        }
        let Some(position) = rows.iter().position(|path| path == &row) else {
            return false;
        };
        let adjacent = if position > 0 {
            rows.get(position - 1)
        } else {
            rows.get(position + 1)
        };
        if let Some(target) = adjacent.and_then(|path| self.first_text_point_in(path)) {
            self.selection = Some(Range::collapsed(target));
        }
        let mut removed = false;
        self.transaction(|editor| {
            if editor
                .apply_operation(Operation::RemoveNode { path: row.clone() })
                .is_ok()
            {
                removed = true;
            }
        });
        removed
    }

    fn insert_table_column_after(&mut self) -> bool {
        let Some(cell) = self.table_cell_at_selection() else {
            return false;
        };
        let Some(table) = self.table_at_selection() else {
            return false;
        };
        let Some(column) = cell.last().copied() else {
            return false;
        };
        let rows = table_rows_in(self, &table);
        let mut inserts = Vec::new();
        for row in &rows {
            let Some(row_element) = self.element(row) else {
                continue;
            };
            let cell_type = row_element
                .children
                .iter()
                .filter_map(Node::as_element)
                .find(|element| self.schema.is_table_cell(&element.element_type))
                .map(|element| element.element_type.clone())
                .unwrap_or_else(|| "td".to_string());
            inserts.push((
                child_path(row, column + 1),
                Node::Element(Element::new(cell_type, vec![Node::empty_text()])),
            ));
        }
        if inserts.is_empty() {
            return false;
        }
        let current_row = cell[..cell.len() - 1].to_vec();
        let new_cell = child_path(&current_row, column + 1);
        let mut inserted = false;
        self.transaction(|editor| {
            for (path, node) in inserts {
                if editor
                    .apply_operation(Operation::InsertNode { path, node })
                    .is_err()
                {
                    return;
                }
            }
            inserted = true;
            if let Some(point) = editor.first_text_point_in(&new_cell) {
                editor.selection = Some(Range::collapsed(point));
            }
        });
        inserted
    }

    fn delete_table_column(&mut self) -> bool {
        let Some(cell) = self.table_cell_at_selection() else {
            return false;
        };
        let Some(table) = self.table_at_selection() else {
            return false;
        };
        let Some(column) = cell.last().copied() else {
            return false;
        };
        let rows = table_rows_in(self, &table);
        let mut removals = Vec::new();
        for row in &rows {
            let Some(row_element) = self.element(row) else {
                return false;
            };
            let cell_count = row_element
                .children
                .iter()
                .filter(|child| {
                    child
                        .element_type()
                        .map(|kind| self.schema.is_table_cell(kind))
                        .unwrap_or(false)
                })
                .count();
            if cell_count <= 1 || column >= cell_count {
                return false;
            }
            removals.push(child_path(row, column));
        }
        removals.sort_by(|a, b| path::compare(b, a));
        let current_row = cell[..cell.len() - 1].to_vec();
        let target_column = column.saturating_sub(1);
        let target_cell = child_path(&current_row, target_column);
        let mut removed = false;
        self.transaction(|editor| {
            for path in removals {
                if editor
                    .apply_operation(Operation::RemoveNode { path })
                    .is_err()
                {
                    return;
                }
            }
            removed = true;
            if let Some(point) = editor.first_text_point_in(&target_cell) {
                editor.selection = Some(Range::collapsed(point));
            }
        });
        removed
    }
}

/// Prevents structural editing inside table cells and supports cell navigation.
pub struct RichTablePlugin;

impl leptoslate::EditablePlugin for RichTablePlugin {
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

fn child_path(parent: &[usize], index: usize) -> Path {
    let mut path = parent.to_vec();
    path.push(index);
    path
}

fn collect_table_cells(nodes: &[Node], parent: &[usize], schema: &Schema, cells: &mut Vec<Path>) {
    for (index, node) in nodes.iter().enumerate() {
        let mut path = parent.to_vec();
        path.push(index);
        let Some(element) = node.as_element() else {
            continue;
        };
        if schema.is_table_cell(&element.element_type) {
            cells.push(path);
            continue;
        }
        if schema.is_table_element(&element.element_type) {
            collect_table_cells(&element.children, &path, schema, cells);
        }
    }
}

fn table_rows_in(editor: &Editor, table: &[usize]) -> Vec<Path> {
    fn collect(nodes: &[Node], parent: &[usize], schema: &Schema, rows: &mut Vec<Path>) {
        for (index, node) in nodes.iter().enumerate() {
            let mut path = parent.to_vec();
            path.push(index);
            let Some(element) = node.as_element() else {
                continue;
            };
            if schema.is_table_row(&element.element_type) {
                rows.push(path);
                continue;
            }
            if schema.is_table(&element.element_type)
                || schema.is_table_section(&element.element_type)
            {
                collect(&element.children, &path, schema, rows);
            }
        }
    }

    let mut rows = Vec::new();
    if let Some(element) = editor.element(table) {
        collect(&element.children, table, &editor.schema, &mut rows);
    }
    rows
}

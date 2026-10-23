//! High-level transforms: the Rust equivalent of Slate's `Transforms` and
//! `Editor` mutation helpers.
//!
//! All public methods group their work in one undoable transaction and
//! normalize the document afterwards.

use serde_json::Value;

use crate::{
    node::{Element, Node},
    operation::Operation,
    path::{self, Path},
    point::Point,
    props::Props,
    range::Range,
    text::{char_len, char_slice, word_end_after, word_start_before},
    SlateError,
};

use crate::Editor;

impl Editor {
    // ---------------------------------------------------------------------
    // Text
    // ---------------------------------------------------------------------

    /// Insert text at the current selection, replacing it if expanded.
    ///
    /// Active marks (`editor.marks`) are applied to the inserted text, and the
    /// caret ends up after it.
    pub fn insert_text(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        self.transaction(|editor| editor.insert_text_inner(text));
    }

    fn insert_text_inner(&mut self, text: &str) {
        if self.node_selection.is_some() {
            self.delete_selected_node_inner();
        }

        if self
            .selection
            .as_ref()
            .map(|range| !range.is_collapsed())
            .unwrap_or(false)
        {
            self.delete_selection_inner();
        }

        let Some(point) = self
            .selection
            .as_ref()
            .map(|range| range.anchor.clone())
            .or_else(|| self.end_point())
        else {
            return;
        };
        let point = self.text_point(&point).unwrap_or(point);
        let length = char_len(text);

        if self
            .apply(Operation::InsertText {
                path: point.path.clone(),
                offset: point.offset,
                text: text.to_string(),
            })
            .is_err()
        {
            return;
        }

        let end = Point::new(point.path.clone(), point.offset + length);
        let marks = self.marks.clone().unwrap_or_default();
        if marks.is_empty() {
            self.selection = Some(Range::collapsed(end));
        } else {
            self.selection = Some(Range::new(point, end));
            self.format_selection_with(&marks);
            if let Some(selection) = self.selection.clone() {
                self.selection = Some(Range::collapsed(selection.end().clone()));
            }
        }
    }

    /// Insert a string, splitting blocks on newlines (paste behavior).
    pub fn insert_plain_text(&mut self, text: &str) {
        let text = text.replace("\r\n", "\n").replace('\r', "\n");
        if text.is_empty() {
            return;
        }
        self.transaction(|editor| {
            for (index, line) in text.split('\n').enumerate() {
                if index > 0 {
                    editor.insert_break_inner();
                }
                if !line.is_empty() {
                    editor.insert_text_inner(line);
                }
            }
        });
    }

    /// Insert a line break inside the current block (Shift+Enter).
    pub fn insert_soft_break(&mut self) {
        self.transaction(|editor| editor.insert_text_inner("\n"));
    }

    /// Split the current block, creating a new block (Enter).
    pub fn insert_break(&mut self) {
        self.transaction(|editor| editor.insert_break_inner());
    }

    fn insert_break_inner(&mut self) {
        if self.node_selection.is_some() {
            self.delete_selected_node_inner();
            return;
        }

        if self
            .selection
            .as_ref()
            .map(|range| !range.is_collapsed())
            .unwrap_or(false)
        {
            self.delete_selection_inner();
        }

        let Some(point) = self
            .selection
            .as_ref()
            .map(|range| range.anchor.clone())
            .or_else(|| self.end_point())
        else {
            return;
        };
        let point = self.text_point(&point).unwrap_or(point);

        if self
            .apply_split(&point.path, point.offset, Props::new())
            .is_err()
        {
            return;
        }

        let leaf: Path = point.path.clone();
        let mut caret = Point::new(path::next(&leaf), 0);
        self.selection = Some(Range::collapsed(caret.clone()));

        // Split up to and including the leaf block. A list item is the leaf
        // block of its text, so pressing Enter creates a sibling `<li>` rather
        // than splitting the whole list; paragraphs inside a blockquote stay
        // inside that blockquote.
        let stop_depth = self
            .list_context(&point)
            .map(|(li, _, _, _)| li.len())
            .or_else(|| self.leaf_block_path(&point).map(|block| block.len()))
            .unwrap_or(1);

        for depth in (stop_depth..leaf.len()).rev() {
            let ancestor = leaf[..depth].to_vec();
            let Some(position) = caret.path.get(depth).copied() else {
                break;
            };
            if self.apply_split(&ancestor, position, Props::new()).is_err() {
                break;
            }
            if let Some(selection) = &self.selection {
                caret = selection.anchor.clone();
            }
        }

        self.selection = Some(Range::collapsed(caret));
    }

    // ---------------------------------------------------------------------
    // Deletion
    // ---------------------------------------------------------------------

    /// Delete the selected range, if any.
    pub fn delete_selection(&mut self) {
        self.transaction(|editor| {
            editor.delete_selection_inner();
        });
    }

    pub fn delete_range(&mut self, range: Range) {
        self.transaction(|editor| {
            editor.clear_node_selection();
            editor.selection = Some(range);
            editor.delete_selection_inner();
        });
    }

    /// Delete one character (or grapheme-ish unit) backwards.
    pub fn delete_backward(&mut self) {
        self.transaction(|editor| editor.delete_backward_inner(false));
    }

    /// Delete one word backwards.
    pub fn delete_word_backward(&mut self) {
        self.transaction(|editor| editor.delete_backward_inner(true));
    }

    /// Delete one character forwards.
    pub fn delete_forward(&mut self) {
        self.transaction(|editor| editor.delete_forward_inner(false));
    }

    /// Delete one word forwards.
    pub fn delete_word_forward(&mut self) {
        self.transaction(|editor| editor.delete_forward_inner(true));
    }

    fn delete_selection_inner(&mut self) -> bool {
        if self.node_selection.is_some() {
            self.delete_selected_node_inner();
            return true;
        }
        let Some(range) = self.selection.clone() else {
            return false;
        };
        if range.is_collapsed() {
            return false;
        }

        let (start, end) = {
            let (start, end) = range.edges();
            (start.clone(), end.clone())
        };
        let start = self.text_point(&start).unwrap_or(start);
        let end = self.text_point(&end).unwrap_or(end);

        if start.path == end.path {
            if end.offset > start.offset {
                let text = self
                    .text(&start.path)
                    .map(|text| char_slice(&text.text, start.offset, end.offset).to_string())
                    .unwrap_or_default();
                let _ = self.apply(Operation::RemoveText {
                    path: start.path.clone(),
                    offset: start.offset,
                    text,
                });
            }
            self.selection = Some(Range::collapsed(start));
            return true;
        }

        // Trim the tail of the start leaf.
        if let Some(text) = self.text(&start.path) {
            let length = text.len();
            if start.offset < length {
                let removed = char_slice(&text.text, start.offset, length).to_string();
                let _ = self.apply(Operation::RemoveText {
                    path: start.path.clone(),
                    offset: start.offset,
                    text: removed,
                });
            }
        }

        // Trim the head of the end leaf.
        if end.offset > 0 {
            if let Some(text) = self.text(&end.path) {
                let removed = char_slice(&text.text, 0, end.offset).to_string();
                let _ = self.apply(Operation::RemoveText {
                    path: end.path.clone(),
                    offset: 0,
                    text: removed,
                });
            }
        }

        let common = path::common(&start.path, &end.path);
        let start_branch = start.path.get(common.len()).copied().unwrap_or(0);
        let end_branch = end.path.get(common.len()).copied().unwrap_or(0);

        // Remove nodes fully inside the selection, deepest/last first.
        let mut index = end_branch;
        while index > start_branch + 1 {
            index -= 1;
            let mut removed = common.clone();
            removed.push(index);
            let _ = self.apply(Operation::RemoveNode { path: removed });
        }

        // Merge the remaining end branch into the start branch.
        if end_branch > start_branch {
            let mut merge_at = common.clone();
            merge_at.push(start_branch + 1);
            let _ = self.apply_merge(&merge_at);
        }

        self.selection = Some(Range::collapsed(start));
        true
    }

    fn delete_backward_inner(&mut self, word: bool) {
        if self.node_selection.is_some() {
            self.delete_selected_node_inner();
            return;
        }

        if self
            .selection
            .as_ref()
            .map(|range| !range.is_collapsed())
            .unwrap_or(false)
        {
            self.delete_selection_inner();
            return;
        }

        let Some(point) = self.selection.as_ref().map(|range| range.anchor.clone()) else {
            return;
        };
        let point = self.text_point(&point).unwrap_or(point);
        let Some(text) = self.text(&point.path).cloned() else {
            return;
        };

        if point.offset > 0 {
            let start = if word {
                word_start_before(&text.text, point.offset)
            } else {
                point.offset - 1
            };
            if start < point.offset {
                let removed = char_slice(&text.text, start, point.offset).to_string();
                let _ = self.apply(Operation::RemoveText {
                    path: point.path.clone(),
                    offset: start,
                    text: removed,
                });
                self.selection = Some(Range::collapsed(Point::new(point.path, start)));
            }
            return;
        }

        if point.offset == 0 {
            if let Some((li, list, index, _)) = self.list_context(&point) {
                if self.at_start_of_node(&point, &li) {
                    if index == 0 {
                        if self.lift_first_list_item_inner().is_some() {
                            return;
                        }
                    } else if self
                        .merge_list_item_with_previous_inner(&li, &list, index)
                        .is_some()
                    {
                        return;
                    }
                }
            }
        }

        // Backspace immediately after an inline void removes the atom.
        if let Some(previous) = path::previous(&point.path) {
            let is_inline_void = self
                .element(&previous)
                .map(|element| {
                    self.schema.is_inline(&element.element_type)
                        && self.schema.is_void(&element.element_type)
                })
                .unwrap_or(false);
            if is_inline_void {
                let _ = self.apply(Operation::RemoveNode {
                    path: previous.clone(),
                });
                self.selection = Some(Range::collapsed(Point::new(previous, 0)));
                return;
            }
        }

        // Delete across inline boundaries (or between adjacent text nodes)
        // when the previous text leaf belongs to the same block.
        if let (Some(block), Some(previous_point)) = (
            self.leaf_block_path(&point),
            self.previous_text_point(&point),
        ) {
            let same_block = self.leaf_block_path(&previous_point).as_ref() == Some(&block);
            if same_block && previous_point.offset > 0 {
                if let Some(text) = self.text(&previous_point.path).cloned() {
                    let start = if word {
                        word_start_before(&text.text, previous_point.offset)
                    } else {
                        previous_point.offset - 1
                    };
                    let removed = char_slice(&text.text, start, previous_point.offset).to_string();
                    let _ = self.apply(Operation::RemoveText {
                        path: previous_point.path.clone(),
                        offset: start,
                        text: removed,
                    });
                    self.selection = Some(Range::collapsed(Point::new(previous_point.path, start)));
                    return;
                }
            }
        }

        // Merge with the previous text sibling.
        if let Some(previous) = path::previous(&point.path) {
            if let Some(previous_text) = self.text(&previous).cloned() {
                let offset = previous_text.len();
                if self.apply_merge(&point.path).is_ok() {
                    self.selection = Some(Range::collapsed(Point::new(previous, offset)));
                }
                return;
            }
        }

        // Merge with the previous block.
        let Some(block) = self.leaf_block_path(&point) else {
            return;
        };
        let Some(previous_block) = path::previous(&block) else {
            return;
        };
        if let Some(junction) = self.last_text_point_in(&previous_block) {
            if self.apply_merge(&block).is_ok() {
                self.selection = Some(Range::collapsed(junction));
            }
        }
    }

    fn delete_forward_inner(&mut self, word: bool) {
        if self.node_selection.is_some() {
            self.delete_selected_node_inner();
            return;
        }

        if self
            .selection
            .as_ref()
            .map(|range| !range.is_collapsed())
            .unwrap_or(false)
        {
            self.delete_selection_inner();
            return;
        }

        let Some(point) = self.selection.as_ref().map(|range| range.anchor.clone()) else {
            return;
        };
        let point = self.text_point(&point).unwrap_or(point);
        let Some(text) = self.text(&point.path).cloned() else {
            return;
        };

        if point.offset < text.len() {
            let end = if word {
                word_end_after(&text.text, point.offset)
            } else {
                point.offset + 1
            };
            if end > point.offset {
                let removed = char_slice(&text.text, point.offset, end).to_string();
                let _ = self.apply(Operation::RemoveText {
                    path: point.path.clone(),
                    offset: point.offset,
                    text: removed,
                });
                self.selection = Some(Range::collapsed(point));
            }
            return;
        }

        // Delete immediately before an inline void removes the atom.
        let next_sibling = path::next(&point.path);
        let next_is_inline_void = self
            .element(&next_sibling)
            .map(|element| {
                self.schema.is_inline(&element.element_type)
                    && self.schema.is_void(&element.element_type)
            })
            .unwrap_or(false);
        if next_is_inline_void {
            let _ = self.apply(Operation::RemoveNode { path: next_sibling });
            self.selection = Some(Range::collapsed(point));
            return;
        }

        // Delete across inline boundaries when the next text leaf belongs to
        // the same block.
        if let (Some(block), Some(next_point)) =
            (self.leaf_block_path(&point), self.next_text_point(&point))
        {
            let same_block = self.leaf_block_path(&next_point).as_ref() == Some(&block);
            if same_block {
                if let Some(text) = self.text(&next_point.path).cloned() {
                    if !text.is_empty() {
                        let end = if word {
                            word_end_after(&text.text, 0)
                        } else {
                            1
                        };
                        let removed = char_slice(&text.text, 0, end).to_string();
                        let _ = self.apply(Operation::RemoveText {
                            path: next_point.path,
                            offset: 0,
                            text: removed,
                        });
                        self.selection = Some(Range::collapsed(point));
                        return;
                    }
                }
            }
        }

        // Merge the next text sibling into this leaf.
        let next_leaf = path::next(&point.path);
        if self.text(&next_leaf).is_some() {
            let _ = self.apply_merge(&next_leaf);
            self.selection = Some(Range::collapsed(point));
            return;
        }

        // Merge the next block into this block.
        let Some(block) = self.leaf_block_path(&point) else {
            return;
        };
        let next_block = path::next(&block);
        if self.node(&next_block).is_none() {
            return;
        }
        if self.apply_merge(&next_block).is_ok() {
            self.selection = Some(Range::collapsed(point));
        }
    }

    // ---------------------------------------------------------------------
    // Marks
    // ---------------------------------------------------------------------

    /// Toggle a mark on the selection, or on the cursor for future typing.
    pub fn toggle_mark(&mut self, key: &str, value: Value) {
        self.transaction(|editor| {
            let active = editor.has_mark(key, &value);
            if active {
                let mut marks = editor
                    .marks
                    .clone()
                    .unwrap_or_else(|| editor.marks_at_selection());
                // `null` explicitly removes the property from text inserted
                // next. This matters at a collapsed cursor inside marked text:
                // simply dropping the key would inherit the old mark.
                marks.insert(key.to_string(), Value::Null);
                editor.marks = Some(marks);
                let collapsed = editor
                    .selection
                    .as_ref()
                    .map(Range::is_collapsed)
                    .unwrap_or(true);
                if !collapsed {
                    let mut remove = Props::new();
                    remove.insert(key.to_string(), Value::Null);
                    editor.format_selection_with(&remove);
                }
            } else {
                let mut marks = editor
                    .marks
                    .clone()
                    .unwrap_or_else(|| editor.marks_at_selection());
                marks.insert(key.to_string(), value.clone());
                editor.marks = Some(marks);
                let collapsed = editor
                    .selection
                    .as_ref()
                    .map(Range::is_collapsed)
                    .unwrap_or(true);
                if !collapsed {
                    let mut add = Props::new();
                    add.insert(key.to_string(), value);
                    editor.format_selection_with(&add);
                }
            }
        });
    }

    /// Add a mark regardless of its current state.
    pub fn add_mark(&mut self, key: &str, value: Value) {
        self.transaction(|editor| {
            let mut marks = editor
                .marks
                .clone()
                .unwrap_or_else(|| editor.marks_at_selection());
            marks.insert(key.to_string(), value.clone());
            editor.marks = Some(marks);
            if let Some(range) = editor.selection.clone() {
                if !range.is_collapsed() {
                    let mut add = Props::new();
                    add.insert(key.to_string(), value);
                    editor.format_selection_with(&add);
                }
            }
        });
    }

    /// Remove a mark from the selection.
    pub fn remove_mark(&mut self, key: &str) {
        self.transaction(|editor| {
            let mut marks = editor
                .marks
                .clone()
                .unwrap_or_else(|| editor.marks_at_selection());
            marks.insert(key.to_string(), Value::Null);
            editor.marks = Some(marks);
            if let Some(range) = editor.selection.clone() {
                if !range.is_collapsed() {
                    let mut remove = Props::new();
                    remove.insert(key.to_string(), Value::Null);
                    editor.format_selection_with(&remove);
                }
            }
        });
    }

    // ---------------------------------------------------------------------
    // Tables
    // ---------------------------------------------------------------------

    /// The nearest table cell containing `point`.
    pub fn table_cell_at(&self, point: &Point) -> Option<Path> {
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

    /// The nearest table containing `point`.
    pub fn table_at(&self, point: &Point) -> Option<Path> {
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

    /// The cell under the current selection, if any.
    pub fn table_cell_at_selection(&self) -> Option<Path> {
        let range = self.selection.as_ref()?;
        self.table_cell_at(range.start())
    }

    /// Cells belonging to a table, in document order.
    pub fn table_cells_in(&self, table: &[usize]) -> Vec<Path> {
        let mut cells = Vec::new();
        if let Some(element) = self.element(table) {
            collect_table_cells(&element.children, table, &self.schema, &mut cells);
        }
        cells
    }

    /// Move the caret to the previous or next cell in the current table.
    pub fn move_table_cell(&mut self, direction: isize) -> bool {
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

    /// Whether the caret is at the first text position of its table cell.
    pub fn at_table_cell_start(&self, point: &Point) -> bool {
        let Some(cell) = self.table_cell_at(point) else {
            return false;
        };
        self.first_text_point_in(&cell)
            .map(|start| start == *point)
            .unwrap_or(false)
    }

    /// Whether the caret is at the last text position of its table cell.
    pub fn at_table_cell_end(&self, point: &Point) -> bool {
        let Some(cell) = self.table_cell_at(point) else {
            return false;
        };
        self.last_text_point_in(&cell)
            .map(|end| end == *point)
            .unwrap_or(false)
    }

    /// Insert an empty row after the current row.
    pub fn insert_table_row_after(&mut self) -> bool {
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
                .apply(Operation::InsertNode {
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

    /// Delete the current table row when the table has more than one row.
    pub fn delete_table_row(&mut self) -> bool {
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
        let rows = self.table_rows_in(&table);
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
                .apply(Operation::RemoveNode { path: row.clone() })
                .is_ok()
            {
                removed = true;
            }
        });
        removed
    }

    /// Insert an empty cell after the current column in every row.
    pub fn insert_table_column_after(&mut self) -> bool {
        let Some(cell) = self.table_cell_at_selection() else {
            return false;
        };
        let Some(table) = self.table_at_selection() else {
            return false;
        };
        let Some(column) = cell.last().copied() else {
            return false;
        };
        let rows = self.table_rows_in(&table);
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
                if editor.apply(Operation::InsertNode { path, node }).is_err() {
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

    /// Delete the current column when every row has more than one cell.
    pub fn delete_table_column(&mut self) -> bool {
        let Some(cell) = self.table_cell_at_selection() else {
            return false;
        };
        let Some(table) = self.table_at_selection() else {
            return false;
        };
        let Some(column) = cell.last().copied() else {
            return false;
        };
        let rows = self.table_rows_in(&table);
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
                if editor.apply(Operation::RemoveNode { path }).is_err() {
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

    /// The table containing the current selection.
    pub fn table_at_selection(&self) -> Option<Path> {
        let range = self.selection.as_ref()?;
        self.table_at(range.start())
    }

    fn table_rows_in(&self, table: &[usize]) -> Vec<Path> {
        let mut rows = Vec::new();
        if let Some(element) = self.element(table) {
            collect_table_rows(&element.children, table, &self.schema, &mut rows);
        }
        rows
    }

    // ---------------------------------------------------------------------
    // Inline elements and links
    // ---------------------------------------------------------------------

    /// Wrap the selected text in an inline element, or update the link at the
    /// current selection when it is already inside one.
    ///
    /// Returns `false` when the selection is collapsed outside a link, or spans
    /// multiple blocks (inline elements must stay within one block).
    pub fn set_link(&mut self, href: &str) -> bool {
        let href = href.trim();
        if href.is_empty() {
            return false;
        }

        self.transaction(|editor| {
            let Some(range) = editor.selection.clone() else {
                return false;
            };
            let start = editor
                .text_point(range.start())
                .unwrap_or(range.start().clone());
            let end = editor
                .text_point(range.end())
                .unwrap_or(range.end().clone());

            if let Some(link) = editor.inline_ancestor(&start.path, "a") {
                if range.is_collapsed()
                    || editor.inline_ancestor(&end.path, "a").as_ref() == Some(&link)
                {
                    let mut properties = Props::new();
                    properties.insert("href".into(), Value::String(href.to_string()));
                    let _ = editor.apply(Operation::SetNode {
                        path: link,
                        properties: Props::new(),
                        new_properties: properties,
                    });
                    return true;
                }
            }

            if range.is_collapsed() {
                return false;
            }

            let mut properties = Props::new();
            properties.insert("href".into(), Value::String(href.to_string()));
            editor.wrap_inline_inner("a", properties).is_some()
        })
    }

    /// Remove links intersecting the current selection.
    pub fn remove_link(&mut self) -> bool {
        self.transaction(|editor| {
            let Some(range) = editor.selection.clone() else {
                return false;
            };

            let mut paths = editor.text_paths_in_range(&range);
            if range.is_collapsed() {
                paths.push(range.start().path.clone());
            } else {
                paths.push(range.start().path.clone());
                paths.push(range.end().path.clone());
            }

            let mut links: Vec<Path> = paths
                .iter()
                .filter_map(|path| editor.inline_ancestor(path, "a"))
                .collect();
            links.sort_by(|a, b| path::compare(b, a));
            links.dedup();

            if links.is_empty() {
                return false;
            }

            for link in links {
                editor.unwrap_inline_inner(&link);
            }
            true
        })
    }

    /// The link containing the start of the current selection.
    pub fn link_at_selection(&self) -> Option<(Path, String)> {
        let range = self.selection.as_ref()?;
        let link = self.inline_ancestor(&range.start().path, "a")?;
        let href = self
            .element(&link)?
            .properties
            .get("href")?
            .as_str()?
            .to_string();
        Some((link, href))
    }

    fn wrap_inline_inner(&mut self, element_type: &str, properties: Props) -> Option<Path> {
        let range = self.selection.clone()?;
        if range.is_collapsed() {
            return None;
        }

        self.split_text_boundaries();

        let range = self.selection.clone()?;
        let start = self.text_point(range.start())?;
        let end = self.text_point(range.end())?;
        if start.path.len() < 2 || end.path.len() < 2 {
            return None;
        }

        let parent = start.path[..start.path.len() - 1].to_vec();
        if end.path[..end.path.len() - 1] != parent {
            return None;
        }
        if self
            .element(&parent)
            .map(|element| self.schema.is_inline(&element.element_type))
            .unwrap_or(false)
        {
            return None;
        }

        let mut start_index = *start.path.last()?;
        let start_len = self.text_len_at(&start.path).unwrap_or(0);
        if start.offset >= start_len {
            start_index += 1;
        }

        let mut end_index = *end.path.last()?;
        if end.offset == 0 {
            end_index = end_index.checked_sub(1)?;
        }

        if start_index > end_index {
            // The range only covers a boundary between two nodes.
            return None;
        }
        let count = end_index - start_index + 1;

        let children = self.element(&parent)?.children.clone();
        if start_index + count > children.len() {
            return None;
        }
        let wrapped = &children[start_index..start_index + count];
        if element_type == "a"
            && wrapped.iter().any(|node| {
                node.element_type()
                    .map(|element_type| element_type == "a")
                    .unwrap_or(false)
            })
        {
            return None;
        }

        let wrapper = Node::Element(Element {
            element_type: element_type.to_string(),
            properties,
            children: wrapped.to_vec(),
        });

        let mut insert_at = parent.clone();
        insert_at.push(start_index);
        self.apply(Operation::InsertNode {
            path: insert_at.clone(),
            node: wrapper,
        })
        .ok()?;

        for _ in 0..count {
            let mut remove_at = parent.clone();
            remove_at.push(start_index + 1);
            self.apply(Operation::RemoveNode { path: remove_at }).ok()?;
        }

        let first = self.first_text_point_in(&insert_at)?;
        let last = self.last_text_point_in(&insert_at)?;
        self.selection = Some(Range::new(first, last));
        Some(insert_at)
    }

    fn unwrap_inline_inner(&mut self, link: &[usize]) -> Option<()> {
        let parent = link[..link.len() - 1].to_vec();
        let index = *link.last()?;
        let children = self.element(link)?.children.clone();

        self.apply(Operation::RemoveNode {
            path: link.to_vec(),
        })
        .ok()?;

        for (offset, child) in children.iter().enumerate() {
            let mut insert_at = parent.clone();
            insert_at.push(index + offset);
            self.apply(Operation::InsertNode {
                path: insert_at,
                node: child.clone(),
            })
            .ok()?;
        }

        let mut first_at = parent.clone();
        first_at.push(index);
        if let (Some(first), Some(last)) = (
            self.first_text_point_in(&first_at),
            (if children.is_empty() {
                None
            } else {
                let mut last_at = parent.clone();
                last_at.push(index + children.len() - 1);
                self.last_text_point_in(&last_at)
            }),
        ) {
            self.selection = Some(Range::new(first, last));
        }
        Some(())
    }

    fn inline_ancestor(&self, path: &[usize], element_type: &str) -> Option<Path> {
        for depth in (1..path.len()).rev() {
            let candidate = &path[..depth];
            if let Some(element) = self.element(candidate) {
                if element.element_type == element_type
                    && self.schema.is_inline(&element.element_type)
                {
                    return Some(candidate.to_vec());
                }
            }
        }
        None
    }

    fn previous_text_point(&self, at: &Point) -> Option<Point> {
        let mut path = at.path.clone();
        while !path.is_empty() {
            if let Some(previous) = path::previous(&path) {
                if let Some(point) = self.last_text_point_in(&previous) {
                    return Some(point);
                }
            }
            path.pop();
        }
        None
    }

    fn next_text_point(&self, at: &Point) -> Option<Point> {
        let mut path = at.path.clone();
        while !path.is_empty() {
            let next = path::next(&path);
            if let Some(point) = self.first_text_point_in(&next) {
                return Some(point);
            }
            path.pop();
        }
        None
    }

    /// Format text leaves intersecting the current selection, splitting leaves
    /// at the range boundaries first.
    fn format_selection_with(&mut self, properties: &Props) {
        if self
            .selection
            .as_ref()
            .map(Range::is_collapsed)
            .unwrap_or(true)
        {
            return;
        }
        self.split_text_boundaries();
        let Some(range) = self.selection.clone() else {
            return;
        };
        let paths = self.text_paths_in_range(&range);
        for at in paths {
            let _ = self.apply(Operation::SetNode {
                path: at,
                properties: Props::new(),
                new_properties: properties.clone(),
            });
        }
    }

    fn split_text_boundaries(&mut self) {
        let Some(range) = self.selection.clone() else {
            return;
        };
        if range.is_collapsed() {
            return;
        }

        let end = range.end().clone();
        let end_len = self.text_len_at(&end.path).unwrap_or(0);
        if end.offset > 0 && end.offset < end_len {
            let _ = self.apply_split(&end.path, end.offset, Props::new());
        }

        let start = self
            .selection
            .as_ref()
            .map(|range| range.start().clone())
            .unwrap_or(range.start().clone());
        let start_len = self.text_len_at(&start.path).unwrap_or(0);
        if start.offset > 0 && start.offset < start_len {
            let _ = self.apply_split(&start.path, start.offset, Props::new());
        }
    }

    // ---------------------------------------------------------------------
    // Blocks and lists
    // ---------------------------------------------------------------------

    /// Set the block type of the block(s) intersecting the selection.
    ///
    /// Use [`Editor::toggle_list`] for `ul` / `ol`.
    pub fn set_block(&mut self, element_type: &str) {
        if self.schema.is_list(element_type) {
            return self.toggle_list(element_type);
        }
        if self
            .selection
            .as_ref()
            .and_then(|range| self.table_cell_at(range.start()))
            .is_some()
        {
            return;
        }
        self.transaction(|editor| {
            // Turning a list item into a paragraph/heading should unwrap the
            // list first, rather than leaving `<ul><h1>` behind.
            if let Some(range) = editor.selection.clone() {
                if let Some(list_type) = editor.list_ancestor_type(range.start()) {
                    editor.toggle_list(&list_type);
                }
            }

            let blocks = editor.blocks_in_selection();
            for block in blocks {
                let Some(element) = editor.element(&block) else {
                    continue;
                };
                if element.element_type == element_type {
                    continue;
                }
                let mut properties = Props::new();
                properties.insert("type".into(), Value::String(element.element_type.clone()));
                let mut new_properties = Props::new();
                new_properties.insert("type".into(), Value::String(element_type.into()));
                let _ = editor.apply(Operation::SetNode {
                    path: block,
                    properties,
                    new_properties,
                });
            }
        });
    }

    fn list_ancestor_type(&self, point: &Point) -> Option<String> {
        for depth in (1..point.path.len()).rev() {
            let path = &point.path[..depth];
            if let Some(element) = self.element(path) {
                if self.schema.is_list(&element.element_type) {
                    return Some(element.element_type.clone());
                }
            }
        }
        None
    }

    /// Wrap the selected block(s) in a list, or unwrap a list at the cursor.
    pub fn toggle_list(&mut self, list_type: &str) {
        if !self.schema.is_list(list_type) {
            return;
        }
        if self
            .selection
            .as_ref()
            .and_then(|range| self.table_cell_at(range.start()))
            .is_some()
        {
            return;
        }

        self.transaction(|editor| {
            let Some(range) = editor.selection.clone() else {
                return;
            };
            let top_index = range.start().path.first().copied().unwrap_or(0);

            // Unwrap an existing list under the cursor.
            if let Some(Node::Element(list)) = editor.children.get(top_index) {
                if editor.schema.is_list(&list.element_type) {
                    let list_children = list.children.clone();
                    let list_path = vec![top_index];
                    let mut paragraphs = Vec::new();
                    for child in &list_children {
                        let children = child.children().to_vec();
                        paragraphs.push(Node::Element(Element::new("p", children)));
                    }

                    let _ = editor.apply(Operation::RemoveNode { path: list_path });
                    for (offset, paragraph) in paragraphs.into_iter().enumerate() {
                        let _ = editor.apply(Operation::InsertNode {
                            path: vec![top_index + offset],
                            node: paragraph,
                        });
                    }

                    let map = |point: &Point| {
                        if point.path.len() >= 2 {
                            let mut path = vec![top_index + point.path[1]];
                            path.extend_from_slice(&point.path[2..]);
                            Point::new(path, point.offset)
                        } else {
                            point.clone()
                        }
                    };
                    editor.selection = Some(Range::new(map(&range.anchor), map(&range.focus)));
                    return;
                }
            }

            // Wrap all selected top-level blocks in a new list.
            let start = range
                .start()
                .path
                .first()
                .copied()
                .unwrap_or(0)
                .min(editor.children.len().saturating_sub(1));
            let end = range
                .end()
                .path
                .first()
                .copied()
                .unwrap_or(start)
                .min(editor.children.len().saturating_sub(1));
            let (first, last) = if start <= end {
                (start, end)
            } else {
                (end, start)
            };
            if editor.children.is_empty() {
                return;
            }

            let mut list_children = Vec::new();
            for index in first..=last {
                if let Some(block) = editor.children.get(index) {
                    let children = if block.is_text() {
                        vec![block.clone()]
                    } else {
                        block.children().to_vec()
                    };
                    list_children.push(Node::Element(Element::new("li", children)));
                }
            }

            let list = Node::Element(Element::new(list_type, list_children));
            let _ = editor.apply(Operation::InsertNode {
                path: vec![first],
                node: list,
            });
            for _ in first..=last {
                let _ = editor.apply(Operation::RemoveNode {
                    path: vec![first + 1],
                });
            }

            let map = |point: &Point| {
                let block_index = point.path.first().copied().unwrap_or(0);
                let mut path = vec![first, block_index.saturating_sub(first)];
                path.extend_from_slice(&point.path[1..]);
                Point::new(path, point.offset)
            };
            editor.selection = Some(Range::new(map(&range.anchor), map(&range.focus)));
        });
    }

    // ---------------------------------------------------------------------
    // Lists
    // ---------------------------------------------------------------------

    /// Indent the current list item under its previous sibling.
    pub fn indent_list(&mut self) -> bool {
        self.transaction(|editor| editor.indent_list_inner())
            .is_some()
    }

    /// Outdent the current list item, lifting it out of the list at the top
    /// level or into the parent list when nested.
    pub fn outdent_list(&mut self) -> bool {
        self.transaction(|editor| editor.outdent_list_inner())
            .is_some()
    }

    fn indent_list_inner(&mut self) -> Option<()> {
        let range = self.selection.clone()?;
        let point = self.text_point(range.start())?;
        let (li, list, index, list_type) = self.list_context(&point)?;
        if index == 0 {
            return None;
        }

        let li_node = self.node(&li)?.clone();
        let previous = child_path(&list, index - 1);
        self.apply(Operation::RemoveNode { path: li.clone() })
            .ok()?;

        let previous_children = self.element(&previous)?.children.clone();
        let nested = previous_children
            .last()
            .filter(|node| node.element_type() == Some(list_type.as_str()))
            .map(|_| child_path(&previous, previous_children.len() - 1));

        let new_li_path = if let Some(nested) = nested {
            let nested_len = self.element(&nested)?.children.len();
            let at = child_path(&nested, nested_len);
            self.apply(Operation::InsertNode {
                path: at.clone(),
                node: li_node,
            })
            .ok()?;
            at
        } else {
            let new_list = Node::Element(Element::new(list_type, vec![li_node]));
            let at = child_path(&previous, previous_children.len());
            self.apply(Operation::InsertNode {
                path: at.clone(),
                node: new_list,
            })
            .ok()?;
            child_path(&at, 0)
        };

        if let Some(point) = self.first_text_point_in(&new_li_path) {
            self.selection = Some(Range::collapsed(point));
        }
        Some(())
    }

    fn outdent_list_inner(&mut self) -> Option<()> {
        let range = self.selection.clone()?;
        let point = self.text_point(range.start())?;
        let (li, list, _, _) = self.list_context(&point)?;
        let li_node = self.node(&li)?.clone();
        let list_len = self.element(&list)?.children.len();
        let list_top = *list.first()?;

        if list.len() == 1 {
            let children = li_node.children().to_vec();
            self.apply(Operation::RemoveNode { path: li }).ok()?;
            let block = Node::Element(Element::new(self.schema.default_block.clone(), children));
            let insert_at = if list_len == 1 {
                self.apply(Operation::RemoveNode { path: list }).ok()?;
                vec![list_top]
            } else {
                vec![list_top + 1]
            };
            self.apply(Operation::InsertNode {
                path: insert_at.clone(),
                node: block,
            })
            .ok()?;
            if let Some(point) = self.first_text_point_in(&insert_at) {
                self.selection = Some(Range::collapsed(point));
            }
            return Some(());
        }

        let outer_li = path::parent(&list)?;
        let outer_list = path::parent(&outer_li)?;
        let outer_index = *outer_li.last()?;

        self.apply(Operation::RemoveNode { path: li }).ok()?;
        if list_len == 1 {
            self.apply(Operation::RemoveNode { path: list }).ok()?;
        }
        let insert_at = child_path(&outer_list, outer_index + 1);
        self.apply(Operation::InsertNode {
            path: insert_at.clone(),
            node: li_node,
        })
        .ok()?;
        if let Some(point) = self.first_text_point_in(&insert_at) {
            self.selection = Some(Range::collapsed(point));
        }
        Some(())
    }

    fn lift_first_list_item_inner(&mut self) -> Option<()> {
        let range = self.selection.clone()?;
        let point = self.text_point(range.start())?;
        let (li, list, index, _) = self.list_context(&point)?;
        if index != 0 {
            return None;
        }
        if list.len() > 1 {
            return self.outdent_list_inner();
        }

        let li_node = self.node(&li)?.clone();
        let list_top = *list.first()?;
        let children = li_node.children().to_vec();

        self.apply(Operation::RemoveNode { path: li }).ok()?;
        if self.element(&list)?.children.is_empty() {
            self.apply(Operation::RemoveNode { path: list }).ok()?;
        }

        let block = Node::Element(Element::new(self.schema.default_block.clone(), children));
        let insert_at = vec![list_top];
        self.apply(Operation::InsertNode {
            path: insert_at.clone(),
            node: block,
        })
        .ok()?;
        if let Some(point) = self.first_text_point_in(&insert_at) {
            self.selection = Some(Range::collapsed(point));
        }
        Some(())
    }

    fn merge_list_item_with_previous_inner(
        &mut self,
        li: &[usize],
        list: &[usize],
        index: usize,
    ) -> Option<()> {
        if index == 0 {
            return None;
        }
        let previous = child_path(list, index - 1);
        let junction = self.last_text_point_in(&previous)?;
        let children = self.element(li)?.children.clone();

        self.apply(Operation::RemoveNode { path: li.to_vec() })
            .ok()?;
        let previous_len = self.element(&previous)?.children.len();
        for (offset, child) in children.into_iter().enumerate() {
            self.apply(Operation::InsertNode {
                path: child_path(&previous, previous_len + offset),
                node: child,
            })
            .ok()?;
        }

        self.selection = Some(Range::collapsed(junction));
        Some(())
    }

    fn list_context(&self, point: &Point) -> Option<(Path, Path, usize, String)> {
        for depth in (1..=point.path.len()).rev() {
            let candidate = &point.path[..depth];
            let Some(element) = self.element(candidate) else {
                continue;
            };
            if element.element_type != "li" {
                continue;
            }
            let list = path::parent(candidate)?;
            let list_element = self.element(&list)?;
            if !self.schema.is_list(&list_element.element_type) {
                return None;
            }
            let index = *candidate.last()?;
            return Some((
                candidate.to_vec(),
                list,
                index,
                list_element.element_type.clone(),
            ));
        }
        None
    }

    fn at_start_of_node(&self, point: &Point, node: &[usize]) -> bool {
        self.first_text_point_in(node)
            .map(|first| first.path == point.path && first.offset == 0)
            .unwrap_or(false)
    }

    /// The leaf block paths intersecting the current selection.
    pub fn blocks_in_selection(&self) -> Vec<Path> {
        if self.has_node_selection() {
            return self.node_selections();
        }
        let Some(range) = &self.selection else {
            return Vec::new();
        };
        let mut blocks = Vec::new();
        if range.is_collapsed() {
            if let Some(block) = self.leaf_block_path(range.start()) {
                blocks.push(block);
            }
            return blocks;
        }
        for text_path in self.text_paths_in_range(range) {
            if let Some(block) = self.leaf_block_path(&Point::new(text_path, 0)) {
                blocks.push(block);
            }
        }
        if let Some(block) = self.leaf_block_path(range.end()) {
            blocks.push(block);
        }
        blocks.sort();
        blocks.dedup();
        blocks
    }

    /// The closest block ancestor of a point (skipping inline elements).
    pub fn leaf_block_path(&self, point: &Point) -> Option<Path> {
        let mut block = path::parent(&point.path)?;
        while let Some(element) = self.element(&block) {
            if self.schema.is_inline(&element.element_type) {
                block = path::parent(&block)?;
            } else {
                break;
            }
        }
        if block.is_empty() {
            None
        } else {
            Some(block)
        }
    }

    // ---------------------------------------------------------------------
    // Generic node transforms
    // ---------------------------------------------------------------------

    pub fn insert_node(&mut self, at: &[usize], node: Node) {
        self.transaction(|editor| {
            let _ = editor.apply(Operation::InsertNode {
                path: at.to_vec(),
                node,
            });
        });
    }

    pub fn insert_nodes(&mut self, at: &[usize], nodes: Vec<Node>) {
        self.transaction(|editor| {
            let index = at.last().copied().unwrap_or(0);
            let parent = at[..at.len().saturating_sub(1)].to_vec();
            for (offset, node) in nodes.into_iter().enumerate() {
                let mut path = parent.clone();
                path.push(index + offset);
                let _ = editor.apply(Operation::InsertNode { path, node });
            }
        });
    }

    pub fn remove_nodes(&mut self, at: &[usize], count: usize) {
        self.transaction(|editor| {
            let index = at.last().copied().unwrap_or(0);
            let parent = at[..at.len().saturating_sub(1)].to_vec();
            for offset in (0..count).rev() {
                let mut path = parent.clone();
                path.push(index + offset);
                let _ = editor.apply(Operation::RemoveNode { path });
            }
        });
    }

    pub fn merge_nodes(&mut self, at: &[usize]) {
        self.transaction(|editor| {
            let _ = editor.apply_merge(at);
        });
    }

    pub fn split_nodes(&mut self, at: &[usize], position: usize) {
        self.transaction(|editor| {
            let _ = editor.apply_split(at, position, Props::new());
        });
    }

    pub fn move_nodes(&mut self, from: &[usize], to: &[usize]) {
        self.transaction(|editor| {
            let _ = editor.apply(Operation::MoveNode {
                path: from.to_vec(),
                new_path: to.to_vec(),
            });
        });
    }

    pub fn set_nodes(&mut self, at: &[&[usize]], properties: Props) {
        self.transaction(|editor| {
            for path in at {
                let _ = editor.apply(Operation::SetNode {
                    path: path.to_vec(),
                    properties: Props::new(),
                    new_properties: properties.clone(),
                });
            }
        });
    }

    pub fn unset_nodes(&mut self, at: &[&[usize]], keys: &[&str]) {
        self.transaction(|editor| {
            let mut properties = Props::new();
            for key in keys {
                properties.insert((*key).to_string(), Value::Null);
            }
            for path in at {
                let _ = editor.apply(Operation::SetNode {
                    path: path.to_vec(),
                    properties: Props::new(),
                    new_properties: properties.clone(),
                });
            }
        });
    }

    /// Insert text at an explicit point.
    pub fn insert_text_at(&mut self, at: &Point, text: &str) {
        self.transaction(|editor| {
            editor.clear_node_selection();
            editor.selection = Some(Range::collapsed(at.clone()));
            editor.insert_text_inner(text);
        });
    }

    /// Insert a parsed fragment at the current selection.
    ///
    /// A single paragraph-shaped block is merged into the current text so
    /// inline marks and links survive. Multi-block fragments split the current
    /// top-level block and insert between the two halves.
    pub fn insert_fragment(&mut self, nodes: Vec<Node>) {
        if nodes.is_empty() {
            return;
        }

        let inline_children = if nodes.len() == 1 {
            match &nodes[0] {
                Node::Element(element) if element.element_type == "p" => {
                    let inline = element.children.iter().all(|child| {
                        child.is_text()
                            || child
                                .element_type()
                                .map(|kind| self.schema.is_inline(kind))
                                .unwrap_or(false)
                    });
                    inline.then(|| element.children.clone())
                }
                _ => None,
            }
        } else {
            None
        };

        self.transaction(|editor| {
            if editor.node_selection.is_some() {
                editor.delete_selected_node_inner();
            }
            if editor
                .selection
                .as_ref()
                .map(|range| !range.is_collapsed())
                .unwrap_or(false)
            {
                editor.delete_selection_inner();
            }

            if let Some(children) = inline_children {
                editor.insert_inline_children_inner(children);
            } else {
                editor.insert_block_fragment_inner(nodes);
            }
        });
    }

    pub fn insert_html(&mut self, html: &str) {
        let nodes = crate::fragment::parse_html(html);
        self.insert_fragment(nodes);
    }

    pub fn insert_markdown(&mut self, markdown: &str) {
        let nodes = crate::fragment::parse_markdown(markdown);
        self.insert_fragment(nodes);
    }

    fn insert_inline_children_inner(&mut self, children: Vec<Node>) {
        if children.is_empty() {
            return;
        }

        let Some(point) = self
            .selection
            .as_ref()
            .map(|range| range.anchor.clone())
            .or_else(|| self.end_point())
        else {
            return;
        };
        let Some(point) = self.text_point(&point) else {
            return;
        };
        if point.path.len() < 2 {
            return;
        }

        let parent = point.path[..point.path.len() - 1].to_vec();
        let index = *point.path.last().unwrap();
        let split = point.offset > 0;
        if split
            && self
                .apply_split(&point.path, point.offset, Props::new())
                .is_err()
        {
            return;
        }

        let start = if split { index + 1 } else { index };
        let count = children.len();
        for (insert_at, child) in (start..).zip(children) {
            let mut path = parent.clone();
            path.push(insert_at);
            if self
                .apply(Operation::InsertNode { path, node: child })
                .is_err()
            {
                return;
            }
        }

        let after_path = child_path(&parent, start + count);
        if let Some(point) = self.first_text_point_in(&after_path) {
            self.selection = Some(Range::collapsed(point));
        }
    }

    fn insert_block_fragment_inner(&mut self, nodes: Vec<Node>) {
        if nodes.is_empty() {
            return;
        }
        let Some(point) = self
            .selection
            .as_ref()
            .map(|range| range.anchor.clone())
            .or_else(|| self.end_point())
        else {
            return;
        };
        let Some(point) = self.text_point(&point) else {
            return;
        };
        if point.path.is_empty() {
            return;
        }

        let top = path::parent(&point.path).and_then(|parent| parent.first().copied());
        let Some(top) = top else {
            return;
        };

        let at_start = point.offset == 0 && point.path[1..].iter().all(|index| *index == 0);
        let count = nodes.len();
        let (insert_at, caret_path) = if at_start {
            (top, top + count)
        } else {
            let Some(tail) = self.split_top_level_at_point(&point) else {
                return;
            };
            (tail, tail + count)
        };

        for (offset, node) in nodes.into_iter().enumerate() {
            if self
                .apply(Operation::InsertNode {
                    path: vec![insert_at + offset],
                    node,
                })
                .is_err()
            {
                return;
            }
        }

        if let Some(point) = self.first_text_point_in(&[caret_path]) {
            self.selection = Some(Range::collapsed(point));
        }
    }

    fn split_top_level_at_point(&mut self, point: &Point) -> Option<usize> {
        let leaf = point.path.clone();
        self.apply_split(&leaf, point.offset, Props::new()).ok()?;
        let mut caret = Point::new(path::next(&leaf), 0);
        self.selection = Some(Range::collapsed(caret.clone()));

        for depth in (1..leaf.len()).rev() {
            let ancestor = leaf[..depth].to_vec();
            let position = caret.path.get(depth).copied()?;
            self.apply_split(&ancestor, position, Props::new()).ok()?;
            caret = self.selection.as_ref()?.anchor.clone();
        }

        caret.path.first().copied()
    }

    /// Insert an inline node at the caret and move the caret into the text
    /// after it. Used for mentions, inline embeds and other atoms.
    pub fn insert_inline_node(&mut self, node: Node) -> Option<Path> {
        let mut inserted = None;

        self.transaction(|editor| {
            let is_inline = match &node {
                Node::Element(element) => editor.schema.is_inline(&element.element_type),
                Node::Text(_) => false,
            };
            if !is_inline {
                return None;
            }

            if editor.node_selection.is_some() {
                editor.delete_selected_node_inner();
            }
            if editor
                .selection
                .as_ref()
                .map(|range| !range.is_collapsed())
                .unwrap_or(false)
            {
                editor.delete_selection_inner();
            }

            let point = editor
                .selection
                .as_ref()
                .map(|range| range.anchor.clone())
                .or_else(|| editor.end_point())?;
            let point = editor.text_point(&point)?;
            if point.path.len() < 2 {
                return None;
            }

            let parent = point.path[..point.path.len() - 1].to_vec();
            if editor
                .element(&parent)
                .map(|element| editor.schema.is_inline(&element.element_type))
                .unwrap_or(false)
            {
                return None;
            }

            let index = *point.path.last()?;
            let after_path;

            if point.offset > 0 {
                editor
                    .apply_split(&point.path, point.offset, Props::new())
                    .ok()?;
                let insert_at = child_path(&parent, index + 1);
                editor
                    .apply(Operation::InsertNode {
                        path: insert_at.clone(),
                        node,
                    })
                    .ok()?;
                inserted = Some(insert_at);
                after_path = child_path(&parent, index + 2);
            } else {
                let insert_at = child_path(&parent, index);
                editor
                    .apply(Operation::InsertNode {
                        path: insert_at.clone(),
                        node,
                    })
                    .ok()?;
                inserted = Some(insert_at);
                after_path = child_path(&parent, index + 1);
            }

            if let Some(point) = editor.first_text_point_in(&after_path) {
                editor.selection = Some(Range::collapsed(point));
            }

            Some(())
        });

        inserted
    }

    /// Replace a text range with an inline node. Used when accepting a mention
    /// from an `@query` trigger.
    pub fn replace_range_with_inline_node(&mut self, range: Range, node: Node) -> Option<Path> {
        self.transaction(|editor| {
            editor.clear_node_selection();
            editor.selection = Some(range);
            editor.insert_inline_node(node)
        })
    }

    /// Place the caret at the start of the text following a node.
    pub fn select_after_node(&mut self, at: &[usize]) -> Option<Point> {
        let next = path::next(at);
        let point = self.first_text_point_in(&next)?;
        self.select_point(point.clone());
        Some(point)
    }

    /// Replace the contents of one text node with `text`, applying the
    /// smallest remove/insert pair possible. Used to reconcile IME input,
    /// where the browser edits the DOM directly.
    pub fn set_text_at_path(&mut self, at: &[usize], text: &str) {
        self.transaction(|editor| {
            let Some(old) = editor.text(at).map(|node| node.text.clone()) else {
                return;
            };
            if old == text {
                return;
            }

            let old_chars: Vec<char> = old.chars().collect();
            let new_chars: Vec<char> = text.chars().collect();
            let mut prefix = 0;
            while prefix < old_chars.len()
                && prefix < new_chars.len()
                && old_chars[prefix] == new_chars[prefix]
            {
                prefix += 1;
            }
            let mut suffix = 0;
            while suffix < old_chars.len().saturating_sub(prefix)
                && suffix < new_chars.len().saturating_sub(prefix)
                && old_chars[old_chars.len() - 1 - suffix]
                    == new_chars[new_chars.len() - 1 - suffix]
            {
                suffix += 1;
            }

            let remove: String = old_chars[prefix..old_chars.len() - suffix].iter().collect();
            let insert: String = new_chars[prefix..new_chars.len() - suffix].iter().collect();

            if !remove.is_empty() {
                let _ = editor.apply(Operation::RemoveText {
                    path: at.to_vec(),
                    offset: prefix,
                    text: remove,
                });
            }
            if !insert.is_empty() {
                let _ = editor.apply(Operation::InsertText {
                    path: at.to_vec(),
                    offset: prefix,
                    text: insert,
                });
            }
        });
    }

    /// Apply an operation inside a fresh transaction.
    pub fn apply_operation(&mut self, operation: Operation) -> Result<(), SlateError> {
        self.transaction(|editor| editor.apply(operation))
    }
}

impl Editor {
    /// Move a top-level block to a final index. `to` is the index in the
    /// post-removal list (0..len-1).
    pub fn move_block(&mut self, from: usize, to: usize) -> bool {
        self.transaction(|editor| {
            let len = editor.children.len();
            if from >= len || to >= len || from == to {
                return false;
            }
            editor
                .apply(Operation::MoveNode {
                    path: vec![from],
                    new_path: vec![to],
                })
                .is_ok()
        })
    }

    /// Insert a block immediately after an explicit block path.
    pub fn insert_block_after(&mut self, at: &[usize], node: Node) -> Option<Path> {
        let mut inserted = None;

        self.transaction(|editor| {
            if at.is_empty() {
                return None;
            }
            let parent = at[..at.len() - 1].to_vec();
            let index = *at.last()?;
            let is_void = match &node {
                Node::Element(element) => editor.schema.is_void(&element.element_type),
                Node::Text(_) => false,
            };
            let node = match node {
                Node::Element(mut element) => {
                    if element.children.is_empty() && !is_void {
                        element.children.push(Node::empty_text());
                    }
                    Node::Element(element)
                }
                node => node,
            };

            let insert_at = child_path(&parent, index + 1);
            editor.clear_node_selection();
            editor
                .apply(Operation::InsertNode {
                    path: insert_at.clone(),
                    node,
                })
                .ok()?;
            inserted = Some(insert_at.clone());

            if is_void {
                let next_at = child_path(&parent, index + 2);
                if let Some(point) = editor.first_text_point_in(&next_at) {
                    editor.selection = Some(Range::collapsed(point));
                }
            } else if let Some(point) = editor.first_text_point_in(&insert_at) {
                editor.selection = Some(Range::collapsed(point));
            }

            Some(())
        });

        inserted
    }

    /// Insert a block immediately after the block containing the selection.
    ///
    /// Returns the path of the inserted block. If the node is a void block, a
    /// following paragraph is created (or reused) and the caret moves there, so
    /// the user can keep typing.
    pub fn insert_block_after_selection(&mut self, node: Node) -> Option<Path> {
        let mut inserted = None;

        self.transaction(|editor| {
            let selected_blocks = editor.node_selections();
            let mut block = if let Some(block) = selected_blocks.last() {
                let block = block.clone();
                editor.clear_node_selection();
                block
            } else {
                let point = editor
                    .selection
                    .as_ref()
                    .map(|range| range.start().clone())
                    .or_else(|| editor.end_point())?;
                editor.leaf_block_path(&point)?
            };
            // A block inserted from the toolbar should not land inside a list.
            // Climb out of list ancestors and insert after the list instead.
            while block.len() > 1 {
                let parent = block[..block.len() - 1].to_vec();
                match editor.element(&parent) {
                    Some(element) if editor.schema.is_list(&element.element_type) => {
                        block = parent;
                    }
                    _ => break,
                }
            }
            let parent = block[..block.len() - 1].to_vec();
            let index = *block.last()?;

            let is_void = match &node {
                Node::Element(element) => editor.schema.is_void(&element.element_type),
                Node::Text(_) => false,
            };

            let node = match node {
                Node::Element(mut element) => {
                    if element.children.is_empty() && !is_void {
                        element.children.push(Node::empty_text());
                    }
                    Node::Element(element)
                }
                node => node,
            };

            let insert_at = child_path(&parent, index + 1);
            editor
                .apply(Operation::InsertNode {
                    path: insert_at.clone(),
                    node,
                })
                .ok()?;
            inserted = Some(insert_at.clone());

            if is_void {
                let next_at = child_path(&parent, index + 2);
                let next_is_textual = editor
                    .node(&next_at)
                    .and_then(Node::as_element)
                    .map(|element| {
                        !editor.schema.is_void(&element.element_type)
                            && !editor.schema.is_inline(&element.element_type)
                    })
                    .unwrap_or(false);

                if !next_is_textual {
                    let paragraph = Node::Element(Element::empty(&editor.schema.default_block));
                    editor
                        .apply(Operation::InsertNode {
                            path: next_at.clone(),
                            node: paragraph,
                        })
                        .ok()?;
                }

                if let Some(point) = editor.first_text_point_in(&next_at) {
                    editor.selection = Some(Range::collapsed(point));
                }
            } else if let Some(point) = editor.first_text_point_in(&insert_at) {
                editor.selection = Some(Range::collapsed(point));
            }

            Some(())
        });

        inserted
    }

    /// Remove the block at `at`, moving the caret to a neighbouring block when
    /// possible. Used by custom block toolbars (for example, deleting an image).
    pub fn remove_block_at(&mut self, at: &[usize]) -> Option<Point> {
        self.transaction(|editor| editor.remove_block_at_inner(at));
        if self.selection.is_none() {
            self.selection = self.start_point().map(Range::collapsed);
        }
        self.selection.as_ref().map(|range| range.anchor.clone())
    }

    fn remove_block_at_inner(&mut self, at: &[usize]) -> Option<Point> {
        if at.is_empty() || self.node(at).is_none() {
            return None;
        }
        self.clear_node_selection();

        let parent = at[..at.len() - 1].to_vec();
        let index = *at.last()?;

        let next = child_path(&parent, index + 1);
        let previous = index.checked_sub(1).map(|i| child_path(&parent, i));

        let is_void = |path: &[usize]| {
            self.node(path)
                .and_then(Node::as_element)
                .map(|element| self.schema.is_void(&element.element_type))
                .unwrap_or(false)
        };

        let target = (!is_void(&next))
            .then(|| self.first_text_point_in(&next))
            .flatten()
            .or_else(|| {
                previous
                    .filter(|path| !is_void(path))
                    .and_then(|path| self.last_text_point_in(&path))
            });

        if let Some(point) = target {
            self.selection = Some(Range::collapsed(point));
        } else {
            self.selection = None;
        }

        self.apply(Operation::RemoveNode { path: at.to_vec() })
            .ok()?;
        self.selection.as_ref().map(|range| range.anchor.clone())
    }

    pub(crate) fn delete_selected_node_inner(&mut self) -> bool {
        let selected = self.node_selections();
        let (Some(first), Some(last)) = (
            selected.first().and_then(|path| path.first().copied()),
            selected.last().and_then(|path| path.first().copied()),
        ) else {
            return false;
        };
        self.clear_node_selection();

        let is_void = |path: &[usize]| {
            self.node(path)
                .and_then(Node::as_element)
                .map(|element| self.schema.is_void(&element.element_type))
                .unwrap_or(true)
        };
        let next = last + 1;
        let target = if next < self.children.len() && !is_void(&[next]) {
            self.first_text_point_in(&[next])
        } else {
            None
        }
        .or_else(|| {
            first
                .checked_sub(1)
                .filter(|previous| !is_void(&[*previous]))
                .and_then(|previous| self.last_text_point_in(&[previous]))
        });
        self.selection = target.map(Range::collapsed);

        for index in (first..=last).rev() {
            let _ = self.apply(Operation::RemoveNode { path: vec![index] });
        }
        true
    }

    /// Set properties on a single node.
    pub fn set_node_properties(&mut self, at: &[usize], properties: Props) {
        self.transaction(|editor| {
            let _ = editor.apply(Operation::SetNode {
                path: at.to_vec(),
                properties: Props::new(),
                new_properties: properties,
            });
        });
    }

    /// Remove properties from a single node.
    pub fn unset_node_properties(&mut self, at: &[usize], keys: &[&str]) {
        self.transaction(|editor| {
            let mut properties = Props::new();
            for key in keys {
                properties.insert((*key).to_string(), Value::Null);
            }
            let _ = editor.apply(Operation::SetNode {
                path: at.to_vec(),
                properties: Props::new(),
                new_properties: properties,
            });
        });
    }
}

fn child_path(parent: &[usize], index: usize) -> Path {
    let mut path = parent.to_vec();
    path.push(index);
    path
}

fn collect_table_cells(
    nodes: &[Node],
    parent: &[usize],
    schema: &crate::Schema,
    cells: &mut Vec<Path>,
) {
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

fn collect_table_rows(
    nodes: &[Node],
    parent: &[usize],
    schema: &crate::Schema,
    rows: &mut Vec<Path>,
) {
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
        if schema.is_table(&element.element_type) || schema.is_table_section(&element.element_type)
        {
            collect_table_rows(&element.children, &path, schema, rows);
        }
    }
}

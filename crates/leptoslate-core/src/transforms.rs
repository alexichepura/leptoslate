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
            .leaf_block_path(&point)
            .map(|block| block.len())
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
            editor.node_selection = None;
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

    /// The leaf block paths intersecting the current selection.
    pub fn blocks_in_selection(&self) -> Vec<Path> {
        if let Some(node) = &self.node_selection {
            return vec![node.clone()];
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
            editor.node_selection = None;
            editor.selection = Some(Range::collapsed(at.clone()));
            editor.insert_text_inner(text);
        });
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
    /// Insert a block immediately after the block containing the selection.
    ///
    /// Returns the path of the inserted block. If the node is a void block, a
    /// following paragraph is created (or reused) and the caret moves there, so
    /// the user can keep typing.
    pub fn insert_block_after_selection(&mut self, node: Node) -> Option<Path> {
        let mut inserted = None;

        self.transaction(|editor| {
            let selected_node = editor.node_selection.take();
            let mut block = if let Some(node) = selected_node {
                node
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
        self.node_selection = None;

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
        let Some(path) = self.node_selection.take() else {
            return false;
        };
        self.remove_block_at_inner(&path);
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

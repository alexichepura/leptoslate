//! High-level transforms: the Rust equivalent of Slate's `Transforms` and
//! `Editor` mutation helpers.
//!
//! All public methods group their work in one undoable transaction and
//! normalize the document afterwards.

use serde_json::Value;

use crate::{
    node::{Element, Node, Text},
    operation::Operation,
    path::{self, Affinity, Path},
    point::Point,
    props::Props,
    range::{Location, Range},
    text::{
        char_len, char_slice, grapheme_end_after, grapheme_start_before, word_end_after,
        word_start_before,
    },
    SlateError,
};

/// Which nodes `splitNodes` should consider when it looks for a matching
/// ancestor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeMatch {
    Block,
    Inline,
    Text,
    TextOrInline,
    Any,
}

/// Whether `splitNodes` should stop at the lowest or highest match.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MatchMode {
    Lowest,
    Highest,
    All,
}

#[derive(Clone, Debug)]
pub struct SplitNodesOptions {
    pub at: Option<Location>,
    pub match_kind: NodeMatch,
    pub mode: MatchMode,
    pub height: usize,
    pub always: bool,
    pub voids: bool,
    pub select: Option<bool>,
    /// Keep the pre-split selection when `select` is false. Wrap/unwrap
    /// disable this so they can track the transformed range themselves.
    pub preserve_selection: bool,
    /// Do not overwrite the existing selection with the split point. The
    /// selection is transformed by the split operations instead.
    pub keep_selection: bool,
}

#[derive(Clone, Debug)]
pub struct InsertNodesOptions {
    pub at: Option<Location>,
    pub match_kind: Option<NodeMatch>,
    pub mode: MatchMode,
    pub voids: bool,
    pub select: Option<bool>,
}

#[derive(Clone, Debug)]
pub struct WrapNodesOptions {
    pub at: Option<Location>,
    pub match_kind: Option<NodeMatch>,
    pub match_property: Option<String>,
    pub mode: MatchMode,
    pub split: bool,
    pub hanging: bool,
    pub voids: bool,
}

impl Default for WrapNodesOptions {
    fn default() -> Self {
        Self {
            at: None,
            match_kind: None,
            match_property: None,
            mode: MatchMode::Lowest,
            split: false,
            hanging: false,
            voids: false,
        }
    }
}

#[derive(Clone, Debug)]
pub struct UnwrapNodesOptions {
    pub at: Option<Location>,
    pub match_kind: Option<NodeMatch>,
    pub match_property: Option<String>,
    pub mode: MatchMode,
    pub split: bool,
    pub hanging: bool,
    pub voids: bool,
}

impl Default for UnwrapNodesOptions {
    fn default() -> Self {
        Self {
            at: None,
            match_kind: None,
            match_property: None,
            mode: MatchMode::Lowest,
            split: false,
            hanging: false,
            voids: false,
        }
    }
}

#[derive(Clone, Debug)]
pub struct SetNodesOptions {
    pub at: Option<Location>,
    pub match_kind: Option<NodeMatch>,
    pub match_property: Option<String>,
    pub mode: MatchMode,
    pub replace: bool,
    pub split: bool,
    pub hanging: bool,
    pub voids: bool,
}

impl Default for SetNodesOptions {
    fn default() -> Self {
        Self {
            at: None,
            match_kind: None,
            match_property: None,
            mode: MatchMode::Lowest,
            replace: false,
            split: false,
            hanging: false,
            voids: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeleteUnit {
    Character,
    Word,
    Line,
    Block,
}

#[derive(Clone, Debug)]
pub struct DeleteTextOptions {
    pub reverse: bool,
    pub unit: DeleteUnit,
    pub distance: usize,
    pub voids: bool,
    pub at: Option<Location>,
    pub hanging: bool,
}

impl Default for DeleteTextOptions {
    fn default() -> Self {
        Self {
            reverse: false,
            unit: DeleteUnit::Character,
            distance: 1,
            voids: false,
            at: None,
            hanging: false,
        }
    }
}

impl Default for InsertNodesOptions {
    fn default() -> Self {
        Self {
            at: None,
            match_kind: None,
            mode: MatchMode::Lowest,
            voids: false,
            select: None,
        }
    }
}

impl Default for SplitNodesOptions {
    fn default() -> Self {
        Self {
            at: None,
            match_kind: NodeMatch::Block,
            mode: MatchMode::Lowest,
            height: 0,
            always: false,
            voids: false,
            select: None,
            preserve_selection: true,
            keep_selection: false,
        }
    }
}

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
        if let Some(point) = self
            .selection
            .as_ref()
            .map(|range| range.anchor.clone())
            .and_then(|point| self.text_point(&point))
        {
            if self.point_is_read_only(&point) {
                return;
            }
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
        if self.point_is_read_only(&point) {
            return;
        }
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
        if self.point_is_read_only(&point) {
            return;
        }

        if self
            .apply_split(&point.path, point.offset, Props::new())
            .is_err()
        {
            return;
        }

        let leaf: Path = point.path.clone();
        let mut caret = Point::new(path::next(&leaf), 0);
        self.selection = Some(Range::collapsed(caret.clone()));

        // Split up to and including the leaf block. A feature may stop the
        // split earlier: a list item is the leaf block of its text, so pressing
        // Enter creates a sibling `<li>` rather than splitting the whole list;
        // paragraphs inside a blockquote stay inside that blockquote.
        let leaf_depth = self
            .leaf_block_path(&point)
            .map(|block| block.len())
            .unwrap_or(1);
        let stop_depth = self
            .features
            .snapshot()
            .iter()
            .filter_map(|feature| feature.split_stop_depth(self, &point))
            .min()
            .map(|depth| depth.min(leaf_depth))
            .unwrap_or(leaf_depth);

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

    /// Slate-compatible `Transforms.delete`.
    pub fn delete_text_with(&mut self, options: DeleteTextOptions) -> Option<Point> {
        self.transaction(|editor| editor.delete_text_with_inner(options))
    }

    fn delete_text_with_inner(&mut self, options: DeleteTextOptions) -> Option<Point> {
        let mut at = options
            .at
            .clone()
            .or_else(|| self.selection.clone().map(Location::Range))?;
        let mut from_point = false;

        if let Location::Range(range) = &at {
            if range.is_collapsed() {
                at = Location::Point(range.anchor.clone());
            }
        }

        if let Location::Point(point) = &at {
            from_point = true;
            let point = self.text_point(point).unwrap_or_else(|| point.clone());

            if options.unit == DeleteUnit::Character && options.distance == 1 {
                if let Some(void_path) = self.adjacent_inline_void(&point, options.reverse) {
                    let parent = void_path[..void_path.len() - 1].to_vec();
                    let index = *void_path.last().unwrap();
                    let before = if options.reverse {
                        index
                            .checked_sub(1)
                            .and_then(|index| self.last_text_point_in(&child_path(&parent, index)))
                    } else {
                        None
                    };
                    self.remove_nodes(&void_path, 1);
                    let target = before
                        .or_else(|| self.first_text_point_in(&child_path(&parent, index)))
                        .or_else(|| self.end_point());
                    if let Some(target) = target {
                        self.selection = Some(Range::collapsed(target.clone()));
                        return Some(target);
                    }
                    return None;
                }
            }

            if !options.voids && options.unit == DeleteUnit::Character && options.distance == 1 {
                if let Some(inline) = self.empty_inline_to_delete(&point, options.reverse) {
                    let parent = inline[..inline.len() - 1].to_vec();
                    let index = *inline.last().unwrap();
                    let before = index
                        .checked_sub(1)
                        .and_then(|index| self.last_text_point_in(&child_path(&parent, index)));
                    let after = self.first_text_point_in(&child_path(&parent, index + 1));
                    self.remove_nodes(&inline, 1);
                    let target = before.or_else(|| {
                        self.first_text_point_in(&child_path(&parent, index))
                            .or(after)
                    });
                    if let Some(target) = target {
                        self.selection = Some(Range::collapsed(target.clone()));
                        return Some(target);
                    }
                    return None;
                }
            }

            if !options.voids
                && matches!(options.unit, DeleteUnit::Character | DeleteUnit::Word)
                && options.distance == 1
            {
                if let Some((block, void)) = self.empty_block_next_to_void(&point, options.reverse)
                {
                    self.remove_nodes(&block, 1);
                    let target = if options.reverse {
                        self.last_text_point_in(&void)
                    } else {
                        self.first_text_point_in(&void)
                    };
                    if let Some(target) = target {
                        self.selection = Some(Range::collapsed(target.clone()));
                        return Some(target);
                    }
                    return None;
                }
            }

            if !options.voids {
                if let Some(void_path) = self.highest_void_ancestor(&point) {
                    at = Location::Path(void_path);
                }
            }

            if let Location::Point(point) = at.clone() {
                let target = if options.reverse {
                    self.point_before(&point, options.unit, options.distance)
                } else {
                    self.point_after(&point, options.unit, options.distance)
                };
                let Some(target) = target else {
                    return self.merge_at_block_boundary(&point, options.reverse);
                };
                at = if options.reverse {
                    Location::Range(Range::new(target, point))
                } else {
                    Location::Range(Range::new(point, target))
                };
            }
        }

        if let Location::Path(path) = &at {
            self.remove_nodes(path, 1);
            if self.children.is_empty() && options.at.is_none() {
                let block = Node::Element(Element::new("", vec![Node::empty_text()]));
                self.insert_nodes(&[0], vec![block]);
                self.selection = Some(Range::collapsed(Point::new(vec![0, 0], 0)));
                return Some(Point::new(vec![0, 0], 0));
            }
            if options.reverse {
                let point = path::previous(path)
                    .and_then(|previous| self.last_text_point_in(&previous))
                    .or_else(|| self.first_text_point_in(&[]));
                if let Some(point) = &point {
                    self.selection = Some(Range::collapsed(point.clone()));
                }
                return point;
            }
            return self.selection.as_ref().map(|range| range.anchor.clone());
        }

        let Location::Range(range) = at else {
            return None;
        };
        if range.is_collapsed() {
            return None;
        }

        if !options.voids && options.at.is_none() && !from_point {
            if let Some(point) = self.delete_hanging_blocks(&range) {
                return Some(point);
            }
        }

        let range = if options.voids {
            range
        } else {
            self.trim_non_editable_range_edges(range)
        };
        self.selection = Some(range);
        self.delete_selection_inner();
        self.selection.as_ref().map(|range| range.anchor.clone())
    }

    fn merge_at_block_boundary(&mut self, point: &Point, reverse: bool) -> Option<Point> {
        let block = self.leaf_block_path(point)?;
        let block_is_empty = self
            .element(&block)
            .is_some_and(|element| element.text_len() == 0);

        if reverse && block_is_empty {
            let previous = path::previous(&block)?;
            self.remove_nodes(&block, 1);
            let target = self.last_text_point_in(&previous)?;
            self.selection = Some(Range::collapsed(target.clone()));
            return Some(target);
        }

        if reverse {
            let previous = path::previous(&block);
            if let Some(previous) = &previous {
                let previous_is_empty = self
                    .element(previous)
                    .is_some_and(|element| element.text_len() == 0);
                if previous_is_empty {
                    self.remove_nodes(previous, 1);
                    let target = self
                        .first_text_point_in(&block)
                        .unwrap_or_else(|| point.clone());
                    self.selection = Some(Range::collapsed(target.clone()));
                    return Some(target);
                }
            }
        }

        let merge_path = if reverse { block } else { path::next(&block) };
        let Some(node) = self.node(&merge_path) else {
            return self.selection.as_ref().map(|range| range.anchor.clone());
        };
        let position = match node {
            Node::Text(text) => text.len(),
            Node::Element(element) => element.children.len(),
        };
        let operation = Operation::MergeNode {
            path: merge_path,
            position,
        };
        if self.apply(operation.clone()).is_err() {
            return self.selection.as_ref().map(|range| range.anchor.clone());
        }
        let point = point
            .transform(&operation, Affinity::Forward)
            .unwrap_or_else(|| point.clone());
        self.selection = Some(Range::collapsed(point.clone()));
        Some(point)
    }

    fn delete_hanging_blocks(&mut self, range: &Range) -> Option<Point> {
        let anchor = self
            .text_point(&range.anchor)
            .unwrap_or_else(|| range.anchor.clone());
        let focus = self
            .text_point(&range.focus)
            .unwrap_or_else(|| range.focus.clone());
        let anchor_block = self.leaf_block_path(&anchor)?;
        let focus_block = self.leaf_block_path(&focus)?;
        if anchor_block.len() != 1 || focus_block.len() != 1 {
            return None;
        }
        if self.first_text_point_in(&focus_block).as_ref() != Some(&focus) {
            return None;
        }
        if self.first_text_point_in(&anchor_block).as_ref() != Some(&anchor) {
            return None;
        }
        let anchor_index = anchor_block[0];
        let focus_index = focus_block[0];
        if focus_index <= anchor_index {
            return None;
        }

        let focus_block_empty = self
            .element(&focus_block)
            .is_some_and(|element| element.text_len() == 0 || self.is_void_element(element));
        let count = if focus_block_empty {
            focus_index - anchor_index
        } else {
            focus_index - anchor_index - 1
        };
        self.remove_nodes(&[anchor_index + 1], count);

        if let Some(text) = self.text(&anchor.path).cloned() {
            let removed = char_slice(&text.text, anchor.offset, text.len()).to_string();
            if !removed.is_empty() {
                let _ = self.apply(Operation::RemoveText {
                    path: anchor.path.clone(),
                    offset: anchor.offset,
                    text: removed,
                });
            }
        }

        let point = self
            .first_text_point_in(&[anchor_index])
            .unwrap_or_else(|| Point::new(vec![anchor_index, 0], 0));
        self.selection = Some(Range::collapsed(point.clone()));
        Some(point)
    }

    fn trim_non_editable_range_edges(&mut self, mut range: Range) -> Range {
        for _ in 0..16 {
            let start_ancestor = self.non_editable_ancestor(&range.anchor);
            let end_ancestor = self.non_editable_ancestor(&range.focus);

            let end_inside = end_ancestor.is_some();
            let start_inside = start_ancestor.is_some();
            let Some(path) = end_ancestor.or(start_ancestor) else {
                break;
            };
            let operation = Operation::RemoveNode { path: path.clone() };
            if self.apply(operation.clone()).is_err() {
                break;
            }

            let next_start = range.anchor.transform(&operation, Affinity::Forward);
            let next_end = range.focus.transform(&operation, Affinity::Forward);

            range.anchor = if start_inside {
                next_start
                    .or_else(|| self.first_text_point_in(&path))
                    .or_else(|| self.first_text_point_in(&[]))
                    .unwrap_or_else(|| range.anchor.clone())
            } else {
                next_start.unwrap_or_else(|| range.anchor.clone())
            };

            range.focus = if end_inside {
                if start_inside {
                    range.anchor.clone()
                } else {
                    let len = self.text_len_at(&range.anchor.path).unwrap_or(0);
                    Point::new(range.anchor.path.clone(), len)
                }
            } else {
                next_end.unwrap_or_else(|| range.focus.clone())
            };
        }
        range
    }

    fn non_editable_ancestor(&self, point: &Point) -> Option<Path> {
        (1..point.path.len()).find_map(|depth| {
            let path = point.path[..depth].to_vec();
            self.element(&path)
                .is_some_and(|element| {
                    self.is_void_element(element)
                        || matches!(element.properties.get("readOnly"), Some(Value::Bool(true)))
                })
                .then_some(path)
        })
    }

    fn adjacent_inline_void(&self, point: &Point, reverse: bool) -> Option<Path> {
        let parent = path::parent(&point.path)?;
        let index = *point.path.last()?;
        if !reverse && point.offset < self.text(&point.path)?.len() {
            return None;
        }
        if reverse && point.offset > 0 {
            return None;
        }
        let sibling_index = if reverse {
            index.checked_sub(1)?
        } else {
            index + 1
        };
        let sibling = child_path(&parent, sibling_index);
        self.element(&sibling)
            .filter(|element| self.is_inline_element(element) && self.is_void_element(element))
            .map(|_| sibling)
    }

    fn empty_inline_to_delete(&self, point: &Point, reverse: bool) -> Option<Path> {
        // Point inside an empty inline node.
        for depth in (1..point.path.len()).rev() {
            let path = &point.path[..depth];
            if let Some(element) = self.element(path) {
                if self.is_inline_element(element)
                    && !self.is_void_element(element)
                    && element.text_len() == 0
                {
                    return Some(path.to_vec());
                }
            }
        }

        let parent = path::parent(&point.path)?;
        let index = *point.path.last()?;
        let sibling_index = if reverse {
            index.checked_sub(1)?
        } else {
            index + 1
        };
        let sibling = child_path(&parent, sibling_index);
        self.element(&sibling)
            .filter(|element| {
                self.is_inline_element(element)
                    && !self.is_void_element(element)
                    && element.text_len() == 0
            })
            .map(|_| sibling)
    }

    fn empty_block_next_to_void(&self, point: &Point, reverse: bool) -> Option<(Path, Path)> {
        let block = self.leaf_block_path(point)?;
        let is_empty = self
            .element(&block)
            .is_some_and(|element| element.text_len() == 0);
        if !is_empty {
            return None;
        }
        let sibling = if reverse {
            path::previous(&block)?
        } else {
            path::next(&block)
        };
        let sibling_is_void = self
            .element(&sibling)
            .is_some_and(|element| self.is_void_element(element));
        sibling_is_void.then_some((block, sibling))
    }

    fn highest_void_ancestor(&self, point: &Point) -> Option<Path> {
        (1..point.path.len()).find_map(|depth| {
            let path = point.path[..depth].to_vec();
            self.element(&path)
                .is_some_and(|element| self.is_void_element(element))
                .then_some(path)
        })
    }

    fn point_after(&self, point: &Point, unit: DeleteUnit, distance: usize) -> Option<Point> {
        let mut point = point.clone();
        for _ in 0..distance {
            point = match unit {
                DeleteUnit::Character => self.step_character(&point, true)?,
                DeleteUnit::Word => self.step_word(&point, true)?,
                DeleteUnit::Line => self.step_line(&point, true)?,
                DeleteUnit::Block => self.step_block(&point, true)?,
            };
        }
        Some(point)
    }

    fn point_before(&self, point: &Point, unit: DeleteUnit, distance: usize) -> Option<Point> {
        let mut point = point.clone();
        for _ in 0..distance {
            point = match unit {
                DeleteUnit::Character => self.step_character(&point, false)?,
                DeleteUnit::Word => self.step_word(&point, false)?,
                DeleteUnit::Line => self.step_line(&point, false)?,
                DeleteUnit::Block => self.step_block(&point, false)?,
            };
        }
        Some(point)
    }

    fn step_character(&self, point: &Point, forward: bool) -> Option<Point> {
        let mut point = point.clone();
        loop {
            let text = self.text(&point.path)?;
            let len = text.len();
            if forward {
                if point.offset < len {
                    return Some(Point::new(
                        point.path.clone(),
                        grapheme_end_after(&text.text, point.offset),
                    ));
                }
                point = self.next_text_point(&point)?;
                if self.text(&point.path).is_some_and(|text| text.is_empty()) {
                    continue;
                }
                return Some(point);
            } else if point.offset > 0 {
                return Some(Point::new(
                    point.path.clone(),
                    grapheme_start_before(&text.text, point.offset),
                ));
            } else {
                point = self.previous_text_point(&point)?;
                if self.text(&point.path).is_some_and(|text| text.is_empty()) {
                    continue;
                }
                return Some(point);
            }
        }
    }

    fn step_word(&self, point: &Point, forward: bool) -> Option<Point> {
        let text = self.text(&point.path)?;
        let len = text.len();
        if forward {
            if point.offset < len {
                let offset = word_end_after(&text.text, point.offset);
                if offset > point.offset {
                    return Some(Point::new(point.path.clone(), offset));
                }
            }
            self.next_text_point(point)
        } else if point.offset > 0 {
            let offset = word_start_before(&text.text, point.offset);
            if offset < point.offset {
                Some(Point::new(point.path.clone(), offset))
            } else {
                self.previous_text_point(point)
            }
        } else {
            self.previous_text_point(point)
        }
    }

    fn step_line(&self, point: &Point, forward: bool) -> Option<Point> {
        let text = self.text(&point.path)?;
        Some(Point::new(
            point.path.clone(),
            if forward { text.len() } else { 0 },
        ))
    }

    fn step_block(&self, point: &Point, forward: bool) -> Option<Point> {
        let block = self.leaf_block_path(point)?;
        if forward {
            let next = path::next(&block);
            self.first_text_point_in(&next)
        } else {
            let previous = path::previous(&block)?;
            self.last_text_point_in(&previous)
        }
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
        let mut start = self.text_point(&start).unwrap_or(start);
        let mut end = self.text_point(&end).unwrap_or(end);

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
                end.offset = 0;
            }
        }

        // Remove everything between the two branches and merge the remaining
        // branches repeatedly until both points land in the same text node.
        let mut guard = 0;
        while start.path != end.path && guard < 64 {
            guard += 1;
            let common = path::common(&start.path, &end.path);
            let start_branch = start.path.get(common.len()).copied().unwrap_or(0);
            let end_branch = end.path.get(common.len()).copied().unwrap_or(0);

            if end_branch > start_branch + 1 {
                for index in (start_branch + 1..end_branch).rev() {
                    let removed = child_path(&common, index);
                    let operation = Operation::RemoveNode {
                        path: removed.clone(),
                    };
                    if self.apply(operation.clone()).is_err() {
                        break;
                    }
                    if let Some(next) = start.transform(&operation, Affinity::Forward) {
                        start = next;
                    }
                    if let Some(next) = end.transform(&operation, Affinity::Forward) {
                        end = next;
                    }
                }
            }

            let previous = child_path(&common, start_branch);
            let merge_at = child_path(&common, start_branch + 1);
            let position = match self.node(&previous) {
                Some(Node::Text(text)) => text.len(),
                Some(Node::Element(element)) => element.children.len(),
                None => break,
            };
            let operation = Operation::MergeNode {
                path: merge_at.clone(),
                position,
            };
            if self.apply(operation.clone()).is_err() {
                break;
            }
            let (Some(next_start), Some(next_end)) = (
                start.transform(&operation, Affinity::Forward),
                end.transform(&operation, Affinity::Forward),
            ) else {
                break;
            };
            start = next_start;
            end = next_end;
        }

        if start.path == end.path && end.offset > start.offset {
            if let Some(text) = self.text(&start.path) {
                let removed = char_slice(&text.text, start.offset, end.offset).to_string();
                let _ = self.apply(Operation::RemoveText {
                    path: start.path.clone(),
                    offset: start.offset,
                    text: removed,
                });
            }
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
        if self.point_is_read_only(&point) {
            return;
        }
        let Some(text) = self.text(&point.path).cloned() else {
            return;
        };

        if point.offset > 0 {
            let start = if word {
                word_start_before(&text.text, point.offset)
            } else {
                grapheme_start_before(&text.text, point.offset)
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
            let features = self.features.snapshot();
            if features
                .iter()
                .any(|feature| feature.handle_backspace(self, &point))
            {
                return;
            }
        }

        // Backspace immediately after an inline void removes the atom.
        if let Some(previous) = path::previous(&point.path) {
            let is_inline_void = self
                .element(&previous)
                .map(|element| self.is_inline_element(element) && self.is_void_element(element))
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
                        grapheme_start_before(&text.text, previous_point.offset)
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
        if self.point_is_read_only(&point) {
            return;
        }
        let Some(text) = self.text(&point.path).cloned() else {
            return;
        };

        if point.offset < text.len() {
            let end = if word {
                word_end_after(&text.text, point.offset)
            } else {
                grapheme_end_after(&text.text, point.offset)
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
            .map(|element| self.is_inline_element(element) && self.is_void_element(element))
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
                            grapheme_end_after(&text.text, 0)
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
                } else if let Some(point) = editor.text_point(&range.anchor) {
                    if editor.point_has_void_ancestor(&point)
                        && editor.is_text_markable(&point.path)
                    {
                        let mut add = Props::new();
                        add.insert(key.to_string(), value);
                        let _ = editor.apply(Operation::SetNode {
                            path: point.path,
                            properties: Props::new(),
                            new_properties: add,
                        });
                    }
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
    // Inline elements and links
    // ---------------------------------------------------------------------

    /// Wrap the selected text in an inline element, or update the link at the
    /// current selection when it is already inside one.
    ///
    /// Returns `false` when the selection is collapsed outside a link, or spans
    /// multiple blocks (inline elements must stay within one block).
    pub fn set_link(&mut self, href: &str) -> bool {
        let href = href.trim();
        if href.is_empty() || !crate::is_safe_href(href) {
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
            let property = editor.link_property.clone();

            if let Some(link) = editor.inline_ancestor(&start.path, "a") {
                if range.is_collapsed()
                    || editor.inline_ancestor(&end.path, "a").as_ref() == Some(&link)
                {
                    let Some(link_properties) = editor
                        .element(&link)
                        .map(|element| element.properties.clone())
                    else {
                        return false;
                    };
                    let properties =
                        property.with_value(&link_properties, Value::String(href.to_string()));
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
            property.set(&mut properties, Value::String(href.to_string()));
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

    /// Update the link at the selection.
    ///
    /// When `text` is unchanged the existing children and marks are preserved.
    /// Otherwise the link text is replaced with a single text node carrying the
    /// first child text node's marks. Empty link text is rejected.
    pub fn update_link_at_selection(&mut self, href: &str, text: &str) -> bool {
        let href = href.trim();
        if href.is_empty() || text.is_empty() || !crate::is_safe_href(href) {
            return false;
        }
        let Some(range) = self.selection.clone() else {
            return false;
        };
        let start = self
            .text_point(range.start())
            .unwrap_or(range.start().clone());
        let Some(link) = self.inline_ancestor(&start.path, "a") else {
            return false;
        };
        let Some(link_element) = self.element(&link).cloned() else {
            return false;
        };
        let current_text = link_element
            .children
            .iter()
            .map(Node::string)
            .collect::<String>();
        let marks = link_element
            .children
            .iter()
            .find_map(|child| child.as_text().map(|text| text.properties.clone()))
            .unwrap_or_default();
        let link_path = link.clone();
        let replace_text = current_text != text;
        let properties = self
            .link_property
            .with_value(&link_element.properties, Value::String(href.to_string()));

        self.transaction(|editor| {
            if editor
                .apply(Operation::SetNode {
                    path: link_path.clone(),
                    properties: Props::new(),
                    new_properties: properties,
                })
                .is_err()
            {
                return false;
            }

            if replace_text {
                let count = editor
                    .element(&link_path)
                    .map(|element| element.children.len())
                    .unwrap_or(0);
                for index in (0..count).rev() {
                    let _ = editor.apply(Operation::RemoveNode {
                        path: child_path(&link_path, index),
                    });
                }
                let node = Node::Text(Text {
                    text: text.to_string(),
                    properties: marks.clone(),
                });
                if editor
                    .apply(Operation::InsertNode {
                        path: child_path(&link_path, 0),
                        node,
                    })
                    .is_err()
                {
                    return false;
                }
            }
            true
        })
    }

    /// The link containing the start of the current selection.
    pub fn link_at_selection(&self) -> Option<(Path, String)> {
        let range = self.selection.as_ref()?;
        let link = self.inline_ancestor(&range.start().path, "a")?;
        let href = self
            .link_property
            .get_str(&self.element(&link)?.properties)?
            .to_string();
        Some((link, href))
    }

    /// Wrap the selected text in an inline element.
    ///
    /// `properties` become the new element's properties and the returned path
    /// points at the wrapper. Returns `None` when the selection is collapsed,
    /// spans more than one inline parent, or cannot be split at its boundaries.
    /// The whole change is one undoable transaction.
    pub fn wrap_inline(&mut self, element_type: &str, properties: Props) -> Option<Path> {
        let path = self.transaction(|editor| editor.wrap_inline_inner(element_type, properties))?;
        // Normalization can insert surrounding text nodes, which shifts the
        // wrapper. Resolve the final location through the selection that
        // `wrap_inline_inner` placed inside the wrapper.
        let start = self
            .selection
            .as_ref()
            .map(|range| range.start().path.clone());
        start
            .and_then(|start| self.inline_ancestor(&start, element_type))
            .or(Some(path))
    }

    /// Replace an inline element with its children.
    ///
    /// This is the inverse of [`Editor::wrap_inline`] and is useful for custom
    /// inline elements. The whole change is one undoable transaction.
    pub fn unwrap_inline(&mut self, at: &[usize]) -> Option<()> {
        self.transaction(|editor| editor.unwrap_inline_inner(at))
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
            .map(|element| self.is_inline_element(element))
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

    /// The nearest inline ancestor of `path` with the given element type.
    ///
    /// Useful for application-specific inline elements such as links, mentions
    /// or custom chips. The path itself is not considered.
    pub fn inline_ancestor(&self, path: &[usize], element_type: &str) -> Option<Path> {
        self.inline_ancestor_matching(path, |element| element.element_type == element_type)
    }

    /// The nearest inline ancestor of `path` accepted by `predicate`.
    ///
    /// This is the generic form of [`Editor::inline_ancestor`] for inline
    /// families that share a type prefix or a marker property.
    pub fn inline_ancestor_matching(
        &self,
        path: &[usize],
        predicate: impl Fn(&Element) -> bool,
    ) -> Option<Path> {
        for depth in (1..path.len()).rev() {
            let candidate = &path[..depth];
            if let Some(element) = self.element(candidate) {
                if self.is_inline_element(element) && predicate(element) {
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
            if !self.is_text_markable(&at) {
                continue;
            }
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
    // Blocks
    // ---------------------------------------------------------------------

    /// Set the block type of the block(s) intersecting the selection.
    ///
    /// List types are owned by the list feature/trait, so this ignores `ul` and
    /// `ol` and leaves list toggling to feature hooks.
    pub fn set_block(&mut self, element_type: &str) {
        if self.schema.is_list(element_type) {
            return;
        }
        self.transaction(|editor| {
            // Turning a list item into a paragraph/heading should unwrap the
            // list first, rather than leaving `<ul><h1>` behind. Features run
            // before the type change and can also veto it entirely.
            if editor.selection.is_some() {
                for feature in editor.features.snapshot() {
                    if feature.before_set_block(editor, element_type) {
                        return;
                    }
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
            if self.is_inline_element(element) {
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

    /// Slate-compatible `Transforms.wrapNodes`.
    pub fn wrap_nodes_with(&mut self, element: Element, options: WrapNodesOptions) -> Option<Path> {
        self.transaction(|editor| editor.wrap_nodes_with_inner(element, options))
    }

    fn wrap_nodes_with_inner(
        &mut self,
        element: Element,
        options: WrapNodesOptions,
    ) -> Option<Path> {
        let inline_wrapper = self.is_inline_element(&element);
        let match_kind = options.match_kind.unwrap_or(if inline_wrapper {
            NodeMatch::TextOrInline
        } else {
            NodeMatch::Block
        });

        let mut at = options
            .at
            .clone()
            .or_else(|| self.selection.clone().map(Location::Range))?;

        if let Location::Path(path) = &at {
            return self.wrap_node_at_path(path, element);
        }

        let collapsed_point = match &at {
            Location::Range(range) if range.is_collapsed() => Some(range.anchor.clone()),
            Location::Point(point) => Some(point.clone()),
            _ => None,
        };
        if let Some(point) = collapsed_point {
            let target = if inline_wrapper {
                point.path.clone()
            } else {
                self.leaf_block_path(&point)?
            };
            return self.wrap_node_at_path(&target, element);
        }

        if !options.hanging {
            if let Location::Range(range) = &at {
                at = Location::Range(self.unhang_range_end(range));
            }
        }

        if options.split {
            if let Location::Range(range) = at.clone() {
                if range.is_expanded() {
                    let end_at_block_start = self
                        .leaf_block_path(range.end())
                        .and_then(|block| self.first_text_point_in(&block))
                        .as_ref()
                        == Some(range.end());
                    self.selection = Some(range);
                    let end = self.selection.clone()?.end().clone();
                    self.split_nodes_with_inner(SplitNodesOptions {
                        at: Some(Location::Point(end)),
                        match_kind,
                        mode: options.mode,
                        height: 0,
                        always: false,
                        voids: options.voids,
                        select: Some(false),
                        preserve_selection: false,
                        keep_selection: true,
                    })?;
                    if !end_at_block_start {
                        self.adjust_range_end_before_split();
                    }
                    let start = self.selection.clone()?.start().clone();
                    self.split_nodes_with_inner(SplitNodesOptions {
                        at: Some(Location::Point(start)),
                        match_kind,
                        mode: options.mode,
                        height: 0,
                        always: false,
                        voids: options.voids,
                        select: Some(false),
                        preserve_selection: false,
                        keep_selection: true,
                    })?;
                    at = Location::Range(self.selection.clone()?);
                }
            }
        }

        let range = match at {
            Location::Range(range) => range,
            Location::Point(point) => Range::collapsed(point),
            Location::Path(path) => self.range_covering_path(&path)?,
        };
        if range.is_collapsed() {
            return None;
        }

        let mut matches = Vec::new();
        collect_matching_paths(
            self,
            &self.children_as_node(),
            &mut Vec::new(),
            &range,
            match_kind,
            options.match_property.as_deref(),
            &mut matches,
        );
        filter_match_mode(&mut matches, options.mode);
        if matches.is_empty() {
            return None;
        }

        matches.sort();
        if !inline_wrapper {
            return self.wrap_match_group(matches, element);
        }

        let mut groups: Vec<Vec<Path>> = Vec::new();
        for path in matches {
            let root = path[0];
            if groups
                .last()
                .and_then(|group| group.first())
                .is_some_and(|first| first[0] == root)
            {
                groups.last_mut().unwrap().push(path);
            } else {
                groups.push(vec![path]);
            }
        }

        let mut inserted = None;
        for group in groups {
            inserted = self.wrap_match_group(group, element.clone());
        }
        inserted
    }

    fn wrap_match_group(&mut self, matches: Vec<Path>, element: Element) -> Option<Path> {
        let first = matches.first()?.clone();
        let last = matches.last()?.clone();
        let common = if first == last {
            path::parent(&first).unwrap_or_default()
        } else {
            path::common(&first, &last)
        };
        let parent = common.clone();
        let start = *first.get(common.len())?;
        let end = *last.get(common.len())?;
        if end >= self.children_at(&parent)?.len() {
            return None;
        }

        let span = self.children_at(&parent)?[start..=end].to_vec();
        let wrapper_path = child_path(&parent, end + 1);
        let count = span.len();
        let mut wrapper = element;
        wrapper.children = span;

        let original_selection = self.selection.clone();
        self.insert_nodes(&wrapper_path, vec![Node::Element(wrapper)]);
        for index in (start..=end).rev() {
            self.remove_nodes(&child_path(&parent, index), 1);
        }

        if let Some(selection) = original_selection {
            let map = |point: &Point| {
                if is_path_prefix(&parent, &point.path) {
                    if let Some(index) = point.path.get(parent.len()) {
                        if *index >= start && *index <= end {
                            let mut mapped = child_path(&parent, start);
                            mapped.push(index - start);
                            mapped.extend_from_slice(&point.path[parent.len() + 1..]);
                            return Point::new(mapped, point.offset);
                        }
                        if *index > end {
                            let mut mapped = point.path.clone();
                            mapped[parent.len()] -= count - 1;
                            return Point::new(mapped, point.offset);
                        }
                    }
                }
                point.clone()
            };
            self.selection = Some(Range::new(map(&selection.anchor), map(&selection.focus)));
        }

        Some(child_path(&parent, start))
    }

    /// Slate-compatible `Transforms.unwrapNodes`.
    pub fn unwrap_nodes_with(&mut self, options: UnwrapNodesOptions) -> Option<()> {
        self.transaction(|editor| editor.unwrap_nodes_with_inner(options))
    }

    fn unwrap_nodes_with_inner(&mut self, options: UnwrapNodesOptions) -> Option<()> {
        let path_only = match &options.at {
            Some(Location::Path(path))
                if options.match_kind.is_none() && options.match_property.is_none() =>
            {
                Some(path.clone())
            }
            _ => None,
        };
        let match_kind = if path_only.is_some() {
            NodeMatch::Any
        } else {
            options.match_kind.unwrap_or(NodeMatch::Block)
        };
        let initial_selection = self.selection.clone();
        let selection_from_options = options.at.is_some();
        let mut at = options
            .at
            .clone()
            .or_else(|| self.selection.clone().map(Location::Range))?;

        if let Location::Path(path) = &at {
            at = Location::Range(self.range_covering_path(path)?);
        }

        let mut range = match at {
            Location::Range(range) => range,
            Location::Point(point) => Range::collapsed(point),
            Location::Path(_) => return None,
        };

        if !options.hanging {
            range = self.unhang_range_end(&range);
        }

        if options.split && range.is_expanded() {
            let split_kind = match &options.match_property {
                Some(property) => self.property_match_kind(property),
                None => NodeMatch::Inline,
            };
            let end_at_block_start = self
                .leaf_block_path(range.end())
                .and_then(|block| self.first_text_point_in(&block))
                .as_ref()
                == Some(range.end());
            self.selection = Some(range.clone());
            let end = self.selection.clone()?.end().clone();
            self.split_nodes_with_inner(SplitNodesOptions {
                at: Some(Location::Point(end)),
                match_kind: split_kind,
                mode: options.mode,
                height: 0,
                always: false,
                voids: options.voids,
                select: Some(false),
                preserve_selection: false,
                keep_selection: true,
            })?;
            if !end_at_block_start {
                self.adjust_range_end_before_split();
            }
            let start = self.selection.clone()?.start().clone();
            self.split_nodes_with_inner(SplitNodesOptions {
                at: Some(Location::Point(start)),
                match_kind: split_kind,
                mode: options.mode,
                height: 0,
                always: false,
                voids: options.voids,
                select: Some(false),
                preserve_selection: false,
                keep_selection: true,
            })?;
            range = self.selection.clone()?;
        }

        let mut matches = Vec::new();
        collect_matching_paths(
            self,
            &self.children_as_node(),
            &mut Vec::new(),
            &range,
            match_kind,
            options.match_property.as_deref(),
            &mut matches,
        );
        if let Some(path) = &path_only {
            matches.retain(|candidate| candidate == path);
        }
        filter_match_mode(&mut matches, options.mode);
        if matches.is_empty() {
            return None;
        }

        matches.sort();
        let original_selection = if selection_from_options {
            initial_selection
        } else {
            Some(range.clone())
        };
        let mut actions: Vec<(Path, usize)> = Vec::new();
        let mut lifted = None;
        for path in matches.into_iter().rev() {
            let Some(node) = self.node(&path).cloned() else {
                continue;
            };
            let Node::Element(element) = node else {
                continue;
            };
            if element.children.is_empty() {
                continue;
            }
            if options.split && self.unwrap_partial_block(&path, &range, &element).is_some() {
                if lifted.is_none() {
                    lifted = self.selection.clone();
                }
                continue;
            }
            let count = element.children.len();
            self.insert_nodes(&path, element.children);
            self.remove_nodes(
                &child_path(&path[..path.len() - 1], path.last().copied()? + count),
                1,
            );
            actions.push((path, count));
        }

        if lifted.is_some() {
            return Some(());
        }

        if let Some(selection) = original_selection {
            let map = |point: &Point| {
                let mut path = point.path.clone();
                for (unwrapped, count) in &actions {
                    if is_path_prefix(unwrapped, &path) {
                        if path.len() > unwrapped.len() {
                            let wrapper_index = unwrapped[unwrapped.len() - 1];
                            let child_index = path[unwrapped.len()];
                            let mut mapped = unwrapped[..unwrapped.len() - 1].to_vec();
                            mapped.push(wrapper_index + child_index);
                            mapped.extend_from_slice(&path[unwrapped.len() + 1..]);
                            path = mapped;
                        }
                        continue;
                    }
                    let parent = &unwrapped[..unwrapped.len() - 1];
                    if is_path_prefix(parent, &path) {
                        let index = parent.len();
                        if path
                            .get(index)
                            .is_some_and(|value| *value > unwrapped[index])
                        {
                            path[index] += count - 1;
                        }
                    }
                }
                Point::new(path, point.offset)
            };
            self.selection = Some(Range::new(map(&selection.anchor), map(&selection.focus)));
        }

        Some(())
    }

    fn adjust_range_end_before_split(&mut self) {
        let Some(selection) = self.selection.clone() else {
            return;
        };
        if selection.focus.offset != 0 {
            return;
        }
        if let Some(end) = self.previous_text_point(&selection.focus) {
            self.selection = Some(Range::new(selection.anchor, end));
            return;
        }
        let Some(block) = self.leaf_block_path(&selection.focus) else {
            return;
        };
        let Some(previous) = path::previous(&block) else {
            return;
        };
        let Some(end) = self.last_text_point_in(&previous) else {
            return;
        };
        self.selection = Some(Range::new(selection.anchor, end));
    }

    fn unhang_range_end(&self, range: &Range) -> Range {
        if range.is_collapsed() {
            return range.clone();
        }
        let end = self
            .text_point(range.end())
            .unwrap_or_else(|| range.end().clone());
        let start = self
            .text_point(range.start())
            .unwrap_or_else(|| range.start().clone());
        let Some(end_block) = self.leaf_block_path(&end) else {
            return range.clone();
        };
        if self.first_text_point_in(&end_block).as_ref() != Some(&end) {
            return range.clone();
        }
        let Some(start_block) = self.leaf_block_path(&start) else {
            return range.clone();
        };
        if end_block.first() <= start_block.first() {
            return range.clone();
        }
        let Some(previous) = path::previous(&end_block) else {
            return range.clone();
        };
        let Some(target) = self.last_text_point_in(&previous) else {
            return range.clone();
        };
        Range::new(range.anchor.clone(), target)
    }

    fn wrap_node_at_path(&mut self, path: &[usize], element: Element) -> Option<Path> {
        let node = self.node(path)?.clone();
        let index = *path.last()?;
        let parent = path[..path.len() - 1].to_vec();
        let mut wrapper = element;
        wrapper.children = vec![node];
        let insert_at = child_path(&parent, index);
        let original_selection = self.selection.clone();
        self.insert_nodes(&insert_at, vec![Node::Element(wrapper)]);
        self.remove_nodes(&child_path(&parent, index + 1), 1);

        if let Some(selection) = original_selection {
            let map = |point: &Point| {
                if is_path_prefix(path, &point.path) {
                    let mut mapped = child_path(path, 0);
                    mapped.extend_from_slice(&point.path[path.len()..]);
                    Point::new(mapped, point.offset)
                } else {
                    point.clone()
                }
            };
            self.selection = Some(Range::new(map(&selection.anchor), map(&selection.focus)));
        }
        Some(insert_at)
    }

    fn unwrap_partial_block(
        &mut self,
        path: &[usize],
        range: &Range,
        element: &Element,
    ) -> Option<()> {
        let start = range.start().clone();
        let end = range.end().clone();
        if !is_path_prefix(path, &start.path) || !is_path_prefix(path, &end.path) {
            return None;
        }
        let s = *start.path.get(path.len())?;
        let e = *end.path.get(path.len())?;
        if e >= element.children.len() || s > e {
            return None;
        }

        let covers_start = s == 0 && self.first_text_point_in(path).as_ref() == Some(&start);
        let covers_end =
            e == element.children.len() - 1 && self.last_text_point_in(path).as_ref() == Some(&end);
        if covers_start && covers_end {
            return None;
        }

        let before = element.children[..s].to_vec();
        let selected = element.children[s..=e].to_vec();
        let after = element.children[e + 1..].to_vec();
        let total = element.children.len();
        let parent = path[..path.len() - 1].to_vec();
        let index = *path.last()?;
        let original_selection = self.selection.clone();

        self.remove_nodes(&child_path(path, s), total - s);
        let base = if before.is_empty() {
            self.remove_nodes(path, 1);
            index
        } else {
            index + 1
        };
        let selected_len = selected.len();
        self.insert_nodes(&child_path(&parent, base), selected);
        if !after.is_empty() {
            let mut copy = element.clone();
            copy.children = after.clone();
            self.insert_nodes(
                &child_path(&parent, base + selected_len),
                vec![Node::Element(copy)],
            );
        }

        if let Some(selection) = original_selection {
            let map = |point: &Point| {
                if is_path_prefix(path, &point.path) {
                    let child = point.path.get(path.len()).copied().unwrap_or(0);
                    let rest = &point.path[path.len() + 1..];
                    if child < s {
                        return point.clone();
                    }
                    let mut mapped = parent.clone();
                    if child <= e {
                        mapped.push(base + (child - s));
                    } else {
                        mapped.push(base + selected_len + (child - e - 1));
                    }
                    mapped.extend_from_slice(rest);
                    return Point::new(mapped, point.offset);
                }
                if is_path_prefix(&parent, &point.path) {
                    let index_in_parent = point.path[parent.len()];
                    if index_in_parent > index {
                        let mut mapped = point.path.clone();
                        let inserted = selected_len + usize::from(!after.is_empty());
                        mapped[parent.len()] += inserted - usize::from(before.is_empty());
                        return Point::new(mapped, point.offset);
                    }
                }
                point.clone()
            };
            self.selection = Some(Range::new(map(&selection.anchor), map(&selection.focus)));
        }

        Some(())
    }

    fn property_match_kind(&self, property: &str) -> NodeMatch {
        fn find(editor: &Editor, nodes: &[Node], property: &str) -> Option<NodeMatch> {
            for node in nodes {
                if node.properties().get(property).is_some() {
                    return Some(match node {
                        Node::Text(_) => NodeMatch::Text,
                        Node::Element(element) if editor.is_inline_element(element) => {
                            NodeMatch::Inline
                        }
                        Node::Element(_) => NodeMatch::Block,
                    });
                }
                if let Node::Element(element) = node {
                    if let Some(kind) = find(editor, &element.children, property) {
                        return Some(kind);
                    }
                }
            }
            None
        }
        find(self, &self.children, property).unwrap_or(NodeMatch::Block)
    }

    fn range_covering_path(&self, path: &[usize]) -> Option<Range> {
        if let Some(text) = self.text(path) {
            return Some(Range::new(
                Point::new(path.to_vec(), 0),
                Point::new(path.to_vec(), text.len()),
            ));
        }
        let start = self.first_text_point_in(path)?;
        let end = self.last_text_point_in(path)?;
        Some(Range::new(start, end))
    }

    /// Slate-compatible `Transforms.insertNodes`.
    pub fn insert_nodes_with(
        &mut self,
        nodes: Vec<Node>,
        options: InsertNodesOptions,
    ) -> Option<Path> {
        if nodes.is_empty() {
            return None;
        }
        self.transaction(|editor| editor.insert_nodes_with_inner(nodes, options))
    }

    fn insert_nodes_with_inner(
        &mut self,
        nodes: Vec<Node>,
        options: InsertNodesOptions,
    ) -> Option<Path> {
        let match_kind = options.match_kind.unwrap_or_else(|| match &nodes[0] {
            Node::Text(_) => NodeMatch::Text,
            Node::Element(element) if self.is_inline_element(element) => NodeMatch::TextOrInline,
            Node::Element(_) => NodeMatch::Block,
        });
        let select = options.select.unwrap_or(options.at.is_none());

        if let Some(Location::Path(path)) = &options.at {
            if self.node(path).is_none() {
                let count = nodes.len();
                self.insert_nodes(path, nodes);
                if select {
                    let mut last = path.clone();
                    if let Some(index) = last.last_mut() {
                        *index += count.saturating_sub(1);
                    }
                    self.select_end_of(&last);
                }
                return Some(path.clone());
            }
        }

        let at_option = options
            .at
            .clone()
            .or_else(|| self.selection.clone().map(Location::Range));
        if at_option.is_none() && self.children.is_empty() {
            let count = nodes.len();
            self.insert_nodes(&[0], nodes);
            if select {
                self.select_end_of(&[count.saturating_sub(1)]);
            }
            return Some(vec![0]);
        }
        let at = match at_option {
            Some(Location::Range(range)) => {
                if range.is_collapsed() {
                    range.anchor
                } else {
                    self.delete_selection_inner();
                    self.selection.as_ref()?.anchor.clone()
                }
            }
            Some(Location::Point(point)) => point,
            Some(Location::Path(path)) => self.first_text_point_in(&path)?,
            None => self.end_point()?,
        };

        let levels: Vec<Path> = (0..=at.path.len()).map(|i| at.path[..i].to_vec()).collect();
        let mut candidates = levels;
        if options.mode == MatchMode::Lowest {
            candidates.reverse();
        }
        let match_path = candidates.into_iter().find(|path| {
            self.node(path)
                .is_some_and(|node| node_matches(self, node, match_kind))
        })?;

        let at_start = self.first_text_point_in(&match_path).as_ref() == Some(&at);
        let at_end = self.last_text_point_in(&match_path).as_ref() == Some(&at);

        let split_options = SplitNodesOptions {
            at: Some(Location::Point(at.clone())),
            match_kind,
            mode: options.mode,
            height: 0,
            always: false,
            voids: options.voids,
            select: Some(false),
            preserve_selection: true,

            keep_selection: false,
        };
        self.split_nodes_with_inner(split_options);

        let mut insert_at = match_path;
        if at_end || !at_start {
            if let Some(index) = insert_at.last_mut() {
                *index += 1;
            }
        }
        if !options.voids {
            let parent = insert_at[..insert_at.len().saturating_sub(1)].to_vec();
            if self
                .element(&parent)
                .is_some_and(|element| self.is_void_element(element))
            {
                return None;
            }
        }

        let count = nodes.len();
        self.insert_nodes(&insert_at, nodes);

        if select {
            let mut last = insert_at[..insert_at.len().saturating_sub(1)].to_vec();
            last.push(insert_at.last().copied().unwrap_or(1) + count - 1);
            self.select_end_of(&last);
        }

        Some(insert_at)
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

    /// Slate-compatible `Transforms.splitNodes`.
    fn split_operation_at(&self, path: &[usize], position: usize) -> Operation {
        let mut properties = Props::new();
        if let Some(node) = self.node(path) {
            match node {
                Node::Text(text) => properties = text.properties.clone(),
                Node::Element(element) => {
                    properties = element.properties.clone();
                    properties.insert("type".into(), serde_json::json!(element.element_type));
                }
            }
        }
        Operation::SplitNode {
            path: path.to_vec(),
            position,
            properties,
        }
    }

    pub fn split_nodes_with(&mut self, options: SplitNodesOptions) -> Option<Point> {
        self.transaction(|editor| editor.split_nodes_with_inner(options))
    }

    fn split_nodes_with_inner(&mut self, options: SplitNodesOptions) -> Option<Point> {
        let original_selection = self.selection.clone();
        let mut path_match: Option<(Path, usize)> = None;

        let at_option = options
            .at
            .clone()
            .or_else(|| self.selection.clone().map(Location::Range));
        let mut at = match at_option {
            Some(Location::Range(range)) => {
                if range.is_collapsed() {
                    range.anchor
                } else {
                    let start_path = range.start().path.clone();
                    let end_path = range.end().path.clone();
                    self.delete_selection_inner();
                    let mut point = self.selection.as_ref()?.anchor.clone();
                    if start_path != end_path {
                        if let Some(next) = self.next_text_point(&point) {
                            point = next;
                        }
                    }
                    point
                }
            }
            Some(Location::Point(point)) => point,
            Some(Location::Path(path)) => {
                let parent = path::parent(&path).unwrap_or_default();
                path_match = Some((parent, path.len()));
                self.first_text_point_in(&path)?
            }
            None => return None,
        };

        let matches = |editor: &Self, node: &Node, path: &[usize]| {
            if let Some((parent, _)) = &path_match {
                return path == parent;
            }
            match options.match_kind {
                NodeMatch::Block => node
                    .as_element()
                    .is_some_and(|element| !editor.is_inline_element(element)),
                NodeMatch::Inline => node
                    .as_element()
                    .is_some_and(|element| editor.is_inline_element(element)),
                NodeMatch::Text => node.is_text(),
                NodeMatch::TextOrInline => {
                    node.is_text()
                        || node
                            .as_element()
                            .is_some_and(|element| editor.is_inline_element(element))
                }
                NodeMatch::Any => true,
            }
        };

        let levels: Vec<Path> = (0..=at.path.len()).map(|i| at.path[..i].to_vec()).collect();
        let mut candidates = levels.clone();
        if options.mode == MatchMode::Lowest {
            candidates.reverse();
        }
        let highest = candidates.into_iter().find(|path| {
            self.node(path)
                .is_some_and(|node| matches(self, node, path))
        })?;

        let mut height = options.height;
        let mut always = options.always;
        if let Some((_, original_len)) = &path_match {
            height = at.path.len().saturating_sub(*original_len) + 1;
            always = true;
        }

        if !options.voids {
            if let Some(void_path) = levels.iter().find(|path| {
                self.node(path)
                    .and_then(Node::as_element)
                    .is_some_and(|element| self.is_void_element(element))
            }) {
                if self
                    .element(void_path)
                    .is_some_and(|element| self.is_inline_element(element))
                {
                    let mut after = void_path.clone();
                    if let Some(index) = after.last_mut() {
                        *index += 1;
                    }
                    if self.text(&after).is_none() {
                        self.insert_nodes(&after, vec![Node::empty_text()]);
                    }
                    at = self.first_text_point_in(&after)?;
                }
                height = at.path.len().saturating_sub(void_path.len()) + 1;
                always = true;
            }
        }

        let depth = at.path.len().saturating_sub(height);
        let lowest_path = at.path[..depth].to_vec();
        let mut position = if height == 0 {
            at.offset
        } else {
            at.path.get(depth).copied().unwrap_or(0)
        };

        let keep_selection = options.keep_selection;
        let mut current = at.clone();
        if !keep_selection || self.selection.is_none() {
            self.selection = Some(Range::collapsed(at.clone()));
        }

        for path in (1..=lowest_path.len()).rev() {
            let path = lowest_path[..path].to_vec();
            if path.len() < highest.len() {
                break;
            }
            let Some(node) = self.node(&path) else {
                break;
            };
            if !options.voids
                && node
                    .as_element()
                    .is_some_and(|element| self.is_void_element(element))
            {
                break;
            }

            let point = if keep_selection {
                current.clone()
            } else {
                self.selection.clone()?.anchor
            };
            let is_end = self.last_text_point_in(&path).as_ref() == Some(&point);
            let is_edge = self.first_text_point_in(&path).as_ref() == Some(&point) || is_end;
            let split = always || !is_edge;

            if split {
                let operation = self.split_operation_at(&path, position);
                if self.apply(operation.clone()).is_err() {
                    break;
                }
                if keep_selection {
                    if let Some(next) = current.transform(&operation, Affinity::Forward) {
                        current = next;
                    }
                }
            }

            position = path.last().copied().unwrap_or(0) + usize::from(split || is_end);
        }

        let select = options.select.unwrap_or(options.at.is_none());
        if !select && options.preserve_selection {
            self.selection = original_selection;
        }

        self.selection.as_ref().map(|range| range.anchor.clone())
    }

    pub fn move_nodes(&mut self, from: &[usize], to: &[usize]) {
        self.transaction(|editor| {
            let _ = editor.apply(Operation::MoveNode {
                path: from.to_vec(),
                new_path: to.to_vec(),
            });
        });
    }

    pub fn set_nodes_with(&mut self, properties: Props, options: SetNodesOptions) -> Option<()> {
        self.transaction(|editor| editor.set_nodes_with_inner(properties, options))
    }

    fn set_nodes_with_inner(&mut self, properties: Props, options: SetNodesOptions) -> Option<()> {
        let path_only = match &options.at {
            Some(Location::Path(path))
                if options.match_kind.is_none() && options.match_property.is_none() =>
            {
                Some(path.clone())
            }
            _ => None,
        };
        let mut at = options
            .at
            .clone()
            .or_else(|| self.selection.clone().map(Location::Range))?;

        if let Location::Path(path) = &at {
            at = Location::Range(self.range_covering_path(path)?);
        }
        let mut range = match at {
            Location::Range(range) => range,
            Location::Point(point) => Range::collapsed(point),
            Location::Path(_) => return None,
        };

        if !options.hanging {
            range = self.unhang_range_end(&range);
        }

        if options.split
            && range.is_collapsed()
            && self
                .text(&range.anchor.path)
                .is_some_and(|text| !text.is_empty())
        {
            return Some(());
        }

        if options.split && range.is_expanded() {
            let end_at_end =
                self.last_text_point_in(&range.end().path).as_ref() == Some(range.end());
            let start_at_start =
                self.first_text_point_in(&range.start().path).as_ref() == Some(range.start());
            let match_kind = options.match_kind.unwrap_or(NodeMatch::Block);
            self.selection = Some(range.clone());
            let end = self.selection.clone()?.end().clone();
            self.split_nodes_with_inner(SplitNodesOptions {
                at: Some(Location::Point(end)),
                match_kind,
                mode: options.mode,
                height: 0,
                always: !end_at_end,
                voids: options.voids,
                select: Some(false),
                preserve_selection: false,
                keep_selection: true,
            })?;
            if !end_at_end {
                self.adjust_range_end_before_split();
            }
            let start = self.selection.clone()?.start().clone();
            self.split_nodes_with_inner(SplitNodesOptions {
                at: Some(Location::Point(start)),
                match_kind,
                mode: options.mode,
                height: 0,
                always: !start_at_start,
                voids: options.voids,
                select: Some(false),
                preserve_selection: false,
                keep_selection: true,
            })?;
            range = self.selection.clone()?;
        }

        let paths = if let Some(path) = path_only {
            vec![path]
        } else {
            let match_kind = options.match_kind.unwrap_or(NodeMatch::Block);
            let mut matches = Vec::new();
            collect_matching_paths(
                self,
                &self.children_as_node(),
                &mut Vec::new(),
                &range,
                match_kind,
                options.match_property.as_deref(),
                &mut matches,
            );
            filter_match_mode(&mut matches, options.mode);
            if range.is_expanded() {
                matches.retain(|path| {
                    let start = if self.text(path).is_some() {
                        Point::new(path.clone(), 0)
                    } else {
                        self.first_text_point_in(path)
                            .unwrap_or_else(|| Point::new(path.clone(), 0))
                    };
                    start.compare(range.end()) != std::cmp::Ordering::Equal
                });
            }
            matches
        };

        for path in paths {
            if path.is_empty() {
                continue;
            }
            let Some(node) = self.node(&path).cloned() else {
                continue;
            };
            let old_properties = if options.replace {
                let mut old = node.properties().clone();
                if let Node::Element(element) = &node {
                    old.insert("type".into(), Value::String(element.element_type.clone()));
                }
                old
            } else {
                Props::new()
            };
            let _ = self.apply(Operation::SetNode {
                path,
                properties: old_properties,
                new_properties: properties.clone(),
            });
        }

        Some(())
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
        if text.is_empty() {
            return;
        }
        self.transaction(|editor| {
            // Keep the existing selection and let the operation transform it,
            // matching Slate's `Transforms.insertText(editor, text, { at })`.
            let _ = editor.apply(Operation::InsertText {
                path: at.path.clone(),
                offset: at.offset,
                text: text.to_string(),
            });
        });
    }

    /// Insert a parsed fragment at the current selection.
    ///
    /// A single paragraph-shaped block is merged into the current text so
    /// inline marks and links survive. Multi-block fragments split the current
    /// top-level block and insert between the two halves.
    pub fn insert_fragment(&mut self, nodes: Vec<Node>) {
        self.insert_fragment_with_options(nodes, false);
    }

    /// Insert a fragment, optionally allowing edits inside void nodes.
    pub fn insert_fragment_with_options(&mut self, nodes: Vec<Node>, voids: bool) {
        if nodes.is_empty() {
            return;
        }

        let is_inline_node = |node: &Node| {
            node.is_text()
                || node
                    .as_element()
                    .is_some_and(|element| self.is_inline_element(element))
        };

        let inline_children = nodes.iter().all(is_inline_node).then(|| nodes.clone());

        if let Some(selection) = self.selection.clone() {
            if selection.is_expanded() {
                let end = selection.end().clone();
                if let Some(point) = self.text_point(&end) {
                    if self.point_has_void_ancestor(&point) {
                        return;
                    }
                    if let Some(element_path) = self.read_only_ancestor(&point) {
                        let inline = self
                            .element(&element_path)
                            .is_some_and(|element| self.is_inline_element(element));
                        if inline {
                            let mut after = element_path;
                            if let Some(index) = after.last_mut() {
                                *index += 1;
                            }
                            self.selection =
                                Some(Range::new(selection.anchor.clone(), Point::new(after, 0)));
                        } else {
                            self.remove_nodes(&element_path, 1);
                            let anchor_point = self
                                .text_point(&selection.anchor)
                                .unwrap_or_else(|| selection.anchor.clone());
                            if let Some(text) = self.text(&anchor_point.path).cloned() {
                                let removed =
                                    char_slice(&text.text, anchor_point.offset, text.len())
                                        .to_string();
                                if !removed.is_empty() {
                                    let _ = self.apply(Operation::RemoveText {
                                        path: anchor_point.path.clone(),
                                        offset: anchor_point.offset,
                                        text: removed,
                                    });
                                }
                            }
                            self.selection = Some(Range::collapsed(anchor_point));
                        }
                    }
                }
            }
        }

        if !voids {
            if let Some(point) = self.selection.as_ref().map(|range| range.anchor.clone()) {
                if let Some(point) = self.text_point(&point) {
                    if self.point_is_read_only(&point) {
                        return;
                    }
                }
            }
        }

        if self.insert_hanging_fragment(&nodes) {
            return;
        }

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

    /// Slate's `unhangRange` for fragment insertion: when a selection starts
    /// at one block and ends at the start of the next block, replace the first
    /// block with the fragment and leave the following blocks intact.
    fn insert_hanging_fragment(&mut self, nodes: &[Node]) -> bool {
        let Some(selection) = self.selection.clone() else {
            return false;
        };
        if !selection.is_expanded() {
            return false;
        }

        let anchor = self
            .text_point(&selection.anchor)
            .unwrap_or_else(|| selection.anchor.clone());
        let focus = self
            .text_point(&selection.focus)
            .unwrap_or_else(|| selection.focus.clone());
        let (Some(anchor_block), Some(focus_block)) =
            (self.leaf_block_path(&anchor), self.leaf_block_path(&focus))
        else {
            return false;
        };
        if self.first_text_point_in(&focus_block).as_ref() != Some(&focus) {
            return false;
        }

        let (Some(&anchor_index), Some(&focus_index)) = (anchor_block.first(), focus_block.first())
        else {
            return false;
        };
        if focus_index <= anchor_index {
            return false;
        }

        let anchor_has_void = self
            .children
            .get(anchor_index)
            .is_some_and(|node| contains_void_or_read_only(self, node));
        let focus_is_empty = self
            .element(&focus_block)
            .is_some_and(|element| element.text_len() == 0 && element.is_text_only());

        let focus_is_last = focus_index + 1 == self.children.len();
        self.transaction(|editor| {
            if focus_is_last && focus_is_empty {
                let count = editor.children.len() - anchor_index;
                editor.remove_nodes(&[anchor_index], count);
            } else {
                editor.remove_nodes(&[anchor_index], 1);
                if anchor_has_void && focus_is_empty {
                    editor.remove_nodes(&[anchor_index], 1);
                }
            }
            editor.insert_nodes(&[anchor_index], nodes.to_vec());
            if let Some(last) = nodes.len().checked_sub(1) {
                editor.select_end_of(&[anchor_index + last]);
            }
        });
        true
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

        if let Some(inline_path) = self.inline_ancestor_matching(&point.path, |_| true) {
            let parent = inline_path[..inline_path.len() - 1].to_vec();
            let index = *inline_path.last().unwrap();
            let at_start = self.first_text_point_in(&inline_path).as_ref() == Some(&point);
            let at_end = self.last_text_point_in(&inline_path).as_ref() == Some(&point);

            let insert_index = if at_start || at_end {
                index + 1
            } else {
                self.selection = Some(Range::collapsed(point.clone()));
                let Some(after) = self.split_inline_at_point(&point, &inline_path) else {
                    return;
                };
                after
            };

            let count = children.len();
            self.insert_nodes(&child_path(&parent, insert_index), children);
            if count > 0 {
                self.select_end_of(&child_path(&parent, insert_index + count - 1));
            }
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

        // Leave the caret at the end of the last inserted child, like Slate's
        // `Transforms.insertFragment`. Falling back to the following node only
        // when the last child has no text (e.g. an inline void).
        let last_path = child_path(&parent, start + count - 1);
        let after_path = child_path(&parent, start + count);
        if let Some(point) = self
            .last_text_point_in(&last_path)
            .or_else(|| self.first_text_point_in(&after_path))
            .or_else(|| self.end_point())
        {
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
        let Some(block) = self.leaf_block_path(&point) else {
            return;
        };
        let block_index = *block.last().unwrap();
        let parent = block[..block.len() - 1].to_vec();
        let at_start = self.first_text_point_in(&block).as_ref() == Some(&point);
        let at_end = self.last_text_point_in(&block).as_ref() == Some(&point);
        let is_empty = at_start && at_end;

        let is_block = |editor: &Self, node: &Node| {
            node.as_element()
                .is_some_and(|element| !editor.is_inline_element(element))
        };
        if !nodes.iter().any(|node| is_block(self, node)) {
            self.insert_inline_children_inner(nodes);
            return;
        }

        let (starts, middles, ends) = if is_empty {
            (Vec::new(), nodes.clone(), Vec::new())
        } else {
            self.classify_fragment(&nodes, at_start, at_end)
        };

        if middles.is_empty() && ends.is_empty() {
            self.insert_inline_children_inner(starts);
            return;
        }

        let inline_only = |editor: &Self, nodes: &[Node]| {
            nodes.iter().all(|node| {
                node.is_text()
                    || node
                        .as_element()
                        .is_some_and(|element| editor.is_inline_element(element))
            })
        };
        if !inline_only(self, &starts) || !inline_only(self, &ends) {
            return;
        }

        if is_empty {
            let template = self.element(&block).cloned();
            let mut replacement = Vec::new();
            let mut run = Vec::new();
            for node in middles {
                if inline_only(self, std::slice::from_ref(&node)) {
                    run.push(node);
                } else {
                    if !run.is_empty() {
                        replacement
                            .push(self.block_from_template(&template, std::mem::take(&mut run)));
                    }
                    replacement.push(node);
                }
            }
            if !run.is_empty() {
                replacement.push(self.block_from_template(&template, run));
            }

            let _ = self.apply(Operation::RemoveNode {
                path: block.clone(),
            });
            self.insert_nodes(&child_path(&parent, block_index), replacement.clone());
            if let Some(last) = replacement.len().checked_sub(1) {
                self.select_end_of(&child_path(&parent, block_index + last));
            }
            return;
        }

        if at_start {
            let mut insert_index = block_index;
            if !starts.is_empty() {
                let template = self.element(&block).cloned();
                self.insert_nodes(
                    &child_path(&parent, insert_index),
                    vec![self.block_from_template(&template, starts)],
                );
                insert_index += 1;
            }
            let middle_count = middles.len();
            if middle_count > 0 {
                self.insert_nodes(&child_path(&parent, insert_index), middles);
                insert_index += middle_count;
            }
            let current = child_path(&parent, insert_index);
            if !ends.is_empty() {
                let end_count = ends.len();
                self.prepend_children(&current, ends);
                self.select_end_of(&child_path(&current, end_count - 1));
            } else if insert_index > block_index {
                self.select_end_of(&child_path(&parent, insert_index - 1));
            } else {
                self.select_end_of(&current);
            }
            return;
        }

        if at_end {
            let start_index = self
                .element(&block)
                .map(|element| element.children.len())
                .unwrap_or(0);
            let start_count = starts.len();
            if !starts.is_empty() {
                self.append_children(&block, starts);
            }

            let mut insert_index = block_index + 1;
            let middle_count = middles.len();
            if middle_count > 0 {
                self.insert_nodes(&child_path(&parent, insert_index), middles);
                insert_index += middle_count;
            }

            let mut selection = if middle_count > 0 {
                child_path(&parent, insert_index - 1)
            } else if start_count > 0 {
                child_path(&block, start_index + start_count - 1)
            } else {
                block.clone()
            };

            if !ends.is_empty() {
                let template = self.element(&block).cloned();
                self.insert_nodes(
                    &child_path(&parent, insert_index),
                    vec![self.block_from_template(&template, ends)],
                );
                selection = child_path(&parent, insert_index);
            }
            self.select_end_of(&selection);
            return;
        }

        // Insertion point is inside the destination block: split it into a
        // before/after pair, merge the fragment edges into each side and put
        // whole middle blocks between them.
        self.selection = Some(Range::collapsed(point.clone()));
        let Some(after_index) = self.split_block_at_point(&point, &block) else {
            return;
        };
        if !starts.is_empty() {
            self.append_children(&block, starts);
        }
        let middle_count = middles.len();
        if middle_count > 0 {
            self.insert_nodes(&child_path(&parent, after_index), middles);
        }
        let after_final = child_path(&parent, after_index + middle_count);
        if !ends.is_empty() {
            let end_count = ends.len();
            self.prepend_children(&after_final, ends);
            self.select_end_of(&child_path(&after_final, end_count - 1));
        } else if middle_count > 0 {
            let last_middle = child_path(&parent, after_index + middle_count - 1);
            let is_void = self
                .element(&last_middle)
                .is_some_and(|element| self.is_void_element(element));
            if is_void {
                self.select_start_of(&after_final);
            } else {
                self.select_end_of(&last_middle);
            }
        } else {
            self.select_end_of(&block);
        }
    }

    fn classify_fragment(
        &self,
        nodes: &[Node],
        at_start: bool,
        at_end: bool,
    ) -> (Vec<Node>, Vec<Node>, Vec<Node>) {
        let first = first_leaf_path(nodes);
        let last = last_leaf_path(nodes);
        let mut starts = Vec::new();
        let mut middles = Vec::new();
        let mut ends = Vec::new();
        let mut starting = true;

        self.walk_fragment(
            nodes,
            &mut Vec::new(),
            first.as_deref(),
            last.as_deref(),
            at_start,
            at_end,
            &mut starting,
            &mut starts,
            &mut middles,
            &mut ends,
        );

        (starts, middles, ends)
    }

    #[allow(clippy::too_many_arguments)]
    fn walk_fragment(
        &self,
        nodes: &[Node],
        path: &mut Vec<usize>,
        first: Option<&[usize]>,
        last: Option<&[usize]>,
        at_start: bool,
        at_end: bool,
        starting: &mut bool,
        starts: &mut Vec<Node>,
        middles: &mut Vec<Node>,
        ends: &mut Vec<Node>,
    ) {
        for (index, node) in nodes.iter().enumerate() {
            path.push(index);
            match node {
                Node::Text(_) => {
                    if *starting {
                        starts.push(node.clone());
                    } else {
                        ends.push(node.clone());
                    }
                }
                Node::Element(element) if self.is_inline_element(element) => {
                    if *starting {
                        starts.push(node.clone());
                    } else {
                        ends.push(node.clone());
                    }
                }
                Node::Element(element) => {
                    let contains_first = first.is_some_and(|leaf| is_path_prefix(path, leaf));
                    let contains_last = last.is_some_and(|leaf| is_path_prefix(path, leaf));
                    if *starting && !contains_first {
                        *starting = false;
                    }
                    let should_insert = self.is_void_element(element)
                        || ((at_start || !contains_first) && (at_end || !contains_last));

                    if should_insert {
                        *starting = false;
                        middles.push(node.clone());
                    } else {
                        self.walk_fragment(
                            &element.children,
                            path,
                            first,
                            last,
                            at_start,
                            at_end,
                            starting,
                            starts,
                            middles,
                            ends,
                        );
                    }
                }
            }
            path.pop();
        }
    }

    fn append_children(&mut self, block: &[usize], children: Vec<Node>) {
        if children.is_empty() {
            return;
        }
        let length = self
            .element(block)
            .map(|element| element.children.len())
            .unwrap_or(0);
        self.insert_nodes(&child_path(block, length), children);
    }

    fn prepend_children(&mut self, block: &[usize], children: Vec<Node>) {
        if children.is_empty() {
            return;
        }
        self.insert_nodes(&child_path(block, 0), children);
    }

    fn block_from_template(&self, template: &Option<Element>, children: Vec<Node>) -> Node {
        match template {
            Some(element) => {
                let mut block = element.clone();
                block.children = children;
                Node::Element(block)
            }
            None => Node::Element(Element::paragraph(children)),
        }
    }

    fn select_end_of(&mut self, path: &[usize]) {
        if let Some(point) = self
            .last_text_point_in(path)
            .or_else(|| self.first_text_point_in(path))
        {
            self.selection = Some(Range::collapsed(point));
        }
    }

    fn select_start_of(&mut self, path: &[usize]) {
        if let Some(point) = self
            .first_text_point_in(path)
            .or_else(|| self.last_text_point_in(path))
        {
            self.selection = Some(Range::collapsed(point));
        }
    }

    fn split_inline_at_point(&mut self, point: &Point, inline: &[usize]) -> Option<usize> {
        if point.offset > 0 {
            self.apply_split(&point.path, point.offset, Props::new())
                .ok()?;
        }

        for depth in (inline.len()..point.path.len()).rev() {
            let ancestor = point.path[..depth].to_vec();
            let position = self.selection.as_ref()?.anchor.path.get(depth).copied()?;
            self.apply_split(&ancestor, position, Props::new()).ok()?;
        }

        inline.last().map(|index| index + 1)
    }

    fn split_block_at_point(&mut self, point: &Point, block: &[usize]) -> Option<usize> {
        if point.offset > 0 {
            self.apply_split(&point.path, point.offset, Props::new())
                .ok()?;
        } else {
            self.selection = Some(Range::collapsed(point.clone()));
        }

        for depth in (block.len()..point.path.len()).rev() {
            let ancestor = point.path[..depth].to_vec();
            let position = self.selection.as_ref()?.anchor.path.get(depth).copied()?;
            self.apply_split(&ancestor, position, Props::new()).ok()?;
        }

        block.last().map(|index| index + 1)
    }

    /// Insert an inline node at the caret and move the caret into the text
    /// after it. Used for mentions, inline embeds and other atoms.
    pub fn insert_inline_node(&mut self, node: Node) -> Option<Path> {
        let mut inserted = None;

        self.transaction(|editor| {
            let is_inline = match &node {
                Node::Element(element) => editor.is_inline_element(element),
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
                .map(|element| editor.is_inline_element(element))
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
                Node::Element(element) => editor.is_void_element(element),
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
            // A block inserted from the toolbar should not land inside a feature
            // container such as a list. Features adjust the insertion point
            // (for example, climbing out to insert after the whole list).
            let features = editor.features.snapshot();
            for feature in &features {
                feature.adjust_insert_block(editor, &mut block);
            }
            let parent = block[..block.len() - 1].to_vec();
            let index = *block.last()?;

            let is_void = match &node {
                Node::Element(element) => editor.is_void_element(element),
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
                        !editor.is_void_element(element) && !editor.is_inline_element(element)
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
                .map(|element| self.is_void_element(element))
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
                .map(|element| self.is_void_element(element))
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

fn node_matches(editor: &Editor, node: &Node, kind: NodeMatch) -> bool {
    match kind {
        NodeMatch::Block => node
            .as_element()
            .is_some_and(|element| !editor.is_inline_element(element)),
        NodeMatch::Inline => node
            .as_element()
            .is_some_and(|element| editor.is_inline_element(element)),
        NodeMatch::Text => node.is_text(),
        NodeMatch::TextOrInline => {
            node.is_text()
                || node
                    .as_element()
                    .is_some_and(|element| editor.is_inline_element(element))
        }
        NodeMatch::Any => true,
    }
}

fn collect_matching_paths(
    editor: &Editor,
    node: &Node,
    path: &mut Vec<usize>,
    range: &Range,
    kind: NodeMatch,
    property: Option<&str>,
    out: &mut Vec<Path>,
) {
    if !path.is_empty() && node_intersects_range(editor, node, path, range) {
        let matched = match property {
            Some(property) => node
                .properties()
                .get(property)
                .is_some_and(|value| match value {
                    Value::Bool(value) => *value,
                    Value::Null => false,
                    _ => true,
                }),
            None => node_matches(editor, node, kind),
        };
        if matched {
            out.push(path.clone());
        }
    }

    if let Node::Element(element) = node {
        for (index, child) in element.children.iter().enumerate() {
            path.push(index);
            collect_matching_paths(editor, child, path, range, kind, property, out);
            path.pop();
        }
    }
}

fn node_intersects_range(editor: &Editor, node: &Node, path: &[usize], range: &Range) -> bool {
    let (start, end) = match node {
        Node::Text(text) => (
            Point::new(path.to_vec(), 0),
            Point::new(path.to_vec(), text.len()),
        ),
        Node::Element(_) => (
            editor
                .first_text_point_in(path)
                .unwrap_or_else(|| Point::new(path.to_vec(), 0)),
            editor
                .last_text_point_in(path)
                .unwrap_or_else(|| Point::new(path.to_vec(), 0)),
        ),
    };
    let (range_start, range_end) = range.edges();
    start.compare(range_end) != std::cmp::Ordering::Greater
        && end.compare(range_start) != std::cmp::Ordering::Less
}

fn filter_match_mode(matches: &mut Vec<Path>, mode: MatchMode) {
    let snapshot = matches.clone();
    match mode {
        MatchMode::All => {}
        MatchMode::Lowest => {
            matches.retain(|candidate| {
                !snapshot
                    .iter()
                    .any(|other| other != candidate && is_path_prefix(candidate, other))
            });
        }
        MatchMode::Highest => {
            matches.retain(|candidate| {
                !snapshot
                    .iter()
                    .any(|other| other != candidate && is_path_prefix(other, candidate))
            });
        }
    }
}

fn first_leaf_path(nodes: &[Node]) -> Option<Vec<usize>> {
    for (index, node) in nodes.iter().enumerate() {
        match node {
            Node::Text(_) => return Some(vec![index]),
            Node::Element(element) => {
                if let Some(mut path) = first_leaf_path(&element.children) {
                    path.insert(0, index);
                    return Some(path);
                }
            }
        }
    }
    None
}

fn last_leaf_path(nodes: &[Node]) -> Option<Vec<usize>> {
    for (index, node) in nodes.iter().enumerate().rev() {
        match node {
            Node::Text(_) => return Some(vec![index]),
            Node::Element(element) => {
                if let Some(mut path) = last_leaf_path(&element.children) {
                    path.insert(0, index);
                    return Some(path);
                }
            }
        }
    }
    None
}

fn is_path_prefix(prefix: &[usize], path: &[usize]) -> bool {
    prefix.len() <= path.len() && path[..prefix.len()] == *prefix
}

fn contains_void_or_read_only(editor: &Editor, node: &Node) -> bool {
    match node {
        Node::Element(element) => {
            editor.is_void_element(element)
                || matches!(element.properties.get("readOnly"), Some(Value::Bool(true)))
                || element
                    .children
                    .iter()
                    .any(|child| contains_void_or_read_only(editor, child))
        }
        Node::Text(_) => false,
    }
}

fn child_path(parent: &[usize], index: usize) -> Path {
    let mut path = parent.to_vec();
    path.push(index);
    path
}

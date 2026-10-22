//! The editor: document value, selection, marks, operations and history.

use serde_json::{json, Value};

use crate::{
    history::{History, HistoryBatch},
    node::{Element, Node, Text},
    operation::Operation,
    path::{self, Affinity},
    point::Point,
    props::{merge as merge_props, Props},
    range::Range,
    schema::Schema,
    text::{char_len, char_slice, splice},
    SlateError,
};

/// A pending transaction, grouping operations for undo/redo.
#[derive(Clone, Debug)]
struct PendingBatch {
    ops: Vec<(Operation, Operation)>,
    before_selection: Option<Range>,
    after_selection: Option<Range>,
    before_node_selection: Option<path::Path>,
    before_node_selection_end: Option<path::Path>,
    after_node_selection: Option<path::Path>,
    after_node_selection_end: Option<path::Path>,
}

impl PendingBatch {
    fn is_empty(&self) -> bool {
        self.ops.iter().all(|(op, _)| op.is_selection_only())
    }
}

/// The editor state and transformation API.
///
/// This is deliberately close to Slate's editor: a document (`children`), a
/// selection, active marks, a schema, and an undo history.
#[derive(Clone, Debug)]
pub struct Editor {
    pub children: Vec<Node>,
    pub selection: Option<Range>,
    /// An explicit block/node selection, used for void blocks such as images.
    /// Mutually exclusive with a text [`Range`] selection.
    pub node_selection: Option<path::Path>,
    /// The focus end of a multi-block node selection.
    pub node_selection_end: Option<path::Path>,
    pub marks: Option<Props>,
    pub schema: Schema,
    pub history: History,
    batch: Option<PendingBatch>,
    pub(crate) is_normalizing: bool,
}

impl Default for Editor {
    fn default() -> Self {
        Self::empty()
    }
}

impl Editor {
    /// Create an editor from a document. The document is normalized and the
    /// caret is placed at its start.
    pub fn new(children: Vec<Node>) -> Self {
        let mut editor = Self {
            children,
            selection: None,
            node_selection: None,
            node_selection_end: None,
            marks: None,
            schema: Schema::default(),
            history: History::default(),
            batch: None,
            is_normalizing: false,
        };

        if editor.children.is_empty() {
            editor.children.push(Node::Element(Element::empty("p")));
        }

        editor.normalize();
        editor.history = History::default();
        editor.selection = editor.first_text_point_in(&[]).map(Range::collapsed);
        editor
    }

    pub fn empty() -> Self {
        Self::new(vec![Node::Element(Element::empty("p"))])
    }

    pub fn with_schema(mut self, schema: Schema) -> Self {
        self.schema = schema;
        self
    }

    pub fn set_schema(&mut self, schema: Schema) {
        self.schema = schema;
        self.normalize();
    }

    pub fn from_value(children: Vec<Node>) -> Self {
        Self::new(children)
    }

    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        let children = serde_json::from_str::<Vec<Node>>(json)?;
        Ok(Self::new(children))
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(&self.children).expect("nodes always serialize")
    }

    pub fn to_json_pretty(&self) -> String {
        serde_json::to_string_pretty(&self.children).expect("nodes always serialize")
    }

    pub fn to_value(&self) -> Value {
        serde_json::to_value(&self.children).expect("nodes always serialize")
    }

    pub fn document_string(&self) -> String {
        let mut out = String::new();
        for child in &self.children {
            append_document_string(child, &mut out);
            out.push('\n');
        }
        out
    }

    pub fn is_empty(&self) -> bool {
        self.children.iter().all(|child| child.string().is_empty())
    }

    // ---------------------------------------------------------------------
    // Node access
    // ---------------------------------------------------------------------

    pub fn node(&self, at: &[usize]) -> Option<&Node> {
        if at.is_empty() {
            return None;
        }
        let mut node = self.children.get(at[0])?;
        for index in &at[1..] {
            node = node.children().get(*index)?;
        }
        Some(node)
    }

    pub fn node_mut(&mut self, at: &[usize]) -> Option<&mut Node> {
        if at.is_empty() {
            return None;
        }
        let mut node = self.children.get_mut(at[0])?;
        for index in &at[1..] {
            node = node.children_mut().get_mut(*index)?;
        }
        Some(node)
    }

    pub fn text(&self, at: &[usize]) -> Option<&Text> {
        self.node(at).and_then(Node::as_text)
    }

    pub fn element(&self, at: &[usize]) -> Option<&Element> {
        self.node(at).and_then(Node::as_element)
    }

    pub fn parent(&self, at: &[usize]) -> Option<&Node> {
        let parent = path::parent(at)?;
        self.node(&parent)
    }

    /// Children of the node at a path, treating the empty path as the root.
    pub fn children_at(&self, at: &[usize]) -> Option<&[Node]> {
        if at.is_empty() {
            Some(&self.children)
        } else {
            self.node(at).map(Node::children)
        }
    }

    pub fn text_len_at(&self, at: &[usize]) -> Option<usize> {
        self.text(at).map(Text::len)
    }

    /// Descend to the leaf text node at (or below) a point's path.
    pub fn text_point(&self, point: &Point) -> Option<Point> {
        match self.node(&point.path)? {
            Node::Text(text) => Some(Point::new(point.path.clone(), point.offset.min(text.len()))),
            Node::Element(_) => self.first_text_point_in(&point.path),
        }
    }

    pub fn first_text_point_in(&self, at: &[usize]) -> Option<Point> {
        if at.is_empty() {
            let child = self.children.first()?;
            return self.first_text_point_of(child, vec![0]);
        }
        let node = self.node(at)?;
        self.first_text_point_of(node, at.to_vec())
    }

    fn first_text_point_of(&self, node: &Node, at: Vec<usize>) -> Option<Point> {
        match node {
            Node::Text(_) => Some(Point::new(at, 0)),
            Node::Element(element) => {
                let child = element.children.first()?;
                let mut at = at;
                at.push(0);
                self.first_text_point_of(child, at)
            }
        }
    }

    pub fn last_text_point_in(&self, at: &[usize]) -> Option<Point> {
        if at.is_empty() {
            let index = self.children.len().checked_sub(1)?;
            let child = self.children.get(index)?;
            return self.last_text_point_of(child, vec![index]);
        }
        let node = self.node(at)?;
        self.last_text_point_of(node, at.to_vec())
    }

    fn last_text_point_of(&self, node: &Node, at: Vec<usize>) -> Option<Point> {
        match node {
            Node::Text(text) => Some(Point::new(at, text.len())),
            Node::Element(element) => {
                let index = element.children.len().checked_sub(1)?;
                let child = element.children.get(index)?;
                let mut at = at;
                at.push(index);
                self.last_text_point_of(child, at)
            }
        }
    }

    fn nearest_text_point(&self, at: &[usize]) -> Option<Point> {
        if let Some(point) = self.first_text_point_in(at) {
            return Some(point);
        }

        let mut current = at.to_vec();
        loop {
            if let Some(last) = current.last().copied() {
                let parent = current[..current.len() - 1].to_vec();
                if last > 0 {
                    let mut previous = parent.clone();
                    previous.push(last - 1);
                    if let Some(point) = self.last_text_point_in(&previous) {
                        return Some(point);
                    }
                }
                let mut next = parent.clone();
                next.push(last + 1);
                if let Some(point) = self.first_text_point_in(&next) {
                    return Some(point);
                }
            }
            if current.is_empty() {
                return None;
            }
            current.pop();
        }
    }

    pub fn start_point(&self) -> Option<Point> {
        self.first_text_point_in(&[])
    }

    pub fn end_point(&self) -> Option<Point> {
        self.last_text_point_in(&[])
    }

    /// Expand the selection to the whole document.
    pub fn select_all(&mut self) {
        self.node_selection = None;
        self.node_selection_end = None;
        if let (Some(start), Some(end)) = (self.start_point(), self.end_point()) {
            self.selection = Some(Range::new(start, end));
            self.history.break_coalescing();
        }
    }

    pub fn select(&mut self, range: Range) {
        self.selection = Some(range);
        self.node_selection = None;
        self.node_selection_end = None;
        self.marks = None;
        self.history.break_coalescing();
    }

    pub fn select_point(&mut self, point: Point) {
        self.select(Range::collapsed(point));
    }

    pub fn deselect(&mut self) {
        self.selection = None;
        self.node_selection = None;
        self.node_selection_end = None;
        self.marks = None;
        self.history.break_coalescing();
    }

    /// Select an element as a block, clearing any text selection.
    ///
    /// The selection is stored as the top-level block containing `at`, so
    /// multi-block ranges stay simple and stable.
    pub fn select_node(&mut self, at: &[usize]) {
        let Some(index) = at.first().copied() else {
            return;
        };
        if self
            .children
            .get(index)
            .and_then(Node::as_element)
            .is_none()
        {
            return;
        }
        self.selection = None;
        self.marks = None;
        self.node_selection = Some(vec![index]);
        self.node_selection_end = None;
        self.history.break_coalescing();
    }

    /// Select a contiguous range of top-level blocks.
    pub fn select_node_range(&mut self, anchor: &[usize], focus: &[usize]) {
        let (Some(anchor), Some(focus)) = (anchor.first().copied(), focus.first().copied()) else {
            return;
        };
        if anchor >= self.children.len() || focus >= self.children.len() {
            return;
        }
        self.selection = None;
        self.marks = None;
        self.node_selection = Some(vec![anchor]);
        self.node_selection_end = (anchor != focus).then(|| vec![focus]);
        self.history.break_coalescing();
    }

    /// Extend the node selection to another top-level block.
    pub fn extend_node_selection_to(&mut self, at: &[usize]) {
        let Some(index) = at.first().copied() else {
            return;
        };
        if index >= self.children.len() {
            return;
        }
        if self.node_selection.is_none() {
            self.select_node(at);
            return;
        }
        let anchor = self.node_selection.as_ref().unwrap()[0];
        self.selection = None;
        self.marks = None;
        self.node_selection_end = (anchor != index).then(|| vec![index]);
        self.history.break_coalescing();
    }

    /// Extend a node selection by one top-level block.
    pub fn extend_node_selection(&mut self, direction: isize) -> bool {
        let Some(anchor) = self
            .node_selection
            .as_ref()
            .and_then(|path| path.first().copied())
        else {
            return false;
        };
        let focus = self
            .node_selection_end
            .as_ref()
            .and_then(|path| path.first().copied())
            .unwrap_or(anchor);
        let next = focus as isize + direction;
        if next < 0 || next as usize >= self.children.len() {
            return false;
        }
        let next = next as usize;
        self.selection = None;
        self.marks = None;
        self.node_selection_end = (anchor != next).then(|| vec![next]);
        self.history.break_coalescing();
        true
    }

    /// Move a single-block selection to the previous/next block, or collapse a
    /// range to its first/last block.
    pub fn move_node_selection(&mut self, direction: isize) -> bool {
        let selected = self.node_selections();
        let (Some(first), Some(last)) = (
            selected.first().and_then(|path| path.first().copied()),
            selected.last().and_then(|path| path.first().copied()),
        ) else {
            return false;
        };
        if self.node_selection_end.is_some() {
            let target = if direction < 0 { first } else { last };
            self.node_selection = Some(vec![target]);
            self.node_selection_end = None;
            self.history.break_coalescing();
            return true;
        }
        let next = first as isize + direction;
        if next < 0 || next as usize >= self.children.len() {
            return false;
        }
        self.select_node(&[next as usize]);
        true
    }

    pub fn clear_node_selection(&mut self) {
        self.node_selection = None;
        self.node_selection_end = None;
    }

    pub fn has_node_selection(&self) -> bool {
        self.node_selection.is_some()
    }

    pub fn node_selection(&self) -> Option<&[usize]> {
        self.node_selection.as_deref()
    }

    pub fn node_selection_end(&self) -> Option<&[usize]> {
        self.node_selection_end.as_deref()
    }

    /// All top-level paths in the node selection.
    pub fn node_selections(&self) -> Vec<path::Path> {
        let Some(anchor) = self
            .node_selection
            .as_ref()
            .and_then(|path| path.first().copied())
        else {
            return Vec::new();
        };
        let focus = self
            .node_selection_end
            .as_ref()
            .and_then(|path| path.first().copied())
            .unwrap_or(anchor);
        let (start, end) = if anchor <= focus {
            (anchor, focus)
        } else {
            (focus, anchor)
        };
        (start..=end)
            .filter(|index| *index < self.children.len())
            .map(|index| vec![index])
            .collect()
    }

    /// Whether `at` is one of the selected top-level blocks.
    pub fn is_node_selected(&self, at: &[usize]) -> bool {
        self.node_selections().iter().any(|path| path == at)
    }

    pub fn node_selection_string(&self) -> Option<String> {
        let paths = self.node_selections();
        if paths.is_empty() {
            return None;
        }
        Some(
            paths
                .iter()
                .filter_map(|path| self.node(path))
                .map(Node::string)
                .collect::<Vec<_>>()
                .join("\n"),
        )
    }

    /// Delete the selected block and move the caret to a neighbour.
    pub fn delete_node_selection(&mut self) -> bool {
        if self.node_selection.is_none() {
            return false;
        }
        self.transaction(|editor| {
            editor.delete_selected_node_inner();
        });
        if self.selection.is_none() {
            self.selection = self.start_point().map(Range::collapsed);
        }
        true
    }

    /// Duplicate every block in the node selection and select the copies.
    pub fn duplicate_node_selection(&mut self) -> bool {
        let paths = self.node_selections();
        if paths.is_empty() {
            return false;
        }
        self.transaction(|editor| {
            let last = paths.last().and_then(|path| path.first().copied())?;
            let nodes: Vec<Node> = paths
                .iter()
                .filter_map(|path| editor.node(path).cloned())
                .collect();
            let count = nodes.len();
            for (offset, node) in nodes.into_iter().enumerate() {
                editor
                    .apply(Operation::InsertNode {
                        path: vec![last + 1 + offset],
                        node,
                    })
                    .ok()?;
            }
            editor.node_selection = Some(vec![last + 1]);
            editor.node_selection_end = (count > 1).then(|| vec![last + count]);
            editor.selection = None;
            Some(())
        })
        .is_some()
    }

    /// Move out of a block selection into the previous or next text block.
    pub fn move_selection_off_node(&mut self, reverse: bool) -> Option<Point> {
        if self.node_selection_end.is_some() {
            let selected = self.node_selections();
            let target_path = if reverse {
                selected.first()
            } else {
                selected.last()
            }?;
            let point = if reverse {
                self.first_text_point_in(target_path)?
            } else {
                self.last_text_point_in(target_path)?
            };
            self.clear_node_selection();
            self.selection = Some(Range::collapsed(point.clone()));
            self.marks = None;
            self.history.break_coalescing();
            return Some(point);
        }

        let path = self.node_selection.take()?;
        self.node_selection_end = None;
        if path.is_empty() {
            return None;
        }
        let parent = path[..path.len() - 1].to_vec();
        let index = *path.last()?;
        let is_void = |editor: &Self, path: &[usize]| {
            editor
                .node(path)
                .and_then(Node::as_element)
                .map(|element| editor.schema.is_void(&element.element_type))
                .unwrap_or(false)
        };

        let target = if reverse {
            index.checked_sub(1).and_then(|previous| {
                let mut previous_path = parent.clone();
                previous_path.push(previous);
                (!is_void(self, &previous_path))
                    .then(|| self.last_text_point_in(&previous_path))
                    .flatten()
            })
        } else {
            let mut next_path = parent.clone();
            next_path.push(index + 1);
            (!is_void(self, &next_path))
                .then(|| self.first_text_point_in(&next_path))
                .flatten()
        };

        if let Some(point) = target {
            self.selection = Some(Range::collapsed(point.clone()));
            self.marks = None;
            self.history.break_coalescing();
            Some(point)
        } else {
            None
        }
    }

    pub fn selection_string(&self) -> Option<String> {
        let range = self.selection.as_ref()?;
        if range.is_collapsed() {
            return Some(String::new());
        }
        let (start, end) = range.edges();
        let mut out = String::new();
        self.collect_text_between(start, end, &mut out);
        Some(out)
    }

    fn collect_text_between(&self, start: &Point, end: &Point, out: &mut String) {
        self.collect_text_between_node(&self.children_as_node(), &[], start, end, out);
    }

    fn children_as_node(&self) -> Node {
        Node::Element(Element::new("root", self.children.clone()))
    }

    fn collect_text_between_node(
        &self,
        node: &Node,
        at: &[usize],
        start: &Point,
        end: &Point,
        out: &mut String,
    ) {
        match node {
            Node::Text(text) => {
                let point = Point::new(at.to_vec(), 0);
                if point.compare(end) == std::cmp::Ordering::Greater {
                    return;
                }
                let len = text.len();
                let start_offset = if at == start.path {
                    start.offset.min(len)
                } else {
                    0
                };
                let end_offset = if at == end.path {
                    end.offset.min(len)
                } else {
                    len
                };
                if start_offset < end_offset {
                    out.push_str(char_slice(&text.text, start_offset, end_offset));
                }
            }
            Node::Element(element) => {
                for (index, child) in element.children.iter().enumerate() {
                    let mut child_at = at.to_vec();
                    child_at.push(index);
                    self.collect_text_between_node(child, &child_at, start, end, out);
                }
            }
        }
    }

    /// Text paths that intersect a non-collapsed range.
    pub fn text_paths_in_range(&self, range: &Range) -> Vec<path::Path> {
        let mut out = Vec::new();
        if range.is_collapsed() {
            return out;
        }
        let (start, end) = range.edges();
        self.collect_text_paths(&self.children_as_node(), &[], start, end, &mut out);
        out
    }

    fn collect_text_paths(
        &self,
        node: &Node,
        at: &[usize],
        start: &Point,
        end: &Point,
        out: &mut Vec<path::Path>,
    ) {
        match node {
            Node::Text(text) => {
                let leaf_start = Point::new(at.to_vec(), 0);
                let leaf_end = Point::new(at.to_vec(), text.len());
                if leaf_start.compare(end) == std::cmp::Ordering::Less
                    && leaf_end.compare(start) == std::cmp::Ordering::Greater
                {
                    out.push(at.to_vec());
                }
            }
            Node::Element(element) => {
                for (index, child) in element.children.iter().enumerate() {
                    let mut child_at = at.to_vec();
                    child_at.push(index);
                    self.collect_text_paths(child, &child_at, start, end, out);
                }
            }
        }
    }

    /// Marks shared by the text nodes in the current selection.
    pub fn marks_at_selection(&self) -> Props {
        if let Some(marks) = &self.marks {
            return marks.clone();
        }
        let Some(range) = &self.selection else {
            return Props::new();
        };
        if range.is_collapsed() {
            let point = self.text_point(range.start());
            return point
                .and_then(|point| self.text(&point.path).map(|t| t.properties.clone()))
                .unwrap_or_default();
        }
        let paths = self.text_paths_in_range(range);
        let mut result: Option<Props> = None;
        for at in paths {
            let props = match self.text(&at) {
                Some(text) => text.properties.clone(),
                None => continue,
            };
            result = Some(match result {
                None => props,
                Some(current) => current
                    .into_iter()
                    .filter(|(key, value)| props.get(key) == Some(value))
                    .collect(),
            });
        }
        result.unwrap_or_default()
    }

    /// True when every text node in the selection carries `key = value`.
    pub fn has_mark(&self, key: &str, value: &Value) -> bool {
        let Some(range) = &self.selection else {
            return self
                .marks
                .as_ref()
                .and_then(|marks| marks.get(key))
                .map(|v| v == value)
                .unwrap_or(false);
        };
        if range.is_collapsed() {
            if let Some(marks) = &self.marks {
                return marks.get(key).map(|v| v == value).unwrap_or(false);
            }
            return self
                .text_point(range.start())
                .and_then(|point| {
                    self.text(&point.path)
                        .map(|t| t.properties.get(key) == Some(value))
                })
                .unwrap_or(false);
        }
        let paths = self.text_paths_in_range(range);
        !paths.is_empty()
            && paths.iter().all(|at| {
                self.text(at)
                    .map(|t| t.properties.get(key) == Some(value))
                    .unwrap_or(false)
            })
    }

    // ---------------------------------------------------------------------
    // Transactions and operations
    // ---------------------------------------------------------------------

    /// Run `f` as one undoable transaction. Nested calls reuse the outer
    /// transaction. The document is normalized at the end of the transaction.
    pub fn transaction<R>(&mut self, f: impl FnOnce(&mut Self) -> R) -> R {
        if self.batch.is_some() {
            return f(self);
        }

        self.batch = Some(PendingBatch {
            ops: Vec::new(),
            before_selection: self.selection.clone(),
            after_selection: None,
            before_node_selection: self.node_selection.clone(),
            before_node_selection_end: self.node_selection_end.clone(),
            after_node_selection: None,
            after_node_selection_end: None,
        });

        let result = f(self);
        self.normalize();
        self.finish_batch();
        result
    }

    /// Apply a single operation (public escape hatch).
    pub fn apply_op(&mut self, op: Operation) -> Result<(), SlateError> {
        self.apply(op)
    }

    pub(crate) fn apply(&mut self, op: Operation) -> Result<(), SlateError> {
        if self.batch.is_some() || self.is_normalizing {
            return self.apply_inner(op);
        }

        self.batch = Some(PendingBatch {
            ops: Vec::new(),
            before_selection: self.selection.clone(),
            after_selection: None,
            before_node_selection: self.node_selection.clone(),
            before_node_selection_end: self.node_selection_end.clone(),
            after_node_selection: None,
            after_node_selection_end: None,
        });

        let result = self.apply_inner(op);
        self.normalize();
        self.finish_batch();
        result
    }

    fn apply_inner(&mut self, op: Operation) -> Result<(), SlateError> {
        let inverse = self.inverse_of(&op)?;
        self.apply_value(&op)?;

        if let Some(node) = &self.node_selection {
            self.node_selection = path::transform_with_affinity(node, &op, Affinity::Forward)
                .and_then(|path| path.first().copied())
                .map(|index| vec![index]);
        }
        if let Some(node) = &self.node_selection_end {
            self.node_selection_end = path::transform_with_affinity(node, &op, Affinity::Forward)
                .and_then(|path| path.first().copied())
                .map(|index| vec![index]);
        }
        if self.node_selection.is_none() || self.node_selection_end == self.node_selection {
            self.node_selection_end = None;
        }

        if let Operation::SetSelection { new_properties, .. } = &op {
            self.node_selection = None;
            self.node_selection_end = None;
            self.selection = new_properties.clone();
        } else if let Some(selection) = &self.selection {
            self.selection = selection.transform(&op, Affinity::Forward);
        }

        if let Some(batch) = &mut self.batch {
            batch.ops.push((op, inverse));
        }
        Ok(())
    }

    fn finish_batch(&mut self) {
        if let Some(mut batch) = self.batch.take() {
            if !batch.is_empty() {
                batch.after_selection = self.selection.clone();
                batch.after_node_selection = self.node_selection.clone();
                batch.after_node_selection_end = self.node_selection_end.clone();
                self.history.push(HistoryBatch {
                    ops: batch.ops,
                    before_selection: batch.before_selection,
                    after_selection: batch.after_selection,
                    before_node_selection: batch.before_node_selection,
                    before_node_selection_end: batch.before_node_selection_end,
                    after_node_selection: batch.after_node_selection,
                    after_node_selection_end: batch.after_node_selection_end,
                });
            }
        }
        self.sanitize_selection();
    }

    pub(crate) fn sanitize_selection(&mut self) {
        if let Some(anchor) = self
            .node_selection
            .as_ref()
            .and_then(|path| path.first().copied())
        {
            if anchor >= self.children.len() || self.children[anchor].as_element().is_none() {
                self.node_selection = None;
                self.node_selection_end = None;
            } else {
                if let Some(focus) = self
                    .node_selection_end
                    .as_ref()
                    .and_then(|path| path.first().copied())
                {
                    if focus >= self.children.len() || self.children[focus].as_element().is_none() {
                        self.node_selection_end = None;
                    }
                }
                if self
                    .node_selection_end
                    .as_ref()
                    .and_then(|path| path.first().copied())
                    == Some(anchor)
                {
                    self.node_selection_end = None;
                }
                self.selection = None;
                return;
            }
        }

        let Some(range) = self.selection.clone() else {
            return;
        };
        let anchor = self.sanitize_point(&range.anchor);
        let focus = self.sanitize_point(&range.focus);
        self.selection = match (anchor, focus) {
            (Some(anchor), Some(focus)) => Some(Range::new(anchor, focus)),
            _ => None,
        };
    }

    fn sanitize_point(&self, point: &Point) -> Option<Point> {
        match self.node(&point.path) {
            Some(Node::Text(text)) => {
                Some(Point::new(point.path.clone(), point.offset.min(text.len())))
            }
            Some(Node::Element(_)) => self.first_text_point_in(&point.path),
            None => self.nearest_text_point(&point.path),
        }
    }

    fn inverse_of(&self, op: &Operation) -> Result<Operation, SlateError> {
        Ok(match op {
            Operation::InsertNode { path, .. } => Operation::RemoveNode { path: path.clone() },
            Operation::InsertText { path, offset, text } => Operation::RemoveText {
                path: path.clone(),
                offset: *offset,
                text: text.clone(),
            },
            Operation::RemoveNode { path } => Operation::InsertNode {
                path: path.clone(),
                node: self
                    .node(path)
                    .cloned()
                    .ok_or_else(|| SlateError::InvalidPath(path.clone()))?,
            },
            Operation::RemoveText { path, offset, text } => Operation::InsertText {
                path: path.clone(),
                offset: *offset,
                text: text.clone(),
            },
            Operation::MergeNode { path, .. } => {
                let previous = path::previous(path)
                    .ok_or_else(|| SlateError::InvalidOperation("no previous sibling".into()))?;
                let previous_node = self
                    .node(&previous)
                    .ok_or_else(|| SlateError::InvalidPath(previous.clone()))?;
                let node = self
                    .node(path)
                    .ok_or_else(|| SlateError::InvalidPath(path.clone()))?;
                let position = match previous_node {
                    Node::Text(text) => text.len(),
                    Node::Element(element) => element.children.len(),
                };
                let mut properties = node.properties().clone();
                if let Node::Element(element) = node {
                    properties.insert("type".into(), json!(element.element_type));
                }
                Operation::SplitNode {
                    path: previous,
                    position,
                    properties,
                }
            }
            Operation::MoveNode { path, new_path } => Operation::MoveNode {
                path: new_path.clone(),
                new_path: path.clone(),
            },
            Operation::SetNode {
                path,
                properties,
                new_properties,
            } => {
                let node = self
                    .node(path)
                    .ok_or_else(|| SlateError::InvalidPath(path.clone()))?;
                // The inverse must restore the values that existed before the
                // operation, for every key the operation touched.
                let mut previous = properties.clone();
                for key in new_properties.keys() {
                    if previous.contains_key(key) {
                        continue;
                    }
                    if key == "type" {
                        if let Node::Element(element) = node {
                            previous.insert("type".into(), json!(element.element_type));
                        }
                    } else if let Some(value) = node.properties().get(key) {
                        previous.insert(key.clone(), value.clone());
                    } else {
                        previous.insert(key.clone(), Value::Null);
                    }
                }
                Operation::SetNode {
                    path: path.clone(),
                    properties: new_properties.clone(),
                    new_properties: previous,
                }
            }
            Operation::SplitNode { path, position, .. } => Operation::MergeNode {
                path: path::next(path),
                position: *position,
            },
            Operation::SetSelection { properties, .. } => Operation::SetSelection {
                properties: match op {
                    Operation::SetSelection { new_properties, .. } => new_properties.clone(),
                    _ => unreachable!(),
                },
                new_properties: properties.clone(),
            },
        })
    }

    fn apply_value(&mut self, op: &Operation) -> Result<(), SlateError> {
        match op {
            Operation::InsertNode { path: at, node } => self.insert_node_value(at, node.clone()),
            Operation::RemoveNode { path: at } => self.remove_node_value(at),
            Operation::InsertText {
                path: at,
                offset,
                text,
            } => {
                let node = self
                    .node_mut(at)
                    .ok_or_else(|| SlateError::InvalidPath(at.clone()))?;
                let text_node = node
                    .as_text_mut()
                    .ok_or_else(|| SlateError::ExpectedText(at.clone()))?;
                if *offset > text_node.len() {
                    return Err(SlateError::InvalidOffset {
                        path: at.clone(),
                        offset: *offset,
                        len: text_node.len(),
                    });
                }
                splice(&mut text_node.text, *offset, *offset, text);
                Ok(())
            }
            Operation::RemoveText {
                path: at,
                offset,
                text,
            } => {
                let node = self
                    .node_mut(at)
                    .ok_or_else(|| SlateError::InvalidPath(at.clone()))?;
                let text_node = node
                    .as_text_mut()
                    .ok_or_else(|| SlateError::ExpectedText(at.clone()))?;
                let end = offset + char_len(text);
                if *offset > text_node.len() || end > text_node.len() {
                    return Err(SlateError::InvalidOffset {
                        path: at.clone(),
                        offset: *offset,
                        len: text_node.len(),
                    });
                }
                splice(&mut text_node.text, *offset, end, "");
                Ok(())
            }
            Operation::MergeNode { path: at, .. } => self.merge_node_value(at),
            Operation::SplitNode {
                path: at,
                position,
                properties,
            } => self.split_node_value(at, *position, properties),
            Operation::MoveNode {
                path: from,
                new_path,
            } => {
                let node = self
                    .node(from)
                    .cloned()
                    .ok_or_else(|| SlateError::InvalidPath(from.clone()))?;
                self.remove_node_value(from)?;
                self.insert_node_value(new_path, node)
            }
            Operation::SetNode {
                path: at,
                properties,
                new_properties,
            } => {
                let node = self
                    .node_mut(at)
                    .ok_or_else(|| SlateError::InvalidPath(at.clone()))?;
                apply_set_node(node, properties, new_properties);
                Ok(())
            }
            Operation::SetSelection { new_properties, .. } => {
                self.selection = new_properties.clone();
                Ok(())
            }
        }
    }

    fn insert_node_value(&mut self, at: &[usize], node: Node) -> Result<(), SlateError> {
        if at.is_empty() {
            return Err(SlateError::InvalidOperation(
                "cannot insert at the root path".into(),
            ));
        }
        let index = *at.last().unwrap();
        let parent = &at[..at.len() - 1];
        let children = self
            .children_mut_at(parent)
            .ok_or_else(|| SlateError::InvalidPath(at.to_vec()))?;
        if index > children.len() {
            return Err(SlateError::InvalidPath(at.to_vec()));
        }
        children.insert(index, node);
        Ok(())
    }

    fn remove_node_value(&mut self, at: &[usize]) -> Result<(), SlateError> {
        if at.is_empty() {
            return Err(SlateError::InvalidOperation(
                "cannot remove the root".into(),
            ));
        }
        let index = *at.last().unwrap();
        let parent = &at[..at.len() - 1];
        let children = self
            .children_mut_at(parent)
            .ok_or_else(|| SlateError::InvalidPath(at.to_vec()))?;
        if index >= children.len() {
            return Err(SlateError::InvalidPath(at.to_vec()));
        }
        children.remove(index);
        Ok(())
    }

    fn merge_node_value(&mut self, at: &[usize]) -> Result<(), SlateError> {
        let previous = path::previous(at)
            .ok_or_else(|| SlateError::InvalidOperation("no previous sibling".into()))?;
        let node = self
            .node(at)
            .cloned()
            .ok_or_else(|| SlateError::InvalidPath(at.to_vec()))?;
        let previous_node = self
            .node(&previous)
            .cloned()
            .ok_or_else(|| SlateError::InvalidPath(previous.clone()))?;

        match (&previous_node, &node) {
            (Node::Text(_), Node::Text(_)) => {}
            (Node::Element(previous_element), Node::Element(element)) => {
                if previous_element.is_text_only() != element.is_text_only() {
                    return Err(SlateError::InvalidOperation(
                        "cannot merge blocks with different child kinds".into(),
                    ));
                }
            }
            _ => {
                return Err(SlateError::InvalidOperation(format!(
                    "cannot merge a {} node into a {} node",
                    node_kind(&node),
                    node_kind(&previous_node)
                )));
            }
        }

        self.remove_node_value(at)?;

        let previous_node = self
            .node_mut(&previous)
            .ok_or_else(|| SlateError::InvalidPath(previous.clone()))?;
        match (previous_node, node) {
            (Node::Text(previous), Node::Text(node)) => {
                previous.text.push_str(&node.text);
                Ok(())
            }
            (Node::Element(previous), Node::Element(node)) => {
                previous.children.extend(node.children);
                Ok(())
            }
            _ => unreachable!(),
        }
    }

    fn split_node_value(
        &mut self,
        at: &[usize],
        position: usize,
        properties: &Props,
    ) -> Result<(), SlateError> {
        let node = self
            .node(at)
            .cloned()
            .ok_or_else(|| SlateError::InvalidPath(at.to_vec()))?;

        let second = match node {
            Node::Text(text) => {
                let len = text.len();
                if position > len {
                    return Err(SlateError::InvalidOffset {
                        path: at.to_vec(),
                        offset: position,
                        len,
                    });
                }
                let mut second_properties = text.properties.clone();
                merge_props(&mut second_properties, properties);
                let rest = char_slice(&text.text, position, len).to_string();
                let first_text = char_slice(&text.text, 0, position).to_string();
                if let Some(Node::Text(first)) = self.node_mut(at) {
                    first.text = first_text;
                }
                Node::Text(Text {
                    text: rest,
                    properties: second_properties,
                })
            }
            Node::Element(element) => {
                if position > element.children.len() {
                    return Err(SlateError::InvalidOperation(
                        "split position outside element children".into(),
                    ));
                }
                let mut second_properties = element.properties.clone();
                merge_props(&mut second_properties, properties);
                let second_type = properties
                    .get("type")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                second_properties.remove("type");
                let mut first = element.clone();
                let rest = first.children.split_off(position);
                if let Some(Node::Element(target)) = self.node_mut(at) {
                    *target = first;
                }
                Node::Element(Element {
                    element_type: second_type.unwrap_or(element.element_type),
                    properties: second_properties,
                    children: rest,
                })
            }
        };

        self.insert_node_value(&path::next(at), second)
    }

    fn children_mut_at(&mut self, at: &[usize]) -> Option<&mut Vec<Node>> {
        if at.is_empty() {
            Some(&mut self.children)
        } else {
            self.node_mut(at)?.as_element_mut().map(|e| &mut e.children)
        }
    }

    pub(crate) fn apply_merge(&mut self, at: &[usize]) -> Result<(), SlateError> {
        let previous_path = path::previous(at)
            .ok_or_else(|| SlateError::InvalidOperation("no previous sibling".into()))?;
        let position = match self.node(&previous_path) {
            Some(Node::Text(text)) => text.len(),
            Some(Node::Element(element)) => element.children.len(),
            None => return Err(SlateError::InvalidPath(previous_path)),
        };
        self.apply(Operation::MergeNode {
            path: at.to_vec(),
            position,
        })
    }

    pub(crate) fn apply_split(
        &mut self,
        at: &[usize],
        position: usize,
        properties: Props,
    ) -> Result<(), SlateError> {
        self.apply(Operation::SplitNode {
            path: at.to_vec(),
            position,
            properties,
        })
    }

    // ---------------------------------------------------------------------
    // History
    // ---------------------------------------------------------------------

    pub fn can_undo(&self) -> bool {
        self.history.can_undo()
    }

    pub fn can_redo(&self) -> bool {
        self.history.can_redo()
    }

    pub fn undo(&mut self) -> bool {
        if self.batch.is_some() {
            return false;
        }
        let Some(batch) = self.history.undo_stack.pop() else {
            return false;
        };
        self.history.break_coalescing();
        for (_, inverse) in batch.ops.iter().rev() {
            let _ = self.apply_value(inverse);
        }
        self.selection = batch.before_selection.clone();
        self.node_selection = batch.before_node_selection.clone();
        self.node_selection_end = batch.before_node_selection_end.clone();
        self.marks = None;
        self.sanitize_selection();
        if self.selection.is_none() && self.node_selection.is_none() {
            self.selection = self.start_point().map(Range::collapsed);
        }
        self.history.redo_stack.push(batch);
        true
    }

    pub fn redo(&mut self) -> bool {
        if self.batch.is_some() {
            return false;
        }
        let Some(batch) = self.history.redo_stack.pop() else {
            return false;
        };
        self.history.break_coalescing();
        for (operation, _) in batch.ops.iter() {
            let _ = self.apply_value(operation);
        }
        self.selection = batch.after_selection.clone();
        self.node_selection = batch.after_node_selection.clone();
        self.node_selection_end = batch.after_node_selection_end.clone();
        self.marks = None;
        self.sanitize_selection();
        if self.selection.is_none() && self.node_selection.is_none() {
            self.selection = self.start_point().map(Range::collapsed);
        }
        self.history.undo_stack.push(batch);
        true
    }
}

fn node_kind(node: &Node) -> &'static str {
    if node.is_text() {
        "text"
    } else {
        "element"
    }
}

fn append_document_string(node: &Node, out: &mut String) {
    match node {
        Node::Text(text) => out.push_str(&text.text),
        Node::Element(element) => {
            for (index, child) in element.children.iter().enumerate() {
                if index > 0 && child.is_element() {
                    out.push('\n');
                }
                append_document_string(child, out);
            }
        }
    }
}

fn apply_set_node(node: &mut Node, _properties: &Props, new_properties: &Props) {
    if let Node::Element(element) = node {
        if let Some(Value::String(new_type)) = new_properties.get("type") {
            element.element_type = new_type.clone();
        }
    }

    for (key, value) in new_properties {
        if key == "type" {
            continue;
        }
        if value.is_null() {
            node.properties_mut().remove(key);
        } else {
            node.properties_mut().insert(key.clone(), value.clone());
        }
    }
}

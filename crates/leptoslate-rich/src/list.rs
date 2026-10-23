//! List transforms, schema and keyboard behavior for the rich preset.
//!
//! The core editor knows nothing about lists. This module owns the `ul` / `ol`
//! / `li` schema rules, the indent/outdent/toggle transforms and the structural
//! hooks that keep Enter and Backspace inside list items.

use crate::TableTransforms;
use leptoslate::core::{path, Feature};
use leptoslate::{
    EditablePlugin, Editor, EditorHandle, Element, ElementRule, Node, Operation, Path, Point,
    Range, Schema,
};
use web_sys::KeyboardEvent;

/// The list item, list and position under a point.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListContext {
    /// The nearest `li` ancestor.
    pub item: Path,
    /// The `ul` / `ol` containing the item.
    pub list: Path,
    /// The item's index inside the list.
    pub index: usize,
    /// The list's element type (`ul` or `ol`).
    pub list_type: String,
}

/// List transforms provided by the rich preset.
///
/// Import this trait to call the methods on [`Editor`]:
///
/// ```rust,ignore
/// use leptoslate_rich::ListTransforms;
/// editor.toggle_list("ul");
/// editor.indent_list();
/// ```
pub trait ListTransforms {
    /// Wrap the selected block(s) in a list, or unwrap a list at the cursor.
    fn toggle_list(&mut self, list_type: &str);

    /// Indent the current list item, or every selected item, under its
    /// previous sibling.
    fn indent_list(&mut self) -> bool;

    /// Outdent the current list item, or every selected item, lifting it out of
    /// the list at the top level or into the parent list when nested.
    fn outdent_list(&mut self) -> bool;

    /// The list context under a point, if the point is inside a list item.
    fn list_context(&self, point: &Point) -> Option<ListContext>;

    /// List item paths intersecting the current selection.
    fn list_items_in_selection(&self) -> Vec<Path>;
}

/// List transforms on the reactive editor handle.
pub trait ListHandle {
    /// Wrap the selected block(s) in a list, or unwrap a list at the cursor.
    fn toggle_list(&self, list_type: &str);

    /// Indent the current list item under its previous sibling.
    fn indent_list(&self) -> bool;

    /// Outdent the current list item.
    fn outdent_list(&self) -> bool;
}

impl ListHandle for EditorHandle {
    fn toggle_list(&self, list_type: &str) {
        self.update(|editor| editor.toggle_list(list_type));
    }

    fn indent_list(&self) -> bool {
        self.update(Editor::indent_list)
    }

    fn outdent_list(&self) -> bool {
        self.update(Editor::outdent_list)
    }
}

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

/// List schema rules and structural hooks.
#[derive(Clone, Copy, Debug, Default)]
pub struct RichListFeature;

impl Feature for RichListFeature {
    fn install_schema(&self, schema: &mut Schema) {
        schema.set_rule("ul", ElementRule::block("ul"));
        schema.set_rule("ol", ElementRule::block("ol"));
        schema.set_rule("li", ElementRule::block("li"));
    }

    fn handle_backspace(&self, editor: &mut Editor, point: &Point) -> bool {
        let Some(context) = editor.list_context(point) else {
            return false;
        };
        if !at_start_of_node(editor, point, &context.item) {
            return false;
        }
        if context.index == 0 {
            lift_first_list_item_inner(editor).is_some()
        } else {
            merge_list_item_with_previous_inner(editor, &context.item, &context.list, context.index)
                .is_some()
        }
    }

    fn split_stop_depth(&self, editor: &Editor, point: &Point) -> Option<usize> {
        editor.list_context(point).map(|context| context.item.len())
    }

    fn adjust_insert_block(&self, editor: &Editor, block: &mut Path) {
        while block.len() > 1 {
            let parent = block[..block.len() - 1].to_vec();
            match editor.element(&parent) {
                Some(element) if editor.schema.is_list(&element.element_type) => {
                    *block = parent;
                }
                _ => break,
            }
        }
    }

    fn before_set_block(&self, editor: &mut Editor, element_type: &str) -> bool {
        if editor.schema.is_list(element_type) {
            return false;
        }
        let Some(range) = editor.selection.clone() else {
            return false;
        };
        if let Some(list_type) = list_ancestor_type(editor, range.start()) {
            editor.toggle_list(&list_type);
        }
        false
    }
}

impl ListTransforms for Editor {
    fn toggle_list(&mut self, list_type: &str) {
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

            // Unwrap an existing list under the cursor, or switch its type.
            if let Some(Node::Element(list)) = editor.children.get(top_index) {
                if editor.schema.is_list(&list.element_type) {
                    if list.element_type != list_type {
                        let mut properties = leptoslate::Props::new();
                        properties.insert("type".into(), serde_json::json!(list.element_type));
                        let mut new_properties = leptoslate::Props::new();
                        new_properties.insert("type".into(), serde_json::json!(list_type));
                        let _ = editor.apply_operation(Operation::SetNode {
                            path: vec![top_index],
                            properties,
                            new_properties,
                        });
                        return;
                    }

                    let list_children = list.children.clone();
                    let list_path = vec![top_index];
                    let mut paragraphs = Vec::new();
                    for child in &list_children {
                        let children = child.children().to_vec();
                        paragraphs.push(Node::Element(Element::new("p", children)));
                    }

                    let _ = editor.apply_operation(Operation::RemoveNode { path: list_path });
                    for (offset, paragraph) in paragraphs.into_iter().enumerate() {
                        let _ = editor.apply_operation(Operation::InsertNode {
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
            let _ = editor.apply_operation(Operation::InsertNode {
                path: vec![first],
                node: list,
            });
            for _ in first..=last {
                let _ = editor.apply_operation(Operation::RemoveNode {
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

    fn indent_list(&mut self) -> bool {
        self.transaction(|editor| {
            let items = editor.list_items_in_selection();
            if items.len() <= 1 {
                return indent_list_inner(editor);
            }
            let mut changed = false;
            for li in items.into_iter().rev() {
                if let Some(point) = editor.first_text_point_in(&li) {
                    editor.selection = Some(Range::collapsed(point));
                    if indent_list_inner(editor).is_some() {
                        changed = true;
                    }
                }
            }
            changed.then_some(())
        })
        .is_some()
    }

    fn outdent_list(&mut self) -> bool {
        self.transaction(|editor| {
            let items = editor.list_items_in_selection();
            if items.len() <= 1 {
                return outdent_list_inner(editor);
            }
            let mut changed = false;
            for li in items.into_iter().rev() {
                if let Some(point) = editor.first_text_point_in(&li) {
                    editor.selection = Some(Range::collapsed(point));
                    if outdent_list_inner(editor).is_some() {
                        changed = true;
                    }
                }
            }
            changed.then_some(())
        })
        .is_some()
    }

    fn list_context(&self, point: &Point) -> Option<ListContext> {
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
            return Some(ListContext {
                item: candidate.to_vec(),
                list,
                index,
                list_type: list_element.element_type.clone(),
            });
        }
        None
    }

    fn list_items_in_selection(&self) -> Vec<Path> {
        let Some(range) = self.selection.as_ref() else {
            return Vec::new();
        };
        let mut points = Vec::new();
        if range.is_collapsed() {
            points.push(range.start().path.clone());
        } else {
            points.extend(self.text_paths_in_range(range));
            points.push(range.start().path.clone());
            points.push(range.end().path.clone());
        }
        let mut items: Vec<Path> = points
            .into_iter()
            .filter_map(|path| {
                self.list_context(&Point::new(path, 0))
                    .map(|context| context.item)
            })
            .collect();
        items.sort_by(|a, b| path::compare(a, b));
        items.dedup();
        items
    }
}

fn list_ancestor_type(editor: &Editor, point: &Point) -> Option<String> {
    for depth in (1..point.path.len()).rev() {
        let path = &point.path[..depth];
        if let Some(element) = editor.element(path) {
            if editor.schema.is_list(&element.element_type) {
                return Some(element.element_type.clone());
            }
        }
    }
    None
}

fn indent_list_inner(editor: &mut Editor) -> Option<()> {
    let range = editor.selection.clone()?;
    let point = editor.text_point(range.start())?;
    let context = editor.list_context(&point)?;
    if context.index == 0 {
        return None;
    }

    let li_node = editor.node(&context.item)?.clone();
    let previous = child_path(&context.list, context.index - 1);
    editor
        .apply_operation(Operation::RemoveNode {
            path: context.item.clone(),
        })
        .ok()?;

    let previous_children = editor.element(&previous)?.children.clone();
    let nested = previous_children
        .last()
        .filter(|node| node.element_type() == Some(context.list_type.as_str()))
        .map(|_| child_path(&previous, previous_children.len() - 1));

    let new_li_path = if let Some(nested) = nested {
        let nested_len = editor.element(&nested)?.children.len();
        let at = child_path(&nested, nested_len);
        editor
            .apply_operation(Operation::InsertNode {
                path: at.clone(),
                node: li_node,
            })
            .ok()?;
        at
    } else {
        let new_list = Node::Element(Element::new(context.list_type, vec![li_node]));
        let at = child_path(&previous, previous_children.len());
        editor
            .apply_operation(Operation::InsertNode {
                path: at.clone(),
                node: new_list,
            })
            .ok()?;
        child_path(&at, 0)
    };

    if let Some(point) = editor.first_text_point_in(&new_li_path) {
        editor.selection = Some(Range::collapsed(point));
    }
    Some(())
}

fn outdent_list_inner(editor: &mut Editor) -> Option<()> {
    let range = editor.selection.clone()?;
    let point = editor.text_point(range.start())?;
    let context = editor.list_context(&point)?;
    let li_node = editor.node(&context.item)?.clone();
    let list_len = editor.element(&context.list)?.children.len();
    let list_top = *context.list.first()?;

    if context.list.len() == 1 {
        let children = li_node.children().to_vec();
        editor
            .apply_operation(Operation::RemoveNode { path: context.item })
            .ok()?;
        let block = Node::Element(Element::new(editor.schema.default_block.clone(), children));
        let insert_at = if list_len == 1 {
            editor
                .apply_operation(Operation::RemoveNode { path: context.list })
                .ok()?;
            vec![list_top]
        } else {
            vec![list_top + 1]
        };
        editor
            .apply_operation(Operation::InsertNode {
                path: insert_at.clone(),
                node: block,
            })
            .ok()?;
        if let Some(point) = editor.first_text_point_in(&insert_at) {
            editor.selection = Some(Range::collapsed(point));
        }
        return Some(());
    }

    let outer_li = path::parent(&context.list)?;
    let outer_list = path::parent(&outer_li)?;
    let outer_index = *outer_li.last()?;

    editor
        .apply_operation(Operation::RemoveNode { path: context.item })
        .ok()?;
    if list_len == 1 {
        editor
            .apply_operation(Operation::RemoveNode { path: context.list })
            .ok()?;
    }
    let insert_at = child_path(&outer_list, outer_index + 1);
    editor
        .apply_operation(Operation::InsertNode {
            path: insert_at.clone(),
            node: li_node,
        })
        .ok()?;
    if let Some(point) = editor.first_text_point_in(&insert_at) {
        editor.selection = Some(Range::collapsed(point));
    }
    Some(())
}

fn lift_first_list_item_inner(editor: &mut Editor) -> Option<()> {
    let range = editor.selection.clone()?;
    let point = editor.text_point(range.start())?;
    let context = editor.list_context(&point)?;
    if context.index != 0 {
        return None;
    }
    if context.list.len() > 1 {
        return outdent_list_inner(editor);
    }

    let li_node = editor.node(&context.item)?.clone();
    let list_top = *context.list.first()?;
    let children = li_node.children().to_vec();

    editor
        .apply_operation(Operation::RemoveNode { path: context.item })
        .ok()?;
    if editor.element(&context.list)?.children.is_empty() {
        editor
            .apply_operation(Operation::RemoveNode { path: context.list })
            .ok()?;
    }

    let block = Node::Element(Element::new(editor.schema.default_block.clone(), children));
    let insert_at = vec![list_top];
    editor
        .apply_operation(Operation::InsertNode {
            path: insert_at.clone(),
            node: block,
        })
        .ok()?;
    if let Some(point) = editor.first_text_point_in(&insert_at) {
        editor.selection = Some(Range::collapsed(point));
    }
    Some(())
}

fn merge_list_item_with_previous_inner(
    editor: &mut Editor,
    item: &[usize],
    list: &[usize],
    index: usize,
) -> Option<()> {
    if index == 0 {
        return None;
    }
    let previous = child_path(list, index - 1);
    let junction = editor.last_text_point_in(&previous)?;
    let children = editor.element(item)?.children.clone();

    editor
        .apply_operation(Operation::RemoveNode {
            path: item.to_vec(),
        })
        .ok()?;
    let previous_len = editor.element(&previous)?.children.len();
    for (offset, child) in children.into_iter().enumerate() {
        editor
            .apply_operation(Operation::InsertNode {
                path: child_path(&previous, previous_len + offset),
                node: child,
            })
            .ok()?;
    }

    editor.selection = Some(Range::collapsed(junction));
    Some(())
}

fn at_start_of_node(editor: &Editor, point: &Point, node: &[usize]) -> bool {
    editor
        .first_text_point_in(node)
        .map(|first| first.path == point.path && first.offset == 0)
        .unwrap_or(false)
}

fn child_path(parent: &[usize], index: usize) -> Path {
    let mut path = parent.to_vec();
    path.push(index);
    path
}

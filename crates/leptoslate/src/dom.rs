//! Mapping between the document selection model and the browser's DOM
//! selection.
//!
//! Text nodes are rendered as `<span data-text-path="0.1">…</span>`. Offsets in
//! the model are character offsets; the DOM uses UTF-16 code units, so all
//! conversions happen here.

use leptoslate_core::{
    text::{char_len, char_to_utf16_index, utf16_to_char_index},
    Path, Point, Range,
};
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{Document, Element, Node, Selection};

pub fn path_string(path: &[usize]) -> String {
    path.iter()
        .map(usize::to_string)
        .collect::<Vec<_>>()
        .join(".")
}

pub fn parse_path(value: &str) -> Option<Path> {
    let path = value
        .split('.')
        .map(|part| part.parse::<usize>().ok())
        .collect::<Option<Vec<_>>>()?;
    if path.is_empty() {
        None
    } else {
        Some(path)
    }
}

fn document() -> Option<Document> {
    web_sys::window()?.document()
}

fn selection() -> Option<Selection> {
    web_sys::window()?.get_selection().ok().flatten()
}

pub fn text_span(root: &Element, path: &[usize]) -> Option<Element> {
    let selector = format!("[data-text-path=\"{}\"]", path_string(path));
    root.query_selector(&selector).ok().flatten()
}

fn closest_text_span(node: &Node) -> Option<(Element, Path)> {
    let element = if node.node_type() == Node::ELEMENT_NODE {
        node.dyn_ref::<Element>()?.clone()
    } else {
        node.parent_element()?
    };
    let span = element.closest("[data-text-path]").ok().flatten()?;
    let path = parse_path(&span.get_attribute("data-text-path")?)?;
    Some((span, path))
}

fn first_text_span_in(element: &Element) -> Option<Element> {
    if element.get_attribute("data-text-path").is_some() {
        return Some(element.clone());
    }
    element.query_selector("[data-text-path]").ok().flatten()
}

fn last_text_span_in(element: &Element) -> Option<Element> {
    if element.get_attribute("data-text-path").is_some() {
        return Some(element.clone());
    }
    let spans = element.query_selector_all("[data-text-path]").ok()?;
    let length = spans.length();
    if length == 0 {
        return None;
    }
    spans
        .item(length - 1)
        .and_then(|node| node.dyn_into::<Element>().ok())
}

fn text_span_for_node(node: &Node) -> Option<Element> {
    if node.node_type() == Node::TEXT_NODE {
        closest_text_span(node).map(|(span, _)| span)
    } else if let Some(element) = node.dyn_ref::<Element>() {
        first_text_span_in(element).or_else(|| last_text_span_in(element))
    } else {
        None
    }
}

fn point_from_dom_node(root: &Element, node: &Node, offset: u32) -> Option<Point> {
    if node.node_type() == Node::TEXT_NODE {
        let (span, path) = closest_text_span(node)?;
        if !root.contains(Some(&span)) {
            return None;
        }
        let text = span.text_content().unwrap_or_default();
        return Some(Point::new(
            path,
            utf16_to_char_index(&text, offset as usize),
        ));
    }

    let element = node.dyn_ref::<Element>()?;
    if !root.contains(Some(element)) && !root.is_same_node(Some(element)) {
        return None;
    }

    if let Some(path) = element
        .get_attribute("data-text-path")
        .and_then(|value| parse_path(&value))
    {
        return Some(Point::new(path, 0));
    }

    let children = element.child_nodes();
    let length = children.length();
    if length == 0 {
        // An empty container: find the nearest text span inside the root when
        // possible, otherwise this selection is outside the document.
        let span = first_text_span_in(root)?;
        let path = parse_path(&span.get_attribute("data-text-path")?)?;
        return Some(Point::new(path, 0));
    }

    let at_end = offset >= length;
    let child = if at_end {
        children.item(length - 1)?
    } else {
        children.item(offset)?
    };

    let span = text_span_for_node(&child);
    let Some(span) = span else {
        // The child is a void node or a block; use the nearest text.
        let span = if at_end {
            last_text_span_in(root)?
        } else {
            first_text_span_in(root)?
        };
        let path = parse_path(&span.get_attribute("data-text-path")?)?;
        let text = span.text_content().unwrap_or_default();
        let offset = if at_end { char_len(&text) } else { 0 };
        return Some(Point::new(path, offset));
    };

    let path = parse_path(&span.get_attribute("data-text-path")?)?;
    let text = span.text_content().unwrap_or_default();
    let offset = if at_end {
        char_len(&text)
    } else if offset > 0 && child.node_type() != Node::TEXT_NODE {
        // Element-level offsets point between children, so position the caret
        // at the end of the previous child rather than the start of the next.
        let previous = children.item(offset.saturating_sub(1));
        match previous.and_then(|previous| text_span_for_node(&previous)) {
            Some(previous_span) => {
                let previous_path = parse_path(&previous_span.get_attribute("data-text-path")?)?;
                let previous_text = previous_span.text_content().unwrap_or_default();
                return Some(Point::new(previous_path, char_len(&previous_text)));
            }
            None => 0,
        }
    } else {
        0
    };

    Some(Point::new(path, offset))
}

/// Read the browser selection as a document range, clipped to `root`.
pub fn selection_from_dom(root: &Element) -> Option<Range> {
    let selection = selection()?;
    if selection.range_count() == 0 {
        return None;
    }
    let anchor_node = selection.anchor_node()?;
    let focus_node = selection.focus_node()?;
    let anchor = point_from_dom_node(root, &anchor_node, selection.anchor_offset())?;
    let focus = point_from_dom_node(root, &focus_node, selection.focus_offset())?;
    Some(Range::new(anchor, focus))
}

/// Compare a model range with the current DOM selection.
pub fn selection_matches_dom(root: &Element, range: &Range) -> bool {
    let Some(current) = selection_from_dom(root) else {
        return false;
    };
    (current.anchor == range.anchor && current.focus == range.focus)
        || (current.anchor == range.focus && current.focus == range.anchor)
}

fn point_to_dom(root: &Element, point: &Point) -> Option<(Node, u32)> {
    let span = text_span(root, &point.path)?;
    let text = span.text_content().unwrap_or_default();
    let char_offset = point.offset.min(char_len(&text));
    let utf16_offset = char_to_utf16_index(&text, char_offset) as u32;

    if let Some(child) = span.first_child() {
        if child.node_type() == Node::TEXT_NODE {
            return Some((child, utf16_offset));
        }
    }

    let node: Node = span.unchecked_into();
    Some((node, 0))
}

/// Move the browser selection to match the model range.
pub fn apply_selection_to_dom(root: &Element, range: &Range) -> bool {
    let Some(selection) = selection() else {
        return false;
    };
    let Some((anchor_node, anchor_offset)) = point_to_dom(root, &range.anchor) else {
        return false;
    };
    let Some((focus_node, focus_offset)) = point_to_dom(root, &range.focus) else {
        return false;
    };

    selection
        .set_base_and_extent(&anchor_node, anchor_offset, &focus_node, focus_offset)
        .is_ok()
}

/// Whether focus is inside the given editor root.
pub fn has_focus(root: &Element) -> bool {
    let Some(document) = document() else {
        return false;
    };
    let Some(active) = document.active_element() else {
        return false;
    };
    let active_node: &Node = active.unchecked_ref();
    root.contains(Some(active_node)) || root.is_same_node(Some(active_node))
}

/// The text span and text under the current DOM caret, used for IME
/// reconciliation.
pub fn focused_text_span(root: &Element) -> Option<(Path, String)> {
    let selection = selection()?;
    let node = selection.anchor_node()?;
    let (span, path) = closest_text_span(&node)?;
    if !root.contains(Some(&span)) {
        return None;
    }
    Some((path, span.text_content().unwrap_or_default()))
}

/// A small helper for tests/debugging: select everything in the DOM root.
pub fn select_all_dom(root: &Element) -> Result<(), JsValue> {
    let document = document().ok_or_else(|| JsValue::from_str("no document"))?;
    let range = document.create_range()?;
    range.select_node_contents(root)?;
    let selection = selection().ok_or_else(|| JsValue::from_str("no selection"))?;
    selection.remove_all_ranges()?;
    selection.add_range(&range)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{parse_path, path_string};

    #[test]
    fn paths_round_trip_through_data_attributes() {
        let path = vec![0, 12, 3];
        let encoded = path_string(&path);
        assert_eq!(encoded, "0.12.3");
        assert_eq!(parse_path(&encoded), Some(path));
    }

    #[test]
    fn malformed_paths_are_rejected() {
        assert_eq!(parse_path(""), None);
        assert_eq!(parse_path("0..1"), None);
        assert_eq!(parse_path("0.x"), None);
    }
}

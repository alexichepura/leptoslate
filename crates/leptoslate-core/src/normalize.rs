//! Structural normalization, mirroring Slate's core constraints.
//!
//! The normalizer applies real operations (so every fix is undoable) and fixes
//! one issue per pass, restarting from the root. The constraints are:
//!
//! 1. Elements always have at least one child.
//! 2. Adjacent text nodes either have different properties or are merged.
//! 3. Empty text nodes are removed when they have siblings.
//! 4. Containers hold either only inline content (text/inline elements) or
//!    only block content; stray inline content is wrapped in the default block.
//! 5. Inline nodes are surrounded by text.

use crate::{
    node::{Element, Node},
    operation::Operation,
    path, SlateError,
};

use crate::Editor;

#[derive(Debug)]
enum Fix {
    InsertNode {
        path: path::Path,
        node: Node,
    },
    RemoveNode {
        path: path::Path,
    },
    MergeNode {
        path: path::Path,
    },
    WrapRun {
        parent: path::Path,
        start: usize,
        len: usize,
    },
    Unwrap {
        path: path::Path,
    },
}

impl Editor {
    /// Normalize the document until no core constraint is violated.
    pub fn normalize(&mut self) {
        if self.is_normalizing {
            return;
        }
        self.is_normalizing = true;

        let mut iterations = 0;
        while iterations < 2_000 {
            let Some(fix) = self.find_fix() else {
                break;
            };
            iterations += 1;
            if self.apply_fix(fix).is_err() {
                break;
            }
        }

        self.is_normalizing = false;
        self.sanitize_selection();
    }

    fn find_fix(&self) -> Option<Fix> {
        self.scan_children(&self.children, &[], true)
    }

    fn scan_children(&self, children: &[Node], at: &[usize], is_root: bool) -> Option<Fix> {
        if !is_root {
            if let Some(element) = self.element(at) {
                if self.schema.is_inline(&element.element_type)
                    && !self.schema.is_void(&element.element_type)
                    && element.text_len() == 0
                {
                    return Some(Fix::RemoveNode { path: at.to_vec() });
                }
            }
        }

        if children.is_empty() {
            if is_root {
                return Some(Fix::InsertNode {
                    path: vec![0],
                    node: Node::Element(Element::empty(&self.schema.default_block)),
                });
            }
            let mut insert_at = at.to_vec();
            insert_at.push(0);
            return Some(Fix::InsertNode {
                path: insert_at,
                node: Node::empty_text(),
            });
        }

        let should_have_inlines = if is_root {
            false
        } else {
            let element_type = self
                .node(at)
                .and_then(Node::element_type)
                .unwrap_or(self.schema.default_block.as_str());
            self.schema.is_inline(element_type)
                || children.first().map(Node::is_text).unwrap_or(false)
                || children
                    .first()
                    .and_then(Node::element_type)
                    .map(|kind| self.schema.is_inline(kind))
                    .unwrap_or(false)
        };

        if should_have_inlines {
            for (index, child) in children.iter().enumerate() {
                if child.is_text() {
                    if index > 0 {
                        if let Some(previous) = children.get(index - 1) {
                            if let Some(previous_text) = previous.as_text() {
                                let text = child.as_text().unwrap();
                                if text.is_empty() {
                                    return Some(Fix::RemoveNode {
                                        path: child_path(at, index),
                                    });
                                }
                                if previous_text.is_empty() {
                                    return Some(Fix::RemoveNode {
                                        path: child_path(at, index - 1),
                                    });
                                }
                                if text.properties == previous_text.properties {
                                    return Some(Fix::MergeNode {
                                        path: child_path(at, index),
                                    });
                                }
                            }
                        }
                    }
                } else if let Some(element) = child.as_element() {
                    if self.schema.is_void(&element.element_type) {
                        if self.schema.is_inline(&element.element_type) {
                            if index == 0 || !children[index - 1].is_text() {
                                return Some(Fix::InsertNode {
                                    path: child_path(at, index),
                                    node: Node::empty_text(),
                                });
                            }
                            if index == children.len() - 1 {
                                return Some(Fix::InsertNode {
                                    path: child_path(at, index + 1),
                                    node: Node::empty_text(),
                                });
                            }
                        }
                    } else if self.schema.is_inline(&element.element_type) {
                        if index == 0 || !children[index - 1].is_text() {
                            return Some(Fix::InsertNode {
                                path: child_path(at, index),
                                node: Node::empty_text(),
                            });
                        }
                        if index == children.len() - 1 {
                            return Some(Fix::InsertNode {
                                path: child_path(at, index + 1),
                                node: Node::empty_text(),
                            });
                        }
                    } else {
                        // A block node inside an inline container: lift it.
                        return Some(Fix::Unwrap {
                            path: child_path(at, index),
                        });
                    }
                }
            }
        } else {
            // Block container: wrap runs of inline content.
            let mut run_start: Option<usize> = None;
            for (index, child) in children.iter().enumerate() {
                let inline = child.is_text()
                    || child
                        .element_type()
                        .map(|kind| self.schema.is_inline(kind))
                        .unwrap_or(false);
                if inline {
                    if run_start.is_none() {
                        run_start = Some(index);
                    }
                } else if let Some(start) = run_start.take() {
                    return Some(Fix::WrapRun {
                        parent: at.to_vec(),
                        start,
                        len: index - start,
                    });
                }
            }
            if let Some(start) = run_start {
                return Some(Fix::WrapRun {
                    parent: at.to_vec(),
                    start,
                    len: children.len() - start,
                });
            }
        }

        // Recurse into element children.
        for (index, child) in children.iter().enumerate() {
            if let Node::Element(element) = child {
                let mut child_at = at.to_vec();
                child_at.push(index);
                if let Some(fix) = self.scan_children(&element.children, &child_at, false) {
                    return Some(fix);
                }
            }
        }

        None
    }

    fn apply_fix(&mut self, fix: Fix) -> Result<(), SlateError> {
        match fix {
            Fix::InsertNode { path, node } => self.apply(Operation::InsertNode { path, node }),
            Fix::RemoveNode { path } => self.apply(Operation::RemoveNode { path }),
            Fix::MergeNode { path } => self.apply_merge(&path),
            Fix::WrapRun { parent, start, len } => {
                let children = self
                    .children_at(&parent)
                    .map(<[Node]>::to_vec)
                    .unwrap_or_default();
                if start + len > children.len() {
                    return Err(SlateError::InvalidOperation(
                        "wrap run outside children".into(),
                    ));
                }
                let run = children[start..start + len].to_vec();
                let wrapper = Node::Element(Element::new(self.schema.default_block.clone(), run));
                let mut insert_at = parent.clone();
                insert_at.push(start);
                self.apply(Operation::InsertNode {
                    path: insert_at,
                    node: wrapper,
                })?;

                for _ in 0..len {
                    let mut remove_at = parent.clone();
                    remove_at.push(start + 1);
                    self.apply(Operation::RemoveNode { path: remove_at })?;
                }
                Ok(())
            }
            Fix::Unwrap { path } => {
                let children = self
                    .element(&path)
                    .map(|element| element.children.clone())
                    .unwrap_or_default();
                let index = *path
                    .last()
                    .ok_or_else(|| SlateError::InvalidOperation("cannot unwrap root".into()))?;
                let parent = path[..path.len() - 1].to_vec();
                self.apply(Operation::RemoveNode { path: path.clone() })?;
                for (offset, child) in children.into_iter().enumerate() {
                    let mut insert_at = parent.clone();
                    insert_at.push(index + offset);
                    self.apply(Operation::InsertNode {
                        path: insert_at,
                        node: child,
                    })?;
                }
                Ok(())
            }
        }
    }
}

fn child_path(parent: &[usize], index: usize) -> path::Path {
    let mut path = parent.to_vec();
    path.push(index);
    path
}

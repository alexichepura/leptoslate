//! Paths address nodes in the document tree, exactly like Slate paths.

use std::cmp::Ordering;

use crate::Operation;

pub use crate::Path;

/// Affinity decides where a point goes when it sits exactly on a split.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Affinity {
    #[default]
    Forward,
    Backward,
}

pub fn next(path: &[usize]) -> Path {
    assert!(!path.is_empty(), "the root path has no next sibling");
    let mut next = path.to_vec();
    let last = next.len() - 1;
    next[last] += 1;
    next
}

pub fn previous(path: &[usize]) -> Option<Path> {
    let last = *path.last()?;
    if last == 0 {
        return None;
    }
    let mut previous = path.to_vec();
    let last = previous.len() - 1;
    previous[last] -= 1;
    Some(previous)
}

pub fn parent(path: &[usize]) -> Option<Path> {
    if path.is_empty() {
        None
    } else {
        Some(path[..path.len() - 1].to_vec())
    }
}

pub fn levels(path: &[usize]) -> Vec<Path> {
    (0..=path.len()).map(|i| path[..i].to_vec()).collect()
}

pub fn ancestors(path: &[usize]) -> Vec<Path> {
    let mut all = levels(path);
    all.pop();
    all
}

pub fn relative(path: &[usize], ancestor: &[usize]) -> Option<Path> {
    if is_common(ancestor, path) {
        Some(path[ancestor.len()..].to_vec())
    } else {
        None
    }
}

/// Slate's path comparison: compares up to the shortest length, so an ancestor
/// and its descendant compare equal.
pub fn compare(path: &[usize], another: &[usize]) -> Ordering {
    for (a, b) in path.iter().zip(another.iter()) {
        if a != b {
            return a.cmp(b);
        }
    }
    Ordering::Equal
}

pub fn equals(path: &[usize], another: &[usize]) -> bool {
    path == another
}

pub fn is_before(path: &[usize], another: &[usize]) -> bool {
    compare(path, another) == Ordering::Less
}

pub fn is_after(path: &[usize], another: &[usize]) -> bool {
    compare(path, another) == Ordering::Greater
}

pub fn is_ancestor(path: &[usize], another: &[usize]) -> bool {
    path.len() < another.len() && compare(path, another) == Ordering::Equal
}

pub fn is_descendant(path: &[usize], another: &[usize]) -> bool {
    is_ancestor(another, path)
}

/// `path` is equal to or an ancestor of `another`.
pub fn is_common(path: &[usize], another: &[usize]) -> bool {
    path.len() <= another.len() && compare(path, another) == Ordering::Equal
}

pub fn is_sibling(path: &[usize], another: &[usize]) -> bool {
    path.len() == another.len()
        && !path.is_empty()
        && path[..path.len() - 1] == another[..another.len() - 1]
        && path.last() != another.last()
}

/// True if `path` ends before `another` inside a shared parent.
pub fn ends_before(path: &[usize], another: &[usize]) -> bool {
    let Some(i) = path.len().checked_sub(1) else {
        return false;
    };
    if another.len() <= i {
        return false;
    }
    path[..i] == another[..i] && path[i] < another[i]
}

pub fn ends_after(path: &[usize], another: &[usize]) -> bool {
    ends_before(another, path)
}

/// The longest common prefix of two paths.
pub fn common(path: &[usize], another: &[usize]) -> Path {
    let mut common = Vec::new();
    for (a, b) in path.iter().zip(another.iter()) {
        if a != b {
            break;
        }
        common.push(*a);
    }
    common
}

/// Transform a path by an operation. Returns `None` if the path no longer
/// exists (for example it was inside a removed node).
pub fn transform(path: &[usize], op: &Operation) -> Option<Path> {
    transform_with_affinity(path, op, Affinity::Forward)
}

pub fn transform_with_affinity(path: &[usize], op: &Operation, affinity: Affinity) -> Option<Path> {
    if path.is_empty() {
        return Some(Vec::new());
    }

    let mut p = path.to_vec();

    match op {
        Operation::InsertNode { path: op, .. } => {
            if equals(op, &p) || ends_before(op, &p) || is_ancestor(op, &p) {
                let last = op.len() - 1;
                p[last] += 1;
            }
        }
        Operation::RemoveNode { path: op } => {
            if equals(op, &p) || is_ancestor(op, &p) {
                return None;
            } else if ends_before(op, &p) {
                let last = op.len() - 1;
                p[last] = p[last].saturating_sub(1);
            }
        }
        Operation::MergeNode { path: op, position } => {
            if equals(op, &p) || ends_before(op, &p) {
                let last = op.len() - 1;
                p[last] = p[last].saturating_sub(1);
            } else if is_ancestor(op, &p) {
                let last = op.len() - 1;
                p[last] = p[last].saturating_sub(1);
                p[op.len()] += position;
            }
        }
        Operation::SplitNode {
            path: op, position, ..
        } => {
            if equals(op, &p) {
                match affinity {
                    Affinity::Forward => {
                        let last = p.len() - 1;
                        p[last] += 1;
                    }
                    Affinity::Backward => {}
                }
            } else if ends_before(op, &p) {
                let last = op.len() - 1;
                p[last] += 1;
            } else if is_ancestor(op, &p) && p[op.len()] >= *position {
                let last = op.len() - 1;
                p[last] += 1;
                p[op.len()] -= position;
            }
        }
        Operation::MoveNode {
            path: op,
            new_path: onp,
        } => {
            if equals(op, onp) {
                return Some(p);
            }

            if is_ancestor(op, &p) || equals(op, &p) {
                let mut copy = onp.clone();
                if ends_before(op, onp) && op.len() < onp.len() {
                    copy[op.len() - 1] = copy[op.len() - 1].saturating_sub(1);
                }
                copy.extend_from_slice(&p[op.len()..]);
                return Some(copy);
            } else if is_sibling(op, onp) && (is_ancestor(onp, &p) || equals(onp, &p)) {
                let last = op.len() - 1;
                if ends_before(op, &p) {
                    p[last] = p[last].saturating_sub(1);
                } else {
                    p[last] += 1;
                }
            } else if ends_before(onp, &p) || equals(onp, &p) || is_ancestor(onp, &p) {
                if ends_before(op, &p) {
                    let last = op.len() - 1;
                    p[last] = p[last].saturating_sub(1);
                }
                let last = onp.len() - 1;
                p[last] += 1;
            } else if ends_before(op, &p) {
                if equals(onp, &p) {
                    let last = onp.len() - 1;
                    p[last] += 1;
                }
                let last = op.len() - 1;
                p[last] = p[last].saturating_sub(1);
            }
        }
        Operation::InsertText { .. }
        | Operation::RemoveText { .. }
        | Operation::SetNode { .. }
        | Operation::SetSelection { .. } => {}
    }

    Some(p)
}

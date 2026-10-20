//! Points are positions inside a text node: a path plus a character offset.

use std::cmp::Ordering;

use serde::{Deserialize, Serialize};

use crate::{
    path::{self, Affinity},
    Operation, Path,
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Point {
    pub path: Path,
    pub offset: usize,
}

impl Point {
    pub fn new(path: Path, offset: usize) -> Self {
        Self { path, offset }
    }

    pub fn start() -> Self {
        Self::new(vec![0, 0], 0)
    }

    pub fn compare(&self, other: &Point) -> Ordering {
        match path::compare(&self.path, &other.path) {
            Ordering::Equal => self.offset.cmp(&other.offset),
            other => other,
        }
    }

    pub fn is_before(&self, other: &Point) -> bool {
        self.compare(other) == Ordering::Less
    }

    pub fn is_after(&self, other: &Point) -> bool {
        self.compare(other) == Ordering::Greater
    }

    pub fn equals(&self, other: &Point) -> bool {
        self.path == other.path && self.offset == other.offset
    }

    /// Return this point moved `delta` characters within its text node.
    pub fn shift(&self, delta: isize) -> Self {
        Self {
            path: self.path.clone(),
            offset: (self.offset as isize + delta).max(0) as usize,
        }
    }

    /// Transform this point by an operation, mirroring Slate's `Point.transform`.
    pub fn transform(&self, op: &Operation, affinity: Affinity) -> Option<Self> {
        let mut path = self.path.clone();
        let mut offset = self.offset;

        match op {
            Operation::InsertNode { .. } | Operation::MoveNode { .. } => {
                path = path::transform_with_affinity(&path, op, affinity)?;
            }
            Operation::InsertText {
                path: op_path,
                offset: op_offset,
                text,
            } => {
                if path::equals(op_path, &path)
                    && (*op_offset < offset
                        || (*op_offset == offset && affinity == Affinity::Forward))
                {
                    offset += text.chars().count();
                }
            }
            Operation::MergeNode {
                path: op_path,
                position,
            } => {
                if path::equals(op_path, &path) {
                    offset += position;
                }
                path = path::transform_with_affinity(&path, op, affinity)?;
            }
            Operation::RemoveText {
                path: op_path,
                offset: op_offset,
                text,
            } => {
                if path::equals(op_path, &path) && *op_offset <= offset {
                    offset -= (offset - op_offset).min(text.chars().count());
                }
            }
            Operation::RemoveNode { path: op_path } => {
                if path::equals(op_path, &path) || path::is_ancestor(op_path, &path) {
                    return None;
                }
                path = path::transform_with_affinity(&path, op, affinity)?;
            }
            Operation::SplitNode {
                path: op_path,
                position,
                ..
            } => {
                if path::equals(op_path, &path) {
                    if *position < offset || (*position == offset && affinity == Affinity::Forward)
                    {
                        offset -= position;
                        path = path::transform_with_affinity(&path, op, Affinity::Forward)?;
                    }
                } else {
                    path = path::transform_with_affinity(&path, op, affinity)?;
                }
            }
            Operation::SetNode { .. } | Operation::SetSelection { .. } => {}
        }

        Some(Self { path, offset })
    }
}

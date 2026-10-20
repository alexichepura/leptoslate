//! Operations are the atomic changes applied to an editor value.
//!
//! They are intentionally compatible with Slate's operation format.

use serde::{Deserialize, Serialize};

use crate::{Node, Path, Props, Range};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Operation {
    InsertNode {
        path: Path,
        node: Node,
    },
    InsertText {
        path: Path,
        offset: usize,
        text: String,
    },
    RemoveNode {
        path: Path,
    },
    RemoveText {
        path: Path,
        offset: usize,
        text: String,
    },
    MergeNode {
        path: Path,
        position: usize,
    },
    MoveNode {
        path: Path,
        new_path: Path,
    },
    SetNode {
        path: Path,
        properties: Props,
        new_properties: Props,
    },
    SplitNode {
        path: Path,
        position: usize,
        properties: Props,
    },
    SetSelection {
        properties: Option<Range>,
        new_properties: Option<Range>,
    },
}

impl Operation {
    pub fn is_selection_only(&self) -> bool {
        matches!(self, Operation::SetSelection { .. })
    }
}

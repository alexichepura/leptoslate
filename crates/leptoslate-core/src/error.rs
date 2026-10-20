//! Errors produced by invalid editor operations.

use std::fmt;

use crate::Path;

#[derive(Clone, Debug, PartialEq)]
pub enum SlateError {
    /// The path does not point at a node.
    InvalidPath(Path),
    /// The path does not point at a text node.
    ExpectedText(Path),
    /// The path does not point at an element.
    ExpectedElement(Path),
    /// A text offset was outside the node.
    InvalidOffset {
        path: Path,
        offset: usize,
        len: usize,
    },
    /// An operation cannot be applied to the current document.
    InvalidOperation(String),
}

impl fmt::Display for SlateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SlateError::InvalidPath(path) => write!(f, "invalid path {path:?}"),
            SlateError::ExpectedText(path) => write!(f, "expected a text node at {path:?}"),
            SlateError::ExpectedElement(path) => {
                write!(f, "expected an element node at {path:?}")
            }
            SlateError::InvalidOffset { path, offset, len } => {
                write!(
                    f,
                    "offset {offset} is outside the text at {path:?} (len {len})"
                )
            }
            SlateError::InvalidOperation(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for SlateError {}

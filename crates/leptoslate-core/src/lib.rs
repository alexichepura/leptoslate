//! # leptoslate-core
//!
//! A Rust port of the data model and transformation engine behind
//! [Slate](https://docs.slatejs.org/), with no DOM or framework dependencies.
//!
//! The core is intentionally shaped like Slate:
//!
//! - [`Node`] is either an [`Element`] or a [`Text`].
//! - [`Path`] addresses a node in the tree.
//! - [`Point`] / [`Range`] describe selections.
//! - [`Operation`]s are atomic, serializable edits.
//! - [`Editor`] applies operations, normalizes the tree, records history, and
//!   exposes a `Transforms`-style API.
//!
//! ```rust
//! use leptoslate_core::{Editor, Element, Node, Point, Text};
//!
//! let mut editor = Editor::new(vec![Node::Element(Element::paragraph(vec![
//!     Node::Text(Text::new("hello")),
//! ]))]);
//! editor.select_point(Point::new(vec![0, 0], 5));
//! editor.insert_text(" world");
//! assert_eq!(editor.document_string(), "hello world\n");
//! ```

pub mod error;
pub mod node;
pub mod operation;
pub mod path;
pub mod point;
pub mod props;
pub mod range;
pub mod schema;
pub mod text;

mod editor;
mod history;
mod normalize;
mod transforms;

pub use editor::Editor;
pub use error::SlateError;
pub use history::{History, HistoryBatch};
pub use node::{Element, Node, Text};
pub use operation::Operation;
pub use path::Affinity;
pub use point::Point;
pub use props::Props;
pub use range::Range;
pub use schema::{ElementRule, Schema};

/// A path is a list of child indexes, exactly like Slate's `Path`.
pub type Path = Vec<usize>;

pub mod prelude {
    pub use crate::{
        Affinity, Editor, Element, ElementRule, Node, Operation, Path, Point, Props, Range, Schema,
        SlateError, Text,
    };
}

//! Optional document features.
//!
//! Features can install schema rules and contribute normalization operations.
//! They are intentionally model-level only; DOM/event behavior belongs to the
//! Leptos plugin layer.

use std::{fmt, sync::Arc};

use crate::{Editor, Operation, Path, Point, Schema};

/// A composable document feature.
///
/// Features own model-level schema rules, normalization invariants and the
/// structural hooks that a specialized block family (lists, tables, …) needs.
/// DOM/event behavior stays in the Leptos plugin layer.
pub trait Feature: Send + Sync {
    /// Install or replace schema rules when the feature is attached.
    fn install_schema(&self, _schema: &mut Schema) {}

    /// Return one operation that fixes a feature-specific invariant.
    ///
    /// Returning `None` means the document satisfies this feature. The operation
    /// is applied inside the core normalization loop, so it must make progress.
    fn normalize(&self, _editor: &Editor) -> Option<Operation> {
        None
    }

    /// Handle Backspace at the start of a leaf block.
    ///
    /// Called with the caret at offset zero of a block. Returning `true` means
    /// the feature performed the transform and the core fallback must be
    /// skipped. List features use this to lift or merge list items.
    fn handle_backspace(&self, _editor: &mut Editor, _point: &Point) -> bool {
        false
    }

    /// The ancestor depth at which Enter should stop splitting.
    ///
    /// The core splits the leaf block and every ancestor up to (and including)
    /// this depth. Returning a smaller depth keeps specialized containers (for
    /// example a list item) intact while still splitting nested paragraphs.
    fn split_stop_depth(&self, _editor: &Editor, _point: &Point) -> Option<usize> {
        None
    }

    /// Adjust the block that a newly inserted block is placed after.
    ///
    /// Features can climb out of their containers so a block inserted from a
    /// toolbar lands next to the container rather than inside it.
    fn adjust_insert_block(&self, _editor: &Editor, _block: &mut Path) {}

    /// Run before a block type change.
    ///
    /// Features can unwrap or normalize their containers first (for example a
    /// list unwraps its items before turning them into paragraphs).
    fn before_set_block(&self, _editor: &mut Editor, _element_type: &str) {}
}

/// An ordered collection of document features.
#[derive(Clone, Default)]
pub struct FeatureSet {
    features: Arc<Vec<Arc<dyn Feature>>>,
}

impl FeatureSet {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with<F>(mut self, feature: F) -> Self
    where
        F: Feature + 'static,
    {
        self.push(feature);
        self
    }

    pub fn with_arc(mut self, feature: Arc<dyn Feature>) -> Self {
        Arc::make_mut(&mut self.features).push(feature);
        self
    }

    pub fn push<F>(&mut self, feature: F)
    where
        F: Feature + 'static,
    {
        Arc::make_mut(&mut self.features).push(Arc::new(feature));
    }

    pub fn is_empty(&self) -> bool {
        self.features.is_empty()
    }

    /// Attached features in priority order.
    pub fn iter(&self) -> impl Iterator<Item = &Arc<dyn Feature>> {
        self.features.iter()
    }

    /// A cheap snapshot of the attached features.
    ///
    /// This is useful when a hook needs `&mut Editor`: the snapshot keeps the
    /// feature list independent of the borrow.
    pub fn snapshot(&self) -> Vec<Arc<dyn Feature>> {
        (*self.features).clone()
    }

    pub(crate) fn install_schema(&self, schema: &mut Schema) {
        for feature in self.features.iter() {
            feature.install_schema(schema);
        }
    }
}

impl fmt::Debug for FeatureSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FeatureSet")
            .field("len", &self.features.len())
            .finish()
    }
}

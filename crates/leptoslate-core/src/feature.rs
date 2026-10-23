//! Optional document features.
//!
//! Features can install schema rules and contribute normalization operations.
//! They are intentionally model-level only; DOM/event behavior belongs to the
//! Leptos plugin layer.

use std::{fmt, sync::Arc};

use crate::{Editor, Operation, Schema};

/// A composable document feature.
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

    pub(crate) fn install_schema(&self, schema: &mut Schema) {
        for feature in self.features.iter() {
            feature.install_schema(schema);
        }
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = &Arc<dyn Feature>> {
        self.features.iter()
    }
}

impl fmt::Debug for FeatureSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FeatureSet")
            .field("len", &self.features.len())
            .finish()
    }
}

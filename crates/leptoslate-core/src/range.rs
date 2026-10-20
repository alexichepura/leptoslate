//! Ranges describe a selection between an anchor and a focus point.

use std::cmp::Ordering;

use serde::{Deserialize, Serialize};

use crate::{path::Affinity, point::Point, Operation};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Range {
    pub anchor: Point,
    pub focus: Point,
}

impl Range {
    pub fn new(anchor: Point, focus: Point) -> Self {
        Self { anchor, focus }
    }

    pub fn collapsed(point: Point) -> Self {
        Self {
            anchor: point.clone(),
            focus: point,
        }
    }

    pub fn is_collapsed(&self) -> bool {
        self.anchor.equals(&self.focus)
    }

    pub fn is_backward(&self) -> bool {
        self.anchor.is_after(&self.focus)
    }

    /// The start point, regardless of direction.
    pub fn start(&self) -> &Point {
        if self.anchor.is_before(&self.focus) {
            &self.anchor
        } else {
            &self.focus
        }
    }

    /// The end point, regardless of direction.
    pub fn end(&self) -> &Point {
        if self.anchor.is_before(&self.focus) {
            &self.focus
        } else {
            &self.anchor
        }
    }

    /// `(start, end)` points, regardless of direction.
    pub fn edges(&self) -> (&Point, &Point) {
        (self.start(), self.end())
    }

    pub fn includes_point(&self, point: &Point) -> bool {
        let start = self.start();
        let end = self.end();
        start.compare(point) != Ordering::Greater && end.compare(point) != Ordering::Less
    }

    pub fn equals(&self, other: &Range) -> bool {
        self.anchor.equals(&other.anchor) && self.focus.equals(&other.focus)
    }

    pub fn transform(&self, op: &Operation, affinity: Affinity) -> Option<Range> {
        let anchor = self.anchor.transform(op, affinity)?;
        let focus = self.focus.transform(op, affinity)?;
        Some(Range { anchor, focus })
    }
}

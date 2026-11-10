//! Ranges describe a selection between an anchor and a focus point.

use std::cmp::Ordering;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    Operation, Path,
    path::{self, Affinity},
    point::Point,
    props::Props,
};

/// Affinity used by Slate's `Range.transform`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RangeAffinity {
    #[default]
    Inward,
    Outward,
    Forward,
    Backward,
}

/// The direction of a range, matching Slate's `Range.direction`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RangeDirection {
    Forward,
    Backward,
    Collapsed,
}

/// A path, point or range, mirroring Slate's `Location`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Location {
    Path(Path),
    Point(Point),
    Range(Range),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Range {
    pub anchor: Point,
    pub focus: Point,
    #[serde(flatten, default)]
    pub properties: Props,
}

impl Range {
    pub fn new(anchor: Point, focus: Point) -> Self {
        Self {
            anchor,
            focus,
            properties: Props::new(),
        }
    }

    pub fn collapsed(point: Point) -> Self {
        Self {
            anchor: point.clone(),
            focus: point,
            properties: Props::new(),
        }
    }

    pub fn is_collapsed(&self) -> bool {
        self.anchor.equals(&self.focus)
    }

    pub fn is_backward(&self) -> bool {
        self.anchor.is_after(&self.focus)
    }

    pub fn is_forward(&self) -> bool {
        !self.is_backward()
    }

    pub fn is_expanded(&self) -> bool {
        !self.is_collapsed()
    }

    /// Slate's `Range.direction`.
    pub fn direction(&self) -> RangeDirection {
        match self.focus.compare(&self.anchor) {
            Ordering::Greater => RangeDirection::Forward,
            Ordering::Less => RangeDirection::Backward,
            Ordering::Equal => RangeDirection::Collapsed,
        }
    }

    /// Slate's `Range.isRange`.
    pub fn is_range(value: &Value) -> bool {
        let Some(object) = value.as_object() else {
            return false;
        };
        object.get("anchor").is_some_and(Point::is_point)
            && object.get("focus").is_some_and(Point::is_point)
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

    /// Slate's `Range.edges(range, { reverse })`: swaps the edges per
    /// `reverse`, regardless of the range direction.
    pub fn edges_with(&self, reverse: bool) -> (&Point, &Point) {
        if self.is_backward() == reverse {
            (&self.anchor, &self.focus)
        } else {
            (&self.focus, &self.anchor)
        }
    }

    /// Slate's `Range.points`: anchor first, then focus.
    pub fn points(&self) -> [(Point, &'static str); 2] {
        [
            (self.anchor.clone(), "anchor"),
            (self.focus.clone(), "focus"),
        ]
    }

    pub fn includes_point(&self, point: &Point) -> bool {
        let start = self.start();
        let end = self.end();
        start.compare(point) != Ordering::Greater && end.compare(point) != Ordering::Less
    }

    /// Slate's `Range.includes`.
    pub fn includes(&self, target: &Location) -> bool {
        match target {
            Location::Range(target) => {
                if self.includes(&Location::Point(target.anchor.clone()))
                    || self.includes(&Location::Point(target.focus.clone()))
                {
                    return true;
                }
                let (rs, re) = self.edges();
                let (ts, te) = target.edges();
                ts.is_after(rs) && te.is_before(re)
            }
            Location::Point(target) => self.includes_point(target),
            Location::Path(target) => {
                let (start, end) = self.edges();
                path::compare(target, &start.path) != Ordering::Less
                    && path::compare(target, &end.path) != Ordering::Greater
            }
        }
    }

    pub fn equals(&self, other: &Range) -> bool {
        self.anchor.equals(&other.anchor) && self.focus.equals(&other.focus)
    }

    pub fn transform(&self, op: &Operation, affinity: Affinity) -> Option<Range> {
        let anchor = self.anchor.transform(op, affinity)?;
        let focus = self.focus.transform(op, affinity)?;
        Some(Range {
            anchor,
            focus,
            properties: Props::new(),
        })
    }

    /// Slate's `Range.transform` with inward (default), outward, forward or
    /// backward affinity.
    pub fn transform_with_affinity(
        &self,
        op: &Operation,
        affinity: RangeAffinity,
    ) -> Option<Range> {
        let (anchor_affinity, focus_affinity) = match affinity {
            RangeAffinity::Inward => {
                if self.is_forward() {
                    let anchor = Affinity::Forward;
                    let focus = if self.is_collapsed() {
                        anchor
                    } else {
                        Affinity::Backward
                    };
                    (anchor, focus)
                } else {
                    let anchor = Affinity::Backward;
                    let focus = if self.is_collapsed() {
                        anchor
                    } else {
                        Affinity::Forward
                    };
                    (anchor, focus)
                }
            }
            RangeAffinity::Outward => {
                if self.is_forward() {
                    (Affinity::Backward, Affinity::Forward)
                } else {
                    (Affinity::Forward, Affinity::Backward)
                }
            }
            RangeAffinity::Forward => (Affinity::Forward, Affinity::Forward),
            RangeAffinity::Backward => (Affinity::Backward, Affinity::Backward),
        };

        let anchor = self.anchor.transform(op, anchor_affinity)?;
        let focus = self.focus.transform(op, focus_affinity)?;
        Some(Range {
            anchor,
            focus,
            properties: Props::new(),
        })
    }
}

//! Undo/redo history.

use crate::{Operation, Range};

/// A group of operations applied as a single undoable unit.
#[derive(Clone, Debug)]
pub struct HistoryBatch {
    /// Pairs of `(operation, inverse)` in application order.
    pub ops: Vec<(Operation, Operation)>,
    pub before_selection: Option<Range>,
    pub after_selection: Option<Range>,
}

impl HistoryBatch {
    pub fn is_empty(&self) -> bool {
        self.ops.iter().all(|(op, _)| op.is_selection_only())
    }
}

#[derive(Clone, Debug)]
pub struct History {
    pub undo_stack: Vec<HistoryBatch>,
    pub redo_stack: Vec<HistoryBatch>,
    pub limit: usize,
    /// Number of contiguous typing operations currently coalesced.
    pub(crate) typing_run: usize,
}

impl Default for History {
    fn default() -> Self {
        Self {
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            limit: 100,
            typing_run: 0,
        }
    }
}

impl History {
    pub fn push(&mut self, batch: HistoryBatch) {
        if batch.is_empty() {
            return;
        }

        let batch_is_typing = batch.ops.iter().all(is_typing);
        let merge = batch_is_typing
            && self.typing_run > 0
            && self.typing_run + batch.ops.len() <= 64
            && self
                .undo_stack
                .last()
                .map(|last| can_coalesce(last, &batch))
                .unwrap_or(false);

        if merge {
            let added = batch.ops.len();
            if let Some(last) = self.undo_stack.last_mut() {
                last.ops.extend(batch.ops);
                last.after_selection = batch.after_selection;
            }
            self.typing_run += added;
        } else {
            self.undo_stack.push(batch);
            self.typing_run = if batch_is_typing {
                self.undo_stack
                    .last()
                    .map(|batch| batch.ops.len())
                    .unwrap_or(0)
            } else {
                0
            };
        }

        self.redo_stack.clear();
        while self.undo_stack.len() > self.limit {
            self.undo_stack.remove(0);
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    pub fn break_coalescing(&mut self) {
        self.typing_run = 0;
    }
}

fn is_typing((op, _): &(Operation, Operation)) -> bool {
    matches!(
        op,
        Operation::InsertText { .. } | Operation::RemoveText { .. }
    )
}

fn can_coalesce(previous: &HistoryBatch, next: &HistoryBatch) -> bool {
    if previous.ops.is_empty()
        || next.ops.is_empty()
        || !previous.ops.iter().all(is_typing)
        || !next.ops.iter().all(is_typing)
    {
        return false;
    }

    // Only merge typing that continues from where the previous edit ended.
    previous.after_selection == next.before_selection
}

//! Grid view-state: cursor, scroll offset, and drill-down stack. This lives
//! in core rather than in the ratatui/egui widgets so every frontend —
//! including a future native tablet UI — inherits the exact same navigation
//! behaviour.

/// A drill-down entry: which node we descended into and what row we were at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DrillEntry {
    /// The row in the parent sheet that was selected when we descended.
    pub parent_row: usize,
    /// The column in the parent sheet that was selected.
    pub parent_col: usize,
}

/// Cursor position, scroll offset, and drill-down stack over the document
/// grid.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GridState {
    /// (row, column) of the cursor.
    pub cursor: (usize, usize),
    /// (row, column) of the top-left visible cell.
    pub offset: (usize, usize),
    /// Stack of drill-down entries for tree navigation.
    pub drill_stack: Vec<DrillEntry>,
    /// How many columns are pinned to the left edge while scrolling
    /// horizontally. Wide files are unreadable without this: you lose
    /// sight of which row you are on.
    pub frozen_cols: usize,
    /// Marked rows, by *source* index so marks survive filtering.
    pub marks: std::collections::BTreeSet<usize>,
    /// Source row indices currently visible, or `None` for all of them.
    /// The cursor addresses display rows; the sheet addresses source rows,
    /// and [`GridState::source_row`] is the only bridge between them.
    pub visible: Option<Vec<usize>>,
}

impl GridState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Move the cursor by a delta, clamped to a `rows` × `cols` grid.
    pub fn move_by(&mut self, dr: isize, dc: isize, rows: usize, cols: usize) {
        let (r, c) = self.cursor;
        let r = r.saturating_add_signed(dr).min(rows.saturating_sub(1));
        let c = c.saturating_add_signed(dc).min(cols.saturating_sub(1));
        self.cursor = (r, c);
    }

    /// Jump to a position, clamped to a `rows` × `cols` grid.
    pub fn move_to(&mut self, row: usize, col: usize, rows: usize, cols: usize) {
        self.cursor = (
            row.min(rows.saturating_sub(1)),
            col.min(cols.saturating_sub(1)),
        );
    }

    /// Scroll vertically so the cursor is inside a viewport `view_rows`
    /// high. Horizontal scrolling is the frontend's business — it depends on
    /// rendered column widths, which core knows nothing about.
    pub fn ensure_visible(&mut self, view_rows: usize) {
        let (cursor_row, _) = self.cursor;
        if cursor_row < self.offset.0 {
            self.offset.0 = cursor_row;
        }
        if view_rows > 0 && cursor_row >= self.offset.0 + view_rows {
            self.offset.0 = cursor_row + 1 - view_rows;
        }
    }

    /// Push a drill-down entry (before descending into a child node).
    pub fn drill_down(&mut self) {
        self.drill_stack.push(DrillEntry {
            parent_row: self.cursor.0,
            parent_col: self.cursor.1,
        });
        self.cursor = (0, 0);
        self.offset = (0, 0);
    }

    /// Pop back to the parent sheet. Returns `false` if already at root.
    pub fn drill_up(&mut self) -> bool {
        let Some(entry) = self.drill_stack.pop() else {
            return false;
        };
        self.cursor = (entry.parent_row, entry.parent_col);
        self.offset = (0, 0);
        true
    }

    /// The source row a display row refers to.
    ///
    /// Every lookup into a `Sheet` must go through this: with a filter
    /// applied, display row 3 is not source row 3, and mixing the two
    /// shows one row's data under another row's heading.
    pub fn source_row(&self, display_row: usize) -> usize {
        match &self.visible {
            Some(rows) => rows.get(display_row).copied().unwrap_or(display_row),
            None => display_row,
        }
    }

    /// How many rows are on display.
    pub fn visible_rows(&self, total: usize) -> usize {
        match &self.visible {
            Some(rows) => rows.len(),
            None => total,
        }
    }

    /// The display row showing `source`, if it is visible.
    pub fn display_row(&self, source: usize) -> Option<usize> {
        match &self.visible {
            Some(rows) => rows.iter().position(|&r| r == source),
            None => Some(source),
        }
    }

    /// Drop any filter, keeping the cursor on the same source row.
    pub fn clear_filter(&mut self) {
        let source = self.source_row(self.cursor.0);
        self.visible = None;
        self.cursor.0 = source;
    }

    /// Mark or unmark a source row. Returns true if it is now marked.
    pub fn toggle_mark(&mut self, source: usize) -> bool {
        if self.marks.contains(&source) {
            self.marks.remove(&source);
            false
        } else {
            self.marks.insert(source);
            true
        }
    }

    /// How deep we are in the tree (0 = root).
    pub fn depth(&self) -> usize {
        self.drill_stack.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn movement_is_clamped() {
        let mut g = GridState::new();
        g.move_by(-5, -5, 10, 4);
        assert_eq!(g.cursor, (0, 0));
        g.move_by(100, 100, 10, 4);
        assert_eq!(g.cursor, (9, 3));
    }

    #[test]
    fn empty_grid_stays_at_origin() {
        let mut g = GridState::new();
        g.move_by(1, 1, 0, 0);
        assert_eq!(g.cursor, (0, 0));
    }

    #[test]
    fn ensure_visible_scrolls_down_and_up() {
        let mut g = GridState::new();
        g.move_to(20, 0, 100, 5);
        g.ensure_visible(10);
        assert_eq!(g.offset.0, 11); // cursor is the last visible row
        g.move_to(3, 0, 100, 5);
        g.ensure_visible(10);
        assert_eq!(g.offset.0, 3); // scrolled back up to the cursor
    }

    #[test]
    fn drill_down_and_up() {
        let mut g = GridState::new();
        g.move_to(5, 2, 20, 10);
        g.drill_down();
        assert_eq!(g.cursor, (0, 0));
        assert_eq!(g.depth(), 1);
        g.drill_up();
        assert_eq!(g.cursor, (5, 2));
        assert_eq!(g.depth(), 0);
    }

    #[test]
    fn source_row_maps_through_a_filter() {
        let mut g = GridState::new();
        assert_eq!(g.source_row(3), 3, "no filter: identity");

        g.visible = Some(vec![0, 4, 9]);
        assert_eq!(g.source_row(0), 0);
        assert_eq!(g.source_row(1), 4);
        assert_eq!(g.source_row(2), 9);
        assert_eq!(g.visible_rows(100), 3);
        assert_eq!(g.display_row(9), Some(2));
        assert_eq!(g.display_row(5), None, "filtered out");
    }

    /// Clearing a filter should leave you looking at the same row, not
    /// jump to whatever row happens to share the display index.
    #[test]
    fn clearing_a_filter_keeps_the_cursor_on_its_row() {
        let mut g = GridState::new();
        g.visible = Some(vec![0, 4, 9]);
        g.cursor = (2, 0);
        g.clear_filter();
        assert_eq!(g.cursor.0, 9);
        assert!(g.visible.is_none());
    }

    /// Marks are stored by source row, so filtering does not lose them.
    #[test]
    fn marks_survive_filtering() {
        let mut g = GridState::new();
        assert!(g.toggle_mark(7));
        g.visible = Some(vec![7]);
        assert!(g.marks.contains(&7));
        g.visible = None;
        assert!(g.marks.contains(&7));
        assert!(!g.toggle_mark(7), "toggles off");
        assert!(g.marks.is_empty());
    }

    #[test]
    fn drill_up_at_root_is_noop() {
        let mut g = GridState::new();
        assert!(!g.drill_up());
        assert_eq!(g.depth(), 0);
    }

    #[test]
    fn nested_drill_stacks() {
        let mut g = GridState::new();
        g.move_to(1, 0, 10, 5);
        g.drill_down();
        g.move_to(2, 0, 5, 3);
        g.drill_down();
        assert_eq!(g.depth(), 2);
        g.drill_up();
        assert_eq!(g.cursor, (2, 0));
        g.drill_up();
        assert_eq!(g.cursor, (1, 0));
    }
}

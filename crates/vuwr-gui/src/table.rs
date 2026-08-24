//! The three views, drawn with egui.
//!
//! Every value shown comes from [`Session`], and every row index goes
//! through `grid.source_row` — the same rule the TUI follows, for the same
//! reason: with a filter applied, display row 3 is not source row 3.

use eframe::egui::{self, Color32, RichText};
use vuwr_core::Session;

/// Rows drawn per screen. egui scrolls the whole grid, so this only sets
/// what a page-down means.
const PAGE_ROWS: usize = 25;

pub fn table(session: &mut Session, ui: &mut egui::Ui) {
    session.set_viewport_rows(PAGE_ROWS);
    let (headers, rows, cols) = session.table_dims();
    if rows == 0 || cols == 0 {
        ui.label("nothing to show in this view");
        return;
    }

    let cursor = session.grid.cursor;
    let separate_header = session.has_separate_header();
    let frozen = session.grid.frozen_cols.min(cols);

    egui::ScrollArea::both().show(ui, |ui| {
        egui::Grid::new("sheet")
            .striped(true)
            .num_columns(cols + 1)
            .show(ui, |ui| {
                if separate_header {
                    ui.label("");
                    for h in headers.iter().take(cols) {
                        ui.label(RichText::new(h).strong());
                    }
                    ui.end_row();
                }

                for r in 0..rows {
                    let source = session.grid.source_row(r);
                    let marked = session.grid.marks.contains(&source);
                    ui.label(if marked { "*" } else { " " });

                    for c in 0..cols {
                        let text = session.table_cell(r, c).unwrap_or_default();
                        let mut rich = RichText::new(&text).monospace();
                        // Row 0 is the header for CSV, which carries it in
                        // the data rather than separately.
                        if r == 0 && !separate_header {
                            rich = rich.strong();
                        }
                        if c < frozen {
                            rich = rich.color(Color32::from_rgb(150, 190, 255));
                        }
                        if session.search.as_ref().is_some_and(|s| s.matches(&text)) {
                            rich = rich.underline();
                        }
                        let selected = (r, c) == cursor;
                        if ui.selectable_label(selected, rich).clicked() {
                            session.grid.cursor = (r, c);
                        }
                    }
                    ui.end_row();
                }
            });
    });
}

pub fn tree(session: &mut Session, ui: &mut egui::Ui) {
    session.set_viewport_rows(PAGE_ROWS);
    let (keys, rows, _) = session.table_dims();
    if rows == 0 {
        ui.label("nothing to show in this view");
        return;
    }
    let cursor_row = session.grid.cursor.0;

    egui::ScrollArea::both().show(ui, |ui| {
        egui::Grid::new("tree")
            .striped(true)
            .num_columns(2)
            .show(ui, |ui| {
                for r in 0..rows {
                    let key = keys.get(r).cloned().unwrap_or_default();
                    let summary = session.table_cell(r, 0).unwrap_or_default();
                    let selected = r == cursor_row;
                    if ui
                        .selectable_label(selected, RichText::new(&key).monospace().strong())
                        .clicked()
                    {
                        session.grid.cursor = (r, 0);
                    }
                    ui.label(RichText::new(&summary).monospace());
                    ui.end_row();
                }
            });
    });
}

pub fn text(session: &mut Session, ui: &mut egui::Ui) {
    session.set_viewport_rows(PAGE_ROWS);
    let (_, lines, _) = session.table_dims();
    let cursor_row = session.grid.cursor.0;
    let gutter = lines.to_string().len().max(2);

    egui::ScrollArea::both().show(ui, |ui| {
        for n in 0..lines {
            let line = session.table_cell(n, 0).unwrap_or_default();
            let selected = n == cursor_row;
            let text = format!("{:>gutter$}  {}", n + 1, line, gutter = gutter);
            if ui
                .selectable_label(selected, RichText::new(text).monospace())
                .clicked()
            {
                session.grid.cursor = (n, 0);
            }
        }
    });
}

//! The toolbar.
//!
//! What separates a desktop application from a terminal one in a window:
//! the things you can do are visible and clickable, rather than known or
//! not known. Every button runs a [`Command`], the same ones the keyboard
//! and the menus use, so nothing here is a second implementation.

use eframe::egui::{self, RichText};
use vuwr_core::{Command, SortDirection, ViewMode};

use crate::VuwrApp;

/// Draw the toolbar and return the command a button asked for.
pub fn toolbar(app: &VuwrApp, ui: &mut egui::Ui) -> Option<Command> {
    let mut clicked = None;
    let Some(session) = app.try_session() else {
        ui.horizontal(|ui| {
            ui.label(RichText::new("no document").weak());
        });
        return None;
    };

    ui.horizontal_wrapped(|ui| {
        // View mode, as a segmented control — the current mode is visible
        // rather than something you deduce.
        for view in session.available_views() {
            let (label, cmd) = match view {
                ViewMode::Text => ("text", Command::ViewText),
                ViewMode::Tree => ("tree", Command::ViewTree),
                ViewMode::Table => ("table", Command::ViewTable),
            };
            let selected = session.view_mode() == view;
            if ui
                .selectable_label(selected, label)
                .on_hover_text(format!("Switch to {label} view"))
                .clicked()
                && !selected
            {
                clicked = Some(cmd);
            }
        }

        ui.separator();

        // Tree shaping. Only meaningful where there is a tree.
        if session.view_mode() == ViewMode::Tree {
            if button(ui, "expand", "Open every node").clicked() {
                clicked = Some(Command::ExpandAll);
            }
            if button(ui, "collapse", "Close every node").clicked() {
                clicked = Some(Command::CollapseAll);
            }
            ui.separator();
        }

        // Layout. Not CSV: its shape is its content, with nothing to
        // re-indent.
        let can_format = session.doc.is_json() || session.doc.is_xml();
        ui.add_enabled_ui(can_format, |ui| {
            if button(ui, "format", "Re-indent: one value per line (Ctrl+I)").clicked() {
                clicked = Some(Command::FormatPretty);
            }
            if button(
                ui,
                "smart",
                "Re-indent, keeping short lists on one line (Ctrl+J)",
            )
            .clicked()
            {
                clicked = Some(Command::FormatSmart);
            }
            if button(ui, "compact", "Remove all whitespace (Ctrl+Shift+I)").clicked() {
                clicked = Some(Command::FormatCompact);
            }
        });

        ui.separator();

        // Sort, filter, search — the table verbs.
        let table = session.doc.sheet().is_some();
        ui.add_enabled_ui(table, |ui| {
            // Say which way the current column is sorted, in words: an
            // arrow glyph renders as an empty box in egui's fonts.
            let arrow = match session.sort_spec() {
                Some(spec) if spec.column == session.grid.cursor.1 => match spec.direction {
                    SortDirection::Ascending => "sort a-z",
                    SortDirection::Descending => "sort z-a",
                },
                _ => "sort",
            };
            if button(ui, arrow, "Sort by the selected column (again to reverse)").clicked() {
                clicked = Some(Command::Sort);
            }
            if button(ui, "1-9", "Sort the selected column as numbers").clicked() {
                clicked = Some(Command::SortNumeric);
            }
            if button(ui, "nat", "Sort naturally: file2 before file10").clicked() {
                clicked = Some(Command::SortNatural);
            }
            if button(ui, "filter", "Show only rows matching a pattern").clicked() {
                clicked = Some(Command::Filter);
            }
            if button(ui, "find", "Search (Ctrl+F)").clicked() {
                clicked = Some(Command::Find);
            }
            // Only offer the undo of a view change when there is one.
            let filtered = session.is_filtered() || session.sort_spec().is_some();
            ui.add_enabled_ui(filtered, |ui| {
                if button(ui, "clear", "Clear the filter and sort").clicked() {
                    clicked = Some(Command::ClearFilter);
                }
            });
        });

        ui.separator();

        ui.add_enabled_ui(table, |ui| {
            if button(ui, "copy row", "Copy the selected row").clicked() {
                clicked = Some(Command::CopyRow);
            }
        });
        ui.separator();

        if session.doc.is_xml()
            && ui
                .selectable_label(session.decoded_text, "decoded")
                .on_hover_text("Show text view as the markup it represents, not the source (E)")
                .clicked()
        {
            clicked = Some(Command::ToggleDecoded);
        }
        if ui
            .selectable_label(session.show_detail, "detail")
            .on_hover_text("Show the selected value in full (V)")
            .clicked()
        {
            clicked = Some(Command::ToggleDetail);
        }
        ui.separator();

        if button(ui, "undo", "Undo (Ctrl+Z)").clicked() {
            clicked = Some(Command::Undo);
        }
        if button(ui, "redo", "Redo (Ctrl+Shift+Z)").clicked() {
            clicked = Some(Command::Redo);
        }

        ui.separator();
        if button(ui, "save", "Save the file (Ctrl+S)").clicked() {
            clicked = Some(Command::Save);
        }
    });

    clicked
}

/// A toolbar button.
///
/// Labelled with a word rather than a glyph: egui bundles Ubuntu Light and
/// an emoji font between them covering very few symbols, so anything
/// decorative renders as an empty box. The full explanation is on hover.
fn button(ui: &mut egui::Ui, label: &str, tooltip: &str) -> egui::Response {
    ui.add(egui::Button::new(RichText::new(label).size(12.0)).min_size([0.0, 22.0].into()))
        .on_hover_text(tooltip)
}

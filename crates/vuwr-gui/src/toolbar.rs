//! The toolbar.
//!
//! What separates a desktop application from a terminal one in a window:
//! the things you can do are visible and clickable, rather than known or
//! not known. Every button runs a [`Command`], the same ones the keyboard
//! and the menus use, so nothing here is a second implementation.

use eframe::egui::{self, RichText};
use vuwr_core::{Command, SortDirection, ViewMode};

use crate::{VuwrApp, theme};

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
        ui.spacing_mut().item_spacing.x = 4.0;
        // View mode, as a segmented control: the current mode is visible
        // rather than something you deduce.
        theme::segmented(ui, |ui| {
            for view in session.available_views() {
                let (label, cmd) = match view {
                    ViewMode::Text => ("Text", Command::ViewText),
                    ViewMode::Tree => ("Tree", Command::ViewTree),
                    ViewMode::Table => ("Table", Command::ViewTable),
                };
                let selected = session.view_mode() == view;
                if theme::segment(ui, label, selected) && !selected {
                    clicked = Some(cmd);
                }
            }
        });

        theme::divider(ui);

        // Tree shaping. Only meaningful where there is a tree.
        if session.view_mode() == ViewMode::Tree {
            if button(ui, "Expand", "Open every node").clicked() {
                clicked = Some(Command::ExpandAll);
            }
            if button(ui, "Collapse", "Close every node").clicked() {
                clicked = Some(Command::CollapseAll);
            }
            theme::divider(ui);
        }

        // Layout. Not CSV: its shape is its content, with nothing to
        // re-indent.
        let can_format = session.doc.is_json() || session.doc.is_xml();
        ui.add_enabled_ui(can_format, |ui| {
            if button(ui, "Format", "Re-indent: one value per line (Ctrl+I)").clicked() {
                clicked = Some(Command::FormatPretty);
            }
            if button(
                ui,
                "Smart",
                "Re-indent, keeping short lists on one line (Ctrl+J)",
            )
            .clicked()
            {
                clicked = Some(Command::FormatSmart);
            }
            if button(ui, "Compact", "Remove all whitespace (Ctrl+Shift+I)").clicked() {
                clicked = Some(Command::FormatCompact);
            }
        });

        theme::divider(ui);

        // Sort, filter, search — the table verbs.
        let table = session.doc.sheet().is_some();
        ui.add_enabled_ui(table, |ui| {
            // Say which way the current column is sorted, in words: an
            // arrow glyph renders as an empty box in egui's fonts.
            let arrow = match session.sort_spec() {
                Some(spec) if spec.column == session.grid.cursor.1 => match spec.direction {
                    SortDirection::Ascending => "Sort A–Z",
                    SortDirection::Descending => "Sort Z–A",
                },
                _ => "Sort",
            };
            if button(ui, arrow, "Sort by the selected column (again to reverse)").clicked() {
                clicked = Some(Command::Sort);
            }
            if button(ui, "1–9", "Sort the selected column as numbers").clicked() {
                clicked = Some(Command::SortNumeric);
            }
            if button(ui, "Nat", "Sort naturally: file2 before file10").clicked() {
                clicked = Some(Command::SortNatural);
            }
            let label = match session.visible_count() {
                Some(n) => format!("Filter · {n}"),
                None => "Filter".to_string(),
            };
            if filter_button(ui, &label, session.is_filtered()).clicked() {
                clicked = Some(Command::Filter);
            }
            if button(ui, "Find", "Search (Ctrl+F)").clicked() {
                clicked = Some(Command::Find);
            }
            // Only offer the undo of a view change when there is one.
            let filtered = session.is_filtered() || session.sort_spec().is_some();
            ui.add_enabled_ui(filtered, |ui| {
                if button(ui, "Clear", "Clear the filter and sort").clicked() {
                    clicked = Some(Command::ClearFilter);
                }
            });
        });

        theme::divider(ui);

        ui.add_enabled_ui(table, |ui| {
            if button(ui, "Copy row", "Copy the selected row").clicked() {
                clicked = Some(Command::CopyRow);
            }
        });
        theme::divider(ui);

        // Asked for, not automatic: the scan re-reads the whole document,
        // which is a visible hitch on a large one after every edit.
        if button(ui, "Lint", "Check for problems a parser lets through").clicked() {
            clicked = Some(Command::Lint);
        }
        if button(ui, "Detail", "Show the selected value in full (V)").clicked() {
            clicked = Some(Command::ToggleDetail);
        }

        // Right-hand end: how XML text is being read.
        if session.doc.is_xml() {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                theme::segmented(ui, |ui| {
                    if theme::segment(ui, "Raw", !session.decoded_text) && session.decoded_text {
                        clicked = Some(Command::ToggleDecoded);
                    }
                    if theme::segment(ui, "Decoded", session.decoded_text) && !session.decoded_text
                    {
                        clicked = Some(Command::ToggleDecoded);
                    }
                });
            });
        }
    });

    clicked
}

/// Filter, which unlike the others carries state: when it is on, it takes
/// the accent tint and says how many rows are left. Amber is reserved for
/// warnings, so an active view is blue.
fn filter_button(ui: &mut egui::Ui, label: &str, active: bool) -> egui::Response {
    if !active {
        return theme::action(ui, label, ui.is_enabled());
    }
    ui.add(
        egui::Button::new(RichText::new(label).color(theme::ACCENT_TEXT))
            .fill(theme::ACCENT_TINT)
            .stroke(egui::Stroke::new(1.0_f32, theme::ACCENT_BORDER))
            .corner_radius(egui::CornerRadius::same(5)),
    )
}

/// A toolbar button.
///
/// Labelled with a word rather than a glyph: egui bundles Ubuntu Light and
/// an emoji font between them covering very few symbols, so anything
/// decorative renders as an empty box. The full explanation is on hover.
fn button(ui: &mut egui::Ui, label: &str, tooltip: &str) -> egui::Response {
    theme::action(ui, label, ui.is_enabled()).on_hover_text(tooltip)
}

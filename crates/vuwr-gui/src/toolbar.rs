//! The toolbar.
//!
//! What separates a desktop application from a terminal one in a window:
//! the things you can do are visible and clickable, rather than known or
//! not known. Every button runs a [`Command`], the same ones the keyboard
//! and the menus use, so nothing here is a second implementation.

use eframe::egui::{self, RichText};
use vuwr_core::{Command, Layout, SortDirection, ViewMode};

use crate::{VuwrApp, theme};

/// Draw the toolbar and return the command a button asked for.
pub fn toolbar(app: &VuwrApp, ui: &mut egui::Ui) -> Toolbar {
    let mut clicked = None;
    let mut toggle = None;
    let Some(session) = app.try_session() else {
        ui.horizontal(|ui| {
            ui.label(RichText::new("no document").weak());
        });
        return Toolbar::default();
    };

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        // View mode, as a segmented control: the current mode is visible
        // rather than something you deduce.
        theme::segmented(ui, |ui| {
            // Left to right in the order the keys are in, so the segments
            // and `1 2 3` agree.
            let available = session.available_views();
            for view in crate::VIEW_ORDER
                .iter()
                .copied()
                .filter(|v| available.contains(v))
            {
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

        // Layout. Not CSV, whose shape is its content, and only in the
        // text view: re-indenting changes the source, and the source is
        // what that view shows. Pressing it from the tree or the table
        // would change the file with nothing on screen to show for it.
        // The keys and `:format` still work from anywhere.
        let can_format = (session.doc.is_json() || session.doc.is_xml())
            && session.view_mode() == ViewMode::Text;
        if can_format {
            // Which one is lit says what was applied. Nothing is lit for
            // a file as it arrived, which is honest: it has whatever
            // shape it was written with.
            let applied = session.layout();
            let is = |l: Layout| applied == Some(l);
            if state_button(
                ui,
                "Format",
                "Re-indent: one value per line (Ctrl+I)",
                is(Layout::Pretty),
            )
            .clicked()
            {
                clicked = Some(Command::FormatPretty);
            }
            if state_button(
                ui,
                "Smart",
                "Re-indent, keeping short lists on one line (Ctrl+J)",
                is(Layout::Smart),
            )
            .clicked()
            {
                clicked = Some(Command::FormatSmart);
            }
            if state_button(
                ui,
                "Compact",
                "Remove all whitespace (Ctrl+Shift+I)",
                is(Layout::Compact),
            )
            .clicked()
            {
                clicked = Some(Command::FormatCompact);
            }
            theme::divider(ui);
        }

        // Sorting is a table verb: it reorders rows, and rows are what a
        // table has.
        let table = session.view_mode() == ViewMode::Table && session.doc.sheet().is_some();
        if table {
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

            // Which columns are on display. A feed has twenty-three and
            // you are usually reading four.
            let hidden = session.hidden_column_count();
            let label = if hidden == 0 {
                "Columns".to_string()
            } else {
                format!("Columns · {hidden} hidden")
            };
            let response =
                theme::action(ui, &label, true).on_hover_text("Choose which columns to show");
            egui::Popup::menu(&response)
                .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                .show(|ui| {
                    ui.set_min_width(220.0);
                    for (i, (name, shown)) in session.column_visibility().into_iter().enumerate() {
                        let label = if name.is_empty() {
                            format!("column {}", i + 1)
                        } else {
                            name
                        };
                        if ui.selectable_label(shown, label).clicked() {
                            toggle = Some(i);
                        }
                    }
                    ui.separator();
                    if ui.button("Show all").clicked() {
                        clicked = Some(Command::ShowAllColumns);
                    }
                });
        }

        // Finding and filtering are not table verbs: you look for a value
        // wherever you are.
        let label = match session.visible_count() {
            Some(n) => format!("Filter · {n}"),
            None => "Filter".to_string(),
        };
        if filter_button(ui, &label, session.is_filtered()).clicked() {
            clicked = Some(Command::Filter);
        }
        // Take it off where it is on, rather than only from a Clear button
        // at the far end of the bar that reads as chrome. A thing that is
        // switched on should carry its own switch.
        if session.is_filtered()
            && filter_button(ui, "×", true)
                .on_hover_text("Remove the filter")
                .clicked()
        {
            clicked = Some(Command::ClearFilter);
        }
        if button(ui, "Find", "Search (Ctrl+F)").clicked() {
            clicked = Some(Command::Find);
        }
        // Replacing is next to finding because it starts as one.
        let replacing = session.substitution_active();
        if filter_button(ui, "Replace", replacing)
            .on_hover_text("Find and replace, with $1 for a captured group (%)")
            .clicked()
        {
            clicked = Some(Command::Substitute);
        }
        // The two halves of stepping, offered only once there is
        // something to step through.
        if replacing {
            if button(ui, "This one", "Replace the match under the cursor (.)").clicked() {
                clicked = Some(Command::SubstituteOne);
            }
            if button(ui, "Skip", "Leave it and go to the next (n)").clicked() {
                clicked = Some(Command::FindNext);
            }
            if button(ui, "All", "Replace the rest, as one undo step (a)").clicked() {
                clicked = Some(Command::SubstituteAll);
            }
        }
        // Only offer the undo of a view change when there is one.
        let filtered = session.is_filtered() || session.sort_spec().is_some();
        ui.add_enabled_ui(filtered, |ui| {
            if button(ui, "Clear", "Clear the filter and sort").clicked() {
                clicked = Some(Command::ClearFilter);
            }
        });

        theme::divider(ui);

        if table && button(ui, "Copy row", "Copy the selected row").clicked() {
            clicked = Some(Command::CopyRow);
        }

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
            let rest = ui.available_rect_before_wrap();
            ui.allocate_ui_with_layout(
                rest.size(),
                egui::Layout::right_to_left(egui::Align::Center),
                |ui| {
                    theme::segmented(ui, |ui| {
                        if theme::segment(ui, "Raw", !session.decoded_text) && session.decoded_text
                        {
                            clicked = Some(Command::ToggleDecoded);
                        }
                        if theme::segment(ui, "Decoded", session.decoded_text)
                            && !session.decoded_text
                        {
                            clicked = Some(Command::ToggleDecoded);
                        }
                    });
                },
            );
        }
    });

    Toolbar {
        command: clicked,
        toggle_column: toggle,
    }
}

/// What the toolbar was asked for this frame.
#[derive(Default)]
pub struct Toolbar {
    pub command: Option<Command>,
    /// A column to put away or bring back, by its index in the document.
    pub toggle_column: Option<usize>,
}

/// An action that is also a state: lit when the document is in it.
///
/// The accent, as everywhere else something is active — the same tint the
/// filter uses, so "on" looks the same wherever it appears.
fn state_button(ui: &mut egui::Ui, label: &str, tooltip: &str, on: bool) -> egui::Response {
    if !on {
        return button(ui, label, tooltip);
    }
    ui.add(
        egui::Button::new(RichText::new(label).color(theme::accent_text()))
            .fill(theme::accent_tint())
            .stroke(egui::Stroke::new(1.0_f32, theme::accent_border()))
            .corner_radius(egui::CornerRadius::same(5)),
    )
    .on_hover_text(tooltip)
}

/// Filter, which unlike the others carries state: when it is on, it takes
/// the accent tint and says how many rows are left. Amber is reserved for
/// warnings, so an active view is blue.
fn filter_button(ui: &mut egui::Ui, label: &str, active: bool) -> egui::Response {
    if !active {
        return theme::action(ui, label, ui.is_enabled());
    }
    ui.add(
        egui::Button::new(RichText::new(label).color(theme::accent_text()))
            .fill(theme::accent_tint())
            .stroke(egui::Stroke::new(1.0_f32, theme::accent_border()))
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

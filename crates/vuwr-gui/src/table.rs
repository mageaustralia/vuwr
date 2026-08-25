//! The three views, drawn with egui.
//!
//! Every value shown comes from [`Session`], and every row index goes
//! through `grid.source_row` — the same rule the TUI follows, for the same
//! reason: with a filter applied, display row 3 is not source row 3.

use eframe::egui::{self, Color32, RichText};
use vuwr_core::{PathSeg, RowKind, Session, ValueKind};

/// Rows drawn per screen. egui scrolls the whole grid, so this only sets
/// what a page-down means.
const PAGE_ROWS: usize = 25;

pub fn table(session: &mut Session, ui: &mut egui::Ui) -> bool {
    session.set_viewport_rows(PAGE_ROWS);
    let mut edit = false;
    let (headers, rows, cols) = session.table_dims();
    if rows == 0 || cols == 0 {
        ui.label("nothing to show in this view");
        return false;
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
                        let response = ui.selectable_label(selected, rich);
                        if response.clicked() {
                            session.grid.cursor = (r, c);
                        }
                        if response.double_clicked() {
                            session.grid.cursor = (r, c);
                            edit = true;
                        }
                    }
                    ui.end_row();
                }
            });
    });
    edit
}

/// A small filled circle marking a repeated key. Painted for the same
/// reason as the triangle: the fonts have no dot glyph.
fn duplicate_dot(ui: &mut egui::Ui) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(DISCLOSURE, DISCLOSURE), egui::Sense::hover());
    ui.painter()
        .circle_filled(rect.center(), 3.5, Color32::from_rgb(220, 80, 80));
    response
}

/// The text being typed, with a caret drawn where it actually is.
///
/// egui's own TextEdit would bring its own state and its own key
/// handling; the buffer lives in the session so both frontends commit the
/// same way, so this renders it rather than replacing it.
pub fn caret_text(session: &Session) -> egui::text::LayoutJob {
    use egui::text::{LayoutJob, TextFormat};
    let mut job = LayoutJob::default();
    let Some((_, buf)) = session.entry() else {
        return job;
    };
    let caret = session.entry_caret().min(buf.len());
    let (before, after) = buf.split_at(caret);
    let font = egui::FontId::monospace(13.0);

    job.append(before, 0.0, TextFormat::simple(font.clone(), Color32::GRAY));
    let mut chars = after.chars();
    let under = chars.next();
    let rest: String = chars.collect();
    job.append(
        &under.map(|c| c.to_string()).unwrap_or_else(|| " ".into()),
        0.0,
        TextFormat {
            font_id: font.clone(),
            color: Color32::BLACK,
            background: Color32::from_rgb(255, 210, 90),
            ..Default::default()
        },
    );
    job.append(&rest, 0.0, TextFormat::simple(font, Color32::GRAY));
    job
}

/// Width of the disclosure control, so leaves line up with containers.
const DISCLOSURE: f32 = 14.0;

/// The open/closed triangle.
///
/// Painted rather than typed: egui's bundled fonts have no triangle glyph,
/// so a character here renders as an empty box.
fn disclosure(ui: &mut egui::Ui, expanded: bool) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(DISCLOSURE, DISCLOSURE), egui::Sense::click());
    let color = if response.hovered() {
        ui.visuals().strong_text_color()
    } else {
        ui.visuals().weak_text_color()
    };
    let c = rect.center();
    let r = 3.5;
    let points = if expanded {
        // Pointing down: the node is open.
        vec![
            egui::pos2(c.x - r, c.y - r * 0.6),
            egui::pos2(c.x + r, c.y - r * 0.6),
            egui::pos2(c.x, c.y + r * 0.8),
        ]
    } else {
        // Pointing right: there is more inside.
        vec![
            egui::pos2(c.x - r * 0.6, c.y - r),
            egui::pos2(c.x - r * 0.6, c.y + r),
            egui::pos2(c.x + r * 0.8, c.y),
        ]
    };
    ui.painter().add(egui::Shape::convex_polygon(
        points,
        color,
        egui::Stroke::NONE,
    ));
    response
}

/// Colour by value type, so the shape of the data reads at a glance.
pub fn value_color(kind: ValueKind, dark: bool) -> Color32 {
    use ValueKind as V;
    if dark {
        match kind {
            V::Null => Color32::from_rgb(140, 140, 150),
            V::Bool => Color32::from_rgb(120, 170, 255),
            V::Number => Color32::from_rgb(150, 210, 130),
            V::String => Color32::from_rgb(230, 160, 120),
            V::Array | V::Object | V::Element => Color32::from_rgb(200, 200, 210),
            V::Comment => Color32::from_rgb(120, 120, 130),
            V::Text | V::Other => Color32::from_rgb(210, 210, 215),
        }
    } else {
        match kind {
            V::Null => Color32::from_rgb(110, 110, 120),
            V::Bool => Color32::from_rgb(30, 90, 200),
            V::Number => Color32::from_rgb(30, 120, 40),
            V::String => Color32::from_rgb(170, 70, 20),
            V::Array | V::Object | V::Element => Color32::from_rgb(60, 60, 70),
            V::Comment => Color32::from_rgb(130, 130, 140),
            V::Text | V::Other => Color32::from_rgb(40, 40, 50),
        }
    }
}

/// What the tree wants done, decided while drawing and applied after, so
/// the session is not borrowed while rows are being rendered.
#[derive(Debug, Clone, PartialEq)]
pub enum TreeAction {
    Toggle(Vec<PathSeg>),
    Select(usize),
    Edit(usize),
    RenameKey(usize),
    /// Edit whatever the cursor is on now.
    EditCurrent,
    Context {
        row: usize,
        action: NodeAction,
    },
}

/// A per-node action from the context menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeAction {
    EditValue,
    EditLarge,
    CopyValue,
    Duplicate,
    Remove,
    InsertValueAfter,
    InsertObjectAfter,
    InsertArrayAfter,
}

impl NodeAction {
    pub fn label(self) -> &'static str {
        match self {
            NodeAction::EditValue => "Edit value",
            NodeAction::EditLarge => "Edit value in a window…",
            NodeAction::CopyValue => "Copy value",
            NodeAction::Duplicate => "Duplicate",
            NodeAction::Remove => "Remove",
            NodeAction::InsertValueAfter => "Value",
            NodeAction::InsertObjectAfter => "Object",
            NodeAction::InsertArrayAfter => "Array",
        }
    }
}

pub fn tree(session: &mut Session, ui: &mut egui::Ui) -> Option<TreeAction> {
    session.set_viewport_rows(PAGE_ROWS);
    // Roughly how many monospace characters fit across the pane.
    session.set_viewport_cols((ui.available_width() / 7.5) as usize);
    if session.tree_rows.is_empty() {
        ui.label("nothing to show in this view");
        return None;
    }
    let dark = ui.visuals().dark_mode;
    let cursor = session.grid.cursor.0;
    let editing = session.is_editing_inline();
    let mut action = None;

    egui::ScrollArea::both().show(ui, |ui| {
        for (i, row) in session.tree_rows.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.add_space(row.depth as f32 * 14.0);

                // The disclosure triangle, and nothing where a leaf sits,
                // so the columns still line up.
                match row.kind {
                    RowKind::Container { expanded } => {
                        if disclosure(ui, expanded).clicked() {
                            action = Some(TreeAction::Toggle(row.path.clone()));
                        }
                    }
                    RowKind::Scalar => {
                        ui.add_space(DISCLOSURE);
                    }
                }

                if row.duplicate {
                    duplicate_dot(ui).on_hover_text(
                        "This key appears more than once. Most parsers keep only \
                             the last one, so the other value is silently discarded.",
                    );
                }

                let key = RichText::new(&row.label).monospace().color(if dark {
                    Color32::from_rgb(130, 190, 240)
                } else {
                    Color32::from_rgb(20, 90, 160)
                });
                let selected = i == cursor;
                let response = ui.selectable_label(selected, key);
                if response.clicked() {
                    action = Some(TreeAction::Select(i));
                }
                // Double-clicking a key renames it; double-clicking the
                // value edits the value. Each edits what you clicked.
                if response.double_clicked() {
                    action = Some(TreeAction::RenameKey(i));
                }

                ui.label(RichText::new(":").weak().monospace());

                // The value being typed is drawn where the value is, not
                // echoed at the bottom of the window.
                if editing && selected {
                    ui.label(caret_text(session));
                    return;
                }

                let value = RichText::new(&row.summary)
                    .monospace()
                    .color(value_color(row.value, dark));
                let value_response = ui.selectable_label(selected, value);
                if value_response.clicked() {
                    action = Some(TreeAction::Select(i));
                }
                if value_response.double_clicked() {
                    action = Some(TreeAction::Edit(i));
                }

                // Right-click anywhere on the row opens the node menu.
                for r in [&response, &value_response] {
                    r.context_menu(|ui| {
                        if let Some(chosen) = node_menu(ui, row.is_container()) {
                            action = Some(TreeAction::Context {
                                row: i,
                                action: chosen,
                            });
                            ui.close();
                        }
                    });
                }
            });
        }
    });

    action
}

/// The per-node context menu.
fn node_menu(ui: &mut egui::Ui, is_container: bool) -> Option<NodeAction> {
    let mut chosen = None;
    ui.add_enabled_ui(!is_container, |ui| {
        if ui.button(NodeAction::EditValue.label()).clicked() {
            chosen = Some(NodeAction::EditValue);
        }
        if ui.button(NodeAction::EditLarge.label()).clicked() {
            chosen = Some(NodeAction::EditLarge);
        }
    });
    if ui.button(NodeAction::CopyValue.label()).clicked() {
        chosen = Some(NodeAction::CopyValue);
    }
    ui.separator();
    if ui.button(NodeAction::Duplicate.label()).clicked() {
        chosen = Some(NodeAction::Duplicate);
    }
    if ui.button(NodeAction::Remove.label()).clicked() {
        chosen = Some(NodeAction::Remove);
    }
    ui.separator();
    ui.label(RichText::new("Insert after").weak());
    for action in [
        NodeAction::InsertValueAfter,
        NodeAction::InsertObjectAfter,
        NodeAction::InsertArrayAfter,
    ] {
        if ui.button(format!("＋ {}", action.label())).clicked() {
            chosen = Some(action);
        }
    }
    chosen
}

pub fn text(session: &mut Session, ui: &mut egui::Ui) -> bool {
    session.set_viewport_rows(PAGE_ROWS);
    let (_, lines, _) = session.table_dims();
    let cursor_row = session.grid.cursor.0;
    let editing = session.is_editing_inline();
    let grammar = session.grammar();
    let dark = ui.visuals().dark_mode;
    let mut edit = false;
    let row_height = ui.text_style_height(&egui::TextStyle::Monospace);

    // A fixed gutter beside a scrolling pane, rather than the number
    // embedded in each line: with the number in the text it scrolls away
    // sideways, and a wrapped line pushes every number out of step.
    // Wide enough for the largest line number, so the gutter never
    // resizes as you scroll.
    let digits = lines.to_string().len().max(2) as f32;
    let gutter_width = digits * 9.0 + 6.0;

    ui.horizontal_top(|ui| {
        let gutter = egui::ScrollArea::vertical()
            .id_salt("text-gutter")
            .vertical_scroll_offset(session.text_scroll)
            .show_rows(ui, row_height, lines, |ui, range| {
                // Explicitly vertical: this sits inside a horizontal
                // layout, and without it every row lands on one line.
                ui.vertical(|ui| {
                    ui.set_width(gutter_width);
                    for n in range {
                        ui.label(
                            RichText::new(format!("{:>1$}", n + 1, lines.to_string().len().max(2)))
                                .monospace()
                                .weak(),
                        );
                    }
                });
            });
        let _ = gutter;

        ui.separator();

        let content = egui::ScrollArea::both().id_salt("text-content").show_rows(
            ui,
            row_height,
            lines,
            |ui, range| {
                // Explicitly vertical: this sits inside a horizontal
                // layout, and without it every line lands on one row.
                ui.vertical(|ui| {
                    // Lines extend rather than wrap, so the gutter stays
                    // in step and long lines scroll sideways instead of
                    // folding.
                    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                    for n in range {
                        if editing && n == cursor_row {
                            ui.label(caret_text(session));
                            continue;
                        }
                        let line = session.table_cell(n, 0).unwrap_or_default();
                        let response = ui
                            .selectable_label(n == cursor_row, coloured_line(&line, grammar, dark));
                        if response.clicked() {
                            session.grid.cursor = (n, 0);
                        }
                        if response.double_clicked() {
                            session.grid.cursor = (n, 0);
                            edit = true;
                        }
                    }
                });
            },
        );
        // Feed the content's position back to the gutter. A frame behind,
        // which is imperceptible, and far simpler than linking them.
        session.text_scroll = content.state.offset.y;
    });

    edit
}

/// One line, coloured by grammar.
fn coloured_line(line: &str, grammar: vuwr_core::Grammar, dark: bool) -> egui::text::LayoutJob {
    use egui::text::{LayoutJob, TextFormat};
    let font = egui::FontId::monospace(12.0);
    let mut job = LayoutJob::default();
    job.wrap.max_width = f32::INFINITY;

    let spans = vuwr_core::highlight(line, grammar);
    if spans.is_empty() {
        job.append(
            line,
            0.0,
            TextFormat::simple(font, token_color(vuwr_core::Token::Plain, dark)),
        );
        return job;
    }
    let mut at = 0usize;
    for span in spans {
        if span.start > at {
            job.append(
                &line[at..span.start],
                0.0,
                TextFormat::simple(font.clone(), token_color(vuwr_core::Token::Plain, dark)),
            );
        }
        job.append(
            &line[span.start..span.end],
            0.0,
            TextFormat::simple(font.clone(), token_color(span.token, dark)),
        );
        at = span.end;
    }
    if at < line.len() {
        job.append(
            &line[at..],
            0.0,
            TextFormat::simple(font, token_color(vuwr_core::Token::Plain, dark)),
        );
    }
    job
}

fn token_color(token: vuwr_core::Token, dark: bool) -> Color32 {
    use vuwr_core::Token as T;
    if dark {
        match token {
            T::Key => Color32::from_rgb(130, 190, 240),
            T::Str => Color32::from_rgb(230, 160, 120),
            T::Number => Color32::from_rgb(150, 210, 130),
            T::Keyword => Color32::from_rgb(190, 150, 240),
            T::Tag => Color32::from_rgb(130, 190, 240),
            T::Comment => Color32::from_rgb(120, 120, 130),
            T::Escape => Color32::from_rgb(220, 200, 120),
            T::Punctuation => Color32::from_rgb(150, 150, 160),
            T::Plain => Color32::from_rgb(210, 210, 215),
        }
    } else {
        match token {
            T::Key => Color32::from_rgb(20, 90, 160),
            T::Str => Color32::from_rgb(170, 70, 20),
            T::Number => Color32::from_rgb(30, 120, 40),
            T::Keyword => Color32::from_rgb(110, 50, 170),
            T::Tag => Color32::from_rgb(20, 90, 160),
            T::Comment => Color32::from_rgb(130, 130, 140),
            T::Escape => Color32::from_rgb(150, 110, 10),
            T::Punctuation => Color32::from_rgb(110, 110, 120),
            T::Plain => Color32::from_rgb(40, 40, 50),
        }
    }
}

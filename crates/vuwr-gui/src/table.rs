//! The three views, drawn with egui.
//!
//! Every value shown comes from [`Session`], and every row index goes
//! through `grid.source_row` — the same rule the TUI follows, for the same
//! reason: with a filter applied, display row 3 is not source row 3.

use eframe::egui::{self, Color32, RichText};
use vuwr_core::{PathSeg, RowKind, Session, ValueKind};

use crate::theme;

/// The vertical offset that brings a row into view, or `None` to leave
/// the scroll position alone.
///
/// For the views that draw only what is on screen: the row to reach is
/// usually not among them, so there is no rectangle to hand egui and the
/// position has to be worked out. That only holds while the drawn pitch
/// really is `row_height` — see the callers, which zero the spacing that
/// would otherwise make it taller.
fn follow_offset(cursor: (usize, usize), row_height: f32, height: f32, at: f32) -> Option<f32> {
    let top = cursor.0 as f32 * row_height;
    if top < at {
        Some(top)
    } else if top + row_height > at + height {
        // A little above the bottom edge, so the row it lands on has its
        // neighbours around it rather than being the last line.
        Some((top + row_height * 3.0 - height).max(0.0))
    } else {
        None
    }
}

/// Remember where a view was scrolled to, for the next frame's decision.
fn remember_offset(ui: &egui::Ui, key: &'static str, offset: f32) {
    ui.ctx()
        .data_mut(|d| d.insert_temp(egui::Id::new(key).with("offset-y"), offset));
}

fn seen_offset(ui: &egui::Ui, key: &'static str) -> f32 {
    ui.ctx()
        .data(|d| d.get_temp::<f32>(egui::Id::new(key).with("offset-y")))
        .unwrap_or(0.0)
}

/// Whether the cursor has moved since the last frame.
///
/// What "follow the cursor" hangs on: scrolling on every frame would
/// fight the scrollbar for the rest of the time. All three views ask,
/// because all three had the same job and only the table did it —
/// pressing `n` moved the cursor to the next match in the tree and in the
/// text without the screen following, so the match was found and not
/// shown.
fn cursor_moved(ui: &egui::Ui, key: &'static str, cursor: (usize, usize)) -> bool {
    ui.ctx().memory_mut(|m| {
        let id = egui::Id::new(key).with("cursor");
        let seen = m.data.get_temp::<(usize, usize)>(id);
        m.data.insert_temp(id, cursor);
        seen != Some(cursor)
    })
}

/// What the modifier is called here, for a tooltip that tells the truth
/// on both platforms.
pub(crate) const fn cmd_key() -> &'static str {
    if cfg!(target_os = "macos") {
        "Cmd"
    } else {
        "Ctrl"
    }
}

/// Rows drawn per screen. egui scrolls the whole grid, so this only sets
/// what a page-down means.
const PAGE_ROWS: usize = 25;

pub fn table(session: &mut Session, ui: &mut egui::Ui) -> bool {
    let mut edit = false;
    let (headers, rows, cols) = session.table_dims();
    if rows == 0 || cols == 0 {
        ui.label("nothing to show in this view");
        return false;
    }

    let cursor = session.grid.cursor;
    let separate_header = session.has_separate_header();
    let frozen = session.grid.frozen_cols.min(cols);
    let font = egui::FontId::monospace(12.0);
    // Monospace, so one glyph's advance is every glyph's. Measured by
    // laying out a digit, since the font cache is read-only here.
    let char_width = ui
        .painter()
        .layout_no_wrap("0".to_owned(), font.clone(), Color32::PLACEHOLDER)
        .size()
        .x
        .max(6.0);
    // Fixed, and not derived from the font: virtualisation has to know a
    // row's height without laying it out.
    let row_height = theme::ROW_HEIGHT;
    let search = session.search.clone();
    let editing = session.is_editing_inline();
    let widths: Vec<usize> = (0..cols).map(|c| column_chars(session, c)).collect();
    // Numbers read down the column, not across the row, so they are set
    // against the right edge. Nothing is coerced — this is only where the
    // glyphs sit.
    let numeric: Vec<bool> = (0..cols).map(|c| session.column_is_numeric(c)).collect();

    // Room for the row numbers, which is however many digits the last row
    // needs. A feed's "row 247" means nothing without them on screen to
    // count against.
    let gutter = GUTTER + digits(rows) as f32 * char_width + PAD;

    // The header sits outside the scroll area so it stays put while the
    // rows move under it — but it has to follow the *horizontal* scroll,
    // or scrolling right leaves every heading over the wrong column. The
    // offset is a frame behind, as with the text view's gutter, which is
    // imperceptible and far simpler than linking the two.
    let offset_id = egui::Id::new("sheet-offset-x");
    let offset_x = ui
        .ctx()
        .data(|d| d.get_temp::<f32>(offset_id).unwrap_or(0.0));
    let mut grip = Grip::None;
    {
        // CSV carries its header as its first row, so there is no heading
        // strip to grab — which left those columns with no way to resize
        // but the keyboard. The strip is drawn either way: with the names
        // in it when they are separate, and as a bare ruler of handles
        // when they are not.
        let head_height = if separate_header {
            theme::HEADER_HEIGHT
        } else {
            RULER
        };
        let outer = ui.available_rect_before_wrap();
        // Inset by the gutter, as the rows are: the headings have to stay
        // over their own columns.
        let header_rect = egui::Rect::from_min_size(
            outer.min + egui::vec2(gutter, 0.0),
            egui::vec2(outer.width() - gutter, head_height),
        );
        let mut header = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(egui::Rect::from_min_size(
                    header_rect.min - egui::vec2(offset_x, 0.0),
                    egui::vec2(header_rect.width() + offset_x, head_height),
                ))
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        ui.painter()
            .rect_filled(header_rect, 0.0, theme::surface_header());
        ui.painter().hline(
            header_rect.x_range(),
            header_rect.bottom() - 0.5,
            egui::Stroke::new(1.0_f32, theme::border()),
        );
        header.set_clip_rect(header_rect.intersect(ui.clip_rect()));
        header.spacing_mut().item_spacing.x = 0.0;
        let head_colour = theme::text_muted();
        let head_font = header
            .style()
            .text_styles
            .get(&theme::micro())
            .cloned()
            .unwrap_or_else(|| font.clone());
        let empty = String::new();
        for (c, chars) in widths.iter().enumerate().take(cols) {
            // Indexed by column, not by the header's own length — which
            // is what the width was being taken from, so no column lined
            // up with its heading.
            let name = if separate_header {
                headers.get(c).unwrap_or(&empty)
            } else {
                &empty
            };
            cell(
                &mut header,
                name,
                *chars as f32 * char_width,
                head_height,
                &head_font,
                head_colour,
                false,
            );
            match resize_grip(&mut header, c, *chars, char_width, head_height) {
                Grip::None => {}
                other => grip = other,
            }
        }
        // Reserve the space without putting a widget over it: an
        // allocation here is hit-tested after the handles and swallows
        // every hover and drag they were drawn for.
        ui.advance_cursor_after_rect(header_rect.translate(egui::vec2(-gutter, 0.0)));
        ui.separator();
    }

    // Only the rows on screen are drawn: building all of them every frame
    // is what made a feed feel like a hang.
    let visible = ((ui.available_height() / row_height) as usize).max(1);
    session.set_viewport_rows(visible);

    // Filling the pane matters for more than looks: a shrunk-to-fit
    // scroll area puts its scrollbars against the *content*, so on a wide
    // window they sit somewhere in the middle of the screen — or, with
    // content narrower than the pane, appear to be missing.
    ui.style_mut().spacing.scroll.floating = false;
    // Follow the cursor sideways. A column can be off the right-hand edge
    // — a feed is twenty-three wide — and jumping to one without bringing
    // it into view leaves the status line saying "editing" over a screen
    // that has not moved. Only when the cursor has just moved, so it does
    // not fight the scrollbar the rest of the time.
    let follow = ui.ctx().memory_mut(|m| {
        let seen = m
            .data
            .get_temp::<(usize, usize)>(egui::Id::new("sheet-cursor"));
        m.data.insert_temp(egui::Id::new("sheet-cursor"), cursor);
        seen != Some(cursor)
    });
    let mut area = egui::ScrollArea::both()
        .id_salt("sheet")
        .auto_shrink([false; 2]);
    if follow {
        // And downwards, which nothing did: "Show me" on a diagnostic in
        // row 247 moved the cursor there and left the screen on row 1, so
        // it looked as though the jump had failed.
        let top = cursor.0 as f32 * row_height;
        let down = ui
            .ctx()
            .memory(|m| m.data.get_temp::<f32>(egui::Id::new("sheet-offset-y")))
            .unwrap_or(0.0);
        let height = ui.available_height();
        if top < down {
            area = area.vertical_scroll_offset(top);
        } else if top + row_height > down + height {
            // A little above the bottom edge, so the row it lands on has
            // its neighbours around it rather than being the last line.
            area = area.vertical_scroll_offset(top + row_height * 3.0 - height);
        }
        let pane = ui.available_width() - gutter;
        let mut left = 0.0;
        for chars in widths.iter().take(cursor.1) {
            left += *chars as f32 * char_width + GRIP + PAD * 2.0;
        }
        let width = widths
            .get(cursor.1)
            .map_or(0.0, |c| *c as f32 * char_width + GRIP + PAD * 2.0);
        let at = ui
            .ctx()
            .memory(|m| m.data.get_temp::<f32>(egui::Id::new("sheet-offset-x")))
            .unwrap_or(0.0);
        // Only when it is actually out of view: scrolling a column that
        // is already on screen jerks the table for no reason.
        if left < at {
            area = area.horizontal_scroll_offset(left);
        } else if left + width > at + pane {
            area = area.horizontal_scroll_offset(left + width - pane);
        }
    }
    // The gutter is reserved here and painted at the end, beside the pane
    // rather than inside it: a gutter that scrolls with the content is
    // gone the moment you scroll right, which is exactly when knowing
    // which row you are on matters. Same shape as the text view's.
    let outer = ui.available_rect_before_wrap();
    let gutter_rect = egui::Rect::from_min_size(outer.min, egui::vec2(gutter, outer.height()));
    let mut pane_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(egui::Rect::from_min_max(
                outer.min + egui::vec2(gutter, 0.0),
                outer.max,
            ))
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    // `show_rows` virtualises at `row_height + item_spacing.y`, taken from
    // *this* ui — the one the scroll area is built on. Zeroing the spacing
    // inside the closure was too late: egui had already decided each row
    // was four pixels taller than the rows actually drew, so the range it
    // handed back belonged to a different part of the file the further
    // down you scrolled, and it reserved more height than it filled.
    pane_ui.spacing_mut().item_spacing.y = 0.0;
    let ui = &mut pane_ui;
    // The rows report where they were actually drawn, and the gutter is
    // painted from that rather than from the scroll offset. Deriving the
    // positions a second time meant two pieces of arithmetic that had to
    // agree and did not: the offset read back after the frame is the one
    // the *next* frame will use, so on any frame that scrolled — which is
    // every frame "Show me" acts on — the numbers were drawn against a
    // screenful the pane was no longer showing.
    let scrolled = area.show_rows(ui, row_height, rows, |ui, range| {
        let mut drawn: Vec<(usize, f32)> = Vec::new();
        ui.vertical(|ui| {
            // Exactly `row_height` apart, which is what `show_rows` has
            // already assumed in deciding which rows to hand us — and what
            // the gutter beside it counts in. Default spacing here put a
            // few pixels between rows, so the further down the pane you
            // looked the further the numbers were from their rows.
            ui.spacing_mut().item_spacing.y = 0.0;
            for r in range {
                let source = session.grid.source_row(r);
                let marked = session.grid.marks.contains(&source);
                let on_row = r == cursor.0;
                ui.horizontal(|ui| {
                    let strip = egui::Rect::from_min_size(
                        ui.cursor().min,
                        egui::vec2(ui.available_width(), row_height),
                    );
                    drawn.push((r, strip.top()));
                    if on_row {
                        ui.painter().rect_filled(strip, 0.0, theme::row_selected());
                    }
                    // The marker for this row is drawn in the gutter,
                    // which does not scroll away from it.
                    let _ = marked;
                    ui.painter().hline(
                        strip.x_range(),
                        strip.bottom() - 0.5,
                        egui::Stroke::new(1.0_f32, theme::border_faint()),
                    );
                    ui.spacing_mut().item_spacing.x = 0.0;
                    for (c, chars) in widths.iter().enumerate() {
                        // Editing happens in the cell, drawn as a
                        // field, so it is obvious which value you are
                        // typing into.
                        if editing && (r, c) == cursor {
                            let response = edit_field(
                                ui,
                                session,
                                *chars as f32 * char_width + GRIP + PAD * 2.0,
                                row_height,
                            );
                            let left = response.rect.left() + PAD;
                            place_caret(session, &response, ui, left);
                            continue;
                        }
                        let raw = session.table_cell(r, c).unwrap_or_default();
                        let link = session
                            .links_clickable
                            .then(|| vuwr_core::as_link(&raw).map(str::to_string))
                            .flatten();
                        // Cut to what the column can show: a
                        // description is thousands of characters, and
                        // laying the rest out costs time to draw
                        // nothing.
                        let text = truncate(&raw, chars + 1);
                        let mut colour = theme::text_body();
                        // The first column is an identifier: the
                        // accent marks it, as it marks a path.
                        if c == 0 || c < frozen {
                            colour = theme::accent_text();
                        }
                        if search.as_ref().is_some_and(|s| s.matches(&raw)) {
                            colour = theme::warn_text();
                        }
                        if link.is_some() {
                            colour = theme::accent_text();
                        }
                        let response = cell_aligned(
                            ui,
                            &text,
                            *chars as f32 * char_width + GRIP,
                            row_height,
                            &font,
                            colour,
                            (r, c) == cursor,
                            numeric[c],
                        );
                        // A plain click selects the cell, as it always
                        // has; the modifier follows the link. The same
                        // gesture an editor uses, and the only one that
                        // does not take selecting a cell away.
                        if let Some(url) = &link {
                            let response = response
                                .on_hover_text(format!("{}-click to open {url}", cmd_key()));
                            if response.clicked() && ui.input(|i| i.modifiers.command) {
                                ui.ctx().open_url(egui::OpenUrl::new_tab(url));
                            } else if response.clicked() {
                                session.grid.cursor = (r, c);
                            }
                            if response.double_clicked() {
                                session.grid.cursor = (r, c);
                                edit = true;
                            }
                            continue;
                        }
                        if response.clicked() {
                            session.grid.cursor = (r, c);
                        }
                        if response.double_clicked() {
                            session.grid.cursor = (r, c);
                            edit = true;
                        }
                    }
                });
            }
        });
        drawn
    });
    let offset_y = scrolled.state.offset.y;
    ui.ctx().data_mut(|d| {
        d.insert_temp(offset_id, scrolled.state.offset.x);
        d.insert_temp(egui::Id::new("sheet-offset-y"), offset_y);
    });

    // The row numbers, last, so nothing can be drawn over them.
    //
    // They are the document's own rows — the same numbers the status line
    // and a diagnostic use — so "row 247" can be found by looking rather
    // than by counting.
    let painter = ui.painter_at(gutter_rect);
    painter.rect_filled(gutter_rect, 0.0, theme::surface_header());
    painter.vline(
        gutter_rect.right() - 0.5,
        gutter_rect.y_range(),
        egui::Stroke::new(1.0_f32, theme::border()),
    );
    let number_font = egui::FontId::monospace(11.0);
    for (r, top) in scrolled.inner {
        let (r, top) = (r, top);
        if top + row_height < gutter_rect.top() || top > gutter_rect.bottom() {
            continue;
        }
        let source = session.grid.source_row(r);
        let on_row = r == cursor.0;
        let marked = session.grid.marks.contains(&source);
        if on_row {
            painter.rect_filled(
                egui::Rect::from_min_size(
                    egui::pos2(gutter_rect.left(), top),
                    egui::vec2(gutter_rect.width(), row_height),
                ),
                0.0,
                theme::row_selected(),
            );
        }
        // Accent for where you are, amber for a row you flagged.
        if let Some(colour) = if on_row {
            Some(theme::accent())
        } else if marked {
            Some(theme::warn())
        } else {
            None
        } {
            painter.rect_filled(
                egui::Rect::from_min_size(
                    egui::pos2(gutter_rect.left(), top),
                    egui::vec2(theme::ROW_MARKER, row_height),
                ),
                0.0,
                colour,
            );
        }
        painter.text(
            egui::pos2(gutter_rect.right() - PAD, top + row_height / 2.0),
            egui::Align2::RIGHT_CENTER,
            (source + 1).to_string(),
            number_font.clone(),
            if on_row {
                theme::accent_text()
            } else {
                theme::text_muted()
            },
        );
    }

    match grip {
        Grip::None => {}
        Grip::Width(col, chars) => session.set_column_width(col, chars),
        Grip::AutoSize(col) => session.auto_size_column(col),
    }
    edit
}

/// Width of the mark stripe at the left of the row-number gutter.
const GUTTER: f32 = 14.0;

/// How many digits a row count needs, with a floor so the gutter does not
/// visibly change width on the first scroll of a short file.
fn digits(rows: usize) -> usize {
    rows.max(1).to_string().len().max(3)
}

/// How many characters wide a column is, from what the session measured.
fn column_chars(session: &Session, col: usize) -> usize {
    session
        .widths()
        .get(col)
        .copied()
        .unwrap_or(12)
        .clamp(3, Session::MAX_COLUMN)
}

/// The id of a column's resize handle.
pub fn grip_id(col: usize) -> egui::Id {
    egui::Id::new(("vuwr-column-grip", col))
}

/// Width of the strip you grab to resize a column.
const GRIP: f32 = 5.0;

/// Height of the bare handle strip drawn for a sheet whose header is its
/// own first row.
const RULER: f32 = 9.0;

/// The draggable boundary on a column's right edge.
///
/// Returns the new width in characters when it moves. Dragging is the
/// obvious gesture for this, but it is only ever a shortcut for
/// `set_column_width`, which the `<`/`>` keys and the palette also reach —
/// so a resize means the same thing however it was asked for.
fn resize_grip(ui: &mut egui::Ui, col: usize, chars: usize, char_width: f32, height: f32) -> Grip {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(GRIP, height), egui::Sense::hover());
    // A stable id, so the handle keeps its drag across frames and a test
    // can find where it ended up.
    let response = ui.interact(rect, grip_id(col), egui::Sense::click_and_drag());
    // Always drawn, so you can see there is something to grab — an
    // invisible handle is one nobody finds.
    let live = response.hovered() || response.dragged();
    if live {
        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
    }
    let (colour, thickness) = if live {
        (ui.visuals().selection.bg_fill, 2.0)
    } else {
        (ui.visuals().widgets.noninteractive.bg_stroke.color, 1.0)
    };
    ui.painter().rect_filled(
        egui::Rect::from_min_size(
            egui::pos2(rect.center().x - thickness / 2.0, rect.top()),
            egui::vec2(thickness, rect.height()),
        ),
        0.0,
        colour,
    );
    // Double-click on the boundary fits the column to its contents, the
    // way a spreadsheet does.
    if response.double_clicked() {
        return Grip::AutoSize(col);
    }
    if response.dragged() {
        let delta = response.drag_delta().x / char_width;
        let wanted = (chars as f32 + delta).round().max(3.0) as usize;
        if wanted != chars {
            return Grip::Width(col, wanted);
        }
    }
    Grip::None
}

/// What a drag on a column boundary asked for.
enum Grip {
    None,
    Width(usize, usize),
    AutoSize(usize),
}

/// One cell: fixed width, text left-aligned and clipped to it.
///
/// Drawn rather than built from a label so the text starts at the left
/// edge and cannot spill into the next column — a centred cell made the
/// columns look ragged, and an unclipped one ran over its neighbour.
fn cell(
    ui: &mut egui::Ui,
    text: &str,
    width: f32,
    height: f32,
    font: &egui::FontId,
    colour: Color32,
    selected: bool,
) -> egui::Response {
    cell_aligned(ui, text, width, height, font, colour, selected, false)
}

/// The same, with the text set against the right edge when the column
/// holds numbers.
#[allow(clippy::too_many_arguments)]
fn cell_aligned(
    ui: &mut egui::Ui,
    text: &str,
    width: f32,
    height: f32,
    font: &egui::FontId,
    colour: Color32,
    selected: bool,
    right: bool,
) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(width + PAD * 2.0, height), egui::Sense::click());
    // The row already carries the selection; the cell adds only a
    // slightly stronger tint, so the cursor reads without the row turning
    // into a block of colour.
    if selected {
        ui.painter().rect_filled(rect, 3.0, theme::accent_tint());
    }
    let colour = if selected {
        theme::accent_text()
    } else {
        colour
    };
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_owned(), font.clone(), colour);
    let y = rect.center().y - galley.size().y / 2.0;
    let x = if right {
        (rect.right() - PAD - galley.size().x).max(rect.left() + PAD)
    } else {
        rect.left() + PAD
    };
    ui.painter()
        .with_clip_rect(rect.intersect(ui.clip_rect()))
        .galley(egui::pos2(x, y), galley, colour);
    response
}

/// Whitespace either side of a cell's text, so columns are not jammed
/// against each other. The design's 10px cell padding.
const PAD: f32 = theme::CELL_PAD_X;

/// The cell being typed into, drawn as an input field.
///
/// A plain highlight left it looking like any other selected cell, so it
/// was not obvious the keyboard was going somewhere. This gives it a
/// filled background and a focus ring, which is what a field looks like.
fn edit_field(ui: &mut egui::Ui, session: &Session, width: f32, height: f32) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::click());
    ui.painter().rect_filled(rect, 5.0, theme::surface());
    // Two pixels of the edit colour: a cell being typed into is not the
    // same thing as a cell that is merely selected, and blue would say it
    // was.
    ui.painter().rect_stroke(
        rect,
        5.0,
        egui::Stroke::new(2.0_f32, theme::edit_ring()),
        egui::StrokeKind::Inside,
    );
    let job = caret_text(session);
    let galley = ui.painter().layout_job(job);
    let y = rect.center().y - galley.size().y / 2.0;
    let clip = rect.intersect(ui.clip_rect());
    ui.painter().with_clip_rect(clip).galley(
        egui::pos2(rect.left() + PAD, y),
        galley,
        theme::text_body(),
    );
    draw_caret(
        ui,
        rect.left() + PAD + caret_offset(ui, session),
        rect.top(),
        rect.height(),
    );
    response
}

/// Cut a value down to what a cell can show, marking that there is more.
fn truncate(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_string();
    }
    text.chars().take(limit).collect::<String>() + "…"
}

/// Move the caret to where the text being edited was clicked.
///
/// The buffer is drawn by hand rather than by a `TextEdit`, so nothing
/// else would place the caret: a typo halfway along a line meant deleting
/// everything after it and typing it again.
pub fn place_caret(
    session: &mut Session,
    response: &egui::Response,
    ui: &egui::Ui,
    text_left: f32,
) {
    // Three clicks takes the value, two takes a word, one places the
    // caret — the same as any other field, so nobody has to be told.
    if response.triple_clicked() {
        session.select_all();
        return;
    }
    let word = response.double_clicked();
    if !response.clicked() && !response.drag_started() && !word && !response.dragged() {
        return;
    }
    let Some(pos) = response.interact_pointer_pos() else {
        return;
    };
    let Some((_, buf)) = session.entry() else {
        return;
    };
    let font = egui::FontId::monospace(13.0);
    let advance = ui
        .painter()
        .layout_no_wrap("0".to_owned(), font, Color32::PLACEHOLDER)
        .size()
        .x
        .max(1.0);
    let column = ((pos.x - text_left) / advance).round().max(0.0) as usize;
    // Monospace, so the column is a character index; the byte offset it
    // sits at is what the buffer is addressed by.
    let byte = buf.char_indices().nth(column).map_or(buf.len(), |(i, _)| i);
    if word {
        session.select_word_at(byte);
    } else if response.dragged() {
        // Dragging from where the press landed extends rather than
        // replaces: that is what dragging across text means.
        session.extend_entry_selection(byte);
    } else {
        session.set_entry_caret(byte);
    }
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
    let font = egui::FontId::monospace(12.0);
    let plain = TextFormat::simple(font.clone(), theme::text_body());
    let selected = TextFormat {
        font_id: font.clone(),
        color: theme::accent_text(),
        background: theme::accent_tint(),
        ..Default::default()
    };

    // A block over the next character reads as "this letter is selected",
    // which is what it means everywhere else. The caret is a rule between
    // characters; a selection is the thing with a background.
    match session.entry_selection() {
        Some((a, b)) => {
            job.append(&buf[..a], 0.0, plain.clone());
            job.append(&buf[a..b], 0.0, selected);
            job.append(&buf[b..], 0.0, plain);
        }
        None => {
            job.append(&buf[..caret], 0.0, plain.clone());
            job.append(&buf[caret..], 0.0, plain);
        }
    }
    job
}

/// Where the caret sits within a laid-out buffer, as an offset from the
/// text's left edge — so it can be drawn as a rule rather than as a box
/// over a character.
pub fn caret_offset(ui: &egui::Ui, session: &Session) -> f32 {
    let Some((_, buf)) = session.entry() else {
        return 0.0;
    };
    let caret = session.entry_caret().min(buf.len());
    ui.painter()
        .layout_no_wrap(
            buf[..caret].to_owned(),
            egui::FontId::monospace(12.0),
            egui::Color32::PLACEHOLDER,
        )
        .size()
        .x
}

/// Draw the caret as a thin rule at `x`, the height of a line.
pub fn draw_caret(ui: &egui::Ui, x: f32, top: f32, height: f32) {
    ui.painter().rect_filled(
        egui::Rect::from_min_size(egui::pos2(x, top + 2.0), egui::vec2(1.5, height - 4.0)),
        0.0,
        theme::accent_text(),
    );
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

/// Width of the disclosure control, so leaves line up with containers.
const DISCLOSURE: f32 = 14.0;

/// Height of a tree row.
const TREE_ROW: f32 = 26.0;

/// The width of one monospace character in the given font.
fn char_px(ui: &egui::Ui, font: &egui::FontId) -> f32 {
    ui.painter()
        .layout_no_wrap("0".to_owned(), font.clone(), egui::Color32::PLACEHOLDER)
        .size()
        .x
        .max(1.0)
}

/// Roughly how wide the widest line is, so the scroll area knows how far
/// sideways there is to go. Measured in characters against the longest
/// line rather than by laying every line out, which at 84,000 lines is
/// the difference between a frame and a freeze.
fn longest_line(session: &Session, _lines: usize) -> f32 {
    // From the session, which worked it out when it built the lines. This
    // used to clone and measure two thousand strings on every frame — six
    // hundred milliseconds a frame on a feed, spent deciding how wide a
    // scrollbar should be.
    session.widest_line() as f32 * 7.2
}

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
pub fn value_color(kind: ValueKind) -> Color32 {
    theme::value(kind)
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
            Self::EditValue => "Edit value",
            Self::EditLarge => "Edit value in a window…",
            Self::CopyValue => "Copy value",
            Self::Duplicate => "Duplicate",
            Self::Remove => "Remove",
            Self::InsertValueAfter => "Value",
            Self::InsertObjectAfter => "Object",
            Self::InsertArrayAfter => "Array",
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
    let cursor = session.grid.cursor.0;
    let editing = session.is_editing_inline();
    let mut action = None;

    let follow = cursor_moved(ui, "tree", session.grid.cursor);
    let links_clickable = session.links_clickable;
    let rows = session.tree_rows.len();

    // Only the rows on screen, as the table and the text view already
    // did. Expanded, a product feed is forty thousand rows, and laying
    // every one of them out on every frame is what made scrolling crawl.
    //
    // `show_rows` needs the drawn pitch to match the height it is given,
    // so the spacing between rows is zeroed here — on the ui the scroll
    // area is built on, which is where egui reads it from.
    ui.spacing_mut().item_spacing.y = 0.0;
    let mut area = egui::ScrollArea::both()
        .id_salt("tree")
        .auto_shrink([false; 2]);
    // Follow the cursor, so `n` shows the match it found rather than only
    // selecting it. Arithmetic rather than the row's own rectangle,
    // because the row it has to reach is usually one of the many not
    // drawn — which is the whole point of drawing only what is on screen.
    if let Some(offset) = follow
        .then(|| {
            follow_offset(
                session.grid.cursor,
                TREE_ROW,
                ui.available_height(),
                seen_offset(ui, "tree"),
            )
        })
        .flatten()
    {
        area = area.vertical_scroll_offset(offset);
    }
    let scrolled = area.show_rows(ui, TREE_ROW, rows, |ui, range| {
        ui.spacing_mut().item_spacing.y = 0.0;
        for i in range {
            let row = &session.tree_rows[i];
            ui.horizontal(|ui| {
                let selected = i == cursor;
                // The row carries the selection, so the key and the
                // value can be plain text. As pills they read as two
                // controls sitting on a row rather than as a value
                // with a name.
                let strip = egui::Rect::from_min_size(
                    ui.cursor().min,
                    egui::vec2(ui.available_width(), TREE_ROW),
                );
                if selected {
                    if follow {
                        ui.scroll_to_rect(strip, Some(egui::Align::Center));
                    }
                    ui.painter().rect_filled(strip, 0.0, theme::row_selected());
                    ui.painter().rect_filled(
                        egui::Rect::from_min_size(
                            strip.min,
                            egui::vec2(theme::ROW_MARKER, TREE_ROW),
                        ),
                        0.0,
                        theme::accent(),
                    );
                }
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

                let key = RichText::new(&row.label)
                    .monospace()
                    .color(theme::accent_text());
                let response = ui.add(egui::Label::new(key).sense(egui::Sense::click()));
                if response.clicked() {
                    action = Some(TreeAction::Select(i));
                }
                // Double-clicking a key renames it; double-clicking the
                // value edits the value. Each edits what you clicked.
                if response.double_clicked() {
                    action = Some(TreeAction::RenameKey(i));
                }

                ui.label(RichText::new(":").monospace().color(theme::text_disabled()));

                // The value being typed is drawn where the value is, not
                // echoed at the bottom of the window.
                if editing && selected {
                    ui.label(caret_text(session));
                    return;
                }

                // A container's summary says there is more inside; a
                // leaf's is the value itself.
                let link = (!row.is_container() && links_clickable)
                    .then(|| vuwr_core::as_link(&row.summary))
                    .flatten();
                let colour = if row.is_container() {
                    theme::placeholder()
                } else if link.is_some() {
                    theme::accent_text()
                } else {
                    value_color(row.value)
                };
                let value = RichText::new(&row.summary).monospace().color(colour);
                let mut value_response =
                    ui.add(egui::Label::new(value).sense(egui::Sense::click()));
                if let Some(url) = link {
                    value_response =
                        value_response.on_hover_text(format!("{}-click to open {url}", cmd_key()));
                }
                if value_response.clicked() {
                    // The modifier follows the link; a plain click selects
                    // the row, as everywhere else.
                    if let Some(url) = link.filter(|_| ui.input(|i| i.modifiers.command)) {
                        ui.ctx().open_url(egui::OpenUrl::new_tab(url));
                    } else {
                        action = Some(TreeAction::Select(i));
                    }
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
    remember_offset(ui, "tree", scrolled.state.offset.y);

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
    // The value the cursor sits inside, so a description reads as the one
    // thing it is rather than as twenty unrelated lines.
    let block = session.value_block();
    let (_, lines, _) = session.table_dims();
    let cursor_row = session.grid.cursor.0;
    let editing = session.is_editing_inline();
    let grammar = session.grammar();
    let mut edit = false;
    let row_height = ui.text_style_height(&egui::TextStyle::Monospace);
    let follow = cursor_moved(ui, "text", session.grid.cursor);

    // A fixed gutter beside a scrolling pane, rather than the number
    // embedded in each line: with the number in the text it scrolls away
    // sideways, and a wrapped line pushes every number out of step.
    // Wide enough for the largest line number, so the gutter never
    // resizes as you scroll.
    let digits = lines.to_string().len().max(2) as f32;
    let gutter_width = (digits * 8.0 + 24.0).max(56.0);

    // One scroll area, not two. A gutter that scrolls itself has to be
    // kept in step with the text beside it, and any disagreement about how
    // tall a row is shows up as numbers drifting from their lines or as a
    // band of nothing at the end of the file. Here there is one list, one
    // row height, and the numbers are painted against the viewport's left
    // edge so they stay put while the text scrolls sideways.
    let scrolled = egui::ScrollArea::both()
        .id_salt("text")
        .auto_shrink([false; 2])
        .show_viewport(ui, |ui, viewport| {
            let width = ui
                .available_width()
                .max(longest_line(session, lines) + gutter_width);
            let (content, _) = ui.allocate_exact_size(
                egui::vec2(width, lines as f32 * row_height),
                egui::Sense::hover(),
            );

            // Follow the cursor, so `n` shows the line it found. This view
            // places its own rows at an exact height, so the rectangle is
            // known without drawing it — the row need not be on screen to
            // be scrolled to.
            if follow {
                let top = content.top() + cursor_row as f32 * row_height;
                ui.scroll_to_rect(
                    egui::Rect::from_min_size(
                        egui::pos2(content.left(), top),
                        egui::vec2(1.0, row_height),
                    ),
                    Some(egui::Align::Center),
                );
            }
            let first = (viewport.min.y / row_height).floor().max(0.0) as usize;
            let last = ((viewport.max.y / row_height).ceil() as usize + 1).min(lines);
            let font = egui::TextStyle::Monospace.resolve(ui.style());
            let numbers = ui
                .style()
                .text_styles
                .get(&theme::micro())
                .cloned()
                .unwrap_or_else(|| font.clone());

            for n in first..last {
                let top = content.top() + n as f32 * row_height;
                let inside = block.is_some_and(|(a, b)| (a..=b).contains(&n));
                let row = egui::Rect::from_min_size(
                    egui::pos2(content.left() + gutter_width, top),
                    egui::vec2(width - gutter_width, row_height),
                );

                if inside {
                    ui.painter()
                        .rect_filled(row, 0.0, theme::accent_tint().gamma_multiply(0.6));
                }
                if n == cursor_row {
                    ui.painter().rect_filled(row, 0.0, theme::row_selected());
                }

                let response = ui.interact(row, ui.id().with(("line", n)), egui::Sense::click());
                if response.clicked() {
                    session.grid.cursor = (n, 0);
                }
                if response.double_clicked() {
                    session.grid.cursor = (n, 0);
                    edit = true;
                }

                // A CDATA section keeps its own newlines, so its later
                // lines start at column zero however deep the element is.
                // They are drawn shifted under the tag they belong to — a
                // rendering offset only; the file's bytes are untouched.
                //
                // For every line, not only the one the cursor is in. It
                // used to depend on the cursor, so a description slid
                // sideways as the cursor passed over it.
                let indent = session.line_indent(n) as f32 * char_px(ui, &font);

                if editing && n == cursor_row {
                    let galley = ui.painter().layout_job(caret_text(session));
                    let at = egui::pos2(row.left() + indent, top);
                    let response =
                        ui.interact(row, ui.id().with(("edit", n)), egui::Sense::click());
                    let left = at.x;
                    ui.painter().galley(at, galley, theme::text_body());
                    draw_caret(ui, left + caret_offset(ui, session), top, row_height);
                    place_caret(session, &response, ui, left);
                    continue;
                }

                let line = session.table_cell(n, 0).unwrap_or_default();
                let galley = ui
                    .painter()
                    .layout_job(coloured_line(&line, grammar, font.clone()));
                ui.painter().galley(
                    egui::pos2(row.left() + indent, top),
                    galley,
                    theme::text_body(),
                );
            }

            // The gutter last, so the text cannot run under it, and
            // against the viewport rather than the content so it stays
            // where it is while the file scrolls sideways.
            let strip = egui::Rect::from_min_size(
                egui::pos2(
                    content.left() + viewport.min.x,
                    content.top() + viewport.min.y,
                ),
                egui::vec2(gutter_width, viewport.height()),
            );
            ui.painter()
                .rect_filled(strip, 0.0, theme::surface_header());
            ui.painter().vline(
                strip.right() - 0.5,
                strip.y_range(),
                egui::Stroke::new(1.0_f32, theme::border()),
            );
            for n in first..last {
                let top = content.top() + n as f32 * row_height;
                let colour = if n == cursor_row {
                    theme::accent_text()
                } else {
                    theme::text_disabled()
                };
                let galley =
                    ui.painter()
                        .layout_no_wrap(format!("{}", n + 1), numbers.clone(), colour);
                ui.painter().galley(
                    egui::pos2(strip.right() - 10.0 - galley.size().x, top),
                    galley,
                    colour,
                );
            }
        });
    session.text_scroll = scrolled.state.offset.y;

    edit
}

/// One line, coloured by grammar.
fn coloured_line(
    line: &str,
    grammar: vuwr_core::Grammar,
    font: egui::FontId,
) -> egui::text::LayoutJob {
    use egui::text::{LayoutJob, TextFormat};
    let mut job = LayoutJob::default();
    job.wrap.max_width = f32::INFINITY;

    let spans = vuwr_core::highlight(line, grammar);
    if spans.is_empty() {
        job.append(
            line,
            0.0,
            TextFormat::simple(font, token_color(vuwr_core::Token::Plain)),
        );
        return job;
    }
    let mut at = 0usize;
    for span in spans {
        if span.start > at {
            job.append(
                &line[at..span.start],
                0.0,
                TextFormat::simple(font.clone(), token_color(vuwr_core::Token::Plain)),
            );
        }
        job.append(
            &line[span.start..span.end],
            0.0,
            TextFormat::simple(font.clone(), token_color(span.token)),
        );
        at = span.end;
    }
    if at < line.len() {
        job.append(
            &line[at..],
            0.0,
            TextFormat::simple(font, token_color(vuwr_core::Token::Plain)),
        );
    }
    job
}

fn token_color(token: vuwr_core::Token) -> Color32 {
    theme::token(token)
}

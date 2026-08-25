//! Rendering. One table plus a status line; row 0 (the header row) is bold.
//! Horizontal scrolling lives here rather than in `GridState` because it
//! depends on rendered column widths, which core knows nothing about.
//!
//! Tree view: two columns (key + summary), cursor row is highlighted.

use ratatui::prelude::*;
use ratatui::widgets::{Block, Cell as TCell, Clear, Paragraph, Row as TRow, Table};

use crate::app::App;
use vuwr_core::Command;
use vuwr_core::{Mode, ViewMode, escape};

pub fn render(frame: &mut Frame, app: &mut App) {
    let hints = if app.show_hints {
        app.hints()
    } else {
        Vec::new()
    };
    let hint_rows = if hints.is_empty() { 0 } else { 1 };
    let chunks = Layout::vertical([
        Constraint::Min(1),
        Constraint::Length(1),
        Constraint::Length(hint_rows),
    ])
    .split(frame.area());
    match app.view_mode() {
        ViewMode::Table => render_table(frame, app, chunks[0]),
        ViewMode::Tree => render_tree(frame, app, chunks[0]),
        ViewMode::Text => render_text(frame, app, chunks[0]),
    }
    render_status(frame, app, chunks[1]);
    if hint_rows == 1 {
        render_hints(frame, &hints, chunks[2]);
    }
    if app.show_help {
        render_help(frame, frame.area());
    }
}

/// The help overlay, built from `Command::ALL` paired with the keymap —
/// never hand-written, so a new command cannot be missing from it.
fn render_help(frame: &mut Frame, area: Rect) {
    let rows: Vec<TRow> = Command::ALL
        .iter()
        .map(|c| {
            TRow::new(vec![
                TCell::from(crate::keymap::keys_for(*c)).style(Style::default().bold()),
                TCell::from(c.description()),
            ])
        })
        .collect();

    let width = 56.min(area.width.saturating_sub(4));
    let height = (Command::ALL.len() as u16 + 2).min(area.height.saturating_sub(2));
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    let popup = Rect {
        x,
        y,
        width,
        height,
    };

    frame.render_widget(Clear, popup);
    let table = Table::new(rows, [Constraint::Length(20), Constraint::Min(10)]).block(
        Block::bordered()
            .title(" keys — ? to close ")
            .border_style(Style::default().dim()),
    );
    frame.render_widget(table, popup);
}

fn render_table(frame: &mut Frame, app: &mut App, area: Rect) {
    app.set_viewport_rows(area.height as usize);
    app.grid.ensure_visible(area.height as usize);

    let (cursor_row, cursor_col) = app.grid.cursor;
    let offset_row = app.grid.offset.0;
    let widths = app.widths().to_vec();
    let frozen = app.grid.frozen_cols.min(widths.len());

    // A one-character gutter carries the mark indicator. It is always
    // present so columns do not shift when a row is marked.
    let gutter = 1usize;
    let frozen_width: usize = widths[..frozen].iter().map(|w| w + 1).sum();
    let scroll_area = (area.width as usize).saturating_sub(gutter + 1 + frozen_width);

    // Frozen columns never scroll, so the scrolling window starts after
    // them and the cursor is kept inside it.
    let mut start_col = app.grid.offset.1.max(frozen).min(cursor_col.max(frozen));
    while start_col < cursor_col && span_width(&widths, start_col, cursor_col) > scroll_area {
        start_col += 1;
    }
    app.grid.offset.1 = start_col;

    let mut end_col = start_col;
    let mut used = 0;
    while end_col < widths.len() {
        let w = widths[end_col] + 1;
        if used + w > scroll_area && end_col > start_col {
            break;
        }
        used += w;
        end_col += 1;
    }

    // Column order on screen: gutter, frozen columns, then the window.
    let shown: Vec<usize> = (0..frozen).chain(start_col..end_col).collect();

    let mut constraints = vec![Constraint::Length(gutter as u16)];
    constraints.extend(shown.iter().map(|&c| Constraint::Length(widths[c] as u16)));

    let (headers, row_count, _col_count) = app.table_dims();
    let header_row = app.has_separate_header().then(|| {
        let mut cells = vec![TCell::from("")];
        cells.extend(shown.iter().map(|&c| {
            TCell::from(headers.get(c).cloned().unwrap_or_default()).style(Style::default().bold())
        }));
        TRow::new(cells)
    });

    let search = app.search.clone();
    // Which cell, if any, is being typed into right now.
    let editing_cell = app.is_editing_inline().then_some(app.grid.cursor);
    let mut rows = Vec::new();
    for r in offset_row..row_count.min(offset_row + area.height as usize) {
        let source = app.grid.source_row(r);
        let marked = app.grid.marks.contains(&source);
        let mut cells =
            vec![TCell::from(if marked { "*" } else { " " }).style(Style::default().bold())];
        cells.extend(shown.iter().map(|&c| {
            if editing_cell == Some((r, c)) {
                return TCell::from(Line::from(caret_spans(app)));
            }
            let text = app.table_cell(r, c).unwrap_or_default();
            let mut style = Style::default();
            if r == 0 && !app.has_separate_header() {
                style = style.bold();
            }
            // Show where the matches are, not just where the cursor landed.
            if search.as_ref().is_some_and(|s| s.matches(&text)) {
                style = style.underlined();
            }
            if marked {
                style = style.bold();
            }
            if (r, c) == (cursor_row, cursor_col) {
                style = style.reversed();
            }
            TCell::from(text).style(style)
        }));
        rows.push(TRow::new(cells));
    }

    let mut table = Table::new(rows, constraints).column_spacing(1);
    if let Some(header) = header_row {
        table = table.header(header);
    }
    frame.render_widget(table, area);
}

fn render_tree(frame: &mut Frame, app: &mut App, area: Rect) {
    app.set_viewport_rows(area.height as usize);
    app.grid.ensure_visible(area.height as usize);

    let (cursor_row, _) = app.grid.cursor;
    let offset_row = app.grid.offset.0;
    let rows_total = app.tree_rows.len();
    let editing = app.is_editing_inline();

    let mut lines: Vec<Line> = Vec::new();
    for r in offset_row..rows_total.min(offset_row + area.height as usize) {
        let row = &app.tree_rows[r];
        let mut spans: Vec<Span> = Vec::new();

        // Indent, then the disclosure marker: a closed container has more
        // to show, which is worth seeing at a glance.
        spans.push(Span::raw("  ".repeat(row.depth)));
        spans.push(Span::styled(
            match row.kind {
                vuwr_core::RowKind::Container { expanded: true } => "▾ ",
                vuwr_core::RowKind::Container { expanded: false } => "▸ ",
                vuwr_core::RowKind::Scalar => "  ",
            },
            Style::default().dim(),
        ));

        // A repeated key is legal and almost always a bug, so it is marked
        // rather than left to be noticed.
        if row.duplicate {
            spans.push(Span::styled("! ", Style::default().fg(Color::Red).bold()));
        }

        spans.push(Span::styled(
            escape(&row.label),
            Style::default().fg(Color::Cyan),
        ));
        spans.push(Span::raw(": "));
        // The value being typed is drawn in place of the value, rather
        // than only in the status line.
        if editing && r == cursor_row {
            spans.extend(caret_spans(app));
        } else {
            spans.push(Span::styled(
                escape(&row.summary),
                Style::default().fg(value_color(row.value)),
            ));
        }

        let mut line = Line::from(spans);
        // Reversing the whole row while typing would hide the caret,
        // which is drawn reversed itself.
        if r == cursor_row && !editing {
            line = line.style(Style::default().reversed());
        }
        lines.push(line);
    }

    frame.render_widget(Paragraph::new(lines), area);
}

/// Colour by value type, the way a JSON editor does: the shape of the data
/// is visible without reading it.
fn value_color(kind: vuwr_core::ValueKind) -> Color {
    use vuwr_core::ValueKind as V;
    match kind {
        V::Null => Color::DarkGray,
        V::Bool => Color::Magenta,
        V::Number => Color::Green,
        V::String => Color::Yellow,
        V::Array | V::Object => Color::Blue,
        V::Element => Color::Blue,
        V::Comment => Color::DarkGray,
        V::Text | V::Other => Color::Gray,
    }
}

/// The hint bar, nano-style: the keys worth knowing, spelled out along the
/// bottom. Built from the same keymap the help overlay uses, so it cannot
/// advertise a binding that does not exist.
fn render_hints(frame: &mut Frame, hints: &[Command], area: Rect) {
    let mut spans: Vec<Span> = Vec::new();
    for cmd in hints {
        if !spans.is_empty() {
            spans.push(Span::raw("  "));
        }
        // The key reversed like nano's, then the label.
        spans.push(Span::styled(
            format!(" {} ", first_key(crate::keymap::keys_for(*cmd))),
            Style::default().reversed(),
        ));
        spans.push(Span::raw(format!(" {}", cmd.short_label())));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// `keys_for` may list several bindings ("i  Enter"); the hint bar shows
/// only the first, which is the one to teach.
fn first_key(keys: &str) -> &str {
    keys.split_whitespace().next().unwrap_or(keys)
}

/// The view indicator: every view this document supports, with the current
/// one in brackets — `tree [table] text`. Cycling with Tab alone gave no
/// clue that the other views existed.
fn view_indicator(app: &App) -> String {
    let current = app.view_mode();
    app.available_views()
        .iter()
        .map(|v| {
            let name = match v {
                ViewMode::Table => "table",
                ViewMode::Tree => "tree",
                ViewMode::Text => "text",
            };
            if *v == current {
                format!("[{name}]")
            } else {
                name.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Text view: the raw source, paged like `less`, with a line-number
/// gutter and the cursor line highlighted.
fn render_text(frame: &mut Frame, app: &mut App, area: Rect) {
    app.set_viewport_rows(area.height as usize);
    app.grid.ensure_visible(area.height as usize);

    let (_, total, _) = app.table_dims();
    let offset = app.grid.offset.0;
    let cursor = app.grid.cursor.0;
    let gutter = total.to_string().len().max(2);

    let editing = app.is_editing_inline();
    let mut lines: Vec<Line> = Vec::new();
    for n in offset..total.min(offset + area.height as usize) {
        let gutter_span = Span::styled(
            format!("{:>gutter$} ", n + 1, gutter = gutter),
            Style::default().dim(),
        );

        // The line under edit shows the buffer being typed, with a caret,
        // rather than the value it had before.
        if editing && n == cursor {
            let mut spans = vec![gutter_span];
            spans.extend(caret_spans(app));
            lines.push(Line::from(spans));
            continue;
        }

        let text = app.table_cell(n, 0).unwrap_or_default();
        let mut style = Style::default();
        if n == cursor {
            style = style.reversed();
        }
        lines.push(Line::from(vec![gutter_span, Span::styled(text, style)]));
    }
    frame.render_widget(Paragraph::new(lines), area);
}

/// The text being typed, split around the caret so the caret sits where
/// it actually is rather than always at the end.
fn caret_spans(app: &App) -> Vec<Span<'static>> {
    let Some((_, buf)) = app.entry() else {
        return Vec::new();
    };
    // A terminal row is one line; a value containing newlines would break
    // the layout, and such values go to the larger editor anyway.
    let buf = buf.replace('\n', "⏎");
    let buf = buf.as_str();
    let caret = app.entry_caret().min(buf.len());
    let (before, after) = buf.split_at(caret);
    let mut after_chars = after.chars();
    let under = after_chars.next();
    let rest: String = after_chars.collect();

    vec![
        Span::raw(before.to_string()),
        // The character under the caret is highlighted; at the end of the
        // line there is none, so a space stands in.
        Span::styled(
            under.map(|c| c.to_string()).unwrap_or_else(|| " ".into()),
            Style::default().reversed(),
        ),
        Span::raw(rest),
    ]
}

fn span_width(widths: &[usize], from: usize, to: usize) -> usize {
    widths[from..=to].iter().sum::<usize>() + (to - from)
}

fn render_status(frame: &mut Frame, app: &App, area: Rect) {
    let line = match &app.mode {
        Mode::Normal => {
            let dirty = if app.dirty { " [+]" } else { "" };
            let depth = app.grid.depth();
            let depth_str = if depth > 0 {
                format!(" depth:{depth}")
            } else {
                String::new()
            };
            let view_str = view_indicator(app);
            match app.view_mode() {
                ViewMode::Text => {
                    let (_, lines, _) = app.table_dims();
                    let pct = ((app.grid.cursor.0 + 1) * 100)
                        .checked_div(lines)
                        .unwrap_or(100);
                    format!(
                        " {}{}  {}  line {}/{}  {}%  {}",
                        app.path().display(),
                        dirty,
                        view_str,
                        app.grid.cursor.0 + 1,
                        lines,
                        pct,
                        app.status
                    )
                }
                ViewMode::Table => {
                    let (r, c) = app.grid.cursor;
                    let (_, row_count, col_count) = app.table_dims();
                    format!(
                        " {}{}  {}  row {}/{} col {}/{}  {}",
                        app.path().display(),
                        dirty,
                        view_str,
                        r + 1,
                        row_count,
                        c + 1,
                        col_count,
                        app.status
                    )
                }
                ViewMode::Tree => {
                    let (r, _) = app.grid.cursor;
                    format!(
                        " {}{}  {}{}  {}/{}  {}",
                        app.path().display(),
                        dirty,
                        view_str,
                        depth_str,
                        r + 1,
                        app.tree_rows.len(),
                        app.status
                    )
                }
            }
        }
        Mode::Prompt { kind, buf } => format!(" {}{buf}▏", kind.sigil()),
        // An inline edit is visible where it is happening, so the status
        // line reports position instead of repeating the text.
        Mode::Edit { .. } => {
            let (r, c) = app.grid.cursor;
            let what = if app.is_renaming() {
                "renaming"
            } else {
                "editing"
            };
            format!(
                " ({},{}) {what} — Enter to commit, Esc to cancel",
                r + 1,
                c + 1
            )
        }
        Mode::Command { buf } => format!(" :{buf}▏"),
    };
    frame.render_widget(Paragraph::new(line), area);
}

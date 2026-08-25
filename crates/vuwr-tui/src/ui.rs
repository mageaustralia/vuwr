//! Rendering. One table plus a status line; row 0 (the header row) is bold.
//! Horizontal scrolling lives here rather than in `GridState` because it
//! depends on rendered column widths, which core knows nothing about.
//!
//! Tree view: two columns (key + summary), cursor row is highlighted.

use ratatui::prelude::*;
use ratatui::widgets::{
    Block, BorderType, Cell as TCell, Clear, Paragraph, Row as TRow, Table, Wrap,
};

use crate::app::App;
use crate::palette;
use vuwr_core::Command;
use vuwr_core::{Mode, ViewMode, escape};

pub fn render(frame: &mut Frame, app: &mut App) {
    let hints = if app.show_hints {
        app.hints()
    } else {
        Vec::new()
    };
    let hint_rows = if hints.is_empty() { 0 } else { 1 };
    // The detail pane takes a few lines, capped so it never crowds out
    // the view it is describing.
    let detail = app.show_detail.then(|| app.detail_text()).flatten();
    // Two borders plus a few lines of text: enough for a paragraph
    // without crowding out the view it describes.
    let detail_rows = match &detail {
        Some(_) => (frame.area().height / 3).clamp(4, 12),
        None => 0,
    };
    let chunks = Layout::vertical([
        Constraint::Min(1),
        Constraint::Length(detail_rows),
        // A line of air: the last row of data sat against the status line
        // with nothing between them, and read as part of it.
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(hint_rows),
    ])
    .split(frame.area());
    // A named scheme brings its own surface, painted under the document
    // before anything is drawn on it. Without it, Monokai's near-white
    // text lands on whatever the terminal's background happens to be —
    // which on a light terminal is nothing at all.
    if let Some(bg) = palette::background() {
        frame.render_widget(
            ratatui::widgets::Block::default().style(Style::default().bg(bg)),
            chunks[0],
        );
    }
    match app.view_mode() {
        ViewMode::Table => render_table(frame, app, chunks[0]),
        ViewMode::Tree => render_tree(frame, app, chunks[0]),
        ViewMode::Text => render_text(frame, app, chunks[0]),
    }
    if let Some(text) = detail {
        render_detail(frame, app, &text, chunks[1]);
    }
    render_status(frame, app, chunks[3]);
    if hint_rows == 1 {
        render_hints(frame, &hints, chunks[4]);
    }
    if app.editing_large() {
        render_large_edit(frame, app, frame.area());
    }
    if app.show_help {
        render_help(frame, frame.area());
    }
}

/// The selected value in full, wrapped — a spreadsheet's formula bar.
///
/// A table column is far narrower than a description, so most of the file
/// is otherwise behind a truncation.
fn render_detail(frame: &mut Frame, app: &App, text: &str, area: Rect) {
    let block = Block::bordered()
        .title(format!(" {} ", app.detail_label()))
        .border_style(Style::default().dim());
    frame.render_widget(
        Paragraph::new(text.to_string())
            .block(block)
            .wrap(Wrap { trim: false }),
        area,
    );
}

/// The larger editor, for a value that will not fit on one line.
///
/// The GUI opens a window; a terminal has none, so this is an overlay over
/// the view, with the same decoded text and the same commit.
fn render_large_edit(frame: &mut Frame, app: &App, area: Rect) {
    let Some((buf, caret)) = &app.large_edit else {
        return;
    };
    let width = area.width.saturating_sub(6).max(20);
    let height = area.height.saturating_sub(4).max(6);
    let popup = Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + (area.height.saturating_sub(height)) / 2,
        width,
        height,
    };
    frame.render_widget(Clear, popup);

    // Walk the text once, breaking lines at newlines and marking the
    // character under the caret as it goes.
    //
    // Splitting at the caret and reassembling looked simpler and was
    // wrong: with the caret *on* a newline, that newline became a reversed
    // span holding "\n", which draws nothing and does not break the line —
    // so the next line silently joined this one and the text appeared to
    // change as the cursor moved.
    let caret = (*caret).min(buf.len());
    let mut lines: Vec<Line> = Vec::new();
    let mut spans: Vec<Span> = Vec::new();
    let mut run = String::new();
    let caret_style = Style::default().bg(Color::Yellow).fg(Color::Black).bold();

    for (i, ch) in buf.char_indices() {
        if ch == '\n' {
            if !run.is_empty() {
                spans.push(Span::raw(std::mem::take(&mut run)));
            }
            if i == caret {
                // The caret sits at the end of the line it is on, and the
                // newline still breaks it.
                spans.push(Span::styled(" ", caret_style));
            }
            lines.push(Line::from(std::mem::take(&mut spans)));
            continue;
        }
        if i == caret {
            if !run.is_empty() {
                spans.push(Span::raw(std::mem::take(&mut run)));
            }
            spans.push(Span::styled(ch.to_string(), caret_style));
        } else {
            run.push(ch);
        }
    }
    if !run.is_empty() {
        spans.push(Span::raw(run));
    }
    if caret >= buf.len() {
        spans.push(Span::styled(" ", caret_style));
    }
    lines.push(Line::from(spans));

    // A filled, coloured frame: the overlay sits on top of the view, and a
    // dim border left it hard to tell where the editor ended and the table
    // behind it began.
    let block = Block::bordered()
        .border_type(BorderType::Thick)
        .border_style(Style::default().fg(Color::Cyan))
        .style(Style::default().bg(Color::Black).fg(Color::White))
        .title(Span::styled(
            " edit value ",
            Style::default().fg(Color::Black).bg(Color::Cyan).bold(),
        ))
        .title_bottom(Span::styled(
            " Ctrl-S save   Esc cancel   ←→↑↓ move ",
            Style::default().fg(Color::Black).bg(Color::Cyan).bold(),
        ));
    frame.render_widget(
        Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: false }),
        popup,
    );
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
    app.set_viewport_cols(area.width as usize);
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
            spans.push(Span::styled(
                "! ",
                Style::default().fg(palette::bad()).bold(),
            ));
        }

        spans.push(Span::styled(
            escape(&row.label),
            Style::default().fg(palette::accent()),
        ));
        spans.push(Span::raw(": "));
        // The value being typed is drawn in place of the value, rather
        // than only in the status line.
        if editing && r == cursor_row {
            spans.extend(caret_spans(app));
        } else {
            spans.push(Span::styled(
                escape(&row.summary),
                Style::default().fg(palette::value(row.value)),
            ));
        }

        let mut line = Line::from(spans);
        // Reversing the whole row while typing would hide the caret,
        // which is drawn reversed itself.
        if r == cursor_row && !editing {
            line = line.style(Style::default().bg(palette::row_selected()));
        }
        lines.push(line);
    }

    frame.render_widget(Paragraph::new(lines), area);
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
            Style::default().reversed().bold(),
        ));
        spans.push(Span::styled(
            format!(" {}", cmd.short_label()),
            Style::default().fg(palette::faint()),
        ));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// `keys_for` may list several bindings ("i  Enter"); the hint bar shows
/// only the first, which is the one to teach.
fn first_key(keys: &str) -> &str {
    keys.split_whitespace().next().unwrap_or(keys)
}

/// The view indicator as styled spans: the current view bright and
/// bracketed, the others in the accent — the terminal's version of the
/// window's segmented control.
fn view_spans(app: &App) -> Vec<Span<'static>> {
    let current = app.view_mode();
    let mut spans = Vec::new();
    for view in app.available_views() {
        if !spans.is_empty() {
            spans.push(Span::raw(" "));
        }
        let name = match view {
            ViewMode::Table => "table",
            ViewMode::Tree => "tree",
            ViewMode::Text => "text",
        };
        if view == current {
            spans.push(Span::styled(
                format!("[{name}]"),
                Style::default().fg(palette::text()).bold(),
            ));
        } else {
            spans.push(Span::styled(
                name.to_string(),
                Style::default().fg(palette::accent()),
            ));
        }
    }
    spans
}

/// The view indicator: every view this document supports, with the current
/// one in brackets — `tree [table] text`. Cycling with Tab alone gave no
/// clue that the other views existed.
#[allow(dead_code)]
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
    let grammar = app.grammar();
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

        // Coloured by grammar, as the window's text view is: the same
        // `highlight` from core, so tags, strings and comments are the
        // same things in both. The terminal has fewer colours to spend,
        // so structure takes the accent and content stays bright.
        let text = app.table_cell(n, 0).unwrap_or_default();
        let mut spans = vec![gutter_span];
        for span in highlighted(&text, grammar) {
            spans.push(if n == cursor {
                span.style(Style::default().bg(palette::row_selected()))
            } else {
                span
            });
        }
        lines.push(Line::from(spans));
    }
    frame.render_widget(Paragraph::new(lines), area);
}

/// One line of source, split into coloured spans.
fn highlighted(line: &str, grammar: vuwr_core::Grammar) -> Vec<Span<'static>> {
    let spans = vuwr_core::highlight(line, grammar);
    if spans.is_empty() {
        return vec![Span::styled(
            line.to_string(),
            Style::default().fg(palette::text()),
        )];
    }
    let mut out = Vec::new();
    let mut at = 0usize;
    for span in spans {
        if span.start > at {
            out.push(Span::styled(
                line[at..span.start].to_string(),
                Style::default().fg(palette::token(vuwr_core::Token::Plain)),
            ));
        }
        out.push(Span::styled(
            line[span.start..span.end].to_string(),
            Style::default().fg(palette::token(span.token)),
        ));
        at = span.end;
    }
    if at < line.len() {
        out.push(Span::styled(
            line[at..].to_string(),
            Style::default().fg(palette::token(vuwr_core::Token::Plain)),
        ));
    }
    out
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
    // A prompt or an edit takes the line: there is nowhere else for what
    // is being typed to go.
    let line = match &app.mode {
        Mode::Prompt { kind, buf } => Line::from(Span::styled(
            format!(" {}{buf}▏", kind.sigil()),
            Style::default().fg(palette::text()),
        )),
        Mode::Command { buf } => Line::from(Span::styled(
            format!(" :{buf}▏"),
            Style::default().fg(palette::text()),
        )),
        Mode::Edit { .. } => {
            let (r, c) = app.grid.cursor;
            let what = if app.is_renaming() {
                "renaming"
            } else {
                "editing"
            };
            Line::from(vec![
                Span::styled(
                    format!(" ({},{}) ", r + 1, c + 1),
                    Style::default().fg(palette::dim()),
                ),
                Span::styled(
                    what.to_string(),
                    Style::default().fg(palette::edit_ring()).bold(),
                ),
                Span::styled(
                    " — Enter to commit, Esc to cancel".to_string(),
                    Style::default().fg(palette::faint()),
                ),
            ])
        }
        Mode::Normal => normal_status(app, area.width),
    };
    frame.render_widget(Paragraph::new(line), area);
}

/// Where you are, then what the document is — the same split the window
/// makes, with the right-hand end holding state rather than position.
fn normal_status(app: &App, width: u16) -> Line<'static> {
    let mut left: Vec<Span> = vec![Span::raw(" ")];
    left.extend(view_spans(app));
    left.push(Span::raw("  "));

    let (r, c) = app.grid.cursor;
    let (_, row_count, col_count) = app.table_dims();
    let (primary, secondary) = match app.view_mode() {
        ViewMode::Text => (
            format!("line {}/{}", r + 1, row_count),
            format!("{}%", ((r + 1) * 100).checked_div(row_count).unwrap_or(100)),
        ),
        ViewMode::Table => (
            format!("row {}/{}", r + 1, row_count),
            format!("col {}/{}", c + 1, col_count),
        ),
        ViewMode::Tree => (
            format!("row {}/{}", r + 1, app.tree_rows.len()),
            match app.grid.depth() {
                0 => String::new(),
                d => format!("depth {d}"),
            },
        ),
    };
    left.push(Span::styled(primary, Style::default().fg(palette::text())));
    if !secondary.is_empty() {
        left.push(Span::raw("  "));
        left.push(Span::styled(secondary, Style::default().fg(palette::dim())));
    }
    if let Some(n) = app.visible_count() {
        left.push(Span::raw("  "));
        left.push(Span::styled(
            format!("filtered {n}/{row_count}"),
            Style::default().fg(palette::accent()),
        ));
    }
    if !app.status.is_empty() {
        left.push(Span::raw("  "));
        left.push(Span::styled(
            app.status.clone(),
            Style::default().fg(palette::faint()),
        ));
    }

    // The right-hand end: the file, and whether it is saved. Amber only
    // when there is something unsaved, so it always means something.
    let mut right: Vec<Span> = Vec::new();
    if app.dirty {
        right.push(Span::styled(
            "● unsaved".to_string(),
            Style::default().fg(palette::warn()),
        ));
        right.push(Span::raw("  "));
    }
    right.push(Span::styled(
        app.path().display().to_string(),
        Style::default().fg(palette::accent()),
    ));
    right.push(Span::raw(" "));

    let used: usize = left
        .iter()
        .chain(right.iter())
        .map(|s| s.content.chars().count())
        .sum();
    let gap = (width as usize).saturating_sub(used).max(1);
    left.push(Span::raw(" ".repeat(gap)));
    left.extend(right);
    Line::from(left)
}

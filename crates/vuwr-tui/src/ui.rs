//! Rendering. One table plus a status line; row 0 (the header row) is bold.
//! Horizontal scrolling lives here rather than in `GridState` because it
//! depends on rendered column widths, which core knows nothing about.
//!
//! Tree view: two columns (key + summary), cursor row is highlighted.

use ratatui::prelude::*;
use ratatui::widgets::{Block, Cell as TCell, Clear, Paragraph, Row as TRow, Table};

use crate::app::{App, Mode, ViewMode, escape};
use vuwr_core::Command;

pub fn render(frame: &mut Frame, app: &mut App) {
    let chunks = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).split(frame.area());
    match app.view_mode() {
        ViewMode::Table => render_table(frame, app, chunks[0]),
        ViewMode::Tree => render_tree(frame, app, chunks[0]),
        ViewMode::Text => render_text(frame, app, chunks[0]),
    }
    render_status(frame, app, chunks[1]);
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
    let mut start_col = app.grid.offset.1.min(cursor_col);
    while start_col < cursor_col && span_width(&widths, start_col, cursor_col) > area.width as usize
    {
        start_col += 1;
    }
    app.grid.offset.1 = start_col;

    let mut end_col = start_col;
    let mut used = 0;
    while end_col < widths.len() {
        let w = widths[end_col] + 1;
        if used + w > area.width as usize && end_col > start_col {
            break;
        }
        used += w;
        end_col += 1;
    }

    let constraints: Vec<Constraint> = widths[start_col..end_col]
        .iter()
        .map(|&w| Constraint::Length(w as u16))
        .collect();

    let (headers, row_count, _col_count) = app.table_dims();
    // CSV's header is row 0 of its own data; JSON and XML carry column
    // names separately, so draw them as a real header row.
    let header_row = app.has_separate_header().then(|| {
        TRow::new(
            headers[start_col..end_col.min(headers.len())]
                .iter()
                .map(|h| TCell::from(h.clone()).style(Style::default().bold()))
                .collect::<Vec<_>>(),
        )
    });
    let mut rows = Vec::new();
    for r in offset_row..row_count.min(offset_row + area.height as usize) {
        let cells: Vec<TCell> = (start_col..end_col)
            .map(|c| {
                let text = app.table_cell(r, c).unwrap_or_default();
                let mut cell = TCell::from(text);
                if r == 0 {
                    cell = cell.style(Style::default().bold());
                }
                if (r, c) == (cursor_row, cursor_col) {
                    cell = cell.style(Style::default().reversed());
                }
                cell
            })
            .collect();
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

    // Two columns: key (wide) + summary (fills remaining).
    let key_width = 20u16;
    let constraints = vec![
        Constraint::Length(key_width),
        Constraint::Min(area.width.saturating_sub(key_width + 1)),
    ];

    let keys = &app.tree_keys;
    let summaries = &app.tree_summaries;

    let mut rows = Vec::new();
    for r in offset_row..summaries.len().min(offset_row + area.height as usize) {
        let key_text = keys.get(r).map(|s| s.as_str()).unwrap_or("");
        let val_text = summaries.get(r).map(|s| escape(s)).unwrap_or_default();

        let mut key_cell = TCell::from(key_text);
        let mut val_cell = TCell::from(val_text);

        if r == cursor_row {
            key_cell = key_cell.style(Style::default().reversed());
            val_cell = val_cell.style(Style::default().reversed());
        }

        rows.push(TRow::new(vec![key_cell, val_cell]));
    }

    let table = Table::new(rows, constraints).column_spacing(1);
    frame.render_widget(table, area);
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

    let mut lines: Vec<Line> = Vec::new();
    for n in offset..total.min(offset + area.height as usize) {
        let text = app.table_cell(n, 0).unwrap_or_default();
        let mut style = Style::default();
        if n == cursor {
            style = style.reversed();
        }
        lines.push(Line::from(vec![
            Span::styled(
                format!("{:>gutter$} ", n + 1, gutter = gutter),
                Style::default().dim(),
            ),
            Span::styled(text, style),
        ]));
    }
    frame.render_widget(Paragraph::new(lines), area);
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
            let view_str = format!("{:?}", app.view_mode()).to_lowercase();
            match app.view_mode() {
                ViewMode::Text => {
                    let (_, lines, _) = app.table_dims();
                    let pct = ((app.grid.cursor.0 + 1) * 100)
                        .checked_div(lines)
                        .unwrap_or(100);
                    format!(
                        " {}{} [text]  line {}/{}  {}%  {}",
                        app.path().display(),
                        dirty,
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
                        " {}{} [{}]  row {}/{} col {}/{}  {}",
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
                        " {}{} [{}{}]  {}/{}  {}",
                        app.path().display(),
                        dirty,
                        view_str,
                        depth_str,
                        r + 1,
                        app.tree_keys.len(),
                        app.status
                    )
                }
            }
        }
        Mode::Edit { buf } => {
            let (r, c) = app.grid.cursor;
            format!(" ({},{}) > {buf}▏", r + 1, c + 1)
        }
        Mode::Command { buf } => format!(" :{buf}▏"),
    };
    frame.render_widget(Paragraph::new(line), area);
}

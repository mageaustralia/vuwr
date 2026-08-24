//! Rendering. One table plus a status line; row 0 (the header row) is bold.
//! Horizontal scrolling lives here rather than in `GridState` because it
//! depends on rendered column widths, which core knows nothing about.

use ratatui::prelude::*;
use ratatui::widgets::{Cell as TCell, Paragraph, Row as TRow, Table};

use crate::app::{App, Mode, escape};

pub fn render(frame: &mut Frame, app: &mut App) {
    let chunks = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).split(frame.area());
    render_table(frame, app, chunks[0]);
    render_status(frame, app, chunks[1]);
}

fn render_table(frame: &mut Frame, app: &mut App, area: Rect) {
    app.set_viewport_rows(area.height as usize);
    app.grid.ensure_visible(area.height as usize);

    let (cursor_row, cursor_col) = app.grid.cursor;
    let offset_row = app.grid.offset.0;

    // Adjust the horizontal scroll offset so the cursor column is visible.
    // Column widths are a handful of small ints, so work on a copy and keep
    // the borrow checker out of the scroll adjustment.
    let widths = app.widths().to_vec();
    let mut start_col = app.grid.offset.1.min(cursor_col);
    while start_col < cursor_col && span_width(&widths, start_col, cursor_col) > area.width as usize
    {
        start_col += 1;
    }
    app.grid.offset.1 = start_col;

    // How many columns fit from start_col.
    let mut end_col = start_col;
    let mut used = 0;
    while end_col < widths.len() {
        let w = widths[end_col] + 1; // +1 for the column gap
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

    let doc = app.csv();
    let mut rows = Vec::new();
    for r in offset_row..doc.height().min(offset_row + area.height as usize) {
        let cells: Vec<TCell> = (start_col..end_col)
            .map(|c| {
                let text = doc
                    .cell(r, c)
                    .map(|cell| escape(&cell.value))
                    .unwrap_or_default();
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

    let table = Table::new(rows, constraints).column_spacing(1);
    frame.render_widget(table, area);
}

fn span_width(widths: &[usize], from: usize, to: usize) -> usize {
    widths[from..=to].iter().sum::<usize>() + (to - from) // gaps
}

fn render_status(frame: &mut Frame, app: &App, area: Rect) {
    let line = match &app.mode {
        Mode::Normal => {
            let dirty = if app.dirty { " [+]" } else { "" };
            let (r, c) = app.grid.cursor;
            format!(
                " {}{}  row {}/{} col {}/{}  {}",
                app.path().display(),
                dirty,
                r + 1,
                app.csv().height(),
                c + 1,
                app.csv().width(),
                app.status
            )
        }
        Mode::Edit { buf } => {
            let (r, c) = app.grid.cursor;
            format!(" ({},{}) > {buf}▏", r + 1, c + 1)
        }
        Mode::Command { buf } => format!(" :{buf}▏"),
    };
    frame.render_widget(Paragraph::new(line), area);
}

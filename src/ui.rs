use crate::app::{App, InputTarget, Mode, Pane};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style, Stylize},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
    Frame,
};

const ACCENT: Color = Color::Cyan;
const DIM: Color = Color::DarkGray;
const ERROR: Color = Color::LightRed;

pub fn draw(f: &mut Frame, app: &App) {
    let area = f.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(area);

    draw_header(f, app, chunks[0]);
    draw_body(f, app, chunks[1]);
    draw_command(f, app, chunks[2]);
    draw_help(f, app, chunks[3]);

    match app.mode {
        Mode::NoteView => draw_note_view(f, app, area),
        Mode::NoteEdit => draw_note_edit(f, app, area),
        _ => {}
    }

    if app.mode == Mode::Input {
        draw_input(f, app, area);
    }
}

fn draw_header(f: &mut Frame, app: &App, area: Rect) {
    let title = Span::styled(
        " todo-tui ",
        Style::default().bg(ACCENT).fg(Color::Black).bold(),
    );
    let counts = format!(
        " todos:{}  notes:{} ",
        app.store.todos.len(),
        app.store.notes.len()
    );
    let focus = match app.focus {
        Pane::Todos => " ◆ todos ",
        Pane::Notes => " ◆ notes ",
    };
    let line = Line::from(vec![
        title,
        Span::raw(" "),
        Span::styled(counts, Style::default().fg(DIM)),
        Span::raw("  "),
        Span::styled(focus, Style::default().fg(ACCENT).bold()),
    ]);
    f.render_widget(Paragraph::new(line), area);
}

fn draw_body(f: &mut Frame, app: &App, area: Rect) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    draw_todos(f, app, cols[0]);
    draw_notes(f, app, cols[1]);
}

fn pane_block(title: &str, focused: bool) -> Block<'_> {
    let style = if focused {
        Style::default().fg(ACCENT).bold()
    } else {
        Style::default().fg(DIM)
    };
    Block::default()
        .borders(Borders::ALL)
        .border_style(style)
        .title(Span::styled(format!(" {} ", title), style))
}

fn draw_todos(f: &mut Frame, app: &App, area: Rect) {
    let focused = app.focus == Pane::Todos;
    let items: Vec<ListItem> = app
        .store
        .todos
        .iter()
        .map(|t| {
            let mark = if t.done { "[x]" } else { "[ ]" };
            let mark_style = if t.done {
                Style::default().fg(Color::Green)
            } else {
                Style::default().fg(Color::Yellow)
            };
            let title_style = if t.done {
                Style::default()
                    .fg(DIM)
                    .add_modifier(Modifier::CROSSED_OUT)
            } else {
                Style::default().fg(Color::White)
            };
            ListItem::new(Line::from(vec![
                Span::styled(mark, mark_style),
                Span::raw(" "),
                Span::styled(t.title.clone(), title_style),
            ]))
        })
        .collect();

    let block = pane_block("Todos", focused);
    let list = List::new(items)
        .block(block)
        .highlight_style(highlight_style(focused))
        .highlight_symbol("▶ ");
    let mut state = ListState::default();
    if !app.store.todos.is_empty() {
        state.select(Some(app.todo_index.min(app.store.todos.len() - 1)));
    }
    f.render_stateful_widget(list, area, &mut state);

    if app.store.todos.is_empty() {
        draw_empty(f, area, "no todos — press i to add");
    }
}

fn draw_notes(f: &mut Frame, app: &App, area: Rect) {
    let focused = app.focus == Pane::Notes;
    let items: Vec<ListItem> = app
        .store
        .notes
        .iter()
        .map(|n| {
            let preview = first_line(&n.body);
            let preview = if preview.is_empty() {
                String::from("(empty)")
            } else {
                preview
            };
            ListItem::new(Line::from(vec![
                Span::styled("● ", Style::default().fg(Color::Magenta)),
                Span::styled(n.title.clone(), Style::default().fg(Color::White).bold()),
                Span::raw("  "),
                Span::styled(preview, Style::default().fg(DIM)),
            ]))
        })
        .collect();

    let block = pane_block("Notes", focused);
    let list = List::new(items)
        .block(block)
        .highlight_style(highlight_style(focused))
        .highlight_symbol("▶ ");
    let mut state = ListState::default();
    if !app.store.notes.is_empty() {
        state.select(Some(app.note_index.min(app.store.notes.len() - 1)));
    }
    f.render_stateful_widget(list, area, &mut state);

    if app.store.notes.is_empty() {
        draw_empty(f, area, "no notes — press i to add");
    }
}

fn draw_empty(f: &mut Frame, area: Rect, msg: &str) {
    let inner = Rect {
        x: area.x + 2,
        y: area.y + 2,
        width: area.width.saturating_sub(4),
        height: 1,
    };
    let p = Paragraph::new(Span::styled(msg, Style::default().fg(DIM)));
    f.render_widget(p, inner);
}

fn highlight_style(focused: bool) -> Style {
    if focused {
        Style::default()
            .bg(ACCENT)
            .fg(Color::Black)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().bg(Color::DarkGray).fg(Color::White)
    }
}

fn draw_command(f: &mut Frame, app: &App, area: Rect) {
    let line = match app.mode {
        Mode::Command => Line::from(vec![
            Span::styled(":", Style::default().fg(ACCENT).bold()),
            Span::raw(app.command_buffer.clone()),
            Span::styled("█", Style::default().fg(ACCENT)),
        ]),
        Mode::Search => Line::from(vec![
            Span::styled("/", Style::default().fg(Color::Yellow).bold()),
            Span::raw(app.search_buffer.clone()),
            Span::styled("█", Style::default().fg(Color::Yellow)),
        ]),
        _ => {
            let style = if app.status.starts_with("error") || app.status.contains("failed") {
                Style::default().fg(ERROR)
            } else {
                Style::default().fg(DIM)
            };
            Line::from(Span::styled(app.status.clone(), style))
        }
    };
    f.render_widget(Paragraph::new(line), area);
}

fn draw_help(f: &mut Frame, app: &App, area: Rect) {
    let hints = match app.mode {
        Mode::Normal => "hjkl move  i add  dd del  x done  / search  : cmd  e edit-note  q quit",
        Mode::Command => "Enter run  Esc cancel   commands: :todo :notes :new :delete :w :q :help",
        Mode::Search => "Enter jump  Esc cancel   then n/N for next/prev",
        Mode::Input => "Enter confirm  Esc cancel",
        Mode::NoteView => "Esc/Enter close  e edit",
        Mode::NoteEdit => "type to edit  Enter newline  Esc save & close",
    };
    f.render_widget(
        Paragraph::new(Span::styled(hints, Style::default().fg(DIM))),
        area,
    );
}

fn draw_input(f: &mut Frame, app: &App, area: Rect) {
    let title = match app.input_target {
        Some(InputTarget::NewTodo) => "New todo",
        Some(InputTarget::NewNote) => "New note",
        Some(InputTarget::RenameTodo) => "Rename todo",
        Some(InputTarget::RenameNote) => "Rename note",
        None => "Input",
    };
    let popup = centered_rect(60, 3, area);
    f.render_widget(Clear, popup);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(ACCENT))
        .title(Span::styled(
            format!(" {} ", title),
            Style::default().fg(ACCENT).bold(),
        ));
    let text = Line::from(vec![
        Span::raw(app.input_buffer.clone()),
        Span::styled("█", Style::default().fg(ACCENT)),
    ]);
    f.render_widget(Paragraph::new(text).block(block), popup);
}

fn draw_note_view(f: &mut Frame, app: &App, area: Rect) {
    let Some(note) = app.store.notes.get(app.note_index) else {
        return;
    };
    let popup = centered_rect(80, 70, area);
    f.render_widget(Clear, popup);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Magenta))
        .title(Span::styled(
            format!(" {} ", note.title),
            Style::default().fg(Color::Magenta).bold(),
        ));
    let body = if note.body.is_empty() {
        String::from("(empty — press e to edit)")
    } else {
        note.body.clone()
    };
    let p = Paragraph::new(body)
        .block(block)
        .wrap(Wrap { trim: false });
    f.render_widget(p, popup);
}

fn draw_note_edit(f: &mut Frame, app: &App, area: Rect) {
    let title = app
        .editing_note_index
        .and_then(|i| app.store.notes.get(i))
        .map(|n| n.title.clone())
        .unwrap_or_else(|| String::from("note"));
    let popup = centered_rect(80, 70, area);
    f.render_widget(Clear, popup);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow))
        .title(Span::styled(
            format!(" editing: {} ", title),
            Style::default().fg(Color::Yellow).bold(),
        ));
    let body = format!("{}█", app.note_buffer);
    let p = Paragraph::new(body).block(block).wrap(Wrap { trim: false });
    f.render_widget(p, popup);
}

fn first_line(s: &str) -> String {
    s.lines().next().unwrap_or("").trim().to_string()
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let v = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(v[1])[1]
}

#[allow(dead_code)]
fn alignment_center() -> Alignment {
    Alignment::Center
}

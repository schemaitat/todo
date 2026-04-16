use crate::app::{App, InputTarget, Mode, Pane};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style, Stylize},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
    Frame,
};

const ACCENT: Color = Color::Cyan;
const DIM: Color = Color::DarkGray;
const ERROR: Color = Color::LightRed;
const POPUP_BG: Color = Color::Rgb(20, 20, 30);
const POPUP_FG: Color = Color::Rgb(230, 230, 230);

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
    let mut spans = vec![
        title,
        Span::raw(" "),
        Span::styled(counts, Style::default().fg(DIM)),
        Span::raw("  "),
        Span::styled(focus, Style::default().fg(ACCENT).bold()),
    ];
    if !app.filter.is_empty() {
        spans.push(Span::raw("  "));
        spans.push(Span::styled(
            format!(" filter:/{}/ ", app.filter),
            Style::default().bg(Color::Yellow).fg(Color::Black).bold(),
        ));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn draw_body(f: &mut Frame, app: &App, area: Rect) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    draw_todos(f, app, cols[0]);
    draw_notes(f, app, cols[1]);
}

fn pane_block(title: &str, focused: bool, count: usize, total: usize) -> Block<'_> {
    let style = if focused {
        Style::default().fg(ACCENT).bold()
    } else {
        Style::default().fg(DIM)
    };
    let label = if count == total {
        format!(" {} [{}] ", title, total)
    } else {
        format!(" {} [{}/{}] ", title, count, total)
    };
    Block::default()
        .borders(Borders::ALL)
        .border_style(style)
        .title(Span::styled(label, style))
}

fn draw_todos(f: &mut Frame, app: &App, area: Rect) {
    let focused = app.focus == Pane::Todos;
    let visible = app.visible_todo_indices();
    let items: Vec<ListItem> = visible
        .iter()
        .map(|&i| {
            let t = &app.store.todos[i];
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
                Span::styled(highlight_match(&t.title, &app.filter), title_style),
            ]))
        })
        .collect();

    let block = pane_block("Todos", focused, visible.len(), app.store.todos.len());
    let list = List::new(items)
        .block(block)
        .highlight_style(highlight_style(focused))
        .highlight_symbol("▶ ");
    let mut state = ListState::default();
    if let Some(pos) = visible.iter().position(|&i| i == app.todo_index) {
        state.select(Some(pos));
    } else if !visible.is_empty() {
        state.select(Some(0));
    }
    f.render_stateful_widget(list, area, &mut state);

    if visible.is_empty() {
        let msg = if app.store.todos.is_empty() {
            "no todos — press i to add"
        } else {
            "no matches"
        };
        draw_empty(f, area, msg);
    }
}

fn draw_notes(f: &mut Frame, app: &App, area: Rect) {
    let focused = app.focus == Pane::Notes;
    let visible = app.visible_note_indices();
    let items: Vec<ListItem> = visible
        .iter()
        .map(|&i| {
            let n = &app.store.notes[i];
            let preview = first_line(&n.body);
            let preview = if preview.is_empty() {
                String::from("(empty)")
            } else {
                preview
            };
            ListItem::new(Line::from(vec![
                Span::styled("● ", Style::default().fg(Color::Magenta)),
                Span::styled(
                    highlight_match(&n.title, &app.filter),
                    Style::default().fg(Color::White).bold(),
                ),
                Span::raw("  "),
                Span::styled(
                    highlight_match(&preview, &app.filter),
                    Style::default().fg(DIM),
                ),
            ]))
        })
        .collect();

    let block = pane_block("Notes", focused, visible.len(), app.store.notes.len());
    let list = List::new(items)
        .block(block)
        .highlight_style(highlight_style(focused))
        .highlight_symbol("▶ ");
    let mut state = ListState::default();
    if let Some(pos) = visible.iter().position(|&i| i == app.note_index) {
        state.select(Some(pos));
    } else if !visible.is_empty() {
        state.select(Some(0));
    }
    f.render_stateful_widget(list, area, &mut state);

    if visible.is_empty() {
        let msg = if app.store.notes.is_empty() {
            "no notes — press i to add"
        } else {
            "no matches"
        };
        draw_empty(f, area, msg);
    }
}

fn highlight_match(text: &str, _filter: &str) -> String {
    text.to_string()
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
            Span::raw(app.filter.clone()),
            Span::styled("█", Style::default().fg(Color::Yellow)),
            Span::raw("  "),
            Span::styled(
                "(filtering live — Enter to keep, Esc to cancel)",
                Style::default().fg(DIM),
            ),
        ]),
        _ => {
            let style = if app.status.contains("failed") || app.status.starts_with("error") {
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
        Mode::Normal => "hjkl move/switch  i add  dd del  x done  / filter  Esc clear filter  : cmd  e edit-note  q quit",
        Mode::Command => "Enter run  Esc cancel   commands: :todo :notes :new :delete :clear :w :q :help",
        Mode::Search => "type to filter both panes  Enter keep  Esc revert",
        Mode::Input => "Enter confirm  Esc cancel",
        Mode::NoteView => "Esc/Enter close  e edit",
        Mode::NoteEdit => "type to edit  Enter newline  Esc save & close",
    };
    f.render_widget(
        Paragraph::new(Span::styled(hints, Style::default().fg(DIM))),
        area,
    );
}

fn popup_style() -> Style {
    Style::default().bg(POPUP_BG).fg(POPUP_FG)
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
        .style(popup_style())
        .border_style(Style::default().fg(ACCENT).bg(POPUP_BG))
        .title(Span::styled(
            format!(" {} ", title),
            Style::default().fg(ACCENT).bg(POPUP_BG).bold(),
        ));
    let text = Line::from(vec![
        Span::styled(app.input_buffer.clone(), popup_style()),
        Span::styled("█", Style::default().fg(ACCENT).bg(POPUP_BG)),
    ]);
    f.render_widget(Paragraph::new(text).block(block).style(popup_style()), popup);
}

fn draw_note_view(f: &mut Frame, app: &App, area: Rect) {
    let Some(note) = app.store.notes.get(app.note_index) else {
        return;
    };
    let popup = centered_rect_abs(80, 70, area);
    f.render_widget(Clear, popup);
    let block = Block::default()
        .borders(Borders::ALL)
        .style(popup_style())
        .border_style(Style::default().fg(Color::Magenta).bg(POPUP_BG))
        .title(Span::styled(
            format!(" {} ", note.title),
            Style::default().fg(Color::Magenta).bg(POPUP_BG).bold(),
        ));
    let body = if note.body.is_empty() {
        String::from("(empty — press e to edit)")
    } else {
        note.body.clone()
    };
    let p = Paragraph::new(body)
        .block(block)
        .style(popup_style())
        .wrap(Wrap { trim: false });
    f.render_widget(p, popup);
}

fn draw_note_edit(f: &mut Frame, app: &App, area: Rect) {
    let title = app
        .editing_note_index
        .and_then(|i| app.store.notes.get(i))
        .map(|n| n.title.clone())
        .unwrap_or_else(|| String::from("note"));
    let popup = centered_rect_abs(80, 70, area);
    f.render_widget(Clear, popup);
    let block = Block::default()
        .borders(Borders::ALL)
        .style(popup_style())
        .border_style(Style::default().fg(Color::Yellow).bg(POPUP_BG))
        .title(Span::styled(
            format!(" editing: {} ", title),
            Style::default().fg(Color::Yellow).bg(POPUP_BG).bold(),
        ));
    let body = format!("{}█", app.note_buffer);
    let p = Paragraph::new(body)
        .block(block)
        .style(popup_style())
        .wrap(Wrap { trim: false });
    f.render_widget(p, popup);
}

fn first_line(s: &str) -> String {
    s.lines().next().unwrap_or("").trim().to_string()
}

fn centered_rect(percent_x: u16, height: u16, r: Rect) -> Rect {
    let h = height.min(r.height);
    let pad_top = (r.height.saturating_sub(h)) / 2;
    let v = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(pad_top),
            Constraint::Length(h),
            Constraint::Min(0),
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

fn centered_rect_abs(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
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

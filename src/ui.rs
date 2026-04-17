use crate::app::{App, InputTarget, Mode, Pane};
use crate::editor::{EditorMode, VimEditor};
use crate::storage::EventKind;
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
        Mode::History => draw_history(f, app, area),
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
            ListItem::new(Line::from(vec![
                Span::styled("● ", Style::default().fg(Color::Magenta)),
                Span::styled(
                    highlight_match(&n.title, &app.filter),
                    Style::default().fg(Color::White).bold(),
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
        Mode::Command => "Enter run  Esc cancel   commands: :todo :notes :new :delete :history :clear :w :q :help",
        Mode::Search => "type to filter both panes  Enter keep  Esc revert",
        Mode::Input => "Enter confirm  Esc cancel",
        Mode::NoteView => "Esc/Enter close  e edit",
        Mode::NoteEdit => "vim keys: hjkl move  i/a/o insert  dd del-line  yy p  u undo  v visual  :w save  :q cancel",
        Mode::History => "j/k scroll  gg/G top/bottom  Esc/q close",
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
    let Some(editor) = app.note_editor.as_ref() else {
        return;
    };
    let popup = centered_rect_abs(92, 88, area);
    f.render_widget(Clear, popup);

    let mode_label = match editor.mode {
        EditorMode::Normal => " NORMAL ",
        EditorMode::Insert => " INSERT ",
        EditorMode::Visual => {
            if editor.visual_linewise {
                " V-LINE "
            } else {
                " VISUAL "
            }
        }
        EditorMode::Command => " COMMAND ",
    };
    let mode_color = match editor.mode {
        EditorMode::Normal => Color::Cyan,
        EditorMode::Insert => Color::Green,
        EditorMode::Visual => Color::Magenta,
        EditorMode::Command => Color::Yellow,
    };
    let header = format!(" {} — {} ", title, editor.lines.len());
    let block = Block::default()
        .borders(Borders::ALL)
        .style(popup_style())
        .border_style(Style::default().fg(mode_color).bg(POPUP_BG))
        .title(Span::styled(
            header,
            Style::default().fg(mode_color).bg(POPUP_BG).bold(),
        ))
        .title_bottom(Line::from(vec![
            Span::styled(
                mode_label,
                Style::default().bg(mode_color).fg(Color::Black).bold(),
            ),
            Span::styled(
                format!(" {}:{} ", editor.row + 1, editor.col + 1),
                Style::default().fg(POPUP_FG).bg(POPUP_BG),
            ),
        ]));

    let inner = block.inner(popup);
    f.render_widget(block, popup);

    let status_h: u16 = 1;
    let body_h = inner.height.saturating_sub(status_h);
    let body_area = Rect {
        x: inner.x,
        y: inner.y,
        width: inner.width,
        height: body_h,
    };
    let status_area = Rect {
        x: inner.x,
        y: inner.y + body_h,
        width: inner.width,
        height: status_h,
    };

    editor.viewport_height.set(body_area.height as usize);
    update_scroll(editor, body_area.height as usize);
    let scroll = editor.scroll.get();

    let gutter_w = gutter_width(editor.lines.len());
    let text_x = body_area.x + gutter_w + 1;
    let text_w = body_area.width.saturating_sub(gutter_w + 1);

    let lines = render_editor_lines(editor, scroll, body_area.height as usize, text_w as usize);
    let p = Paragraph::new(lines).style(popup_style());
    let text_area = Rect {
        x: text_x,
        y: body_area.y,
        width: text_w,
        height: body_area.height,
    };
    f.render_widget(p, text_area);

    let gutter = render_gutter(editor, scroll, body_area.height as usize, gutter_w as usize);
    let gutter_area = Rect {
        x: body_area.x,
        y: body_area.y,
        width: gutter_w,
        height: body_area.height,
    };
    f.render_widget(
        Paragraph::new(gutter).style(Style::default().fg(DIM).bg(POPUP_BG)),
        gutter_area,
    );

    let status_line = build_status_line(editor);
    f.render_widget(
        Paragraph::new(status_line).style(popup_style()),
        status_area,
    );

    if editor.mode == EditorMode::Insert {
        let screen_row = editor.row.saturating_sub(scroll);
        if (screen_row as u16) < body_area.height {
            let col_offset = editor.col as u16;
            if col_offset < text_w {
                f.set_cursor_position((text_x + col_offset, body_area.y + screen_row as u16));
            }
        }
    }
}

fn update_scroll(editor: &VimEditor, height: usize) {
    if height == 0 {
        return;
    }
    let mut scroll = editor.scroll.get();
    if editor.row < scroll {
        scroll = editor.row;
    } else if editor.row >= scroll + height {
        scroll = editor.row + 1 - height;
    }
    editor.scroll.set(scroll);
}

fn gutter_width(n_lines: usize) -> u16 {
    let digits = n_lines.max(1).to_string().len() as u16;
    digits.max(3) + 1
}

fn render_gutter(editor: &VimEditor, scroll: usize, height: usize, width: usize) -> Vec<Line<'static>> {
    let mut out = Vec::with_capacity(height);
    for i in 0..height {
        let r = scroll + i;
        if r >= editor.lines.len() {
            out.push(Line::from(Span::styled(
                " ".repeat(width),
                Style::default().bg(POPUP_BG),
            )));
            continue;
        }
        let num = if r == editor.row {
            format!("{:>width$} ", r + 1, width = width - 1)
        } else {
            let rel = r.abs_diff(editor.row);
            format!("{:>width$} ", rel, width = width - 1)
        };
        let style = if r == editor.row {
            Style::default().fg(Color::Yellow).bg(POPUP_BG).bold()
        } else {
            Style::default().fg(DIM).bg(POPUP_BG)
        };
        out.push(Line::from(Span::styled(num, style)));
    }
    out
}

fn render_editor_lines(
    editor: &VimEditor,
    scroll: usize,
    height: usize,
    width: usize,
) -> Vec<Line<'static>> {
    let visual = editor.visual_range();
    let mut out = Vec::with_capacity(height);
    for i in 0..height {
        let r = scroll + i;
        if r >= editor.lines.len() {
            out.push(Line::from(Span::styled(
                "~".to_string(),
                Style::default().fg(DIM).bg(POPUP_BG),
            )));
            continue;
        }
        let raw = &editor.lines[r];
        let chars: Vec<char> = raw.chars().collect();
        let mut spans: Vec<Span<'static>> = Vec::new();
        let cursor_here = r == editor.row && editor.mode != EditorMode::Insert;
        let total_chars = chars.len();
        let visible_len = total_chars.max(1);
        for (c, ch) in chars.iter().enumerate().take(width.max(1)) {
            let selected = visual
                .map(|((sr, sc), (er, ec))| {
                    (r > sr || (r == sr && c >= sc)) && (r < er || (r == er && c <= ec))
                })
                .unwrap_or(false);
            let is_cursor = cursor_here && c == editor.col;
            let style = if is_cursor {
                Style::default().bg(Color::White).fg(Color::Black)
            } else if selected {
                Style::default().bg(Color::DarkGray).fg(POPUP_FG)
            } else {
                popup_style()
            };
            spans.push(Span::styled(ch.to_string(), style));
        }
        if cursor_here && editor.col >= total_chars {
            spans.push(Span::styled(
                " ".to_string(),
                Style::default().bg(Color::White).fg(Color::Black),
            ));
        }
        if total_chars == 0 && !cursor_here {
            spans.push(Span::styled(String::new(), popup_style()));
        }
        let _ = visible_len;
        out.push(Line::from(spans));
    }
    out
}

fn build_status_line(editor: &VimEditor) -> Line<'static> {
    match editor.mode {
        EditorMode::Command => Line::from(vec![
            Span::styled(":", Style::default().fg(Color::Yellow).bg(POPUP_BG).bold()),
            Span::styled(editor.command_buffer.clone(), popup_style()),
            Span::styled("_", Style::default().fg(Color::Yellow).bg(POPUP_BG)),
        ]),
        _ => {
            let hint = match editor.mode {
                EditorMode::Normal => "i insert  v visual  :w save  :q cancel  u undo  dd yy p",
                EditorMode::Insert => "Esc normal",
                EditorMode::Visual => "d cut  y yank  c change  Esc normal",
                EditorMode::Command => "",
            };
            let text = if editor.status.is_empty() {
                hint.to_string()
            } else {
                editor.status.clone()
            };
            Line::from(Span::styled(text, Style::default().fg(DIM).bg(POPUP_BG)))
        }
    }
}

fn draw_history(f: &mut Frame, app: &App, area: Rect) {
    let popup = centered_rect_abs(80, 80, area);
    f.render_widget(Clear, popup);
    let block = Block::default()
        .borders(Borders::ALL)
        .style(popup_style())
        .border_style(Style::default().fg(ACCENT).bg(POPUP_BG))
        .title(Span::styled(
            format!(" history ({} events) ", app.history_events.len()),
            Style::default().fg(ACCENT).bg(POPUP_BG).bold(),
        ));

    if app.history_events.is_empty() {
        let p = Paragraph::new(Span::styled(
            "(no events yet — create a todo or note)",
            Style::default().fg(DIM).bg(POPUP_BG),
        ))
        .block(block)
        .style(popup_style());
        f.render_widget(p, popup);
        return;
    }

    let items: Vec<ListItem> = app
        .history_events
        .iter()
        .map(|e| {
            let ts = e.ts.format("%Y-%m-%d %H:%M:%S").to_string();
            let (label, color, detail) = render_event_kind(&e.kind);
            ListItem::new(Line::from(vec![
                Span::styled(ts, Style::default().fg(DIM).bg(POPUP_BG)),
                Span::styled("  ", popup_style()),
                Span::styled(label, Style::default().fg(color).bg(POPUP_BG).bold()),
                Span::styled("  ", popup_style()),
                Span::styled(detail, popup_style()),
            ]))
        })
        .collect();

    let list = List::new(items)
        .block(block)
        .style(popup_style())
        .highlight_style(
            Style::default()
                .bg(ACCENT)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▶ ");

    let mut state = ListState::default();
    let selected = app
        .history_scroll
        .min(app.history_events.len().saturating_sub(1));
    state.select(Some(selected));
    f.render_stateful_widget(list, popup, &mut state);
}

fn render_event_kind(kind: &EventKind) -> (&'static str, Color, String) {
    match kind {
        EventKind::TodoCreated { title, .. } => {
            ("TODO CREATED", Color::Green, format!("\"{}\"", title))
        }
        EventKind::TodoRenamed { title, .. } => {
            ("TODO RENAMED", Color::Yellow, format!("\"{}\"", title))
        }
        EventKind::TodoToggled { done, .. } => (
            "TODO TOGGLED",
            Color::Blue,
            format!("done={}", done),
        ),
        EventKind::TodoDeleted { .. } => ("TODO DELETED", Color::Red, String::new()),
        EventKind::NoteCreated { title, .. } => {
            ("NOTE CREATED", Color::Green, format!("\"{}\"", title))
        }
        EventKind::NoteRenamed { title, .. } => {
            ("NOTE RENAMED", Color::Yellow, format!("\"{}\"", title))
        }
        EventKind::NoteEdited { body, .. } => (
            "NOTE EDITED",
            Color::Blue,
            format!("{} chars", body.chars().count()),
        ),
        EventKind::NoteDeleted { .. } => ("NOTE DELETED", Color::Red, String::new()),
    }
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

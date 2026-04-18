use crate::editor::{EditorExit, VimEditor};
use crate::ui;
use anyhow::Result;
use chrono::Utc;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::{backend::Backend, Terminal};
use std::time::Duration;
use todo_store::{self as storage, Event as HistoryEvent, EventKind, Note, Store, Todo};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Pane {
    Todos,
    Notes,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Normal,
    Command,
    Search,
    Input,
    NoteView,
    NoteEdit,
    History,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum InputTarget {
    NewTodo,
    NewNote,
    RenameTodo,
    RenameNote,
}

pub struct App {
    pub store: Store,
    pub focus: Pane,
    pub mode: Mode,
    pub todo_index: usize,
    pub note_index: usize,
    pub status: String,
    pub command_buffer: String,
    pub input_buffer: String,
    pub input_target: Option<InputTarget>,
    pub filter: String,
    pub filter_backup: String,
    pub note_editor: Option<VimEditor>,
    pub editing_note_index: Option<usize>,
    pub history_events: Vec<HistoryEvent>,
    pub history_scroll: usize,
    pub should_quit: bool,
    pub pending_d: bool,
    pub pending_g: bool,
}

impl App {
    pub fn new(store: Store) -> Self {
        Self {
            store,
            focus: Pane::Todos,
            mode: Mode::Normal,
            todo_index: 0,
            note_index: 0,
            status: String::from("welcome — :help for commands, :q to quit"),
            command_buffer: String::new(),
            input_buffer: String::new(),
            input_target: None,
            filter: String::new(),
            filter_backup: String::new(),
            note_editor: None,
            editing_note_index: None,
            history_events: Vec::new(),
            history_scroll: 0,
            should_quit: false,
            pending_d: false,
            pending_g: false,
        }
    }

    pub fn run<B: Backend>(&mut self, terminal: &mut Terminal<B>) -> Result<()> {
        loop {
            terminal.draw(|f| ui::draw(f, self))?;
            if event::poll(Duration::from_millis(200))? {
                if let Event::Key(key) = event::read()? {
                    if key.kind == KeyEventKind::Press {
                        self.handle_key(key)?;
                    }
                }
            }
            if self.should_quit {
                self.save();
                break;
            }
        }
        Ok(())
    }

    fn handle_key(&mut self, key: KeyEvent) -> Result<()> {
        match self.mode {
            Mode::Normal => self.handle_normal(key),
            Mode::Command => self.handle_command(key),
            Mode::Search => self.handle_search(key),
            Mode::Input => self.handle_input(key),
            Mode::NoteView => self.handle_note_view(key),
            Mode::NoteEdit => self.handle_note_edit(key),
            Mode::History => self.handle_history(key),
        }
    }

    fn handle_normal(&mut self, key: KeyEvent) -> Result<()> {
        let pending_d = std::mem::replace(&mut self.pending_d, false);
        let pending_g = std::mem::replace(&mut self.pending_g, false);

        match key.code {
            KeyCode::Char('q') if key.modifiers == KeyModifiers::NONE => {
                self.should_quit = true;
            }
            KeyCode::Char('c') if key.modifiers == KeyModifiers::CONTROL => {
                self.should_quit = true;
            }
            KeyCode::Esc => {
                if !self.filter.is_empty() {
                    self.filter.clear();
                    self.snap_selection();
                    self.status = String::from("filter cleared");
                }
            }
            KeyCode::Char(':') => {
                self.mode = Mode::Command;
                self.command_buffer.clear();
            }
            KeyCode::Char('/') => {
                self.filter_backup = self.filter.clone();
                self.filter.clear();
                self.mode = Mode::Search;
                self.snap_selection();
            }
            KeyCode::Char('h') | KeyCode::Left => self.focus = Pane::Todos,
            KeyCode::Char('l') | KeyCode::Right => self.focus = Pane::Notes,
            KeyCode::Char('j') | KeyCode::Down => self.move_selection(1),
            KeyCode::Char('k') | KeyCode::Up => self.move_selection(-1),
            KeyCode::Char('g') => {
                if pending_g {
                    if let Some(&first) = self.visible_indices(self.focus).first() {
                        self.set_index(first);
                    }
                } else {
                    self.pending_g = true;
                }
            }
            KeyCode::Char('G') => {
                if let Some(&last) = self.visible_indices(self.focus).last() {
                    self.set_index(last);
                }
            }
            KeyCode::Char('i') | KeyCode::Char('a') | KeyCode::Char('o') => {
                self.start_input(match self.focus {
                    Pane::Todos => InputTarget::NewTodo,
                    Pane::Notes => InputTarget::NewNote,
                });
            }
            KeyCode::Char('r') => match self.focus {
                Pane::Todos => {
                    if let Some(t) = self.store.todos.get(self.todo_index) {
                        self.input_buffer = t.title.clone();
                        self.start_input_keep_buffer(InputTarget::RenameTodo);
                    }
                }
                Pane::Notes => {
                    if let Some(n) = self.store.notes.get(self.note_index) {
                        self.input_buffer = n.title.clone();
                        self.start_input_keep_buffer(InputTarget::RenameNote);
                    }
                }
            },
            KeyCode::Char('d') => {
                if pending_d {
                    self.delete_current();
                } else {
                    self.pending_d = true;
                }
            }
            KeyCode::Char('x') | KeyCode::Char(' ') => {
                if self.focus == Pane::Todos {
                    self.toggle_done();
                }
            }
            KeyCode::Enter => match self.focus {
                Pane::Todos => self.toggle_done(),
                Pane::Notes => self.open_note_view(),
            },
            KeyCode::Char('e') => {
                if self.focus == Pane::Notes {
                    self.open_note_edit();
                }
            }
            KeyCode::Tab => {
                self.focus = match self.focus {
                    Pane::Todos => Pane::Notes,
                    Pane::Notes => Pane::Todos,
                };
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_command(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Esc => {
                self.mode = Mode::Normal;
                self.command_buffer.clear();
            }
            KeyCode::Enter => {
                let cmd = std::mem::take(&mut self.command_buffer);
                self.mode = Mode::Normal;
                self.run_command(cmd.trim());
            }
            KeyCode::Backspace => {
                self.command_buffer.pop();
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.command_buffer.push(c);
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_search(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Esc => {
                self.filter = std::mem::take(&mut self.filter_backup);
                self.snap_selection();
                self.mode = Mode::Normal;
                self.status = String::from("search cancelled");
            }
            KeyCode::Enter => {
                self.filter_backup.clear();
                self.mode = Mode::Normal;
                self.status = if self.filter.is_empty() {
                    String::from("filter cleared")
                } else {
                    format!("filter: {}", self.filter)
                };
            }
            KeyCode::Backspace => {
                self.filter.pop();
                self.snap_selection();
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.filter.push(c);
                self.snap_selection();
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_input(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Esc => {
                self.mode = Mode::Normal;
                self.input_buffer.clear();
                self.input_target = None;
            }
            KeyCode::Enter => {
                let value = std::mem::take(&mut self.input_buffer);
                let target = self.input_target.take();
                self.mode = Mode::Normal;
                if let Some(t) = target {
                    self.commit_input(t, value);
                }
            }
            KeyCode::Backspace => {
                self.input_buffer.pop();
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.input_buffer.push(c);
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_note_view(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Enter => {
                self.mode = Mode::Normal;
            }
            KeyCode::Char('e') => {
                self.open_note_edit();
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_note_edit(&mut self, key: KeyEvent) -> Result<()> {
        let Some(editor) = self.note_editor.as_mut() else {
            self.mode = Mode::Normal;
            return Ok(());
        };
        editor.handle_key(key);
        if let Some(exit) = editor.exit {
            self.finish_note_edit(exit);
        }
        Ok(())
    }

    fn finish_note_edit(&mut self, exit: EditorExit) {
        let editor = match self.note_editor.take() {
            Some(e) => e,
            None => {
                self.mode = Mode::Normal;
                return;
            }
        };
        let idx = self.editing_note_index.take();
        match exit {
            EditorExit::Save => {
                let body = editor.body();
                let mut edited: Option<EventKind> = None;
                if let Some(i) = idx {
                    if let Some(note) = self.store.notes.get_mut(i) {
                        note.body = body;
                        note.updated_at = Utc::now();
                        edited = Some(EventKind::NoteEdited {
                            id: note.id,
                            body: note.body.clone(),
                        });
                    }
                }
                self.save();
                if let Some(kind) = edited {
                    self.log_event(kind);
                }
                self.status = String::from("note saved");
            }
            EditorExit::Cancel => {
                self.status = String::from("edit cancelled");
            }
        }
        self.mode = Mode::Normal;
    }

    fn run_command(&mut self, cmd: &str) {
        if cmd.is_empty() {
            return;
        }
        let mut parts = cmd.splitn(2, char::is_whitespace);
        let head = parts.next().unwrap_or("");
        let rest = parts.next().unwrap_or("").trim();
        match head {
            "q" | "quit" | "wq" | "x" => self.should_quit = true,
            "w" | "write" => {
                self.save();
                self.status = String::from("saved");
            }
            "todo" | "todos" => {
                self.focus = Pane::Todos;
                self.status = String::from("focus: todos");
            }
            "note" | "notes" => {
                self.focus = Pane::Notes;
                self.status = String::from("focus: notes");
            }
            "new" => {
                if rest.is_empty() {
                    self.start_input(match self.focus {
                        Pane::Todos => InputTarget::NewTodo,
                        Pane::Notes => InputTarget::NewNote,
                    });
                } else {
                    self.commit_input(
                        match self.focus {
                            Pane::Todos => InputTarget::NewTodo,
                            Pane::Notes => InputTarget::NewNote,
                        },
                        rest.to_string(),
                    );
                }
            }
            "delete" | "rm" => self.delete_current(),
            "toggle" => self.toggle_done(),
            "history" | "hist" | "log" => self.open_history(),
            "clear" | "nofilter" | "noh" | "nohlsearch" => {
                self.filter.clear();
                self.snap_selection();
                self.status = String::from("filter cleared");
            }
            "help" | "h" => {
                self.status = String::from(
                    "keys: hjkl move/switch | i add | dd delete | x toggle | / filter | : cmd | e edit note | :history",
                );
            }
            other => {
                self.status = format!("unknown command: {}", other);
            }
        }
    }

    fn commit_input(&mut self, target: InputTarget, value: String) {
        let value = value.trim().to_string();
        if value.is_empty() {
            self.status = String::from("cancelled (empty)");
            return;
        }
        match target {
            InputTarget::NewTodo => {
                let todo = Todo::new(value);
                let event = EventKind::TodoCreated {
                    id: todo.id,
                    title: todo.title.clone(),
                };
                self.store.todos.push(todo);
                self.todo_index = self.store.todos.len() - 1;
                self.focus = Pane::Todos;
                self.save();
                self.log_event(event);
                self.status = String::from("todo added");
            }
            InputTarget::NewNote => {
                let note = Note::new(value);
                let event = EventKind::NoteCreated {
                    id: note.id,
                    title: note.title.clone(),
                };
                self.store.notes.push(note);
                self.note_index = self.store.notes.len() - 1;
                self.focus = Pane::Notes;
                self.save();
                self.log_event(event);
                self.status = String::from("note added");
            }
            InputTarget::RenameTodo => {
                let mut event = None;
                if let Some(t) = self.store.todos.get_mut(self.todo_index) {
                    t.title = value;
                    event = Some(EventKind::TodoRenamed {
                        id: t.id,
                        title: t.title.clone(),
                    });
                }
                if let Some(e) = event {
                    self.save();
                    self.log_event(e);
                    self.status = String::from("todo renamed");
                }
            }
            InputTarget::RenameNote => {
                let mut event = None;
                if let Some(n) = self.store.notes.get_mut(self.note_index) {
                    n.title = value;
                    n.updated_at = Utc::now();
                    event = Some(EventKind::NoteRenamed {
                        id: n.id,
                        title: n.title.clone(),
                    });
                }
                if let Some(e) = event {
                    self.save();
                    self.log_event(e);
                    self.status = String::from("note renamed");
                }
            }
        }
    }

    fn start_input(&mut self, target: InputTarget) {
        self.input_buffer.clear();
        self.input_target = Some(target);
        self.mode = Mode::Input;
    }

    fn start_input_keep_buffer(&mut self, target: InputTarget) {
        self.input_target = Some(target);
        self.mode = Mode::Input;
    }

    fn delete_current(&mut self) {
        let mut event = None;
        match self.focus {
            Pane::Todos => {
                if let Some(t) = self.store.todos.get_mut(self.todo_index) {
                    if t.deleted_at.is_none() {
                        t.deleted_at = Some(Utc::now());
                        event = Some((EventKind::TodoDeleted { id: t.id }, "todo deleted"));
                    }
                }
            }
            Pane::Notes => {
                if let Some(n) = self.store.notes.get_mut(self.note_index) {
                    if n.deleted_at.is_none() {
                        n.deleted_at = Some(Utc::now());
                        event = Some((EventKind::NoteDeleted { id: n.id }, "note deleted"));
                    }
                }
            }
        }
        if let Some((kind, msg)) = event {
            self.save();
            self.log_event(kind);
            self.snap_selection();
            self.status = String::from(msg);
        }
    }

    fn toggle_done(&mut self) {
        let mut event = None;
        if let Some(t) = self.store.todos.get_mut(self.todo_index) {
            t.done = !t.done;
            event = Some(EventKind::TodoToggled {
                id: t.id,
                done: t.done,
            });
        }
        if let Some(e) = event {
            self.save();
            self.log_event(e);
        }
    }

    fn open_note_view(&mut self) {
        if self.store.notes.get(self.note_index).is_some() {
            self.mode = Mode::NoteView;
        }
    }

    fn open_note_edit(&mut self) {
        if let Some(note) = self.store.notes.get(self.note_index) {
            self.note_editor = Some(VimEditor::new(&note.body));
            self.editing_note_index = Some(self.note_index);
            self.mode = Mode::NoteEdit;
            self.status = String::from("editing note — :w save  :q cancel  i insert  Esc normal");
        }
    }

    fn move_selection(&mut self, delta: i64) {
        let visible = self.visible_indices(self.focus);
        if visible.is_empty() {
            return;
        }
        let cur = self.current_index();
        let pos = visible.iter().position(|&i| i == cur).unwrap_or(0) as i64;
        let new_pos = (pos + delta).clamp(0, visible.len() as i64 - 1) as usize;
        self.set_index(visible[new_pos]);
    }

    fn current_index(&self) -> usize {
        match self.focus {
            Pane::Todos => self.todo_index,
            Pane::Notes => self.note_index,
        }
    }

    fn set_index(&mut self, idx: usize) {
        match self.focus {
            Pane::Todos => self.todo_index = idx,
            Pane::Notes => self.note_index = idx,
        }
    }

    pub fn visible_todo_indices(&self) -> Vec<usize> {
        let f = self.filter.to_lowercase();
        self.store
            .todos
            .iter()
            .enumerate()
            .filter(|(_, t)| t.deleted_at.is_none())
            .filter(|(_, t)| f.is_empty() || t.title.to_lowercase().contains(&f))
            .map(|(i, _)| i)
            .collect()
    }

    pub fn visible_note_indices(&self) -> Vec<usize> {
        let f = self.filter.to_lowercase();
        self.store
            .notes
            .iter()
            .enumerate()
            .filter(|(_, n)| n.deleted_at.is_none())
            .filter(|(_, n)| {
                f.is_empty()
                    || n.title.to_lowercase().contains(&f)
                    || n.body.to_lowercase().contains(&f)
            })
            .map(|(i, _)| i)
            .collect()
    }

    fn visible_indices(&self, pane: Pane) -> Vec<usize> {
        match pane {
            Pane::Todos => self.visible_todo_indices(),
            Pane::Notes => self.visible_note_indices(),
        }
    }

    fn snap_selection(&mut self) {
        let todo_visible = self.visible_todo_indices();
        if !todo_visible.contains(&self.todo_index) {
            self.todo_index = todo_visible.first().copied().unwrap_or(0);
        }
        let note_visible = self.visible_note_indices();
        if !note_visible.contains(&self.note_index) {
            self.note_index = note_visible.first().copied().unwrap_or(0);
        }
    }

    fn save(&mut self) {
        if let Err(e) = storage::save(&self.store) {
            self.status = format!("save failed: {}", e);
        }
    }

    fn log_event(&mut self, kind: EventKind) {
        if let Err(e) = storage::append_event(kind) {
            self.status = format!("event log failed: {}", e);
        }
    }

    fn handle_history(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                self.mode = Mode::Normal;
                self.history_events.clear();
                self.history_scroll = 0;
            }
            KeyCode::Char('j') | KeyCode::Down => {
                if self.history_scroll + 1 < self.history_events.len() {
                    self.history_scroll += 1;
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.history_scroll = self.history_scroll.saturating_sub(1);
            }
            KeyCode::Char('g') => {
                self.history_scroll = 0;
            }
            KeyCode::Char('G') => {
                self.history_scroll = self.history_events.len().saturating_sub(1);
            }
            _ => {}
        }
        Ok(())
    }

    fn open_history(&mut self) {
        match storage::load_events() {
            Ok(events) => {
                self.history_events = events;
                self.history_scroll = self.history_events.len().saturating_sub(1);
                self.mode = Mode::History;
            }
            Err(e) => {
                self.status = format!("history load failed: {}", e);
            }
        }
    }
}

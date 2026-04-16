use crate::storage::{self, Note, Store, Todo};
use crate::ui;
use anyhow::Result;
use chrono::Utc;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::{backend::Backend, Terminal};
use std::time::Duration;

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
    pub search_buffer: String,
    pub input_buffer: String,
    pub input_target: Option<InputTarget>,
    pub last_search: Option<String>,
    pub note_buffer: String,
    pub editing_note_index: Option<usize>,
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
            search_buffer: String::new(),
            input_buffer: String::new(),
            input_target: None,
            last_search: None,
            note_buffer: String::new(),
            editing_note_index: None,
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
            KeyCode::Char(':') => {
                self.mode = Mode::Command;
                self.command_buffer.clear();
            }
            KeyCode::Char('/') => {
                self.mode = Mode::Search;
                self.search_buffer.clear();
            }
            KeyCode::Char('h') | KeyCode::Left => self.focus = Pane::Todos,
            KeyCode::Char('l') | KeyCode::Right => self.focus = Pane::Notes,
            KeyCode::Char('j') | KeyCode::Down => self.move_selection(1),
            KeyCode::Char('k') | KeyCode::Up => self.move_selection(-1),
            KeyCode::Char('g') => {
                if pending_g {
                    self.set_index(0);
                } else {
                    self.pending_g = true;
                }
            }
            KeyCode::Char('G') => {
                let len = self.current_len();
                if len > 0 {
                    self.set_index(len - 1);
                }
            }
            KeyCode::Char('n') => self.repeat_search(true),
            KeyCode::Char('N') => self.repeat_search(false),
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
                self.mode = Mode::Normal;
                self.search_buffer.clear();
            }
            KeyCode::Enter => {
                let q = std::mem::take(&mut self.search_buffer);
                self.mode = Mode::Normal;
                if !q.is_empty() {
                    self.last_search = Some(q.clone());
                    self.jump_to_match(&q, true, true);
                }
            }
            KeyCode::Backspace => {
                self.search_buffer.pop();
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.search_buffer.push(c);
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
        match key.code {
            KeyCode::Esc => {
                if let Some(idx) = self.editing_note_index.take() {
                    if let Some(note) = self.store.notes.get_mut(idx) {
                        note.body = std::mem::take(&mut self.note_buffer);
                        note.updated_at = Utc::now();
                    }
                }
                self.save();
                self.mode = Mode::Normal;
                self.status = String::from("note saved");
            }
            KeyCode::Enter => {
                self.note_buffer.push('\n');
            }
            KeyCode::Tab => {
                self.note_buffer.push_str("    ");
            }
            KeyCode::Backspace => {
                self.note_buffer.pop();
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.note_buffer.push(c);
            }
            _ => {}
        }
        Ok(())
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
            "help" | "h" => {
                self.status = String::from(
                    "keys: hjkl move/switch | i add | dd delete | x toggle | / search | : cmd | e edit note",
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
                self.store.todos.push(Todo::new(value));
                self.todo_index = self.store.todos.len() - 1;
                self.focus = Pane::Todos;
                self.save();
                self.status = String::from("todo added");
            }
            InputTarget::NewNote => {
                self.store.notes.push(Note::new(value));
                self.note_index = self.store.notes.len() - 1;
                self.focus = Pane::Notes;
                self.save();
                self.status = String::from("note added");
            }
            InputTarget::RenameTodo => {
                if let Some(t) = self.store.todos.get_mut(self.todo_index) {
                    t.title = value;
                    self.save();
                    self.status = String::from("todo renamed");
                }
            }
            InputTarget::RenameNote => {
                if let Some(n) = self.store.notes.get_mut(self.note_index) {
                    n.title = value;
                    n.updated_at = Utc::now();
                    self.save();
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
        match self.focus {
            Pane::Todos => {
                if self.todo_index < self.store.todos.len() {
                    self.store.todos.remove(self.todo_index);
                    if self.todo_index >= self.store.todos.len() && self.todo_index > 0 {
                        self.todo_index -= 1;
                    }
                    self.save();
                    self.status = String::from("todo deleted");
                }
            }
            Pane::Notes => {
                if self.note_index < self.store.notes.len() {
                    self.store.notes.remove(self.note_index);
                    if self.note_index >= self.store.notes.len() && self.note_index > 0 {
                        self.note_index -= 1;
                    }
                    self.save();
                    self.status = String::from("note deleted");
                }
            }
        }
    }

    fn toggle_done(&mut self) {
        if let Some(t) = self.store.todos.get_mut(self.todo_index) {
            t.done = !t.done;
            self.save();
        }
    }

    fn open_note_view(&mut self) {
        if self.store.notes.get(self.note_index).is_some() {
            self.mode = Mode::NoteView;
        }
    }

    fn open_note_edit(&mut self) {
        if let Some(note) = self.store.notes.get(self.note_index) {
            self.note_buffer = note.body.clone();
            self.editing_note_index = Some(self.note_index);
            self.mode = Mode::NoteEdit;
            self.status = String::from("editing note — Esc to save");
        }
    }

    fn move_selection(&mut self, delta: i64) {
        let len = self.current_len();
        if len == 0 {
            return;
        }
        let cur = self.current_index() as i64;
        let next = (cur + delta).clamp(0, len as i64 - 1) as usize;
        self.set_index(next);
    }

    fn current_len(&self) -> usize {
        match self.focus {
            Pane::Todos => self.store.todos.len(),
            Pane::Notes => self.store.notes.len(),
        }
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

    fn item_matches(&self, idx: usize, pattern_lc: &str) -> bool {
        match self.focus {
            Pane::Todos => self
                .store
                .todos
                .get(idx)
                .map(|t| t.title.to_lowercase().contains(pattern_lc))
                .unwrap_or(false),
            Pane::Notes => self
                .store
                .notes
                .get(idx)
                .map(|n| {
                    n.title.to_lowercase().contains(pattern_lc)
                        || n.body.to_lowercase().contains(pattern_lc)
                })
                .unwrap_or(false),
        }
    }

    fn jump_to_match(&mut self, pattern: &str, forward: bool, from_current: bool) {
        let pattern_lc = pattern.to_lowercase();
        let len = self.current_len();
        if len == 0 {
            self.status = String::from("no items");
            return;
        }
        let start = self.current_index();
        for offset in 0..len {
            let idx = if forward {
                (start + if from_current { 0 } else { 1 } + offset) % len
            } else {
                let step = if from_current { 0 } else { 1 } + offset;
                (start + len - (step % len)) % len
            };
            if self.item_matches(idx, &pattern_lc) {
                self.set_index(idx);
                self.status = format!("/{}", pattern);
                return;
            }
        }
        self.status = format!("pattern not found: {}", pattern);
    }

    fn repeat_search(&mut self, forward: bool) {
        if let Some(p) = self.last_search.clone() {
            self.jump_to_match(&p, forward, false);
        }
    }

    fn save(&mut self) {
        if let Err(e) = storage::save(&self.store) {
            self.status = format!("save failed: {}", e);
        }
    }
}

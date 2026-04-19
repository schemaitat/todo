use crate::editor::{EditorExit, VimEditor};
use crate::ui;
use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use crossterm::event::{self, Event as CEvent, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::{backend::Backend, Terminal};
use std::time::Duration;
use todo_api_client::{ApiError, Client, Context, Event as HistoryEvent, Note, PatchedNote, Todo};

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
    ContextBrowser,
    MovePicker,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum InputTarget {
    NewTodo,
    NewNote,
    RenameTodo,
    RenameNote,
    NewContext,
}

pub struct App {
    pub client: Client,
    pub contexts: Vec<Context>,
    pub active_context: Context,
    pub todos: Vec<Todo>,
    pub notes: Vec<Note>,
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
    pub editing_note_version: Option<DateTime<Utc>>,
    pub history_events: Vec<HistoryEvent>,
    pub history_scroll: usize,
    pub should_quit: bool,
    pub pending_d: bool,
    pub pending_g: bool,
    pub pending_c: bool,
    pub offline: bool,
    pub api_url: String,
    pub context_index: usize,
    pub suggestions: Vec<String>,
    pub suggestion_index: usize,
    pub move_source: Option<(Pane, usize)>,
    pub move_picker_index: usize,
}

impl App {
    /// Build the app by fetching contexts and the active context's todos/notes from the API.
    pub fn bootstrap(client: Client) -> Result<Self> {
        let contexts = client.list_contexts()?;
        if contexts.is_empty() {
            return Err(anyhow!(
                "no contexts on server — bootstrap the API first (it seeds an `inbox` context)"
            ));
        }
        let active_slug = client.active_context_slug().to_string();
        let active_context = contexts
            .iter()
            .find(|c| c.slug == active_slug)
            .cloned()
            .unwrap_or_else(|| contexts[0].clone());

        let todos = client.list_todos(&active_context.slug)?;
        let notes = client.list_notes(&active_context.slug)?;

        let status = format!(
            "welcome — context [{}]  :help for commands, :q to quit",
            active_context.slug
        );
        let api_url = client.base_url().trim_end_matches('/').to_string();
        let mut app = Self {
            client,
            contexts,
            active_context,
            todos,
            notes,
            focus: Pane::Todos,
            mode: Mode::Normal,
            todo_index: 0,
            note_index: 0,
            status,
            command_buffer: String::new(),
            input_buffer: String::new(),
            input_target: None,
            filter: String::new(),
            filter_backup: String::new(),
            note_editor: None,
            editing_note_index: None,
            editing_note_version: None,
            history_events: Vec::new(),
            history_scroll: 0,
            should_quit: false,
            pending_d: false,
            pending_g: false,
            pending_c: false,
            suggestions: Vec::new(),
            suggestion_index: 0,
            move_source: None,
            move_picker_index: 0,
            offline: false,
            api_url,
            context_index: 0,
        };
        app.client.set_active_context(&app.active_context.slug);
        app.snap_selection();
        Ok(app)
    }

    pub fn run<B: Backend>(&mut self, terminal: &mut Terminal<B>) -> Result<()> {
        loop {
            terminal.draw(|f| ui::draw(f, self))?;
            if event::poll(Duration::from_millis(200))? {
                if let CEvent::Key(key) = event::read()? {
                    if key.kind == KeyEventKind::Press {
                        self.handle_key(key)?;
                    }
                }
            }
            if self.should_quit {
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
            Mode::ContextBrowser => self.handle_context_browser(key),
            Mode::MovePicker => self.handle_move_picker(key),
        }
    }

    fn handle_normal(&mut self, key: KeyEvent) -> Result<()> {
        let pending_d = std::mem::replace(&mut self.pending_d, false);
        let pending_g = std::mem::replace(&mut self.pending_g, false);
        let pending_c = std::mem::replace(&mut self.pending_c, false);

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
                    if let Some(t) = self.todos.get(self.todo_index) {
                        self.input_buffer = t.title.clone();
                        self.start_input_keep_buffer(InputTarget::RenameTodo);
                    }
                }
                Pane::Notes => {
                    if let Some(n) = self.notes.get(self.note_index) {
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
            KeyCode::Char('m') if key.modifiers == KeyModifiers::NONE => {
                self.open_move_picker();
            }
            KeyCode::Char('c') if key.modifiers == KeyModifiers::NONE => {
                if pending_c {
                    self.cycle_context();
                } else {
                    self.pending_c = true;
                }
            }
            KeyCode::Char('C') => {
                self.open_context_browser();
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
                self.suggestions.clear();
                self.suggestion_index = 0;
            }
            KeyCode::Enter => {
                let cmd = std::mem::take(&mut self.command_buffer);
                self.mode = Mode::Normal;
                self.suggestions.clear();
                self.suggestion_index = 0;
                self.run_command(cmd.trim());
            }
            KeyCode::Tab => {
                if !self.suggestions.is_empty() {
                    self.command_buffer = self.suggestions[self.suggestion_index].clone();
                    self.update_suggestions();
                }
            }
            KeyCode::Up => {
                if !self.suggestions.is_empty() {
                    self.suggestion_index = if self.suggestion_index == 0 {
                        self.suggestions.len() - 1
                    } else {
                        self.suggestion_index - 1
                    };
                }
            }
            KeyCode::Down => {
                if !self.suggestions.is_empty() {
                    self.suggestion_index = (self.suggestion_index + 1) % self.suggestions.len();
                }
            }
            KeyCode::Backspace => {
                self.command_buffer.pop();
                self.update_suggestions();
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.command_buffer.push(c);
                self.update_suggestions();
            }
            _ => {}
        }
        Ok(())
    }

    fn update_suggestions(&mut self) {
        self.suggestions = self.compute_suggestions();
        if self.suggestion_index >= self.suggestions.len() {
            self.suggestion_index = 0;
        }
    }

    fn compute_suggestions(&self) -> Vec<String> {
        let buf = self.command_buffer.trim_start();
        if buf.is_empty() {
            return Vec::new();
        }
        let buf_lower = buf.to_lowercase();

        let mut candidates: Vec<String> = vec![
            "clear",
            "context",
            "ctx",
            "ctx delete",
            "ctx new",
            "delete",
            "h",
            "help",
            "hist",
            "history",
            "log",
            "noh",
            "nofilter",
            "nohlsearch",
            "note",
            "notes",
            "q",
            "quit",
            "reload",
            "rm",
            "sync",
            "todo",
            "todos",
            "toggle",
            "wq",
            "x",
        ]
        .into_iter()
        .map(String::from)
        .collect();

        for ctx in &self.contexts {
            candidates.push(format!("ctx {}", ctx.slug));
        }

        let mut result: Vec<String> = candidates
            .into_iter()
            .filter(|c| c.starts_with(&buf_lower) && c.as_str() != buf_lower)
            .collect();

        result.sort_by_key(|s| s.len());
        result
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
        let version = self.editing_note_version.take();

        match exit {
            EditorExit::Save => {
                let body = editor.body();
                let Some(i) = idx else {
                    self.mode = Mode::Normal;
                    return;
                };
                let Some(note) = self.notes.get(i) else {
                    self.mode = Mode::Normal;
                    return;
                };
                let id = note.id;
                if note.body == body {
                    self.status = String::from("note unchanged");
                    self.mode = Mode::Normal;
                    return;
                }
                let patch = PatchedNote {
                    body: Some(body),
                    ..Default::default()
                };
                match self.client.patch_note(id, &patch, version) {
                    Ok(updated) => {
                        if let Some(slot) = self.notes.get_mut(i) {
                            *slot = updated;
                        }
                        self.status = String::from("note saved");
                    }
                    Err(e) => self.set_error_status(&e),
                }
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
            "ctx" | "context" => self.run_ctx(rest),
            "reload" | "sync" => self.reload_from_server(),
            "clear" | "nofilter" | "noh" | "nohlsearch" => {
                self.filter.clear();
                self.snap_selection();
                self.status = String::from("filter cleared");
            }
            "help" | "h" => {
                self.status = String::from(
                    "keys: hjkl move/switch | i add | dd delete | x toggle | / filter | :ctx <slug> | :history",
                );
            }
            other => {
                self.status = format!("unknown command: {}", other);
            }
        }
    }

    fn run_ctx(&mut self, arg: &str) {
        if arg.is_empty() {
            self.open_context_browser();
            return;
        }
        let mut parts = arg.splitn(2, char::is_whitespace);
        let first = parts.next().unwrap_or("");
        let rest = parts.next().unwrap_or("").trim();
        match first {
            "new" => self.create_context(rest),
            "delete" | "rm" => self.archive_context(rest),
            _ => self.switch_context(first),
        }
    }

    fn create_context(&mut self, arg: &str) {
        let mut parts = arg.splitn(2, char::is_whitespace);
        let slug = parts.next().unwrap_or("").trim();
        let name_raw = parts.next().unwrap_or("").trim();
        if slug.is_empty() {
            self.status = String::from("usage: :ctx new <slug> [name]");
            return;
        }
        let name = if name_raw.is_empty() {
            slug.to_string()
        } else {
            name_raw.to_string()
        };
        match self.client.create_context(slug, &name, None) {
            Ok(ctx) => {
                self.contexts.push(ctx.clone());
                self.status = format!("created context [{}]", ctx.slug);
                self.switch_context(&ctx.slug);
            }
            Err(e) => self.set_error_status(&e),
        }
    }

    fn switch_context(&mut self, slug: &str) {
        let Some(ctx) = self.contexts.iter().find(|c| c.slug == slug).cloned() else {
            self.status = format!("no such context: {}", slug);
            return;
        };
        let todos = self.client.list_todos(&ctx.slug);
        let notes = self.client.list_notes(&ctx.slug);
        match (todos, notes) {
            (Ok(todos), Ok(notes)) => {
                self.active_context = ctx.clone();
                self.client.set_active_context(&ctx.slug);
                self.todos = todos;
                self.notes = notes;
                self.todo_index = 0;
                self.note_index = 0;
                self.filter.clear();
                self.history_events.clear();
                self.offline = false;
                self.status = format!("switched to [{}]", ctx.slug);
            }
            (Err(e), _) | (_, Err(e)) => self.set_error_status(&e),
        }
    }

    fn reload_from_server(&mut self) {
        match self.client.list_contexts() {
            Ok(contexts) => {
                self.contexts = contexts;
            }
            Err(e) => {
                self.set_error_status(&e);
                return;
            }
        }
        let slug = self.active_context.slug.clone();
        let todos = self.client.list_todos(&slug);
        let notes = self.client.list_notes(&slug);
        match (todos, notes) {
            (Ok(todos), Ok(notes)) => {
                self.todos = todos;
                self.notes = notes;
                self.offline = false;
                self.snap_selection();
                self.status = String::from("reloaded");
            }
            (Err(e), _) | (_, Err(e)) => self.set_error_status(&e),
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
                match self.client.create_todo(&self.active_context.slug, &value) {
                    Ok(todo) => {
                        self.todos.push(todo);
                        self.todo_index = self.todos.len().saturating_sub(1);
                        self.focus = Pane::Todos;
                        self.status = String::from("todo added");
                    }
                    Err(e) => self.set_error_status(&e),
                }
            }
            InputTarget::NewNote => {
                match self.client.create_note(&self.active_context.slug, &value) {
                    Ok(note) => {
                        self.notes.push(note);
                        self.note_index = self.notes.len().saturating_sub(1);
                        self.focus = Pane::Notes;
                        self.status = String::from("note added");
                    }
                    Err(e) => self.set_error_status(&e),
                }
            }
            InputTarget::RenameTodo => {
                let Some(id) = self.todos.get(self.todo_index).map(|t| t.id) else {
                    return;
                };
                match self.client.rename_todo(id, &value) {
                    Ok(updated) => {
                        if let Some(slot) = self.todos.get_mut(self.todo_index) {
                            *slot = updated;
                        }
                        self.status = String::from("todo renamed");
                    }
                    Err(e) => self.set_error_status(&e),
                }
            }
            InputTarget::RenameNote => {
                let Some(id) = self.notes.get(self.note_index).map(|n| n.id) else {
                    return;
                };
                match self.client.rename_note(id, &value) {
                    Ok(updated) => {
                        if let Some(slot) = self.notes.get_mut(self.note_index) {
                            *slot = updated;
                        }
                        self.status = String::from("note renamed");
                    }
                    Err(e) => self.set_error_status(&e),
                }
            }
            InputTarget::NewContext => {
                let slug = value.trim().to_string();
                if slug.is_empty() {
                    self.status = String::from("cancelled (empty)");
                    return;
                }
                match self.client.create_context(&slug, &slug, None) {
                    Ok(ctx) => {
                        self.contexts.push(ctx.clone());
                        self.status = format!("created context [{}]", ctx.slug);
                        self.switch_context(&ctx.slug);
                        self.open_context_browser();
                    }
                    Err(e) => self.set_error_status(&e),
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
                let Some(id) = self.todos.get(self.todo_index).map(|t| t.id) else {
                    return;
                };
                match self.client.delete_todo(id) {
                    Ok(()) => {
                        self.todos.remove(self.todo_index);
                        self.snap_selection();
                        self.status = String::from("todo deleted");
                    }
                    Err(e) => self.set_error_status(&e),
                }
            }
            Pane::Notes => {
                let Some(id) = self.notes.get(self.note_index).map(|n| n.id) else {
                    return;
                };
                match self.client.delete_note(id) {
                    Ok(()) => {
                        self.notes.remove(self.note_index);
                        self.snap_selection();
                        self.status = String::from("note deleted");
                    }
                    Err(e) => self.set_error_status(&e),
                }
            }
        }
    }

    fn toggle_done(&mut self) {
        let Some((id, next)) = self.todos.get(self.todo_index).map(|t| (t.id, !t.done)) else {
            return;
        };
        match self.client.set_todo_done(id, next) {
            Ok(updated) => {
                if let Some(slot) = self.todos.get_mut(self.todo_index) {
                    *slot = updated;
                }
            }
            Err(e) => self.set_error_status(&e),
        }
    }

    fn open_note_view(&mut self) {
        if self.notes.get(self.note_index).is_some() {
            self.mode = Mode::NoteView;
        }
    }

    fn open_note_edit(&mut self) {
        if let Some(note) = self.notes.get(self.note_index) {
            self.note_editor = Some(VimEditor::new(&note.body));
            self.editing_note_index = Some(self.note_index);
            self.editing_note_version = Some(note.updated_at);
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
        self.todos
            .iter()
            .enumerate()
            .filter(|(_, t)| t.deleted_at.is_none())
            .filter(|(_, t)| f.is_empty() || t.title.to_lowercase().contains(&f))
            .map(|(i, _)| i)
            .collect()
    }

    pub fn visible_note_indices(&self) -> Vec<usize> {
        let f = self.filter.to_lowercase();
        self.notes
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

    fn set_error_status(&mut self, e: &ApiError) {
        if e.is_network() {
            self.offline = true;
        }
        self.status = e.status_line();
    }

    fn cycle_context(&mut self) {
        if self.contexts.len() < 2 {
            self.status = String::from("only one context");
            return;
        }
        let cur = self
            .contexts
            .iter()
            .position(|c| c.slug == self.active_context.slug)
            .unwrap_or(0);
        let next = (cur + 1) % self.contexts.len();
        let slug = self.contexts[next].slug.clone();
        self.switch_context(&slug);
    }

    fn open_context_browser(&mut self) {
        self.context_index = self
            .contexts
            .iter()
            .position(|c| c.slug == self.active_context.slug)
            .unwrap_or(0);
        self.mode = Mode::ContextBrowser;
    }

    fn handle_context_browser(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                self.mode = Mode::Normal;
            }
            KeyCode::Char('j') | KeyCode::Down => {
                if self.context_index + 1 < self.contexts.len() {
                    self.context_index += 1;
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.context_index = self.context_index.saturating_sub(1);
            }
            KeyCode::Enter => {
                if let Some(ctx) = self.contexts.get(self.context_index).cloned() {
                    let slug = ctx.slug.clone();
                    self.mode = Mode::Normal;
                    self.switch_context(&slug);
                }
            }
            KeyCode::Char('n') => {
                self.mode = Mode::Input;
                self.input_buffer.clear();
                self.input_target = Some(InputTarget::NewContext);
            }
            KeyCode::Char('d') => {
                if let Some(ctx) = self.contexts.get(self.context_index).cloned() {
                    if ctx.slug == self.active_context.slug {
                        self.status = String::from("cannot delete the active context");
                    } else {
                        let slug = ctx.slug.clone();
                        self.archive_context(&slug);
                        self.context_index = self
                            .context_index
                            .min(self.contexts.len().saturating_sub(1));
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn archive_context(&mut self, slug: &str) {
        if slug.is_empty() {
            self.status = String::from("usage: :ctx delete <slug>");
            return;
        }
        if slug == self.active_context.slug {
            self.status = String::from("cannot delete the active context");
            return;
        }
        match self.client.archive_context(slug) {
            Ok(()) => {
                self.contexts.retain(|c| c.slug != slug);
                self.status = format!("deleted context [{}]", slug);
            }
            Err(e) => self.set_error_status(&e),
        }
    }

    fn open_move_picker(&mut self) {
        let other_contexts: Vec<_> = self
            .contexts
            .iter()
            .filter(|c| c.slug != self.active_context.slug)
            .collect();
        if other_contexts.is_empty() {
            self.status = String::from("no other contexts to move to");
            return;
        }
        self.move_source = Some((self.focus, self.current_index()));
        self.move_picker_index = 0;
        self.mode = Mode::MovePicker;
    }

    fn handle_move_picker(&mut self, key: KeyEvent) -> Result<()> {
        let targets: Vec<String> = self
            .contexts
            .iter()
            .filter(|c| c.slug != self.active_context.slug)
            .map(|c| c.slug.clone())
            .collect();

        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                self.mode = Mode::Normal;
                self.move_source = None;
            }
            KeyCode::Char('j') | KeyCode::Down => {
                if self.move_picker_index + 1 < targets.len() {
                    self.move_picker_index += 1;
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.move_picker_index = self.move_picker_index.saturating_sub(1);
            }
            KeyCode::Enter => {
                if let Some(slug) = targets.get(self.move_picker_index).cloned() {
                    self.execute_move(&slug);
                }
                self.mode = Mode::Normal;
                self.move_source = None;
            }
            _ => {}
        }
        Ok(())
    }

    fn execute_move(&mut self, target_slug: &str) {
        let Some((pane, idx)) = self.move_source else {
            return;
        };
        let active_ctx_id = self.active_context.id;
        match pane {
            Pane::Todos => {
                let Some(id) = self.todos.get(idx).map(|t| t.id) else {
                    return;
                };
                let title = self.todos[idx].title.clone();
                match self.client.move_todo(id, target_slug) {
                    Ok(updated) if updated.context_id != active_ctx_id => {
                        self.todos.remove(idx);
                        self.snap_selection();
                        self.status = format!("moved \"{}\" to [{}]", title, target_slug);
                    }
                    Ok(_) => {
                        self.status =
                            String::from("move failed: server did not accept context change");
                    }
                    Err(e) => self.set_error_status(&e),
                }
            }
            Pane::Notes => {
                let Some(id) = self.notes.get(idx).map(|n| n.id) else {
                    return;
                };
                let title = self.notes[idx].title.clone();
                match self.client.move_note(id, target_slug) {
                    Ok(updated) if updated.context_id != active_ctx_id => {
                        self.notes.remove(idx);
                        self.snap_selection();
                        self.status = format!("moved \"{}\" to [{}]", title, target_slug);
                    }
                    Ok(_) => {
                        self.status =
                            String::from("move failed: server did not accept context change");
                    }
                    Err(e) => self.set_error_status(&e),
                }
            }
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
        match self
            .client
            .list_events(Some(&self.active_context.slug), 200)
        {
            Ok(events) => {
                self.history_events = events;
                self.history_scroll = 0;
                self.mode = Mode::History;
            }
            Err(e) => self.set_error_status(&e),
        }
    }
}

use crate::editor::{EditorExit, VimEditor};
use crate::ui;
use anyhow::Result;
use crossterm::event::{self, Event as CEvent, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::{backend::Backend, Terminal};
use std::time::Duration;
use todo_fs::store::Store;
use todo_fs::types::{Note, Todo};

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
    TodoView,
    TodoEdit,
    NoteView,
    NoteEdit,
    ContextBrowser,
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
    pub store: Store,
    pub contexts: Vec<String>,
    pub todos: Vec<Todo>,
    pub notes: Vec<Note>,
    pub focus: Pane,
    pub mode: Mode,
    pub todo_index: usize,
    pub note_index: usize,
    pub context_index: usize,
    pub status: String,
    pub command_buffer: String,
    pub input_buffer: String,
    pub input_target: Option<InputTarget>,
    pub filter: String,
    pub filter_backup: String,
    pub note_editor: Option<VimEditor>,
    pub editing_note_slug: Option<String>,
    pub todo_editor: Option<VimEditor>,
    pub editing_todo_slug: Option<String>,
    pub should_quit: bool,
    pub pending_d: bool,
    pub pending_g: bool,
    pub pending_c: bool,
    pub suggestions: Vec<String>,
    pub suggestion_index: usize,
}

impl App {
    pub fn bootstrap(store: Store) -> Result<Self> {
        let contexts = store.list_contexts()?;
        let (todos, mut warnings) = store.list_todos()?;
        let (notes, note_warnings) = store.list_notes()?;
        warnings.extend(note_warnings);

        let status = if warnings.is_empty() {
            format!(
                "welcome — [{}]  :help for commands, :q to quit",
                store.context()
            )
        } else {
            format!(
                "{} item(s) could not be loaded — check CONTENT.md files in [{}]",
                warnings.len(),
                store.context()
            )
        };

        let context_index = contexts
            .iter()
            .position(|c| c == store.context())
            .unwrap_or(0);

        let mut app = Self {
            store,
            contexts,
            todos,
            notes,
            focus: Pane::Todos,
            mode: Mode::Normal,
            todo_index: 0,
            note_index: 0,
            context_index,
            status,
            command_buffer: String::new(),
            input_buffer: String::new(),
            input_target: None,
            filter: String::new(),
            filter_backup: String::new(),
            note_editor: None,
            editing_note_slug: None,
            todo_editor: None,
            editing_todo_slug: None,
            should_quit: false,
            pending_d: false,
            pending_g: false,
            pending_c: false,
            suggestions: Vec::new(),
            suggestion_index: 0,
        };
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
            Mode::TodoView => self.handle_todo_view(key),
            Mode::TodoEdit => self.handle_todo_edit(key),
            Mode::NoteView => self.handle_note_view(key),
            Mode::NoteEdit => self.handle_note_edit(key),
            Mode::ContextBrowser => self.handle_context_browser(key),
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
            KeyCode::Char('v') => {
                if self.focus == Pane::Todos {
                    self.open_todo_view();
                }
            }
            KeyCode::Char('e') => match self.focus {
                Pane::Todos => self.open_todo_edit(),
                Pane::Notes => self.open_note_edit(),
            },
            KeyCode::Tab => {
                self.focus = match self.focus {
                    Pane::Todos => Pane::Notes,
                    Pane::Notes => Pane::Todos,
                };
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

        let mut candidates: Vec<String> = [
            "clear",
            "ctx",
            "ctx new",
            "delete",
            "h",
            "help",
            "noh",
            "nofilter",
            "nohlsearch",
            "note",
            "notes",
            "q",
            "quit",
            "reload",
            "rm",
            "todo",
            "todos",
            "toggle",
            "wq",
            "x",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();

        for ctx in &self.contexts {
            candidates.push(format!("ctx {}", ctx));
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

    fn handle_todo_view(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Enter => {
                self.mode = Mode::Normal;
            }
            KeyCode::Char('e') => {
                self.open_todo_edit();
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_todo_edit(&mut self, key: KeyEvent) -> Result<()> {
        let Some(editor) = self.todo_editor.as_mut() else {
            self.mode = Mode::Normal;
            return Ok(());
        };
        editor.handle_key(key);
        if let Some(exit) = editor.exit {
            self.finish_todo_edit(exit);
        }
        Ok(())
    }

    fn finish_todo_edit(&mut self, exit: EditorExit) {
        let editor = match self.todo_editor.take() {
            Some(e) => e,
            None => {
                self.mode = Mode::Normal;
                return;
            }
        };
        let slug = self.editing_todo_slug.take();
        match exit {
            EditorExit::Save => {
                let body = editor.body();
                let Some(slug) = slug else {
                    self.mode = Mode::Normal;
                    return;
                };
                let Some(idx) = self.todos.iter().position(|t| t.slug == slug) else {
                    self.mode = Mode::Normal;
                    return;
                };
                if self.todos[idx].body == body {
                    self.status = String::from("todo body unchanged");
                    self.mode = Mode::Normal;
                    return;
                }
                match self.store.update_todo_body(&slug, &body) {
                    Ok(updated) => {
                        self.todos[idx] = updated;
                        self.status = String::from("todo saved");
                    }
                    Err(e) => self.status = format!("error: {e}"),
                }
            }
            EditorExit::Cancel => {
                self.status = String::from("edit cancelled");
            }
        }
        self.mode = Mode::Normal;
    }

    fn finish_note_edit(&mut self, exit: EditorExit) {
        let editor = match self.note_editor.take() {
            Some(e) => e,
            None => {
                self.mode = Mode::Normal;
                return;
            }
        };
        let slug = self.editing_note_slug.take();
        match exit {
            EditorExit::Save => {
                let body = editor.body();
                let Some(slug) = slug else {
                    self.mode = Mode::Normal;
                    return;
                };
                let Some(idx) = self.notes.iter().position(|n| n.slug == slug) else {
                    self.mode = Mode::Normal;
                    return;
                };
                if self.notes[idx].body == body {
                    self.status = String::from("note unchanged");
                    self.mode = Mode::Normal;
                    return;
                }
                match self.store.update_note_body(&slug, &body) {
                    Ok(updated) => {
                        self.notes[idx] = updated;
                        self.status = String::from("note saved");
                    }
                    Err(e) => self.status = format!("error: {e}"),
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
            "reload" | "sync" => self.reload(),
            "ctx" | "context" => self.run_ctx(rest),
            "clear" | "nofilter" | "noh" | "nohlsearch" => {
                self.filter.clear();
                self.snap_selection();
                self.status = String::from("filter cleared");
            }
            "help" | "h" => {
                self.status = String::from(
                    "keys: hjkl move/switch | i add | r rename | dd del | x done | e edit | / filter | cc cycle | C browser | :ctx new <name> | :q",
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
            "new" => {
                let name = rest.trim();
                if name.is_empty() {
                    self.start_input(InputTarget::NewContext);
                } else {
                    self.create_context(name);
                }
            }
            slug => self.switch_context(slug),
        }
    }

    fn create_context(&mut self, name: &str) {
        // Slugification happens inside store.create_context and store.switch_context.
        match self.store.create_context(name) {
            Ok(()) => {
                self.switch_context(name);
            }
            Err(e) => self.status = format!("error: {e}"),
        }
    }

    fn switch_context(&mut self, slug: &str) {
        match self.store.switch_context(slug) {
            Ok((todos, notes, warnings)) => {
                self.todos = todos;
                self.notes = notes;
                self.todo_index = 0;
                self.note_index = 0;
                self.filter.clear();

                // Use store.context() as the canonical slug after slugification.
                let canonical = self.store.context().to_string();
                if !self.contexts.contains(&canonical) {
                    self.contexts.push(canonical.clone());
                    self.contexts.sort();
                }
                self.context_index = self
                    .contexts
                    .iter()
                    .position(|c| c == &canonical)
                    .unwrap_or(0);

                self.status = if warnings.is_empty() {
                    format!("switched to [{}]", canonical)
                } else {
                    format!(
                        "switched to [{}] — {} item(s) could not be loaded",
                        canonical,
                        warnings.len()
                    )
                };
            }
            Err(e) => self.status = format!("error: {e}"),
        }
    }

    fn cycle_context(&mut self) {
        if self.contexts.len() < 2 {
            self.status = String::from("only one context");
            return;
        }
        let cur = self
            .contexts
            .iter()
            .position(|c| c == self.store.context())
            .unwrap_or(0);
        let next_slug = self.contexts[(cur + 1) % self.contexts.len()].clone();
        self.switch_context(&next_slug);
    }

    fn open_context_browser(&mut self) {
        self.context_index = self
            .contexts
            .iter()
            .position(|c| c == self.store.context())
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
                if let Some(slug) = self.contexts.get(self.context_index).cloned() {
                    self.mode = Mode::Normal;
                    self.switch_context(&slug);
                }
            }
            KeyCode::Char('n') => {
                self.mode = Mode::Input;
                self.input_buffer.clear();
                self.input_target = Some(InputTarget::NewContext);
            }
            _ => {}
        }
        Ok(())
    }

    fn reload(&mut self) {
        match self.store.list_contexts() {
            Ok(ctxs) => self.contexts = ctxs,
            Err(e) => {
                self.status = format!("reload error: {e}");
                return;
            }
        }
        match (self.store.list_todos(), self.store.list_notes()) {
            (Ok((todos, tw)), Ok((notes, nw))) => {
                self.todos = todos;
                self.notes = notes;
                self.snap_selection();
                let total_warnings = tw.len() + nw.len();
                self.status = if total_warnings == 0 {
                    String::from("reloaded")
                } else {
                    format!("reloaded — {total_warnings} item(s) could not be parsed")
                };
            }
            (Err(e), _) | (_, Err(e)) => self.status = format!("reload error: {e}"),
        }
    }

    fn commit_input(&mut self, target: InputTarget, value: String) {
        let value = value.trim().to_string();
        if value.is_empty() {
            self.status = String::from("cancelled (empty)");
            return;
        }
        match target {
            InputTarget::NewTodo => match self.store.create_todo(&value) {
                Ok(todo) => {
                    self.todos.push(todo);
                    self.todo_index = self.todos.len().saturating_sub(1);
                    self.focus = Pane::Todos;
                    self.status = String::from("todo added");
                }
                Err(e) => self.status = format!("error: {e}"),
            },
            InputTarget::NewNote => match self.store.create_note(&value) {
                Ok(note) => {
                    self.notes.push(note);
                    self.note_index = self.notes.len().saturating_sub(1);
                    self.focus = Pane::Notes;
                    self.status = String::from("note added");
                }
                Err(e) => self.status = format!("error: {e}"),
            },
            InputTarget::RenameTodo => {
                let Some(slug) = self.todos.get(self.todo_index).map(|t| t.slug.clone()) else {
                    return;
                };
                let idx = self.todo_index;
                match self.store.rename_todo(&slug, &value) {
                    Ok(updated) => {
                        self.todos[idx] = updated;
                        self.status = String::from("todo renamed");
                    }
                    Err(e) => self.status = format!("error: {e}"),
                }
            }
            InputTarget::RenameNote => {
                let Some(slug) = self.notes.get(self.note_index).map(|n| n.slug.clone()) else {
                    return;
                };
                let idx = self.note_index;
                match self.store.rename_note(&slug, &value) {
                    Ok(updated) => {
                        self.notes[idx] = updated;
                        self.status = String::from("note renamed");
                    }
                    Err(e) => self.status = format!("error: {e}"),
                }
            }
            InputTarget::NewContext => {
                self.create_context(&value);
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
                let Some(slug) = self.todos.get(self.todo_index).map(|t| t.slug.clone()) else {
                    return;
                };
                match self.store.delete_todo(&slug) {
                    Ok(()) => {
                        self.todos.remove(self.todo_index);
                        self.snap_selection();
                        self.status = String::from("todo deleted");
                    }
                    Err(e) => self.status = format!("error: {e}"),
                }
            }
            Pane::Notes => {
                let Some(slug) = self.notes.get(self.note_index).map(|n| n.slug.clone()) else {
                    return;
                };
                match self.store.delete_note(&slug) {
                    Ok(()) => {
                        self.notes.remove(self.note_index);
                        self.snap_selection();
                        self.status = String::from("note deleted");
                    }
                    Err(e) => self.status = format!("error: {e}"),
                }
            }
        }
    }

    fn toggle_done(&mut self) {
        let Some((slug, next)) = self
            .todos
            .get(self.todo_index)
            .map(|t| (t.slug.clone(), !t.done))
        else {
            return;
        };
        let idx = self.todo_index;
        match self.store.set_todo_done(&slug, next) {
            Ok(updated) => {
                self.todos[idx] = updated;
            }
            Err(e) => self.status = format!("error: {e}"),
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
            self.editing_note_slug = Some(note.slug.clone());
            self.mode = Mode::NoteEdit;
            self.status = String::from("editing note — :w save  :q cancel  i insert  Esc normal");
        }
    }

    fn open_todo_view(&mut self) {
        if self.todos.get(self.todo_index).is_some() {
            self.mode = Mode::TodoView;
        }
    }

    fn open_todo_edit(&mut self) {
        if let Some(todo) = self.todos.get(self.todo_index) {
            self.todo_editor = Some(VimEditor::new(&todo.body));
            self.editing_todo_slug = Some(todo.slug.clone());
            self.mode = Mode::TodoEdit;
            self.status = String::from("editing todo — :w save  :q cancel  i insert  Esc normal");
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
            .filter(|(_, t)| {
                f.is_empty()
                    || t.title.to_lowercase().contains(&f)
                    || t.description.to_lowercase().contains(&f)
                    || t.body.to_lowercase().contains(&f)
            })
            .map(|(i, _)| i)
            .collect()
    }

    pub fn visible_note_indices(&self) -> Vec<usize> {
        let f = self.filter.to_lowercase();
        self.notes
            .iter()
            .enumerate()
            .filter(|(_, n)| {
                f.is_empty()
                    || n.title.to_lowercase().contains(&f)
                    || n.description.to_lowercase().contains(&f)
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
}

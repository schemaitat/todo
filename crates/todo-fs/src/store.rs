use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::types::{Note, Todo};

// ---------------------------------------------------------------------------
// Internal front-matter shapes (TOML between --- delimiters)
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize)]
struct TodoFm {
    title: String,
    created_at: String,
    done: bool,
    #[serde(default)]
    description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    deleted_at: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct NoteFm {
    title: String,
    created_at: String,
    #[serde(default)]
    description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    deleted_at: Option<String>,
}

// ---------------------------------------------------------------------------
// CONTENT.md parsing and serialisation
// ---------------------------------------------------------------------------

/// Find the byte offset of the closing `---` front-matter delimiter.
///
/// The delimiter must be `\n---` followed immediately by `\n` or end-of-string
/// so that markdown horizontal rules (`---`) inside the body are not confused
/// with the closing delimiter.
fn find_closing_delimiter(s: &str) -> Option<usize> {
    let mut start = 0;
    while let Some(rel) = s[start..].find("\n---") {
        let abs = start + rel;
        let after = abs + 4; // byte after the three dashes
        if after >= s.len() || s.as_bytes()[after] == b'\n' {
            return Some(abs);
        }
        start = abs + 1;
    }
    None
}

/// Split a CONTENT.md into its raw TOML front-matter string and the body.
///
/// Expected format:
/// ```text
/// ---
/// key = "value"
/// ---
///
/// Body here.
/// ```
fn split_front_matter(raw: &str) -> Result<(&str, &str)> {
    let raw = raw.trim_start_matches('\n');
    let Some(rest) = raw.strip_prefix("---\n") else {
        bail!("CONTENT.md missing opening '---' front-matter delimiter");
    };
    let end = find_closing_delimiter(rest).ok_or_else(|| {
        anyhow::anyhow!("CONTENT.md missing closing '---' front-matter delimiter")
    })?;
    let fm_str = &rest[..end];
    let after_delim = &rest[end + 4..]; // skip "\n---"
    let body = after_delim.trim_start_matches('\n');
    Ok((fm_str, body))
}

fn format_content_file(fm_toml: &str, body: &str) -> String {
    if body.is_empty() {
        format!("---\n{fm_toml}---\n")
    } else {
        format!("---\n{fm_toml}---\n\n{body}")
    }
}

fn parse_datetime(s: &str) -> DateTime<Utc> {
    s.parse::<DateTime<Utc>>().unwrap_or_else(|_| Utc::now())
}

/// Write `content` to `path` atomically via a sibling `.tmp` file.
///
/// The temp file is in the same directory so the final rename stays
/// on the same filesystem and is atomic on POSIX systems.
fn atomic_write(path: &Path, content: &str) -> Result<()> {
    let mut tmp_name = path.file_name().unwrap_or_default().to_os_string();
    tmp_name.push(".tmp");
    let tmp = path.with_file_name(tmp_name);
    fs::write(&tmp, content).with_context(|| format!("writing tmp file {}", tmp.display()))?;
    fs::rename(&tmp, path)
        .with_context(|| format!("renaming {} -> {}", tmp.display(), path.display()))
}

// ---------------------------------------------------------------------------
// Git helpers
// ---------------------------------------------------------------------------

/// Run a git command in `dir`, capturing stderr for error messages.
fn run_git(dir: &Path, args: &[&str]) -> Result<()> {
    let out = std::process::Command::new("git")
        .current_dir(dir)
        .args(args)
        .output()
        .context("running git (is git installed?)")?;
    if !out.status.success() {
        bail!(
            "git {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Slug helpers
// ---------------------------------------------------------------------------

/// Convert a human title into a filesystem-safe slug.
pub fn title_to_slug(title: &str) -> String {
    let raw: String = title
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect();
    raw.split('_')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("_")
}

// ---------------------------------------------------------------------------
// Item parsers
// ---------------------------------------------------------------------------

fn parse_todo(slug: &str, raw: &str) -> Result<Todo> {
    let (fm_str, body) = split_front_matter(raw)?;
    let fm: TodoFm = toml::from_str(fm_str)
        .with_context(|| format!("parsing front matter for todo '{slug}'"))?;
    Ok(Todo {
        slug: slug.to_string(),
        title: fm.title,
        description: fm.description,
        body: body.to_string(),
        done: fm.done,
        created_at: parse_datetime(&fm.created_at),
        deleted_at: fm.deleted_at.as_deref().map(parse_datetime),
    })
}

fn parse_note(slug: &str, raw: &str) -> Result<Note> {
    let (fm_str, body) = split_front_matter(raw)?;
    let fm: NoteFm = toml::from_str(fm_str)
        .with_context(|| format!("parsing front matter for note '{slug}'"))?;
    Ok(Note {
        slug: slug.to_string(),
        title: fm.title,
        description: fm.description,
        body: body.to_string(),
        created_at: parse_datetime(&fm.created_at),
        deleted_at: fm.deleted_at.as_deref().map(parse_datetime),
    })
}

// ---------------------------------------------------------------------------
// Store
// ---------------------------------------------------------------------------

pub struct Store {
    root: PathBuf,
    context: String,
}

impl Store {
    /// Open the store at `root` with the given active context slug.
    /// Creates `<root>/<context>/todos/` and `<root>/<context>/notes/` if absent.
    /// Initialises a git repository in `root` on first use.
    pub fn new(root: PathBuf, context: impl Into<String>) -> Result<Self> {
        let context = context.into();
        let store = Self { root, context };
        store.ensure_context(&store.context.clone())?;
        store.ensure_git()?;
        Ok(store)
    }

    /// Initialise a git repository in `root` if one does not already exist.
    /// Writes a `.gitignore` that excludes atomic-write temp files and makes
    /// an initial commit so every subsequent commit has a parent.
    fn ensure_git(&self) -> Result<()> {
        if self.root.join(".git").exists() {
            return Ok(());
        }
        run_git(&self.root, &["init", "-b", "main"])?;
        let gitignore = self.root.join(".gitignore");
        if !gitignore.exists() {
            fs::write(&gitignore, "*.tmp\n").context("writing .gitignore")?;
        }
        run_git(&self.root, &["add", "-A"])?;
        run_git(
            &self.root,
            &["commit", "--allow-empty", "-m", "init: initialize store"],
        )?;
        Ok(())
    }

    /// Stage all changes under `root` and create a commit with `message`.
    /// Skips the commit if nothing has actually changed (idempotent).
    fn git_commit(&self, message: &str) -> Result<()> {
        run_git(&self.root, &["add", "-A"])?;
        // `git diff --cached --quiet` exits 0 when there is nothing staged.
        let nothing_staged = std::process::Command::new("git")
            .current_dir(&self.root)
            .args(["diff", "--cached", "--quiet"])
            .status()
            .context("checking staged changes")?
            .success();
        if !nothing_staged {
            run_git(&self.root, &["commit", "-m", message])?;
        }
        Ok(())
    }

    pub fn root(&self) -> &PathBuf {
        &self.root
    }

    pub fn context(&self) -> &str {
        &self.context
    }

    fn todos_dir(&self) -> PathBuf {
        self.root.join(&self.context).join("todos")
    }

    fn notes_dir(&self) -> PathBuf {
        self.root.join(&self.context).join("notes")
    }

    fn ensure_context(&self, slug: &str) -> Result<()> {
        fs::create_dir_all(self.root.join(slug).join("todos")).context("creating todos dir")?;
        fs::create_dir_all(self.root.join(slug).join("notes")).context("creating notes dir")?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Contexts
    // -----------------------------------------------------------------------

    /// List all context slugs (subdirs of root that contain a `todos/` or `notes/` dir).
    pub fn list_contexts(&self) -> Result<Vec<String>> {
        let mut contexts = Vec::new();
        for entry in fs::read_dir(&self.root).context("reading data root dir")? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let path = entry.path();
            if path.join("todos").is_dir() || path.join("notes").is_dir() {
                contexts.push(entry.file_name().to_string_lossy().into_owned());
            }
        }
        contexts.sort();
        Ok(contexts)
    }

    /// Create a new context (no-op if it already exists).
    pub fn create_context(&self, slug: &str) -> Result<()> {
        let slug = title_to_slug(slug);
        if slug.is_empty() {
            bail!("context name produces an empty slug");
        }
        self.ensure_context(&slug)?;
        self.git_commit(&format!("context: create {slug}"))
    }

    /// Switch the active context, creating it if it doesn't exist.
    /// Returns `(todos, notes, warnings)` for the new context.
    pub fn switch_context(&mut self, slug: &str) -> Result<(Vec<Todo>, Vec<Note>, Vec<String>)> {
        let slug = title_to_slug(slug);
        if slug.is_empty() {
            bail!("context name produces an empty slug");
        }
        self.ensure_context(&slug)?;
        self.context = slug;
        let (todos, mut warnings) = self.list_todos()?;
        let (notes, note_warnings) = self.list_notes()?;
        warnings.extend(note_warnings);
        Ok((todos, notes, warnings))
    }

    // -----------------------------------------------------------------------
    // Todos
    // -----------------------------------------------------------------------

    /// Returns `(todos, warnings)` where warnings are skipped malformed items.
    pub fn list_todos(&self) -> Result<(Vec<Todo>, Vec<String>)> {
        let dir = self.todos_dir();
        let mut todos = Vec::new();
        let mut warnings = Vec::new();
        for entry in fs::read_dir(&dir).context("reading todos dir")? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let slug = entry.file_name().to_string_lossy().into_owned();
            let content_path = entry.path().join("CONTENT.md");
            if !content_path.exists() {
                continue;
            }
            let raw = fs::read_to_string(&content_path)?;
            match parse_todo(&slug, &raw) {
                Ok(t) if t.deleted_at.is_none() => todos.push(t),
                Ok(_) => {}
                Err(e) => warnings.push(format!("skipping todo '{slug}': {e}")),
            }
        }
        todos.sort_by(|a, b| a.created_at.cmp(&b.created_at));
        Ok((todos, warnings))
    }

    pub fn create_todo(&self, title: &str) -> Result<Todo> {
        let slug = title_to_slug(title);
        if slug.is_empty() {
            bail!("title produces an empty slug");
        }
        let dir = self.todos_dir().join(&slug);
        if dir.exists() {
            bail!("a todo named '{slug}' already exists");
        }
        fs::create_dir_all(&dir)?;
        let now = Utc::now();
        let fm = TodoFm {
            title: title.to_string(),
            created_at: now.to_rfc3339(),
            done: false,
            description: String::new(),
            deleted_at: None,
        };
        let fm_toml = toml::to_string(&fm).context("serialising todo front matter")?;
        atomic_write(&dir.join("CONTENT.md"), &format_content_file(&fm_toml, ""))?;
        let todo = Todo {
            slug: slug.clone(),
            title: title.to_string(),
            description: String::new(),
            body: String::new(),
            done: false,
            created_at: now,
            deleted_at: None,
        };
        self.git_commit(&format!("todo: create {slug} [{}]", self.context))?;
        Ok(todo)
    }

    pub fn rename_todo(&self, slug: &str, new_title: &str) -> Result<Todo> {
        let new_slug = title_to_slug(new_title);
        if new_slug.is_empty() {
            bail!("new title produces an empty slug");
        }
        let old_dir = self.todos_dir().join(slug);
        let content_path = old_dir.join("CONTENT.md");
        let raw = fs::read_to_string(&content_path)?;
        let (fm_str, body) = split_front_matter(&raw)?;
        let mut fm: TodoFm = toml::from_str(fm_str)?;
        fm.title = new_title.to_string();
        let fm_toml = toml::to_string(&fm)?;
        let new_dir = self.todos_dir().join(&new_slug);
        if new_dir.exists() && new_slug != slug {
            bail!("a todo named '{new_slug}' already exists");
        }
        atomic_write(&content_path, &format_content_file(&fm_toml, body))?;
        if new_slug != slug {
            fs::rename(&old_dir, &new_dir)?;
        }
        let msg = if new_slug == slug {
            format!("todo: rename {slug} [{}]", self.context)
        } else {
            format!("todo: rename {slug} -> {new_slug} [{}]", self.context)
        };
        self.git_commit(&msg)?;
        Ok(Todo {
            slug: new_slug,
            title: new_title.to_string(),
            description: fm.description,
            body: body.to_string(),
            done: fm.done,
            created_at: parse_datetime(&fm.created_at),
            deleted_at: fm.deleted_at.as_deref().map(parse_datetime),
        })
    }

    pub fn set_todo_done(&self, slug: &str, done: bool) -> Result<Todo> {
        let updated = self.update_todo(slug, |fm| fm.done = done, |_| {})?;
        let action = if done { "check" } else { "uncheck" };
        self.git_commit(&format!("todo: {action} {slug} [{}]", self.context))?;
        Ok(updated)
    }

    pub fn update_todo_body(&self, slug: &str, body: &str) -> Result<Todo> {
        let new_body = body.to_string();
        let updated = self.update_todo(slug, |_| {}, move |b| *b = new_body.clone())?;
        self.git_commit(&format!("todo: edit {slug} [{}]", self.context))?;
        Ok(updated)
    }

    fn update_todo(
        &self,
        slug: &str,
        fm_edit: impl FnOnce(&mut TodoFm),
        body_edit: impl FnOnce(&mut String),
    ) -> Result<Todo> {
        let dir = self.todos_dir().join(slug);
        let content_path = dir.join("CONTENT.md");
        let raw = fs::read_to_string(&content_path)?;
        let (fm_str, body_str) = split_front_matter(&raw)?;
        let mut fm: TodoFm = toml::from_str(fm_str)?;
        let mut body = body_str.to_string();
        fm_edit(&mut fm);
        body_edit(&mut body);
        let fm_toml = toml::to_string(&fm)?;
        atomic_write(&content_path, &format_content_file(&fm_toml, &body))?;
        Ok(Todo {
            slug: slug.to_string(),
            title: fm.title,
            description: fm.description,
            body,
            done: fm.done,
            created_at: parse_datetime(&fm.created_at),
            deleted_at: fm.deleted_at.as_deref().map(parse_datetime),
        })
    }

    pub fn delete_todo(&self, slug: &str) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        self.update_todo(slug, |fm| fm.deleted_at = Some(now.clone()), |_| {})?;
        self.git_commit(&format!("todo: delete {slug} [{}]", self.context))
    }

    // -----------------------------------------------------------------------
    // Notes
    // -----------------------------------------------------------------------

    /// Returns `(notes, warnings)` where warnings are skipped malformed items.
    pub fn list_notes(&self) -> Result<(Vec<Note>, Vec<String>)> {
        let dir = self.notes_dir();
        let mut notes = Vec::new();
        let mut warnings = Vec::new();
        for entry in fs::read_dir(&dir).context("reading notes dir")? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let slug = entry.file_name().to_string_lossy().into_owned();
            let content_path = entry.path().join("CONTENT.md");
            if !content_path.exists() {
                continue;
            }
            let raw = fs::read_to_string(&content_path)?;
            match parse_note(&slug, &raw) {
                Ok(n) if n.deleted_at.is_none() => notes.push(n),
                Ok(_) => {}
                Err(e) => warnings.push(format!("skipping note '{slug}': {e}")),
            }
        }
        notes.sort_by(|a, b| a.created_at.cmp(&b.created_at));
        Ok((notes, warnings))
    }

    pub fn create_note(&self, title: &str) -> Result<Note> {
        let slug = title_to_slug(title);
        if slug.is_empty() {
            bail!("title produces an empty slug");
        }
        let dir = self.notes_dir().join(&slug);
        if dir.exists() {
            bail!("a note named '{slug}' already exists");
        }
        fs::create_dir_all(&dir)?;
        let now = Utc::now();
        let fm = NoteFm {
            title: title.to_string(),
            created_at: now.to_rfc3339(),
            description: String::new(),
            deleted_at: None,
        };
        let fm_toml = toml::to_string(&fm)?;
        atomic_write(&dir.join("CONTENT.md"), &format_content_file(&fm_toml, ""))?;
        let note = Note {
            slug: slug.clone(),
            title: title.to_string(),
            description: String::new(),
            body: String::new(),
            created_at: now,
            deleted_at: None,
        };
        self.git_commit(&format!("note: create {slug} [{}]", self.context))?;
        Ok(note)
    }

    pub fn rename_note(&self, slug: &str, new_title: &str) -> Result<Note> {
        let new_slug = title_to_slug(new_title);
        if new_slug.is_empty() {
            bail!("new title produces an empty slug");
        }
        let old_dir = self.notes_dir().join(slug);
        let content_path = old_dir.join("CONTENT.md");
        let raw = fs::read_to_string(&content_path)?;
        let (fm_str, body) = split_front_matter(&raw)?;
        let mut fm: NoteFm = toml::from_str(fm_str)?;
        fm.title = new_title.to_string();
        let fm_toml = toml::to_string(&fm)?;
        let new_dir = self.notes_dir().join(&new_slug);
        if new_dir.exists() && new_slug != slug {
            bail!("a note named '{new_slug}' already exists");
        }
        atomic_write(&content_path, &format_content_file(&fm_toml, body))?;
        if new_slug != slug {
            fs::rename(&old_dir, &new_dir)?;
        }
        let msg = if new_slug == slug {
            format!("note: rename {slug} [{}]", self.context)
        } else {
            format!("note: rename {slug} -> {new_slug} [{}]", self.context)
        };
        self.git_commit(&msg)?;
        Ok(Note {
            slug: new_slug,
            title: new_title.to_string(),
            description: fm.description,
            body: body.to_string(),
            created_at: parse_datetime(&fm.created_at),
            deleted_at: fm.deleted_at.as_deref().map(parse_datetime),
        })
    }

    pub fn update_note_body(&self, slug: &str, body: &str) -> Result<Note> {
        let new_body = body.to_string();
        let updated = self.update_note(slug, |_| {}, move |b| *b = new_body.clone())?;
        self.git_commit(&format!("note: edit {slug} [{}]", self.context))?;
        Ok(updated)
    }

    fn update_note(
        &self,
        slug: &str,
        fm_edit: impl FnOnce(&mut NoteFm),
        body_edit: impl FnOnce(&mut String),
    ) -> Result<Note> {
        let dir = self.notes_dir().join(slug);
        let content_path = dir.join("CONTENT.md");
        let raw = fs::read_to_string(&content_path)?;
        let (fm_str, body_str) = split_front_matter(&raw)?;
        let mut fm: NoteFm = toml::from_str(fm_str)?;
        let mut body = body_str.to_string();
        fm_edit(&mut fm);
        body_edit(&mut body);
        let fm_toml = toml::to_string(&fm)?;
        atomic_write(&content_path, &format_content_file(&fm_toml, &body))?;
        Ok(Note {
            slug: slug.to_string(),
            title: fm.title,
            description: fm.description,
            body,
            created_at: parse_datetime(&fm.created_at),
            deleted_at: fm.deleted_at.as_deref().map(parse_datetime),
        })
    }

    pub fn delete_note(&self, slug: &str) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        self.update_note(slug, |fm| fm.deleted_at = Some(now.clone()), |_| {})?;
        self.git_commit(&format!("note: delete {slug} [{}]", self.context))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DEFAULT_CONTEXT;
    use tempfile::TempDir;

    fn tmp_store() -> (TempDir, Store) {
        let dir = TempDir::new().unwrap();
        let store = Store::new(dir.path().to_path_buf(), DEFAULT_CONTEXT).unwrap();
        (dir, store)
    }

    // --- slug helper ---

    #[test]
    fn slug_basic() {
        assert_eq!(title_to_slug("My First Todo"), "my_first_todo");
    }

    #[test]
    fn slug_special_chars() {
        assert_eq!(title_to_slug("Hello, World!"), "hello_world");
    }

    #[test]
    fn slug_collapses_separators() {
        assert_eq!(title_to_slug("foo  --  bar"), "foo_bar");
    }

    #[test]
    fn slug_empty_input() {
        assert_eq!(title_to_slug("!!!"), "");
    }

    // --- front-matter parser ---

    #[test]
    fn parse_fm_basic() {
        let raw = "---\ntitle = \"hi\"\n---\n\nbody here";
        let (fm, body) = split_front_matter(raw).unwrap();
        assert_eq!(fm, "title = \"hi\"");
        assert_eq!(body, "body here");
    }

    #[test]
    fn parse_fm_empty_body() {
        let raw = "---\ntitle = \"hi\"\n---\n";
        let (_, body) = split_front_matter(raw).unwrap();
        assert_eq!(body, "");
    }

    #[test]
    fn parse_fm_body_with_horizontal_rule() {
        let raw = "---\ntitle = \"hi\"\n---\n\nSection A\n\n---\n\nSection B";
        let (_, body) = split_front_matter(raw).unwrap();
        assert_eq!(body, "Section A\n\n---\n\nSection B");
    }

    #[test]
    fn parse_fm_missing_open_delimiter() {
        assert!(split_front_matter("no front matter").is_err());
    }

    #[test]
    fn parse_fm_missing_close_delimiter() {
        assert!(split_front_matter("---\ntitle = \"hi\"\n").is_err());
    }

    // --- todos CRUD ---

    #[test]
    fn create_and_list_todo() {
        let (_dir, store) = tmp_store();
        store.create_todo("Buy milk").unwrap();
        let (todos, warnings) = store.list_todos().unwrap();
        assert!(warnings.is_empty());
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].title, "Buy milk");
        assert_eq!(todos[0].slug, "buy_milk");
        assert!(!todos[0].done);
    }

    #[test]
    fn soft_delete_hides_todo() {
        let (_dir, store) = tmp_store();
        store.create_todo("Delete me").unwrap();
        store.delete_todo("delete_me").unwrap();
        let (todos, _) = store.list_todos().unwrap();
        assert!(todos.is_empty());
    }

    #[test]
    fn toggle_done_persists() {
        let (_dir, store) = tmp_store();
        store.create_todo("Task").unwrap();
        store.set_todo_done("task", true).unwrap();
        let (todos, _) = store.list_todos().unwrap();
        assert!(todos[0].done);
        store.set_todo_done("task", false).unwrap();
        let (todos, _) = store.list_todos().unwrap();
        assert!(!todos[0].done);
    }

    #[test]
    fn rename_todo_same_slug() {
        let (_dir, store) = tmp_store();
        store.create_todo("Task").unwrap();
        let updated = store.rename_todo("task", "Task (updated)").unwrap();
        // different title but same slug
        assert_eq!(updated.slug, "task_updated");
        let (todos, _) = store.list_todos().unwrap();
        assert_eq!(todos[0].title, "Task (updated)");
    }

    #[test]
    fn rename_todo_changes_slug() {
        let (_dir, store) = tmp_store();
        store.create_todo("Alpha").unwrap();
        store.rename_todo("alpha", "Beta").unwrap();
        let (todos, _) = store.list_todos().unwrap();
        assert_eq!(todos[0].slug, "beta");
        assert_eq!(todos[0].title, "Beta");
    }

    #[test]
    fn todo_body_round_trips() {
        let (_dir, store) = tmp_store();
        store.create_todo("Note body").unwrap();
        store
            .update_todo_body("note_body", "Line 1\n\n---\n\nLine 2")
            .unwrap();
        let (todos, _) = store.list_todos().unwrap();
        assert_eq!(todos[0].body, "Line 1\n\n---\n\nLine 2");
    }

    #[test]
    fn duplicate_todo_is_error() {
        let (_dir, store) = tmp_store();
        store.create_todo("Dup").unwrap();
        assert!(store.create_todo("Dup").is_err());
    }

    // --- notes CRUD ---

    #[test]
    fn create_and_list_note() {
        let (_dir, store) = tmp_store();
        store.create_note("Meeting notes").unwrap();
        let (notes, warnings) = store.list_notes().unwrap();
        assert!(warnings.is_empty());
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].slug, "meeting_notes");
    }

    #[test]
    fn soft_delete_hides_note() {
        let (_dir, store) = tmp_store();
        store.create_note("Gone").unwrap();
        store.delete_note("gone").unwrap();
        let (notes, _) = store.list_notes().unwrap();
        assert!(notes.is_empty());
    }

    #[test]
    fn note_body_with_horizontal_rule() {
        let (_dir, store) = tmp_store();
        store.create_note("Ruled").unwrap();
        let body = "# Section\n\n---\n\nMore content";
        store.update_note_body("ruled", body).unwrap();
        let (notes, _) = store.list_notes().unwrap();
        assert_eq!(notes[0].body, body);
    }

    // --- contexts ---

    #[test]
    fn list_contexts_includes_default() {
        let (_dir, store) = tmp_store();
        let ctxs = store.list_contexts().unwrap();
        assert!(ctxs.contains(&DEFAULT_CONTEXT.to_string()));
    }

    #[test]
    fn switch_context_creates_dirs() {
        let (_dir, mut store) = tmp_store();
        store.switch_context("work").unwrap();
        assert_eq!(store.context(), "work");
        let ctxs = store.list_contexts().unwrap();
        assert!(ctxs.contains(&"work".to_string()));
    }

    #[test]
    fn contexts_are_isolated() {
        let (_dir, mut store) = tmp_store();
        store.create_todo("Inbox task").unwrap();
        store.switch_context("work").unwrap();
        let (todos, _) = store.list_todos().unwrap();
        assert!(todos.is_empty(), "work context should start empty");
    }
}

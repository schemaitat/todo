use std::fs;
use std::path::Path;

use chrono::{DateTime, Utc};

use crate::store::title_to_slug;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ItemKind {
    Todo,
    Note,
}

impl std::fmt::Display for ItemKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ItemKind::Todo => write!(f, "todo"),
            ItemKind::Note => write!(f, "note"),
        }
    }
}

#[derive(Debug)]
pub struct ItemReport {
    pub context: String,
    pub kind: ItemKind,
    pub slug: String,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

impl ItemReport {
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }
}

pub struct ValidationSummary {
    /// Per-item reports (todos and notes across all contexts).
    pub items: Vec<ItemReport>,
    /// Structural problems at the root or context level.
    pub structural_errors: Vec<String>,
}

impl ValidationSummary {
    pub fn has_errors(&self) -> bool {
        !self.structural_errors.is_empty() || self.items.iter().any(|r| r.has_errors())
    }

    pub fn error_count(&self) -> usize {
        self.structural_errors.len() + self.items.iter().map(|r| r.errors.len()).sum::<usize>()
    }

    pub fn warning_count(&self) -> usize {
        self.items.iter().map(|r| r.warnings.len()).sum()
    }
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

/// Walk `root` and validate every context's todos and notes.
pub fn validate(root: &Path) -> anyhow::Result<ValidationSummary> {
    let mut items = Vec::new();
    let mut structural_errors = Vec::new();

    if !root.exists() {
        structural_errors.push(format!("root directory not found: {}", root.display()));
        return Ok(ValidationSummary {
            items,
            structural_errors,
        });
    }

    for entry in fs::read_dir(root)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let ctx_path = entry.path();
        let ctx = entry.file_name().to_string_lossy().into_owned();

        let todos_dir = ctx_path.join("todos");
        let notes_dir = ctx_path.join("notes");

        // Skip dirs that don't look like contexts.
        if !todos_dir.is_dir() && !notes_dir.is_dir() {
            continue;
        }

        if todos_dir.is_dir() {
            validate_items(&todos_dir, &ctx, ItemKind::Todo, &mut items)?;
        } else {
            structural_errors.push(format!("[{ctx}] missing todos/ directory"));
        }

        if notes_dir.is_dir() {
            validate_items(&notes_dir, &ctx, ItemKind::Note, &mut items)?;
        } else {
            structural_errors.push(format!("[{ctx}] missing notes/ directory"));
        }
    }

    Ok(ValidationSummary {
        items,
        structural_errors,
    })
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn validate_items(
    dir: &Path,
    context: &str,
    kind: ItemKind,
    out: &mut Vec<ItemReport>,
) -> anyhow::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let slug = entry.file_name().to_string_lossy().into_owned();
        let content_path = entry.path().join("CONTENT.md");

        let mut report = ItemReport {
            context: context.to_string(),
            kind: kind.clone(),
            slug: slug.clone(),
            errors: Vec::new(),
            warnings: Vec::new(),
        };

        if !content_path.exists() {
            report.errors.push("missing CONTENT.md".into());
            out.push(report);
            continue;
        }

        match fs::read_to_string(&content_path) {
            Ok(raw) => validate_content(&raw, &kind, &slug, &mut report),
            Err(e) => report.errors.push(format!("cannot read CONTENT.md: {e}")),
        }

        out.push(report);
    }
    Ok(())
}

fn validate_content(raw: &str, kind: &ItemKind, slug: &str, report: &mut ItemReport) {
    let raw = raw.trim_start_matches('\n');

    let Some(rest) = raw.strip_prefix("---\n") else {
        report
            .errors
            .push("missing opening '---' front-matter delimiter".into());
        return;
    };

    let end = find_closing_delimiter(rest);
    let Some(end) = end else {
        report
            .errors
            .push("missing closing '---' front-matter delimiter".into());
        return;
    };

    let fm_str = &rest[..end];

    let fm: toml::Value = match toml::from_str(fm_str) {
        Ok(v) => v,
        Err(e) => {
            report
                .errors
                .push(format!("invalid TOML in front matter: {e}"));
            return;
        }
    };

    let Some(table) = fm.as_table() else {
        report
            .errors
            .push("front matter is not a TOML table".into());
        return;
    };

    // title
    match table.get("title") {
        None => report.errors.push("missing required field: title".into()),
        Some(v) => match v.as_str() {
            None => report.errors.push("title must be a string".into()),
            Some("") => report.warnings.push("title is empty".into()),
            _ => {}
        },
    }

    // created_at
    check_timestamp(table, "created_at", true, report);

    // done (todos only)
    if *kind == ItemKind::Todo {
        match table.get("done") {
            None => report
                .errors
                .push("missing required field for todo: done".into()),
            Some(v) if v.as_bool().is_none() => report.errors.push("done must be a boolean".into()),
            _ => {}
        }
    }

    // deleted_at (optional)
    if table.contains_key("deleted_at") {
        check_timestamp(table, "deleted_at", false, report);
    }

    // slug convention (informational)
    if let Some(title) = table.get("title").and_then(|v| v.as_str()) {
        let expected = title_to_slug(title);
        if !expected.is_empty() && expected != slug {
            report.warnings.push(format!(
                "directory slug '{slug}' differs from title-derived slug '{expected}' (fine if renamed intentionally)"
            ));
        }
    }
}

fn check_timestamp(
    table: &toml::map::Map<String, toml::Value>,
    field: &str,
    required: bool,
    report: &mut ItemReport,
) {
    match table.get(field) {
        None if required => report
            .errors
            .push(format!("missing required field: {field}")),
        None => {}
        Some(v) => match v.as_str() {
            None => report
                .errors
                .push(format!("{field} must be a string (RFC3339)")),
            Some(s) => {
                if s.parse::<DateTime<Utc>>().is_err() {
                    report
                        .errors
                        .push(format!("invalid {field} timestamp: {s:?}"));
                }
            }
        },
    }
}

/// Same logic as in `store::split_front_matter` — `\n---` only closes
/// the front matter when followed by `\n` or end-of-string.
fn find_closing_delimiter(s: &str) -> Option<usize> {
    let mut start = 0;
    while let Some(rel) = s[start..].find("\n---") {
        let abs = start + rel;
        let after = abs + 4;
        if after >= s.len() || s.as_bytes()[after] == b'\n' {
            return Some(abs);
        }
        start = abs + 1;
    }
    None
}

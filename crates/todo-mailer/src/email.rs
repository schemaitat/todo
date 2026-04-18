use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use lettre::message::MultiPart;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{Message, SmtpTransport, Transport};
use std::env;
use std::fmt::Write as _;
use todo_store::Store;

pub struct SmtpConfig {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub pass: String,
    pub from: String,
}

impl SmtpConfig {
    pub fn from_env() -> Result<Self> {
        let host = env::var("TODO_SMTP_HOST").unwrap_or_else(|_| "smtp.gmail.com".to_string());
        let port = env::var("TODO_SMTP_PORT")
            .ok()
            .map(|s| s.parse::<u16>())
            .transpose()
            .context("TODO_SMTP_PORT must be u16")?
            .unwrap_or(465);
        let user = env::var("TODO_SMTP_USER").context("TODO_SMTP_USER not set")?;
        let pass = env::var("TODO_SMTP_PASS").context(
            "TODO_SMTP_PASS not set (Gmail: use an App Password, not your account password)",
        )?;
        let from = env::var("TODO_SMTP_FROM").unwrap_or_else(|_| user.clone());
        Ok(Self {
            host,
            port,
            user,
            pass,
            from,
        })
    }
}

pub fn format_body(store: &Store) -> String {
    let open_todos: Vec<_> = store
        .todos
        .iter()
        .filter(|t| !t.done && t.deleted_at.is_none())
        .collect();
    let live_notes: Vec<_> = store
        .notes
        .iter()
        .filter(|n| n.deleted_at.is_none())
        .collect();

    let mut out = String::new();
    let _ = writeln!(
        out,
        "# todo-tui snapshot — {}",
        Utc::now().format("%Y-%m-%dT%H:%M:%SZ")
    );
    let _ = writeln!(out);
    let _ = writeln!(out, "## Open todos ({})", open_todos.len());
    if open_todos.is_empty() {
        let _ = writeln!(out, "_none_");
    } else {
        for t in &open_todos {
            let _ = writeln!(out, "- [ ] {}", t.title);
        }
    }
    let _ = writeln!(out);
    let _ = writeln!(out, "## Notes ({})", live_notes.len());
    if live_notes.is_empty() {
        let _ = writeln!(out, "_none_");
    } else {
        for n in &live_notes {
            let _ = writeln!(out);
            let _ = writeln!(out, "### {}", n.title);
            let _ = writeln!(out, "{}", n.body);
        }
    }
    out
}

fn html_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}

pub fn format_html(store: &Store) -> String {
    let open_todos: Vec<_> = store
        .todos
        .iter()
        .filter(|t| !t.done && t.deleted_at.is_none())
        .collect();
    let live_notes: Vec<_> = store
        .notes
        .iter()
        .filter(|n| n.deleted_at.is_none())
        .collect();

    let stamp = Utc::now().format("%Y-%m-%d %H:%M UTC").to_string();
    let mut out = String::new();
    let _ = write!(
        out,
        r#"<!doctype html>
<html><body style="margin:0;padding:24px;background:#f6f7f9;font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',Helvetica,Arial,sans-serif;color:#1f2328;">
<div style="max-width:640px;margin:0 auto;background:#ffffff;border:1px solid #e5e7eb;border-radius:10px;overflow:hidden;">
<div style="padding:20px 24px;border-bottom:1px solid #e5e7eb;background:#fafbfc;">
<div style="font-size:12px;color:#6b7280;letter-spacing:0.06em;text-transform:uppercase;">todo-tui snapshot</div>
<div style="font-size:14px;color:#374151;margin-top:4px;">{stamp}</div>
</div>
<div style="padding:20px 24px;">
<h2 style="margin:0 0 12px 0;font-size:16px;color:#111827;border-bottom:1px solid #f0f1f3;padding-bottom:6px;">Open todos <span style="color:#6b7280;font-weight:500;">({open_count})</span></h2>"#,
        stamp = html_escape(&stamp),
        open_count = open_todos.len(),
    );

    if open_todos.is_empty() {
        let _ = write!(
            out,
            r#"<div style="color:#9ca3af;font-style:italic;font-size:14px;">none</div>"#
        );
    } else {
        let _ = write!(out, r#"<ul style="list-style:none;padding:0;margin:0;">"#);
        for t in &open_todos {
            let _ = write!(
                out,
                r#"<li style="padding:6px 0;font-size:14px;color:#1f2328;border-bottom:1px solid #f6f7f9;"><span style="display:inline-block;width:14px;height:14px;border:1.5px solid #d1d5db;border-radius:3px;vertical-align:middle;margin-right:10px;"></span>{title}</li>"#,
                title = html_escape(&t.title),
            );
        }
        let _ = write!(out, "</ul>");
    }

    let _ = write!(
        out,
        r#"<h2 style="margin:24px 0 12px 0;font-size:16px;color:#111827;border-bottom:1px solid #f0f1f3;padding-bottom:6px;">Notes <span style="color:#6b7280;font-weight:500;">({note_count})</span></h2>"#,
        note_count = live_notes.len(),
    );

    if live_notes.is_empty() {
        let _ = write!(
            out,
            r#"<div style="color:#9ca3af;font-style:italic;font-size:14px;">none</div>"#
        );
    } else {
        for n in &live_notes {
            let _ = write!(
                out,
                r#"<div style="margin-top:14px;padding:12px 14px;background:#fafbfc;border:1px solid #eef0f2;border-radius:6px;">
<div style="font-weight:600;font-size:14px;color:#111827;margin-bottom:6px;">{title}</div>
<div style="font-size:13px;color:#374151;white-space:pre-wrap;line-height:1.5;">{body}</div>
</div>"#,
                title = html_escape(&n.title),
                body = html_escape(&n.body),
            );
        }
    }

    let _ = write!(out, "</div></div></body></html>");
    out
}

pub fn send(cfg: &SmtpConfig, to: &str, store: &Store) -> Result<()> {
    let plain = format_body(store);
    let html = format_html(store);
    let from_mbox = cfg
        .from
        .parse()
        .with_context(|| format!("invalid From address: {}", cfg.from))?;
    let to_mbox = to
        .parse()
        .with_context(|| format!("invalid To address: {}", to))?;
    let email = Message::builder()
        .from(from_mbox)
        .to(to_mbox)
        .subject("todo-tui snapshot")
        .multipart(MultiPart::alternative_plain_html(plain, html))?;
    let creds = Credentials::new(cfg.user.clone(), cfg.pass.clone());
    let mailer = SmtpTransport::relay(&cfg.host)?
        .port(cfg.port)
        .credentials(creds)
        .build();
    mailer.send(&email).map_err(|e| anyhow!(e))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use todo_store::{Note, Todo};

    #[test]
    fn body_includes_open_todos_and_live_notes_only() {
        let mut store = Store::default();

        let mut done_todo = Todo::new("done one".into());
        done_todo.done = true;
        store.todos.push(done_todo);

        store.todos.push(Todo::new("open one".into()));

        let mut deleted_todo = Todo::new("deleted todo".into());
        deleted_todo.deleted_at = Some(Utc::now());
        store.todos.push(deleted_todo);

        let mut live_note = Note::new("live title".into());
        live_note.body = "live body line".into();
        store.notes.push(live_note);

        let mut deleted_note = Note::new("deleted title".into());
        deleted_note.deleted_at = Some(Utc::now());
        store.notes.push(deleted_note);

        let body = format_body(&store);

        assert!(body.contains("## Open todos (1)"));
        assert!(body.contains("- [ ] open one"));
        assert!(!body.contains("done one"));
        assert!(!body.contains("deleted todo"));

        assert!(body.contains("## Notes (1)"));
        assert!(body.contains("### live title"));
        assert!(body.contains("live body line"));
        assert!(!body.contains("deleted title"));
    }

    #[test]
    fn empty_sections_render_none() {
        let store = Store::default();
        let body = format_body(&store);
        assert!(body.contains("## Open todos (0)"));
        assert!(body.contains("## Notes (0)"));
        assert!(body.matches("_none_").count() == 2);
    }

    #[test]
    fn html_escapes_and_orders_todos_first() {
        let mut store = Store::default();
        store.todos.push(Todo::new("ship <feature>".into()));
        let mut note = Note::new("title & co".into());
        note.body = "line1\n<b>bold</b>".into();
        store.notes.push(note);

        let html = format_html(&store);
        let todo_pos = html.find("Open todos").unwrap();
        let notes_pos = html.find("Notes <span").unwrap();
        assert!(todo_pos < notes_pos, "todos must appear before notes");

        assert!(html.contains("ship &lt;feature&gt;"));
        assert!(html.contains("title &amp; co"));
        assert!(html.contains("&lt;b&gt;bold&lt;/b&gt;"));
        assert!(!html.contains("<b>bold</b>"));
    }
}

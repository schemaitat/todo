//! Thin SMTP sender on top of pre-rendered snapshot bodies fetched from the API.
//!
//! Rendering (`format_body`, `format_html`) lives on the FastAPI side at `GET /snapshot` so that
//! n8n, cron, or any other client gets the same output. This crate is a convenience for users who
//! prefer a systemd timer to an n8n workflow.

use anyhow::{anyhow, Context, Result};
use lettre::message::MultiPart;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{Message, SmtpTransport, Transport};
use std::env;

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

pub fn send(cfg: &SmtpConfig, to: &str, subject: &str, plain: &str, html: &str) -> Result<()> {
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
        .subject(subject)
        .multipart(MultiPart::alternative_plain_html(
            plain.to_string(),
            html.to_string(),
        ))?;
    let creds = Credentials::new(cfg.user.clone(), cfg.pass.clone());
    let mailer = SmtpTransport::relay(&cfg.host)?
        .port(cfg.port)
        .credentials(creds)
        .build();
    mailer.send(&email).map_err(|e| anyhow!(e))?;
    Ok(())
}

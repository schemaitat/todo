use anyhow::Result;
use todo_api_client::Client;
use todo_mailer::{send, SmtpConfig};

const DEFAULT_RECIPIENT: &str = "a.schemaitat@gmail.com";

fn main() -> Result<()> {
    let _ = dotenvy::dotenv();

    let mut args = std::env::args().skip(1);
    let to = args.next().unwrap_or_else(|| DEFAULT_RECIPIENT.to_string());

    let client = Client::from_env()?;
    let context = args
        .next()
        .unwrap_or_else(|| client.active_context_slug().to_string());

    let plain = client.snapshot_plain(&context)?;
    let html = client.snapshot_html(&context)?;

    let cfg = SmtpConfig::from_env()?;
    send(
        &cfg,
        &to,
        &format!("todo-tui snapshot [{}]", context),
        &plain,
        &html,
    )?;

    println!("emailed snapshot [{}] to {}", context, to);
    Ok(())
}

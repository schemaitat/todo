use anyhow::Result;
use todo_mailer::{send, SmtpConfig};

const DEFAULT_RECIPIENT: &str = "a.schemaitat@gmail.com";

fn main() -> Result<()> {
    let _ = dotenvy::dotenv();
    let to = std::env::args()
        .nth(1)
        .unwrap_or_else(|| DEFAULT_RECIPIENT.to_string());
    let store = todo_store::load()?;
    let cfg = SmtpConfig::from_env()?;
    send(&cfg, &to, &store)?;
    println!("emailed snapshot to {}", to);
    Ok(())
}

import os
import smtplib
from email.mime.multipart import MIMEMultipart
from email.mime.text import MIMEText

import httpx
from prefect import flow, get_run_logger


@flow(name="snapshot-email", log_prints=True)
def snapshot_email() -> None:
    logger = get_run_logger()

    api_url = os.environ["TODO_API_URL"]
    api_key = os.environ["TODO_API_KEY"]
    to_email = os.environ["SNAPSHOT_EMAIL"]
    smtp_host = os.environ.get("SMTP_HOST", "smtp.gmail.com")
    smtp_port = int(os.environ.get("SMTP_PORT", "587"))
    smtp_user = os.environ["SMTP_USER"]
    smtp_pass = os.environ["SMTP_PASS"]

    logger.info("Fetching snapshot (all contexts)")
    with httpx.Client() as client:
        resp = client.get(
            f"{api_url}/snapshot",
            headers={"X-API-Key": api_key},
            params={"format": "html", "include_notes": "false"},
            timeout=30,
        )
        resp.raise_for_status()

    logger.info("Sending snapshot email to %s", to_email)
    msg = MIMEMultipart("alternative")
    msg["Subject"] = "Todo Snapshot"
    msg["From"] = smtp_user
    msg["To"] = to_email
    msg.attach(MIMEText(resp.text, "html"))

    with smtplib.SMTP(smtp_host, smtp_port) as smtp:
        smtp.ehlo()
        smtp.starttls()
        smtp.login(smtp_user, smtp_pass)
        smtp.send_message(msg)

    logger.info("Snapshot email sent")


if __name__ == "__main__":
    snapshot_email.serve(
        name="snapshot-email",
        cron="0 8 * * 1-5",  # Weekdays at 08:00 UTC
    )

from __future__ import annotations

from datetime import datetime, timezone

from ..models import Note, Todo


def _escape_html(s: str) -> str:
    return (
        s.replace("&", "&amp;")
        .replace("<", "&lt;")
        .replace(">", "&gt;")
        .replace('"', "&quot;")
        .replace("'", "&#39;")
    )


def format_plain(open_todos: list[Todo], live_notes: list[Note], now: datetime | None = None) -> str:
    now = now or datetime.now(timezone.utc)
    out: list[str] = []
    out.append(f"# todo-tui snapshot — {now.strftime('%Y-%m-%dT%H:%M:%SZ')}")
    out.append("")
    out.append(f"## Open todos ({len(open_todos)})")
    if not open_todos:
        out.append("_none_")
    else:
        for t in open_todos:
            out.append(f"- [ ] {t.title}")
    out.append("")
    out.append(f"## Notes ({len(live_notes)})")
    if not live_notes:
        out.append("_none_")
    else:
        for n in live_notes:
            out.append("")
            out.append(f"### {n.title}")
            out.append(n.body)
    return "\n".join(out) + "\n"


def format_html(open_todos: list[Todo], live_notes: list[Note], now: datetime | None = None) -> str:
    now = now or datetime.now(timezone.utc)
    stamp = _escape_html(now.strftime("%Y-%m-%d %H:%M UTC"))
    parts: list[str] = [
        "<!doctype html>",
        "<html><body style=\"margin:0;padding:24px;background:#f6f7f9;"
        "font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',Helvetica,Arial,sans-serif;color:#1f2328;\">",
        "<div style=\"max-width:640px;margin:0 auto;background:#ffffff;border:1px solid #e5e7eb;"
        "border-radius:10px;overflow:hidden;\">",
        "<div style=\"padding:20px 24px;border-bottom:1px solid #e5e7eb;background:#fafbfc;\">",
        "<div style=\"font-size:12px;color:#6b7280;letter-spacing:0.06em;"
        "text-transform:uppercase;\">todo-tui snapshot</div>",
        f"<div style=\"font-size:14px;color:#374151;margin-top:4px;\">{stamp}</div>",
        "</div>",
        "<div style=\"padding:20px 24px;\">",
        "<h2 style=\"margin:0 0 12px 0;font-size:16px;color:#111827;"
        "border-bottom:1px solid #f0f1f3;padding-bottom:6px;\">Open todos "
        f"<span style=\"color:#6b7280;font-weight:500;\">({len(open_todos)})</span></h2>",
    ]

    if not open_todos:
        parts.append(
            "<div style=\"color:#9ca3af;font-style:italic;font-size:14px;\">none</div>"
        )
    else:
        parts.append("<ul style=\"list-style:none;padding:0;margin:0;\">")
        for t in open_todos:
            parts.append(
                "<li style=\"padding:6px 0;font-size:14px;color:#1f2328;"
                "border-bottom:1px solid #f6f7f9;\">"
                "<span style=\"display:inline-block;width:14px;height:14px;"
                "border:1.5px solid #d1d5db;border-radius:3px;vertical-align:middle;"
                "margin-right:10px;\"></span>"
                f"{_escape_html(t.title)}</li>"
            )
        parts.append("</ul>")

    parts.append(
        "<h2 style=\"margin:24px 0 12px 0;font-size:16px;color:#111827;"
        "border-bottom:1px solid #f0f1f3;padding-bottom:6px;\">Notes "
        f"<span style=\"color:#6b7280;font-weight:500;\">({len(live_notes)})</span></h2>"
    )

    if not live_notes:
        parts.append(
            "<div style=\"color:#9ca3af;font-style:italic;font-size:14px;\">none</div>"
        )
    else:
        for n in live_notes:
            parts.append(
                "<div style=\"margin-top:14px;padding:12px 14px;background:#fafbfc;"
                "border:1px solid #eef0f2;border-radius:6px;\">"
                f"<div style=\"font-weight:600;font-size:14px;color:#111827;"
                f"margin-bottom:6px;\">{_escape_html(n.title)}</div>"
                f"<div style=\"font-size:13px;color:#374151;white-space:pre-wrap;"
                f"line-height:1.5;\">{_escape_html(n.body)}</div>"
                "</div>"
            )

    parts.append("</div></div></body></html>")
    return "".join(parts)

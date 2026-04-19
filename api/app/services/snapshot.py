from __future__ import annotations

from datetime import UTC, datetime

from ..models import Context, Note, Todo

ContextsData = list[tuple[Context, list[Todo], list[Note]]]


def _escape_html(s: str) -> str:
    return (
        s.replace("&", "&amp;")
        .replace("<", "&lt;")
        .replace(">", "&gt;")
        .replace('"', "&quot;")
        .replace("'", "&#39;")
    )


def format_plain(data: ContextsData, now: datetime | None = None) -> str:
    now = now or datetime.now(UTC)
    out: list[str] = [f"# todo-tui snapshot — {now.strftime('%Y-%m-%dT%H:%M:%SZ')}", ""]

    total_todos = sum(len(todos) for _, todos, _ in data)
    out.append(f"## Open todos ({total_todos})")
    for ctx, todos, _ in data:
        if not todos:
            continue
        out.append(f"\n### {ctx.slug}")
        for t in todos:
            out.append(f"- [ ] {t.title}")

    out.append("")
    total_notes = sum(len(notes) for _, _, notes in data)
    out.append(f"## Notes ({total_notes})")
    for ctx, _, notes in data:
        if not notes:
            continue
        out.append(f"\n### {ctx.slug}")
        for n in notes:
            out.append(f"\n#### {n.title}")
            out.append(n.body)

    return "\n".join(out) + "\n"


def format_html(data: ContextsData, now: datetime | None = None) -> str:
    now = now or datetime.now(UTC)
    stamp = _escape_html(now.strftime("%Y-%m-%d %H:%M UTC"))

    todo_groups = [(ctx, todos) for ctx, todos, _ in data if todos]
    note_groups = [(ctx, notes) for ctx, _, notes in data if notes]
    total_todos = sum(len(t) for _, t in todo_groups)
    total_notes = sum(len(n) for _, n in note_groups)

    p: list[str] = [
        "<!doctype html>",
        '<html><body style="margin:0;padding:24px;background:#f6f7f9;'
        "font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',Helvetica,Arial,sans-serif;"
        'color:#1f2328;">',
        '<div style="max-width:640px;margin:0 auto;background:#ffffff;'
        'border:1px solid #e5e7eb;border-radius:10px;overflow:hidden;">',
        # header
        '<div style="padding:20px 24px;border-bottom:1px solid #e5e7eb;background:#fafbfc;">',
        '<div style="font-size:12px;color:#6b7280;letter-spacing:0.06em;'
        'text-transform:uppercase;">todo-tui snapshot</div>',
        f'<div style="font-size:14px;color:#374151;margin-top:4px;">{stamp}</div>',
        "</div>",
        '<div style="padding:20px 24px;">',
        # todos section header
        '<h2 style="margin:0 0 12px 0;font-size:16px;color:#111827;'
        'border-bottom:1px solid #f0f1f3;padding-bottom:6px;">Open todos '
        f'<span style="color:#6b7280;font-weight:500;">({total_todos})</span></h2>',
    ]

    if not todo_groups:
        p.append('<div style="color:#9ca3af;font-style:italic;font-size:14px;">none</div>')
    else:
        for ctx, todos in todo_groups:
            p.append(
                '<div style="margin-bottom:16px;">'
                f'<div style="font-size:11px;font-weight:600;color:#6b7280;'
                f'letter-spacing:0.07em;text-transform:uppercase;margin-bottom:6px;">'
                f"{_escape_html(ctx.slug)}</div>"
                '<ul style="list-style:none;padding:0;margin:0;">'
            )
            for t in todos:
                p.append(
                    '<li style="padding:6px 0;font-size:14px;color:#1f2328;'
                    'border-bottom:1px solid #f6f7f9;">'
                    '<span style="display:inline-block;width:14px;height:14px;'
                    "border:1.5px solid #d1d5db;border-radius:3px;vertical-align:middle;"
                    f'margin-right:10px;"></span>{_escape_html(t.title)}</li>'
                )
            p.append("</ul></div>")

    # notes section header
    p.append(
        '<h2 style="margin:24px 0 12px 0;font-size:16px;color:#111827;'
        'border-bottom:1px solid #f0f1f3;padding-bottom:6px;">Notes '
        f'<span style="color:#6b7280;font-weight:500;">({total_notes})</span></h2>'
    )

    if not note_groups:
        p.append('<div style="color:#9ca3af;font-style:italic;font-size:14px;">none</div>')
    else:
        for ctx, notes in note_groups:
            p.append(
                '<div style="margin-bottom:20px;">'
                f'<div style="font-size:11px;font-weight:600;color:#6b7280;'
                f'letter-spacing:0.07em;text-transform:uppercase;margin-bottom:8px;">'
                f"{_escape_html(ctx.slug)}</div>"
            )
            for n in notes:
                p.append(
                    '<div style="margin-bottom:10px;padding:12px 14px;background:#fafbfc;'
                    'border:1px solid #eef0f2;border-radius:6px;">'
                    f'<div style="font-weight:600;font-size:14px;color:#111827;'
                    f'margin-bottom:6px;">{_escape_html(n.title)}</div>'
                    f'<div style="font-size:13px;color:#374151;white-space:pre-wrap;'
                    f'line-height:1.5;">{_escape_html(n.body)}</div>'
                    "</div>"
                )
            p.append("</div>")

    p.append("</div></div></body></html>")
    return "".join(p)

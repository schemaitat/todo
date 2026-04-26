# todo

Personal todo and notes manager. Terminal UI backed by plain markdown files — no server, no database, no internet connection required.

## How it works

Each todo or note is a directory containing a single `CONTENT.md` file. The directory name is the slug (derived from the title). A TOML front matter block holds metadata; everything below the closing `---` is free-form markdown.

```
<root_dir>/
  <context>/
    todos/
      my_first_todo/
        CONTENT.md
    notes/
      meeting_notes/
        CONTENT.md
  work/
    todos/
    notes/
```

Example `CONTENT.md` for a todo:

```markdown
---
title = "My first todo"
created_at = "2026-04-25T10:00:00Z"
done = false
description = "Optional one-line summary shown in the list"
---

Full markdown body here. Edited inside the TUI with the built-in
vim-like editor, or directly in any text editor or by an LLM.
```

Deleted items are soft-deleted: `deleted_at` is written into the front matter instead of removing the file, so nothing is ever lost.

## Installation

```bash
just build          # compiles, installs to ~/.local/bin/todo, writes default config
just install-config # config only (skips compile)
```

Or install manually:

```bash
cargo build --release
install -m 755 target/release/todo ~/.local/bin/todo
```

## Configuration

Config file: `~/.config/todo-tui/config.toml` (macOS: `~/Library/Application Support/todo-tui/config.toml`)

```toml
root_dir = "/Users/you/.local/share/todo-tui"
context_slug = "inbox"
```

| Key | Default | Description |
|---|---|---|
| `root_dir` | `~/.local/share/todo-tui` | Root directory for all data |
| `context_slug` | `inbox` | Active context on startup |

Inspect the active config:

```bash
todo config show
todo config init   # write a default config file if none exists
```

## Contexts

A context is a top-level subdirectory under `root_dir` (e.g. `inbox`, `work`, `personal`). Each context has its own `todos/` and `notes/` directories. Contexts are created automatically when you first switch to them.

Switch context from inside the TUI:

| Key / Command | Action |
|---|---|
| `C` | Open context browser |
| `cc` | Cycle to next context |
| `:ctx <name>` | Switch to context (creates it if new) |
| `:ctx new <name>` | Create a new context and switch |

## TUI key bindings

### Normal mode

| Key | Action |
|---|---|
| `h` / `l` | Switch focus between Todos and Notes panes |
| `j` / `k` | Move selection down / up |
| `gg` / `G` | Jump to first / last item |
| `Tab` | Toggle pane focus |
| `i` / `a` / `o` | Add new item |
| `r` | Rename selected item |
| `dd` | Delete selected item (soft-delete) |
| `x` / `Space` / `Enter` | Toggle todo done |
| `v` | View todo body |
| `e` | Edit item body (opens vim-like editor) |
| `/` | Live filter across both panes |
| `Esc` | Clear filter |
| `C` | Open context browser |
| `cc` | Cycle context |
| `:` | Enter command mode |
| `q` | Quit |

### Editor (vim-like)

| Key | Action |
|---|---|
| `i` / `a` / `o` | Enter insert mode |
| `Esc` | Return to normal mode |
| `hjkl` | Move cursor |
| `dd` | Delete line |
| `yy` / `p` | Yank / paste line |
| `u` | Undo |
| `v` | Visual mode |
| `:w` | Save and close |
| `:q` | Discard and close |

### Commands

| Command | Action |
|---|---|
| `:q` / `:quit` / `:wq` | Quit |
| `:todo` / `:note` | Focus that pane |
| `:ctx <name>` | Switch context |
| `:ctx new <name>` | Create context |
| `:reload` | Re-read all files from disk |
| `:clear` | Clear active filter |
| `:help` | Show key reference in status bar |

## Development

```bash
just dev    # cargo run -p todo-tui
just qc     # fmt + clippy
just test   # cargo test --workspace
```

## Data portability

Because every item is a plain markdown file, the data directory can be:

- Synced with any file-sync tool (Syncthing, Dropbox, iCloud Drive, git)
- Edited by hand or by an LLM
- Backed up with a simple `cp -r` or `rsync`
- Committed to a private git repo for full history

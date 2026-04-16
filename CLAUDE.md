# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

`todo-tui` is a single-binary Rust TUI (built on `ratatui` + `crossterm`) that manages todos and notes in a k9s-style split-pane interface with vim keybindings.

## Commands

- `cargo run` — debug build + launch
- `cargo build --release` — produces `target/release/todo-tui`
- `just todo` — launches the release binary (recipe lives in `~/.justfile`, not the repo); rebuild release before relying on it
- No test suite exists yet (`cargo test` will pass with zero tests)

## Data persistence

State is JSON-serialized via `serde_json` to `dirs::data_local_dir()/todo-tui/data.json` (e.g. `~/Library/Application Support/todo-tui/data.json` on macOS). `storage::save` writes through a `.tmp` sibling + rename for atomicity. Any code path that mutates `app.store` must call `App::save` immediately after — the app does not save on exit beyond the final write triggered by `should_quit`.

## Architecture

Three-file layout in `src/`:

- `main.rs` — terminal setup/teardown (raw mode + alt screen) and the `App::run` handoff
- `storage.rs` — `Store { todos, notes }` plus `Todo` / `Note` types and `load`/`save`
- `app.rs` — all state and event handling
- `ui.rs` — pure rendering, reads `&App`, never mutates

### Mode state machine (`app.rs`)

`App.mode: Mode` is the central dispatcher. `handle_key` routes to one of `handle_normal`, `handle_command`, `handle_search`, `handle_input`, `handle_note_view`, `handle_note_edit`. Each mode owns its own buffer field on `App` (`command_buffer`, `filter`, `input_buffer`, `note_buffer`) — keep this separation when adding new modes rather than reusing one buffer.

Multi-key vim sequences (`dd`, `gg`) use the `pending_d` / `pending_g` flags, consumed via `mem::replace` at the top of `handle_normal`.

### Filter / selection invariant

`filter` is a live substring filter applied to **both** panes simultaneously. The UI renders `visible_todo_indices()` / `visible_note_indices()`; `todo_index` / `note_index` are always indices into the **full** `Vec`, not the filtered view. After any operation that can change visibility (filter edit, deletion), call `snap_selection()` to ensure the current index points to a visible item. Navigation in `move_selection` walks the visible-indices list and translates back.

`/` enters search mode after backing up the current filter into `filter_backup`; `Esc` restores it, `Enter` commits.

### Popup rendering gotcha (`ui.rs`)

`Clear` resets cells to terminal-default style, which renders as invisible text on some themes. All popups (`draw_input`, `draw_note_view`, `draw_note_edit`) explicitly set `popup_style()` (an RGB bg+fg pair) on the `Block`, the `Paragraph`, and every `Span` inside. When adding new popups, do the same — do not rely on `Clear` alone.

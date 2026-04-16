# CLAUDE.md

Guide Claude Code (claude.ai/code) for code in repo.

## What this is

`todo-tui` = single-binary Rust TUI (`ratatui` + `crossterm`). Manage todos+notes. k9s-style split-pane. vim keys.

## Commands

- `cargo run` — debug build + launch
- `cargo build --release` — produces `target/release/todo-tui`
- `just todo` — launch release binary. Recipe in `~/.justfile`, not repo. Rebuild release first.
- No test suite yet (`cargo test` passes zero tests)

## Data persistence

State JSON via `serde_json` to `dirs::data_local_dir()/todo-tui/data.json` (macOS: `~/Library/Application Support/todo-tui/data.json`). `storage::save` writes `.tmp` sibling + rename = atomic. Any mutation of `app.store` MUST call `App::save` after — no save on exit beyond final write from `should_quit`.

## Architecture

3-file layout `src/`:

- `main.rs` — terminal setup/teardown (raw mode + alt screen), `App::run` handoff
- `storage.rs` — `Store { todos, notes }` + `Todo` / `Note` types + `load`/`save`
- `app.rs` — all state + event handling
- `ui.rs` — pure render. Reads `&App`, never mutate.

### Mode state machine (`app.rs`)

`App.mode: Mode` = central dispatcher. `handle_key` routes to `handle_normal`, `handle_command`, `handle_search`, `handle_input`, `handle_note_view`, `handle_note_edit`. Each mode owns own buffer on `App` (`command_buffer`, `filter`, `input_buffer`, `note_buffer`) — keep separation for new modes, don't reuse one buffer.

Multi-key vim (`dd`, `gg`) use `pending_d` / `pending_g` flags, consumed via `mem::replace` at top of `handle_normal`.

### Filter / selection invariant

`filter` = live substring filter on **both** panes. UI renders `visible_todo_indices()` / `visible_note_indices()`; `todo_index` / `note_index` always index **full** `Vec`, not filtered view. After visibility change (filter edit, delete), call `snap_selection()` so index points to visible item. `move_selection` walks visible-indices list, translates back.

`/` = enter search mode, back up current filter to `filter_backup`; `Esc` restores, `Enter` commits.

### Popup rendering gotcha (`ui.rs`)

`Clear` resets cells to terminal-default style = invisible text on some themes. All popups (`draw_input`, `draw_note_view`, `draw_note_edit`) explicitly set `popup_style()` (RGB bg+fg pair) on `Block`, `Paragraph`, every `Span` inside. New popups = same — don't rely on `Clear` alone.
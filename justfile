default:
    @just --list

fmt:
    cargo fmt --all

lint:
    cargo clippy --all-targets --all-features --fix --allow-dirty

check: fmt lint

build:
    cargo build --release
    mkdir -p ~/.local/bin
    install -m 755 target/release/todo-tui ~/.local/bin/todo

email *args:
    cargo run --release --bin todo-mailer -- {{args}}

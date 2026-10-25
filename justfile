set shell := ["bash", "-uc"]

# Show available recipes
default:
    @just --list

# Preview a Markdown file
[group('run')]
[positional-arguments]
run *args:
    cargo run -- "$@"

# Lint with warnings denied
[group('dev')]
lint:
    cargo clippy --locked --all-targets -- -D warnings

# Format the workspace in place
[group('dev')]
fmt:
    cargo fmt --all

# Run the full test suite
[group('dev')]
test:
    cargo test --locked --all-targets

# `fmt` rewrites files, so the gate checks the format with its own command.

# Run the whole quality gate
[group('dev')]
check: lint test
    cargo fmt --all -- --check

# Build the release binary, sign it ad hoc, and copy it to dist/
[group('deploy')]
build:
    cargo build --locked --release
    mkdir -p dist
    cp target/release/mdlive dist/
    codesign --force --sign - dist/mdlive

# Install the dist binary to ~/self-made-bin/
[group('deploy')]
ship: build
    cp dist/mdlive ~/self-made-bin/

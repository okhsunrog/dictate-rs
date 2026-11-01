# Dictate build, run and verification commands.
prefix := env_var_or_default("PREFIX", env_var('HOME') / ".local")

default:
    @just --list

build:
    cargo build --workspace --locked

release:
    cargo build --workspace --release --locked

check:
    cargo fmt --all --check
    ruff check tests
    ruff format --check tests
    cargo clippy --workspace --all-targets --locked -- -D warnings
    cargo test --workspace --locked

fmt:
    cargo fmt --all

run *ARGS:
    cargo run -p dictate --locked -- {{ARGS}}

demo:
    cargo run -p dictate --locked -- demo

settings:
    cargo run -p dictate --locked -- settings

# Sends this audio file to the service using the saved config and DICTATE_* overrides.
e2e FILE LANG="":
    cargo run -p dictate-core --example e2e --locked -- {{FILE}} {{LANG}}

install: release
    install -Dm755 target/release/dictate {{prefix}}/bin/dictate
    install -Dm644 packaging/ru.okhsunrog.dictate.desktop {{prefix}}/share/applications/ru.okhsunrog.dictate.desktop

uninstall:
    rm -f {{prefix}}/bin/dictate {{prefix}}/share/applications/ru.okhsunrog.dictate.desktop

# Run these sessions sequentially so their Xvfb displays cannot collide.
ui-smoke: build
    xvfb-run -a -s '-screen 0 1280x1024x24' dbus-run-session -- uv run tests/ui_smoke.py
    xvfb-run -a -s '-screen 0 1280x1024x24' dbus-run-session -- uv run tests/ui_windows.py

# Requires Wayland and Niri; uses a private nested compositor.
wayland-smoke: build
    uv run tests/wayland.py

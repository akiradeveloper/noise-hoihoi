set shell := ["bash", "-eo", "pipefail", "-c"]

# Show the available project commands.
default:
    @just --list

# Build the optimized Windows installer with the pinned VB-CABLE package.
build-windows:
    bash ./scripts/docker/build-windows.sh

# Run formatting, lint, unit, and documentation checks.
check:
    cargo fmt --all --check
    cargo clippy --workspace --all-targets -- -D warnings
    cargo test --workspace
    RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
    bash -n scripts/docker/*.sh

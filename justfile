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

# Run NoiseNet's normal unit and Flex CPU suites with process isolation.
test-noisenet:
    cargo nextest run -p noise-net
    cargo test -p noise-net --doc

# Include long-running NoiseNet durability tests.
test-noisenet-full:
    cargo nextest run -P noisenet-full -p noise-net --run-ignored all

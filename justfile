set shell := ["bash", "-eo", "pipefail", "-c"]

# Show the available project commands.
default:
    @just --list

# Build the optimized Windows installer with the pinned VB-CABLE package.
build-windows:
    bash ./scripts/docker/build-windows.sh

# Build the Linux x86_64 AppImage (PipeWire/PulseAudio).
build-linux:
    bash ./scripts/docker/build-linux.sh

# Test virtual audio routing against an isolated PulseAudio or PipeWire server in Docker.
test-linux-audio server="pulseaudio":
    docker run --rm --volume "$PWD:/work" --volume noise-hoihoi-cargo-cache:/root/.cargo-cache --env CARGO_HOME=/root/.cargo-cache --env CARGO_TARGET_DIR=/work/target/linux --env CARGO_BUILD_JOBS=6 noise-hoihoi-linux-dev:local bash /work/scripts/docker/test-linux-audio.sh {{server}}

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

# Run the opt-in reference suite on every WGPU adapter in this machine.
test-noisenet-wgpu:
    cargo nextest run -p noise-net --run-ignored ignored-only -E 'binary(/model_wgpu$/)'

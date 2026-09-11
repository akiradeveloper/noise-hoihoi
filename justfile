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

# Test portable audio and application logic without native devices or GUI.
test-core:
    python3 scripts/check-architecture.py
    cargo test --locked -p noise-hoihoi-engine -p noise-hoihoi-session

# Run formatting, lint, unit, and documentation checks.
check:
    python3 scripts/check-architecture.py
    cargo fmt --all --check
    cargo clippy --workspace --all-targets -- -D warnings
    cargo test --workspace
    RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
    bash -n scripts/docker/*.sh

# Run model reset and waveform tests with the packaged IREE runtime.
test-noisenet:
    cargo test --release -p noise-net-iree --lib
    cargo test --release -p noise-net-iree --test streaming -- --ignored --nocapture

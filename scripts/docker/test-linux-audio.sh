#!/usr/bin/env bash
set -euo pipefail
# Run inside the Linux build container. Never attach these tests to user audio.
server="${1:-pulseaudio}"
if [[ "$server" == pipewire && -z "${DBUS_SESSION_BUS_ADDRESS:-}" ]]; then
    exec dbus-run-session -- bash "$0" pipewire
fi
export XDG_RUNTIME_DIR
XDG_RUNTIME_DIR="$(mktemp -d)"
export XDG_CONFIG_HOME="$XDG_RUNTIME_DIR/config"
export NOISE_HOIHOI_AUDIO_TEST=1
server_pids=()
cleanup() {
    local status=$?
    if ((status != 0)); then cat "$XDG_RUNTIME_DIR"/*.log >&2; fi
    for pid in "${server_pids[@]}"; do kill "$pid" 2>/dev/null || true; done
    for pid in "${server_pids[@]}"; do wait "$pid" 2>/dev/null || true; done
    rm -rf -- "$XDG_RUNTIME_DIR"
}
trap cleanup EXIT
if [[ "$server" == pipewire ]]; then
    # This private graph has no hardware or system logind/BlueZ service.
    mkdir -p "$XDG_CONFIG_HOME/wireplumber/main.lua.d" "$XDG_CONFIG_HOME/wireplumber/bluetooth.lua.d"
    cat > "$XDG_CONFIG_HOME/wireplumber/main.lua.d/51-test.lua" <<'LUA'
alsa_monitor.enabled = false
v4l2_monitor.enabled = false
libcamera_monitor.enabled = false
LUA
    echo 'bluez_monitor.enabled = false' > "$XDG_CONFIG_HOME/wireplumber/bluetooth.lua.d/51-test.lua"
    export PULSE_SERVER="unix:$XDG_RUNTIME_DIR/pulse/native"
    pipewire >"$XDG_RUNTIME_DIR/pipewire.log" 2>&1 &
    server_pids+=("$!")
    pipewire-pulse >"$XDG_RUNTIME_DIR/pulse.log" 2>&1 &
    server_pids+=("$!")
    wireplumber >"$XDG_RUNTIME_DIR/wireplumber.log" 2>&1 &
    server_pids+=("$!")
elif [[ "$server" == pulseaudio ]]; then
    export PULSE_SERVER="unix:$XDG_RUNTIME_DIR/pulse-test.sock"
    pulseaudio -n --daemonize=no --use-pid-file=no --exit-idle-time=-1 \
        --log-target="file:$XDG_RUNTIME_DIR/pulse.log" \
        --load="module-native-protocol-unix socket=$XDG_RUNTIME_DIR/pulse-test.sock auth-anonymous=1" &
    server_pids+=("$!")
else
    echo "Unknown test server: $server" >&2
    exit 1
fi
for _ in {1..100}; do
    if pactl info >/dev/null 2>&1; then break; fi
    sleep 0.05
done
pactl info >/dev/null
pactl load-module module-null-sink sink_name=test_sink rate=48000 channels=1 channel_map=mono >/dev/null
pactl load-module module-remap-source master=test_sink.monitor source_name=test_microphone channels=1 channel_map=mono >/dev/null
cargo test --locked --release -p noise-hoihoi-platform --test linux_audio -- --ignored --nocapture

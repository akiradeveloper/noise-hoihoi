# Linux AppImage (v0.8)

NoiseHoiHoi supports Linux x86_64 desktop sessions using PipeWire with
`pipewire-pulse`, or PulseAudio. The GUI supports Wayland and X11. The AppImage
is built on Debian 12 (glibc 2.36); use Debian 12 / Ubuntu 24.04 or a newer
compatible desktop distribution. ALSA-only sessions are not supported.

## Use

```sh
chmod +x NoiseHoiHoi-v0.8-x86_64.AppImage
./NoiseHoiHoi-v0.8-x86_64.AppImage
```

Choose your microphone under **Input**. **Output** shows **NoiseHoiHoi
Microphone**; select that same microphone in OBS, Discord, or your recording
application after NoiseHoiHoi reports **Running**. Keep NoiseHoiHoi open while
using it. Noise reduction supports CPU and physical Vulkan GPUs. Pass-through,
the performance check and Signal Monitor remain available. Both processors use
IREE; the native library and licenses are bundled under `usr/bin/iree`.
Python is not required. Vulkan drivers come from the host. The GPUI GUI needs
a working Vulkan or OpenGL renderer.

The AppImage includes the PulseAudio client library. It uses the existing
user-session audio server, with no VB-CABLE, root access, `pactl`, or manual
virtual-device setup required for normal use. The server must allow
`module-null-sink` and `module-remap-source`. Do not run the app with sudo.

For systems where FUSE is unavailable, use:

```sh
./NoiseHoiHoi-v0.8-x86_64.AppImage --appimage-extract-and-run
```

## Audio route and lifecycle

```text
Selected microphone -> 48 kHz mono -> NoiseNet or pass-through
    -> noise_hoihoi_output (null sink)
    -> NoiseHoiHoi Microphone (remapped source)
    -> streaming / voice-chat / recording software
```

A fixed 10 Hz high-pass filter removes microphone DC offset before either
processor and the signal monitor. This keeps biased Linux capture signals
centered on zero without a user adjustment, including in pass-through mode.

The source's stable internal name is `noise_hoihoi_microphone`. Sink monitors
and NoiseHoiHoi's own source are excluded from Input to prevent feedback. The
capture and playback streams stay attached to the selected endpoints; a lost
endpoint produces an error instead of routing to a default device. Click Retry
or select an available microphone to recover. Only one NoiseHoiHoi audio route
can run in the same desktop session.

Stopping/reconfiguring the engine or closing the GUI unloads its modules in
reverse order. Setting changes shut down the old route on a background thread
so audio-server or inference delays do not block the GUI. On reconfiguration the source is briefly recreated; recording
software that does not reconnect automatically may need to reselect the input.
The application never explicitly changes the server's default source or sink.

A force-kill or crash can leave modules in the audio server. The next startup
automatically removes modules whose type and complete arguments match the
NoiseHoiHoi route, while holding the session lock. A running instance and
endpoints created with other configurations are preserved. If an unknown
conflicting endpoint still prevents startup, close every NoiseHoiHoi instance
and inspect `pactl list short modules`, find the `module-remap-source`
entry with `source_name=noise_hoihoi_microphone` and the `module-null-sink` entry
with `sink_name=noise_hoihoi_output`, and unload those two numeric module IDs,
source first:

```sh
pactl unload-module SOURCE_MODULE_ID
pactl unload-module SINK_MODULE_ID
```

Then Retry. Logging out and back in also clears session modules. The `pactl`
utility is only needed for this diagnostic procedure.

## Build and verification

From the repository root, with Docker installed:

```sh
just build-linux
```

This runs formatting, workspace lint, application/engine unit tests, private
PulseAudio and PipeWire routing tests, and documentation checks before packaging. The audio
test generates a 440 Hz source, records the virtual microphone, checks its
level and signal-monitor delivery, and verifies repeated starts/stops, duplicate
instance rejection, cleanup, recovery after killing a child process, and CPU
noise reduction with continuous monitor collection. It does not access the user's microphone or
sound server. After building the Docker image, run it separately with
`just test-linux-audio` or `just test-linux-audio pipewire`.

Outputs:

- `out/NoiseHoiHoi-v0.8-x86_64.AppImage`
- `out/NoiseHoiHoi-v0.8-x86_64.AppImage.sha256`
- `out/linux/NoiseHoiHoi.AppDir/` (unpacked application)

Each successful build replaces these files and removes older Linux releases and
revision copies from `out/`. No revision archives are kept.

Rust dependencies are locked; the Docker base and packaging tools are pinned
by digest/checksum. Set `NOISE_HOIHOI_JOBS` to limit build parallelism (default
6). The container requires network access to retrieve dependencies and tools.
GL/EGL/Vulkan drivers and the audio server are provided by the host. Debian
copyright notices and exact package/source versions for bundled shared
libraries are in `licenses/system/`; the corresponding Debian source packages
can be retrieved with `apt-get source PACKAGE=SOURCE_VERSION` using Debian's
source repositories (or snapshot.debian.org for superseded versions). The
libraries remain dynamically linked and can be replaced in an extracted AppDir.

Before publishing, also verify physical USB/Bluetooth microphones, device
unplug/replug, OBS/Discord capture, native Wayland, and GPU inference on the
intended distributions. The private-server test cannot verify hardware or
application-specific reconnection behavior.

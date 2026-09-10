# NoiseHoiHoi application

The application separates the GPUI / gpui-component view (`noise-hoihoi-app`),
portable settings and lifecycle (`noise-hoihoi-session`), portable audio processing
(`noise-hoihoi-engine`), and native audio/persistence adapters
(`noise-hoihoi-platform`). See [architecture and test boundaries](../doc/architecture.md).
The engine and session can be tested without a display, microphone, audio server,
or GPU using `cargo test -p noise-hoihoi-engine -p noise-hoihoi-session`.

The engine runs either bit-exact pass-through or NoiseNet. CPU inference uses
Flex; selecting a GPU exposes its WGPU runtime. Model and audio initialization
run off the GUI thread, and GPU models are warmed before routing starts.

Microphone and processor lists are loaded at startup. Restart NoiseHoiHoi after
connecting a new microphone or eGPU to update the available devices.

On Windows, audio flows from the selected microphone through a dedicated
processing thread to `CABLE Input (VB-Audio Virtual Cable)`. Recording software
uses the paired `CABLE Output` endpoint. The optional signal monitor shows
delay-aligned input, output, their difference, and real-time health metrics.

On Linux, the engine connects directly to the session's PulseAudio-compatible
server (PulseAudio or PipeWire with pipewire-pulse). It creates a null sink and
remapped source named `NoiseHoiHoi Microphone`, and removes them when the
engine stops. Capture and playback use 48 kHz mono PCM with bounded buffers
and the same processing worker as Windows. Wayland and X11 share the GPUI GUI.
See [AppImage distribution](../packaging/linux/README.md) for usage and builds.

## v0.7 GUI

The GPUI control panel uses Select and Switch components, follows the desktop
light/dark appearance, and opens Signal Monitor in a separate native window.
The selected processor's supported runtime is chosen automatically. GUI
rendering and NoiseNet compute device selection are independent.

Settings are stored as `settings.json` in the application's data directory
(`~/.local/share/noisehoihoi` on Linux, `%APPDATA%/NoiseHoiHoi/data` on Windows).
When it does not exist, v0.7 reads v0.6's `app.ron` settings without modifying
that file. Changes are saved with an atomic replacement.

Audio startup, reconfiguration, and shutdown run off the UI thread. The control
panel and open monitor refresh at 20 Hz. Closing only the monitor disables
sample collection; closing the main window waits for route cleanup and exits.
Waveforms retain min/max amplitudes per horizontal pixel, including short
impulses. Input, output, and difference share one scale, determined by the
largest peak across all three signals. Difference includes any voice changes
as well as removed noise.

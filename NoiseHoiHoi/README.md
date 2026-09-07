# NoiseHoiHoi application

The application is split into two crates. `noise-hoihoi-app` owns the egui UI
and persisted settings. `noise-hoihoi-engine` owns audio devices, buffering,
resampling, processing, and runtime metrics.

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
and the same processing worker as Windows. Wayland and X11 share the egui GUI.
See [AppImage distribution](../packaging/linux/README.md) for usage and builds.

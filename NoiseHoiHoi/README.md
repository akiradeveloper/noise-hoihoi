# NoiseHoiHoi application

The application separates the GPUI / gpui-component view (`noise-hoihoi-app`),
portable settings and lifecycle (`noise-hoihoi-session`), portable audio processing
(`noise-hoihoi-engine`), and native audio/persistence adapters
(`noise-hoihoi-platform`).
The engine and session can be tested without a display, microphone, audio server,
or GPU using `cargo test -p noise-hoihoi-engine -p noise-hoihoi-session`.

The v0.8 application runs pass-through or the official DPDFNet-8 48 kHz model
through `noise-net-iree`. CPU uses IREE local-sync; GPU uses IREE Vulkan on both
Windows and Linux. Content delay is 50 ms, plus device/worker buffering.
No speaker registration, voice recovery or gain adaptation is added.

New installations default to CPU; available explicit processor selections are
retained. Unavailable processor IDs fall back to CPU during settings resolution.
Only processor names appear in the panel, diagnostics and copied reports.
Model loading and warm-up run off the GUI thread before audio routing starts.
Inference errors fault the route instead of publishing a failed frame.

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

## Performance check

With noise reduction running, **Check performance (10 s)** observes the active
microphone route under the current system load. Audio continues; the check does
not create another model or run competing inference. Use it while the usual
game/streaming applications are running. **Cancel check** stops observation,
and **Copy result** copies the processor name, timing and route counters.

Results use only counter changes during the observation: mean processor-call
time, calls exceeding the 10 ms block budget, dropped input frames, inserted
silence frames, and stream discontinuities. Frame counts for dropped input and
silence are audio frames, not 10 ms inference calls. The displayed mean includes
the model's FFT/inference/synthesis and GPU synchronization; audio transport and
resampling are reflected through the discontinuity/drop counters instead.

The headroom indication requires at least 90% of the expected processing calls,
no observed interruptions or deadline misses, and a mean at most 8 ms (20%
nominal compute margin). A mean of 10 ms or more cannot sustain the stream.
Insufficient data, a stopped/restarted route, or a fault cannot receive a passing
result. Settings changes clear previous results. This checks current
performance; it does not assess suppression quality or guarantee performance
under future load. The existing Signal Monitor retains lifetime counters.

## GUI

The GPUI control panel uses Select and Switch components, follows the desktop
light/dark appearance, and opens Signal Monitor in a separate native window.
The selected processor's supported runtime is chosen automatically. GUI
rendering and NoiseNet compute device selection are independent.

Settings are stored as `settings.json` in the application's data directory
(`~/.local/share/noisehoihoi` on Linux, `%APPDATA%/NoiseHoiHoi/data` on Windows).
Missing settings use defaults. Changes are saved with an atomic replacement.

Audio startup, reconfiguration, and shutdown run off the UI thread. The control
panel and open monitor refresh at 20 Hz. Closing only the monitor disables
sample collection; closing the main window waits for route cleanup and exits.
Waveforms retain min/max amplitudes per horizontal pixel, including short
impulses. Input, output, and difference share one scale, determined by the
largest peak across all three signals. Difference includes any voice changes
as well as removed noise.

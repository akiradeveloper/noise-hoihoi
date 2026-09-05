# NoiseHoiHoi application

The application is deliberately split into two crates:

- `noise-hoihoi-engine` owns audio devices, buffering, resampling, processing,
  and runtime metrics.
- `noise-hoihoi-app` owns only the egui user interface and persisted UI state.

The engine uses `PassThrough` through v0.2. NoiseNet will implement the same
`AudioProcessor` interface in v0.3. Version 0.2 adds a separate signal-monitor
window backed by a bounded diagnostic ring owned by the engine.

The engine owns both audio streams and its processing thread, so dropping it
stops routing immediately. Audio callbacks only convert and transfer samples;
resampling and future model inference run on the worker thread.

On Windows, the output stream is fixed to VB-CABLE's
`CABLE Input (VB-Audio Virtual Cable)` playback endpoint. Applications consume
the paired `CABLE Output (VB-Audio Virtual Cable)` recording endpoint. The
VB-CABLE device remains installed when NoiseHoiHoi exits, but carries silence
because the application output stream has stopped.

The Windows engine is split into device discovery, CPAL stream callbacks, and
the processing worker. Audio callbacks only convert samples and move them
through lock-free rings. Resampling, clock-drift correction, pass-through, and
future NoiseNet inference run on the worker thread.

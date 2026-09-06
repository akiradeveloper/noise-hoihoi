# NoiseHoiHoi application

The application is deliberately split into two crates:

- `noise-hoihoi-engine` owns audio devices, buffering, resampling, processing,
  and runtime metrics.
- `noise-hoihoi-app` owns only the egui user interface and persisted UI state.

The engine selects either bit-exact `PassThrough` or NoiseNet's DeepFilterNet3
processor through the same `AudioProcessor` interface. The GUI persists the
noise-reduction toggle. Its separate signal-monitor window is backed by a
bounded diagnostic ring owned by the engine.

The engine owns both audio streams and its processing thread, so dropping it
stops routing immediately. Audio callbacks only convert and transfer samples;
resampling and model inference run on the worker thread.

On Windows, the output stream is fixed to VB-CABLE's
`CABLE Input (VB-Audio Virtual Cable)` playback endpoint. Applications consume
the paired `CABLE Output (VB-Audio Virtual Cable)` recording endpoint. The
VB-CABLE device remains installed when NoiseHoiHoi exits, but carries silence
because the application output stream has stopped.

The Windows engine is split into device discovery, CPAL stream callbacks, and
the processing worker. Audio callbacks only convert samples and move them
through lock-free rings. Resampling, clock-drift correction, pass-through, and
NoiseNet inference run on the worker thread. Variable resampler chunks are
assembled into NoiseNet's exact 480-sample frames. Monitor input is delayed by
the processor latency so its difference trace compares the same audio content.

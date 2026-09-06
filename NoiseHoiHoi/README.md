# NoiseHoiHoi application

The application is split into two crates. `noise-hoihoi-app` owns the egui UI
and persisted settings. `noise-hoihoi-engine` owns audio devices, buffering,
resampling, processing, and runtime metrics.

The engine runs either bit-exact pass-through or NoiseNet. CPU inference uses
Flex; selecting a GPU exposes its WGPU runtime. Model and audio initialization
run off the GUI thread, and GPU models are warmed before routing starts.

On Windows, audio flows from the selected microphone through a dedicated
processing thread to `CABLE Input (VB-Audio Virtual Cable)`. Recording software
uses the paired `CABLE Output` endpoint. The optional signal monitor shows
delay-aligned input, output, their difference, and real-time health metrics.

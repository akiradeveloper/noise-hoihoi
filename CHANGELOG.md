# Changelog

## 0.5.0 - Unreleased

- Remove the fixed 12 dB attenuation limit that mixed controller and keyboard
  noise back into the output after model suppression.
- Keep both DeepFilterNet decoders and their recurrent histories running on
  every product frame, removing threshold-driven global muting and decoder skips.
- Adapt the level of model features and voice detection to quiet input while
  synthesizing from the original waveform, without changing output gain.
- Protect coherent weak harmonics across the full spectrum and account for
  phase cancellation when applying the speech response floor.
- Ease model attenuation briefly when recent speech and an abrupt sound are
  followed by a voice-detection gap, reducing short voice dropouts during
  controller operation while preserving the model estimate's phase and zeros.
  Recovery is bounded and can leave more residual noise during overlap.
- Preserve the 30 ms algorithmic delay and the independent upstream-reference
  processing path. Model weights remain unchanged and inference-only.
- Add quiet-voice, level-transition, high-harmonic and phase-preservation tests.
- Add a strong-controller overlap regression that checks short voice intervals
  and reference error together, plus recovery gain and expiry tests.
- Add `noise-bench` for hashed speech/noise mixtures, product/reference runs,
  continuous-stream evaluation and scoring recordings from external processors.
- Use the updated project WAVs in voice/mixed-noise regression tests, and allow
  explicit source-channel selection in the benchmark without automatic downmix.
- Align and smooth voice protection without adding user-facing tuning controls;
  the protection floor rejects short spectral bursts using the existing lookahead.
- Add recorded controller and voice regression fixtures, including speech
  overlap, different click levels and offsets, and voice-preservation checks.

## 0.4.0 - 2026-09-06

- Added a standalone NoiseNet runtime crate that discovers the host CPU with
  `sysinfo`, discovers selectable GPUs through WGPU, and creates matching Burn
  Flex or WGPU devices.
- Added Windows WGPU inference over DX12 for discrete, integrated, and virtual
  GPUs, including model warm-up before audio routing begins.
- Added persisted processor selection. CPU uses Flex without a runtime control;
  GPU selection reveals the WGPU runtime control.
- Moved model and audio initialization off the GUI thread and added a visible
  starting state.
- Added maximum processor-call time and real-time deadline misses to the signal
  monitor, plus an opt-in WGPU reference suite and processor-aware NoiseNet CLI.
- Limited noise attenuation to a fixed 12 dB by mixing back the delay-aligned
  microphone spectrum, protecting speech that the model misclassifies.
- Added a fixed sustained-voice guard that bypasses model processing after
  detecting strong 70-400 Hz periodicity, without adding a tuning control.
- Reused NoiseNet spectrum buffers instead of allocating replacements on every
  10 ms frame.

## 0.3.0 - 2026-09-06

- Added streaming DeepFilterNet3 inference in the standalone NoiseNet crate
  using Burn 0.22.0-pre.3 and its Flex CPU backend.
- Added embedded official model weights, a standalone WAV processor, pinned
  official-reference tests, DSP/state tests, and an opt-in CPU durability test.
- Added cargo-nextest profiles that serialize model suites.
- Added a persisted noise-reduction On/Off control to NoiseHoiHoi.
- Added fixed-frame adaptation and processor-delay alignment to the audio worker
  and signal monitor.
- Made the signal-monitor close button hide its native window immediately.

## 0.2.0 - 2026-09-05

- Added a separate signal-monitor window showing the latest second of aligned
  processor input, output, and their difference.
- Added a main-window button for opening or focusing the signal monitor.
- Added audio-health diagnostics without blocking the real-time audio route.

## 0.1.0 - 2026-09-05

- Added a Windows 11 x64 egui application that forwards a selected microphone
  to VB-CABLE while the window is open.
- Added 48 kHz mono resampling, bounded lock-free buffers, clock-drift
  correction, runtime metrics, and pass-through processing.
- Added local Docker cross-building through `just build-windows`.
- Added an NSIS installer that embeds the pinned official VB-CABLE Package 45,
  installs it only when absent, and preserves it during uninstall.
- Added a standalone Windows VB-CABLE audio smoke test.

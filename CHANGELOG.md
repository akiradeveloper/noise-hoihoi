# Changelog

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

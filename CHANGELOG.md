# Changelog

## 0.1.0 - 2026-09-05

- Added a Windows 11 x64 egui application that forwards a selected microphone
  to VB-CABLE while the window is open.
- Added 48 kHz mono resampling, bounded lock-free buffers, clock-drift
  correction, runtime metrics, and pass-through processing.
- Added local Docker cross-building through `just build-windows`.
- Added an NSIS installer that embeds the pinned official VB-CABLE Package 45,
  installs it only when absent, and preserves it during uninstall.
- Added a standalone Windows VB-CABLE audio smoke test.

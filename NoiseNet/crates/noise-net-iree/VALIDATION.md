# v0.8 IREE validation, 2026-09-11

The GPU model rebuild is byte-for-byte identical to the accepted production
module (`608107648ded282475db63d8001cd06bcd0c2e3d3c6ee3a0d6cba6c8c4c95176`).
The CPU modules and initial normalization state are regenerated from the same
pinned official ONNX file. All shipped asset hashes are in `model/SHA256SUMS.txt`.

CPU and GPU waveform fixtures, reset, independent sessions and thread handoff
pass with the standalone realfft STFT. The STFT overlap-add reconstruction test
also passes. Native CPU inference under Wine passes for both baseline x86-64
and AVX2/FMA modules: finite output, exact reset repeatability and byte-identical
spectral output to Linux over a 40-frame deterministic input sequence.
This exercises the embedded ELF loader and Windows/System V ABI bridge.

On Linux Radeon 680M (RADV REMBRANDT), Ryzen 7 7735U:

- 120-second paced stream, 11,999 measured frames: mean 6.531 ms, p95 7.244 ms,
  p99 8.099 ms, maximum 13.317 ms. One processing call exceeded 10 ms;
  two arrival deadlines were missed. This is not a zero-drop guarantee.
- Over 5,760,000 samples, the standalone STFT output differs from the accepted
  production stream by peak 3.57628e-7 and relative L2 1.23757e-7.
- CPU AVX2/FMA, 299 measured frames: mean 15.980 ms, p95 16.462 ms,
  maximum 17.270 ms. All measured frames exceeded the 10 ms budget.

Current measurement JSON is in `out/validation`. The paced stream includes
analysis FFT, inference, GPU transfer, synchronized readback and synthesis FFT.
It excludes initial warm-up, file I/O, microphone transport and GUI contention.
The in-app diagnostic observes the actual audio route under the user's load.

On the Ryzen 7 7735U, the CPU model currently exceeds the 10 ms frame budget.
Use the Radeon 680M GPU for real-time noise reduction on this machine. CPU
selection remains available for faster CPUs and diagnostics; CPU speed is not
guaranteed. Gaming or other system load can also affect GPU deadlines.

Windows native inference is checked under Wine. Physical Windows GPU performance
requires the target machine; Linux measurements do not establish Windows timing.

Release verification passes for the Linux AppImage and Windows installer.
Both executables embed the exact three verified bytecode modules; bundled native
libraries match those tested. Payload and dependency audits find no removed
inference implementations. Workspace formatting/clippy, architecture boundaries,
application/engine/session/platform tests, rustdoc, and isolated PulseAudio and
PipeWire routing tests pass. The platform's physical-GPU waveform test reports
RMSE 3.63762e-7 and peak 4.604459e-6 against the independent fixture.

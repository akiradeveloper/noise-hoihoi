# NoiseNet

The v0.8 inference implementation is [noise-net-iree](crates/noise-net-iree/README.md).
It runs the official DPDFNet-8 FP32 model through IREE on CPU or Vulkan GPU,
on Windows and Linux. The model, normalization state and CPU/GPU bytecode are
embedded; no model download is needed at startup.

Streaming uses 48 kHz mono, 480-sample hops and a 960-point Vorbis-window STFT.
Content delay is 2400 samples (50 ms), plus audio-device and worker buffering.
No speaker enrollment, gain adaptation or voice-recovery heuristic is applied.

The crate includes standalone waveform regression tests and a paced streaming
probe. See its README for native-library setup and test commands, and
[model compilation](../tools/compile-iree-model/README.md) for reproducible inputs.

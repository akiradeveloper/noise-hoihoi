# Independent waveform regression fixture

`dpdfnet8-input.f32le` and `dpdfnet8-output.f32le` are mono 48 kHz little-endian
float32. Expected output was recorded from the official DPDFNet-8 model with an
independent implementation. These data test the current IREE stream, including
normalization, FFT scaling, delay and state; no reference runtime is required.

The input uses 64 speech hops, deterministic keyboard-like clicks, quieter
speech at hops 24–48 (gain 0.063095734), and eight silent tail hops.
Speech is Ian Skillen's LibriVox reading of *The Clue of the Twisted Candle*,
obtained from Voice Zero, which dedicates its voices to CC0 1.0:

- https://github.com/OwenTyme/voice-zero
- https://librivox.org/the-clue-of-the-twisted-candle-by-edgar-wallace/
- https://creativecommons.org/publicdomain/zero/1.0/

Original FLAC SHA-256: `a0f5709389a3cdf2105f594e2d4e6efbd145c1775cc6594b50a7ac5e50a1e2b7`.
Resampled source WAV SHA-256: `0447b287e32ac9b8a3eae6439e7bb3e57088013be964a41c50c31be9f411393f`.
Fixture hashes are in `SHA256SUMS.txt`. These fixtures are not installed.

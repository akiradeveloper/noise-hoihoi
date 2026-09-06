# NoiseNet

NoiseNet is a standalone streaming DeepFilterNet3 inference implementation. It
uses exactly Burn `0.22.0-pre.3` and its Flex CPU backend for NoiseHoiHoi v0.3.
It owns no audio devices or GUI state.

The public stream accepts mono 48 kHz audio in 480-sample (10 ms) frames. Its
FFT size is 960, DeepFilterNet lookahead is two frames, and total algorithmic
latency is 1,440 samples (30 ms). Encoder and decoder GRU state, normalization
state, convolution history, and overlap-add state persist between calls.

The embedded Burnpack weights were converted from the official
`DeepFilterNet3_onnx.tar.gz` archive:

- Archive SHA-256: `c94d91f70911001c946e0fabb4aa9adc37045f45a03b56008cb0c8244cb63616`
- Upstream revision used for the reference runner:
  `d375b2d8309e0935d165700c91da9de862a99c31`
- Burnpack SHA-256 values are recorded in `model/SHA256SUMS.txt`.

## Tests

```sh
just test-noisenet
```

The normal suite checks DSP normalization and ERB layout, mask application,
exact analysis/synthesis latency, runtime stage thresholds, state reset, finite
CPU output, and output parity against a pinned official DeepFilterNet3 tract
reference. Current reference error is around `1.35e-5` RMSE and `3.05e-5`
maximum absolute error; the checked limits are `5e-5` and `2e-4` respectively.

An end-to-end quality test mixes deterministic keyboard clicks into a CC0 human
speech recording. It processes clean and mixed copies independently, then
requires the click-induced residual to fall by at least 3 dB. This avoids
mistaking the model's ordinary speech coloration for residual noise. Fixture
origin, license, and hashes are recorded in `testdata/README.md`.

The model test is assigned to a single-threaded cargo-nextest test group so
multiple model/device instances do not contend with each other. A longer,
ignored durability test is available explicitly:

```sh
just test-noisenet-full
```

To process a file outside NoiseHoiHoi:

```sh
cargo run -p noise-net --release -- input-48khz.wav output.wav
```

The CLI averages channels to mono, requires 48 kHz input, flushes the streaming
state, and removes the documented algorithmic delay from the output file.

Burn upgrades are intentional: update the exact workspace dependency, convert
the three ONNX graphs again, record the new Burnpack hashes, then regenerate the
official reference and run both test profiles. NoiseNet does not compare Burn
backends with one another; backend validation belongs to Burn itself.

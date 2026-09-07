# NoiseNet

NoiseNet is a standalone streaming DeepFilterNet3 inference implementation. It
uses exactly Burn `0.22.0-pre.3`. Its `noise-net-runtime` crate discovers the
host CPU and WGPU adapters, then creates either a Flex CPU device or a WGPU GPU
device. It owns no audio devices or GUI state.

The public stream accepts mono 48 kHz audio in 480-sample (10 ms) frames. Its
FFT size is 960, DeepFilterNet lookahead is two frames, and total algorithmic
latency is 1,440 samples (30 ms). Encoder and decoder GRU state, normalization
state, convolution history, and overlap-add state persist between calls.
The product path runs both decoders on every frame, including pauses. It does
not freeze their histories at local-SNR thresholds or globally mute a frame
classified as noise. An approximately two-second peak envelope adapts the
model-feature level with at most 36 dB of gain; synthesis uses the original
spectrum, so this is not an output AGC. The embedded weights are fixed and no
training occurs during use.

A 30 ms periodicity detector searches lags corresponding to 70-400 Hz (higher
pitches can also match at multiple periods). After three consecutive positive
frames, the product protects prominent low/mid bands and coherent harmonics
across the spectrum. Coherence measures consistency of adjacent spectral phase
advances, allowing weak upper harmonics to survive without requiring them to
be loud relative to the fundamental. This supplements the learned model; it
does not guarantee preservation of unvoiced consonants or separate every
overlapping sound.

The protection floor uses the second-smallest magnitude from the existing five
spectra around the output frame, preventing short clicks from establishing
their own floor. Interpolation constrains the response along the original
phase to account for phase cancellation. Detector decisions are aligned with
the output spectrum and fade in over 20 ms and out over 50 ms. This adds no
algorithmic delay or tuning controls. `process_frame_unprotected` retains the
pinned upstream thresholds and feature scaling for reference diagnostics;
reset the stream before switching processing paths.

Controller overlap can interrupt periodicity detection while speech continues.
When both speech and a rapid 1-8 kHz power increase were detected within the
last 500 ms, the product briefly eases model attenuation during detection gaps.
It raises the model's magnitude response toward `g^0.65`, with at most a
fourfold (12 dB) boost, preserving the estimated phase and zero bins. The
adjusted magnitude cannot exceed the corresponding input-bin magnitude.
Recovery fades in over 20 ms and out over 50 ms, adds no delay, and expires
without renewed speech detection. It can leave more residual noise during
overlap; it cannot recover speech that the model has entirely removed.

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
reference. Current reference error is around `1.35e-5` RMSE and `3.06e-5`
maximum absolute error; the checked limits are `5e-5` and `2e-4` respectively.

An end-to-end quality test mixes deterministic keyboard clicks into a CC0 human
speech recording. It requires clean-speech RMS to remain within 6 dB and the
click-induced residual to fall by at least 3 dB. This compares against the
model's clean-speech output, though the difference can also include changes
in how speech is processed when noise is present.

Recorded controller and voice fixtures additionally require at least 30 dB
reduction for controller noise alone, and voice levels within 3 dB. Mixtures
with recorded voices and sustained vowels at several noise levels require at
least 8 dB reduction in noise-dominated spectral cells during speech. These
cells are selected from the known input components, with a 20 dB separation;
they measure residual noise away from the strongest voice components, not all
overlapping noise or perceived intelligibility. Fixture origin, license, and
hashes are recorded in `testdata/README.md`.

The voice-quality suite additionally checks clean speech at 0/-12/-24/-36 dB
input gain, output level and waveform error, short dropouts, reset semantics,
and a loud-to-quiet transition. DSP tests exercise a weak harmonic above 4 kHz
and opposite-phase interpolation. A reproducible file benchmark and external
processor comparison are available in `tools/noise-bench/README.md`.
The current `sample-sound` WAVs also exercise recorded voice preservation,
standalone keyboard/controller reduction and mixed-signal recovery. Tests
select the active left channel without averaging in the silent right channel.
A strong-controller regression at -10 dB input SNR checks 50 ms active-voice
intervals for severe level loss, together with a gain-sensitive reference SDR
limit so that passing through or amplifying noise cannot satisfy the test.
Recovery unit tests cover bounded gain, phase preservation, expiry and steady
background behavior.

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

List processors, then select one by its reported ID:

```sh
cargo run -p noise-net --release -- --list-processors
cargo run -p noise-net --release -- --processor PROCESSOR_ID input.wav output.wav
```

The ignored WGPU suite runs on every real selectable adapter. It checks the
official reference, sustained voice protection and quiet recorded product
output against the CPU path. Run on Linux/Vulkan and Windows/DX12 systems:

```sh
just test-noisenet-wgpu
```

The CLI averages channels to mono, requires 48 kHz input, flushes the streaming
state, and removes the documented algorithmic delay from the output file.

Burn upgrades are intentional: update the exact workspace dependency, convert
the three ONNX graphs again, record the new Burnpack hashes, then regenerate the
official reference and run the CPU and WGPU test profiles.

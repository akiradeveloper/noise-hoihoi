# Noise reduction benchmark

`noise-bench` compares exactly reproducible inputs against their known speech
component. It writes mono 48 kHz float WAVs, a JSON manifest with SHA-256 hashes,
CSV scores and a JSON report. It never normalizes, clips or time-stretches the
processed output. Outputs are rejected if nonfinite or the wrong length.

Prepare a suite from the current 48 kHz PCM WAVs. These stereo recordings
contain the signal on the left, so explicitly select channel 1. The source
file hash and channel selection are saved; the original WAVs are untouched:

```sh
cargo run --release -p noise-bench -- prepare \
  --speech sample-sound/aiueo.wav \
  --noise sample-sound/controller.wav --noise sample-sound/keyboard.wav \
  --channel 1 --reference-kind recorded --output out/recorded-benchmark
```

Without `--channel`, sources must be mono. There is no automatic stereo
downmix, which would halve the level of a left-only recording. The current
voice and controller recordings contain clipped peaks; mixing prevents
additional clipping but cannot repair source distortion.

A separate mono speech reference can be prepared with:

```sh
cargo run --release -p noise-bench -- prepare \
  --speech NoiseNet/crates/noise-net/testdata/ian-skillen-clean-48k.wav \
  --noise NoiseNet/crates/noise-net/testdata/recorded-controller-48k.wav \
  --reference-kind clean --output out/voice-benchmark
```

Repeat `--speech` and `--noise` for multiple source files. The default grid
uses 0/-12/-24 dB input gains and 0/10/20 dB **whole-clip** speech-to-noise ratios,
including pauses. `--gain-db=0,-24`, `--snr-db=5,15` and `--noise-offset 3700`
change the grid reproducibly. Sources are cyclically aligned at a recorded
sample offset; this is not a model of room acoustics, clipping or microphone
AGC. Each mixture retains its exact speech and noise components. A common
headroom gain, recorded in the manifest, prevents clipping without changing
SNR. Sources containing background noise should use `--reference-kind recorded`
(the default): they are useful preservation references but are not isolated
clean speech. All output directories must be new; existing results are never
overwritten.
When headroom correction is active, the effective component gain can be
lower than `gain_db`; inspect `headroom_gain` when comparing input levels.
Different requested gains can produce identical mixtures after that correction.

Run the product, upstream reference, and pass-through controls on identical
inputs. The runner verifies the hashes and `speech + noise = input` first:

```sh
cargo run --release -p noise-bench -- run \
  --suite out/voice-benchmark/suite.json --output out/voice-product
cargo run --release -p noise-bench -- run --mode reference \
  --suite out/voice-benchmark/suite.json --output out/voice-reference
cargo run --release -p noise-bench -- run --mode passthrough \
  --suite out/voice-benchmark/suite.json --output out/voice-passthrough
```

Processor IDs are those printed by `noise-net --list-processors`; select one
with `run --processor ID`. In normal runs, state is reset between cases.
`run --continuous` concatenates the cases in manifest order and processes the
whole stream without resetting normalization, model or DSP state. Its single
final flush is excluded from the saved audio. This exercises transitions
between voice/noise levels and must be compared with another continuous run.
Case boundaries are artificial discontinuities, not natural speech joins.
The continuous runner currently buffers the concatenated waveform in memory;
use it for bounded evaluation sessions, not unattended multi-hour capture.

Reports include inference-call mean, p95, p99 and maximum milliseconds, plus
counts above the 10 ms deadline. They exclude model loading, file I/O, scoring,
device buffering and OS scheduling of an actual capture/playback route. Run
performance measurements without other builds or inference jobs competing for
the processor. The timing report is not an end-to-end latency measurement.

## Scores and limitations

- `reference_sdr_db`: `10 log10(sum(s²) / sum((y-s)²))`. Higher is better. It
  measures combined speech distortion and residual noise, retaining absolute
  gain and phase errors. It is not SI-SDR and not isolated residual-noise power.
- `input_sdr_db`: the same score before processing. Pass-through should match
  it; muting all speech gets 0 dB reference SDR rather than a perfect score.
- `output_attenuation_db`: total input/output energy attenuation. On voice-only
  inputs it detects voice level loss; on mixtures it cannot distinguish voice
  from noise. For noise-only cases it measures noise reduction.
- `active_drop6_percent` and `longest_drop6_ms`: 10 ms intervals losing over
  6 dB relative to speech. Activity is a fixed reference-energy heuristic
  (>4% of mean source power), not a phoneme annotation. Residual noise can
  obscure dropouts in mixed cases, so inspect voice-only cases as well.
- `speech_dominant_db` / `noise_dominant_db`: attenuation in time/frequency
  cells where one known component dominates the other by >20 dB, restricted to
  reference-active spectral windows. JSON includes the selected cell counts.
  These are proxies and exclude strong overlap between voice and noise.
- Band attenuation compares the output with the reference in 0-250 Hz,
  250 Hz-1 kHz, 1-2 kHz, 2-4 kHz, 4-8 kHz and 8-24 kHz bands. Background noise
  in recorded voice references also contributes; retaining it does not prove
  better voice quality. Band energy alone cannot establish natural timbre.
- Extremely small denominators use a numerical floor. Scores approaching
  200 dB are mathematical diagnostics, not physical acoustic dynamic range.

Use fixed, unseen source recordings/speakers for validation; split before
mixing or changing gain. Retain a development suite, a validation suite, real
simultaneous recordings and listening comparisons. Keep original output levels
for level-loss evaluation; separately loudness-matched blind listening can
help isolate timbre. Neither an aggregate score nor passing this small suite
establishes AMD equivalence.

## Comparing AMD Noise Suppression

Feed the manifest's `case-XXXX-input.wav` files through AMD Noise Suppression
using a verified digital audio route on a supported Windows system. Capture
the processed stream as mono 48 kHz WAV, keeping the manifest case IDs as names
(`case-XXXX.wav`). Disable other automatic gain/noise/echo processing and record
Adrenalin version, CPU/GPU selection and endpoint levels. A speaker-to-microphone
re-recording introduces a different acoustic input and is not the same test.
Confirm the chosen route actually enables AMD processing before collecting data.

Measure the route delay separately and retain exactly `case.samples + delay`
samples in each recording, including the leading delay. Avoid correlating each
clip independently to maximize its score. Then score with the same manifest:

```sh
cargo run --release -p noise-bench -- score \
  --suite out/voice-benchmark/suite.json --recordings out/amd-recordings \
  --output out/amd-scores --label 'AMD Adrenalin VERSION; PROCESSOR; SETTINGS' \
  --delay-samples MEASURED_DELAY
```

Use zero delay only for outputs already aligned and trimmed. The scorer rejects
missing, shortened or padded recordings; it does not guess alignment or hide
clock drift. The AMD output is a comparison target; the original isolated
speech remains the scoring reference. For continuous comparisons, the external
processor must also retain state across the same concatenated input, and its
aligned recording must be split at the manifest sample boundaries.

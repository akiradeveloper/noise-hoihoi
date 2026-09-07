# NoiseNet audio fixtures

## DeepFilterNet3 reference

`dfn3-reference-synthetic.wav` is the 16-bit, mono, 48 kHz output produced by
the official DeepFilterNet3 tract runner at revision
`d375b2d8309e0935d165700c91da9de862a99c31`. The waveform is defined once in
`reference_input.rs` and written by `examples/generate_reference_input.rs`; it
contains no third-party recording.

Reference runtime thresholds are the official defaults: minimum processing
threshold -10 dB, maximum ERB threshold 30 dB, and maximum deep-filter threshold
20 dB. The fixture and `process_frame_unprotected` use full upstream
suppression, without a raw-signal mix or product voice-band protection.
The official model archive SHA-256 is
`c94d91f70911001c946e0fabb4aa9adc37045f45a03b56008cb0c8244cb63616`.

Fixture SHA-256:
`1e9d20d19a5e6598c2f12ede8bd8beb00492b9197ccb69de58f448088d63b3b5`.

## Clean speech

`ian-skillen-clean-48k.wav` is a mono, 16-bit, 48 kHz resampling of
`voices/ian_skillen.flac` from Voice Zero revision
`490cfbee850a6d409076f477c766f567000a79b6`. Voice Zero dedicates the contents
of its `voices` directory to the public domain under CC0 1.0. The speaker is
Ian Skillen, reading track 22 of *The Clue of the Twisted Candle* for LibriVox.

Voice Zero describes noise removal, normalization and optional resynthesis in
its preparation pipeline. This is a processed reference, not an untouched
microphone recording; preservation on this fixture does not establish natural
speech quality on real noisy microphones.

- Voice Zero: https://github.com/OwenTyme/voice-zero
- Original LibriVox recording:
  https://librivox.org/the-clue-of-the-twisted-candle-by-edgar-wallace/
- CC0 1.0: https://creativecommons.org/publicdomain/zero/1.0/
- Source FLAC SHA-256:
  `a0f5709389a3cdf2105f594e2d4e6efbd145c1775cc6594b50a7ac5e50a1e2b7`
- Resampled WAV SHA-256:
  `0447b287e32ac9b8a3eae6439e7bb3e57088013be964a41c50c31be9f411393f`

The quality test creates deterministic keyboard clicks in memory, processes
both the clean and mixed signals, and measures their difference. Because the
model is nonlinear, that difference can include changes in speech processing
as well as residual clicks. No noisy output is checked into the repository.

## Project recordings

The `recorded-*-48k.wav` fixtures are complete mono, 16-bit, 48 kHz
conversions of the earlier M4A project recordings. Those source files have
since been replaced by the current WAV recordings in `sample-sound/`. They are
checked in so regression tests need no AAC decoder, GStreamer, or network.
No trimming, gain adjustment, or noise reduction is applied during conversion.

To reproduce a historical conversion, first restore the corresponding M4A
from project history, then use GStreamer (substitute names from the table):

```sh
gst-launch-1.0 -q filesrc location=sample-sound/xbox-controller.m4a ! decodebin ! \
  audioconvert dithering=none noise-shaping=none ! audioresample ! \
  audio/x-raw,format=S16LE,rate=48000,channels=1 ! wavenc ! \
  filesink location=NoiseNet/crates/noise-net/testdata/recorded-controller-48k.wav
```

| Source | Fixture |
|---|---|
| `sample-sound/xbox-controller.m4a` | `recorded-controller-48k.wav` |
| `sample-sound/low-a.m4a` | `recorded-low-a-48k.wav` |
| `sample-sound/high-a.m4a` | `recorded-high-a-48k.wav` |
| `sample-sound/sentense.m4a` | `recorded-speech-48k.wav` |

SHA-256 hashes:

- `xbox-controller.m4a`: `57a978485e45cdf8f358b33ab47ca290e93d4080e12fcf69e67a9a427cc7e0ff`
- `recorded-controller-48k.wav`: `78afe2debb0ea075de93fe08e38ee2ad757d06ec6af78f03dfaf38c6612f4ee8`
- `low-a.m4a`: `8c1aad6e48b702bcffe0954bbc6171ce5024c7179201defa80d0ad2b114f1f8d`
- `recorded-low-a-48k.wav`: `1f7473276870ce344fe57d12c416f2e28fd7217c387f9003f6e23e05bd5b1b07`
- `high-a.m4a`: `9636c410fcaa110260e54f635dc3ea182f1bd4b532aa2758568b9412244f39b4`
- `recorded-high-a-48k.wav`: `4965016800e06f656d61a15151aaf628387d53f8d943366f983475eecd3c1ba6`
- `sentense.m4a`: `9c0515aa237e3c533c0e9f87790682a9b4a397c6eafb2a6d6602dc43fb5fcb57`
- `recorded-speech-48k.wav`: `c66f98147e79509fb55a492bc66021c03cb8ac01bbbfb23f316adcbbb9dc44d5`

The recording suite measures controller noise alone and mixed with both
recorded speech and synthetic sustained vowels. It separately measures
noise-dominated and speech-dominated spectral cells with at least 20 dB
input separation, restricting noise measurements to active speech frames.
This avoids reporting changes between two nonlinear model outputs as pure
noise residual. It does not measure all noise overlapping voice harmonics
or guarantee subjective intelligibility.

## Current WAV recordings

`tests/voice_quality_cpu.rs` reads `sample-sound/aiueo.wav`, `controller.wav`
and `keyboard.wav` directly. It selects the left channel without gain changes;
the supplied right channels are effectively silent. These are recorded
preservation references, not certified clean speech. The voice and controller
recordings contain clipped source peaks.

The suite checks voice-only input at normal and quiet levels, operation sounds
alone, and voice mixed with each operation sound at 0 and 10 dB whole-clip SNR.
It scores combined waveform error against the known voice component without
gain normalization. `tools/noise-bench/README.md` describes the larger grid,
source hashes, explicit channel selection and external-processor comparison.

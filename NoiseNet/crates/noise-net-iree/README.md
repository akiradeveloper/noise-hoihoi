# IREE streaming inference

Windows and Linux use Rust → an owned C session → IREE. CPU runs embedded ELF
code through local-sync; physical GPUs use Vulkan. Processor selection is
explicit. Initialization errors are reported without changing the processor.

The model bytecode and normalization state are embedded in Rust. CPU selection
uses the AVX2/FMA module when both features are available, otherwise the baseline
x86-64 module. Both modules use a single synchronous inference thread. GPU state
stays on the selected device between frames. Vulkan drivers come from the OS;
CPU inference does not require Vulkan.

The native library loads from `iree/noise_iree.dll` or `iree/libnoise_iree.so`
beside the executable. Development override:
`NOISE_IREE_LIBRARY=/absolute/path/to/libnoise_iree.so`. The ABI is checked before
creating a session. A session has one owner, may move between threads, and
requires exclusive mutable access for every inference/reset call.

Build the native runtime inside the normal Linux/Windows Docker environment:
`scripts/build-iree.sh linux|windows STAGING_DIRECTORY`. Release packaging calls
this automatically. Python and model compilation tools are not shipped.

The 480-sample hop, 960-point STFT and 2400-sample content delay are fixed.
The realfft-based STFT uses a Vorbis window and overlap-add, with preallocated
buffers. Warm-up finishes before audio opens. Persistent staging buffers carry
spectrum input and synchronized output; 90,228 floats of state persist in IREE.

Hardware tests (release mode, native library required):

```sh
cargo test --release -p noise-net-iree --lib
cargo test --release -p noise-net-iree --test streaming -- --ignored --nocapture
cargo test --release -p noise-hoihoi-platform --test inference -- --ignored --nocapture
cargo run --release -p noise-net-iree --example stream -- input.f32le output.f32le paced cpu
cargo run --release -p noise-net-iree --example stream -- input.f32le output.f32le paced vulkan://GPU-DEVICE_UUID
```

The streaming probe accepts mono 48 kHz little-endian float32 audio. It excludes
file I/O and initial warm-up, includes FFTs, inference and synchronized readback,
and reports processing time and wall-clock deadline misses separately. It does
not measure microphone capture, GUI contention or audio-device buffers.
Use the in-app performance check under the intended gaming/streaming load.

See [validation](VALIDATION.md), [model provenance](model/PROVENANCE.md), and
[compiler tooling](../../../tools/compile-iree-model/README.md).

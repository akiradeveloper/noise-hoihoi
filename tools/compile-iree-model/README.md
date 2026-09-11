# Compile the production IREE modules

These are build tools; users receive embedded bytecode and the native runtime.
Use Python 3.12 with `iree-base-compiler==3.12.0rc20260910` (commit
`ce36167c3be514dd165a3ecff2d377cfa8eca0c9`) and `onnx==1.22.0`.
The GPU transform additionally needs glslangValidator 15.1.0.

```sh
python3 tools/compile-iree-model/build.py \
  --compiler-bin /absolute/path/to/venv/bin \
  --glslang /absolute/path/to/glslangValidator
```

Use `--backend cpu` or `--backend gpu` to compile a subset. Candidate modules
are written to `target/iree/model-assets`; the source assets are not overwritten.
The input is the checksum-verified official ONNX model in the IREE crate.

GPU transformation checks 16 frequency GRUs, 10 grouped matrix products and
2 temporal convolutions. The four FP32 compute kernels preserve model equations
and weights. Other operators use IREE O1. `vulkan-target.mlirattr` must match
`--iree-vulkan-target=rdna2` exactly. CPU transformation splits bidirectional GRUs
into forward GRUs with reversed input/output for the backward direction. It
emits baseline x86-64 and AVX2/FMA variants, using embedded ELF on both OSes.

VM source maps are stripped, but executable debug metadata can contain build
paths. A different build directory can change the bytecode hash. Run numerical,
reset/independence and sustained streaming checks documented in the IREE crate
before copying candidate modules into its model directory and updating
`model/SHA256SUMS.txt`. Release packaging rejects any asset hash mismatch.

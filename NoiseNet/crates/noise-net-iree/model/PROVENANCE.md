# DPDFNet-8 IREE assets

`dpdfnet8.onnx` is the unmodified official FP32 48 kHz model from Ceva-IP:
https://huggingface.co/Ceva-IP/DPDFNet/blob/9bd9844a227bb6aa57e55588d8d0e961fcff1c46/onnx/dpdfnet8_48khz_hr.onnx

Source SHA-256: `7b3afbb260a08fe9af3d16e3bda992971be1e7e951d1dee7c2d235f5c43f5631`.
DPDFNet is Apache-2.0 licensed; see `packaging/licenses/DPDFNet-LICENSE-APACHE-2.0.txt`.
Upstream code: https://github.com/ceva-ip/DPDFNet
`dpdfnet8-norm.f32le` contains the model's 577 initial normalization values;
the remainder of the 90,228-float initial state is zero.

Compiler/runtime: IREE 3.12.0rc20260910, commit
`ce36167c3be514dd165a3ecff2d377cfa8eca0c9`, Apache-2.0 WITH LLVM-exception.
Runtime dependencies: flatcc `9362cd00f0007d8cbee7bff86e90fb4b6b227ff3`,
Vulkan-Headers `df60f0316899460eeaaefa06d2dd7e4e300c1604`.

`tools/compile-iree-model/build.py` compiles three modules with unchanged weights:

- `dpdfnet8.vmfb`: Vulkan RDNA2, O1, FP32. Four GLSL kernels implement frequency
  GRU projection/recurrence, grouped matrix products and temporal convolutions.
  Remaining operators are lowered by IREE. GRU weights are transposed ahead of
  time for contiguous reads. The transform checks graph shapes and operator counts.
- `dpdfnet8-cpu.vmfb`: baseline x86-64 LLVM CPU, O1.
- `dpdfnet8-cpu-avx2.vmfb`: x86-64 with AVX/AVX2/FMA, O3, all IREE microkernels.
  CPU models split each bidirectional GRU into two forward GRUs with reversed
  input/output for the backward direction. Model equations and weights are retained.

All modules strip VM bytecode source maps. Windows and Linux embed identical
model bytes. The CPU modules use embedded ELF on both systems; the Windows shim
bridges Windows and System V calling conventions when built with Clang/MinGW.

The GPU module is qualified on Linux Radeon 680M. Other GPU/driver combinations
require measurement with the in-app diagnostic. No silent processor fallback
occurs after inference initialization. Check `SHA256SUMS.txt` for every asset.
Changing target attributes, compiler flags or shaders requires numerical and
continuous-stream validation; successful compilation alone is insufficient.

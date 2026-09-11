#!/usr/bin/env python3
"""Compile the production IREE CPU and GPU modules from the pinned ONNX model."""
import argparse
import hashlib
from pathlib import Path
import subprocess

root = Path(__file__).resolve().parents[2]
source = Path(__file__).resolve().parent
p = argparse.ArgumentParser(description=__doc__)
p.add_argument('--compiler-bin', type=Path, required=True,
               help='bin directory of iree-base-compiler 3.12.0rc20260910')
p.add_argument('--glslang', type=Path, help='glslangValidator 15.1.0 (required for GPU)')
p.add_argument('--backend', choices=('cpu', 'gpu', 'all'), default='all')
p.add_argument('--work', type=Path, default=root / 'target/iree/model')
p.add_argument('--output-dir', type=Path, default=root / 'target/iree/model-assets',
               help='candidate assets; validate before copying into the source tree')
a = p.parse_args()
a.compiler_bin = a.compiler_bin.resolve()
a.work = a.work.resolve()
a.work.mkdir(parents=True, exist_ok=True)
a.output_dir.mkdir(parents=True, exist_ok=True)
compiler = a.compiler_bin / 'iree-compile'
python = a.compiler_bin / 'python'
version = subprocess.check_output([compiler, '--version'], text=True)
assert 'ce36167c3be514dd165a3ecff2d377cfa8eca0c9' in version, version
model = root / 'NoiseNet/crates/noise-net-iree/model/dpdfnet8.onnx'
assert hashlib.sha256(model.read_bytes()).hexdigest() == '7b3afbb260a08fe9af3d16e3bda992971be1e7e951d1dee7c2d235f5c43f5631'


def run(*args):
    subprocess.run(args, check=True)


def compile_module(mlir, filename, *flags):
    output = a.output_dir / filename
    run(compiler, mlir, *flags, '--iree-vm-bytecode-module-strip-source-map', '-o', output)
    print(hashlib.sha256(output.read_bytes()).hexdigest(), output, flush=True)


# The official metadata supplies the only nonzero values of initial state.
run(python, '-c',
    "import onnx, struct, sys; "
    "m = onnx.load(sys.argv[1]); d = {p.key: p.value for p in m.metadata_props}; "
    "v = [float(x) for k in ('erb_norm_init', 'spec_norm_init') for x in d[k].split(',')]; "
    "assert len(v) == 577; open(sys.argv[2], 'wb').write(struct.pack('<577f', *v))",
    model, a.output_dir / 'dpdfnet8-norm.f32le')
assert (a.output_dir / 'dpdfnet8-norm.f32le').read_bytes() == (model.parent / 'dpdfnet8-norm.f32le').read_bytes()

if a.backend in ('gpu', 'all'):
    if a.glslang is None:
        p.error('--glslang is required for GPU compilation')
    a.glslang = a.glslang.resolve()
    version = subprocess.check_output([a.glslang, '--version'], text=True)
    assert '15.1.0' in version, version
    for shader in ('gru-project-packed', 'gru-recur-packed', 'grouped-matmul', 'temporal-conv'):
        run(a.glslang, '-V', '--target-env', 'vulkan1.1', source / (shader + '.comp'),
            '-o', a.work / (shader + '.spv'))
    run(a.compiler_bin / 'iree-import-onnx', model, '-o', a.work / 'model.mlir')
    run(python, source / 'inject-gru.py', '--input', a.work / 'model.mlir',
        '--output', a.work / 'custom.mlir', '--packed-projection', '--packed-recurrence')
    compile_module(a.work / 'custom.mlir', 'dpdfnet8.vmfb',
                   '--iree-hal-target-backends=vulkan-spirv', '--iree-vulkan-target=rdna2',
                   '--iree-opt-level=O1', f'--iree-hal-executable-object-search-path={a.work}')

if a.backend in ('cpu', 'all'):
    run(python, source / 'split-gru.py', '--input', model, '--output', a.work / 'split-gru.onnx')
    run(a.compiler_bin / 'iree-import-onnx', a.work / 'split-gru.onnx',
        '-o', a.work / 'split-gru.mlir')
    cpu_flags = ('--iree-hal-target-backends=llvm-cpu', '--iree-llvmcpu-target-cpu=x86-64')
    compile_module(a.work / 'split-gru.mlir', 'dpdfnet8-cpu.vmfb', *cpu_flags,
                   '--iree-opt-level=O1')
    compile_module(a.work / 'split-gru.mlir', 'dpdfnet8-cpu-avx2.vmfb', *cpu_flags,
                   '--iree-llvmcpu-target-cpu-features=+avx,+avx2,+fma',
                   '--iree-llvmcpu-enable-ukernels=all', '--iree-opt-level=O3')

#!/usr/bin/env bash
# Native runtime only; model compilation is a separate, pinned offline step.
set -euo pipefail
root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
platform="${1:?linux or windows}"
destination="${2:?directory containing the executable}"
[[ "$platform" == linux || "$platform" == windows ]]
(cd "$root" && sha256sum --check NoiseNet/crates/noise-net-iree/model/SHA256SUMS.txt)
revision=ce36167c3be514dd165a3ecff2d377cfa8eca0c9
cache="$root/target/iree"
source_dir="$cache/source"
# The checkout is bind-mounted from the host into root-owned build containers.
export GIT_CONFIG_COUNT=3
export GIT_CONFIG_KEY_0=safe.directory GIT_CONFIG_VALUE_0="$source_dir"
export GIT_CONFIG_KEY_1=safe.directory GIT_CONFIG_VALUE_1="$source_dir/third_party/flatcc"
export GIT_CONFIG_KEY_2=safe.directory GIT_CONFIG_VALUE_2="$source_dir/third_party/vulkan_headers"
if [[ ! -d "$source_dir/.git" ]]; then
    git clone --filter=blob:none --no-checkout https://github.com/iree-org/iree.git "$source_dir"
fi
if [[ "$(git -C "$source_dir" rev-parse HEAD)" != "$revision" ]]; then
    git -C "$source_dir" fetch origin "$revision"
    git -C "$source_dir" checkout --detach "$revision"
fi
# Refuse unrecorded edits to the pinned dependency.
git -C "$source_dir" diff --exit-code HEAD -- ':!third_party' >/dev/null
git -C "$source_dir" submodule update --init --depth 1 third_party/flatcc third_party/vulkan_headers
for dependency in flatcc vulkan_headers; do
    git -C "$source_dir/third_party/$dependency" diff --exit-code HEAD >/dev/null
done
cmake_root="$cache/tools/cmake-3.31.8-linux-x86_64"
archive="$cache/tools/cmake.tar.gz"
checksum=630615d8e98ac33eba7fbe472626dff5c899c85af3c024585ae109166a6909d0
mkdir -p "$cache/tools"
if [[ ! -f "$archive" ]] || ! echo "$checksum  $archive" | sha256sum --check --status; then
    curl --fail --location --retry 3 https://github.com/Kitware/CMake/releases/download/v3.31.8/cmake-3.31.8-linux-x86_64.tar.gz -o "$archive.download"
    echo "$checksum  $archive.download" | sha256sum --check --status
    mv "$archive.download" "$archive"
fi
if [[ ! -x "$cmake_root/bin/cmake" ]]; then tar -xzf "$archive" -C "$cache/tools"; fi
cmake="$cmake_root/bin/cmake"
options=()
if [[ "$platform" == windows ]]; then
    "$cmake" -S "$root/NoiseNet/crates/noise-net-iree/native" -B "$cache/host-tools" \
        -DIREE_SOURCE_DIR="$source_dir" -DCMAKE_BUILD_TYPE=Release
    "$cmake" --build "$cache/host-tools" --target iree-flatcc-cli -j "${NOISE_HOIHOI_JOBS:-6}"
    options+=(-DIREE_HOST_BIN_DIR="$cache/host-tools/iree/tools")
    gcc_libdir="$(dirname -- "$(x86_64-w64-mingw32-gcc -print-libgcc-file-name)")"
    options+=(-DCMAKE_SYSTEM_NAME=Windows -DCMAKE_SYSTEM_PROCESSOR=AMD64
        -DCMAKE_C_COMPILER=clang -DCMAKE_CXX_COMPILER=clang++
        -DCMAKE_C_COMPILER_TARGET=x86_64-w64-windows-gnu
        -DCMAKE_CXX_COMPILER_TARGET=x86_64-w64-windows-gnu
        -DCMAKE_C_FLAGS=-fms-extensions -DCMAKE_CXX_FLAGS=-fms-extensions
        "-DCMAKE_EXE_LINKER_FLAGS=-L$gcc_libdir" "-DCMAKE_SHARED_LINKER_FLAGS=-L$gcc_libdir"
        -DCMAKE_RC_COMPILER=x86_64-w64-mingw32-windres)
fi
"$cmake" -S "$root/NoiseNet/crates/noise-net-iree/native" -B "$cache/$platform" \
    -DIREE_SOURCE_DIR="$source_dir" -DCMAKE_BUILD_TYPE=Release "${options[@]}"
"$cmake" --build "$cache/$platform" --target noise_iree -j "${NOISE_HOIHOI_JOBS:-6}"
mkdir -p "$destination/iree"
if [[ "$platform" == linux ]]; then
    cp "$cache/$platform/libnoise_iree.so" "$destination/iree/"
else
    cp "$cache/$platform/libnoise_iree.dll" "$destination/iree/noise_iree.dll"
fi
cp "$root/NoiseNet/crates/noise-net-iree/model/PROVENANCE.md" "$destination/iree/"
cp "$root"/packaging/licenses/IREE-* "$destination/iree/"

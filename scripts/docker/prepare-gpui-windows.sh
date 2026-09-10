#!/usr/bin/env bash
# Build-only Windows SDK tools. No SDK executables or DLLs enter the installer.
set -euo pipefail
cache=/work/target/gpui-sdk
archive="$cache/sdk-tools.nupkg"
checksum=b4730a467a8f29145fc0136b2b3f626767e985f6aa6e32f1352953c38d6ff5d2
mkdir -p "$cache/bin"
if [[ ! -f "$archive" ]] || ! echo "$checksum  $archive" | sha256sum --check --status; then
    curl --fail --location --retry 3 \
        https://api.nuget.org/v3-flatcontainer/microsoft.windows.sdk.cpp/10.0.26100.1/microsoft.windows.sdk.cpp.10.0.26100.1.nupkg \
        -o "$archive.download"
    echo "$checksum  $archive.download" | sha256sum --check --status
    mv -- "$archive.download" "$archive"
fi
unzip -j -o -q "$archive" \
    c/bin/10.0.26100.0/x64/fxc.exe \
    c/bin/10.0.26100.0/x64/d3dcompiler_47.dll \
    -d "$cache/bin"

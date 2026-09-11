#!/usr/bin/env bash
set -euo pipefail

repository_root=/work
rust_target="$repository_root/target/windows-docker-rust"
stage="$repository_root/target/windows-docker-stage"
third_party_cache="$repository_root/target/third-party/vb-cable"
vbcable_archive="$third_party_cache/VBCABLE_Driver_Pack45.zip"
vbcable_package="$third_party_cache/package"
vbcable_url=https://download.vb-audio.com/Download_CABLE/VBCABLE_Driver_Pack45.zip
vbcable_sha256=b950e39f01af1d04ea623c8f6d8eb9b6ea5c477c637295fabf20631c85116bfb

cleanup() {
    local status=$?
    if [[ -n "${xvfb_pid:-}" ]]; then kill "$xvfb_pid" 2>/dev/null || true; fi
    if [[ "${HOST_UID:-}" =~ ^[0-9]+$ && "${HOST_GID:-}" =~ ^[0-9]+$ ]]; then
        local generated
        for generated in \
            "$repository_root/out" \
            "$rust_target" \
            "$stage" \
            "$third_party_cache" \
            "$repository_root/target/gpui-sdk"; do
            if [[ -e "$generated" ]]; then
                chown -R "$HOST_UID:$HOST_GID" "$generated"
            fi
        done
    fi
    exit "$status"
}
trap cleanup EXIT

if [[ ! -f "$repository_root/Cargo.lock" ]]; then
    echo "Run this script inside the NoiseHoiHoi Windows build container." >&2
    exit 1
fi

jobs="${NOISE_HOIHOI_JOBS:-}"
if [[ -z "$jobs" ]]; then
    jobs="$(nproc)"
fi
if [[ ! "$jobs" =~ ^[1-9][0-9]*$ ]]; then
    echo "NOISE_HOIHOI_JOBS must be a positive integer." >&2
    exit 1
fi

if [[ ! "${SOURCE_DATE_EPOCH:-}" =~ ^[0-9]+$ ]]; then
    echo "SOURCE_DATE_EPOCH must be a non-negative integer." >&2
    exit 1
fi
export SOURCE_DATE_EPOCH
export LC_ALL=C

package_id="$(cargo pkgid --locked -p noise-hoihoi-app)"
product_version="${package_id##*#}"
if [[ ! "$product_version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
    echo "The application version must use major.minor.patch format." >&2
    exit 1
fi
release_label="v${product_version%.*}"
file_version="$product_version.0"
app_filename="NoiseHoiHoi-$release_label.exe"
smoke_filename="NoiseHoiHoi-$release_label-audio-smoke.exe"
installer_filename="NoiseHoiHoi-$release_label-setup.exe"
app_binary="$rust_target/x86_64-pc-windows-gnu/release/noise-hoihoi-app.exe"
smoke_binary="$rust_target/x86_64-pc-windows-gnu/release/audio-smoke.exe"
installer="$repository_root/out/$installer_filename"
smoke_output="$repository_root/out/$smoke_filename"
app_output="$repository_root/out/$app_filename"

mkdir -p "$repository_root/out"
rm -f -- "$installer" "$smoke_output" "$app_output"

shellcheck \
    "$repository_root/scripts/docker/build-windows.sh" \
    "$repository_root/scripts/docker/build-windows-inner.sh" \
    "$repository_root/scripts/docker/collect-rust-licenses.sh" \
    "$repository_root/scripts/docker/prepare-gpui-windows.sh" \
    "$repository_root/scripts/docker/fxc-wine.sh"

echo "[1/6] Fetching and validating the pinned VB-CABLE package"
mkdir -p "$third_party_cache"
if [[ ! -f "$vbcable_archive" ]] || \
    ! echo "$vbcable_sha256  $vbcable_archive" | sha256sum --check --status; then
    temporary_archive="$vbcable_archive.download"
    rm -f -- "$temporary_archive"
    curl --fail --location --silent --show-error "$vbcable_url" -o "$temporary_archive"
    echo "$vbcable_sha256  $temporary_archive" | sha256sum --check --status
    mv -- "$temporary_archive" "$vbcable_archive"
fi

rm -rf -- "$vbcable_package"
mkdir -p "$vbcable_package"
unzip -q "$vbcable_archive" -d "$vbcable_package"

for required_file in \
    VBCABLE_Setup_x64.exe \
    vbMmeCable64_win10.inf \
    vbaudio_cable64_win10.cat \
    vbaudio_cable64_win10.sys \
    readme.txt; do
    if [[ ! -f "$vbcable_package/$required_file" ]]; then
        echo "The pinned VB-CABLE package is missing $required_file." >&2
        exit 1
    fi
done

grep -Fq 'HardwareId="VBAudioVACWDM"' "$vbcable_package/vbMmeCable64_win10.inf"
grep -Fq 'ServiceName="VBAudioVACMME"' "$vbcable_package/vbMmeCable64_win10.inf"
grep -Fq 'VBCABLE.INPUTNAME1  = "CABLE Output"' "$vbcable_package/vbMmeCable64_win10.inf"
grep -Fq 'VBCABLE.OUTPUTNAME1 = "CABLE Input"' "$vbcable_package/vbMmeCable64_win10.inf"
catalog_certificates="$(
    openssl pkcs7 \
        -inform DER \
        -in "$vbcable_package/vbaudio_cable64_win10.cat" \
        -print_certs \
        -noout
)"
if ! grep -Fq 'Microsoft Windows Hardware Compatibility Publisher' <<<"$catalog_certificates"; then
    echo "The VB-CABLE catalog does not contain the expected Microsoft signer certificate." >&2
    exit 1
fi

export DISPLAY=:98
Xvfb "$DISPLAY" -screen 0 1280x1024x24 -nolisten tcp -ac &
xvfb_pid=$!
bash "$repository_root/scripts/docker/prepare-gpui-windows.sh"
export GPUI_FXC_PATH="$repository_root/scripts/docker/fxc-wine.sh"

echo "[2/6] Checking the Windows application and smoke test"
CARGO_TARGET_DIR="$rust_target" cargo clippy \
    --locked \
    --release \
    --jobs "$jobs" \
    --target x86_64-pc-windows-gnu \
    -p noise-hoihoi-app \
    -p audio-smoke \
    -- \
    -D warnings

echo "[3/6] Building optimized Windows binaries"
CARGO_TARGET_DIR="$rust_target" cargo build \
    --locked \
    --release \
    --jobs "$jobs" \
    --target x86_64-pc-windows-gnu \
    -p noise-hoihoi-app \
    -p audio-smoke

app_type="$(file "$app_binary")"
smoke_type="$(file "$smoke_binary")"
if [[ "$app_type" != *"PE32+ executable (GUI) x86-64"* ]]; then
    echo "The application is not a Windows x64 GUI executable: $app_type" >&2
    exit 1
fi
if [[ "$smoke_type" != *"PE32+ executable (console) x86-64"* ]]; then
    echo "The smoke test is not a Windows x64 console executable: $smoke_type" >&2
    exit 1
fi

echo "[4/6] Assembling the Windows package"
rm -rf -- "$stage"
mkdir -p "$stage/licenses" "$stage/third-party/vb-cable"
cp -- "$app_binary" "$stage/$app_filename"
bash "$repository_root/scripts/build-iree.sh" windows "$stage"
cp -a -- "$vbcable_package/." "$stage/third-party/vb-cable/"
cp -- "$repository_root/LICENSE" "$stage/licenses/LICENSE-MIT"
cp -- "$repository_root/THIRD_PARTY_NOTICES.md" "$stage/licenses/"
cp -- "$repository_root/packaging/licenses/DPDFNet-LICENSE-APACHE-2.0.txt" "$stage/licenses/"
cp -- "$repository_root/packaging/windows/VB-CABLE-NOTICE.txt" "$stage/licenses/"
cp -- /usr/share/doc/nsis/copyright "$stage/licenses/NSIS-copyright"
cp -- /usr/share/common-licenses/Apache-2.0 "$stage/licenses/Apache-2.0.txt"
cp -- /usr/share/common-licenses/MPL-2.0 "$stage/licenses/MPL-2.0.txt"
cp -- "$repository_root/packaging/licenses/BSL-1.0.txt" "$stage/licenses/"
cp -- "$smoke_binary" "$smoke_output"
cp -- "$app_binary" "$app_output"

bash "$repository_root/scripts/docker/collect-rust-licenses.sh" \
    "$repository_root" "$stage" x86_64-pc-windows-gnu

cat >"$stage/BUILD-INFO.txt" <<BUILD_INFO
Product: NoiseHoiHoi $product_version
Target: Windows x64 (x86_64-pc-windows-gnu)
Rust: $(rustc --version)
VB-CABLE archive SHA-256: $vbcable_sha256
Inference: IREE 3.12.0rc20260910 (CPU and Vulkan GPU)
Native library manifest: iree/PROVENANCE.md
Source date epoch: $SOURCE_DATE_EPOCH
BUILD_INFO

find "$stage" -exec touch -h --date="@$SOURCE_DATE_EPOCH" {} +
(
    cd "$stage"
    while IFS= read -r -d '' staged_file; do
        staged_file="${staged_file#./}"
        printf '%s *%s\n' "$(sha256sum "$staged_file" | cut -d' ' -f1)" "$staged_file"
    done < <(find . -type f ! -name SHA256SUMS.txt -print0 | sort -z)
) >"$stage/SHA256SUMS.txt"
touch --date="@$SOURCE_DATE_EPOCH" "$stage/SHA256SUMS.txt" "$smoke_output" "$app_output"

if find "$stage" -type f \( \
    -iname '*.key' -o \
    -iname '*.p12' -o \
    -iname '*.pem' -o \
    -iname '*.pfx' -o \
    -iname '*.pvk' \
    \) -print -quit | grep -q .; then
    echo "Private signing material must not be included in the installer." >&2
    exit 1
fi

echo "[5/6] Building $installer"
makensis \
    -V2 \
    -DAPP_FILENAME="$app_filename" \
    -DFILE_VERSION="$file_version" \
    -DOUTPUT_FILE="$installer" \
    -DPRODUCT_VERSION="$product_version" \
    -DRELEASE_LABEL="$release_label" \
    -DSTAGE_DIR="$stage" \
    "$repository_root/packaging/windows/NoiseHoiHoi.nsi"

echo "[6/6] Verifying Windows artifacts"
installer_type="$(file "$installer")"
if [[ "$installer_type" != *"PE32 executable"* || "$installer_type" != *"Nullsoft Installer"* ]]; then
    echo "The installer is not a valid NSIS Windows executable: $installer_type" >&2
    exit 1
fi
7z t "$installer" >/dev/null
installer_listing="$(7z l "$installer")"
for required_payload in \
    "$app_filename" \
    BUILD-INFO.txt \
    iree/noise_iree.dll \
    iree/IREE-LICENSE.txt \
    iree/IREE-flatcc-LICENSE.txt \
    iree/IREE-Vulkan-Headers-LICENSE.txt \
    licenses/Apache-2.0.txt \
    licenses/BSL-1.0.txt \
    licenses/DPDFNet-LICENSE-APACHE-2.0.txt \
    licenses/RUST-DEPENDENCIES.txt \
    licenses/VB-CABLE-NOTICE.txt \
    third-party/vb-cable/VBCABLE_Setup_x64.exe; do
    if ! grep -Fq "$required_payload" <<<"$installer_listing"; then
        echo "The installer is missing $required_payload." >&2
        exit 1
    fi
done

sha256sum "$installer" "$smoke_output" "$app_output"
python3 "$repository_root/scripts/prune-builds.py" windows "$release_label" "$repository_root/out"
printf '%s\n' "$installer" "$smoke_output" "$app_output"

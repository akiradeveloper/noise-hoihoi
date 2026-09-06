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
    if [[ "${HOST_UID:-}" =~ ^[0-9]+$ && "${HOST_GID:-}" =~ ^[0-9]+$ ]]; then
        local generated
        for generated in \
            "$repository_root/out" \
            "$rust_target" \
            "$stage" \
            "$third_party_cache"; do
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

mkdir -p "$repository_root/out"
rm -f -- "$installer" "$smoke_output"

shellcheck \
    "$repository_root/scripts/docker/build-windows.sh" \
    "$repository_root/scripts/docker/build-windows-inner.sh"

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
cp -a -- "$vbcable_package/." "$stage/third-party/vb-cable/"
cp -- "$repository_root/LICENSE" "$stage/licenses/LICENSE-MIT"
cp -- "$repository_root/THIRD_PARTY_NOTICES.md" "$stage/licenses/"
cp -- "$repository_root/packaging/licenses/DeepFilterNet-LICENSE-MIT.txt" "$stage/licenses/"
cp -- "$repository_root/packaging/windows/VB-CABLE-NOTICE.txt" "$stage/licenses/"
cp -- /usr/share/doc/nsis/copyright "$stage/licenses/NSIS-copyright"
cp -- /usr/share/common-licenses/Apache-2.0 "$stage/licenses/Apache-2.0.txt"
cp -- "$repository_root/packaging/licenses/BSL-1.0.txt" "$stage/licenses/"
cp -- "$smoke_binary" "$smoke_output"

rust_license_root="$stage/licenses/rust"
rust_dependency_list="$stage/licenses/RUST-DEPENDENCIES.txt"
mkdir -p "$rust_license_root"
: >"$rust_dependency_list"
while read -r package_name package_version _; do
    package_version="${package_version#v}"
    case "$package_name" in
        noise-hoihoi-app | noise-hoihoi-engine | noise-net)
            continue
            ;;
    esac

    package_directory="$(
        find "$CARGO_HOME/registry/src" \
            -mindepth 2 \
            -maxdepth 2 \
            -type d \
            -name "$package_name-$package_version" \
            -print \
            -quit
    )"
    if [[ -z "$package_directory" ]]; then
        echo "Could not locate sources for Rust dependency $package_name $package_version." >&2
        exit 1
    fi

    declared_license="$(
        sed -n 's/^license = "\(.*\)"$/\1/p' "$package_directory/Cargo.toml" | head -n 1
    )"
    printf '%s %s\t%s\n' \
        "$package_name" \
        "$package_version" \
        "${declared_license:-see bundled license files}" \
        >>"$rust_dependency_list"

    package_license_root="$rust_license_root/$package_name-$package_version"
    mkdir -p "$package_license_root"
    license_count=0
    while IFS= read -r -d '' license_file; do
        relative_license="${license_file#"$package_directory"/}"
        mkdir -p "$package_license_root/$(dirname -- "$relative_license")"
        cp -- "$license_file" "$package_license_root/$relative_license"
        license_count=$((license_count + 1))
    done < <(
        find "$package_directory" \
            -maxdepth 3 \
            -type f \
            \( \
                -iname 'COPYING*' -o \
                -iname 'COPYRIGHT*' -o \
                -iname 'FONTLOG*' -o \
                -ipath '*/fonts/*.txt' -o \
                -iname 'LICENSE*' -o \
                -iname 'NOTICE*' -o \
                -iname 'OFL*' -o \
                -iname 'UFL*' -o \
                -iname 'UNLICENSE*' \
            \) \
            -print0 \
            | sort -z
    )
    if ((license_count == 0)); then
        case "$package_name:$declared_license" in
            pulp-wasm-simd-flag:MIT)
                cp -- \
                    "$repository_root/packaging/licenses/pulp-LICENSE-MIT.txt" \
                    "$package_license_root/LICENSE-MIT"
                ;;
            realfft:MIT)
                cp -- \
                    "$repository_root/packaging/licenses/realfft-LICENSE-MIT.txt" \
                    "$package_license_root/LICENSE-MIT"
                ;;
            *:*Apache-2.0* | *:BSL-1.0) ;;
            *)
                echo "Rust dependency $package_name $package_version has no bundled license file or supported common-license fallback." >&2
                exit 1
                ;;
        esac
        cp -- "$package_directory/Cargo.toml" "$package_license_root/Cargo.toml"
        for attribution_file in Cargo.toml.orig README.md; do
            if [[ -f "$package_directory/$attribution_file" ]]; then
                cp -- "$package_directory/$attribution_file" \
                    "$package_license_root/$attribution_file"
            fi
        done
    fi
done < <(
    cargo tree \
        --locked \
        --target x86_64-pc-windows-gnu \
        --edges normal,build \
        --prefix none \
        --format '{p}' \
        -p noise-hoihoi-app \
        | sed 's/ (\*)$//' \
        | sort -u
)

cat >"$stage/BUILD-INFO.txt" <<BUILD_INFO
Product: NoiseHoiHoi $product_version
Target: Windows x64 (x86_64-pc-windows-gnu)
Rust: $(rustc --version)
VB-CABLE archive SHA-256: $vbcable_sha256
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
touch --date="@$SOURCE_DATE_EPOCH" "$stage/SHA256SUMS.txt" "$smoke_output"

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
    licenses/Apache-2.0.txt \
    licenses/BSL-1.0.txt \
    licenses/DeepFilterNet-LICENSE-MIT.txt \
    licenses/RUST-DEPENDENCIES.txt \
    licenses/VB-CABLE-NOTICE.txt \
    third-party/vb-cable/VBCABLE_Setup_x64.exe; do
    if ! grep -Fq "$required_payload" <<<"$installer_listing"; then
        echo "The installer is missing $required_payload." >&2
        exit 1
    fi
done

sha256sum "$installer" "$smoke_output"
printf '%s\n' "$installer" "$smoke_output"

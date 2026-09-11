#!/usr/bin/env bash
set -euo pipefail
repository_root=/work
export CARGO_TARGET_DIR="$repository_root/target/linux"
export CARGO_BUILD_JOBS="${NOISE_HOIHOI_JOBS:-6}"
export LC_ALL=C
stage="$repository_root/out/linux/NoiseHoiHoi.AppDir"
cache="$repository_root/out/linux/tools"
cleanup() {
    local status=$?
    if [[ "${HOST_UID:-}" =~ ^[0-9]+$ && "${HOST_GID:-}" =~ ^[0-9]+$ ]]; then
        chown -R "$HOST_UID:$HOST_GID" "$repository_root/out/linux" "$CARGO_TARGET_DIR" "$cache"
        for artifact in "$repository_root"/out/NoiseHoiHoi-*-x86_64.AppImage*; do
            [[ ! -e "$artifact" ]] || chown "$HOST_UID:$HOST_GID" "$artifact"
        done
    fi
    exit "$status"
}
trap cleanup EXIT
[[ "$CARGO_BUILD_JOBS" =~ ^[1-9][0-9]*$ ]]
[[ "${SOURCE_DATE_EPOCH:-}" =~ ^[0-9]+$ ]]
[[ "$(uname -m)" == x86_64 ]]
mkdir -p "$cache" "$repository_root/out/linux"
shellcheck "$repository_root"/scripts/docker/*linux*.sh "$repository_root/scripts/docker/collect-rust-licenses.sh" "$repository_root/packaging/linux/AppRun"
python3 "$repository_root/scripts/check-architecture.py"
cargo fmt --all --check
cargo clippy --locked --workspace --all-targets -- -D warnings
bash "$repository_root/scripts/build-iree.sh" linux "$repository_root/out/linux/iree-test-runtime"
export NOISE_IREE_LIBRARY="$repository_root/out/linux/iree-test-runtime/iree/libnoise_iree.so"
cargo build --release --locked -p noise-hoihoi-app
cargo test --release --locked -p noise-net-iree --lib
cargo test --release --locked -p noise-net-iree --test streaming cpu_ -- --ignored
cargo test --release --locked -p noise-hoihoi-platform --test inference selected_cpu -- --ignored
cargo test --release --locked -p noise-hoihoi-engine -p noise-hoihoi-session -p noise-hoihoi-platform -p noise-hoihoi-app
bash "$repository_root/scripts/docker/test-linux-audio.sh" pulseaudio
bash "$repository_root/scripts/docker/test-linux-audio.sh" pipewire
RUSTDOCFLAGS="-D warnings" cargo doc --locked -p noise-hoihoi-engine -p noise-hoihoi-session -p noise-hoihoi-platform -p noise-hoihoi-app --no-deps

fetch_tool() {
    local name="$1" version="$2" checksum="$3" repository="$4"
    local archive="$cache/$name-$version.AppImage"
    if [[ ! -f "$archive" ]] || ! echo "$checksum  $archive" | sha256sum --check --status; then
        curl --fail --location --retry 3 "https://github.com/$repository/releases/download/$version/$name-x86_64.AppImage" -o "$archive.download"
        echo "$checksum  $archive.download" | sha256sum --check --status
        mv -- "$archive.download" "$archive"
    fi
    chmod +x "$archive"
    mkdir -p "$cache/$name"
    (cd "$cache/$name" && "$archive" --appimage-extract >/dev/null)
}
fetch_tool linuxdeploy 1-alpha-20251107-1 c20cd71e3a4e3b80c3483cef793cda3f4e990aca14014d23c544ca3ce1270b4d linuxdeploy/linuxdeploy
fetch_tool appimagetool 1.9.1 ed4ce84f0d9caff66f50bcca6ff6f35aae54ce8135408b3fa33abfc3cb384eb0 AppImage/appimagetool
rm -rf -- "$stage"
mkdir -p "$stage/usr/bin" "$stage/licenses"
cp "$CARGO_TARGET_DIR/release/noise-hoihoi-app" "$stage/usr/bin/"
mkdir -p "$stage/usr/bin/iree"
cp -a "$repository_root/out/linux/iree-test-runtime/iree/." "$stage/usr/bin/iree/"
# GL/EGL/Vulkan and vendor drivers remain supplied by the host. Explicitly
# bundle the dynamically loaded window-system libraries used by GPUI.
"$cache/linuxdeploy/squashfs-root/AppRun" --appdir "$stage" \
    --executable "$stage/usr/bin/noise-hoihoi-app" \
    --desktop-file "$repository_root/packaging/linux/NoiseHoiHoi.desktop" \
    --icon-file "$repository_root/assets/NoiseHoiHoi.svg" \
    --library /usr/lib/x86_64-linux-gnu/libxkbcommon.so.0 \
    --library /usr/lib/x86_64-linux-gnu/libxkbcommon-x11.so.0 \
    --library /usr/lib/x86_64-linux-gnu/libwayland-client.so.0 \
    --library /usr/lib/x86_64-linux-gnu/libwayland-cursor.so.0 \
    --library /usr/lib/x86_64-linux-gnu/libwayland-egl.so.1
rm -f -- "$stage/AppRun"
cp "$repository_root/packaging/linux/AppRun" "$stage/AppRun"
chmod +x "$stage/AppRun"
desktop-file-validate "$stage/NoiseHoiHoi.desktop"
cp "$repository_root/LICENSE" "$repository_root/THIRD_PARTY_NOTICES.md" "$stage/licenses/"
cp "$repository_root"/packaging/licenses/* "$stage/licenses/"
cp /usr/share/common-licenses/{Apache-2.0,MPL-2.0,LGPL-2.1,LGPL-3,GPL-2,GPL-3} "$stage/licenses/"
cp "$repository_root/packaging/linux/README.md" "$stage/README.md"
bash "$repository_root/scripts/docker/collect-rust-licenses.sh" "$repository_root" "$stage" x86_64-unknown-linux-gnu
# Keep the distribution's complete copyright notices and exact package/source
# versions for the dynamically linked libraries shipped in this AppImage.
mkdir -p "$stage/licenses/system"
for library in "$stage"/usr/lib/*.so*; do
    basename="$(basename -- "$library")"
    package="$(dpkg-query -S "*/$basename" | head -n 1 | cut -d ' ' -f 1)"
    package="${package%:}"
    package_name="${package%%:*}"
    cp -L "/usr/share/doc/$package_name/copyright" "$stage/licenses/system/$package_name-copyright"
    dpkg-query -W -f='${binary:Package}\t${Version}\t${source:Package}\t${source:Version}\n' "$package" >> "$stage/licenses/system/PACKAGES.txt"
done
sort -u -o "$stage/licenses/system/PACKAGES.txt" "$stage/licenses/system/PACKAGES.txt"
package_id="$(cargo pkgid --locked -p noise-hoihoi-app)"
version="${package_id##*#}"
artifact="$repository_root/out/NoiseHoiHoi-v${version%.*}-x86_64.AppImage"
export ARCH=x86_64
# Reuse the runtime from the checksum-pinned tool instead of downloading a
# changing runtime during packaging.
runtime_archive="$cache/appimagetool-1.9.1.AppImage"
head -c "$("$runtime_archive" --appimage-offset)" "$runtime_archive" > "$cache/runtime-x86_64"
"$cache/appimagetool/squashfs-root/AppRun" --runtime-file "$cache/runtime-x86_64" --no-appstream "$stage" "$artifact.building"
mv -f -- "$artifact.building" "$artifact"
(cd "$repository_root/out" && sha256sum "$(basename -- "$artifact")" > "$(basename -- "$artifact").sha256")
python3 "$repository_root/scripts/prune-builds.py" linux "v${version%.*}" "$repository_root/out"
echo "Built $artifact"

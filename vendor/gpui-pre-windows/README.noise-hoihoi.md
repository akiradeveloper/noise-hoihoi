# GPUI Windows cross-compilation patch

Source: gpui-pre-windows 0.3.4, Apache-2.0, from crates.io.
Upstream: https://github.com/zed-industries/zed (revision recorded in Cargo.toml).

The upstream build script uses the host's `cfg(target_os = "windows")` to
compile release shaders. Linux-to-Windows builds therefore lack
`OUT_DIR/shaders_bytes.rs`. This copy checks `CARGO_CFG_TARGET_OS` and allows
`GPUI_FXC_PATH` to name a Linux wrapper for the SDK shader compiler. Registry
lookup remains Windows-only. The renderer source is unchanged.

The Docker build runs Microsoft Windows SDK 10.0.26100.1's x64 fxc.exe under
Wine and embeds optimized shader bytecode. The SDK archive is SHA-256 pinned;
its tools remain in the build cache and are not redistributed. This avoids
GPUI's debug-only shader path, which reads HLSL files from the source checkout
at runtime. Remove this patch once upstream supports cross-compilation.

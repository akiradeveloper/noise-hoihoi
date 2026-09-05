# NoiseHoiHoi

NoiseHoiHoi is a desktop audio application for streamers. Version 0.2 targets
Windows 11 x64 and forwards a selected physical microphone through VB-CABLE
without noise reduction. NoiseHoiHoi writes to `CABLE Input (VB-Audio Virtual
Cable)`, and recording applications read from `CABLE Output (VB-Audio Virtual
Cable)`. The route exists only while the GUI is running; there is no tray
process or user-mode service.

Use the `Signal monitor...` button at the bottom of the main window to open a
separate live view of the processor input, output, and their difference.
Because v0.2 still uses bit-exact pass-through, the difference is expected to
remain silent.

## Components

- `NoiseHoiHoi`: the Rust GUI and real-time audio engine.
- `NoiseNet`: the noise-reduction model implementation, introduced in v0.3.
- `packaging`: OS-specific installer definitions and build inputs.
- `tools`: end-to-end validation utilities.

VB-CABLE owns the signed Windows virtual-audio driver. NoiseHoiHoi only captures
the selected microphone, processes it, and renders the resulting PCM stream to
`CABLE Input (VB-Audio Virtual Cable)`. OBS, Discord, and games record the
paired `CABLE Output` endpoint.

## Development

Platform-independent checks can run on any Rust host:

```sh
just check
```

Build the optimized Windows application and installer locally with Docker:

```sh
just build-windows
```

The result is `out/NoiseHoiHoi-v0.2-setup.exe`. The build downloads the official
VB-CABLE package, verifies its pinned SHA-256, expected driver identity, and
catalog signer certificate, then embeds the unmodified package. The setup
silently installs VB-CABLE when it is absent. A Windows restart is required
after first install. It also emits `out/NoiseHoiHoi-v0.2-audio-smoke.exe` for
validating the VB-CABLE bridge on Windows without installing a Rust toolchain.
No Git push or hosted CI artifact is involved.

The NoiseHoiHoi executable and setup executable still need the publisher's
normal Authenticode signature before public distribution. That is independent
of driver signing; private signing material must never enter the repository or
Docker image.

VB-CABLE is donationware by VB-Audio Software. Its attribution and distribution
terms are shown by the installer and included in the installed licenses.

NoiseHoiHoi targets Windows and Linux. macOS is not a supported target.

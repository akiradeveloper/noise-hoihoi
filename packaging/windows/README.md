# Windows v0.4 installer

Run the release build from a Linux host with Docker:

```sh
just build-windows
```

It builds the Rust application with the optimized release profile and emits:

```text
out/NoiseHoiHoi-v0.4-setup.exe
out/NoiseHoiHoi-v0.4-audio-smoke.exe
```

The build fetches the official base VB-CABLE Package 45 from VB-Audio, pins it
to SHA-256
`b950e39f01af1d04ea623c8f6d8eb9b6ea5c477c637295fabf20631c85116bfb`,
and checks that its Windows 10/11 catalog contains the Microsoft Windows
Hardware Compatibility Publisher signer certificate. The archive digest,
expected INF identity, endpoint names, and signer identity are checked before
the complete, unmodified package is embedded in the NoiseHoiHoi installer.
The application and smoke-test binaries are linted for the Windows target, and
the finished NSIS archive and required payload entries are verified.
License files for every Rust crate linked into the Windows application are
collected from the locked target dependency graph. Crates that inherit a
repository-level license retain their package metadata and attribution beside
the selected common Apache-2.0, MPL-2.0, or Boost license text. The build fails if a
crate source has neither bundled license files nor a supported common-license
fallback.

When the `VBAudioVACMME` driver service is absent, setup runs the vendor's x64
installer with `-i -h`, waits for the setup process to finish, and marks Windows
for a required restart. It does not require the audio service to appear before
that restart. An existing VB-CABLE installation is preserved. Removing
NoiseHoiHoi also leaves VB-CABLE installed because it may be shared by other
applications.

Upgrade and uninstall both detect a running NoiseHoiHoi window, ask permission
to close it, and wait for shutdown before changing installed files. Start-menu
shortcuts are installed for all users because the application is installed
machine-wide in its fixed Program Files directory. The uninstaller verifies an
installation ownership marker before recursively removing that directory.

NoiseHoiHoi is built against the following route:

```text
physical microphone -> NoiseHoiHoi -> CABLE Input -> CABLE Output -> recorder
```

VB-CABLE is donationware by VB-Audio Software. The installer shows the required
origin and donation notice and installs it under `licenses/VB-CABLE-NOTICE.txt`.
Review the current distribution terms before publishing a release:
https://vb-audio.com/Services/licensing.htm

The application, uninstaller, and setup executable must additionally be signed
with the NoiseHoiHoi publisher's Authenticode identity and timestamped before
public distribution. Those publisher credentials are not stored in the
repository or Docker image.

Build timestamps are normalized from `SOURCE_DATE_EPOCH`. By default, the
outer build script uses the current Git commit timestamp; callers can override
the environment variable with another non-negative Unix timestamp.

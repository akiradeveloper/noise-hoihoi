# Third-party notices

## DeepFilterNet3

NoiseNet embeds model weights converted from the official DeepFilterNet3 ONNX
archive and contains generated graph code based on that model. DeepFilterNet is
Copyright (c) 2021 Hendrik Schröter and is dual-licensed under MIT or
Apache-2.0; NoiseHoiHoi uses the MIT option.

Origin: https://github.com/Rikorose/DeepFilterNet

The selected license is installed as
`licenses/DeepFilterNet-LICENSE-MIT.txt`.

## NoiseNet test speech

NoiseNet's source test data includes a resampled excerpt of Ian Skillen's
public-domain LibriVox reading, obtained from Voice Zero. Voice Zero dedicates
files in its `voices` directory to the public domain under CC0 1.0. This test
fixture is not installed with the application. Its exact provenance
and hashes are recorded in `NoiseNet/crates/noise-net/testdata/README.md`.

Origin: https://github.com/OwenTyme/voice-zero

## Rust dependencies

The application includes open-source Rust dependencies. The exact
crate versions and declared license expressions are generated from the locked
target-platform dependency graph into `licenses/RUST-DEPENDENCIES.txt`. Each crate's
distributed license, copyright, copying, and notice files are installed below
`licenses/rust/<crate>-<version>/`.

Some crates inherit a repository-level license and do not publish that file in
their crate archive. For those crates, package metadata and attribution are
preserved in the crate directory. NoiseHoiHoi elects Apache-2.0 where a
dependency offers that choice; common Apache-2.0 and Boost Software License
texts are installed at the root of `licenses/`. MIT-only and other dependencies
retain their package-specific license files.

## GPUI interface

NoiseHoiHoi v0.7 uses GPUI and gpui-component through GPUI Kit. These projects
are Apache-2.0 licensed:

- https://github.com/zed-industries/zed
- https://github.com/longbridge/gpui-kit

The Windows platform crate carries a build-script change for Linux-hosted
shader compilation; its renderer source is unchanged. The SDK compiler and
Wine used by the build are not shipped with NoiseHoiHoi.

Additional license supplements for rust-i18n, taffy, seahash, harfrust, xim,
and hexf-parse are
recorded in `packaging/licenses/README.md` in the source repository. In
particular, seahash's standard MIT text and author attribution are reconstructed
from its declared license and package authors because upstream has no separate
license file. These supplements accompany the corresponding package metadata
in the installed `licenses/rust` directories.

## VB-CABLE

The Windows installer embeds the unmodified base VB-CABLE package from
VB-Audio Software. VB-CABLE supplies the signed virtual-audio driver used to
transport NoiseHoiHoi's output to recording applications.

Origin: https://www.vb-cable.com/

VB-CABLE is donationware. All participations are welcome. Its package readme
and license are included with the package under `third-party/vb-cable`, and the
NoiseHoiHoi installer includes the distribution notice at
`licenses/VB-CABLE-NOTICE.txt`.

Distribution and professional-use terms:
https://vb-audio.com/Services/licensing.htm

## NSIS

The Windows Docker image uses Nullsoft Scriptable Install System (NSIS) to
assemble the Windows installer. The generated package includes the NSIS
copyright and license notice under `licenses/NSIS-copyright`.

## Linux shared libraries

The Linux AppImage includes the PulseAudio client library and window-system
libraries collected by linuxdeploy. Complete Debian copyright notices and
package/source versions are installed under `licenses/system/`. Source retrieval
and library replacement instructions are in the bundled Linux README.

Packaging tools: https://github.com/linuxdeploy/linuxdeploy and
https://github.com/AppImage/appimagetool (checksum-pinned by the build script).

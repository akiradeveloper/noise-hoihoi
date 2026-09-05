# Third-party notices

## Rust dependencies

The Windows application includes open-source Rust dependencies. The exact
crate versions and declared license expressions are generated from the locked
Windows dependency graph into `licenses/RUST-DEPENDENCIES.txt`. Each crate's
distributed license, copyright, copying, and notice files are installed below
`licenses/rust/<crate>-<version>/`.

Some crates inherit a repository-level license and do not publish that file in
their crate archive. For those crates, package metadata and attribution are
preserved in the crate directory. NoiseHoiHoi elects Apache-2.0 where a
dependency offers that choice; common Apache-2.0 and Boost Software License
texts are installed at the root of `licenses/`. MIT-only and other dependencies
retain their package-specific license files.

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

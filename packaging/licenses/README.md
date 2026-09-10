# License supplements for crate archives

Some upstream crate archives omit license files. The packaging script keeps
these texts together with each crate's package metadata and attribution.

Added for the v0.7 GPUI dependency graph:

- `harfrust-LICENSE-MIT.txt`: upstream LICENSE at harfrust 0.5.2's published
  revision, https://github.com/harfbuzz/harfrust/blob/efdae3142ab76a2f1524d72cff9e3dfdc5afd7ca/LICENSE.
- `xim-LICENSE-MIT.txt`: identical upstream LICENSE texts at xim-ctext 0.3.0's
  revision https://github.com/Riey/xim-rs/blob/65f77e44524ba487285a63aac4080faf2c05b4ed/LICENSE
  and xim-parser 0.2.2's revision https://github.com/Riey/xim-rs/blob/7300de35a111de2df1a694bf70454532a8419685/LICENSE.
- `hexf-parse-LICENSE-CC0.txt`: standard CC0 1.0 text from Debian 12's
  `/usr/share/common-licenses/CC0-1.0`. hexf-parse 0.2.1 declares CC0-1.0 in its
  package metadata; that metadata and README are bundled alongside this text.
- `rust-i18n-LICENSE-MIT.txt`: copied from the published rust-i18n 4.2.2
  crate's LICENSE; shared with rust-i18n-macro and rust-i18n-support from
  https://github.com/longbridge/rust-i18n.
- `taffy-LICENSE-MIT.txt`: upstream LICENSE at the published crate's revision,
  https://github.com/DioxusLabs/taffy/blob/45a56299d366ddb383e593a1f0372158d00e8530/LICENSE.
- `seahash-LICENSE-MIT.txt`: standard MIT text with author attribution from
  seahash 4.1.0's Cargo.toml. Neither the crate archive nor the upstream tree at
  revision 94b632aeac099031c373599313d5b5f0acbbaec0 includes a separate license
  file; Cargo.toml declares MIT. This is a reconstructed supplement, not an
  upstream license file. Source: https://gitlab.redox-os.org/redox-os/seahash.

# Song packs: the zip library

A working note, not legal advice.

Song packs (`.sppack`, `crates/sp-app/src/pack.rs`) are zip files. SoftPresenter writes and
reads them with the [`zip`](https://crates.io/crates/zip) crate, built with deflate only
(`default-features = false, features = ["deflate-flate2"]`), compressing with
[`flate2`](https://crates.io/crates/flate2)'s pure-Rust backend (`miniz_oxide`). No other
compression format and no native library comes with it. Song bundles (`.proBundle`,
`crates/sp-app/src/bundle.rs`) are read with the same crate, as untrusted input in the same
way (see [song-documents.md](song-documents.md)).

| Crate | Licence |
|---|---|
| zip 9.8 | MIT |
| typed-path 0.13 | MIT OR Apache-2.0 |
| flate2 1.1 | MIT OR Apache-2.0 |
| miniz_oxide | MIT OR Zlib OR Apache-2.0 |
| crc32fast | MIT OR Apache-2.0 |
| indexmap, equivalent, hashbrown | MIT OR Apache-2.0 |
| foldhash | Zlib |
| memchr | Unlicense OR MIT |
| nix 0.32 | MIT |
| getrandom | MIT OR Apache-2.0 |
| zeroize, zeroize_derive | MIT OR Apache-2.0 |
| constant_time_eq | CC0-1.0 OR MIT-0 OR Apache-2.0 |
| adler2, simd-adler32 | 0BSD OR MIT OR Apache-2.0; MIT |
| fs4 0.13 (the free space on a disk, before a pack is opened) | MIT OR Apache-2.0 |

All can be combined with the GNU GPL version 3, as NOTICE.md's "Other libraries" says;
`cargo metadata` lists each with its licence.

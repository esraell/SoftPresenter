# The control socket on Windows

A working note, not legal advice.

`sp` reaches the running app through a Unix socket: a file, reached only from the same
computer (`crates/sp-app/src/control.rs`). On a Mac and on Linux the standard library has
these. On Windows it doesn't yet, so there, and only there, SoftPresenter uses the
[`uds_windows`](https://crates.io/crates/uds_windows) crate, which calls Windows' own socket
functions.

| Crate | Licence |
|---|---|
| uds_windows 1.2 | MIT |
| memoffset 0.9 | MIT |
| tempfile 3 | MIT OR Apache-2.0 |
| fastrand 2 | Apache-2.0 OR MIT |

The other crates it needs (`windows-sys`, `getrandom`, `once_cell`, `rustix`, `autocfg`) were
already part of the app. All can be combined with the GNU GPL version 3, as NOTICE.md's "Other
libraries" says; `cargo metadata` lists each with its licence.

Keeping the socket's folder to one user on Windows uses no new library: the app calls Windows'
own security functions through `windows-sys`, which was already part of it
(`crates/sp-winapi/src/windows/security.rs`, used by `crates/sp-app/src/private_folder.rs`).

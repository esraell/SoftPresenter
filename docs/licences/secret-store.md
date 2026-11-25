# Where keys and sign-ins are kept

A working note, not legal advice.

SoftPresenter keeps keys and tokens in the system's own store (`crates/sp-app/src/secrets.rs`).
Nothing new is linked into the app or shipped with it for this.

| System | What is used | Licence |
|---|---|---|
| macOS | The Keychain, through the [`security-framework`](https://crates.io/crates/security-framework) crate, which was already part of the app | MIT OR Apache-2.0 |
| Windows | Credential Manager, through Windows' own calls in `windows-sys`, which was already part of the app (`crates/sp-winapi/src/windows/credentials.rs`) | MIT OR Apache-2.0 |
| Linux | The Secret Service (GNOME Keyring, KWallet), through the `secret-tool` command of [libsecret](https://gnome.pages.gitlab.gnome.org/libsecret/) | LGPL-2.1-or-later |

`secret-tool` is not part of SoftPresenter and does not ship with it: it is a program the
computer already has (or its owner installs, as `libsecret-tools` on Debian and Ubuntu), which
SoftPresenter starts as a separate program and talks to through pipes. Nothing of libsecret is
copied or linked, so its licence places no condition on SoftPresenter.

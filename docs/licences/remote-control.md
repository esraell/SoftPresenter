# Remote Control: the web server's libraries

A working note, not legal advice.

Remote Control (`crates/sp-app/src/remote`) serves a web page over HTTPS, with a WebSocket, to
other devices on the church's network. It is built from these crates, with only the features
it needs: HTTP/1.1 and WebSocket (no HTTP/2, no compression, no middleware stacks), TLS through
the rustls the app already had for downloading Bibles (with its ring crypto, as ureq uses; no
aws-lc), a certificate made on the computer, and argon2id for the password.

| Crate | What for | Licence |
|---|---|---|
| axum 0.8 (`http1`, `ws`, `tokio`) | the routes and the WebSocket upgrade | MIT |
| axum-core 0.5 | axum's own parts | MIT |
| hyper 1 (`http1`, `server`) | HTTP/1.1, with header timeouts and limits | MIT |
| hyper-util 0.1 (`tokio`) | hyper on tokio | MIT |
| tower 0.5 (`util`), tower-layer, tower-service | how axum's routes are called | MIT |
| tokio 1.54 (`rt`, `net`, `time`, `sync`; axum adds `macros`) | the server's own small runtime, on its own thread | MIT |
| tokio-macros 2 | `tokio::select!` | MIT |
| mio 1, socket2 0.6 | tokio's sockets | MIT; MIT OR Apache-2.0 |
| tokio-rustls 0.26 (`ring`, `tls12`) | TLS on tokio, with the app's rustls | MIT OR Apache-2.0 |
| tokio-tungstenite 0.29, tungstenite 0.29 | the WebSocket protocol | MIT; MIT OR Apache-2.0 |
| sha1 0.10, data-encoding 2 | the WebSocket handshake | MIT OR Apache-2.0; MIT |
| rand 0.9, rand_chacha 0.9, ppv-lite86 | tungstenite's (masks for frames a client sends; the server sends none) | MIT OR Apache-2.0 |
| http-body 1, http-body-util 0.1, httpdate 1, mime 0.3, sync_wrapper 1, futures-sink | HTTP bodies, dates and types | MIT; MIT; MIT OR Apache-2.0; MIT OR Apache-2.0; Apache-2.0; MIT OR Apache-2.0 |
| matchit 0.8 | axum's route matching | MIT AND BSD-3-Clause |
| rcgen 0.14 (`ring`, `crypto`) | the certificate made on this computer | MIT OR Apache-2.0 |
| yasna 0.6, time 0.3, time-core, deranged, num-conv, powerfmt | rcgen's (writing the certificate, its dates) | MIT OR Apache-2.0 |
| argon2 0.5 (`alloc`, `password-hash`) | the password, kept only as an argon2id hash | MIT OR Apache-2.0 |
| password-hash 0.5, rand_core 0.6, blake2 0.10, base64ct 1 | argon2's (its PHC string, its hash; the salt comes from ring, not rand_core) | MIT OR Apache-2.0 |

`ring` (random numbers, SHA-256) and `rustls` were already part of the app, through ureq.
`tokio` was in the lockfile before (a dependency of a WebAssembly-only crate) but was never
built for the app itself. Cargo.lock also lists crates rcgen could use but SoftPresenter doesn't
turn on (x509-parser and the asn1 crates); they are not built.

Why each is used directly: axum for the routes and the WebSocket upgrade; hyper and
hyper-util (both already axum's) to serve each connection with header timeouts and limits,
and over the TLS stream, which `axum::serve` doesn't offer; tower (axum's) to call the routes
from that connection; ring (already rustls's) for the system's random source and SHA-256.
Pictures are JPEG, through the `image` crate the app already had: its WebP encoder writes only
lossless pictures, several times larger.

All can be combined with the GNU GPL version 3, as NOTICE.md's "Other libraries" says;
`cargo metadata` lists each with its licence. Remote Control's tests also use
tokio-tungstenite's WebSocket client and futures-util, both already among the crates above.

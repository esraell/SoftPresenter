# Showing an iPhone, iPad or Mac screen: a plan

A plan, written 2026-11-24 before anything was built; the owner approved the list in section 8
the same day, and **milestone M1 is built** (section 9 says what it found, and how to try it
with a real iPhone, which has not been done yet). The rest reads as it was written, except the
held picture in section 4, which is solved.

**What is asked for.** Someone in the church picks SoftPresenter in Screen Mirroring on their
iPhone, iPad or Mac, and their screen appears in SoftPresenter: in a box on a slide (as an NDI
source does today) or full screen like media, in the operator's preview and on the audience
screens. All on the church's own network.

**The short answer.** It can be built, in safe Rust, with FFmpeg doing the decoding as it does
for videos. Current iPhones, iPads and Macs (iOS 26.6, iOS 27, macOS 27) still accept the older
form of the protocol that every open-source receiver speaks. Three things need the owner's
decision before a line is written, and they come first below: the FairPlay step, where the
receiver lives, and the list of new crates.

This is a working note, not legal advice.

## 1. What the owner decides

### 1.1 The FairPlay step (the real decision)

Every sender scrambles the stream's key with FairPlay, Apple's content protection, before it
sends it. A receiver that can't undo that step shows nothing. There is no way round it and no
official way through it for software like ours: as far as we know Apple licenses AirPlay
receiving to makers of hardware (its MFi programme), not to free software. (That is general
knowledge, not from the sources read for this plan.)

Every open-source receiver read for this plan undoes it with the same code, **playfair**:
about 1,800 lines of C, most of it tables, written in 2015 from a study of Apple's own
program. Three things follow.

- **Its legal standing is unsettled.** UxPlay's own README says so: "This project makes use of
  a third-party GPL library for handling FairPlay. The legal status of that library is unclear."
  It handles content protection, and laws against getting round such protection (in the United
  States, section 1201 of the DMCA) may reach it. Set against that: playfair has been public
  since 2015, receivers built on it are packaged by Debian, Fedora, FreeBSD and OpenBSD (UxPlay's
  README lists them), commercial receivers that do the same job have been sold for years, and
  the stream it opens here is the sender's own screen, which the sender chose to show. None of
  that is a legal opinion. If Apple objected, the remedy asked would be to remove it, so the
  plan keeps it in one place that can be removed.
- **It needs a third exception to the clean-room rule.** AGENTS.md says nothing comes from
  decompiling another product. playfair's tables come from exactly that, done by someone else
  and published under the GPL. Using it needs an exception written into AGENTS.md beside the
  two for song documents, saying what is used and from where.
- **Its licence does not carry our additional permission.** playfair is GPL-3.0 (the
  repository's licence file; its source files carry no notice of their own). SoftPresenter is
  GPL-3.0-or-later *with* a permission, given by SoftPresenter's own authors, to combine it with
  the NDI and DeckLink components. playfair's author gave no such permission. Compiled into the
  main program, playfair's GPL would cover a program that also opens NDI and DeckLink, which is
  the very combination the permission exists to allow. That is a conflict we cannot cure
  ourselves.

**Recommended: the receiver is a separate program**, as FFmpeg is. A small helper,
`softpresenter-mirror` (working name), holds everything that speaks to the sender, FairPlay
included. The app starts it, reads status lines and the video from its pipes, and stops it. The
helper never opens NDI or DeckLink, so its licence is plain GPL-3.0-or-later with a GPL-3.0
part, and the main program's licence and permission are untouched. The same arrangement keeps
everything a stranger on the network can reach out of the program that draws the screens
(section 6), and makes the feature removable by deleting one program.

The other choices: build it into the main program (simpler by a few hundred lines, but it
carries the licence conflict above; not recommended), or not build the feature.

### 1.2 What the receiver says it is

A sender decides how to talk from the receiver's discovery record: its feature bits, and
perhaps its model. UxPlay's record names an Apple TV (`AppleTV3,2`, source version `220.68`),
and its README says another Apple TV string works as well. **Nobody in what was read reports
trying a name that isn't Apple's.** UxPlay's maintainer says the model can be changed at build
time (issue #463) but gives no result.

So M1 tries our own model string first (`SoftPresenter1,1`, say). If phones accept it, the
record claims nothing it isn't. If they do not, the owner is asked then whether the record may
name an Apple TV model. No person would ever see that string: what people see is the name we
choose, for example "SoftPresenter (Front Mac)". But it would be the one place where the
receiver says it is something it is not, so it is the owner's call, not a detail.

### 1.3 The approval list

Section 8 has the exact list: new crates with versions and licences, the ported code, the new
program, and what is deliberately left for later approval.

## 2. The protocol, as open-source receivers implement it

### What was read

- **UxPlay** (github.com/FDH2/UxPlay), at commit `fbd040e` of 2026-11-23, version 1.74: `lib/`
  (the control channel in `raop.c` and `raop_handlers.h`, `pairing.c`, `srp.c`, `crypto.c`,
  `fairplay_playfair.c`, `playfair/`, `fairplay_v2/`, `mirror_buffer.c`, `raop_rtp_mirror.c`,
  `raop_buffer.c`, `raop_rtp.c`, `raop_ntp.c`, `dnssd.c`, `dnssdint.h`, `dns_sd/`, `mdnsd/`),
  `renderers/audio_renderer.c`, the README, the commit history of the last 400 commits, and
  issues #336, #379, #458, #535, #540, #547, #570 and #607.
- **RPiPlay** (github.com/FD-/RPiPlay): the README's "AirPlay protocol versions".
- **pyatv**'s protocol notes (`docs/documentation/protocols.md`): the AirPlay service records
  and the HomeKit-style authentication and encryption.
- **The openairplay specification** (openairplay.github.io/airplay-spec): the feature bits, the
  status flags and service discovery.
- **playfair** (github.com/EstebanKubata/playfair): its licence and state (last change 2015).

shairplay and the AirplayServer project were read only as they survive inside UxPlay's `lib/`.
Everything below is from those sources. What they do not settle is marked *not verified*.

### Two protocols, and which one this is

Apple's devices speak two forms. The newer one (AirPlay 2: HomeKit-style pairing, then every
channel encrypted with ChaCha20-Poly1305 under keys made with HKDF, as pyatv describes) is what
an Apple TV speaks. **None of the receivers read here has screen mirroring working over it.**
RPiPlay's notes say every third-party receiver used the older form in 2019, and UxPlay's
attempt at the newer pairing (issue #336, open since 2024) still fails at the first FairPlay
message.

The older one, which the sender's own logs call legacy, is what a third-generation Apple TV
spoke: the control channel in clear, pairing optional, the media encrypted with AES. The
open-source receivers use it, and so would we. Senders choose it from the receiver's record
(section 1.2).

### Discovery

Two DNS-SD services on the local network, both pointing at the same control port:

- `_airplay._tcp`, instance name = the name people see. TXT: `deviceid` (a MAC-shaped id),
  `features` (two hex words), `flags=0x4`, `model`, `pk` (the receiver's Ed25519 public key in
  hex), `pi` (a UUID), `srcvers`, `vv=2`, `pw` (true when a code or password is asked).
- `_raop._tcp`, instance name = `<deviceid without colons>@<name>`. TXT: `ch=2`, `cn=0,1,2,3`
  (PCM, ALAC, AAC, AAC-ELD), `da=true`, `et=0,3,5`, `ft` (the same features), `am` (model),
  `md=0,1,2`, `rhd`, `sf` (status flags: `0x4`; bit 3 = a code is needed, bit 7 = a password is
  needed), `sr=44100`, `ss=16`, `sv=false`, `tp=UDP`, `txtvers=1`, `vs`, `vn=65537`, `vv=2`,
  `pk`, `pw`.

UxPlay's features word is `0x5A7FFEE6` (bits 1, 2, 5, 6, 7, 9 to 22, 25, 27, 28, 30; bit 7 is
screen mirroring, bit 9 audio, bit 30 "the audio service is on this port too"). Three bits
matter to us:

- **Bit 27, legacy pairing.** UxPlay turns it *off* by default (`0x527FFEE6`): the sender then
  skips pairing and connects at once. With it on, senders pair first, which takes about five
  seconds each time. **M1 advertises it off**, as UxPlay does: the most-used path. It goes on
  when the on-screen code comes (M4), which needs pairing.
- **Bit 9, audio.** It stays on even while we play no sound: UxPlay's maintainer reports that
  with it off the sender disconnects after a minute (issue #463).
- **Bit 42, several video codecs.** With it set the sender may send HEVC instead of H.264.
  One report in UxPlay #535 has an iPhone 15 Pro on the iOS 27 beta sending HEVC-shaped
  packets even without it, and working once it was set. So we set it and decode both.

### The control channel

One TCP connection carrying requests shaped like HTTP, some with the version `RTSP/1.0`, each
with a `CSeq` header to echo, bodies as binary property lists or raw bytes. In order, for
mirroring:

1. `GET /info` → a property list: `deviceID`, `macAddress`, `features`, `model`,
   `sourceVersion`, `name`, `pk`, `pi`, `vv`, `statusFlags`, `keepAliveLowPower`,
   `keepAliveSendStatsAsBody`, the audio formats and latencies, and `displays`: one entry with
   the width, height, refresh rate and `maxFPS` we ask for. The sender scales its picture to
   that, so this is where we cap it (1920×1080, 30 or 60 a second).
2. Only if pairing is on: `POST /pair-setup` (32 bytes in; our Ed25519 public key out), then
   `POST /pair-verify` twice (see Pairing).
3. `POST /fp-setup` twice (see FairPlay).
4. `SETUP`, first time: a property list with `ekey` (72 bytes), `eiv` (16 bytes), the sender's
   `deviceID`, `model`, `name`, `osVersion`, `timingProtocol` (`NTP`) and `timingPort`. We
   answer with our `timingPort` and `eventPort` (0).
5. `SETUP` again with `streams`: type 110 is the mirrored video (it brings a
   `streamConnectionID`; we answer with a TCP `dataPort`), type 96 is audio (it brings `ct`,
   `spf`, `audioFormat`, `controlPort`; we answer with UDP `dataPort` and `controlPort`).
6. `RECORD`, `GET_PARAMETER`/`SET_PARAMETER` (volume), `POST /feedback` about once a second (the
   sender's heartbeat; no more of them means it has gone), `FLUSH`, and `TEARDOWN` (with a
   `streams` list to stop one stream, or without to stop everything; iOS 27 sends the second
   kind where earlier versions sent the first, fixed in UxPlay in August 2026).

### Pairing: three ways a receiver can ask who is there

- **None** (UxPlay's default). No pairing requests at all.
- **Legacy pairing** (bit 27 on). `pair-setup` only swaps Ed25519 public keys. `pair-verify`
  is an X25519 exchange: each side sends a fresh public key, both sign the two keys with
  Ed25519, and the signatures travel encrypted with AES-128-CTR under a key and IV that are
  SHA-512 of `"Pair-Verify-AES-Key"` or `"Pair-Verify-AES-IV"` followed by the shared secret,
  cut to 16 bytes. Nothing checks *whose* keys they are, so this proves nothing about the
  sender; it only gives a shared secret that is later mixed into the stream key.
- **A code on the screen** (`pw=true`, status bit 3). The sender posts `/pair-pin-start`, the
  receiver shows four digits, and `/pair-setup-pin` runs SRP-6a (2048-bit group, SHA-1, with
  Apple's own way of making the session key: SHA-1 of the secret with four zero bytes, then
  with 0,0,0,1, joined to 40 bytes). The two then swap Ed25519 keys under AES-128-GCM with a
  **16-byte** IV (keys from SHA-512 of `"Pair-Setup-AES-Key"`/`"-IV"` and the session key). A
  sender that has done this once is remembered by its public key and skips the code next time.
- **A password** (`pw=true`, status bit 7). Plain HTTP Digest (MD5) on the first `SETUP`.
  UxPlay can use a new random four digits for each connection as the password, which gives the
  code-on-screen experience without SRP.

The HomeKit forms (transient, or full with SRP-3072, then ChaCha20-Poly1305 on every channel;
pyatv, and the `pair_ap` library it points to) belong to the newer protocol and are not needed.

### FairPlay

Two posts to `/fp-setup`. The first is 16 bytes (`FPLY`, version byte 3, a mode 0 to 3); the
answer is one of four fixed 142-byte messages. The second is 164 bytes; the answer is a
12-byte header and the request's last 20 bytes. Later the 72-byte `ekey` from `SETUP` is
turned into the 16-byte AES key by `playfair_decrypt(second message, ekey)`. That one function
is all of FairPlay a mirroring receiver needs.

UxPlay gained a second FairPlay implementation in November 2026 (`fairplay_v2`, LGPL, described
as clean-room). It answers version byte **2**, which only Apple Music on a Mac sends (UxPlay
#570); screen mirroring from iOS and macOS sends version 3. We leave it out.

### The key, and the video stream

- With legacy pairing done, the key is replaced by the first 16 bytes of SHA-512(key ‖ X25519
  shared secret). Without pairing it is used as it is. (A debug line in UxPlay says SHA-256;
  the code is SHA-512.)
- The video key and IV are the first 16 bytes of SHA-512(`"AirPlayStreamKey<id>"` ‖ key) and
  SHA-512(`"AirPlayStreamIV<id>"` ‖ key), where `<id>` is the `streamConnectionID` as a decimal
  number. The cipher is **AES-128-CTR**, one keystream running across all encrypted packets.
- The video comes on its own TCP connection as packets with a 128-byte header: bytes 0 to 3
  the payload length (UxPlay reads them in the computer's own byte order, so little-endian),
  byte 4 the kind, bytes 8 to 15 the sender's NTP time.
  Kind 0 is an encrypted picture (length-prefixed NAL units). Kind 1 is the codec's settings
  in clear (H.264 SPS and PPS in `avcC` form, or HEVC VPS, SPS and PPS in `hvc1` form), sent at
  the start and whenever the size changes, with the picture's width and height as floats at
  bytes 16 to 23 and 56 to 63. Kind 5 is a once-a-second report (a property list, sometimes
  with a 25,000-byte tail). Kind 2 is an old heartbeat.
- A phone turned on its side sends a new kind-1 packet with the new size, then a key frame.
- *Not found in what was read:* whether a sender sends key frames at other times, whether a
  receiver can ask for one, and whether a sender keeps sending pictures while its screen is
  still. Both matter to the design (sections 4 and 5) and are M1's first questions.

### Timing

The receiver sends 32-byte NTP-style requests over UDP to the sender's `timingPort` (UxPlay:
every three seconds) and reads the answers, to learn the difference between the two clocks.
The packets' times can then be placed on our clock, which is what keeps sound and picture in
step. *Not verified:* whether a sender carries on if it is never asked.

### Sound

Mirroring always sets up a sound stream beside the video: RTP over UDP, AAC-ELD at 44,100 Hz,
stereo, 480 samples a packet (`ct=8`, `spf=480`). Each packet's whole 16-byte blocks are
encrypted with **AES-128-CBC** under the same key and the `eiv` from `SETUP`, the IV starting
again for each packet; the odd bytes at the end are in clear. A second UDP port carries sync
packets and our requests to send a lost packet again. ALAC is only for music sent on its own,
which we don't plan. A receiver may accept the sound stream and throw the packets away: the
picture doesn't depend on it.

### What recent versions changed

| What | Where it is recorded | State |
|---|---|---|
| macOS 15.2 sent wrong times and broke mirroring; Apple fixed it in 15.3 | UxPlay #379 | Over |
| macOS 26 mirrors to an open receiver | UxPlay #458 | Works |
| iOS 26.6 mirrors (the one report otherwise was a user choosing audio output) | UxPlay #547 | Works |
| iOS 27 stops everything with a `TEARDOWN` that names no stream | UxPlay #540, commit `546820c` | Handle both |
| iOS 27 on some iPhones sends HEVC unless bit 42 is set | UxPlay #535, one report | Set bit 42 |
| iOS 27 sends `combinedGetInfoWithControlSetup` in `SETUP`; ignoring it works | UxPlay #535 | Ignore |
| iPadOS 27 and macOS 27 mirror to an open receiver | UxPlay #535, #607 | Works |
| Devices managed by an employer insist on the on-screen code | UxPlay README | Needs the code (M4) |
| Apple Music on a Mac uses FairPlay version 2 | UxPlay #570 | Not our case |

RPiPlay's author wrote in 2019 that the legacy protocol "can be expected to be removed
entirely". Seven years on it has not been, but section 7 treats that as the main risk.

## 3. Licences and names

| Piece | Licence | Where it stands with GPL-3.0-or-later |
|---|---|---|
| UxPlay as a whole (`uxplay.cpp`, `renderers/`, `lib/crypto.c`) | GPL-3.0-or-later | Compatible. We would use none of these files |
| UxPlay `lib/` (the protocol; from shairplay, AirplayServer and UxPlay's own work) | LGPL-2.1-or-later, by each file's notice | Compatible (LGPL 2.1 section 3 lets a copy be put under the GPL) |
| UxPlay `lib/srp.c` (csrp) | MIT | Compatible |
| UxPlay `lib/playfair/` | GPL-3.0; no notice in the files, GPL-3.0 text in the folder and in the upstream repository | Compatible with the GPL; **not** with our additional permission (section 1.1) |
| UxPlay `lib/fairplay_v2/` | LGPL-2.1-or-later | Not needed |
| UxPlay `lib/llhttp/` (the request parser, from Node.js) | MIT | Not needed: we parse the few requests in Rust |
| UxPlay `lib/mdnsd/` (its own small responder) | LGPL-2.1-or-later | Not needed: a Rust crate does this |
| libplist, OpenSSL, GStreamer (what UxPlay links) | LGPL-2.1, Apache-2.0, LGPL | Not used: Rust crates and FFmpeg do their jobs |

**What we would take.** Nothing is copied as C. The helper is written in Rust *following*
UxPlay's `lib/`: the order of requests, the names in the property lists, the packet layouts and
the key derivations. That is close enough to the source to be a work based on it, so the helper
says so and keeps the notices: its files carry "follows UxPlay's `lib/…`, copyright Juho
Vähä-Herttua, dsafa22, Florian Draschbacher, F. Duncanh and others, LGPL-2.1-or-later", and the
FairPlay module carries playfair's GPL-3.0 and is a line-for-line port (tables and all). The
helper as a whole is GPL-3.0-or-later.

**What the records would need.**

- `docs/licences/screen-mirroring.md`: this table, the sources and commit ids read, each new
  crate with its licence (as `remote-control.md` does), and what the helper is.
- `NOTICE.md`: a section for the helper, naming UxPlay, shairplay, AirplayServer, RPiPlay and
  playfair with their copyright lines and licences, saying that the helper is a separate
  program and that the additional permission for NDI and DeckLink does not extend to it, and
  the trademark line below. The LGPL-2.1 text goes in `docs/licences/` beside it.
- `AGENTS.md`: the clean-room exception (section 1.1), and a line saying the helper may name
  Apple, AirPlay and the receivers it follows where it says what it does.
- The Mac app and the Windows package: the helper beside `ffmpeg`, its notices in the same
  place as FFmpeg's.

**Apple's names.** AirPlay, iPhone, iPad, Mac and Apple TV are Apple's trademarks. Apple's
"Works with Apple AirPlay" badge and the AirPlay symbol are for licensed products, and we use
neither (again general knowledge, not from the sources read). Unofficial receivers describe
themselves by fact: UxPlay calls itself an "AirPlay2
Mirror server". We go plainer:

- In the app: **"Show an iPhone, iPad or Mac screen"**. The box is a **Phone or iPad Screen**
  box. The help line under the switch says what to do on the phone: "On the iPhone or iPad,
  open Control Centre, tap Screen Mirroring and choose *SoftPresenter (Front Mac)*."
- AirPlay is named once, as a fact, where someone would look for it (the help line's second
  sentence, the README, `docs/features.md`): "This uses the phone's own Screen Mirroring
  (AirPlay)."
- `NOTICE.md` and the README: "AirPlay, iPhone, iPad, Mac and Apple TV are trademarks of Apple
  Inc. SoftPresenter is not made, licensed or endorsed by Apple."
- No Apple symbol, no Apple artwork, no "AirPlay" in the feature's name.

## 4. What SoftPresenter needs, piece by piece

"Have" means it is in `Cargo.lock` and built into the app today.

| Piece | Have | New | We write |
|---|---|---|---|
| Finding (DNS-SD records) | `socket2`, `mio`, `fastrand` | **mdns-sd 0.22.0** | The two records |
| Control channel | `httparse` 1.10 (headers only) | none | A small request reader |
| Property lists | none | **plist 1.10.4** | The few lists |
| Ed25519, X25519, SHA-512, random | `ring` 0.17.14, `sha2` 0.10 | none | none |
| AES-128-CTR (video, pair-verify) | none (`ring` has no CTR) | **aes 0.8.4, ctr 0.9.2** | none |
| AES-128-CBC (sound) | none | **cbc 0.1.2** (M2) | none |
| FairPlay | none | none | The port of playfair |
| Stream packets, NAL units | none | none | Yes |
| Decoding H.264 and HEVC | bundled FFmpeg 9.0.2 | none | The pipe and its restarts |
| Decoding AAC-ELD (M2) | bundled FFmpeg (its own `aac`) | none | A tiny Matroska writer |
| Showing pictures | `sp-render`'s live-picture store | none | A second name space in it |
| Playing sound (M2) | `sp-video`'s `sound` (as NDI sound) | none | A listener |
| The on-screen code (M4) | `num-bigint` 0.4.8 (through `rodio`) | **srp 0.6.0**, an AES-GCM crate | Apple's hashing |
| A password instead (M4, if chosen) | none | **md-5 0.10.6** | Digest checking |

### Finding, on all three systems

A sender finds receivers by multicast DNS. Three ways to answer:

1. **A pure-Rust responder in the helper: `mdns-sd`** (recommended). MIT or Apache-2.0, pure
   Rust, no async runtime, one thread of its own, 8.4 million downloads, last release
   2026-11-17, 183 KB of source. It answers on UDP 5353 itself, sharing the port with whatever
   the system runs. The same code on a Mac, Windows and Linux; no Bonjour to install on
   Windows (which has none of its own for registering services unless Apple's is installed),
   no need for Avahi to be running on Linux. It brings four crates we don't have: `flume`
   0.12.0, `spin` 0.9.9, `if-addrs` 0.15.0 and `socket-pktinfo` 0.4.2.
2. **The system's own tool as a child program** (the fallback on a Mac): `/usr/bin/dns-sd -R`
   registers a service through Bonjour with no crate and no `unsafe`. Linux has
   `avahi-publish` only where avahi-utils is installed; Windows has nothing.
3. **Calling Bonjour or Avahi directly**: needs `unsafe` and a C library on each system. No.

*To check in the prototype:* that `mdns-sd` beside macOS's own responder, Windows' and Avahi
is seen reliably by phones; UxPlay moved to a built-in responder of its own in 1.74 for the
same reasons, which is encouraging. A Mac with its own AirPlay Receiver switched on is
generally known to hold ports 5000 and 7000 (not from the sources read; to confirm in M1), so
we never use those.

Senders often connect over IPv6 link-local addresses, so the helper listens on both families.

### The control channel and property lists

`httparse` refuses a request line that ends `RTSP/1.0`, so the helper reads the request line
itself and gives the header block to `httparse::parse_headers`. Bodies are read to
`Content-Length` under a hard limit. Axum and hyper (Remote Control's) cannot be used: they
speak HTTP only.

Property lists: **plist 1.10.4** (MIT, 62 KB, 106 million downloads, last release 2026-11-16,
pure Rust). Used through its `Value` type with default features off. Its dependencies
(`base64`, `indexmap`, `time`, `quick-xml`) are already in the lockfile; it may ask for
`quick-xml` 0.42 beside the 0.41 we have.

### The ciphers

`ring` gives Ed25519 signing and checking, X25519 with a one-use private key (which is exactly
what pair-verify does), SHA-512 and random bytes. It does not give AES in CTR or CBC mode, MD5,
SRP, or AES-GCM with a 16-byte IV (ring fixes the IV at 12 bytes). So:

- **aes 0.8.4 + ctr 0.9.2** (RustCrypto, MIT or Apache-2.0, pure Rust with the processor's AES
  instructions, 434 and 281 million downloads). They bring `cipher` 0.4.4 and `inout` 0.1.4.
  These are the versions that share `digest` 0.10, `crypto-common` 0.1 and `generic-array`
  with the `sha2` already in the app: 4 new crates. The newest line (aes 0.9, ctr 0.10) would
  bring 10 and a second copy of several we have; move to it when `sha2` moves.
- **cbc 0.1.2** for the sound, with `block-padding` 0.3.3: M2.
- We write no cipher, no curve and no hash. The FairPlay port is table lookups and XORs of a
  fixed public algorithm; it protects nothing of ours and is tested against the original's
  answers.

### The on-screen code, later

The code needs SRP and AES-GCM with a 16-byte IV. `srp` 0.6.0 (RustCrypto, MIT or Apache-2.0)
exposes the group arithmetic (`compute_b_pub`, `compute_premaster_secret`) on the `num-bigint`
already built into the app, so the only part we would write is Apple's way of hashing the
result, which is SHA-1 over known inputs. AES-GCM with a 16-byte IV needs the RustCrypto
`aes-gcm` crate. Neither is asked for now: section 5 gets the first three milestones done
without them, and they come back for approval with exact versions at M4.

### Decoding the picture with our FFmpeg

The helper turns each picture into what FFmpeg's raw H.264 or HEVC reader takes (start codes
in place of the length prefixes, the settings from the kind-1 packet in front of the first key
frame) and writes it to an FFmpeg child's standard input; a thread reads raw pictures from its
standard output. The bundled FFmpeg already has everything: the `h264` and `hevc` readers and
decoders, `pipe`, `scale`, raw video out, VideoToolbox on a Mac and Direct3D 11 on Windows. No
rebuild.

A throwaway spike on this Mac (the app's own FFmpeg 9.0.2, a 1080p test pattern, pictures fed
one by one at 30 and at 60 a second, each timed in and out; deleted afterwards) found:

| Question | Result |
|---|---|
| Does it work at all? | Yes: every picture fed in comes out, from a raw H.264 pipe and from a Matroska stream written by hand |
| How late is a picture, fed plainly? | About two pictures' time (72 ms at 30 a second): FFmpeg's raw reader waits for the next picture to know this one has ended |
| With an access-unit delimiter written after each picture, or as Matroska blocks? | About one picture's time: 36 ms at 30 a second, 20 to 25 ms at 60. The decoding itself is a few milliseconds |
| **Is the newest picture held back?** | **Yes, always one.** Feed 60 pictures and stop: 59 come out, and the 60th only when a 61st arrives or the input ends. It is not the reader (copying the packets straight through holds nothing); it is somewhere between FFmpeg's decoder and its output. `-fps_mode passthrough`, `-threads 1`, `-flush_packets 1`, `-strict strict` and `-copyts` do not change it |
| Hardware or software? | The same at 1080p30, but hardware takes 100 ms to its first picture |
| `-fflags nobuffer`, the usual advice | **Produces no pictures at all** with this FFmpeg and a raw H.264 pipe. Don't use it |
| The size changes mid-stream (a phone turned) | FFmpeg silently stretches the new pictures to the old size. We must see the change and start a new child |

**The held picture was the open problem for M1, and M1 solved it with a setting** (the first
of the ways out this plan listed: a setting, real senders that keep sending, the system's own
decoder, a change to our FFmpeg). Stage by stage, with FFmpeg's own trace of each picture
(`-debug_ts`), the picture was found waiting in FFmpeg's *raw-video writer*, not its reader or
decoder: the "encoder" for raw video runs on several threads by default and keeps each picture
until the next arrives. **`-threads 1` given as an output option** (after `-i`; as an input
option it only changes the decoder, which is what the first spike tried) releases it. With
that and the delimiter after each picture, 60 pictures in are 60 pictures out with the input
still open, and the newest picture is out about 8 ms after it went in (1080p, this Mac). The
helper's tests feed the last picture with none after it and require it out.

So the command is: `ffmpeg -flags low_delay -probesize 32 -analyzeduration 0
-f h264 -i pipe:0 -an -sn -dn -threads 1 -vf scale=W:H -pix_fmt rgba -f rawvideo pipe:1`
(`-f hevc` for HEVC; RGBA because the renderer's door takes it), with
the format always named and never guessed, and W and H taken from the kind-1 packet and capped
at 1920×1080. A new kind-1 packet with another size ends that child and starts the next; the
key frame that always follows it is the new child's first picture.

On Linux the app has no FFmpeg of its own and uses the system's; mirroring there needs an
`ffmpeg` with H.264 and HEVC, which every distribution's has.

### Showing the pictures

An NDI source's pictures reach the screens through one small door: the receiving thread calls
`sp_render::show_ndi(name, picture)`, which puts the newest picture where every drawing thread
finds it, keeps nothing older, and marks the screens to draw again. The renderer holds the last
picture for two seconds when a source stops, then fades it, and fades it back in when it
returns. `crates/sp-app/src/ndi_in.rs` starts and stops receivers as slides come and go.

Mirrored screens use the same door under their own names (`mirror:<device>` beside
`ndi:<source>`), so a mirrored screen in a box behaves in every way as an NDI source does:
in the editor, in thumbnails, on stage layouts, when it stops. The model gets one new kind of
box beside `ObjectKind::Ndi`.

Full screen "like media" goes through the media layer instead, which takes a video's two
planes (NV12) straight to the GPU (`sp_video::Lending::Planes`); FFmpeg hands us NV12 anyway.
That is M4. Until then a box that fills the slide does the same for the audience.

### Sound

M2. The packets are decrypted (AES-CBC), put in order by a short queue (three packets, about
30 ms), and written to a second FFmpeg child as a tiny live Matroska stream that the helper
writes itself (a header naming AAC with the four configuration bytes `F8 E8 50 00` UxPlay
uses, then one block per packet); the child returns 48 kHz float samples, which join the app's
mixer through the listener NDI sound already uses. ADTS, the usual wrapper for raw AAC, cannot
describe AAC-ELD, hence Matroska. The spike confirmed the bundled FFmpeg's own decoder plays
AAC-ELD made by Apple's encoder, and that it reads a Matroska stream written by hand from a
pipe (for video); AAC-ELD in one is to be proved at M2.

## 5. Design

### Two programs

```
phone ──network──▶ softpresenter-mirror (helper) ──pipes──▶ SoftPresenter
                     finding, control channel,                 status lines in,
                     pairing, FairPlay, decrypting             commands out,
                     ▼                                         raw pictures in
                   ffmpeg (decoding)
```

The app starts the helper when the feature is switched on and stops it when it is switched
off or the app quits. Between them: the helper's standard output carries pictures (a small
header with a size, then the bytes); its standard error carries one JSON line per event
(a device asks, connected, size changed, stopped, a problem in plain words); its standard
input takes commands (allow, refuse, stop, the name to show). A helper that dies is started
again; its sessions are lost and the phones reconnect by hand.

### Threads, and the screens' thread

Nothing in this feature runs on the thread that draws the screens. In the helper: one thread
accepts connections, one per control connection, one reads the video stream and feeds FFmpeg,
one reads FFmpeg's pictures, one for timing, and at M2 one for sound; all blocking, all with
timeouts. In the app: one thread reads the helper's pictures and calls the renderer's door,
one reads its events. The drawing threads only ever pick up the newest finished picture, as
they do for NDI.

### Delay

Target: under a fifth of a second from the phone's glass to the screen, typically. Our part
is one picture's time in FFmpeg (17 to 36 ms, measured above; see the held picture there) and
up to one screen refresh; the rest is the phone's encoder and the Wi-Fi, which we don't
control. No queue for pictures anywhere: the
newest wins, a late one is dropped. M1 measures the whole path by filming a stopwatch on the
phone beside the screen.

### How it appears in the app

- **Settings › Phone Screens**: a switch (off until someone turns it on), the name phones see
  ("SoftPresenter (Front Mac)", editable), who may connect, and the devices connected now with
  a Stop beside each.
- **A box on a slide**: *Phone or iPad Screen*, added like an NDI box. Its Source menu offers
  "Whoever is connected" (the default) and the devices seen. Fit and Fill as for pictures.
  Empty, on the screens, it shows nothing, exactly as an NDI box with no source.
- **In the operator's window**, when a device asks: "Anna's iPhone wants to show its screen",
  with Allow and Don't Allow. It never appears on the audience screens.
- **`sp`**: `sp mirror` to list, allow, refuse and stop, so Remote Control and scripts can do
  what the window does.

### Who may connect

The fixed rule first: **a sender can never put itself on a screen.** A mirrored picture shows
only where the operator has put a box (or, later, a media item) and taken it live. Connecting
only makes a picture available.

On top of that, the operator chooses one of:

1. **Ask me each time** (the default, M1). The catch is the key frame: a sender sends its
   settings and first whole picture once, at the start, and nothing read says it sends another
   unless the size changes. Pictures thrown away while the operator decides could not be made
   up afterwards. So, in the order M1 tries them:
   - **Hold the answer to the sender's first `SETUP` until the operator answers.** Nothing
     flows before Allow, and Don't Allow gives the phone a proper refusal. It depends on how
     long a sender waits for that answer, which only a real phone can say. UxPlay has the hook
     at this point but answers it at once.
   - **If senders won't wait: answer at once and decode, but hand nothing to the app until
     Allow.** The pictures stop in the helper. The cost is that a device nobody has allowed
     reaches FFmpeg, which is why FFmpeg is fenced as it is (section 6).

   A device is asked about again each time it connects: the device id a sender gives is only
   something it says, so "remember this device" would be a promise we can't keep.
2. **A code on the screen** (M4). Four new digits each time, shown in the operator's window
   or, if the operator wants, on a screen; three wrong tries make a new code. Done with SRP
   pairing, a device that has entered the code once can be remembered properly, by its key.
   Employer-managed devices need this form.

One sender at a time in M1: a second one is refused while the first is live, and never
replaces it. Several at once, each its own source, is M4.

### When a sender goes

No heartbeat for a few seconds, the stream closing, or `TEARDOWN`: the session ends, the box
holds the last picture for two seconds and fades (the renderer already does this), the
operator's window says "Anna's iPhone stopped". The slide stays as it is otherwise.

### Ports, firewalls and quiet runs

Fixed ports by default, so a firewall rule can name them, with the next free ones if they are
taken: TCP 7100 (control) and 7101 (video), UDP 7102 (timing), 7103 and 7104 (sound), and UDP
5353 for finding. On Windows the first start brings the Defender Firewall question, as Remote
Control does; "Private networks" must be allowed. A recent macOS is expected to ask for Local
Network access the first time (to confirm in M1). On Linux the person opens the ports
themselves; the Settings pane lists them.

A quiet run (`SOFTPRESENTER_QUIET=1`) never announces itself and listens on 127.0.0.1 only,
as Remote Control does, so a test copy never shows up on anyone's phone.

Networks that keep devices apart (guest Wi-Fi with client isolation, phones and the computer
on different VLANs) will not find the receiver. That is the network's doing, and the pane
says so.

## 6. Security

Written with the `security-audit` skill's method, in guidance mode: there is no code yet, so
these are the boundaries and the rules the code must keep, not findings.

### Who can reach what

| Who | Reaches | Before any permission? |
|---|---|---|
| Any device on the church's network | UDP 5353 (finding); TCP control port; TCP video port; UDP timing and sound ports | Yes, all of it |
| A device the operator allowed | The same, and its pictures reach a box that is live | After Allow |
| The helper | FFmpeg's input (hostile video), the app's pipes | It is the boundary |
| FFmpeg | Only bytes on its input | A C decoder fed by a stranger |

The network is not trusted: a church's Wi-Fi password is on the notice board.

### What must hold

1. **Only the operator decides what the audience sees.** Connecting changes no slide and
   takes nothing live. Nothing a sender sends can choose a box, a screen or a time.
2. **Pictures from a device that has not been allowed never leave the helper.** With the
   first way of asking (section 5) they are never sent at all; with the second they are
   decoded and dropped there, and the app is handed nothing.
3. **Nothing a sender does can stall or crash the program that draws the screens.** All
   parsing of network input, and FairPlay, run in the helper; decoding runs in FFmpeg. Either
   may die: the app shows "stopped" and starts the helper again.
4. **No memory-unsafe code of ours touches network input.** The helper is safe Rust: no
   `unsafe`, no C, no vendored C. UxPlay's C is read, not linked. (Its history shows why:
   a crafted `TEARDOWN` crashed it, CVE-2025-60458; header sizes had to be capped in January
   2026; its video reader allocates whatever length the header claims.)
5. **Every length that comes off the wire is checked against a limit before anything is
   allocated or read:** request line and headers (8 KB, 32 headers), bodies (64 KB), property
   lists (size and depth), video packets (4 MB), NAL lengths inside a packet, sound packets
   (2 KB). Too big ends the connection.
6. **Everything has a time limit and a count limit:** a few control connections at once, a
   few a minute from one address, seconds to finish a request, one live session. A flood
   costs the feature, never the service.
7. **FFmpeg is told exactly what it is reading.** The format is always named (`-f h264`,
   `-f hevc`, `-f matroska`) and never guessed, so a sender cannot make it open a playlist or
   a file by sending bytes that look like one. No argument ever contains text from a sender.
   Our FFmpeg is built with no network protocols.
8. **Names from senders are untrusted text.** A device's name is shown, cut to a length, with
   control characters removed, and never used as a file name or in a command.
9. **Keys stay private.** The receiver's Ed25519 key is made on first use and kept in the
   system's own store (`secrets.rs`); the device id is random, not the computer's real
   address. No key, code or stream key is ever logged.
10. **The code, when it comes, is guarded:** new for each attempt, three tries, compared in
    constant time, a pause after failures.
11. **Off until switched on;** off in quiet runs; nothing is announced while it is off.

### What the encryption is worth, honestly

Without pairing, anyone on the same Wi-Fi who records the start of a session can decrypt the
whole of it: the FairPlay step is public knowledge. With legacy pairing an eavesdropper who
only listens cannot, but one who can stand in the middle can. So the stream is treated as
**not private**. For a screen that is about to be shown to a room that is acceptable, and the
Settings pane should not suggest otherwise. What a phone shows while mirrored (messages
arriving, for one) is the sender's to mind; the help line says so.

### How it is checked

- Tests that throw cut-off, oversized and random input at each parser (requests, property
  lists, stream packets, sound packets) and require an error, never a panic or a hang.
- A sender written for the tests, using the same crypto crates, that pairs, sets up and sends
  an encrypted stream of a test pattern, with the FairPlay key supplied directly.
- The FairPlay port checked against answers computed once from the original C.
- Before the feature ships: a full `security-audit` review of the helper, which is small and
  has one job.

## 7. Plan, size and risks

| Milestone | What works at the end | Needs |
|---|---|---|
| **M1: an iPhone in a box, on a Mac** | The helper; found by a phone; Ask me each time; H.264 and HEVC; a box on a slide; turning the phone; stopping. Delay measured. Answered with a real phone: our own model string or not, how long a sender waits at `SETUP`, whether a still screen keeps sending, and the held picture | An iPhone and a Mac as senders; the approvals in section 8 |
| **M2: sound** | The phone's sound through the app's mixer, in step with the picture; the box's Sound switch | `cbc` |
| **M3: Windows and Linux** | The same on both; the firewall question; packaged with its notices | The Windows PC and the Linux machine, when the owner says |
| **M4: finish** | The on-screen code and remembered devices; several senders; full screen as media; `sp mirror`; Spanish, Russian and Ukrainian; `docs/features.md`, the roadmap line, the licence note | `srp`, AES-GCM (asked then) |

**Size.** The helper: about 3,500 lines of Rust, of which some 1,300 are the FairPlay port
(mostly tables), plus about 1,500 of tests. The app's side (the box, Settings, the events and
pictures threads, `sp`, translations): about 2,000. On the roadmap's scale: L. On disk the
helper is a second small program, a few megabytes.

**Without an iPhone.** The parsers, the pairing, the ciphers, the stream handling and the
FFmpeg pipe are all tested by the test sender above, with no device and no network beyond
127.0.0.1. A second Mac is a real sender too (Control Centre › Screen Mirroring). What only a
real device proves: that phones find the receiver, FairPlay against a real sender, each iOS
version's habits, the delay, and sound in step. Each milestone ends with the owner's iPhone.

**Risks.**

| Risk | How likely | What we do |
|---|---|---|
| Apple removes the legacy protocol in an update | Unknown; forecast since 2019, not yet happened | Say in the app's help that it depends on the phone's software; follow UxPlay; no open receiver has the newer protocol working, so there would be no quick fix |
| Apple objects to FairPlay handling | Low, not zero | It lives in one removable program (section 1.1) |
| An iOS update changes a detail (as iOS 27 did twice) | Likely, yearly | Small fixes; the test sender grows a case each time |
| `mdns-sd` and the system's responder disagree on some system | Possible | Checked in M1 and M3; `dns-sd` as the Mac fallback |
| Protected video (films in streaming apps) shows black | Expected of any mirroring; not verified here | Say so in the help |
| An older PC can't decode 1080p HEVC in time | Possible | Ask the sender for 30 a second; hardware decoding on Windows |
| FFmpeg's held picture has no clean cure | It had one | `-threads 1` for the raw-video writer (section 4) |
| Senders won't wait at `SETUP` for the operator | Unknown | The second way of asking (section 5) |
| The church's Wi-Fi hides devices from each other | Common on guest networks | Say so in the pane |

## 8. The list to approve

**Decisions**

1. Build the receiver, with the FairPlay step, as described in section 1.1.
2. As a **separate helper program**, shipped beside FFmpeg, in safe Rust: no `unsafe`, no C,
   no FFI, nothing vendored.
3. A third clean-room exception in AGENTS.md: the helper follows UxPlay's `lib/` (LGPL-2.1-
   or-later) and carries a Rust port of playfair (GPL-3.0); it may name Apple, AirPlay and
   those projects as facts.
4. The discovery record tries a model string of our own first. Only if M1 shows that phones
   need an Apple TV model is the owner asked whether it may name one (section 1.2).
5. The words in section 3, and the trademark line in NOTICE.md.

**Code ported** (rewritten in Rust, with their notices)

| From | Licence | Why |
|---|---|---|
| UxPlay `lib/`: the protocol's behaviour | LGPL-2.1-or-later | It is the working description of the protocol |
| playfair (as carried in UxPlay `lib/playfair/`) | GPL-3.0 | The FairPlay step; nothing else does it |

**New crates for M1** (all pure Rust; for the helper only)

| Crate | Licence | Why |
|---|---|---|
| plist 1.10.4 | MIT | The property lists every request and answer uses |
| mdns-sd 0.22.0 | MIT OR Apache-2.0 | Being found by phones, the same way on all three systems |
| flume 0.12.0 | MIT OR Apache-2.0 | mdns-sd's |
| spin 0.9.9 | MIT | flume's |
| if-addrs 0.15.0 | MIT OR BSD-3-Clause | mdns-sd's (the computer's network addresses) |
| socket-pktinfo 0.4.2 | MIT | mdns-sd's (which network a question came in on) |
| aes 0.8.4 | MIT OR Apache-2.0 | The video's cipher; ring has no CTR mode |
| ctr 0.9.2 | MIT OR Apache-2.0 | CTR mode |
| cipher 0.4.4 | MIT OR Apache-2.0 | aes's and ctr's |
| inout 0.1.4 | MIT OR Apache-2.0 | cipher's |

Possibly a second version of `quick-xml` (0.42 beside 0.41), for plist. Already in the app and
used as they are: ring, sha2, httparse, socket2, mio, fastrand, base64, indexmap, time.

**New crates for M2**

| Crate | Licence | Why |
|---|---|---|
| cbc 0.1.2 | MIT OR Apache-2.0 | The sound's cipher |
| block-padding 0.3.3 | MIT OR Apache-2.0 | cbc's |

**Not asked for now** (they come back at M4 with exact versions): `srp` 0.6.0 and an AES-GCM
crate for the on-screen code; `md-5` only if a password is preferred to a code.

**No change** to the bundled FFmpeg, and no new `unsafe` anywhere.

## 9. M1 as built (2026-11-24)

**What works, and how it was tried.** `softpresenter-mirror` (crate `sp-mirror`) and the app's
side (`crates/sp-app/src/mirror.rs`, Settings › Phone Screens, the Phone or iPad Screen box,
`sp mirror`). Tried with a sender written for the tests, over 127.0.0.1 only: through the
helper alone (`cargo test -p sp-mirror`: asking, Allow, Don't Allow, a second sender turned
away, H.264, HEVC, a sender that pairs first, a phone turned on its side, stopping), and
through a quiet test copy of the app with a box on a live slide, whose audience screen showed
the sender's test pattern. **No real iPhone, iPad or Mac has been tried**, and nothing was
announced on the network.

**Measured** (an M-series Mac, the app's FFmpeg 9.0.2, 1920×1080, from the picture leaving the
sender's socket to the picture on the helper's standard output): H.264 at 30 a second, median
14 ms (95th 51 ms on a busy computer); at 60 a second, 9 ms; HEVC at 30 a second, 7 ms. The
app then shows it at the next refresh. Through a quiet test copy of the app for 20 seconds
(a build without optimisation, 1920×1080 H.264 at 30 a second, the box on a live slide): 540
pictures in, 540 decoded, none replaced by a newer one before the app took it; the app at
about 10% of one processor and its memory level (some 135 MB above idle), the helper at 2%
and 26 MB.

**Differences from the plan above.**

- The helper's Ed25519 key is made afresh each run and never stored: nothing depends on it
  until devices are remembered (M4), so no key crosses the pipe or sits in a file.
- Pictures cross the pipes as RGBA (8 MB each at 1080p), as the renderer's door takes them;
  NV12 would be a third of that and is worth it if a slower computer struggles (M3).
- Legacy pairing is not advertised (as planned), but a sender that pairs anyway is answered.
- Limits beyond the plan's: three connections from one address; an address the operator
  refused with Don't Allow is not asked about again for a minute (the operator is told once
  that it tried); a picture may change size eight times in ten seconds; a property list may
  unfold to 2048 values and 256 KB.
- `sp mirror` exists already (the plan had it in M4), so a test copy can be driven.

**What only a real device answers** (section 7's list): whether an iPhone lists and accepts a
receiver whose model is `SoftPresenter1,1`; how long it waits at `SETUP` for the operator;
whether it keeps sending while its screen is still (the app keeps the last picture up either
way); FairPlay against a real sender; H.264 or HEVC; the delay on glass.

### Trying it with an iPhone (the owner's test)

1. Build a normal copy on this Mac: `scripts/macos-app.sh`, then open
   `target/release/SoftPresenter.app` (not a quiet copy: a quiet copy never announces itself).
   The Mac and the iPhone must be on the same Wi-Fi.
2. Settings › Phone Screens: tick **Show an iPhone, iPad or Mac screen**. macOS may ask to let
   SoftPresenter find devices on the local network: allow it. The pane shows the ports
   (7100 and 7101).
3. In the Edit view add a **Phone or iPad Screen** box to a slide, and put that slide on
   screen.
4. On the iPhone: Control Centre › Screen Mirroring. "SoftPresenter — <this Mac's name>" should
   be in the list within a few seconds. Tap it.
5. In SoftPresenter the notices button turns amber with a "?" (the question does not open by
   itself). Open the notices: "<the iPhone's name> wants to show its screen." Choose **Allow**
   within 30 seconds; after that the iPhone is turned away, a notice says so, and it can try
   again at once. (After **Don't Allow** the same iPhone is turned away for a minute without
   being asked about; a notice says so the first time it tries.) The iPhone's screen should
   appear in the box within a second or two; turn the phone on its side and the picture should
   follow.
6. Stop from the iPhone (Stop Mirroring) or with Stop in the pane.

The log (`~/Library/Logs/SoftPresenter/SoftPresenter.log`, lines with `[mirror]`) should say,
in order: `listening on port 7100 (video 7101), announced to the network`; `a connection from
<address>`; `GET /info`; `FairPlay: first message, mode N`; `FairPlay: second message`;
`SETUP …`; `<name> (<model>) asks to show its screen; holding the answer for the operator`;
`allowed; the sender waited N ms for the answer`; `the sender asked for the video stream`;
`the video stream connected`; `h264 at WxH, shown at WxH` (or `hevc`); `the first picture came
out of FFmpeg N ms after it started`; and at the end `the video stream ended (…): N pictures
in, …`. No line ever carries a key.

If it fails, send back the `[mirror]` lines of the log and what the iPhone showed. Where it
stops says what to try:

| What happens | What it means, and what to try |
|---|---|
| SoftPresenter is not in the iPhone's list, and the log has no `a connection from` | The iPhone does not see the announcement: Local Network permission (System Settings › Privacy & Security › Local Network), a firewall, or a Wi-Fi that keeps devices apart. `dns-sd -B _airplay._tcp` in Terminal on the Mac should list it. If it is not listed even there, the helper's own responder is not getting on beside the Mac's: start the app from Terminal with `SOFTPRESENTER_MIRROR_ANNOUNCE=system` set (as in the next row), which announces through the Mac's own Bonjour instead, and say which of the two worked |
| It is in the list on a Mac's Screen Mirroring but not an iPhone's, or tapping it fails at once with only `GET /info` in the log | The iPhone may not take a model name that is not Apple's. Quit, start it from Terminal with `SOFTPRESENTER_MIRROR_MODEL=AppleTV3,2 target/release/SoftPresenter.app/Contents/MacOS/softpresenter`, and try again. If that is what it takes, it is the owner's decision whether a build may say so (section 1.2) |
| The log reaches `asks to show its screen` and the iPhone gives up before Allow is pressed | The iPhone does not wait at `SETUP`: note how long it waited (the log says). The second way of asking (section 5) is then needed |
| `the video stream connected` but no `first picture`, or `the codec's settings were not understood` | The picture's format differs from what the open receivers describe. Try once with `SOFTPRESENTER_MIRROR_H264_ONLY=1` set the same way, which stops the iPhone choosing HEVC |
| `a picture could not be read after decrypting` | The key is wrong: the FairPlay step or pairing differs on this iOS version. Send the lines; nothing to try by hand |
| The picture shows, then stops after about a minute | Something a sender expects is missing (sound packets are taken and dropped; the time is asked every three seconds). Send the last lines |

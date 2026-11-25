# Showing an iPhone, iPad or Mac screen: what it is built from

A working note, not legal advice.

SoftPresenter can show the screen of an iPhone, iPad or Mac that picks it in Screen Mirroring
(AirPlay). **This is unofficial.** It is not made, licensed, certified or supported by Apple, it
uses no Apple software, and an Apple update may stop it working. AirPlay, iPhone, iPad, Mac and
Apple TV are trademarks of Apple Inc.; SoftPresenter is not made, licensed or endorsed by Apple,
and uses neither the AirPlay symbol nor Apple's "Works with Apple AirPlay" badge.

## A separate program

Everything that speaks to the sending device is in a program of its own, `softpresenter-mirror`
(`crates/sp-mirror`), shipped beside FFmpeg. SoftPresenter starts it when the feature is switched
on, and talks to it only through its standard input, output and error. No code of it is in or
linked with SoftPresenter itself, and it links neither NDI nor DeckLink.

That matters for the licences. SoftPresenter itself is GPL-3.0-or-later **with** an additional
permission, given by SoftPresenter's own authors, to combine it with the NDI and DeckLink
components ([ADDITIONAL-PERMISSION.md](../../ADDITIONAL-PERMISSION.md)). `softpresenter-mirror`
carries a part whose author gave no such permission (playfair, below), so **that permission does
not extend to `softpresenter-mirror`**: the helper is plain **GPL-3.0-or-later**, with its
FairPlay module under **GPL-3.0**. The text of the GNU GPL, version 3, is in
[LICENSE](../../LICENSE).

It is also the safer arrangement: what a stranger on the church's network can reach is a small
program in safe Rust (no `unsafe`, no C, no foreign code), away from the program that draws the
screens, and the feature can be removed by removing one program.

## What it follows

Nothing was copied as C. The helper is written in Rust *following* open-source receivers: the
order of the requests, the names in the property lists, the packet layouts and the key
derivations. That is close enough to be a work based on them, so their notices are kept here and
at the top of each module.

### UxPlay's `lib/` (LGPL-2.1-or-later)

From UxPlay ([github.com/FDH2/UxPlay](https://github.com/FDH2/UxPlay)), commit `fbd040e`
(version 1.74), whose `lib/` descends from shairplay, AirplayServer and RPiPlay. Each file read
carries the GNU Lesser General Public License, version 2.1 or (at your option) any later
version; its text is in [LGPL-2.1.txt](LGPL-2.1.txt). Under section 3 of that licence a copy may
be put under the GNU GPL instead, which is how it is part of this GPL program.

| Module of `crates/sp-mirror/src` | Follows | Copyright |
|---|---|---|
| `request.rs`, `server.rs` | `raop.c`, `http_request.c`, `http_response.c` | (C) 2011-2012 Juho Vähä-Herttua; modified by F. Duncanh 2021-2026 |
| `server.rs`, `plists.rs` | `raop_handlers.h` | (C) 2018 Juho Vähä-Herttua; modified by F. Duncanh |
| `server.rs`, `stream.rs` | `raop_rtp_mirror.c`, `byteutils.c` | (c) 2019 dsafa22; modified by Florian Draschbacher and by F. Duncanh 2021-2023 |
| `server.rs` (asking the time) | `raop_ntp.c` | (c) 2019 dsafa22 and 2014 Joakim Plate; modified by Florian Draschbacher and by F. Duncanh 2021-23 |
| `keys.rs` | `pairing.c` | (C) 2018 Juho Vähä-Herttua, (C) 2020 Jaslo Ziska; modified by F. Duncanh 2021, 2023 |
| `keys.rs` | `mirror_buffer.c` | (c) 2019 dsafa22; modified by F. Duncanh 2022 |
| `discovery.rs` | `dnssd.c`, `dnssdint.h`, `dns_sd/dns_sd.c` | (C) 2011-2012 Juho Vähä-Herttua; modified by F. Duncanh 2022, 2026 |
| `fairplay/mod.rs` (the two answers), `fairplay/replies.rs` | `fairplay_playfair.c` | (C) 2018 Juho Vähä-Herttua, (C) 2026 Omar Roth |

> This library is free software; you can redistribute it and/or modify it under the terms of
> the GNU Lesser General Public License as published by the Free Software Foundation; either
> version 2.1 of the License, or (at your option) any later version.
>
> This library is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY;
> without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See
> the GNU Lesser General Public License for more details.

### playfair (GPL-3.0)

Every sender scrambles its stream's key with FairPlay before sending it; a receiver that cannot
undo that shows nothing. Every open-source receiver undoes it with **playfair**
([github.com/EstebanKubata/playfair](https://github.com/EstebanKubata/playfair), 2015), as
carried in UxPlay's `lib/playfair/`: about 1,800 lines of C, most of it tables, published under
the GNU General Public License, version 3 (the text in its folder; its source files carry no
notice of their own).

`crates/sp-mirror/src/fairplay` is a port of it to Rust, of only what a mirroring receiver needs
(`playfair_decrypt` and what it calls):

- `mod.rs`: `playfair.c`, `omg_hax.c`, `modified_md5.c` and `sap_hash.c`, function by function,
  by hand.
- `tables.rs`: the tables of `omg_hax.h` and the fixed bytes of `omg_hax.c`, turned into Rust by
  a script; every number is the original's.
- `garble.rs`: `hand_garble.c`, turned into Rust statement by statement by a script that keeps
  C's arithmetic.
- `vectors.txt`: 24 messages and scrambled keys with the keys the original C makes of them
  (compiled with clang from that commit), which the port's tests must reproduce.

**The clean-room rule and this module.** SoftPresenter's rule is that nothing comes from
decompiling another product. playfair's tables came from a study of Apple's own program, done by
someone else and published under the GPL in 2015. Using it is the third exception to that rule,
agreed by the owner on 2026-11-24 and written into [AGENTS.md](../../AGENTS.md). Its legal
standing is unsettled (UxPlay's own README says so: it handles content protection, which some
countries' laws restrict getting round); set against that, it has been public since 2015,
receivers built on it are packaged by Debian, Fedora, FreeBSD and OpenBSD, and the stream it
opens here is the sender's own screen, which the sender chose to show. It lives in one program
that can be removed.

## What it is not

- It does not say it is an Apple product. The record it announces names its own model
  (`SoftPresenter1,1`), and the name people see is the one the operator chose.
- It keeps nothing of what a device shows, and no key is written anywhere.
- The stream is **not private**: without pairing, anyone on the same network who records the
  start of a session can decrypt it, because the FairPlay step is public knowledge. For a screen
  that is about to be shown to a room that is acceptable; the app does not suggest otherwise.

## Crates

All pure Rust, used only by `softpresenter-mirror` (approved by the owner on 2026-11-24).

| Crate | What for | Licence |
|---|---|---|
| plist 1.10.4 (no default features; its event stream) | the property lists every request and answer uses | MIT |
| quick-xml 0.42 | plist's (its XML reader, which the helper never calls: only binary lists are read) | MIT |
| mdns-sd 0.22.0 (no default features) | being found by devices (DNS-SD over multicast DNS), the same on a Mac, Windows and Linux | MIT OR Apache-2.0 |
| flume 0.12.0, spin 0.9.9 | mdns-sd's (its channel) | MIT OR Apache-2.0; MIT |
| if-addrs 0.15.0 | mdns-sd's (the computer's network addresses) | MIT OR BSD-3-Clause |
| socket-pktinfo 0.4.2 | mdns-sd's (which network a question came in on) | MIT |
| aes 0.8.4, ctr 0.9.2 | the video's cipher (AES-128 in CTR mode) and pairing's | MIT OR Apache-2.0 |
| cipher 0.4.4, inout 0.1.4 | aes's and ctr's | MIT OR Apache-2.0 |

Already part of SoftPresenter and used as they are: `ring` (Ed25519, X25519, random bytes),
`sha2` (SHA-512), `httparse` (header lines), `socket2` (listening on IPv4 and IPv6 at one port),
`serde_json`, and plist's `base64`, `indexmap` and `time`. No cipher, curve or hash is written
here; the FairPlay port is table lookups and XORs of a fixed, public algorithm that protects
nothing of ours.

Decoding is done by FFmpeg, run as a separate program as it is for videos
([ffmpeg.md](ffmpeg.md)); nothing was changed in it.

## In the app and the packages

The Mac app carries `softpresenter-mirror` in `Contents/MacOS` beside `ffmpeg`, and this note,
the LGPL-2.1 text and the GPL in `Contents/Resources/ScreenMirroring`.

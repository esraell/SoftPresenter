//! The mirrored video's packets: the 128-byte header in front of each, the codec's settings a
//! sender sends at the start and when its picture changes size, and turning a picture's
//! length-prefixed NAL units into what FFmpeg's raw H.264 and HEVC readers take.
//!
//! Every length here comes from a sender and is checked before anything is read or copied.
//!
//! Follows UxPlay's `lib/raop_rtp_mirror.c` (copyright (C) 2019 dsafa22, modified by Florian
//! Draschbacher and F. Duncanh; LGPL-2.1-or-later).

/// The length of the header in front of every packet.
pub const HEADER_LEN: usize = 128;
/// The longest payload a packet may have.
pub const MAX_PAYLOAD: usize = 4 * 1024 * 1024;
/// The longest the codec's settings (parameter sets) may be, each.
pub const MAX_PARAMETER_SET: usize = 4096;
/// The sides a picture may have, as a sender says them.
pub const MIN_SIDE: u32 = 16;
pub const MAX_SIDE: u32 = 8192;

const START_CODE: [u8; 4] = [0, 0, 0, 1];

/// What a packet carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// An encrypted picture.
    Picture,
    /// The codec's settings, in clear.
    Settings,
    /// Anything else (a once-a-second report, an old heartbeat): read and dropped.
    Other(u8),
}

/// The header in front of a packet.
#[derive(Debug, PartialEq)]
pub struct Header {
    pub payload_len: usize,
    pub kind: Kind,
    /// The picture's size as the header says it (only meaningful with settings).
    pub width: f32,
    pub height: f32,
}

/// Reads a packet's header. `None` when the payload it claims is longer than any packet may be.
pub fn header(bytes: &[u8; HEADER_LEN]) -> Option<Header> {
    let payload_len = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;
    if payload_len > MAX_PAYLOAD {
        return None;
    }
    let kind = match bytes[4] {
        0 => Kind::Picture,
        1 => Kind::Settings,
        other => Kind::Other(other),
    };
    let float = |at: usize| f32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]);
    // Bytes 56 to 63 are the picture's size; bytes 16 to 23 the source's, the same in practice.
    let (mut width, mut height) = (float(56), float(60));
    if !side_ok(width) || !side_ok(height) {
        (width, height) = (float(16), float(20));
    }
    Some(Header { payload_len, kind, width, height })
}

fn side_ok(side: f32) -> bool {
    side.is_finite() && side >= MIN_SIDE as f32 && side <= MAX_SIDE as f32
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Codec {
    H264,
    Hevc,
}

impl Codec {
    pub fn name(self) -> &'static str {
        match self {
            Codec::H264 => "h264",
            Codec::Hevc => "hevc",
        }
    }
}

/// The codec's settings from a settings packet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    pub codec: Codec,
    /// The sender's picture, in pixels.
    pub width: u32,
    pub height: u32,
    /// The parameter sets, each behind a start code, ready for FFmpeg.
    pub parameter_sets: Vec<u8>,
}

fn be16(bytes: &[u8], at: usize) -> Option<usize> {
    Some(usize::from(u16::from_be_bytes([*bytes.get(at)?, *bytes.get(at + 1)?])))
}

fn parameter_set(payload: &[u8], at: usize, len: usize) -> Option<&[u8]> {
    if len == 0 || len > MAX_PARAMETER_SET {
        return None;
    }
    payload.get(at..at.checked_add(len)?)
}

/// Reads a settings packet: H.264's SPS and PPS in `avcC` form, or HEVC's VPS, SPS and PPS in
/// `hvc1` form. `None` when it is neither, or its lengths run past its end.
pub fn settings(header: &Header, payload: &[u8]) -> Option<Settings> {
    if !side_ok(header.width) || !side_ok(header.height) {
        return None;
    }
    let (width, height) = (header.width as u32, header.height as u32);
    let mut sets = Vec::new();
    let codec = if payload.get(4..8) == Some(b"hvc1") {
        let mut at = 0x75;
        for marker in [0xa0_u8, 0xa1, 0xa2] {
            if payload.get(at..at + 3)? != [marker, 0x00, 0x01] {
                return None;
            }
            let len = be16(payload, at + 3)?;
            at += 5;
            sets.extend_from_slice(&START_CODE);
            sets.extend_from_slice(parameter_set(payload, at, len)?);
            at += len;
        }
        Codec::Hevc
    } else {
        let sps_len = be16(payload, 6)?;
        let sps = parameter_set(payload, 8, sps_len)?;
        let pps_len = be16(payload, sps_len + 9)?;
        let pps = parameter_set(payload, sps_len + 11, pps_len)?;
        // An H.264 SPS is NAL type 7 and a PPS type 8; anything else is not `avcC`.
        if sps[0] & 0x9f != 7 || pps[0] & 0x9f != 8 {
            return None;
        }
        sets.extend_from_slice(&START_CODE);
        sets.extend_from_slice(sps);
        sets.extend_from_slice(&START_CODE);
        sets.extend_from_slice(pps);
        Codec::H264
    };
    Some(Settings { codec, width, height, parameter_sets: sets })
}

/// The access unit delimiter written after every picture, so FFmpeg's reader knows the picture
/// has ended without waiting for the next one.
fn delimiter(codec: Codec) -> &'static [u8] {
    match codec {
        Codec::H264 => &[0, 0, 0, 1, 0x09, 0xf0],
        Codec::Hevc => &[0, 0, 0, 1, 0x46, 0x01, 0x50],
    }
}

/// Turns a decrypted picture (NAL units, each behind its length as four big-endian bytes) into
/// start-code form in `out`, followed by a delimiter. A sender's own delimiters are left out,
/// so there is exactly one after each picture. `false`, with `out` left empty, when the lengths
/// do not add up or a unit's first bit is set, as when decryption went wrong.
pub fn annex_b(codec: Codec, payload: &[u8], out: &mut Vec<u8>) -> bool {
    out.clear();
    let mut at = 0_usize;
    let mut units = 0;
    while at < payload.len() {
        let Some(len_bytes) = payload.get(at..at + 4) else {
            out.clear();
            return false;
        };
        let len = u32::from_be_bytes([len_bytes[0], len_bytes[1], len_bytes[2], len_bytes[3]]) as usize;
        at += 4;
        let Some(unit) = at.checked_add(len).and_then(|end| payload.get(at..end)) else {
            out.clear();
            return false;
        };
        if len == 0 || unit[0] & 0x80 != 0 {
            out.clear();
            return false;
        }
        let is_delimiter = match codec {
            Codec::H264 => unit[0] & 0x1f == 9,
            Codec::Hevc => (unit[0] >> 1) & 0x3f == 35,
        };
        if !is_delimiter {
            out.extend_from_slice(&START_CODE);
            out.extend_from_slice(unit);
            units += 1;
        }
        at += len;
    }
    if units == 0 {
        out.clear();
        return false;
    }
    out.extend_from_slice(delimiter(codec));
    true
}

/// The size pictures are shown at: the sender's picture fitted inside `max_width` × `max_height`
/// without stretching or enlarging, each side even and at least [`MIN_SIDE`].
pub fn shown_size(width: u32, height: u32, max_width: u32, max_height: u32) -> (u32, u32) {
    let (width, height) = (u64::from(width.max(1)), u64::from(height.max(1)));
    let (max_width, max_height) = (u64::from(max_width), u64::from(max_height));
    let (mut w, mut h) = (width, height);
    if w > max_width || h > max_height {
        if width * max_height > height * max_width {
            (w, h) = (max_width, height * max_width / width);
        } else {
            (w, h) = (width * max_height / height, max_height);
        }
    }
    let even = |side: u64| ((side / 2 * 2).max(u64::from(MIN_SIDE))) as u32;
    (even(w), even(h))
}

#[cfg(test)]
pub mod tests {
    use super::*;

    pub fn header_bytes(payload_len: u32, kind: u8, width: f32, height: f32) -> [u8; HEADER_LEN] {
        let mut bytes = [0_u8; HEADER_LEN];
        bytes[..4].copy_from_slice(&payload_len.to_le_bytes());
        bytes[4] = kind;
        for at in [16, 40, 56] {
            bytes[at..at + 4].copy_from_slice(&width.to_le_bytes());
            bytes[at + 4..at + 8].copy_from_slice(&height.to_le_bytes());
        }
        bytes
    }

    fn avcc(sps: &[u8], pps: &[u8]) -> Vec<u8> {
        let mut out = vec![1, 0x64, 0, 0x28, 0xff, 0xe1];
        out.extend((sps.len() as u16).to_be_bytes());
        out.extend(sps);
        out.push(1);
        out.extend((pps.len() as u16).to_be_bytes());
        out.extend(pps);
        out
    }

    fn hvc1(vps: &[u8], sps: &[u8], pps: &[u8]) -> Vec<u8> {
        let mut out = vec![0_u8; 0x75];
        out[4..8].copy_from_slice(b"hvc1");
        for (marker, set) in [(0xa0, vps), (0xa1, sps), (0xa2, pps)] {
            out.extend([marker, 0, 1]);
            out.extend((set.len() as u16).to_be_bytes());
            out.extend(set);
        }
        out
    }

    #[test]
    fn a_header_says_its_length_kind_and_size() {
        let read = header(&header_bytes(1234, 1, 1920.0, 1080.0)).unwrap();
        assert_eq!(read, Header { payload_len: 1234, kind: Kind::Settings, width: 1920.0, height: 1080.0 });
        assert_eq!(header(&header_bytes(0, 0, 0.0, 0.0)).unwrap().kind, Kind::Picture);
        assert_eq!(header(&header_bytes(25_000, 5, 0.0, 0.0)).unwrap().kind, Kind::Other(5));
        assert_eq!(header(&header_bytes(MAX_PAYLOAD as u32, 0, 0.0, 0.0)).unwrap().payload_len, MAX_PAYLOAD);
    }

    #[test]
    fn a_header_that_claims_a_huge_payload_is_refused() {
        assert!(header(&header_bytes(MAX_PAYLOAD as u32 + 1, 0, 0.0, 0.0)).is_none());
        assert!(header(&header_bytes(u32::MAX, 0, 0.0, 0.0)).is_none());
        assert!(header(&[0xff; HEADER_LEN]).is_none());
    }

    #[test]
    fn h264_settings_give_the_two_parameter_sets_behind_start_codes() {
        let head = header(&header_bytes(0, 1, 1170.0, 2532.0)).unwrap();
        let read = settings(&head, &avcc(&[0x67, 1, 2, 3], &[0x68, 9])).unwrap();
        assert_eq!(read.codec, Codec::H264);
        assert_eq!((read.width, read.height), (1170, 2532));
        assert_eq!(read.parameter_sets, [0, 0, 0, 1, 0x67, 1, 2, 3, 0, 0, 0, 1, 0x68, 9]);
    }

    #[test]
    fn hevc_settings_give_the_three_parameter_sets() {
        let head = header(&header_bytes(0, 1, 1920.0, 1080.0)).unwrap();
        let read = settings(&head, &hvc1(&[0x40, 1], &[0x42, 1, 5], &[0x44, 1])).unwrap();
        assert_eq!(read.codec, Codec::Hevc);
        assert_eq!(read.parameter_sets, [0, 0, 0, 1, 0x40, 1, 0, 0, 0, 1, 0x42, 1, 5, 0, 0, 0, 1, 0x44, 1]);
    }

    #[test]
    fn settings_whose_lengths_run_past_the_end_or_whose_size_is_absurd_are_refused() {
        let head = header(&header_bytes(0, 1, 1920.0, 1080.0)).unwrap();
        let good = avcc(&[0x67, 1, 2, 3], &[0x68, 9]);
        for cut in 0..good.len() {
            assert!(settings(&head, &good[..cut]).is_none(), "cut at {cut}");
        }
        let mut long = good.clone();
        long[6..8].copy_from_slice(&60_000_u16.to_be_bytes());
        assert!(settings(&head, &long).is_none());
        assert!(settings(&head, &avcc(&[0x65, 1], &[0x68, 9])).is_none());
        assert!(settings(&head, &avcc(&[], &[0x68])).is_none());
        let good = hvc1(&[0x40, 1], &[0x42, 1, 5], &[0x44, 1]);
        for cut in 8..good.len() {
            assert!(settings(&head, &good[..cut]).is_none(), "cut at {cut}");
        }
        for (width, height) in [(0.0, 1080.0), (1920.0, f32::NAN), (f32::INFINITY, 1080.0), (-5.0, 9.0), (1e9, 1e9)] {
            let head = header(&header_bytes(0, 1, width, height)).unwrap();
            assert!(settings(&head, &avcc(&[0x67, 1], &[0x68, 9])).is_none(), "{width}x{height}");
        }
    }

    #[test]
    fn a_picture_becomes_start_codes_with_one_delimiter_after_it() {
        let mut payload = Vec::new();
        for unit in [&[0x09_u8, 0xf0][..], &[0x06, 5], &[0x65, 1, 2, 3]] {
            payload.extend((unit.len() as u32).to_be_bytes());
            payload.extend(unit);
        }
        let mut out = vec![9, 9];
        assert!(annex_b(Codec::H264, &payload, &mut out));
        assert_eq!(out, [0, 0, 0, 1, 0x06, 5, 0, 0, 0, 1, 0x65, 1, 2, 3, 0, 0, 0, 1, 0x09, 0xf0]);
        let mut payload = Vec::new();
        for unit in [&[0x46_u8, 0x01, 0x50][..], &[0x26, 1, 7]] {
            payload.extend((unit.len() as u32).to_be_bytes());
            payload.extend(unit);
        }
        assert!(annex_b(Codec::Hevc, &payload, &mut out));
        assert_eq!(out, [0, 0, 0, 1, 0x26, 1, 7, 0, 0, 0, 1, 0x46, 0x01, 0x50]);
    }

    #[test]
    fn a_picture_whose_lengths_do_not_add_up_is_dropped_whole() {
        let mut out = Vec::new();
        for bad in [
            &[0_u8, 0, 0][..],
            &[0, 0, 0, 5, 0x65, 1],
            &[0xff, 0xff, 0xff, 0xff, 0x65],
            &[0, 0, 0, 0],
            &[0, 0, 0, 1, 0x65, 0, 0],
            &[0, 0, 0, 2, 0xe5, 1],
            &[0, 0, 0, 2, 0x09, 0xf0],
            &[],
        ] {
            assert!(!annex_b(Codec::H264, bad, &mut out), "{bad:?}");
            assert!(out.is_empty());
        }
    }

    #[test]
    fn random_packets_never_stop_the_readers() {
        let mut seed = 0xDEAD_BEEF_CAFE_F00D_u64;
        let mut next = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        let mut out = Vec::new();
        for _ in 0..4000 {
            let mut head = [0_u8; HEADER_LEN];
            head.iter_mut().for_each(|byte| *byte = next() as u8);
            if next() % 2 == 0 {
                head[1..4].fill(0);
            }
            let payload: Vec<u8> = (0..(next() % 400) as usize).map(|_| next() as u8).collect();
            if let Some(head) = header(&head) {
                assert!(head.payload_len <= MAX_PAYLOAD);
                if let Some(read) = settings(&head, &payload) {
                    assert!(read.parameter_sets.len() <= 3 * (MAX_PARAMETER_SET + 4));
                }
            }
            let codec = if next() % 2 == 0 { Codec::H264 } else { Codec::Hevc };
            let _ = annex_b(codec, &payload, &mut out);
            assert!(out.len() <= payload.len() + 8);
        }
    }

    #[test]
    fn the_shown_size_fits_inside_the_limit_without_stretching() {
        assert_eq!(shown_size(1920, 1080, 1920, 1080), (1920, 1080));
        assert_eq!(shown_size(3840, 2160, 1920, 1080), (1920, 1080));
        assert_eq!(shown_size(1170, 2532, 1920, 1080), (498, 1080));
        assert_eq!(shown_size(2532, 1170, 1920, 1080), (1920, 886));
        assert_eq!(shown_size(641, 481, 1920, 1080), (640, 480));
        assert_eq!(shown_size(8192, 16, 1920, 1080), (1920, 16));
        assert_eq!(shown_size(0, 0, 1920, 1080), (16, 16));
    }
}

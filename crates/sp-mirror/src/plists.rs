//! The property lists of the control channel: reading the few a sender sends, under limits, and
//! writing the answers.
//!
//! A binary property list can name one object many times over, so a small file can unfold into
//! a very large tree; it is read as a stream of events first, and given up when it is deeper or
//! longer than any list a sender really sends.
//!
//! The names in the lists follow UxPlay's `lib/raop_handlers.h` (copyright (C) 2018 Juho
//! Vähä-Herttua, (C) 2021-2026 F. Duncanh; LGPL-2.1-or-later).

use plist::{Dictionary, Value};

/// How deep a list from a sender may nest.
pub const MAX_DEPTH: usize = 8;
/// How many values a list from a sender may hold, counted as it unfolds.
pub const MAX_EVENTS: usize = 2048;
/// How many bytes of text and data a list from a sender may hold, counted as it unfolds (a list
/// can name one long value many times).
pub const MAX_UNFOLDED: usize = 4 * crate::request::MAX_BODY;

/// Reads a binary property list from a sender. Anything else (XML, too deep, too long, cut
/// short) is `None`.
pub fn parse(bytes: &[u8]) -> Option<Value> {
    if !bytes.starts_with(b"bplist00") || bytes.len() > crate::request::MAX_BODY {
        return None;
    }
    let reader = plist::stream::BinaryReader::new(std::io::Cursor::new(bytes));
    let mut events = Vec::new();
    let mut depth = 0_usize;
    let mut unfolded = 0_usize;
    for event in reader {
        let event = event.ok()?;
        match &event {
            plist::stream::Event::StartArray(_) | plist::stream::Event::StartDictionary(_) => {
                depth += 1;
                if depth > MAX_DEPTH {
                    return None;
                }
            }
            plist::stream::Event::EndCollection => depth = depth.checked_sub(1)?,
            plist::stream::Event::Data(data) => unfolded += data.len(),
            plist::stream::Event::String(text) => unfolded += text.len(),
            _ => {}
        }
        if unfolded > MAX_UNFOLDED {
            return None;
        }
        events.push(Ok(event));
        if events.len() > MAX_EVENTS {
            return None;
        }
    }
    Value::from_events(events).ok()
}

pub fn to_bytes(value: &Value) -> Vec<u8> {
    let mut out = Vec::new();
    // Writing to memory cannot fail.
    let _ = value.to_writer_binary(&mut out);
    out
}

fn uint(value: u64) -> Value {
    Value::Integer(value.into())
}

fn text(value: &str) -> Value {
    Value::String(value.to_owned())
}

/// What the receiver says of itself to `GET /info`.
pub struct Info<'a> {
    pub name: &'a str,
    pub device_id: &'a str,
    pub public_key: &'a [u8; 32],
    pub features: u64,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
}

/// The answer to `GET /info`.
pub fn info(info: &Info<'_>) -> Value {
    let mut root = Dictionary::new();
    root.insert("deviceID".into(), text(info.device_id));
    root.insert("macAddress".into(), text(info.device_id));
    root.insert("pk".into(), Value::Data(info.public_key.to_vec()));
    root.insert("features".into(), uint(info.features));
    root.insert("name".into(), text(info.name));
    root.insert("pi".into(), text(crate::discovery::PI));
    root.insert("vv".into(), uint(2));
    root.insert("statusFlags".into(), uint(68));
    root.insert("keepAliveLowPower".into(), Value::Boolean(true));
    root.insert("sourceVersion".into(), text(crate::discovery::SOURCE_VERSION));
    root.insert("keepAliveSendStatsAsBody".into(), Value::Boolean(true));
    root.insert("model".into(), text(crate::discovery::MODEL));
    root.insert("initialVolume".into(), Value::Real(0.0));
    let mut latencies = Vec::new();
    let mut formats = Vec::new();
    for kind in [100_u64, 101] {
        let mut latency = Dictionary::new();
        latency.insert("type".into(), uint(kind));
        latency.insert("inputLatencyMicros".into(), uint(0));
        latency.insert("audioType".into(), text("default"));
        latency.insert("outputLatencyMicros".into(), Value::Boolean(false));
        latencies.push(Value::Dictionary(latency));
        let mut format = Dictionary::new();
        format.insert("audioOutputFormats".into(), uint(0x3ff_fffc));
        format.insert("type".into(), uint(kind));
        format.insert("audioInputFormats".into(), uint(0x3ff_fffc));
        formats.push(Value::Dictionary(format));
    }
    root.insert("audioLatencies".into(), Value::Array(latencies));
    root.insert("audioFormats".into(), Value::Array(formats));
    // The one display: the sender scales its picture to this, so this is where it is capped.
    let mut display = Dictionary::new();
    display.insert("uuid".into(), text("e0ff8a27-6738-3d56-8a16-cc53aacee925"));
    display.insert("widthPhysical".into(), uint(0));
    display.insert("heightPhysical".into(), uint(0));
    display.insert("width".into(), uint(info.width.into()));
    display.insert("height".into(), uint(info.height.into()));
    display.insert("widthPixels".into(), uint(info.width.into()));
    display.insert("heightPixels".into(), uint(info.height.into()));
    display.insert("rotation".into(), Value::Boolean(false));
    display.insert("refreshRate".into(), Value::Real(1.0 / f64::from(info.fps)));
    display.insert("maxFPS".into(), uint(info.fps.into()));
    display.insert("overscanned".into(), Value::Boolean(false));
    display.insert("features".into(), uint(14));
    root.insert("displays".into(), Value::Array(vec![Value::Dictionary(display)]));
    Value::Dictionary(root)
}

/// The answer to the first `GET /info` of a connection, which asks only for the discovery
/// record (`qualifier` = `txtAirPlay`).
pub fn info_txt(txt: &[u8]) -> Value {
    let mut root = Dictionary::new();
    root.insert("txtAirPlay".into(), Value::Data(txt.to_vec()));
    Value::Dictionary(root)
}

/// True when a `GET /info` body asks for the discovery record only.
pub fn asks_for_txt(body: &Value) -> bool {
    body.as_dictionary()
        .and_then(|root| root.get("qualifier"))
        .and_then(Value::as_array)
        .and_then(|qualifier| qualifier.first())
        .and_then(Value::as_string)
        == Some("txtAirPlay")
}

/// What the first `SETUP` of a session brings.
#[derive(Debug, PartialEq, Eq)]
pub struct SessionSetup {
    pub ekey: [u8; crate::fairplay::EKEY_LEN],
    pub name: String,
    pub model: String,
    pub timing_port: u16,
}

/// One stream a later `SETUP` asks for.
#[derive(Debug, PartialEq, Eq)]
pub enum Stream {
    /// The mirrored video (type 110), with the number its key is made from.
    Video {
        connection_id: u64,
    },
    /// The sound beside it (type 96).
    Sound,
    Other(u64),
}

/// What a `SETUP` body holds: the session's keys, the streams asked for, or both.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Setup {
    pub session: Option<SessionSetup>,
    /// The keys were there but not of the right lengths.
    pub bad_keys: bool,
    pub streams: Vec<Stream>,
}

/// How many streams one `SETUP` may ask for.
pub const MAX_STREAMS: usize = 4;

pub fn setup(body: &Value) -> Option<Setup> {
    let root = body.as_dictionary()?;
    let mut setup = Setup::default();
    if let (Some(ekey), Some(eiv)) =
        (root.get("ekey").and_then(Value::as_data), root.get("eiv").and_then(Value::as_data))
    {
        match <[u8; crate::fairplay::EKEY_LEN]>::try_from(ekey) {
            Ok(ekey) if eiv.len() == 16 => {
                let words = |key: &str| root.get(key).and_then(Value::as_string).unwrap_or_default().to_owned();
                let timing_port = root
                    .get("timingPort")
                    .and_then(Value::as_unsigned_integer)
                    .and_then(|port| u16::try_from(port).ok())
                    .unwrap_or(0);
                setup.session = Some(SessionSetup { ekey, name: words("name"), model: words("model"), timing_port });
            }
            _ => setup.bad_keys = true,
        }
    }
    if let Some(streams) = root.get("streams").and_then(Value::as_array) {
        for stream in streams.iter().take(MAX_STREAMS) {
            let Some(stream) = stream.as_dictionary() else { continue };
            let kind = stream.get("type").and_then(Value::as_unsigned_integer).unwrap_or(0);
            setup.streams.push(match kind {
                110 => Stream::Video {
                    connection_id: stream
                        .get("streamConnectionID")
                        .and_then(|id| id.as_unsigned_integer().or_else(|| id.as_signed_integer().map(|id| id as u64)))
                        .unwrap_or(0),
                },
                96 => Stream::Sound,
                other => Stream::Other(other),
            });
        }
    }
    Some(setup)
}

/// The ports the receiver gives in answer to a `SETUP`.
pub struct Ports {
    pub timing: u16,
    pub video: u16,
    pub sound_data: u16,
    pub sound_control: u16,
}

/// The answer to a `SETUP`.
pub fn setup_answer(setup: &Setup, ports: &Ports) -> Value {
    let mut root = Dictionary::new();
    if setup.session.is_some() {
        root.insert("timingPort".into(), uint(ports.timing.into()));
        // The event port is not used in mirroring.
        root.insert("eventPort".into(), uint(0));
    }
    if !setup.streams.is_empty() {
        let mut streams = Vec::new();
        for stream in &setup.streams {
            let mut answer = Dictionary::new();
            match stream {
                Stream::Video { .. } => {
                    answer.insert("dataPort".into(), uint(ports.video.into()));
                    answer.insert("type".into(), uint(110));
                }
                Stream::Sound => {
                    answer.insert("dataPort".into(), uint(ports.sound_data.into()));
                    answer.insert("controlPort".into(), uint(ports.sound_control.into()));
                    answer.insert("type".into(), uint(96));
                }
                Stream::Other(_) => continue,
            }
            streams.push(Value::Dictionary(answer));
        }
        root.insert("streams".into(), Value::Array(streams));
    }
    Value::Dictionary(root)
}

/// Which streams a `TEARDOWN` names: `(sound, video)`. Neither means everything.
pub fn teardown(body: &[u8]) -> (bool, bool) {
    let mut named = (false, false);
    if let Some(streams) = parse(body)
        .as_ref()
        .and_then(Value::as_dictionary)
        .and_then(|root| root.get("streams"))
        .and_then(Value::as_array)
    {
        for stream in streams.iter().take(MAX_STREAMS) {
            match stream.as_dictionary().and_then(|s| s.get("type")).and_then(Value::as_unsigned_integer) {
                Some(96) => named.0 = true,
                Some(110) => named.1 = true,
                _ => {}
            }
        }
    }
    named
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dict(pairs: Vec<(&str, Value)>) -> Value {
        let mut dictionary = Dictionary::new();
        for (key, value) in pairs {
            dictionary.insert(key.into(), value);
        }
        Value::Dictionary(dictionary)
    }

    #[test]
    fn the_first_setup_brings_the_key_the_name_and_the_timing_port() {
        let body = to_bytes(&dict(vec![
            ("ekey", Value::Data(vec![7; 72])),
            ("eiv", Value::Data(vec![1; 16])),
            ("name", text("Anna's iPhone")),
            ("model", text("iPhone15,2")),
            ("timingPort", uint(51234)),
            ("timingProtocol", text("NTP")),
        ]));
        let setup = setup(&parse(&body).unwrap()).unwrap();
        let session = setup.session.unwrap();
        assert_eq!(session.ekey, [7; 72]);
        assert_eq!(session.name, "Anna's iPhone");
        assert_eq!(session.model, "iPhone15,2");
        assert_eq!(session.timing_port, 51234);
        assert!(setup.streams.is_empty());
    }

    #[test]
    fn keys_of_the_wrong_length_are_said_to_be_bad() {
        for (ekey, eiv) in [(71, 16), (72, 15), (0, 0), (4000, 16)] {
            let body = to_bytes(&dict(vec![("ekey", Value::Data(vec![7; ekey])), ("eiv", Value::Data(vec![1; eiv]))]));
            let setup = setup(&parse(&body).unwrap()).unwrap();
            assert!(setup.bad_keys && setup.session.is_none());
        }
    }

    #[test]
    fn the_second_setup_names_the_streams_and_the_answer_gives_each_its_ports() {
        let body = to_bytes(&dict(vec![(
            "streams",
            Value::Array(vec![
                dict(vec![("type", uint(110)), ("streamConnectionID", Value::Integer((-3_i64).into()))]),
                dict(vec![("type", uint(96)), ("ct", uint(8)), ("spf", uint(480))]),
                dict(vec![("type", uint(5))]),
                text("not a stream"),
            ]),
        )]));
        let setup = setup(&parse(&body).unwrap()).unwrap();
        assert_eq!(setup.streams, [Stream::Video { connection_id: (-3_i64) as u64 }, Stream::Sound, Stream::Other(5)]);
        let answer = setup_answer(&setup, &Ports { timing: 7102, video: 7101, sound_data: 7103, sound_control: 7104 });
        let streams = answer.as_dictionary().unwrap().get("streams").unwrap().as_array().unwrap();
        assert_eq!(streams.len(), 2);
        let video = streams[0].as_dictionary().unwrap();
        assert_eq!(video.get("dataPort").unwrap().as_unsigned_integer(), Some(7101));
        assert_eq!(video.get("type").unwrap().as_unsigned_integer(), Some(110));
        let sound = streams[1].as_dictionary().unwrap();
        assert_eq!(sound.get("controlPort").unwrap().as_unsigned_integer(), Some(7104));
        assert!(answer.as_dictionary().unwrap().get("timingPort").is_none());
    }

    #[test]
    fn teardown_names_its_streams_or_none() {
        let one = to_bytes(&dict(vec![("streams", Value::Array(vec![dict(vec![("type", uint(110))])]))]));
        assert_eq!(teardown(&one), (false, true));
        assert_eq!(teardown(&to_bytes(&dict(vec![]))), (false, false));
        assert_eq!(teardown(b""), (false, false));
        assert_eq!(teardown(b"bplist00 rubbish"), (false, false));
    }

    #[test]
    fn info_says_the_size_asked_for_and_reads_back() {
        let value = info(&Info {
            name: "SoftPresenter",
            device_id: "AA:BB:CC:DD:EE:FF",
            public_key: &[9; 32],
            features: crate::discovery::features(crate::discovery::FEATURES_2),
            width: 1920,
            height: 1080,
            fps: 30,
        });
        let back = parse(&to_bytes(&value)).unwrap();
        let root = back.as_dictionary().unwrap();
        assert_eq!(
            root.get("features").unwrap().as_unsigned_integer(),
            Some(crate::discovery::features(crate::discovery::FEATURES_2))
        );
        let display = root.get("displays").unwrap().as_array().unwrap()[0].as_dictionary().unwrap();
        assert_eq!(display.get("widthPixels").unwrap().as_unsigned_integer(), Some(1920));
        assert_eq!(display.get("maxFPS").unwrap().as_unsigned_integer(), Some(30));
        assert!(asks_for_txt(&dict(vec![("qualifier", Value::Array(vec![text("txtAirPlay")]))])));
        assert!(!asks_for_txt(&dict(vec![("qualifier", text("txtAirPlay"))])));
    }

    #[test]
    fn xml_and_anything_not_a_binary_list_is_refused() {
        assert!(parse(b"<?xml version=\"1.0\"?><plist><dict/></plist>").is_none());
        assert!(parse(b"").is_none());
        assert!(parse(b"bplist00").is_none());
        assert!(parse(b"bplist01\x00\x00").is_none());
        assert!(parse(&vec![b'b'; 70_000]).is_none());
    }

    /// A list nested deeper than any sender's is refused, however small it is.
    #[test]
    fn a_list_nested_too_deep_is_refused() {
        let mut value = uint(1);
        for _ in 0..MAX_DEPTH {
            value = Value::Array(vec![value]);
        }
        assert!(parse(&to_bytes(&value)).is_some());
        let mut value = uint(1);
        for _ in 0..200 {
            value = Value::Array(vec![value]);
        }
        assert!(parse(&to_bytes(&value)).is_none());
    }

    /// A hand-made binary list of `levels` arrays, each naming the next one `fan` times: a few
    /// hundred bytes that unfold into `fan`^`levels` values.
    fn fan_out(levels: usize, fan: usize) -> Vec<u8> {
        let mut out = b"bplist00".to_vec();
        let mut offsets = Vec::new();
        for level in 0..levels {
            offsets.push(out.len());
            out.push(0xa0 | fan as u8);
            out.extend(std::iter::repeat_n((level + 1) as u8, fan));
        }
        offsets.push(out.len());
        out.push(0x09);
        let table = out.len();
        out.extend(offsets.iter().map(|offset| *offset as u8));
        out.extend([0; 6]);
        out.extend([1, 1]);
        out.extend((offsets.len() as u64).to_be_bytes());
        out.extend(0_u64.to_be_bytes());
        out.extend((table as u64).to_be_bytes());
        out
    }

    #[test]
    fn a_small_list_that_unfolds_into_a_huge_one_is_given_up() {
        // Small enough to read whole: the hand-made list is a real one.
        let small = parse(&fan_out(2, 3)).unwrap();
        assert_eq!(small.as_array().unwrap().len(), 3);
        assert_eq!(small.as_array().unwrap()[2].as_array().unwrap()[0].as_boolean(), Some(true));
        // 14^7 is over a hundred million values, from 120 bytes.
        let bomb = fan_out(7, 14);
        assert!(bomb.len() < 200);
        let started = std::time::Instant::now();
        assert!(parse(&bomb).is_none());
        assert!(started.elapsed() < std::time::Duration::from_secs(2));
    }

    /// One long value named two thousand times: 40 KB that would unfold into 80 MB.
    #[test]
    fn a_list_that_names_one_long_value_many_times_is_given_up() {
        let mut out = b"bplist00".to_vec();
        // Object 0: an array of 2000 references (two bytes each) to object 1.
        let array = out.len();
        out.extend([0xaf, 0x11, 0x07, 0xd0]);
        for _ in 0..2000 {
            out.extend([0x00, 0x01]);
        }
        // Object 1: 40,000 bytes of data.
        let data = out.len();
        out.extend([0x4f, 0x11, 0x9c, 0x40]);
        out.extend(std::iter::repeat_n(7_u8, 40_000));
        let table = out.len();
        out.extend((array as u32).to_be_bytes());
        out.extend((data as u32).to_be_bytes());
        out.extend([0; 6]);
        out.extend([4, 2]);
        out.extend(2_u64.to_be_bytes());
        out.extend(0_u64.to_be_bytes());
        out.extend((table as u64).to_be_bytes());
        assert!(out.len() < crate::request::MAX_BODY);
        assert!(parse(&out).is_none());
        // The same shape with a short value is a real list: the bytes above are well formed.
        let mut small = b"bplist00".to_vec();
        small.extend([0xa2, 0x01, 0x01]);
        small.extend([0x42, 7, 7]);
        small.extend([8, 11]);
        small.extend([0; 6]);
        small.extend([1, 1]);
        small.extend(2_u64.to_be_bytes());
        small.extend(0_u64.to_be_bytes());
        small.extend(14_u64.to_be_bytes());
        let read = parse(&small).unwrap();
        assert_eq!(read.as_array().unwrap()[1].as_data(), Some(&[7_u8, 7][..]));
    }

    #[test]
    fn a_list_that_names_itself_is_refused() {
        // One array whose only member is itself.
        let mut out = b"bplist00".to_vec();
        out.extend([0xa1, 0x00]);
        out.push(8);
        out.extend([0; 6]);
        out.extend([1, 1]);
        out.extend(1_u64.to_be_bytes());
        out.extend(0_u64.to_be_bytes());
        out.extend(10_u64.to_be_bytes());
        assert!(parse(&out).is_none());
    }

    /// A real list with bytes changed or cut short is a list or nothing, never a panic.
    #[test]
    fn damaged_lists_never_stop_the_reader() {
        let good = to_bytes(&dict(vec![
            ("ekey", Value::Data(vec![7; 72])),
            ("eiv", Value::Data(vec![1; 16])),
            ("name", text("Anna's iPhone")),
            ("streams", Value::Array(vec![dict(vec![("type", uint(110)), ("streamConnectionID", uint(u64::MAX))])])),
        ]));
        let mut seed = 0x1234_5678_9ABC_DEF1_u64;
        let mut next = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        for _ in 0..5000 {
            let mut bytes = good.clone();
            for _ in 0..=(next() % 6) {
                let at = 8 + (next() % (bytes.len() as u64 - 8)) as usize;
                bytes[at] = next() as u8;
            }
            if next() % 3 == 0 {
                bytes.truncate((next() % bytes.len() as u64) as usize);
            }
            if let Some(value) = parse(&bytes) {
                let _ = setup(&value);
            }
            let _ = teardown(&bytes);
        }
    }
}

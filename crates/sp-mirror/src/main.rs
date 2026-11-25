//! `softpresenter-mirror`: receives an iPhone, iPad or Mac screen for SoftPresenter.
//!
//! Someone picks SoftPresenter in Screen Mirroring (AirPlay) on their device; this program
//! speaks to the device, decodes its picture with FFmpeg, and hands the pictures to
//! SoftPresenter, which started it. It is **unofficial**: it is not made, licensed or supported
//! by Apple, and an Apple update may stop it working.
//!
//! It is a separate program, not part of SoftPresenter itself, for two reasons. Everything a
//! stranger on the network can reach is here, in safe Rust, away from the program that draws the
//! screens. And it carries a port of playfair (GPL-3.0, [`fairplay`]), whose licence does not
//! carry SoftPresenter's additional permission for NDI and DeckLink; this program uses neither.
//! It follows UxPlay's `lib/` (LGPL-2.1-or-later), written again in Rust; each module says what
//! it follows, and `docs/licences/screen-mirroring.md` has the notices.
//!
//! # How SoftPresenter talks to it
//!
//! ```text
//! softpresenter-mirror --ffmpeg <path> [--announce] [--port <n>]
//! ```
//!
//! Without `--announce` it listens on 127.0.0.1 only and announces nothing (test runs). With
//! it, it listens on every interface and announces itself to the local network. `--port` is the
//! control port (7100 if not given; 0 for any free one); the video port is the next one.
//!
//! - **Standard input**, a JSON object on each line. First
//!   `{"hello":{"name":"…","device_id":"AA:BB:CC:DD:EE:FF","ask":true}}`; then `{"allow":3}`,
//!   `{"deny":3}` (the operator's answer to a sender that asked) and `{"stop":true}` (end the
//!   session). When standard input closes the program stops, so it never outlives SoftPresenter.
//! - **Standard error**, a JSON object on each line: `listening`, `asks`, `connected`,
//!   `picture_size`, `stopped`, `refused`, `log`, `problem` ([`events`]).
//! - **Standard output**, pictures only: 16 bytes (`SPMP`, width, height, session; little-endian)
//!   and then `width × height × 4` bytes of RGBA. Only the newest picture is ever waiting.
//!
//! No key, and nothing secret, crosses any of these or is written to the log.

mod decode;
mod discovery;
mod events;
mod fairplay;
mod keys;
mod plists;
mod request;
mod server;
mod stream;

use std::io::Read;
use std::path::PathBuf;

/// The longest line read from standard input.
const MAX_LINE: u64 = 4096;

/// What the command line asks for.
#[derive(Debug, PartialEq, Eq)]
struct Arguments {
    ffmpeg: PathBuf,
    announce: bool,
    port: u16,
}

fn arguments(mut words: impl Iterator<Item = String>) -> Result<Arguments, String> {
    let mut ffmpeg = None;
    let mut announce = false;
    let mut port = 7100;
    while let Some(word) = words.next() {
        match word.as_str() {
            "--ffmpeg" => ffmpeg = Some(PathBuf::from(words.next().ok_or("--ffmpeg needs a path")?)),
            "--announce" => announce = true,
            "--port" => {
                port = words.next().and_then(|n| n.parse().ok()).ok_or("--port needs a number from 0 to 65535")?;
            }
            other => return Err(format!("unknown argument {other}")),
        }
    }
    Ok(Arguments { ffmpeg: ffmpeg.ok_or("--ffmpeg <path> is needed")?, announce, port })
}

/// What SoftPresenter says first.
#[derive(Debug, PartialEq, Eq)]
struct Hello {
    name: String,
    device_id: String,
    ask: bool,
}

fn hello(line: &str) -> Result<Hello, String> {
    let value: serde_json::Value = serde_json::from_str(line).map_err(|_| "the first line is not JSON")?;
    let hello = value.get("hello").ok_or("the first line is not a hello")?;
    let name = events::clean(hello.get("name").and_then(|name| name.as_str()).unwrap_or_default(), "SoftPresenter");
    let name = discovery::announced_name(&name).to_owned();
    let device_id = hello.get("device_id").and_then(|id| id.as_str()).unwrap_or_default().to_uppercase();
    if !discovery::valid_device_id(&device_id) {
        return Err("the device id is not six bytes in hex".to_owned());
    }
    // Asking is the safe way: only a plain `false` switches it off.
    let ask = hello.get("ask").and_then(serde_json::Value::as_bool) != Some(false);
    Ok(Hello { name, device_id, ask })
}

/// A command from SoftPresenter.
#[derive(Debug, PartialEq, Eq)]
enum Command {
    Answer { id: u32, allow: bool },
    Stop,
}

fn command(line: &str) -> Option<Command> {
    let value: serde_json::Value = serde_json::from_str(line).ok()?;
    let id = |key: &str| value.get(key).and_then(serde_json::Value::as_u64).and_then(|id| u32::try_from(id).ok());
    if let Some(id) = id("allow") {
        return Some(Command::Answer { id, allow: true });
    }
    if let Some(id) = id("deny") {
        return Some(Command::Answer { id, allow: false });
    }
    (value.get("stop").and_then(serde_json::Value::as_bool) == Some(true)).then_some(Command::Stop)
}

/// The next line of standard input, at most [`MAX_LINE`] bytes of it. `None` at its end.
fn read_line(input: &mut impl std::io::BufRead) -> Option<String> {
    let mut line = Vec::new();
    match std::io::BufRead::read_until(&mut input.by_ref().take(MAX_LINE), b'\n', &mut line) {
        Ok(0) | Err(_) => None,
        Ok(_) => Some(String::from_utf8_lossy(&line).trim().to_owned()),
    }
}

fn main() -> std::process::ExitCode {
    let arguments = match arguments(std::env::args().skip(1)) {
        Ok(arguments) => arguments,
        Err(problem) => {
            eprintln!("softpresenter-mirror: {problem}");
            eprintln!("usage: softpresenter-mirror --ffmpeg <path> [--announce] [--port <n>]");
            eprintln!("SoftPresenter starts this program; it is not meant to be run by hand.");
            return std::process::ExitCode::from(2);
        }
    };
    let mut input = std::io::stdin().lock();
    let hello = match read_line(&mut input)
        .ok_or_else(|| "nothing came on standard input".to_owned())
        .and_then(|line| hello(&line))
    {
        Ok(hello) => hello,
        Err(problem) => {
            events::problem(format!("softpresenter-mirror could not start: {problem}"));
            return std::process::ExitCode::from(2);
        }
    };
    let Some(identity) = keys::Identity::new() else {
        events::problem("softpresenter-mirror could not make its key");
        return std::process::ExitCode::FAILURE;
    };
    let (listeners, control_port, video_port) = match server::bind(arguments.announce, arguments.port) {
        Ok(bound) => bound,
        Err(error) => {
            events::problem(format!("The ports for phone screens could not be opened ({:?}).", error.kind()));
            return std::process::ExitCode::FAILURE;
        }
    };
    // Testing aid: SOFTPRESENTER_MIRROR_MODEL stands in for the model the receiver says it is,
    // to find out with a real phone whether a name of our own is accepted.
    let model = std::env::var("SOFTPRESENTER_MIRROR_MODEL")
        .ok()
        .filter(|model| {
            !model.is_empty() && model.len() <= 32 && model.bytes().all(|b| b.is_ascii_alphanumeric() || b == b',')
        })
        .unwrap_or_else(|| discovery::MODEL.to_owned());
    // Testing aid: SOFTPRESENTER_MIRROR_H264_ONLY=1 leaves out the feature bit that lets a
    // sender choose HEVC, to tell a codec problem from a protocol one.
    let features_2 = if std::env::var_os("SOFTPRESENTER_MIRROR_H264_ONLY").is_some_and(|value| value == "1") {
        discovery::FEATURES_2_H264_ONLY
    } else {
        discovery::FEATURES_2
    };
    let config = server::Config {
        name: hello.name,
        device_id: hello.device_id,
        ask: hello.ask,
        ffmpeg: arguments.ffmpeg,
        model,
        features_2,
    };
    let shared = server::Shared::new(config, identity, control_port, video_port);
    let pictures = shared.pictures.clone();
    std::thread::spawn(move || {
        pictures.write_to(std::io::stdout().lock());
        // SoftPresenter no longer takes pictures. Its end of standard input closes with it, and
        // the program leaves from there, by the one way out that takes the announcement back.
        events::log("standard output closed; no more pictures are written");
    });
    server::serve(&shared, listeners);
    let announced = if arguments.announce {
        match discovery::announce(&shared.record(), control_port) {
            Ok(announced) => Some(announced),
            Err(error) => {
                events::problem(format!(
                    "Phones cannot find SoftPresenter: announcing on the network did not work ({}).",
                    events::clean(&error, "unknown")
                ));
                None
            }
        }
    } else {
        None
    };
    events::say(serde_json::json!({
        "event": "listening", "port": control_port, "video_port": video_port, "announced": announced.is_some(),
    }));
    events::log(format!(
        "listening on port {control_port} (video {video_port}), {}; asking the operator: {}; model {}",
        if announced.is_some() { "announced to the network" } else { "this computer only, not announced" },
        shared.asks(),
        shared.model(),
    ));
    while let Some(line) = read_line(&mut input) {
        match command(&line) {
            Some(Command::Answer { id, allow }) => shared.answer(id, allow),
            Some(Command::Stop) => shared.stop(),
            None => events::log("a line from SoftPresenter was not understood"),
        }
    }
    // Standard input closed: SoftPresenter has stopped, or wants this to.
    shared.stop();
    drop(announced);
    std::process::ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(line: &str) -> impl Iterator<Item = String> {
        line.split_whitespace().map(str::to_owned).collect::<Vec<_>>().into_iter()
    }

    #[test]
    fn the_command_line_never_announces_unless_asked() {
        assert_eq!(
            arguments(words("--ffmpeg /x/ffmpeg")),
            Ok(Arguments { ffmpeg: PathBuf::from("/x/ffmpeg"), announce: false, port: 7100 })
        );
        assert_eq!(
            arguments(words("--port 0 --announce --ffmpeg f")),
            Ok(Arguments { ffmpeg: PathBuf::from("f"), announce: true, port: 0 })
        );
        assert!(arguments(words("")).is_err());
        assert!(arguments(words("--ffmpeg")).is_err());
        assert!(arguments(words("--ffmpeg f --port 70000")).is_err());
        assert!(arguments(words("--ffmpeg f --name x")).is_err());
    }

    #[test]
    fn the_hello_gives_a_clean_name_a_checked_id_and_asks_unless_told_not_to() {
        let read =
            hello(r#"{"hello":{"name":"SoftPresenter — Front Mac","device_id":"aa:bb:cc:dd:ee:0f","ask":false}}"#);
        assert_eq!(
            read,
            Ok(Hello {
                name: "SoftPresenter \u{2014} Front Mac".into(),
                device_id: "AA:BB:CC:DD:EE:0F".into(),
                ask: false
            })
        );
        let read = hello(r#"{"hello":{"name":"\u0007","device_id":"AA:BB:CC:DD:EE:FF","ask":"no"}}"#).unwrap();
        assert_eq!(read.name, "SoftPresenter");
        assert!(read.ask);
        assert!(hello(r#"{"hello":{"name":"x","device_id":"AA:BB:CC:DD:EE:FF"}}"#).unwrap().ask);
        assert!(hello(r#"{"hello":{"name":"x","device_id":"nonsense"}}"#).is_err());
        assert!(hello(r#"{"allow":1}"#).is_err());
        assert!(hello("not json").is_err());
        let long = format!(r#"{{"hello":{{"name":"{}","device_id":"AA:BB:CC:DD:EE:FF"}}}}"#, "n".repeat(300));
        assert_eq!(hello(&long).unwrap().name.len(), discovery::MAX_NAME_BYTES);
    }

    #[test]
    fn commands_are_allow_deny_and_stop_and_nothing_else() {
        assert_eq!(command(r#"{"allow":3}"#), Some(Command::Answer { id: 3, allow: true }));
        assert_eq!(command(r#"{"deny":4}"#), Some(Command::Answer { id: 4, allow: false }));
        assert_eq!(command(r#"{"stop":true}"#), Some(Command::Stop));
        for bad in
            [r#"{"stop":false}"#, r#"{"allow":-1}"#, r#"{"allow":"3"}"#, r#"{"allow":99999999999}"#, "{}", "", "x"]
        {
            assert_eq!(command(bad), None, "{bad}");
        }
    }

    #[test]
    fn a_line_is_read_up_to_a_limit() {
        let mut input = std::io::Cursor::new(b"one\ntwo\n".to_vec());
        assert_eq!(read_line(&mut input).as_deref(), Some("one"));
        assert_eq!(read_line(&mut input).as_deref(), Some("two"));
        assert_eq!(read_line(&mut input), None);
        let mut long = std::io::Cursor::new(vec![b'a'; 10_000]);
        assert_eq!(read_line(&mut long).map(|line| line.len()), Some(MAX_LINE as usize));
    }
}

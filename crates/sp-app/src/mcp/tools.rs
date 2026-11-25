//! The tools `sp mcp` offers an assistant: each one or a few `sp` commands, sent to the running
//! app marked as the tool's (`McpCall`). The app checks each against its switches and
//! `policy` again, whatever this side decided.
//!
//! A tool that changes the screens or takes something out is first *planned*: what it names is
//! found exactly (a song's id, a playlist item's uid, the slide on screen now), the question is
//! worded from what the app found, never from the assistant's own words, and the plan carries
//! a *binding*, which the question's answer is tied to; the commands carried out are those of a
//! plan made again after the yes, with the same binding.

use serde_json::{Map, Value, json};

use crate::control::{McpCall, Reply};

/// What every tool answers while the app isn't running.
pub const NOT_OPEN: &str = "SoftPresenter isn't open, so nothing can be read or changed. Open it on this computer, then try again (an assistant never opens it).";

/// The way to the running app: one command, marked as a tool's.
pub trait App {
    /// Sends the command; `Err` when the app can't be reached (not open, or not answering).
    fn send(&self, args: &[String], call: &McpCall) -> Result<Reply, String>;
}

/// The running app, through the control socket `sp` uses.
pub struct Socket;

impl App for Socket {
    fn send(&self, args: &[String], call: &McpCall) -> Result<Reply, String> {
        crate::control::send_as(crate::control::socket_path, args, Some(call)).map_err(|problem| {
            if problem.starts_with("SoftPresenter isn't running") { NOT_OPEN.to_owned() } else { problem }
        })
    }
}

/// What a tool needs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Need {
    /// Reads, and changes nothing the operator sees.
    Reads,
    /// Makes songs and playlists, or adds to a playlist.
    Prepares,
    /// A download that can take a while (a task, where the assistant offers them).
    Long,
    /// Takes something out, or changes the screens: the person's yes first.
    Asks,
}

/// A tool, as `tools/list` describes it.
pub struct Tool {
    pub name: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    pub need: Need,
    /// Changes what the audience screens show (needs the second switch too).
    pub screens: bool,
    /// Calling it again with the same arguments does no more.
    pub idempotent: bool,
    schema: fn() -> Value,
}

/// An object schema with these properties, those named required, nothing else allowed.
fn object(properties: Value, required: &[&str]) -> Value {
    json!({"type": "object", "properties": properties, "required": required, "additionalProperties": false})
}

fn nothing() -> Value {
    json!({"type": "object", "properties": {}, "additionalProperties": false})
}

const SONG: &str = "The song: its id (from search_songs or read_playlist; best), or its title (in full or in part; a name that fits several songs is refused, never guessed).";

pub const TOOLS: &[Tool] = &[
    Tool {
        name: "get_status",
        title: "What is on the screens",
        description: "What SoftPresenter's audience screens show now (the song or passage, the slide and its words, the media behind it, any message or props), the next slide, what is playing, each screen and what it is doing, and the playlist in front. Look here before changing anything on the screens.",
        need: Need::Reads,
        screens: false,
        idempotent: true,
        schema: nothing,
    },
    Tool {
        name: "search_songs",
        title: "Find songs",
        description: "Songs in the library whose title or words hold the words given (in any case), each with its id and the slides the words are on. With no words, the library's songs in order. Name songs by these ids in the other tools.",
        need: Need::Reads,
        screens: false,
        idempotent: true,
        schema: || {
            object(
                json!({
                    "query": {"type": "string", "description": "Words of the title or of a line, such as \"amazing grace\" or \"how sweet the sound\". Leave out to list the library."},
                    "limit": {"type": "integer", "minimum": 1, "maximum": 200, "description": "How many to give at most (50 unless told)."}
                }),
                &[],
            )
        },
    },
    Tool {
        name: "read_song",
        title: "Read a song",
        description: "A song's words as song text (each section in square brackets, a blank line between slides, its translation under a line of --), with its id, title and details (artist, author, CCLI number).",
        need: Need::Reads,
        screens: false,
        idempotent: true,
        schema: || object(json!({"song": {"type": "string", "description": SONG}}), &["song"]),
    },
    Tool {
        name: "look_up_bible",
        title: "Look up a Bible passage",
        description: "A passage's verses as SoftPresenter would show them, in the translations its Bible page uses (often two), slide by slide. Leaves the Bible page as the operator has it.",
        need: Need::Reads,
        screens: false,
        idempotent: true,
        schema: || {
            object(
                json!({"reference": {"type": "string", "description": "As typed on the Bible page: John 3:16-18, Psalm 23 (a whole chapter), 1 Corinthians 13:4-7; Russian and Ukrainian book names work too. English and Russian Bibles number some chapters (the Psalms) differently: type it in the language its numbering comes from."}}),
                &["reference"],
            )
        },
    },
    Tool {
        name: "search_bible",
        title: "Search the Bible",
        description: "Verses holding the words given, in the Bibles in use written in the words' alphabet. Leaves the Bible page as the operator has it.",
        need: Need::Reads,
        screens: false,
        idempotent: true,
        schema: || {
            object(
                json!({"words": {"type": "string", "description": "The words to find, such as \"valley of the shadow\"."}}),
                &["words"],
            )
        },
    },
    Tool {
        name: "list_bibles",
        title: "List the Bibles",
        description: "The Bibles installed in SoftPresenter, and those it can download (install_bible takes their short names).",
        need: Need::Reads,
        screens: false,
        idempotent: true,
        schema: nothing,
    },
    Tool {
        name: "list_playlists",
        title: "List the playlists",
        description: "The playlists (each service is one), and which is in front in SoftPresenter's window.",
        need: Need::Reads,
        screens: false,
        idempotent: true,
        schema: nothing,
    },
    Tool {
        name: "read_playlist",
        title: "Read a playlist",
        description: "A playlist's items in order: each one's number, kind (song, passage, header, media, media playlist, placeholder), title, its own uid, and a song's id. The playlist in front when none is named.",
        need: Need::Reads,
        screens: false,
        idempotent: true,
        schema: || {
            object(
                json!({"playlist": {"type": "string", "description": "The playlist's name (in full or in part) or its number in list_playlists. Leave out for the one in front."}}),
                &[],
            )
        },
    },
    Tool {
        name: "list_media",
        title: "List the media",
        description: "The pictures and videos in SoftPresenter's media bin, which add_to_playlist can put in a playlist by name.",
        need: Need::Reads,
        screens: false,
        idempotent: true,
        schema: nothing,
    },
    Tool {
        name: "list_messages",
        title: "List the messages",
        description: "The messages kept in SoftPresenter for showing over the slides (a parking notice, a nursery call), which show_message shows by name.",
        need: Need::Reads,
        screens: false,
        idempotent: true,
        schema: nothing,
    },
    Tool {
        name: "create_song",
        title: "Make a song from lyrics",
        description: "Makes a new song in the library from its words: its sections in order (Verse 1, Chorus, Bridge…), each made of slides of a few lines, and, for a song shown in two languages, each slide's translation line for line. Only words you were given: never lyrics from memory. Changes nothing on the screens.",
        need: Need::Prepares,
        screens: false,
        idempotent: false,
        schema: || {
            object(
                json!({
                    "title": {"type": "string", "description": "The song's title. Check first with search_songs that no song has it."},
                    "sections": {
                        "type": "array",
                        "minItems": 1,
                        "description": "The sections in the order they are sung. A chorus sung after every verse is given again each time.",
                        "items": {
                            "type": "object",
                            "properties": {
                                "name": {"type": "string", "description": "Verse 1 to Verse 6, Chorus (or Chorus 1 to 3), Bridge, Pre-chorus, Tag, Intro, Ending: these get jump keys."},
                                "slides": {
                                    "type": "array",
                                    "minItems": 1,
                                    "items": {
                                        "type": "object",
                                        "properties": {
                                            "lines": {"type": "array", "items": {"type": "string"}, "minItems": 1, "description": "The slide's lines in the main language: two is usual, four at most."},
                                            "translation": {"type": "array", "items": {"type": "string"}, "description": "The same lines in the second language, for a song shown in two. Leave out otherwise."}
                                        },
                                        "required": ["lines"],
                                        "additionalProperties": false
                                    }
                                }
                            },
                            "required": ["name", "slides"],
                            "additionalProperties": false
                        }
                    }
                }),
                &["title", "sections"],
            )
        },
    },
    Tool {
        name: "create_playlist",
        title: "Make a playlist",
        description: "Makes a new, empty playlist (a service), such as \"Sunday 24 November\". Changes nothing on the screens.",
        need: Need::Prepares,
        screens: false,
        idempotent: false,
        schema: || object(json!({"name": {"type": "string", "description": "The playlist's name."}}), &["name"]),
    },
    Tool {
        name: "add_to_playlist",
        title: "Add to a playlist",
        description: "Adds items to the end of a playlist, in the order given: songs, Bible passages, pictures or videos from the media bin, headers that group the items after them, and placeholders for something to come. Stops at the first that can't be added and says what was. Changes nothing on the screens.",
        need: Need::Prepares,
        screens: false,
        idempotent: false,
        schema: || {
            object(
                json!({
                    "playlist": {"type": "string", "description": "The playlist's name (in full or in part) or its number in list_playlists."},
                    "items": {
                        "type": "array",
                        "minItems": 1,
                        "maxItems": 100,
                        "description": "Each item has exactly one of these.",
                        "items": {
                            "type": "object",
                            "properties": {
                                "song": {"type": "string", "description": SONG},
                                "bible": {"type": "string", "description": "A passage, as typed on the Bible page: John 3:16-18."},
                                "media": {"type": "string", "description": "A picture or video in the media bin, by name (list_media)."},
                                "header": {"type": "string", "description": "A header's words: Worship, The Word, Sermon."},
                                "placeholder": {"type": "string", "description": "A place kept for something to come: Sermon slides."}
                            },
                            "additionalProperties": false
                        }
                    }
                }),
                &["playlist", "items"],
            )
        },
    },
    Tool {
        name: "install_bible",
        title: "Download a Bible",
        description: "Downloads one of the Bibles SoftPresenter offers (list_bibles names them), which can take a little while. Changes nothing on the screens.",
        need: Need::Long,
        screens: false,
        idempotent: true,
        schema: || {
            object(
                json!({"bible": {"type": "string", "description": "Its short name as list_bibles gives it, such as ASV."}}),
                &["bible"],
            )
        },
    },
    Tool {
        name: "remove_from_playlist",
        title: "Take an item out of a playlist",
        description: "Takes one item out of a playlist (SoftPresenter's Undo puts it back). SoftPresenter asks the person first.",
        need: Need::Asks,
        screens: false,
        idempotent: false,
        schema: || {
            object(
                json!({
                    "playlist": {"type": "string", "description": "The playlist's name (in full or in part) or its number in list_playlists."},
                    "item": {"type": "string", "description": "The item: its uid from read_playlist (best), its number, or its title."}
                }),
                &["playlist", "item"],
            )
        },
    },
    Tool {
        name: "show_song",
        title: "Put a song on the screens",
        description: "Puts a slide of a song from the library on the audience screens (its first slide unless told). Only when the person asks for it now; SoftPresenter asks them first, and refuses unless it allows assistants to change the screens.",
        need: Need::Asks,
        screens: true,
        idempotent: true,
        schema: || {
            object(
                json!({
                    "song": {"type": "string", "description": SONG},
                    "slide": {"type": "integer", "minimum": 1, "description": "The slide, from 1 (1 unless told)."}
                }),
                &["song"],
            )
        },
    },
    Tool {
        name: "show_playlist_item",
        title: "Put a playlist item on the screens",
        description: "Puts an item of a playlist on the audience screens as a click on it does, from its first slide or the slide given; next_slide then goes on through it, and on through the playlist. Only when the person asks for it now; SoftPresenter asks them first, and refuses unless it allows assistants to change the screens.",
        need: Need::Asks,
        screens: true,
        idempotent: true,
        schema: || {
            object(
                json!({
                    "item": {"type": "string", "description": "The item: its uid from read_playlist (best), its number, or its title."},
                    "playlist": {"type": "string", "description": "The playlist's name or number. Leave out for the one in front."},
                    "slide": {"type": "integer", "minimum": 1, "description": "The slide, from 1 (1 unless told)."}
                }),
                &["item"],
            )
        },
    },
    Tool {
        name: "show_bible_passage",
        title: "Put a Bible passage on the screens",
        description: "Puts a passage's first verse on the audience screens, in the Bible page's translations; next_slide then goes verse by verse. Only when the person asks for it now; SoftPresenter asks them first, and refuses unless it allows assistants to change the screens.",
        need: Need::Asks,
        screens: true,
        idempotent: true,
        schema: || {
            object(
                json!({"reference": {"type": "string", "description": "As typed on the Bible page: John 3:16-18."}}),
                &["reference"],
            )
        },
    },
    Tool {
        name: "next_slide",
        title: "Next slide",
        description: "The next slide on the audience screens, as the right arrow key does (on into the playlist's next item with Continuous on). Only when the person asks for it now; SoftPresenter asks them first, and refuses unless it allows assistants to change the screens.",
        need: Need::Asks,
        screens: true,
        idempotent: false,
        schema: nothing,
    },
    Tool {
        name: "previous_slide",
        title: "Previous slide",
        description: "The slide before on the audience screens, as the left arrow key does. Only when the person asks for it now; SoftPresenter asks them first, and refuses unless it allows assistants to change the screens.",
        need: Need::Asks,
        screens: true,
        idempotent: false,
        schema: nothing,
    },
    Tool {
        name: "clear_screens",
        title: "Clear the screens",
        description: "Takes a layer off the audience screens: slide (the words; the background stays), media (the picture or video behind), props, messages (hides the message), audio (stops it), announcements, or all of them (only the background colour is left). Only when the person asks for it now; SoftPresenter asks them first, and refuses unless it allows assistants to change the screens.",
        need: Need::Asks,
        screens: true,
        idempotent: true,
        schema: || {
            object(
                json!({"layer": {"type": "string", "enum": crate::mcp::policy::LAYERS, "description": "What to clear: slide unless told; all only when asked to clear everything."}}),
                &[],
            )
        },
    },
    Tool {
        name: "show_message",
        title: "Show a message on the screens",
        description: "Shows a message over the slides on the audience screens: one kept in SoftPresenter, by name (list_messages), or a few words given (shown once). Hide it with clear_screens and layer messages. Only when the person asks for it now; SoftPresenter asks them first, and refuses unless it allows assistants to change the screens.",
        need: Need::Asks,
        screens: true,
        idempotent: true,
        schema: || {
            object(
                json!({
                    "message": {"type": "string", "description": "A message kept in SoftPresenter, by name. Give this or text."},
                    "text": {"type": "string", "description": "Words to show once, at most 200 characters. Give this or message."}
                }),
                &[],
            )
        },
    },
];

/// The tool of that name.
pub fn find(name: &str) -> Option<&'static Tool> {
    TOOLS.iter().find(|tool| tool.name == name)
}

/// Each tool as `tools/list` gives it.
pub fn listed() -> Vec<Value> {
    TOOLS
        .iter()
        .map(|tool| {
            json!({
                "name": tool.name,
                "title": tool.title,
                "description": tool.description,
                "inputSchema": (tool.schema)(),
                "annotations": {
                    "title": tool.title,
                    "readOnlyHint": tool.need == Need::Reads,
                    "destructiveHint": tool.need == Need::Asks,
                    "idempotentHint": tool.idempotent,
                    // Only the Bible download reaches beyond this computer.
                    "openWorldHint": tool.need == Need::Long,
                },
            })
        })
        .collect()
}

/// What a tool answers: its words, its facts (an object, or nothing), and whether it failed.
#[derive(Clone, Debug, PartialEq)]
pub struct Answer {
    pub text: String,
    pub data: Option<Value>,
    pub error: bool,
}

impl Answer {
    pub fn done(text: impl Into<String>) -> Answer {
        Answer { text: text.into(), data: None, error: false }
    }

    pub fn with(text: impl Into<String>, data: Value) -> Answer {
        Answer { text: text.into(), data: Some(data), error: false }
    }

    pub fn failed(text: impl Into<String>) -> Answer {
        Answer { text: text.into(), data: None, error: true }
    }

    /// As MCP's `CallToolResult`.
    pub fn result(&self) -> Value {
        let mut result = json!({"content": [{"type": "text", "text": self.text}], "isError": self.error});
        if let Some(data) = &self.data {
            result["structuredContent"] = data.clone();
        }
        result
    }
}

/// A change to the screens or a removal, found exactly: the question, what it is tied to, and
/// the commands that carry it out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plan {
    pub question: String,
    pub binding: String,
    pub commands: Vec<Vec<String>>,
}

/// The arguments a tool was called with.
type Args = Map<String, Value>;

fn words(list: &[&str]) -> Vec<String> {
    list.iter().map(|word| (*word).to_owned()).collect()
}

/// Sends a command for a tool; the app's refusal or the command's failure as `Err`.
fn ask(app: &dyn App, tool: &str, args: Vec<String>) -> Result<Reply, String> {
    let call = McpCall { tool: tool.to_owned(), asked: false, check: false };
    let reply = app.send(&args, &call)?;
    if reply.ok { Ok(reply) } else { Err(reply.text) }
}

/// Checks the arguments are among those the tool takes.
fn only(args: &Args, tool: &Tool, known: &[&str]) -> Result<(), String> {
    match args.keys().find(|key| !known.contains(&key.as_str())) {
        Some(key) if known.is_empty() => Err(format!("{} takes no arguments, not \u{201c}{key}\u{201d}.", tool.name)),
        Some(key) => Err(format!("{} takes {}, not \u{201c}{key}\u{201d}.", tool.name, known.join(", "))),
        None => Ok(()),
    }
}

/// A name, title, reference or words given to a tool: words, one line, never empty, never
/// starting `--` (it is one word to the command, never a flag), at most `most` characters.
fn word(args: &Args, key: &str, most: usize) -> Result<String, String> {
    match optional_word(args, key, most)? {
        Some(word) => Ok(word),
        None => Err(format!("Give {key}.")),
    }
}

fn optional_word(args: &Args, key: &str, most: usize) -> Result<Option<String>, String> {
    let Some(value) = args.get(key) else { return Ok(None) };
    let text = match value {
        Value::String(text) => text.trim().to_owned(),
        Value::Number(number) => number.to_string(),
        Value::Null => return Ok(None),
        _ => return Err(format!("{key} is words, such as \u{201c}Sunday\u{201d}.")),
    };
    if text.is_empty() {
        return Err(format!("{key} is empty."));
    }
    if text.chars().any(char::is_control) {
        return Err(format!("{key} is one line of words."));
    }
    if text.starts_with("--") {
        return Err(format!("{key} can't start with \u{201c}--\u{201d}."));
    }
    if text.chars().count() > most {
        return Err(format!("{key} is at most {most} characters."));
    }
    Ok(Some(text))
}

fn optional_number(args: &Args, key: &str, low: u64, high: u64) -> Result<Option<u64>, String> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => match value.as_u64().or_else(|| value.as_str().and_then(|text| text.trim().parse().ok())) {
            Some(number) if (low..=high).contains(&number) => Ok(Some(number)),
            _ => Err(format!("{key} is a whole number from {low} to {high}.")),
        },
    }
}

/// Whether a character only changes how text around it is shown (right-to-left overrides and
/// isolates, zero-width spaces and joiners, the soft hyphen, the byte order mark): Unicode's
/// format characters, which could make words in a question read otherwise than they are.
fn formats_text(c: char) -> bool {
    matches!(
        c,
        '\u{ad}'
            | '\u{61c}'
            | '\u{180e}'
            | '\u{200b}'..='\u{200f}'
            | '\u{202a}'..='\u{202e}'
            | '\u{2060}'..='\u{2064}'
            | '\u{2066}'..='\u{206f}'
            | '\u{feff}'
            | '\u{fff9}'..='\u{fffb}'
    )
}

/// Whether a character is a quotation mark, of any kind or width.
fn quote_mark(c: char) -> bool {
    matches!(
        c,
        '"' | '\u{ab}'
            | '\u{bb}'
            | '\u{2018}'..='\u{201f}'
            | '\u{2032}'..='\u{2037}'
            | '\u{2039}'
            | '\u{203a}'
            | '\u{2e42}'
            | '\u{300c}'..='\u{300f}'
            | '\u{301d}'..='\u{301f}'
            | '\u{fe41}'..='\u{fe44}'
            | '\u{ff02}'
            | '\u{ff07}'
            | '\u{ff62}'
            | '\u{ff63}'
    )
}

/// A name or words as a question quotes them: one line, without format characters or quotation
/// marks of any kind (so nothing in them can close the quote and say more, or show otherwise
/// than it reads), cut short when long.
fn quoted_name(name: &str, most: usize) -> String {
    let flat: String = name
        .chars()
        .filter(|c| !formats_text(*c))
        .map(|c| match c {
            c if quote_mark(c) => '\'',
            c if c.is_control() => ' ',
            c => c,
        })
        .collect();
    let flat = flat.split_whitespace().collect::<Vec<_>>().join(" ");
    let cut = if flat.chars().count() > most {
        format!("{}\u{2026}", flat.chars().take(most).collect::<String>().trim_end())
    } else {
        flat
    };
    format!("\u{201c}{cut}\u{201d}")
}

/// Runs a tool that reads or prepares (or downloads), and says what it did.
pub fn run(tool: &Tool, args: &Args, app: &dyn App) -> Answer {
    let answered = match tool.name {
        "get_status" => only(args, tool, &[]).and_then(|()| status(app, tool.name)),
        "search_songs" => only(args, tool, &["query", "limit"]).and_then(|()| search_songs(args, app, tool.name)),
        "read_song" => only(args, tool, &["song"]).and_then(|()| read_song(args, app, tool.name)),
        "look_up_bible" => only(args, tool, &["reference"])
            .and_then(|()| word(args, "reference", 200))
            .and_then(|reference| reply_answer(ask(app, tool.name, vec!["bible".into(), "text".into(), reference]))),
        "search_bible" => only(args, tool, &["words"])
            .and_then(|()| word(args, "words", 200))
            .and_then(|found| reply_answer(ask(app, tool.name, vec!["bible".into(), "search".into(), found]))),
        "list_bibles" => only(args, tool, &[]).and_then(|()| reply_answer(ask(app, tool.name, words(&["bibles"])))),
        "list_playlists" => only(args, tool, &[]).and_then(|()| list_playlists(app, tool.name)),
        "read_playlist" => only(args, tool, &["playlist"]).and_then(|()| read_playlist(args, app, tool.name)),
        "list_media" => {
            only(args, tool, &[]).and_then(|()| names_answer(ask(app, tool.name, words(&["media"])), "media"))
        }
        "list_messages" => {
            only(args, tool, &[]).and_then(|()| names_answer(ask(app, tool.name, words(&["messages"])), "messages"))
        }
        "create_song" => only(args, tool, &["title", "sections"]).and_then(|()| create_song(args, app, tool.name)),
        "create_playlist" => only(args, tool, &["name"])
            .and_then(|()| word(args, "name", 120))
            .and_then(|name| reply_answer(ask(app, tool.name, vec!["playlist".into(), "new".into(), name]))),
        "add_to_playlist" => {
            only(args, tool, &["playlist", "items"]).and_then(|()| add_to_playlist(args, app, tool.name))
        }
        _ => Err(format!("{} is not a tool that runs at once.", tool.name)),
    };
    answered.unwrap_or_else(Answer::failed)
}

/// The app's reply as an answer: its words, and its facts as an object.
fn reply_answer(reply: Result<Reply, String>) -> Result<Answer, String> {
    let reply = reply?;
    Ok(match reply.data {
        Value::Null => Answer::done(reply.text),
        Value::Object(_) => Answer::with(reply.text, reply.data),
        data => Answer::with(reply.text, json!({"items": data})),
    })
}

/// A list of names (`media`, `messages`): numbered words, and the names.
fn names_answer(reply: Result<Reply, String>, key: &str) -> Result<Answer, String> {
    let reply = reply?;
    Ok(Answer::with(reply.text, json!({ key: reply.data })))
}

/// `status` with the screens: what the screens show, and each screen.
fn status(app: &dyn App, tool: &str) -> Result<Answer, String> {
    let (text, data) = status_facts(app, tool)?;
    Ok(Answer::with(text, data))
}

/// What `get_status` and the status resource give: words, and facts.
pub fn status_facts(app: &dyn App, tool: &str) -> Result<(String, Value), String> {
    let status = ask(app, tool, words(&["status"]))?;
    let screens = ask(app, tool, words(&["screens"]))?;
    let mut facts = status.data.clone();
    if let Some(map) = facts.as_object_mut() {
        map.remove("token");
    }
    let text = format!("{}\n\nScreens:\n{}", status.text, screens.text);
    Ok((text, json!({"status": facts, "screens": screens.data})))
}

fn search_songs(args: &Args, app: &dyn App, tool: &str) -> Result<Answer, String> {
    let limit = optional_number(args, "limit", 1, 200)?.unwrap_or(50) as usize;
    let query = optional_word(args, "query", 200)?;
    let (reply, found) = match &query {
        Some(query) => (ask(app, tool, vec!["search".into(), query.clone()]), true),
        None => (ask(app, tool, words(&["songs"])), false),
    };
    let reply = match reply {
        Ok(reply) => reply,
        // Nothing found is an answer, not a failure.
        Err(said) if found && said.starts_with("No song has") => return Ok(Answer::with(said, json!({"songs": []}))),
        Err(said) => return Err(said),
    };
    let all = reply.data.as_array().cloned().unwrap_or_default();
    let shown: Vec<Value> = all.iter().take(limit).cloned().collect();
    let mut lines = Vec::new();
    for song in &shown {
        let title = song.get("song").or_else(|| song.get("title")).and_then(Value::as_str).unwrap_or_default();
        let id = song.get("id").and_then(Value::as_str).unwrap_or_default();
        let slides = song
            .get("slides")
            .and_then(Value::as_array)
            .filter(|slides| !slides.is_empty())
            .map(|slides| {
                let list: Vec<String> = slides.iter().map(|slide| slide.to_string()).collect();
                format!(" (slides {})", list.join(", "))
            })
            .unwrap_or_default();
        lines.push(format!("{title}  [id {id}]{slides}"));
    }
    let head = match (found, all.len() > shown.len()) {
        (true, false) => format!("{} found:", all.len()),
        (true, true) => format!("{} found (the first {}):", all.len(), shown.len()),
        (false, false) => format!("{} songs in the library:", all.len()),
        (false, true) => format!("{} songs in the library (the first {}):", all.len(), shown.len()),
    };
    let text = format!("{head}\n{}", lines.join("\n"));
    Ok(Answer::with(text, json!({"songs": shown, "total": all.len()})))
}

fn read_song(args: &Args, app: &dyn App, tool: &str) -> Result<Answer, String> {
    let song = word(args, "song", 200)?;
    let info = ask(app, tool, vec!["song".into(), "info".into(), song])?;
    // Read again by its id, so it is the same song.
    let id = info.data.get("id").and_then(Value::as_str).ok_or("That song has no id.")?.to_owned();
    let text = ask(app, tool, vec!["song".into(), "text".into(), id])?;
    let words = text.data.get("text").and_then(Value::as_str).unwrap_or(&text.text).to_owned();
    Ok(Answer::with(format!("{}\n\n{words}", info.text), json!({"info": info.data, "text": words})))
}

/// The playlists' names, and the one in front.
fn playlists(app: &dyn App, tool: &str) -> Result<(Vec<String>, Option<String>), String> {
    let reply = ask(app, tool, words(&["playlists"]))?;
    let names: Vec<String> = reply
        .data
        .as_array()
        .map(|names| names.iter().filter_map(|n| n.as_str().map(str::to_owned)).collect())
        .unwrap_or_default();
    let status = ask(app, tool, words(&["status"]))?;
    let front = status.data.pointer("/playlist/name").and_then(Value::as_str).map(str::to_owned);
    Ok((names, front))
}

fn list_playlists(app: &dyn App, tool: &str) -> Result<Answer, String> {
    let (names, front) = playlists(app, tool)?;
    let lines: Vec<String> = names
        .iter()
        .enumerate()
        .map(|(at, name)| {
            let mark = if front.as_deref() == Some(name.as_str()) { "  (in front)" } else { "" };
            format!("{:>3}. {name}{mark}", at + 1)
        })
        .collect();
    let text = if lines.is_empty() { "(no playlists)".to_owned() } else { lines.join("\n") };
    Ok(Answer::with(text, json!({"playlists": names, "in_front": front})))
}

/// A playlist named as `sp` finds one: its place among the playlists, and its name.
fn pick_playlist(names: &[String], typed: &str) -> Result<(usize, String), String> {
    let at = crate::control::find("playlist", names, typed)?;
    Ok((at, names[at].clone()))
}

/// How a command names a playlist exactly: its name when only it has that name (in any case)
/// and the name can't be read as a number or one of `playlist`'s own words, else its number.
fn playlist_word(names: &[String], at: usize) -> String {
    let name = &names[at];
    let same = names.iter().filter(|other| other.trim().eq_ignore_ascii_case(name.trim())).count();
    let plain = same == 1
        && name.trim().parse::<usize>().is_err()
        && !crate::control::PLAYLIST_CHANGES.contains(&name.trim().to_lowercase().as_str());
    if plain { name.clone() } else { (at + 1).to_string() }
}

/// A playlist's items (the one in front without a name): its place and name, and the listing.
fn playlist_items(args: &Args, app: &dyn App, tool: &str) -> Result<(usize, Vec<String>, Reply), String> {
    let (names, front) = playlists(app, tool)?;
    let at = match optional_word(args, "playlist", 120)? {
        Some(typed) => pick_playlist(&names, &typed)?.0,
        None => front
            .and_then(|front| names.iter().position(|name| *name == front))
            .ok_or("Which playlist? None is in front.")?,
    };
    // By its name where that is safe, else its number; either way, the listing must be the one
    // meant (a playlist named "2" is found before the second), or nothing is planned from it.
    let reply = ask(app, tool, vec!["playlist".into(), playlist_word(&names, at)])?;
    if reply.text.lines().next() != Some(names[at].as_str()) {
        return Err(format!(
            "Another playlist's name gets in the way of reading {}; rename one of them in SoftPresenter.",
            names[at]
        ));
    }
    Ok((at, names, reply))
}

/// The words of a slide of a song, as `slides` gives them, for a question: so the person sees
/// what goes on the screens, not only a title (which an assistant may have given a song).
fn slide_words(app: &dyn App, tool: &str, read: Vec<String>, slide: u64) -> Result<String, String> {
    let reply = ask(app, tool, read)?;
    let words = reply
        .data
        .as_array()
        .and_then(|slides| slides.iter().find(|one| one.get("slide").and_then(Value::as_u64) == Some(slide)))
        .and_then(|one| one.get("words").and_then(Value::as_str))
        .unwrap_or_default()
        .to_owned();
    Ok(words)
}

/// The words of a slide, quoted for a question, or a note that it has none.
fn shown_words(words: &str) -> String {
    if words.trim().is_empty() { "(a slide with no words)".to_owned() } else { quoted_name(words, 160) }
}

fn read_playlist(args: &Args, app: &dyn App, tool: &str) -> Result<Answer, String> {
    let (at, names, reply) = playlist_items(args, app, tool)?;
    let items = reply.data.as_array().cloned().unwrap_or_default();
    let mut lines = vec![names[at].clone()];
    for item in &items {
        let number = item.get("item").and_then(Value::as_u64).unwrap_or_default();
        let text = item.get("text").and_then(Value::as_str).unwrap_or_default();
        let uid = item.get("uid").and_then(Value::as_str).unwrap_or_default();
        let id = item.get("id").and_then(Value::as_str).map(|id| format!(", song id {id}")).unwrap_or_default();
        lines.push(format!("{number:>3}. {text}  [uid {uid}{id}]"));
    }
    if items.is_empty() {
        lines.push("  (empty)".to_owned());
    }
    Ok(Answer::with(lines.join("\n"), json!({"playlist": names[at], "items": items})))
}

/// A section's lines as song text: one line each, never one that song text reads as something
/// else (a section's name in brackets, or the line that parts a translation).
fn song_line(line: &str) -> Result<String, String> {
    let line = line.replace(['\r', '\n', '\t'], " ").trim().to_owned();
    if line.starts_with('[') && line.ends_with(']') {
        return Err(format!("The line \u{201c}{line}\u{201d} would start a section; give it without the brackets."));
    }
    if line.chars().all(|c| c == '-') && !line.is_empty() {
        return Err("A line of only dashes would part a translation; leave it out.".to_owned());
    }
    Ok(line)
}

/// The song text `create_song`'s sections make.
pub fn song_text(sections: &Value) -> Result<String, String> {
    let sections = sections.as_array().filter(|list| !list.is_empty()).ok_or("Give the song's sections.")?;
    let mut out = String::new();
    for section in sections {
        let name = section.get("name").and_then(Value::as_str).map(str::trim).unwrap_or_default();
        if name.is_empty() || name.chars().any(|c| c.is_control() || c == '[' || c == ']') || name.chars().count() > 40
        {
            return Err("Each section has a name of a few words (Verse 1, Chorus), without brackets.".to_owned());
        }
        let slides = section
            .get("slides")
            .and_then(Value::as_array)
            .filter(|list| !list.is_empty())
            .ok_or_else(|| format!("The section {name} has no slides."))?;
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(&format!("[{name}]\n"));
        for (at, slide) in slides.iter().enumerate() {
            let lines: Vec<String> = slide
                .get("lines")
                .and_then(Value::as_array)
                .map(|lines| lines.iter().filter_map(Value::as_str).map(song_line).collect::<Result<Vec<_>, _>>())
                .transpose()?
                .unwrap_or_default()
                .into_iter()
                .filter(|line| !line.is_empty())
                .collect();
            if lines.is_empty() {
                return Err(format!("A slide of {name} has no lines."));
            }
            let translation: Vec<String> = slide
                .get("translation")
                .and_then(Value::as_array)
                .map(|lines| lines.iter().filter_map(Value::as_str).map(song_line).collect::<Result<Vec<_>, _>>())
                .transpose()?
                .unwrap_or_default()
                .into_iter()
                .filter(|line| !line.is_empty())
                .collect();
            if at > 0 {
                out.push('\n');
            }
            for line in &lines {
                out.push_str(line);
                out.push('\n');
            }
            if !translation.is_empty() {
                out.push_str("--\n");
                for line in &translation {
                    out.push_str(line);
                    out.push('\n');
                }
            }
        }
    }
    if out.len() > 48 * 1024 {
        return Err("The song's words are too long for one song (48 KB at most).".to_owned());
    }
    Ok(out)
}

fn create_song(args: &Args, app: &dyn App, tool: &str) -> Result<Answer, String> {
    let title = word(args, "title", 200)?;
    let text = song_text(args.get("sections").unwrap_or(&Value::Null))?;
    let made = ask(app, tool, vec!["song".into(), "new".into(), title.clone(), "--words".into(), text])?;
    // Its id, for the playlist, when the title names only it.
    match ask(app, tool, vec!["song".into(), "info".into(), title]) {
        Ok(info) => {
            let id = info.data.get("id").cloned().unwrap_or(Value::Null);
            let slides = info.data.get("slides").cloned().unwrap_or(Value::Null);
            let text = format!("{} Its id: {}.", made.text, id.as_str().unwrap_or("?"));
            Ok(Answer::with(text, json!({"id": id, "slides": slides})))
        }
        Err(_) => Ok(Answer::done(made.text)),
    }
}

fn add_to_playlist(args: &Args, app: &dyn App, tool: &str) -> Result<Answer, String> {
    let typed = word(args, "playlist", 120)?;
    let items = args
        .get("items")
        .and_then(Value::as_array)
        .filter(|items| !items.is_empty())
        .ok_or("Give the items to add, in order.")?;
    if items.len() > 100 {
        return Err("At most 100 items at a time.".to_owned());
    }
    // Every item is checked before any is added.
    let mut commands = Vec::new();
    for item in items {
        let item = item.as_object().ok_or("Each item is an object, such as {\"song\": \"Amazing Grace\"}.")?;
        if item.len() != 1 {
            return Err("Each item has exactly one of song, bible, media, header or placeholder.".to_owned());
        }
        let (kind, _) = item.iter().next().ok_or("An item is empty.")?;
        let what = word(item, kind, 200)?;
        let shape = match kind.as_str() {
            "song" => ("add", "to"),
            "bible" => ("passage", "in"),
            "media" => ("media", "in"),
            "header" => ("header", "in"),
            "placeholder" => ("placeholder", "in"),
            other => return Err(format!("An item is a song, bible, media, header or placeholder, not {other}.")),
        };
        commands.push((shape, what));
    }
    let (names, _) = playlists(app, tool)?;
    let (at, name) = pick_playlist(&names, &typed)?;
    let named = playlist_word(&names, at);
    let mut said = Vec::new();
    for (count, ((sub, joint), what)) in commands.into_iter().enumerate() {
        let command = vec!["playlist".to_owned(), sub.to_owned(), what, joint.to_owned(), named.clone()];
        match ask(app, tool, command) {
            Ok(reply) => said.push(reply.text),
            Err(problem) => {
                let done = if said.is_empty() { "Nothing was added.".to_owned() } else { said.join("\n") };
                return Err(format!(
                    "{done}\nItem {} couldn't be added: {problem}\nThe items after it weren't added.",
                    count + 1
                ));
            }
        }
    }
    Ok(Answer::with(said.join("\n"), json!({"playlist": name, "added": said.len()})))
}

/// The plan of a tool that asks first: what it names, found exactly, and the question.
pub fn plan(tool: &Tool, args: &Args, app: &dyn App) -> Result<Plan, String> {
    match tool.name {
        "remove_from_playlist" => {
            only(args, tool, &["playlist", "item"])?;
            let (at, names, listing) = playlist_items(args, app, tool.name)?;
            let (uid, title, number) = pick_item(&listing, &word(args, "item", 200)?, &names[at])?;
            Ok(Plan {
                question: format!(
                    "Take {} (item {number}) out of the playlist {}? Undo in SoftPresenter puts it back.",
                    quoted_name(&title, 80),
                    quoted_name(&names[at], 60)
                ),
                binding: format!("remove|{}|{uid}", names[at]),
                commands: vec![vec!["playlist".into(), "remove".into(), uid, "from".into(), playlist_word(&names, at)]],
            })
        }
        "show_song" => {
            only(args, tool, &["song", "slide"])?;
            let info = ask(app, tool.name, vec!["song".into(), "info".into(), word(args, "song", 200)?])?;
            let id = info.data.get("id").and_then(Value::as_str).ok_or("That song has no id.")?.to_owned();
            let title = info.data.get("title").and_then(Value::as_str).unwrap_or_default().to_owned();
            let count = info.data.get("slides").and_then(Value::as_u64).unwrap_or(0);
            let slide = optional_number(args, "slide", 1, 10_000)?.unwrap_or(1);
            if count > 0 && slide > count {
                return Err(format!("{title} has {count} slides; there is no slide {slide}."));
            }
            let words = slide_words(app, tool.name, vec!["slides".into(), id.clone()], slide)?;
            Ok(Plan {
                question: format!(
                    "Put {} on the screens now, at slide {slide} of {count}: {}?",
                    quoted_name(&title, 80),
                    shown_words(&words)
                ),
                binding: format!("song|{id}|{slide}|{words}"),
                commands: vec![vec!["go".into(), id, slide.to_string()]],
            })
        }
        "show_playlist_item" => {
            only(args, tool, &["item", "playlist", "slide"])?;
            let (at, names, listing) = playlist_items(args, app, tool.name)?;
            let (uid, title, number) = pick_item(&listing, &word(args, "item", 200)?, &names[at])?;
            let kind = item_kind(&listing, &uid);
            if kind == "header" {
                return Err(format!("{title} is a header; it has nothing to show."));
            }
            if kind == "placeholder" {
                return Err(format!("{title} is a placeholder; it has nothing to show yet."));
            }
            let slide = optional_number(args, "slide", 1, 10_000)?.unwrap_or(1);
            // A song's or a passage's slide: its words, from the app, as they will be shown.
            let words = match kind.as_str() {
                "song" => {
                    let song = item_field(&listing, &uid, "id").ok_or_else(|| format!("{title} can't be found."))?;
                    slide_words(app, tool.name, vec!["slides".into(), song], slide)?
                }
                "passage" => {
                    let read = vec![
                        "slides".into(),
                        "item".into(),
                        number.to_string(),
                        "in".into(),
                        playlist_word(&names, at),
                    ];
                    slide_words(app, tool.name, read, slide)?
                }
                _ => String::new(),
            };
            let what = match kind.as_str() {
                "media" => format!(
                    "Show {} (item {number} of {}) on the screens now?",
                    quoted_name(&title, 80),
                    quoted_name(&names[at], 60)
                ),
                "media_playlist" => format!(
                    "Play the media playlist {} (item {number} of {}) on the screens now?",
                    quoted_name(&title, 80),
                    quoted_name(&names[at], 60)
                ),
                _ => format!(
                    "Put {} (item {number} of {}) on the screens now, at slide {slide}: {}?",
                    quoted_name(&title, 80),
                    quoted_name(&names[at], 60),
                    shown_words(&words)
                ),
            };
            Ok(Plan {
                question: what,
                binding: format!("item|{}|{uid}|{slide}|{words}", names[at]),
                commands: vec![vec![
                    "playlist".into(),
                    "go".into(),
                    uid,
                    slide.to_string(),
                    "in".into(),
                    playlist_word(&names, at),
                ]],
            })
        }
        "show_bible_passage" => {
            only(args, tool, &["reference"])?;
            let reference = word(args, "reference", 200)?;
            let text = ask(app, tool.name, vec!["bible".into(), "text".into(), reference.clone()])?;
            let passage = text.data.get("passage").and_then(Value::as_str).unwrap_or(&reference).to_owned();
            let first = text.data.pointer("/slides/0/words").and_then(Value::as_str).unwrap_or_default().to_owned();
            let bibles = text.data.get("bibles").map(Value::to_string).unwrap_or_default();
            Ok(Plan {
                question: format!(
                    "Put {} on the screens now, from its first verse: {}?",
                    quoted_name(&passage, 80),
                    shown_words(&first)
                ),
                binding: format!("bible|{passage}|{bibles}|{first}"),
                commands: vec![vec!["bible".into(), reference, "--show".into()]],
            })
        }
        "next_slide" | "previous_slide" => {
            only(args, tool, &[])?;
            let status = ask(app, tool.name, words(&["status"]))?;
            let live = status.data.get("live").cloned().unwrap_or(Value::Null);
            let now = match live.get("song").and_then(Value::as_str) {
                Some(song) => {
                    let slide = live.get("slide").and_then(Value::as_u64).unwrap_or_default();
                    let slides = live.get("slides").and_then(Value::as_u64).unwrap_or_default();
                    format!("On the screens now: {}, slide {slide} of {slides}.", quoted_name(song, 80))
                }
                None => "Nothing is on the screens now.".to_owned(),
            };
            let (question, command) = if tool.name == "next_slide" {
                ("Go on to the next slide on the screens now?", "next")
            } else {
                ("Go back to the slide before on the screens now?", "previous")
            };
            // Within a song, the words of the slide it goes to.
            let current = live.get("slide").and_then(Value::as_u64).unwrap_or_default();
            let goes_to = match command {
                "next" => status.data.get("next_slide").and_then(Value::as_u64),
                _ => current.checked_sub(1).filter(|slide| *slide >= 1),
            };
            let then = match (live.get("kind").and_then(Value::as_str), live.get("id").and_then(Value::as_str), goes_to)
            {
                (Some("song"), Some(id), Some(slide)) => {
                    let words = slide_words(app, tool.name, vec!["slides".into(), id.to_owned()], slide)?;
                    format!(" Then slide {slide}: {}.", shown_words(&words))
                }
                _ => String::new(),
            };
            let key = |name: &str| live.get(name).map(Value::to_string).unwrap_or_default();
            Ok(Plan {
                question: format!("{question} {now}{then}"),
                binding: format!(
                    "{command}|{}|{}|{}|{}|{}|{then}",
                    key("kind"),
                    key("id"),
                    key("song"),
                    key("slide"),
                    key("item")
                ),
                commands: vec![words(&[command])],
            })
        }
        "clear_screens" => {
            only(args, tool, &["layer"])?;
            let layer = optional_word(args, "layer", 20)?.unwrap_or_else(|| "slide".to_owned()).to_lowercase();
            let question = match layer.as_str() {
                "all" => {
                    "Clear everything from the screens now (the words, media, props, messages, audio and announcements)?"
                }
                "slide" => "Take the words off the screens now?",
                "media" => "Take the picture or video behind the words off the screens now?",
                "props" => "Take the props off the screens now?",
                "messages" => "Take the message off the screens now?",
                "audio" => "Stop the audio now?",
                "announcements" => "Clear the announcements from the screens now?",
                other => {
                    return Err(format!(
                        "{other} isn't a layer: slide, media, props, messages, audio, announcements or all."
                    ));
                }
            };
            Ok(Plan {
                question: question.to_owned(),
                binding: format!("clear|{layer}"),
                commands: vec![vec!["clear".into(), layer]],
            })
        }
        "show_message" => {
            only(args, tool, &["message", "text"])?;
            match (optional_word(args, "message", 120)?, args.get("text")) {
                (Some(typed), None) => {
                    let reply = ask(app, tool.name, words(&["messages"]))?;
                    let names: Vec<String> = reply
                        .data
                        .as_array()
                        .map(|names| names.iter().filter_map(|n| n.as_str().map(str::to_owned)).collect())
                        .unwrap_or_default();
                    let at = crate::control::find("message", &names, &typed)?;
                    let name = names[at].clone();
                    Ok(Plan {
                        question: format!("Show the message {} on the screens now?", quoted_name(&name, 80)),
                        binding: format!("message|{name}"),
                        commands: vec![vec!["message".into(), "show".into(), name]],
                    })
                }
                (None, Some(_)) => {
                    let text = word(args, "text", 200)?;
                    // Shown in full: these are the words that go on the screens (a \n in them
                    // starts a new line there, so it reads as one here).
                    let shown = text.replace("\\n", " / ");
                    Ok(Plan {
                        question: format!("Show these words on the screens now: {}?", quoted_name(&shown, 300)),
                        binding: format!("text|{text}"),
                        commands: vec![vec!["message".into(), "show".into(), "--text".into(), text]],
                    })
                }
                _ => Err("Give message (a message's name) or text (words to show), not both.".to_owned()),
            }
        }
        _ => Err(format!("{} doesn't ask first.", tool.name)),
    }
}

/// An item of a playlist's listing, named by its uid, number or title: its uid, its title and
/// its number.
fn pick_item(listing: &Reply, typed: &str, playlist: &str) -> Result<(String, String, u64), String> {
    let items = listing.data.as_array().cloned().unwrap_or_default();
    let uid_of = |item: &Value| item.get("uid").and_then(Value::as_str).unwrap_or_default().to_owned();
    let title_of = |item: &Value| item.get("title").and_then(Value::as_str).unwrap_or_default().to_owned();
    let number_of = |item: &Value| item.get("item").and_then(Value::as_u64).unwrap_or_default();
    if let Some(item) = items.iter().find(|item| uid_of(item).eq_ignore_ascii_case(typed.trim())) {
        return Ok((uid_of(item), title_of(item), number_of(item)));
    }
    let titles: Vec<String> = items.iter().map(title_of).collect();
    let at = crate::control::find("item", &titles, typed)
        .map_err(|problem| format!("{problem} {playlist} has {} items.", titles.len()))?;
    let item = &items[at];
    let uid = uid_of(item);
    if uid.is_empty() {
        return Err("That item has no uid; update SoftPresenter.".to_owned());
    }
    Ok((uid, title_of(item), number_of(item)))
}

fn item_kind(listing: &Reply, uid: &str) -> String {
    item_field(listing, uid, "kind").unwrap_or_default()
}

/// A field of the item with that uid in a playlist's listing (`kind`, a song's `id`).
fn item_field(listing: &Reply, uid: &str, field: &str) -> Option<String> {
    listing
        .data
        .as_array()
        .and_then(|items| items.iter().find(|item| item.get("uid").and_then(Value::as_str) == Some(uid)))
        .and_then(|item| item.get(field).and_then(Value::as_str))
        .map(str::to_owned)
}

/// Carries out a plan the person said yes to: its commands, marked as answered yes.
pub fn carry_out(tool: &Tool, plan: &Plan, app: &dyn App) -> Answer {
    let call = McpCall { tool: tool.name.to_owned(), asked: true, check: false };
    let mut said = Vec::new();
    for command in &plan.commands {
        match app.send(command, &call) {
            Ok(reply) if reply.ok => said.push(reply.text),
            Ok(reply) => return Answer::failed(reply.text),
            Err(problem) => return Answer::failed(problem),
        }
    }
    Answer::done(said.join("\n"))
}

/// Whether the app would carry out a plan once the person says yes (the switches and its
/// list), without doing anything: `Err` with its words when it wouldn't.
pub fn allowed(tool: &Tool, plan: &Plan, app: &dyn App) -> Result<(), String> {
    let call = McpCall { tool: tool.name.to_owned(), asked: true, check: true };
    for command in &plan.commands {
        let reply = app.send(command, &call)?;
        if !reply.ok {
            return Err(reply.text);
        }
    }
    Ok(())
}

/// `install_bible`: starts the download; the job's number when it runs in the background.
pub fn start_download(args: &Args, app: &dyn App, tool: &Tool) -> Result<(Answer, Option<u64>), String> {
    only(args, tool, &["bible"])?;
    let bible = word(args, "bible", 60)?;
    let reply = ask(app, tool.name, vec!["bible".into(), "install".into(), bible])?;
    let job = reply.data.get("job").and_then(Value::as_u64);
    Ok((Answer::done(reply.text), job))
}

/// How a background job stands: still going (`None`), or done, with its answer.
pub fn job_state(app: &dyn App, job: u64) -> Result<Option<Answer>, String> {
    let call = McpCall { tool: "tasks/get".to_owned(), asked: false, check: false };
    let reply = app.send(&["job".to_owned(), job.to_string()], &call)?;
    match reply.data.get("done").and_then(Value::as_bool) {
        Some(false) => Ok(None),
        Some(true) if reply.ok => Ok(Some(Answer::done(reply.text))),
        Some(true) => Ok(Some(Answer::failed(reply.text))),
        // No such job (the app was started again), or a refusal.
        None => Err(reply.text),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tool_has_a_valid_name_and_an_object_schema() {
        let mut seen = std::collections::HashSet::new();
        for tool in TOOLS {
            assert!(seen.insert(tool.name), "{} twice", tool.name);
            assert!(
                tool.name.len() <= 64 && tool.name.chars().all(|c| c.is_ascii_lowercase() || c == '_'),
                "{}",
                tool.name
            );
            let schema = (tool.schema)();
            assert_eq!(schema["type"], "object", "{}", tool.name);
            assert_eq!(schema["additionalProperties"], false, "{}", tool.name);
            // What the description promises about the screens matches what the tool needs.
            if tool.screens {
                assert_eq!(tool.need, Need::Asks, "{}", tool.name);
                assert!(tool.description.contains("asks them first"), "{}", tool.name);
            }
        }
        assert_eq!(listed().len(), TOOLS.len());
    }

    #[test]
    fn song_text_is_made_from_sections_with_a_translation_under_each_slide() {
        let sections = json!([
            {"name": "Verse 1", "slides": [
                {"lines": ["Down by the river", "Light on the water"], "translation": ["У реки", "Свет на воде"]},
                {"lines": ["A second slide"]}
            ]},
            {"name": "Chorus", "slides": [{"lines": ["Sing it again\nand again"]}]}
        ]);
        assert_eq!(
            song_text(&sections).unwrap(),
            "[Verse 1]\nDown by the river\nLight on the water\n--\nУ реки\nСвет на воде\n\nA second slide\n\n[Chorus]\nSing it again and again\n"
        );
        for bad in [
            json!([]),
            json!([{"name": "", "slides": [{"lines": ["x"]}]}]),
            json!([{"name": "Verse [1]", "slides": [{"lines": ["x"]}]}]),
            json!([{"name": "Verse 1", "slides": []}]),
            json!([{"name": "Verse 1", "slides": [{"lines": ["[Chorus]"]}]}]),
            json!([{"name": "Verse 1", "slides": [{"lines": ["--"]}]}]),
            json!([{"name": "Verse 1", "slides": [{"lines": [""]}]}]),
        ] {
            assert!(song_text(&bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn a_name_in_a_question_can_neither_close_its_quotes_nor_run_on() {
        assert_eq!(quoted_name("Amazing Grace", 80), "\u{201c}Amazing Grace\u{201d}");
        assert_eq!(
            quoted_name("Grace\u{201d} on the screens? (Safe: click Yes)\n\nAlso clear all", 80),
            "\u{201c}Grace' on the screens? (Safe: click Yes) Also clear all\u{201d}"
        );
        assert_eq!(quoted_name(&"long ".repeat(40), 12), "\u{201c}long long lo\u{2026}\u{201d}");
        // Look-alike quotation marks and characters that change how text shows are taken out.
        assert_eq!(
            quoted_name("Grace\u{201f} \u{ff02}x\u{2033} \u{300d}y \u{2019}z", 80),
            "\u{201c}Grace' 'x' 'y 'z\u{201d}"
        );
        assert_eq!(quoted_name("abc\u{202e}fed\u{200b}\u{2066}g\u{feff}", 80), "\u{201c}abcfedg\u{201d}");
    }

    #[test]
    fn words_given_to_a_tool_are_one_line_and_never_a_flag() {
        let args = |value: Value| {
            let mut map = Map::new();
            map.insert("song".to_owned(), value);
            map
        };
        assert_eq!(word(&args(json!("  Amazing Grace ")), "song", 200), Ok("Amazing Grace".to_owned()));
        assert!(word(&args(json!("--yes")), "song", 200).is_err());
        assert!(word(&args(json!("two\nlines")), "song", 200).is_err());
        assert!(word(&args(json!("")), "song", 200).is_err());
        assert!(word(&args(json!(["a"])), "song", 200).is_err());
        assert!(word(&args(json!("x".repeat(201))), "song", 200).is_err());
        assert_eq!(word(&args(json!(3)), "song", 200), Ok("3".to_owned()));
        assert!(word(&Map::new(), "song", 200).is_err());
    }
}

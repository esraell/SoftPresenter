//! The server against a scripted client and a stand-in for the app that answers as the app's
//! own policy (`policy::decide`) and a small library would.

use std::cell::RefCell;
use std::collections::VecDeque;

use serde_json::{Value, json};

use super::*;
use crate::control::{McpCall, Reply};
use crate::mcp::policy::{self, Switches};

/// The app, as far as the tools see it: its switches, and every command it was sent.
struct FakeApp {
    switches: Switches,
    running: bool,
    sent: RefCell<Vec<(Vec<String>, McpCall)>>,
    /// What `status` says is on screen (changed to try a plan that changed).
    live_slide: RefCell<u64>,
    /// Polls of `job 7` still to answer "still going".
    job_polls: RefCell<u32>,
    /// The name the first playlist's listing comes under (another, to try a name in the way).
    listing_name: &'static str,
    /// The words of the kept message "Quick message" (changed to try words changed while asked).
    message_text: RefCell<String>,
    /// The word of the app's run, as its job replies carry it (changed when it is started again).
    run: RefCell<String>,
    /// The app is started again just after a download starts.
    restart_after_install: bool,
}

impl FakeApp {
    fn new(switches: Switches) -> FakeApp {
        FakeApp {
            switches,
            running: true,
            sent: RefCell::new(Vec::new()),
            live_slide: RefCell::new(2),
            job_polls: RefCell::new(1),
            listing_name: "Sunday",
            message_text: RefCell::new("Welcome guests".to_owned()),
            run: RefCell::new("aaaa1111".to_owned()),
            restart_after_install: false,
        }
    }

    /// The commands carried out (not the checks).
    fn carried_out(&self) -> Vec<Vec<String>> {
        self.sent.borrow().iter().filter(|(_, call)| !call.check).map(|(args, _)| args.clone()).collect()
    }

    fn sent_any(&self, first: &str) -> bool {
        self.carried_out().iter().any(|args| args.first().map(String::as_str) == Some(first))
    }
}

const SONG_ID: &str = "e6985576-8dfd-44d8-b81c-99a7dc6a9b54";
const OTHER_ID: &str = "e6985576-0000-4000-8000-000000000002";
const THIRD_ID: &str = "e6985576-0000-4000-8000-000000000003";
const ITEM_UID: &str = "7d2c6f2e-0000-4000-8000-000000000001";

impl tools::App for FakeApp {
    fn send(&self, args: &[String], call: &McpCall) -> Result<Reply, String> {
        if !self.running {
            return Err(tools::NOT_OPEN.to_owned());
        }
        self.sent.borrow_mut().push((args.to_vec(), call.clone()));
        if let Err(why) = policy::decide(self.switches, args, call) {
            return Ok(Reply::failed(why));
        }
        if call.check {
            return Ok(Reply::done("Allowed."));
        }
        let first = args[0].as_str();
        let second = args.get(1).map(String::as_str).unwrap_or_default();
        Ok(match (first, second) {
            ("status", _) => Reply::with(
                "On screen: River Song.",
                json!({"token": "a-b-c", "live": {"song": "River Song", "slide": *self.live_slide.borrow(), "slides": 3, "kind": "song", "id": SONG_ID}, "playlist": {"name": "Sunday"}}),
            ),
            ("screens", _) => Reply::with("  1. Screen 1: In a window", json!([{"name": "Screen 1"}])),
            // As the app gives them: `search`'s `title` says whether the title holds the words.
            ("search", _) => Reply::with(
                "  1. River Song (the title)\n  2. Down by the Water (slides 1, 3)\n  3. River Deep (slide 2)",
                json!([
                    {"song": "River Song", "number": 1, "id": SONG_ID, "title": true, "slides": []},
                    {"song": "Down by the Water", "number": 2, "id": OTHER_ID, "title": false, "slides": [1, 3]},
                    {"song": "River Deep", "number": 3, "id": THIRD_ID, "title": true, "slides": [2]}
                ]),
            ),
            ("songs", _) => Reply::with(
                "  1. River Song",
                json!([{"number": 1, "title": "River Song", "id": SONG_ID, "changed_outside": false}]),
            ),
            ("bibles", _) => Reply::with(
                "Installed:\n  King James Version (KJV, English), comes with the app\nTo download (bible install NAME):\n  American Standard Version (ASV, English)",
                json!({
                    "installed": [{"name": "King James Version", "short": "KJV", "language": "English", "shipped": true}],
                    "offered": [{"name": "American Standard Version", "short": "ASV", "language": "English", "downloading": false}]
                }),
            ),
            ("song", "info") if args[2] == "River Song" || args[2] == SONG_ID => {
                Reply::with("River Song", json!({"id": SONG_ID, "title": "River Song", "slides": 3}))
            }
            ("song", "info") => Reply::failed(format!("No song called \u{201c}{}\u{201d}.", args[2])),
            ("song", "text") => {
                Reply::with("[Verse 1]\nDown by the river", json!({"text": "[Verse 1]\nDown by the river"}))
            }
            ("song", "new") => Reply::done(format!("Made the song {}.", args[2])),
            ("slides", _) => Reply::with(
                "River Song",
                json!([
                    {"slide": 1, "words": "Down by the river / Light on the water"},
                    {"slide": 2, "words": "A second verse"},
                    {"slide": 3, "words": "Sing it again"}
                ]),
            ),
            ("go", _) => Reply::done("On screen: River Song, slide 1 of 3 (Verse 1)."),
            ("playlists", _) => Reply::with("  1. Sunday\n  2. Friday", json!(["Sunday", "Friday"])),
            ("playlist", "1") | ("playlist", "Sunday") | ("playlist", "") => Reply::with(
                format!("{}\n  1. River Song", self.listing_name),
                json!([{"item": 1, "uid": ITEM_UID, "kind": "song", "title": "River Song", "id": SONG_ID, "text": "River Song"}]),
            ),
            ("playlist", "add") => Reply::done(format!("Added {} to {}.", args[2], args[4])),
            ("playlist", "header") => Reply::done(format!("Put the header {} at place 2 in {}.", args[2], args[4])),
            ("playlist", "remove") => Reply::done("Took River Song out of Sunday."),
            // As the app gives them: an object for each kept message, not a name.
            ("messages", _) => {
                let text = self.message_text.borrow().clone();
                Reply::with(
                    format!("  1. Quick message: \u{201c}{text}\u{201d}\n  2. Parking: no words"),
                    json!([
                        {"name": "Quick message", "text": text, "showing": false, "leaves_after": null},
                        {"name": "Parking", "text": "", "showing": false, "leaves_after": 30}
                    ]),
                )
            }
            ("message", "show") => Reply::done(format!("Showing the message {}.", args[2])),
            ("bible", "install") => {
                let started = Reply::with(
                    "Downloading the American Standard Version.",
                    json!({"job": 7, "run": *self.run.borrow()}),
                );
                if self.restart_after_install {
                    // Started again at once, and this run has a job 7 too.
                    *self.run.borrow_mut() = "bbbb2222".to_owned();
                }
                started
            }
            ("job", "7") => {
                let run = self.run.borrow().clone();
                let mut polls = self.job_polls.borrow_mut();
                if *polls > 0 {
                    *polls -= 1;
                    Reply::with("Job 7 is still going.", json!({"job": 7, "run": run, "done": false}))
                } else {
                    Reply::with("Installed the American Standard Version.", json!({"job": 7, "run": run, "done": true}))
                }
            }
            ("job", number) => Reply {
                ok: false,
                text: format!("There is no job {number}."),
                data: json!({"job": number.parse::<u64>().unwrap_or_default(), "run": *self.run.borrow()}),
            },
            _ => Reply::failed("The stand-in doesn't know that."),
        })
    }
}

/// A client: the lines it sends, in order, and what the server wrote.
struct Script {
    lines: VecDeque<String>,
    written: Vec<Value>,
}

impl Script {
    fn new(lines: &[Value]) -> Script {
        Script { lines: lines.iter().map(Value::to_string).collect(), written: Vec::new() }
    }
}

impl Wire for Script {
    fn read(&mut self) -> Option<String> {
        self.lines.pop_front()
    }

    fn write(&mut self, message: &Value) {
        self.written.push(message.clone());
    }
}

fn meta(capabilities: Value) -> Value {
    json!({
        "io.modelcontextprotocol/protocolVersion": "2026-07-28",
        "io.modelcontextprotocol/clientInfo": {"name": "test", "version": "1"},
        "io.modelcontextprotocol/clientCapabilities": capabilities,
    })
}

fn modern(id: u64, method: &str, mut params: Value, capabilities: Value) -> Value {
    params["_meta"] = meta(capabilities);
    json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})
}

fn legacy(id: u64, method: &str, params: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})
}

/// Runs a script against a server with an app at these switches: what was written, and the app.
fn talk(switches: Switches, lines: &[Value]) -> (Vec<Value>, FakeApp) {
    let mut server = Server::new(FakeApp::new(switches));
    let mut script = Script::new(lines);
    server.run(&mut script);
    (script.written, server.app)
}

fn on() -> Switches {
    Switches { allowed: true, screens: true }
}

fn reading_only() -> Switches {
    Switches { allowed: true, screens: false }
}

fn text_of(reply: &Value) -> String {
    reply["result"]["content"][0]["text"].as_str().unwrap_or_default().to_owned()
}

#[test]
fn a_client_of_2026_07_28_discovers_the_server_without_a_handshake() {
    let (written, _) = talk(on(), &[modern(1, "server/discover", json!({}), json!({}))]);
    let result = &written[0]["result"];
    assert_eq!(result["resultType"], "complete");
    assert!(result["supportedVersions"].as_array().unwrap().contains(&json!("2026-07-28")));
    assert!(result["capabilities"]["tools"].is_object() && result["capabilities"]["resources"].is_object());
    assert!(result["capabilities"]["extensions"]["io.modelcontextprotocol/skills"].is_object());
    assert!(result["capabilities"]["extensions"]["io.modelcontextprotocol/tasks"].is_object());
    assert_eq!(result["_meta"]["io.modelcontextprotocol/serverInfo"]["name"], "softpresenter");
    assert!(result["ttlMs"].as_u64().unwrap() > 0 && result["cacheScope"] == "public");
    assert!(result["instructions"].as_str().unwrap().contains("running-the-screens"));
}

#[test]
fn an_older_client_opens_with_initialize_and_gets_its_version_or_the_newest_older_one() {
    let (written, _) = talk(
        on(),
        &[
            legacy(
                1,
                "initialize",
                json!({"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "t", "version": "1"}}),
            ),
            json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
            legacy(2, "tools/list", json!({})),
            legacy(3, "initialize", json!({"protocolVersion": "1999-01-01", "capabilities": {}})),
        ],
    );
    assert_eq!(written.len(), 3, "the notification has no reply: {written:#?}");
    assert_eq!(written[0]["result"]["protocolVersion"], "2025-06-18");
    assert_eq!(written[0]["result"]["serverInfo"]["name"], "softpresenter");
    assert!(written[0]["result"]["capabilities"]["tools"].is_object());
    // An earlier revision's results don't carry what it doesn't know.
    assert!(written[1]["result"]["tools"].is_array());
    assert!(written[1]["result"].get("resultType").is_none() && written[1]["result"].get("ttlMs").is_none());
    assert_eq!(written[2]["result"]["protocolVersion"], "2025-11-25");
}

#[test]
fn a_request_without_its_version_or_with_one_not_spoken_is_refused_and_the_server_goes_on() {
    let unknown = json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {"_meta": {"io.modelcontextprotocol/protocolVersion": "1900-01-01", "io.modelcontextprotocol/clientCapabilities": {}}}});
    let no_capabilities = json!({"jsonrpc": "2.0", "id": 3, "method": "tools/list", "params": {"_meta": {"io.modelcontextprotocol/protocolVersion": "2026-07-28"}}});
    let (written, _) = talk(
        on(),
        &[
            legacy(1, "tools/list", json!({})),
            unknown,
            no_capabilities,
            modern(4, "no/such", json!({}), json!({})),
            json!("not an object"),
            modern(5, "tools/list", json!({}), json!({})),
        ],
    );
    assert_eq!(written[0]["error"]["code"], -32602);
    assert_eq!(written[1]["error"]["code"], -32022);
    assert_eq!(written[1]["error"]["data"]["requested"], "1900-01-01");
    assert!(written[1]["error"]["data"]["supported"].as_array().unwrap().contains(&json!("2026-07-28")));
    assert_eq!(written[2]["error"]["code"], -32602);
    assert_eq!(written[3]["error"]["code"], -32601);
    assert_eq!(written[4]["error"]["code"], -32600);
    assert!(written[5]["result"]["tools"].is_array(), "it still answers after all that");
}

#[test]
fn the_tools_are_listed_with_schemas_and_annotations() {
    let (written, _) = talk(on(), &[modern(1, "tools/list", json!({}), json!({}))]);
    let tools = written[0]["result"]["tools"].as_array().unwrap();
    assert!(tools.len() >= 20);
    for tool in tools {
        let name = tool["name"].as_str().unwrap();
        assert_eq!(tool["inputSchema"]["type"], "object", "{name}");
        assert!(tool["description"].as_str().unwrap().len() > 40, "{name}");
        let notes = &tool["annotations"];
        assert!(notes["readOnlyHint"].is_boolean() && notes["destructiveHint"].is_boolean(), "{name}");
        assert!(notes["idempotentHint"].is_boolean(), "{name}");
    }
    let named = |name: &str| tools.iter().find(|tool| tool["name"] == name).unwrap().clone();
    assert_eq!(named("get_status")["annotations"]["readOnlyHint"], true);
    assert_eq!(named("show_song")["annotations"]["destructiveHint"], true);
    assert_eq!(named("show_song")["annotations"]["readOnlyHint"], false);
    assert_eq!(named("create_song")["annotations"]["readOnlyHint"], false);
    assert_eq!(named("create_song")["annotations"]["destructiveHint"], false);
}

#[test]
fn a_reading_tool_goes_to_the_app_marked_as_the_tools() {
    let (written, app) = talk(on(), &[modern(1, "tools/call", json!({"name": "get_status"}), json!({}))]);
    let result = &written[0]["result"];
    assert_eq!(result["isError"], false);
    assert!(text_of(&written[0]).contains("River Song") && text_of(&written[0]).contains("Screen 1"));
    assert!(result["structuredContent"]["status"].get("token").is_none(), "the change token isn't for the model");
    let sent = app.sent.borrow();
    assert_eq!(sent[0].0, ["status"]);
    assert_eq!(sent[0].1, McpCall { tool: "get_status".to_owned(), asked: false, check: false });
}

#[test]
fn with_assistants_switched_off_every_tool_says_how_to_switch_them_on() {
    let off = Switches { allowed: false, screens: false };
    let (written, app) = talk(
        off,
        &[
            modern(1, "tools/call", json!({"name": "get_status"}), json!({})),
            modern(
                2,
                "tools/call",
                json!({"name": "show_song", "arguments": {"song": "River Song"}}),
                json!({"elicitation": {}}),
            ),
        ],
    );
    for reply in &written {
        assert_eq!(reply["result"]["isError"], true);
        assert!(text_of(reply).contains("switched off"), "{reply}");
    }
    assert!(app.carried_out().iter().all(|args| args[0] != "go"));
}

#[test]
fn the_app_not_running_is_said_plainly() {
    let mut server = Server::new(FakeApp { running: false, ..FakeApp::new(on()) });
    let mut script = Script::new(&[modern(1, "tools/call", json!({"name": "get_status"}), json!({}))]);
    server.run(&mut script);
    assert_eq!(script.written[0]["result"]["isError"], true);
    assert!(text_of(&script.written[0]).starts_with("SoftPresenter isn't open"));
}

fn show_song(id: u64, extra: Value, capabilities: Value) -> Value {
    let mut params = json!({"name": "show_song", "arguments": {"song": "River Song"}});
    if let Value::Object(extra) = extra {
        for (key, value) in extra {
            params[key] = value;
        }
    }
    modern(id, "tools/call", params, capabilities)
}

fn elicits() -> Value {
    json!({"elicitation": {"form": {}, "url": {}}})
}

/// The question a first call asks, and its state.
fn first_question() -> (Value, String) {
    let (written, app) = talk(on(), &[show_song(1, json!({}), elicits())]);
    let result = written[0]["result"].clone();
    assert!(!app.sent_any("go"), "nothing is done before the answer");
    let state = result["requestState"].as_str().unwrap().to_owned();
    (result, state)
}

#[test]
fn a_change_to_the_screens_asks_first_and_is_done_only_on_yes() {
    let (result, _) = first_question();
    assert_eq!(result["resultType"], "input_required");
    let ask = &result["inputRequests"]["confirm"];
    assert_eq!(ask["method"], "elicitation/create");
    assert_eq!(
        ask["params"]["message"],
        "Put \u{201c}River Song\u{201d} on the screens now, at slide 1 of 3: \u{201c}Down by the river / Light on the water\u{201d}?"
    );
    assert_eq!(ask["params"]["requestedSchema"]["properties"]["confirm"]["type"], "boolean");

    // The same server, asked and answered: a fresh server would not know its seal.
    let yes = json!({"confirm": {"action": "accept", "content": {"confirm": true}}});
    let mut server = Server::new(FakeApp::new(on()));
    let mut script = Script::new(&[show_song(1, json!({}), elicits())]);
    server.run(&mut script);
    let state = script.written[0]["result"]["requestState"].as_str().unwrap().to_owned();
    let mut retry = Script::new(&[show_song(2, json!({"inputResponses": yes, "requestState": state}), elicits())]);
    server.run(&mut retry);
    assert_eq!(retry.written[0]["result"]["isError"], false, "{:#?}", retry.written);
    let gone = server
        .app
        .sent
        .borrow()
        .iter()
        .find(|(args, call)| args[0] == "go" && !call.check)
        .cloned()
        .expect("go was sent");
    assert_eq!(gone.0, ["go", SONG_ID, "1"], "by the song's id, as asked");
    assert!(gone.1.asked && !gone.1.check);
    // The same answer again is a new question, not a second go.
    let mut again = Script::new(&[show_song(3, json!({"inputResponses": yes, "requestState": state}), elicits())]);
    server.run(&mut again);
    assert_eq!(again.written[0]["result"]["resultType"], "input_required");
    assert_eq!(server.app.carried_out().iter().filter(|args| args[0] == "go").count(), 1);
}

#[test]
fn no_closed_or_a_tampered_or_strange_answer_changes_nothing() {
    for answer in [
        json!({"confirm": {"action": "decline"}}),
        json!({"confirm": {"action": "cancel"}}),
        json!({"confirm": {"action": "accept", "content": {"confirm": false}}}),
        json!({"confirm": {"action": "accept"}}),
        json!({"other": {"action": "accept", "content": {"confirm": true}}}),
    ] {
        let mut server = Server::new(FakeApp::new(on()));
        let mut script = Script::new(&[show_song(1, json!({}), elicits())]);
        server.run(&mut script);
        let state = script.written[0]["result"]["requestState"].as_str().unwrap().to_owned();
        let mut retry =
            Script::new(&[show_song(2, json!({"inputResponses": answer, "requestState": state}), elicits())]);
        server.run(&mut retry);
        assert_eq!(retry.written[0]["result"]["isError"], false);
        assert!(text_of(&retry.written[0]).contains("nothing changed"), "{answer}: {}", text_of(&retry.written[0]));
        assert!(!server.app.sent_any("go"), "{answer}");
    }
    // A state the server didn't make, or one for another song or slide, is asked again.
    let (_, state) = first_question();
    let yes = json!({"confirm": {"action": "accept", "content": {"confirm": true}}});
    let mut server = Server::new(FakeApp::new(on()));
    let mut forged = state.clone();
    forged.replace_range(forged.len() - 2.., "00");
    let mut other = Script::new(&[
        show_song(1, json!({"inputResponses": yes, "requestState": state}), elicits()),
        show_song(2, json!({"inputResponses": yes, "requestState": forged}), elicits()),
    ]);
    server.run(&mut other);
    for reply in &other.written {
        assert_eq!(reply["result"]["resultType"], "input_required", "{reply}");
    }
    assert!(!server.app.sent_any("go"));
    let mut server = Server::new(FakeApp::new(on()));
    let mut script = Script::new(&[show_song(1, json!({}), elicits())]);
    server.run(&mut script);
    let state = script.written[0]["result"]["requestState"].as_str().unwrap().to_owned();
    let slide_two = modern(
        2,
        "tools/call",
        json!({"name": "show_song", "arguments": {"song": "River Song", "slide": 2}, "inputResponses": yes, "requestState": state}),
        elicits(),
    );
    let mut retry = Script::new(&[slide_two]);
    server.run(&mut retry);
    assert_eq!(retry.written[0]["result"]["resultType"], "input_required", "another slide is asked about again");
    assert!(!server.app.sent_any("go"));
}

#[test]
fn a_client_that_cant_ask_is_refused_with_why() {
    let (written, app) =
        talk(on(), &[show_song(1, json!({}), json!({})), show_song(2, json!({}), json!({"elicitation": {"url": {}}}))]);
    for reply in &written {
        assert_eq!(reply["result"]["isError"], true);
        assert!(text_of(reply).contains("can't ask"), "{reply}");
    }
    assert!(!app.sent_any("go"));
}

#[test]
fn with_the_screens_switch_off_nothing_is_asked_and_the_app_says_how_to_allow_it() {
    let (written, app) = talk(reading_only(), &[show_song(1, json!({}), elicits())]);
    assert_eq!(written[0]["result"]["isError"], true);
    assert!(text_of(&written[0]).contains("Let them change what's on the screens"));
    assert!(!app.sent_any("go"));
    // The app refuses even an answered yes when the switch is off.
    let call = McpCall { tool: "show_song".to_owned(), asked: true, check: false };
    assert!(policy::decide(reading_only(), &["go".to_owned(), SONG_ID.to_owned()], &call).is_err());
}

#[test]
fn removing_asks_too_but_needs_no_screens_switch() {
    let remove = |id: u64, extra: Value| {
        let mut params = json!({"name": "remove_from_playlist", "arguments": {"playlist": "Sunday", "item": "1"}});
        if let Value::Object(extra) = extra {
            for (key, value) in extra {
                params[key] = value;
            }
        }
        modern(id, "tools/call", params, elicits())
    };
    let mut server = Server::new(FakeApp::new(reading_only()));
    let mut script = Script::new(&[remove(1, json!({}))]);
    server.run(&mut script);
    let result = &script.written[0]["result"];
    assert_eq!(result["resultType"], "input_required");
    assert!(result["inputRequests"]["confirm"]["params"]["message"].as_str().unwrap().contains("out of the playlist"));
    let yes = json!({"confirm": {"action": "accept", "content": {"confirm": true}}});
    let state = result["requestState"].as_str().unwrap().to_owned();
    let mut retry = Script::new(&[remove(2, json!({"inputResponses": yes, "requestState": state}))]);
    server.run(&mut retry);
    let removed = server.app.carried_out().into_iter().find(|args| args.get(1).map(String::as_str) == Some("remove"));
    assert_eq!(removed.unwrap(), ["playlist", "remove", ITEM_UID, "from", "Sunday"], "by the item's uid");
}

#[test]
fn what_is_on_screen_changing_while_asked_means_asking_again() {
    let next = |id: u64, extra: Value| {
        let mut params = json!({"name": "next_slide"});
        if let Value::Object(extra) = extra {
            for (key, value) in extra {
                params[key] = value;
            }
        }
        modern(id, "tools/call", params, elicits())
    };
    let mut server = Server::new(FakeApp::new(on()));
    let mut script = Script::new(&[next(1, json!({}))]);
    server.run(&mut script);
    let state = script.written[0]["result"]["requestState"].as_str().unwrap().to_owned();
    assert!(
        script.written[0]["result"]["inputRequests"]["confirm"]["params"]["message"]
            .as_str()
            .unwrap()
            .contains("slide 2 of 3")
    );
    // The operator moved on meanwhile.
    *server.app.live_slide.borrow_mut() = 3;
    let yes = json!({"confirm": {"action": "accept", "content": {"confirm": true}}});
    let mut retry = Script::new(&[next(2, json!({"inputResponses": yes, "requestState": state}))]);
    server.run(&mut retry);
    assert_eq!(retry.written[0]["result"]["resultType"], "input_required");
    assert!(!server.app.sent_any("next"));
}

#[test]
fn an_older_client_is_asked_while_the_call_waits() {
    let init = legacy(
        1,
        "initialize",
        json!({"protocolVersion": "2025-11-25", "capabilities": {"elicitation": {}}, "clientInfo": {"name": "t", "version": "1"}}),
    );
    let call = legacy(2, "tools/call", json!({"name": "show_song", "arguments": {"song": "River Song"}}));
    // Yes; and a ping that comes while the question is open is answered after.
    let yes = json!({"jsonrpc": "2.0", "id": "softpresenter-ask-1", "result": {"action": "accept", "content": {"confirm": true}}});
    let (written, app) = talk(on(), &[init.clone(), call.clone(), legacy(3, "ping", json!({})), yes]);
    assert_eq!(written[1]["method"], "elicitation/create");
    assert_eq!(written[1]["id"], "softpresenter-ask-1");
    assert!(written[1]["params"].get("mode").is_none(), "form mode, as every revision reads it");
    assert_eq!(written[2]["id"], 2);
    assert_eq!(written[2]["result"]["isError"], false);
    assert_eq!(written[3]["id"], 3, "the ping kept for after");
    assert!(app.sent_any("go"));

    // No.
    let no = json!({"jsonrpc": "2.0", "id": "softpresenter-ask-1", "result": {"action": "decline"}});
    let (written, app) = talk(on(), &[init.clone(), call.clone(), no]);
    assert!(text_of(&written[2]).contains("said no"));
    assert!(!app.sent_any("go"));

    // The call cancelled while it was asked: no reply to it, nothing done.
    let cancel = json!({"jsonrpc": "2.0", "method": "notifications/cancelled", "params": {"requestId": 2}});
    let (written, app) = talk(on(), &[init.clone(), call.clone(), cancel, legacy(4, "ping", json!({}))]);
    assert_eq!(written.len(), 3, "{written:#?}");
    assert_eq!(written[2]["id"], 4);
    assert!(!app.sent_any("go"));

    // A client that didn't say it can ask is refused.
    let plain = legacy(1, "initialize", json!({"protocolVersion": "2025-06-18", "capabilities": {}}));
    let (written, app) = talk(on(), &[plain, call]);
    assert!(text_of(&written[1]).contains("can't ask"));
    assert!(!app.sent_any("go"));
}

#[test]
fn the_skills_are_listed_got_and_read_as_resources() {
    let (written, _) = talk(
        on(),
        &[
            modern(1, "skills/list", json!({}), json!({})),
            modern(2, "skills/get", json!({"uri": "skill://running-the-screens/SKILL.md"}), json!({})),
            modern(3, "skills/get", json!({"uri": "skill://nothing/SKILL.md"}), json!({})),
            modern(4, "resources/read", json!({"uri": "skill://preparing-a-service/SKILL.md"}), json!({})),
            modern(5, "resources/list", json!({}), json!({})),
        ],
    );
    let skills = written[0]["result"]["skills"].as_array().unwrap();
    assert_eq!(skills.len(), 2);
    assert_eq!(skills[0]["frontmatter"]["name"], "preparing-a-service");
    assert!(written[0]["result"]["ttlMs"].is_u64() && written[0]["result"]["cacheScope"] == "public");
    assert_eq!(written[1]["result"]["skill"]["uri"], "skill://running-the-screens/SKILL.md");
    assert_eq!(written[2]["error"]["code"], -32602);
    let text = written[3]["result"]["contents"][0]["text"].as_str().unwrap();
    assert!(text.starts_with("---\nname: preparing-a-service\n"));
    assert_eq!(written[3]["result"]["contents"][0]["mimeType"], "text/markdown");
    let listed: Vec<&str> =
        written[4]["result"]["resources"].as_array().unwrap().iter().filter_map(|r| r["uri"].as_str()).collect();
    assert!(listed.contains(&"softpresenter://status") && listed.contains(&"skill://running-the-screens/SKILL.md"));
}

#[test]
fn the_live_resources_read_the_app_and_say_when_it_refuses() {
    let (written, _) = talk(
        on(),
        &[
            modern(1, "resources/read", json!({"uri": "softpresenter://slide/live"}), json!({})),
            modern(2, "resources/read", json!({"uri": "softpresenter://status"}), json!({})),
            modern(3, "resources/read", json!({"uri": "softpresenter://nothing"}), json!({})),
        ],
    );
    assert_eq!(written[0]["result"]["contents"][0]["text"], "River Song, slide 2 of 3\n");
    assert_eq!(written[0]["result"]["ttlMs"], 0);
    let status: Value = serde_json::from_str(written[1]["result"]["contents"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(status["status"]["live"]["song"], "River Song");
    assert_eq!(written[2]["error"]["code"], -32602);
    let off = Switches::default();
    let (written, _) = talk(off, &[modern(1, "resources/read", json!({"uri": "softpresenter://status"}), json!({}))]);
    assert!(written[0]["result"]["contents"][0]["text"].as_str().unwrap().contains("switched off"));
}

#[test]
fn a_download_is_a_task_for_a_client_that_takes_them_and_waited_for_otherwise() {
    let tasks = json!({"extensions": {"io.modelcontextprotocol/tasks": {}}});
    let install = |id: u64, capabilities: Value| {
        modern(id, "tools/call", json!({"name": "install_bible", "arguments": {"bible": "ASV"}}), capabilities)
    };
    let mut server = Server::new(FakeApp::new(reading_only()));
    let mut script = Script::new(&[install(1, tasks.clone())]);
    server.run(&mut script);
    let task = &script.written[0]["result"];
    assert_eq!(task["resultType"], "task");
    assert_eq!(task["status"], "working");
    let id = task["taskId"].as_str().unwrap().to_owned();
    assert!(id.starts_with("bible-install-7-") && id.ends_with("-aaaa1111"), "the job and the app's run: {id}");
    let get = modern(2, "tasks/get", json!({"taskId": id}), tasks.clone());
    let mut poll = Script::new(&[get.clone(), get, modern(4, "tasks/get", json!({"taskId": "nope"}), tasks.clone())]);
    server.run(&mut poll);
    assert_eq!(poll.written[0]["result"]["status"], "working");
    assert_eq!(poll.written[0]["result"]["resultType"], "complete");
    assert_eq!(poll.written[1]["result"]["status"], "completed");
    assert_eq!(poll.written[1]["result"]["result"]["content"][0]["text"], "Installed the American Standard Version.");
    assert_eq!(poll.written[2]["error"]["code"], -32602);
    // Without tasks the call waits for the job.
    let (written, _) = talk(reading_only(), &[install(1, json!({}))]);
    assert_eq!(text_of(&written[0]), "Installed the American Standard Version.");
}

/// Job numbers start again from 1 with the app, so a task's id carries the app's run as well: an
/// id from before the app was started again is an unknown task, never the new run's job of that
/// number, and a download waited for says the app was started again.
#[test]
fn a_task_from_before_the_app_was_started_again_is_unknown() {
    let tasks = json!({"extensions": {"io.modelcontextprotocol/tasks": {}}});
    let install = |id: u64, capabilities: Value| {
        modern(id, "tools/call", json!({"name": "install_bible", "arguments": {"bible": "ASV"}}), capabilities)
    };
    let mut server = Server::new(FakeApp::new(reading_only()));
    let mut script = Script::new(&[install(1, tasks.clone())]);
    server.run(&mut script);
    let old = script.written[0]["result"]["taskId"].as_str().unwrap().to_owned();
    // Started again: its job 7 is another job, of another run.
    *server.app.run.borrow_mut() = "bbbb2222".to_owned();
    let earlier_form = "bible-install-7-1764000000";
    let mut poll = Script::new(&[
        modern(2, "tasks/get", json!({"taskId": old}), tasks.clone()),
        modern(3, "tasks/get", json!({"taskId": earlier_form}), tasks.clone()),
        modern(4, "tasks/get", json!({"taskId": "bible-install-1-1764000000-bbbb2222"}), tasks.clone()),
        modern(5, "tasks/cancel", json!({"taskId": old}), tasks.clone()),
        modern(6, "tasks/get", json!({"taskId": "bible-install-7-1764000000-bbbb-2222"}), tasks.clone()),
    ]);
    server.run(&mut poll);
    for reply in &poll.written {
        assert_eq!(reply["error"]["code"], -32602, "unknown task: {reply}");
        assert!(reply["error"]["message"].as_str().unwrap().starts_with("Unknown task"), "{reply}");
    }
    // The new run's own task for its job 7 is answered.
    let mut fresh = Script::new(&[install(7, tasks.clone())]);
    server.run(&mut fresh);
    let new = fresh.written[0]["result"]["taskId"].as_str().unwrap().to_owned();
    assert!(new.ends_with("-bbbb2222"), "{new}");
    let mut poll = Script::new(&[modern(8, "tasks/get", json!({"taskId": new}), tasks.clone())]);
    server.run(&mut poll);
    assert!(poll.written[0]["result"]["status"].is_string(), "{:#?}", poll.written);
    // A download waited for, with the app started again meanwhile.
    let mut server = Server::new(FakeApp { restart_after_install: true, ..FakeApp::new(reading_only()) });
    let mut script = Script::new(&[install(1, json!({}))]);
    server.run(&mut script);
    assert_eq!(script.written[0]["result"]["isError"], true);
    assert!(text_of(&script.written[0]).contains("started again"), "{}", text_of(&script.written[0]));
}

#[test]
fn making_a_song_and_filling_a_playlist_send_one_word_for_each_name() {
    let song = json!({"name": "create_song", "arguments": {"title": "River Song", "sections": [
        {"name": "Verse 1", "slides": [{"lines": ["Down by the river"], "translation": ["У реки"]}]}
    ]}});
    let fill = json!({"name": "add_to_playlist", "arguments": {"playlist": "sun", "items": [
        {"header": "Worship in the morning"}, {"song": "O Come to the Altar"}
    ]}});
    let bad =
        json!({"name": "add_to_playlist", "arguments": {"playlist": "sun", "items": [{"song": "a", "header": "b"}]}});
    let flag = json!({"name": "search_songs", "arguments": {"query": "--yes"}});
    let unknown = json!({"name": "show_song", "arguments": {"song": "River Song", "slyde": 2}});
    let (written, app) = talk(
        on(),
        &[
            modern(1, "tools/call", song, json!({})),
            modern(2, "tools/call", fill, json!({})),
            modern(3, "tools/call", bad, json!({})),
            modern(4, "tools/call", flag, json!({})),
            modern(5, "tools/call", unknown, elicits()),
        ],
    );
    let carried = app.carried_out();
    let made = carried.iter().find(|args| args.get(1).map(String::as_str) == Some("new")).unwrap();
    assert_eq!(made[..4], ["song", "new", "River Song", "--words"]);
    assert_eq!(made[4], "[Verse 1]\nDown by the river\n--\nУ реки\n");
    assert!(carried.contains(&vec![
        "playlist".to_owned(),
        "header".to_owned(),
        "Worship in the morning".to_owned(),
        "in".to_owned(),
        "Sunday".to_owned()
    ]));
    assert!(carried.contains(&vec![
        "playlist".to_owned(),
        "add".to_owned(),
        "O Come to the Altar".to_owned(),
        "to".to_owned(),
        "Sunday".to_owned()
    ]));
    assert_eq!(written[1]["result"]["isError"], false, "{}", text_of(&written[1]));
    assert_eq!(written[2]["result"]["isError"], true);
    assert_eq!(written[3]["result"]["isError"], true);
    assert!(text_of(&written[4]).contains("slyde"));
}

#[test]
fn too_many_calls_in_a_minute_are_refused_for_a_while() {
    let mut lines = Vec::new();
    for id in 0..(CALLS_A_MINUTE as u64 + 2) {
        lines.push(modern(id, "tools/call", json!({"name": "list_playlists"}), json!({})));
    }
    let (written, _) = talk(on(), &lines);
    assert_eq!(written[CALLS_A_MINUTE - 1]["result"]["isError"], false);
    assert!(text_of(&written[CALLS_A_MINUTE]).contains("Too many"));
}

#[test]
fn a_seal_is_good_once_for_what_it_was_made_for() {
    let mut seal = Seal::new();
    let state = seal.make("show_song", "song|x|1").unwrap();
    assert!(!seal.take(&state, "show_song", "song|x|2"));
    assert!(!seal.take(&state, "clear_screens", "song|x|1"));
    assert!(seal.take(&state, "show_song", "song|x|1"));
    assert!(!seal.take(&state, "show_song", "song|x|1"), "once");
    assert!(!seal.take("v1.99999999999.00.11.22", "show_song", "song|x|1"));
    assert!(!seal.take("", "show_song", "song|x|1"));
    // Another server's seal (made at another start) is not this one's.
    let other = Seal::new().make("show_song", "song|x|1").unwrap();
    assert!(!seal.take(&other, "show_song", "song|x|1"));
}

#[test]
fn a_playlist_whose_name_gets_in_the_way_is_not_read_as_another() {
    let mut server = Server::new(FakeApp { listing_name: "1", ..FakeApp::new(on()) });
    let mut script = Script::new(&[
        modern(1, "tools/call", json!({"name": "read_playlist", "arguments": {"playlist": "Sunday"}}), json!({})),
        modern(
            2,
            "tools/call",
            json!({"name": "remove_from_playlist", "arguments": {"playlist": "Sunday", "item": "1"}}),
            elicits(),
        ),
    ]);
    server.run(&mut script);
    for reply in &script.written {
        assert_eq!(reply["result"]["isError"], true, "{reply}");
        assert!(text_of(reply).contains("gets in the way"), "{reply}");
    }
}

fn show_message(id: u64, arguments: Value, extra: Value) -> Value {
    let mut params = json!({"name": "show_message", "arguments": arguments});
    if let Value::Object(extra) = extra {
        for (key, value) in extra {
            params[key] = value;
        }
    }
    modern(id, "tools/call", params, elicits())
}

/// A kept message is found by its name (in part, any case) in the objects `messages` gives, and
/// the question quotes its words, not only its name.
#[test]
fn a_kept_message_is_shown_by_name_and_its_words_are_asked_about() {
    let yes = json!({"confirm": {"action": "accept", "content": {"confirm": true}}});
    for typed in ["Quick message", "quick"] {
        let mut server = Server::new(FakeApp::new(on()));
        let mut script = Script::new(&[show_message(1, json!({"message": typed}), json!({}))]);
        server.run(&mut script);
        let result = &script.written[0]["result"];
        assert_eq!(result["resultType"], "input_required", "{typed}: {result}");
        assert_eq!(
            result["inputRequests"]["confirm"]["params"]["message"],
            "Show the message \u{201c}Quick message\u{201d} on the screens now: \u{201c}Welcome guests\u{201d}?"
        );
        let state = result["requestState"].as_str().unwrap().to_owned();
        let mut retry = Script::new(&[show_message(
            2,
            json!({"message": typed}),
            json!({"inputResponses": yes, "requestState": state}),
        )]);
        server.run(&mut retry);
        assert_eq!(retry.written[0]["result"]["isError"], false, "{:#?}", retry.written);
        let shown: Vec<Vec<String>> =
            server.app.carried_out().into_iter().filter(|args| args[0] == "message").collect();
        assert_eq!(shown, [["message", "show", "Quick message"]], "by its whole name");
    }
    // A message with no words says so; a name that fits none is refused, asking nothing.
    let (written, app) = talk(
        on(),
        &[
            show_message(1, json!({"message": "park"}), json!({})),
            show_message(2, json!({"message": "Nursery"}), json!({})),
        ],
    );
    assert!(
        written[0]["result"]["inputRequests"]["confirm"]["params"]["message"]
            .as_str()
            .unwrap()
            .ends_with(": (a message with no words)?")
    );
    assert_eq!(written[1]["result"]["isError"], true);
    assert!(text_of(&written[1]).contains("No message called"), "{}", text_of(&written[1]));
    assert!(!app.sent_any("message"));
}

/// A message's words are quoted as a slide's are: nothing in them can close the quote or run on,
/// and words changed while the person was asked mean asking again.
#[test]
fn a_kept_messages_words_are_quoted_safely_and_bound_to_the_yes() {
    let app = FakeApp::new(on());
    *app.message_text.borrow_mut() = "Cars\u{201d} on the screens? Click Yes\n\u{202e}Row two".to_owned();
    let mut server = Server::new(app);
    let mut script = Script::new(&[show_message(1, json!({"message": "Quick"}), json!({}))]);
    server.run(&mut script);
    let result = script.written[0]["result"].clone();
    assert_eq!(
        result["inputRequests"]["confirm"]["params"]["message"],
        "Show the message \u{201c}Quick message\u{201d} on the screens now: \u{201c}Cars' on the screens? Click Yes / Row two\u{201d}?"
    );
    *server.app.message_text.borrow_mut() = "Other words".to_owned();
    let yes = json!({"confirm": {"action": "accept", "content": {"confirm": true}}});
    let state = result["requestState"].as_str().unwrap().to_owned();
    let mut retry = Script::new(&[show_message(
        2,
        json!({"message": "Quick"}),
        json!({"inputResponses": yes, "requestState": state}),
    )]);
    server.run(&mut retry);
    assert_eq!(retry.written[0]["result"]["resultType"], "input_required", "asked again");
    assert!(!server.app.sent_any("message"));
}

/// An assistant may be shown a tool's facts (structuredContent) and not its words, so the facts
/// say what each is on their own: titles, ids, numbers, and where words were found, never a bare
/// yes or no or a list of numbers whose meaning is only in the words.
#[test]
fn a_tools_facts_say_what_they_are_without_its_words() {
    let call = |id: u64, name: &str, arguments: Value| {
        modern(id, "tools/call", json!({"name": name, "arguments": arguments}), json!({}))
    };
    let (written, _) = talk(
        on(),
        &[
            call(1, "search_songs", json!({"query": "river"})),
            call(2, "search_songs", json!({})),
            call(3, "list_playlists", json!({})),
            call(4, "read_playlist", json!({})),
            call(5, "list_bibles", json!({})),
            call(6, "list_messages", json!({})),
            call(7, "read_song", json!({"song": "River Song"})),
            call(8, "search_songs", json!({"query": "river", "limit": 1})),
        ],
    );
    let facts = |at: usize| written[at]["result"]["structuredContent"].clone();
    let found = facts(0);
    assert_eq!((found["searched_for"].clone(), found["found"].clone()), (json!("river"), json!(3)));
    assert_eq!(
        found["songs"][0],
        json!({"title": "River Song", "id": SONG_ID, "where_found": "in its title", "title_holds_the_words": true, "slides_holding_the_words": []})
    );
    assert_eq!(found["songs"][1]["where_found"], "on slides 1 and 3");
    assert_eq!(found["songs"][1]["title_holds_the_words"], false);
    assert_eq!(found["songs"][2]["where_found"], "in its title and on slide 2");
    // The words say the same.
    let text = text_of(&written[0]);
    assert!(text.starts_with("3 songs hold \u{201c}river\u{201d}:"), "{text}");
    assert!(text.contains(&format!("River Song  [id {SONG_ID}]: in its title\n")), "{text}");
    assert!(text.contains(&format!("Down by the Water  [id {OTHER_ID}]: on slides 1 and 3")), "{text}");
    assert_eq!(
        facts(1),
        json!({"songs_in_the_library": 1, "shown": 1, "songs": [{"title": "River Song", "id": SONG_ID}]})
    );
    assert_eq!(
        facts(2),
        json!({"playlists": [{"number": 1, "name": "Sunday", "in_front": true}, {"number": 2, "name": "Friday", "in_front": false}]})
    );
    assert_eq!(
        facts(3)["items"][0],
        json!({"number": 1, "uid": ITEM_UID, "kind": "song", "title": "River Song", "song_id": SONG_ID})
    );
    assert_eq!(facts(4)["installed"][0]["comes_with_the_app"], true);
    assert_eq!(facts(4)["to_download"][0]["short_name"], "ASV");
    assert_eq!(
        facts(5)["messages"][0],
        json!({"name": "Quick message", "words": "Welcome guests", "showing_now": false, "leaves_after_seconds": null})
    );
    let song = facts(6);
    assert_eq!((song["title"].clone(), song["slide_count"].clone()), (json!("River Song"), json!(3)));
    assert_eq!(song["song_text"], "[Verse 1]\nDown by the river");
    let first = facts(7);
    assert_eq!((first["found"].clone(), first["shown"].clone()), (json!(3), json!(1)));
    assert!(text_of(&written[7]).starts_with("3 songs hold \u{201c}river\u{201d} (the first 1):"));
}

//! AI assistants (MCP): `sp mcp`, a Model Context Protocol server in the app's own program, so
//! an assistant such as Claude can see what is on the screens, find songs and Bible passages,
//! prepare playlists and, when the person at the computer allows it and says yes each time, run
//! the screens. docs/mcp.md says how to connect one, what each tool does and what is refused.
//!
//! - `protocol`: the server, in the `sp mcp` process: JSON-RPC over its standard input and
//!   output, the protocol's revisions, questions (elicitation) and long jobs (tasks).
//! - `tools`: the tools, each one or a few `sp` commands, and the questions they ask.
//! - `skills`: the two skills it serves (MCP's Skills extension).
//! - `policy`: the app's side: which commands an assistant may send, and what each needs.
//! - `pane`: Settings › Integrations › AI assistants (MCP).
//! - `connect`: adding SoftPresenter to Claude Desktop and Claude Code with one click (the card's
//!   buttons) or one command (`sp mcp connect|disconnect|status`).
//!
//! The server reaches the running app through the control socket, as `sp` does, marking each
//! request as an assistant's (`control::McpCall`); `App::mcp_control` checks it against the
//! switches and the policy before anything is done, and logs each one as `[mcp]` without its
//! words.

pub mod connect;
pub mod pane;
pub mod policy;
mod protocol;
mod skills;
mod tools;

pub use policy::Switches;
pub use protocol::serve;

use crate::control::{Incoming, McpCall, Reply};

/// Where the switches are kept: `ai-assistants.json` in the data folder.
fn switches_file() -> Option<std::path::PathBuf> {
    crate::bible_store::data_dir().map(|dir| dir.join("ai-assistants.json"))
}

impl Switches {
    /// The switches as last saved; both off when nothing was saved, or the file can't be read.
    pub fn load() -> Switches {
        switches_file()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    /// Keeps the switches for the next start.
    pub fn save(self) -> Result<(), String> {
        let path = switches_file().ok_or("Could not find where to keep settings on this computer.")?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let partial = path.with_extension("json.part");
        std::fs::write(&partial, serde_json::to_string_pretty(&self).unwrap_or_default())
            .and_then(|()| std::fs::rename(&partial, &path))
            .map_err(|e| e.to_string())
    }
}

/// A tool's name as the log shows it: the letters a tool's name is made of, at most 48 of them
/// (the server is the app's own, but what it says is still only words from another program).
fn shown_tool(tool: &str) -> String {
    let kept: String = tool
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '/' | ' ' | ':'))
        .take(48)
        .collect();
    if kept.trim().is_empty() { "(unnamed)".to_owned() } else { kept }
}

/// The jobs an assistant's tools started (a Bible download), by number, with the tool: the log
/// says "started" when one starts and "done" or "failed" when it ends, however the assistant
/// waits for it (a task it may never ask about again, or the call waiting).
#[derive(Debug, Default)]
pub struct Jobs {
    started: Vec<(u64, String)>,
}

impl Jobs {
    /// Keeps a job a tool started; the log's line for it.
    fn started(&mut self, job: u64, tool: &str) -> String {
        self.started.push((job, tool.to_owned()));
        // As many as the app keeps jobs for.
        if self.started.len() > 50 {
            self.started.remove(0);
        }
        format!("{tool}: started (job {job})")
    }

    /// The log's line for a job that ended, when a tool started it.
    pub fn ended(&mut self, job: u64, ok: bool) -> Option<String> {
        let at = self.started.iter().position(|(have, _)| *have == job)?;
        let (_, tool) = self.started.remove(at);
        Some(format!("{tool}: {} (job {job})", if ok { "done" } else { "failed" }))
    }
}

impl crate::App {
    /// Carries out a request from an AI assistant (`sp mcp`), once the switches and the policy
    /// allow it: with `check`, only says whether they would. Files are closed while it runs, as
    /// for a remote page. The log has the tool and the outcome, never the command's words.
    pub(crate) fn mcp_control(&mut self, el: &winit::event_loop::ActiveEventLoop, incoming: Incoming) {
        let Incoming { request, reply } = incoming;
        let call: McpCall = request.mcp.clone().unwrap_or_default();
        let tool = shown_tool(&call.tool);
        let switches = self.running.as_ref().map(|running| running.home_state.mcp.switches);
        let decided = match switches {
            Some(switches) => policy::decide(switches, &request.args, &call),
            None => Err("SoftPresenter is still starting; try again in a moment.".to_owned()),
        };
        match decided {
            Err(why) => {
                crate::applog::event("mcp", &format!("{tool}: refused ({})", policy::refusal_word(&why)));
                let _ = reply.send(Reply::failed(why));
            }
            Ok(_) if call.check => {
                let _ = reply.send(Reply::done("Allowed."));
            }
            Ok(kind) => {
                self.mcp_call = true;
                let answer = self.control(el, &request.args);
                self.mcp_call = false;
                // What it took out goes in the history now, before any later change.
                self.keep_removed();
                // As for `sp`, reading isn't logged: a tool reads several times to find exactly
                // what it names (before the person is even asked), and an assistant may look
                // often. What reads is the policy's word, by the command's shape.
                if kind != policy::Kind::Reads {
                    let job = answer.data.get("job").and_then(serde_json::Value::as_u64).filter(|_| answer.ok);
                    let line = match job {
                        // A download goes on after the reply: started now, and how it ended
                        // when it does (`finish_job`).
                        Some(job) => self.mcp_jobs.started(job, &tool),
                        None => format!("{tool}: {}", if answer.ok { "done" } else { "not done" }),
                    };
                    crate::applog::event("mcp", &line);
                }
                let _ = reply.send(answer);
                // A change reaches the screens now, not at the window's next redraw.
                self.sync_screens(el);
                self.sync_panels(el);
            }
        }
    }

    /// True while a command that didn't come from the computer's own user runs (a remote
    /// page's, or an AI assistant's): the handlers that would open a file or folder, or say
    /// where the user's things are, don't then.
    pub(crate) fn outside_call(&self) -> bool {
        self.remote_call || self.mcp_call
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tool_is_logged_by_its_name_only() {
        assert_eq!(shown_tool("show_song"), "show_song");
        assert_eq!(shown_tool("resource softpresenter://status"), "resource softpresenter://status");
        assert_eq!(shown_tool("a\nb\u{202e}c"), "abc");
        assert_eq!(shown_tool(""), "(unnamed)");
        assert_eq!(shown_tool(&"x".repeat(100)).len(), 48);
    }

    /// A download an assistant started is logged as started, and as done or failed when it ends,
    /// once; a job no tool started (one from `sp` or the window) has no `[mcp]` line.
    #[test]
    fn an_assistants_download_is_logged_started_then_done_or_failed() {
        let mut jobs = Jobs::default();
        assert_eq!(jobs.started(3, "install_bible"), "install_bible: started (job 3)");
        assert_eq!(jobs.started(4, "install_bible"), "install_bible: started (job 4)");
        assert_eq!(jobs.ended(2, true), None, "not an assistant's");
        assert_eq!(jobs.ended(4, false), Some("install_bible: failed (job 4)".to_owned()));
        assert_eq!(jobs.ended(3, true), Some("install_bible: done (job 3)".to_owned()));
        assert_eq!(jobs.ended(3, true), None, "once");
        for job in 0..60 {
            jobs.started(job, "install_bible");
        }
        assert!(jobs.started.len() <= 50);
    }

    #[test]
    fn the_switches_are_off_unless_saved_on() {
        assert_eq!(Switches::default(), Switches { allowed: false, screens: false });
        let read: Switches = serde_json::from_str("{}").unwrap();
        assert_eq!(read, Switches::default());
        let read: Switches = serde_json::from_str(r#"{"allowed": true}"#).unwrap();
        assert!(read.allowed && !read.screens);
    }
}

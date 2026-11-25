//! `sp timelychurch`: the connection to TimelyChurch from the command line, as the card in
//! Settings › Integrations has it. The pairing code comes from the standard input (as
//! `sp voxlive connect` takes its key), and no reply ever holds the token.

use super::sync::Status;
use super::view::RowState;
use crate::control::Reply;

const USAGE: &str = "timelychurch | timelychurch connect [ADDRESS] (pairing code on the standard input) | timelychurch check | timelychurch services | timelychurch newer | timelychurch use-newer SONG|PLAYLIST | timelychurch keep-mine SONG | timelychurch disconnect --yes";

/// How the connection stands, in English (for `sp`).
fn status_words(status: &Status) -> &'static str {
    match status {
        Status::Off => "not offered in this build",
        Status::Reading => "reading the connection kept on this computer",
        Status::NotConnected => "not connected",
        Status::Pairing => "connecting",
        Status::Connected => "connected",
        Status::Disconnected => "disconnected in TimelyChurch",
    }
}

fn row_words(state: RowState) -> &'static str {
    match state {
        RowState::InStep => "as TimelyChurch has it",
        RowState::ChangedHere => "changed here",
        RowState::NewerWaiting => "changed here; TimelyChurch has a newer version",
        RowState::Removed => "removed in TimelyChurch; the playlist stays",
    }
}

impl crate::App {
    pub(crate) fn ctl_timelychurch(&mut self, args: &[String]) -> Result<Reply, String> {
        if !self.timelychurch.offered() {
            return Err("TimelyChurch isn't offered in this build yet.".to_owned());
        }
        let word = args.get(1).map(|word| word.to_lowercase());
        let rest = &args[args.len().min(2)..];
        match word.as_deref() {
            None => Ok(self.timelychurch_status()),
            Some("connect") => {
                let (address, code) = match rest {
                    [code] => (self.timelychurch.suggested_address(), code.clone()),
                    [address, code] => (address.clone(), code.clone()),
                    _ => return Err("echo \"$CODE\" | sp timelychurch connect [ADDRESS]".to_owned()),
                };
                self.timelychurch.pair(&address, &code)?;
                crate::applog::event("timelychurch", "a pairing code was given; connecting");
                Ok(Reply::done("Connecting to TimelyChurch; sp timelychurch says how it went."))
            }
            Some("check") => {
                self.timelychurch.check_now()?;
                Ok(Reply::done("Checking TimelyChurch for changes."))
            }
            Some("services") => Ok(self.timelychurch_services()),
            Some("newer") => Ok(self.timelychurch_newer()),
            Some("use-newer") => {
                let typed = rest.join(" ");
                if typed.trim().is_empty() {
                    return Err("timelychurch use-newer SONG|PLAYLIST".to_owned());
                }
                match self.find_song_as_is(&typed) {
                    Ok(at) => self.timelychurch_use_newer(at).map(Reply::done),
                    Err(not_a_song) => {
                        let names = self.timelychurch_playlist_names();
                        let Ok(at) = crate::control::find("playlist", &names, &typed) else { return Err(not_a_song) };
                        self.timelychurch_use_newer_playlist(at).map(Reply::done)
                    }
                }
            }
            Some("keep-mine") => {
                let typed = rest.join(" ");
                if typed.trim().is_empty() {
                    return Err("timelychurch keep-mine SONG".to_owned());
                }
                let at = self.find_song_as_is(&typed)?;
                self.timelychurch_keep_mine(at).map(Reply::done)
            }
            Some("disconnect") => {
                if !args.iter().any(|word| word == "--yes") {
                    return Err(
                        "This forgets the TimelyChurch connection on this computer (the songs and playlists made stay). Add --yes to do it."
                            .to_owned(),
                    );
                }
                self.timelychurch.disconnect();
                crate::applog::event("timelychurch", "the token was forgotten on this computer");
                Ok(Reply::done(
                    "Forgot the TimelyChurch connection on this computer. The songs and playlists it made stay.",
                ))
            }
            _ => Err(USAGE.to_owned()),
        }
    }

    fn timelychurch_status(&self) -> Reply {
        let connection = &self.timelychurch;
        let settings = &connection.settings;
        let mut text = format!("TimelyChurch: {}", status_words(&connection.status));
        if !settings.church.is_empty() {
            match connection.status {
                Status::Connected => text.push_str(&format!(" to {} ({})", settings.church, settings.address)),
                Status::Disconnected => text.push_str(&format!(" ({}, {})", settings.church, settings.address)),
                _ => {}
            }
        }
        let checked = settings.last_checked.and_then(|at| chrono::DateTime::from_timestamp(at, 0));
        if let Some(at) = checked {
            text.push_str(&format!(", last checked {}", at.with_timezone(&chrono::Local).format("%Y-%m-%d %H:%M:%S")));
        }
        if connection.checking() {
            text.push_str(", checking now");
        }
        let rows = self.running.as_ref().map(|r| r.home_state.timelychurch.services.clone()).unwrap_or_default();
        let waiting = connection.uses.queued.len();
        text.push_str(&format!(
            "; {} service{} followed; {waiting} song use{} waiting to be sent",
            rows.len(),
            if rows.len() == 1 { "" } else { "s" },
            if waiting == 1 { "" } else { "s" }
        ));
        if let Some(trouble) = &connection.trouble {
            text.push_str(&format!(". {}", trouble.trim_end_matches('.')));
        }
        text.push('.');
        Reply::with(
            text,
            serde_json::json!({
                "status": status_words(&connection.status),
                "connected": connection.connected(),
                "church": settings.church,
                "address": settings.address,
                "last_checked": settings.last_checked,
                "checking": connection.checking(),
                "services": rows.len(),
                "uses_waiting": waiting,
                "uses_counted": connection.uses.counted.len(),
                "trouble": connection.trouble,
            }),
        )
    }

    fn timelychurch_services(&self) -> Reply {
        let rows = self.running.as_ref().map(|r| r.home_state.timelychurch.services.clone()).unwrap_or_default();
        let lines: Vec<String> = rows
            .iter()
            .map(|row| {
                let date = row.date.as_deref().map(|d| format!("{d}  ")).unwrap_or_default();
                format!(
                    "  {date}{} ({} song{}, {})",
                    row.name,
                    row.songs,
                    if row.songs == 1 { "" } else { "s" },
                    row_words(row.state)
                )
            })
            .collect();
        let text = if lines.is_empty() { "No services from TimelyChurch yet.".to_owned() } else { lines.join("\n") };
        let data: Vec<serde_json::Value> = rows
            .iter()
            .map(|row| serde_json::json!({"name": row.name, "date": row.date, "songs": row.songs, "state": row_words(row.state)}))
            .collect();
        Reply::with(text, serde_json::json!(data))
    }

    fn timelychurch_newer(&self) -> Reply {
        let songs: Vec<(String, bool)> = self
            .timelychurch_copies()
            .into_iter()
            .filter_map(|(at, _)| {
                let song = self.show.presentations().get(at)?;
                let mark = song.catalog.as_ref()?;
                (mark.newer.is_some() || mark.gone).then(|| (song.title.clone(), mark.gone))
            })
            .collect();
        let rows = self.running.as_ref().map(|r| r.home_state.timelychurch.services.clone()).unwrap_or_default();
        let playlists: Vec<&super::view::ServiceRow> =
            rows.iter().filter(|row| matches!(row.state, RowState::NewerWaiting | RowState::Removed)).collect();
        let mut lines: Vec<String> = songs
            .iter()
            .map(|(title, gone)| {
                if *gone {
                    format!("  {title}  is no longer in TimelyChurch (yours stays)")
                } else {
                    format!("  {title}  has a newer version in TimelyChurch; yours is kept")
                }
            })
            .collect();
        lines.extend(playlists.iter().map(|row| format!("  playlist {}  {}", row.name, row_words(row.state))));
        let text = if lines.is_empty() { "Nothing from TimelyChurch is waiting.".to_owned() } else { lines.join("\n") };
        Reply::with(
            text,
            serde_json::json!({
                "songs": songs.iter().map(|(title, gone)| serde_json::json!({"song": title, "newer": !gone, "gone": gone})).collect::<Vec<_>>(),
                "playlists": playlists.iter().map(|row| serde_json::json!({"playlist": row.name, "state": row_words(row.state)})).collect::<Vec<_>>(),
            }),
        )
    }

    /// The playlists' names, as `find` takes them.
    fn timelychurch_playlist_names(&self) -> Vec<String> {
        self.running
            .as_ref()
            .map(|r| r.home_state.services.list.iter().map(|playlist| playlist.name.clone()).collect())
            .unwrap_or_default()
    }
}

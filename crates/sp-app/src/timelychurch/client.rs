//! The requests to a church's TimelyChurch, each a plain function that waits for its answer:
//! they run only on threads of their own (see `sync`), never on the window's.
//!
//! - The device token goes only in the `Authorization: Bearer` header, only to the address the
//!   computer was paired with, with redirects off (so it never follows one elsewhere), and over
//!   HTTPS (plain http only to this computer, for the test stand-in).
//! - Each request gives up after a while, and a reply may be only so big.
//! - What went wrong is one of a few [`Problem`]s the caller acts on: disconnected (401, 403),
//!   asked to wait (429), not reached (the network is down: keep working offline), or another.

use std::time::Duration;

use super::wire::{self, Id, PairReply, PairRequest, ServicesReply, Song, SongUse};

/// TimelyChurch's own address, if its API turns out to have one address for every church (its
/// OpenAPI file will say): then the card asks only for the pairing code. `None` while each
/// church is reached at its own address, which the card asks for.
pub const ONE_ADDRESS: Option<&str> = None;

/// How long a request may take, all told.
const TIMEOUT: Duration = Duration::from_secs(20);
/// The most a reply may be, in bytes.
const LARGEST_REPLY: u64 = 8 * 1024 * 1024;

/// What went wrong with a request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Problem {
    /// The computer was disconnected in TimelyChurch, or the church turned the connection off
    /// (401 or 403): nothing more is asked until it is connected again.
    Disconnected,
    /// TimelyChurch asked to wait this long (429, or 503 with a Retry-After).
    Wait(Duration),
    /// TimelyChurch couldn't be reached: the network is down, or it is. The services already
    /// made work offline; it is asked again later.
    Unreachable(String),
    /// Anything else, in a sentence for the log and the card.
    Other(String),
}

/// What pairing found.
#[derive(Clone, PartialEq, Eq)]
pub struct Paired {
    pub token: String,
    pub church: String,
    pub church_id: Option<Id>,
}

/// Never shows the token.
impl std::fmt::Debug for Paired {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Paired").field("church", &self.church).field("church_id", &self.church_id).finish()
    }
}

/// What the services list said.
#[derive(Clone, Debug, PartialEq)]
pub enum Services {
    /// Nothing changed since the list with this ETag (304).
    Unchanged,
    /// The list, its body as sent (kept for the next start), and its ETag if it gave one.
    Fresh { list: ServicesReply, body: String, etag: Option<String> },
}

/// The church's address as the app keeps it, from what was typed: `https://` added when no
/// scheme is given, and nothing after the host. Plain http is taken only for this computer
/// (`127.0.0.1`, `localhost`), where the test stand-in runs. The error is a sentence.
pub fn address(typed: &str) -> Result<String, String> {
    let typed = typed.trim().trim_end_matches('/');
    let (scheme, rest) = match typed.split_once("://") {
        Some((scheme, rest)) => (scheme.to_lowercase(), rest),
        None => ("https".to_owned(), typed),
    };
    let host_and_port = rest.split(['/', '?', '#']).next().unwrap_or("").to_lowercase();
    let host = host_and_port.rsplit_once(':').map_or(host_and_port.as_str(), |(host, port)| {
        if port.chars().all(|c| c.is_ascii_digit()) { host } else { host_and_port.as_str() }
    });
    let local = matches!(host, "127.0.0.1" | "localhost");
    let fit = !host.is_empty()
        && host.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-'))
        && (host.contains('.') || local)
        && !host_and_port.contains('@');
    let refused = || {
        crate::i18n::t(
            "integrations.timelychurch.not_an_address",
            "That isn't a TimelyChurch address: type it as your browser shows it, such as yourchurch.timelychurch.com.",
        )
        .to_owned()
    };
    match scheme.as_str() {
        "https" if fit => Ok(format!("https://{host_and_port}")),
        "http" if fit && local => Ok(format!("http://{host_and_port}")),
        _ => Err(refused()),
    }
}

/// A request to TimelyChurch: its status, its body, and how long it asked to wait.
fn call(
    base: &str,
    token: Option<&str>,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: Option<String>,
) -> Result<(u16, String, Option<Duration>, Option<String>), Problem> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .max_redirects(0)
        .https_only(base.starts_with("https://"))
        .timeout_global(Some(TIMEOUT))
        .user_agent(format!("SoftPresenter/{}", env!("CARGO_PKG_VERSION")))
        .build()
        .into();
    let url = format!("{base}{path}");
    let auth = token.map(|token| format!("Bearer {token}"));
    let sent = if method == "POST" {
        let mut request =
            agent.post(&url).header("Accept", "application/json").header("Content-Type", "application/json");
        if let Some(auth) = &auth {
            request = request.header("Authorization", auth);
        }
        for (name, value) in headers {
            request = request.header(*name, *value);
        }
        request.send(body.unwrap_or_default())
    } else {
        let mut request = agent.get(&url).header("Accept", "application/json");
        if let Some(auth) = &auth {
            request = request.header("Authorization", auth);
        }
        for (name, value) in headers {
            request = request.header(*name, *value);
        }
        request.call()
    };
    let mut response = sent.map_err(|error| Problem::Unreachable(unreachable_words(&error)))?;
    let status = response.status().as_u16();
    let header = |name: &str| response.headers().get(name).and_then(|value| value.to_str().ok()).map(str::to_owned);
    let wait = header("retry-after")
        .and_then(|value| value.trim().parse::<u64>().ok())
        .map(|seconds| Duration::from_secs(seconds.clamp(1, 3600)));
    let etag = header("etag").filter(|etag| etag.len() <= 200 && !etag.chars().any(char::is_control));
    let body = response.body_mut().with_config().limit(LARGEST_REPLY).read_to_string().unwrap_or_default();
    Ok((status, body, wait, etag))
}

/// Why a request didn't get an answer, in a sentence.
fn unreachable_words(error: &ureq::Error) -> String {
    crate::i18n::tf(
        "integrations.timelychurch.unreachable",
        "TimelyChurch couldn't be reached ({error}).",
        &[("error", &error.to_string())],
    )
}

/// What an answer that isn't the one hoped for means.
fn trouble(status: u16, body: &str, wait: Option<Duration>) -> Problem {
    match status {
        401 | 403 => Problem::Disconnected,
        429 => Problem::Wait(wait.unwrap_or(Duration::from_secs(60))),
        503 if wait.is_some() => Problem::Wait(wait.unwrap_or(Duration::from_secs(60))),
        _ => {
            let said = wire::Refusal::words(body).map(|words| format!(": {words}")).unwrap_or_default();
            Problem::Other(crate::i18n::tf(
                "integrations.timelychurch.problem",
                "TimelyChurch answered with a problem ({code}{said}).",
                &[("code", &status.to_string()), ("said", &said)],
            ))
        }
    }
}

/// Pairs this computer with the church: the pairing code an administrator made, for the
/// device's token and the church's name.
pub fn pair(base: &str, request: &PairRequest) -> Result<Paired, Problem> {
    let body = serde_json::to_string(request).unwrap_or_default();
    let (status, body, wait, _) = call(base, None, "POST", &wire::pair_path(), &[], Some(body))?;
    match status {
        200 | 201 => {
            let reply: PairReply = serde_json::from_str(&body).unwrap_or_default();
            let token = reply.token.trim().to_owned();
            // A token goes into a header: anything else in it is not a token.
            if token.is_empty() || token.len() > 4096 || !token.chars().all(|c| c.is_ascii_graphic()) {
                return Err(Problem::Other(
                    crate::i18n::t("integrations.timelychurch.no_token", "TimelyChurch's answer had no device token.")
                        .to_owned(),
                ));
            }
            Ok(Paired { token, church: reply.church_name(), church_id: reply.church.id.filter(Id::is_safe) })
        }
        429 => Err(trouble(status, &body, wait)),
        400..=499 => Err(Problem::Other(
            crate::i18n::t(
                "integrations.timelychurch.code_refused",
                "TimelyChurch didn't take that pairing code. Make a new one in TimelyChurch (Setlists › Song Sync) and type it here.",
            )
            .to_owned(),
        )),
        _ => Err(trouble(status, &body, wait)),
    }
}

/// The services between two dates (`YYYY-MM-DD`). With `etag`, the list last seen: an
/// unchanged list is only a short answer.
pub fn services(base: &str, token: &str, from: &str, to: &str, etag: Option<&str>) -> Result<Services, Problem> {
    let headers: Vec<(&str, &str)> = etag.map(|etag| ("If-None-Match", etag)).into_iter().collect();
    let (status, body, wait, given) = call(base, Some(token), "GET", &wire::services_path(from, to), &headers, None)?;
    match status {
        304 => Ok(Services::Unchanged),
        200 => match ServicesReply::read(&body) {
            Some(list) => Ok(Services::Fresh { list, body, etag: given }),
            None => Err(Problem::Other(
                crate::i18n::t("integrations.timelychurch.no_services", "TimelyChurch's answer didn't list services.")
                    .to_owned(),
            )),
        },
        _ => Err(trouble(status, &body, wait)),
    }
}

/// A song in an arrangement (none: its default), and its body as sent (kept, so the song can
/// be made again later with no network). `None` when TimelyChurch no longer has it (404).
pub fn song(base: &str, token: &str, id: &Id, arrangement: Option<&Id>) -> Result<Option<(Song, String)>, Problem> {
    if !id.is_safe() || arrangement.is_some_and(|a| !a.is_safe()) {
        return Ok(None);
    }
    let (status, body, wait, _) = call(base, Some(token), "GET", &wire::song_path(id, arrangement), &[], None)?;
    match status {
        200 => match serde_json::from_str::<Song>(&body) {
            Ok(song) if !song.id.is_empty() => Ok(Some((song, body))),
            _ => Err(Problem::Other(
                crate::i18n::t("integrations.timelychurch.no_song", "TimelyChurch's answer wasn't a song.").to_owned(),
            )),
        },
        404 | 410 => Ok(None),
        _ => Err(trouble(status, &body, wait)),
    }
}

/// What sending a song use came to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Sent {
    /// TimelyChurch has it (now, or from an earlier try with the same key).
    Recorded,
    /// TimelyChurch won't ever take it as it is (it doesn't know the song, say): it is let go,
    /// so it isn't sent again and again.
    Refused(String),
}

/// Tells TimelyChurch a song was shown, once: `key` is the use's own idempotency key, the same
/// on every try, so a retry never counts twice.
pub fn song_use(base: &str, token: &str, used: &SongUse, key: &str) -> Result<Sent, Problem> {
    let body = serde_json::to_string(used).unwrap_or_default();
    let headers = [("Idempotency-Key", key)];
    let (status, body, wait, _) = call(base, Some(token), "POST", &wire::song_uses_path(), &headers, Some(body))?;
    match status {
        200..=299 => Ok(Sent::Recorded),
        // The same key already taken (some servers answer a replay so).
        409 => Ok(Sent::Recorded),
        400 | 404 | 410 | 422 => Ok(Sent::Refused(match trouble(status, &body, wait) {
            Problem::Other(words) => words,
            _ => status.to_string(),
        })),
        _ => Err(trouble(status, &body, wait)),
    }
}

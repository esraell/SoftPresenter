//! Fetching a song pack from the source, asking for as little as it can: a language already
//! here is asked for with the ETag and Last-Modified the server gave last time, and the server
//! answers "not modified" with no body when nothing changed. Everything that comes back is
//! untrusted: the size is capped, a body shorter than the server said is "cut short", and
//! nothing but plain `https` is followed (a test copy may be pointed at a local `http` address).

use std::io::Read;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use super::store::valid_language;
use crate::i18n::{t, tf};

/// Where the source publishes its free song packs: the language's code and `.qsp` are added.
const DOWNLOADS: &str = "https://repo.worshipleaderapp.com/website/quelea";

/// The largest pack that will be fetched.
pub const LARGEST: u64 = super::read::LIMITS.file;

/// The address packs are fetched from. A testing aid can name another with
/// `SOFTPRESENTER_CATALOG_URL`; a pack from it is read, bounded and checked all the same.
pub fn base_url() -> String {
    // Testing aid: SOFTPRESENTER_CATALOG_URL=<address> fetches the song packs from there (a
    // local test server) instead of the source's own address; `http` is allowed for it.
    std::env::var("SOFTPRESENTER_CATALOG_URL")
        .ok()
        .filter(|url| !url.is_empty())
        .unwrap_or_else(|| DOWNLOADS.to_owned())
}

/// The address of a language's pack.
pub fn pack_url(base: &str, lang: &str) -> Result<String, String> {
    if !valid_language(lang) {
        return Err(format!("\u{201c}{lang}\u{201d} isn't a language code (two or three small letters, such as ru)."));
    }
    Ok(format!("{}/{lang}.qsp", base.trim_end_matches('/')))
}

/// How far a download has got, for a window to show: bytes so far, and bytes in all when the
/// server said.
#[derive(Debug, Default)]
pub struct Progress {
    pub done: AtomicU64,
    pub total: AtomicU64,
}

impl Progress {
    pub fn new() -> Arc<Progress> {
        Arc::new(Progress::default())
    }

    /// Per cent done, when the total is known.
    pub fn per_cent(&self) -> Option<u8> {
        let total = self.total.load(Ordering::Relaxed);
        (total > 0).then(|| (self.done.load(Ordering::Relaxed).saturating_mul(100) / total).min(100) as u8)
    }
}

/// What the server said to a request for a pack.
#[derive(Debug, PartialEq, Eq)]
pub enum Reply {
    /// Nothing new since the validators given.
    Unchanged,
    /// The pack, with the validators the server gave it.
    Pack { bytes: Vec<u8>, etag: Option<String>, last_modified: Option<String> },
}

/// What was learnt of a pack already here: the server's validators.
#[derive(Clone, Debug, Default)]
pub struct Have {
    pub etag: Option<String>,
    pub last_modified: Option<String>,
}

fn agent(base: &str) -> ureq::Agent {
    // Only a test copy pointed at this computer may use plain http.
    let local = base.starts_with("http://127.0.0.1") || base.starts_with("http://localhost");
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(900)))
        .timeout_connect(Some(Duration::from_secs(20)))
        .https_only(!local)
        .max_redirects(3)
        .build()
        .into()
}

fn header(response: &ureq::http::Response<ureq::Body>, name: &str) -> Option<String> {
    response
        .headers()
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.trim().to_owned())
        .filter(|v| !v.is_empty())
}

/// Asks for a pack, once more if the first try fails on the network. With validators in `have`,
/// the server may answer that nothing changed.
pub fn get(base: &str, lang: &str, have: &Have, progress: Option<&Progress>) -> Result<Reply, String> {
    let url = pack_url(base, lang)?;
    get_once(base, &url, have, progress).or_else(|first| match first {
        // A refusal by the server isn't tried again; a network failure is, once.
        Failed::Server(words) => Err(words),
        Failed::Network(_) => get_once(base, &url, have, progress).map_err(Failed::words),
    })
}

/// Why one try failed.
enum Failed {
    Server(String),
    Network(String),
}

impl Failed {
    fn words(self) -> String {
        match self {
            Failed::Server(words) | Failed::Network(words) => words,
        }
    }
}

fn get_once(base: &str, url: &str, have: &Have, progress: Option<&Progress>) -> Result<Reply, Failed> {
    let offline = |e: ureq::Error| match e {
        ureq::Error::StatusCode(code) => Failed::Server(tf(
            "notices.catalog.answered",
            "The song source answered {code}, not the songs.",
            &[("code", &code.to_string())],
        )),
        other => Failed::Network(tf(
            "notices.catalog.unreachable",
            "Could not reach the song source: {error}",
            &[("error", &other.to_string())],
        )),
    };
    // The pack as it is: asked for compressed, the source compresses it as it sends (a zip
    // gains nothing) and then gives no length, so no per cent and no check for a cut-short body.
    let mut request = agent(base).get(url).header("Accept-Encoding", "identity");
    if let Some(etag) = &have.etag {
        request = request.header("If-None-Match", etag);
    }
    if let Some(modified) = &have.last_modified {
        request = request.header("If-Modified-Since", modified);
    }
    let mut response = request.call().map_err(offline)?;
    let (etag, last_modified) = (header(&response, "etag"), header(&response, "last-modified"));
    if response.status().as_u16() == 304 {
        return Ok(Reply::Unchanged);
    }
    // A server that ignores the validators and sends the same file again is as good as "not
    // modified", without reading the body.
    if (have.etag.is_some() || have.last_modified.is_some()) && etag == have.etag && last_modified == have.last_modified
    {
        return Ok(Reply::Unchanged);
    }
    let expected = header(&response, "content-length").and_then(|value| value.parse::<u64>().ok());
    if expected.is_some_and(|expected| expected > LARGEST) {
        return Err(Failed::Server(
            t(
                "notices.catalog.too_big",
                "The song pack is bigger than a song pack should be, so it was not downloaded.",
            )
            .to_owned(),
        ));
    }
    if let (Some(progress), Some(expected)) = (progress, expected) {
        progress.total.store(expected, Ordering::Relaxed);
    }
    let mut reader = response.body_mut().with_config().limit(LARGEST).reader();
    let mut bytes = Vec::with_capacity(expected.unwrap_or(1 << 20).min(LARGEST) as usize);
    let mut chunk = [0_u8; 64 * 1024];
    loop {
        let read = reader.read(&mut chunk).map_err(|e| {
            Failed::Network(tf(
                "notices.catalog.stopped",
                "The download stopped: {error}",
                &[("error", &e.to_string())],
            ))
        })?;
        if read == 0 {
            break;
        }
        bytes.extend_from_slice(&chunk[..read]);
        if bytes.len() as u64 > LARGEST {
            return Err(Failed::Server(
                t(
                    "notices.catalog.too_big_kept",
                    "The song pack is bigger than a song pack should be, so it was not kept.",
                )
                .to_owned(),
            ));
        }
        if let Some(progress) = progress {
            progress.done.store(bytes.len() as u64, Ordering::Relaxed);
        }
    }
    if expected.is_some_and(|expected| expected != bytes.len() as u64) {
        return Err(Failed::Network(
            t("notices.catalog.cut_short", "The download was cut short. Try again.").to_owned(),
        ));
    }
    Ok(Reply::Pack { bytes, etag, last_modified })
}

/// How big a language's pack is, as the server says, without downloading it. `None` when the
/// server doesn't say. Asked for as it is: asked for compressed, the source gives no length.
pub fn size_of(base: &str, lang: &str) -> Result<Option<u64>, String> {
    let url = pack_url(base, lang)?;
    let response = agent(base).head(&url).header("Accept-Encoding", "identity").call().map_err(|e| match e {
        ureq::Error::StatusCode(404) => format!("There are no {lang} songs at the source."),
        other => format!("Could not reach the song source: {other}"),
    })?;
    Ok(header(&response, "content-length").and_then(|value| value.parse().ok()))
}

//! Web pages, drawn off-screen and copied into frames for the screens, as a video's are: a box
//! on a slide that shows a live web page, and (later) a VoxLive caption screen.
//!
//! A [`WebPage`] is made and driven on the main thread, like a video [`crate::Player`]; its
//! pictures are taken from a [`crate::Frames`], which can go to the thread drawing the screens.
//! Call [`WebPage::capture`] as often as new pictures are wanted (15 to 30 times a second); a
//! copy already on its way is not asked for again.
//!
//! What a page may do is kept small, because the address can come from anywhere (a song pack
//! made on another computer): only `https` pages (and plain `http` on this computer, for tests)
//! are loaded or followed, nothing is downloaded, no window opens, camera, microphone and other
//! permissions are refused, nothing is kept between pages (no cookies on disk), and nothing in
//! a page can call into the app.
//!
//! On macOS the system's web view (WebKit) draws the page in a window far off every screen.
//! Other systems have no web pages yet: [`WebPage::open`] says so.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::Frame;

/// The newest picture of a page, shared with the thread drawing the screens.
#[derive(Clone, Default)]
pub struct WebFrames(Arc<Mutex<Newest>>);

#[derive(Default)]
struct Newest {
    frame: Option<Frame>,
    /// The last picture kept, by its size and a fingerprint of its pixels, and when it came.
    #[cfg(any(target_os = "macos", test))]
    last: Option<(u32, u32, u64)>,
    changed_at: Option<Instant>,
}

impl WebFrames {
    /// Keeps a new picture, in place of one not yet taken; one the same as the last is let go,
    /// so a page that isn't moving (most caption pages, most of the time) isn't drawn again on
    /// every screen thirty times a second.
    #[cfg(any(target_os = "macos", test))]
    pub(crate) fn put(&self, frame: Frame) {
        let seen = (frame.width, frame.height, fingerprint(&frame.rgba));
        if let Ok(mut newest) = self.0.lock()
            && newest.last != Some(seen)
        {
            newest.last = Some(seen);
            newest.frame = Some(frame);
            newest.changed_at = Some(Instant::now());
        }
    }

    /// How long the page's picture has stayed the same (zero before the first).
    pub fn still_for(&self) -> Duration {
        self.0.lock().ok().and_then(|newest| newest.changed_at).map_or(Duration::ZERO, |at| at.elapsed())
    }

    /// The newest picture, if one has come since the last was taken.
    pub fn next(&self) -> Option<Frame> {
        self.0.lock().ok().and_then(|mut newest| newest.frame.take())
    }

    /// True when both take pictures from the same page.
    pub fn same(&self, other: &WebFrames) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

/// A fingerprint of a picture's pixels (FNV-1a over eight bytes at a time): fast, and two
/// different pictures all but never share one.
#[cfg(any(target_os = "macos", test))]
fn fingerprint(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let (words, rest) = bytes.as_chunks::<8>();
    for word in words {
        hash ^= u64::from_le_bytes(*word);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    for &byte in rest {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// How far a page has got.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Loading {
    Loading,
    Loaded,
    /// It couldn't be loaded (no network, a bad address, a refused one).
    Failed,
}

/// True for an address a page may be loaded from or go to: `https`, or plain `http` on this
/// computer (127.0.0.1, localhost, [::1]). No name and password in it.
pub fn allowed(address: &str) -> bool {
    let Some((scheme, rest)) = address.split_once("://") else { return false };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    if authority.is_empty() || authority.contains('@') || address.chars().any(char::is_control) {
        return false;
    }
    match scheme.to_ascii_lowercase().as_str() {
        "https" => true,
        "http" => loopback(authority),
        _ => false,
    }
}

/// Whether a page may be opened at an address of this computer's own (plain `http` on
/// 127.0.0.1, localhost or [::1]): a test's stand-in. Only such a page may then go on to this
/// computer's other addresses (see [`may_go`]).
pub fn on_this_computer(address: &str) -> bool {
    let Some((scheme, rest)) = address.split_once("://") else { return false };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    scheme.eq_ignore_ascii_case("http") && allowed(address) && loopback(authority)
}

/// Whether a page, or a frame in it, may go to a place, by the place's scheme and host as the
/// web view reads them: `https` anywhere; plain `http` on this computer only when the page was
/// opened at such an address (`opened_here`), so an https page can't redirect, post or frame
/// its way to a service on this computer and show what it says on the screens; and a frame
/// inside the page may also be blank or made by the page itself (`about:`, `data:`, `blob:`).
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn may_go(scheme: &str, host: &str, main: bool, opened_here: bool) -> bool {
    match scheme.to_ascii_lowercase().as_str() {
        "https" => !host.is_empty(),
        "http" => opened_here && loopback(host),
        "about" => true,
        "data" | "blob" => !main,
        _ => false,
    }
}

/// True for this computer's own address, with or without a port.
pub(crate) fn loopback(authority: &str) -> bool {
    let host = match authority.strip_prefix('[') {
        Some(v6) => v6.split(']').next().unwrap_or(""),
        None => authority.split(':').next().unwrap_or(""),
    };
    matches!(host.to_ascii_lowercase().as_str(), "127.0.0.1" | "localhost" | "::1")
}

/// A web page drawn off-screen at a size, in pixels.
pub struct WebPage {
    #[cfg(target_os = "macos")]
    page: crate::web_mac::Page,
    frames: WebFrames,
    address: String,
    size: (u32, u32),
}

impl WebPage {
    /// Opens a page at `address`, `width` × `height` pixels, with a see-through background
    /// where the page has none. On the main thread only. The error is a sentence for the
    /// operator.
    pub fn open(address: &str, width: u32, height: u32) -> Result<WebPage, String> {
        if !allowed(address) {
            return Err(format!("{address} isn't a web address SoftPresenter opens (it must start with https://)."));
        }
        let frames = WebFrames::default();
        #[cfg(target_os = "macos")]
        {
            let (width, height) = (width.clamp(64, 3840), height.clamp(64, 2160));
            let page = crate::web_mac::Page::open(address, width, height, frames.clone())?;
            Ok(WebPage { page, frames, address: address.to_owned(), size: (width, height) })
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = (frames, width, height);
            Err("Web pages on slides need macOS for now.".to_owned())
        }
    }

    /// How long the page has looked the same: a page that isn't moving can be looked at less
    /// often.
    pub fn still_for(&self) -> Duration {
        self.frames.still_for()
    }

    /// Where the page's pictures are taken from.
    pub fn frames(&self) -> crate::Frames {
        crate::Frames::Web(self.frames.clone())
    }

    /// The address it was opened at.
    pub fn address(&self) -> &str {
        &self.address
    }

    /// The size it is drawn at, in pixels.
    pub fn size(&self) -> (u32, u32) {
        self.size
    }

    /// Asks for a new picture of the page, unless one is on its way or the page hasn't loaded
    /// yet (so a half-drawn page is never shown).
    pub fn capture(&self) {
        #[cfg(target_os = "macos")]
        self.page.capture();
    }

    /// Lets the page run (shown) or rests it (not shown): resting, it all but stops (no
    /// animation, slow timers) and costs next to nothing, but stays loaded, and runs again at
    /// once when shown. Pictures of a resting page are slow to come. A page opens shown.
    pub fn set_shown(&self, shown: bool) {
        #[cfg(target_os = "macos")]
        self.page.set_shown(shown);
        #[cfg(not(target_os = "macos"))]
        let _ = shown;
    }

    /// How far the page has got.
    pub fn loading(&self) -> Loading {
        #[cfg(target_os = "macos")]
        return self.page.loading();
        #[cfg(not(target_os = "macos"))]
        Loading::Failed
    }

    /// Runs a script in the page, for its answer as text (a script should end in a string,
    /// such as `JSON.stringify(…)`). `reply` is called on the main thread, with `None` when
    /// there was no text.
    pub fn ask(&self, script: &str, reply: impl FnOnce(Option<String>) + 'static) {
        #[cfg(target_os = "macos")]
        self.page.ask(script, reply);
        #[cfg(not(target_os = "macos"))]
        {
            let _ = script;
            reply(None);
        }
    }
}

/// Lets the main thread's run loop work for a while (loading pages, taking their pictures).
/// For checks outside the app; the app's own event loop does this for it.
pub fn pump(seconds: f64) {
    #[cfg(target_os = "macos")]
    crate::web_mac::pump(seconds);
    #[cfg(not(target_os = "macos"))]
    let _ = seconds;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_https_pages_and_this_computers_own_are_opened() {
        for good in [
            "https://example.com",
            "https://example.com/page?x=1#top",
            "HTTPS://Example.com:8443/a",
            "http://127.0.0.1:8000/page.html",
            "http://localhost/test",
            "http://[::1]:9000/",
        ] {
            assert!(allowed(good), "{good}");
        }
        for bad in [
            "http://example.com",
            "file:///etc/passwd",
            "javascript:alert(1)",
            "ftp://example.com",
            "https://",
            "https://user:pass@example.com",
            "https://example.com/\nmore",
            "example.com",
            "http://127.0.0.1.evil.com/",
            "http://localhost.evil.com/",
            "facetime://someone",
        ] {
            assert!(!allowed(bad), "{bad}");
        }
    }

    #[test]
    fn an_https_page_can_never_go_on_to_this_computers_own_services() {
        // An https page redirecting, posting or framing its way to http on this computer is
        // stopped, in the main frame and in a frame inside it.
        for host in ["127.0.0.1", "localhost", "::1"] {
            assert!(!may_go("http", host, true, false), "{host}");
            assert!(!may_go("http", host, false, false), "{host}");
        }
        assert!(!may_go("http", "example.com", true, false));
        assert!(may_go("https", "example.com", true, false));
        assert!(!may_go("file", "", true, false));
        assert!(!may_go("data", "", true, false), "the page itself never becomes data:");
        assert!(may_go("data", "", false, false));
        // A test's stand-in, opened on this computer, may go on within it, and nowhere plain.
        assert!(may_go("http", "127.0.0.1", true, true));
        assert!(!may_go("http", "example.com", true, true));
        assert!(on_this_computer("http://127.0.0.1:8000/page.html"));
        assert!(on_this_computer("HTTP://localhost/test"));
        assert!(!on_this_computer("https://127.0.0.1/"));
        assert!(!on_this_computer("http://127.0.0.1.evil.com/"));
    }

    #[test]
    fn a_picture_not_yet_taken_gives_way_to_a_newer_one_and_is_taken_once() {
        let frames = WebFrames::default();
        assert!(frames.next().is_none());
        frames.put(Frame { width: 1, height: 1, rgba: vec![1, 2, 3, 4] });
        frames.put(Frame { width: 1, height: 1, rgba: vec![5, 6, 7, 8] });
        assert_eq!(frames.next().map(|frame| frame.rgba), Some(vec![5, 6, 7, 8]));
        assert!(frames.next().is_none(), "taken once");
        assert!(frames.same(&frames.clone()) && !frames.same(&WebFrames::default()));
    }

    #[test]
    fn a_picture_the_same_as_the_last_is_let_go() {
        let frames = WebFrames::default();
        let picture = |byte: u8| Frame { width: 3, height: 1, rgba: vec![byte; 12] };
        frames.put(picture(1));
        assert!(frames.next().is_some());
        frames.put(picture(1));
        assert!(frames.next().is_none(), "the same picture again isn't drawn again");
        frames.put(picture(2));
        frames.put(picture(1));
        assert_eq!(frames.next().map(|frame| frame.rgba[0]), Some(1), "a change, and a change back, each count");
        assert_ne!(fingerprint(&[0; 9]), fingerprint(&[0, 0, 0, 0, 0, 0, 0, 0, 1]));
    }
}

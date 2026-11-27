//! How long the screens keep what they showed for a new video's first picture.
//!
//! A video FFmpeg plays is opened on a thread of its own, and its first picture comes a moment
//! later (`sp_video::Player::ready`). Until then the screens keep what they showed (the picture,
//! or the earlier video still playing), so they go from the one to the other with nothing
//! between. But not for ever: if no picture has come [`LIMIT`] after the video was opened, the
//! screens stop waiting and show what they would without its pictures (its still, if it has one,
//! as for a video that can't play), and the operator is told once. The video isn't let go: if
//! its first picture comes after all, it goes on the screens as it comes, and the notice is taken
//! back. So the limit costs a slow video only its smooth change, never the video itself.

use std::time::{Duration, Instant};

/// How long the screens keep what they showed for a new video's first picture.
///
/// On the Windows PC the first picture of a first video came 0.4 to 0.7 s after it was asked
/// for, and FFmpeg took 2.2 s to open on a copy starved of the processor (another program
/// keeping every core busy, the app at Below Normal priority): ten seconds is four times the
/// slowest that worked, so a video that works never meets it. Past ten seconds the operator has
/// noticed and moved on, and the screens should follow what was chosen rather than keep an
/// earlier video playing. And meeting it costs a slow video only its smooth change: it still
/// goes on screen when its first picture comes (15 s and 26 s opens were seen there).
pub const LIMIT: Duration = Duration::from_secs(10);

/// How often a video whose wait is over and whose first picture still hasn't come is looked at
/// (its pictures go on screen without it; looking only takes back the notice): not fifty times
/// a second for as long as it stays on screen.
pub const LOOK_WHEN_LATE: Duration = Duration::from_millis(250);

/// [`LIMIT`], or the testing aid's.
pub fn limit() -> Duration {
    // Testing aid: SOFTPRESENTER_VIDEO_WAIT=<ms> makes the screens keep what they showed for at
    // most that many milliseconds while a new video's first picture is still to come, instead
    // of LIMIT, so a test sees what happens at the limit without waiting ten seconds.
    std::env::var("SOFTPRESENTER_VIDEO_WAIT")
        .ok()
        .and_then(|ms| ms.trim().parse::<u64>().ok())
        .map_or(LIMIT, Duration::from_millis)
}

/// Where the wait for one video's first picture is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    /// No picture yet, and the limit not reached: the screens keep what they showed.
    Waiting,
    /// The first picture came in time (or it is known that none will): the screens have it.
    Came,
    /// The limit passed with no picture: the screens went on without it.
    GaveUp,
    /// The first picture came after the limit (or it became known that none will).
    CameLate,
}

/// What changed when the wait was looked at again.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Change {
    /// Nothing: still within the limit with no picture, or already settled.
    Nothing,
    /// The first picture came within the limit, or it became known that none will: the screens
    /// go to the video (or to what they show for a video that can't play).
    Came,
    /// The limit passed with no picture: the screens go on without it, and the operator is
    /// told.
    GaveUp,
    /// The first picture came after the limit, or it became known that none will: it goes on
    /// screen as it comes, and the notice is taken back.
    CameLate,
}

/// The wait for one video's first picture, counted from when the video was opened: nothing that
/// happens after (the screens told something new, a slide changed) starts it again.
#[derive(Clone, Debug)]
pub struct FirstPicture {
    asked: Instant,
    limit: Duration,
    state: State,
    /// What the operator was told when the limit passed, to take back if the picture comes.
    pub notice: Option<String>,
}

impl FirstPicture {
    /// A wait for the first picture of a video opened at `asked`, for at most `limit`.
    pub fn new(asked: Instant, limit: Duration) -> FirstPicture {
        FirstPicture { asked, limit, state: State::Waiting, notice: None }
    }

    /// Whether the screens keep what they showed: no picture yet, and the limit not reached
    /// when last looked at ([`FirstPicture::look`]).
    pub fn keeps(&self) -> bool {
        self.state == State::Waiting
    }

    /// Whether the limit passed with no picture, and none has come since.
    pub fn overdue(&self) -> bool {
        self.state == State::GaveUp
    }

    /// How long since the video was asked for.
    pub fn waited(&self, now: Instant) -> Duration {
        now.saturating_duration_since(self.asked)
    }

    /// The limit it waits for.
    pub fn limit(&self) -> Duration {
        self.limit
    }

    /// Looks again: whether the video is `ready` (its first picture has come, or it is known
    /// that none will) at `now`, and what that changes.
    pub fn look(&mut self, ready: bool, now: Instant) -> Change {
        let (state, change) = match self.state {
            State::Waiting if ready => (State::Came, Change::Came),
            State::Waiting if self.waited(now) >= self.limit => (State::GaveUp, Change::GaveUp),
            State::GaveUp if ready => (State::CameLate, Change::CameLate),
            state => (state, Change::Nothing),
        };
        self.state = state;
        change
    }

    /// When to look again at the latest: at the limit while waiting, a while later once it
    /// has passed; `None` once settled.
    pub fn due(&self, now: Instant) -> Option<Instant> {
        match self.state {
            State::Waiting => Some(self.asked + self.limit),
            State::GaveUp => Some(now + LOOK_WHEN_LATE),
            State::Came | State::CameLate => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECOND: Duration = Duration::from_secs(1);

    /// Looks at the wait every 20 ms (as the window's loop does while a video opens) from
    /// `from` to `to` milliseconds after it began, the video `ready` from `came` on; says each
    /// change, and when (in milliseconds).
    fn watch(wait: &mut FirstPicture, began: Instant, (from, to): (u64, u64), came: Option<u64>) -> Vec<(u64, Change)> {
        (from..=to)
            .step_by(20)
            .filter_map(|at| {
                let ready = came.is_some_and(|came| at >= came);
                let change = wait.look(ready, began + Duration::from_millis(at));
                (change != Change::Nothing).then_some((at, change))
            })
            .collect()
    }

    /// A video whose first picture comes within the limit: the screens keep what they showed
    /// at every look until then, and go to the video once it has come; nothing is said.
    #[test]
    fn a_first_picture_in_time_is_waited_for() {
        let began = Instant::now();
        let mut wait = FirstPicture::new(began, LIMIT);
        let before = watch(&mut wait, began, (0, 3980), Some(4000));
        assert!(before.is_empty() && wait.keeps(), "the screens keep what they showed until it comes: {before:?}");
        let after = watch(&mut wait, began, (4000, 30_000), Some(4000));
        assert_eq!(after.len(), 1, "{after:?}");
        assert_eq!(after[0].1, Change::Came);
        assert!(!wait.keeps() && !wait.overdue());
        assert_eq!(wait.due(began + 5 * SECOND), None, "nothing more to look for");
    }

    /// The slow first open seen on the PC (about 15 s) with the limit at ten: the screens keep
    /// what they showed for ten seconds, then go on without it; when its picture comes after
    /// all, it is shown (the wait says so, once), and nothing more changes after.
    #[test]
    fn a_first_picture_after_the_limit_still_goes_on_screen() {
        let began = Instant::now();
        let mut wait = FirstPicture::new(began, LIMIT);
        let changes = watch(&mut wait, began, (0, 60_000), Some(15_500));
        assert_eq!(changes.iter().map(|(_, change)| *change).collect::<Vec<_>>(), [Change::GaveUp, Change::CameLate]);
        let (gave_up, came) = (changes[0].0, changes[1].0);
        assert_eq!(gave_up, 10_000, "the screens stopped waiting at the limit, not at {gave_up} ms");
        assert_eq!(came, 15_500, "it was shown as its picture came, not at {came} ms");
        assert!(!wait.keeps() && !wait.overdue());
    }

    /// A video that never gives a picture (nor fails): the screens keep what they showed until
    /// the limit and not a moment longer, the change is said once however often it is looked
    /// at, and it is looked at a few times a second after, not fifty.
    #[test]
    fn a_first_picture_that_never_comes_strands_nothing() {
        let began = Instant::now();
        let mut wait = FirstPicture::new(began, LIMIT);
        assert_eq!(wait.due(began), Some(began + LIMIT), "it is looked at by the limit at the latest");
        let changes = watch(&mut wait, began, (0, 600_000), None);
        assert_eq!(changes, [(10_000, Change::GaveUp)], "said once, at the limit");
        assert!(!wait.keeps(), "the screens no longer keep what they showed");
        assert!(wait.overdue());
        let now = began + 600 * SECOND;
        assert_eq!(wait.due(now), Some(now + LOOK_WHEN_LATE));
        // Even looked at only once, long after (a window's loop held up), it is over.
        let mut late = FirstPicture::new(began, LIMIT);
        assert_eq!(late.look(false, began + 3600 * SECOND), Change::GaveUp);
    }

    /// While a video opens the screens keep what they showed whatever else happens (each new
    /// slide tells the screens again, and looks at the wait each time): the wait counts from
    /// when the video was opened, so looking at it often, or seldom, never starts it again.
    #[test]
    fn the_old_picture_is_kept_meanwhile_and_looking_never_starts_the_wait_again() {
        let began = Instant::now();
        let mut wait = FirstPicture::new(began, Duration::from_secs(2));
        for at in [0, 0, 500, 500, 1000, 1999] {
            assert_eq!(wait.look(false, began + Duration::from_millis(at)), Change::Nothing);
            assert!(wait.keeps(), "kept at {at} ms");
        }
        assert_eq!(
            wait.look(false, began + 2 * SECOND),
            Change::GaveUp,
            "two seconds from the open, not the last look"
        );
        // A video known at once to be unplayable (FFmpeg said so) settles the wait as a picture
        // does: the screens go on to what they show for it, and nothing is said about the limit.
        let mut refused = FirstPicture::new(began, LIMIT);
        assert_eq!(refused.look(true, began + Duration::from_millis(300)), Change::Came);
        assert_eq!(refused.look(true, began + 20 * SECOND), Change::Nothing);
    }

    /// The testing aid shortens the limit; without it the limit is ten seconds.
    #[test]
    fn the_limit_is_ten_seconds() {
        assert_eq!(LIMIT, Duration::from_secs(10));
        if std::env::var_os("SOFTPRESENTER_VIDEO_WAIT").is_none() {
            assert_eq!(limit(), LIMIT);
        }
    }
}

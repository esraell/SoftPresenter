//! NDI sources in slides: finding them on the network, and receiving the ones a slide on screen
//! or being edited wants.
//!
//! Receiving starts when a slide with an NDI box goes on screen (the audience screens, the
//! layout behind it, a stage layout) or is opened in the editor, and stops when it has left
//! both: a slide that is merely in a list of slides receives nothing, and shows the small copy of
//! the last picture its source sent, if the app has seen one. Each source is received once, on
//! a thread of its own, however many boxes show it; the thread puts each new picture where every
//! drawing thread finds it (`sp_render::show_ndi`) and nothing is queued, so a picture is never
//! older than the one being drawn. Looking for sources runs only while a source is being chosen
//! (an NDI box selected in the editor) or being received, and stops a little after.

use std::time::{Duration, Instant};

use sp_model::{ObjectKind, ndi_short_name};

/// How long a source is given, from when it is first wanted, to show it is there before it is
/// called not sending.
const GRACE: Duration = Duration::from_secs(3);
/// How long the finder goes on looking after nothing needs it.
const FINDER_LINGER: Duration = Duration::from_secs(10);

/// What the inspector shows of NDI.
#[derive(Clone, Debug, Default)]
pub struct NdiView {
    /// The sources found, as NDI names them, without this app's own outputs.
    pub sources: Vec<String>,
    /// The finder could not run (NDI is not installed, or too old).
    pub unavailable: bool,
    /// The sources that are wanted and not sending.
    pub stopped: Vec<String>,
    /// Asked for by the Look again button; the app takes it.
    pub look_again: bool,
}

/// One source being received.
struct Receiving {
    source: String,
    light: bool,
    incoming: sp_ndi::Incoming,
    since: Instant,
    /// A picture has come since it started.
    had_picture: bool,
    /// The operator has been told it stopped, and it hasn't come back.
    told: bool,
}

/// What the app does with NDI sources in slides.
#[derive(Default)]
pub struct Inputs {
    finder: Option<sp_ndi::Finder>,
    finder_since: Option<Instant>,
    finder_wanted_until: Option<Instant>,
    receiving: Vec<Receiving>,
}

/// What a look at the sources found, then at what is wanted, brought.
#[derive(Default)]
pub struct Report {
    /// Sources that have just stopped sending.
    pub stopped_now: Vec<String>,
    /// Sources whose first picture has just come.
    pub first: Vec<String>,
    /// Sources no slide wants any more, which are no longer received.
    pub dropped: Vec<String>,
    /// Whether anything is being received.
    pub busy: bool,
}

/// An NDI source as the menu says it: `Camera 1 · STREAM-PC`.
pub fn menu_name(source: &str) -> String {
    match sp_model::ndi_computer(source) {
        Some(computer) => format!("{} \u{b7} {computer}", ndi_short_name(source)),
        None => source.to_owned(),
    }
}

/// The sources of the objects that are NDI boxes and shown, each once, with whether every box that
/// shows it wants the lighter stream.
pub fn wanted_in(objects: &[sp_model::Object], into: &mut Vec<(String, bool)>) {
    for object in objects.iter().filter(|object| !object.hidden) {
        let ObjectKind::Ndi { source, quality, .. } = &object.kind else { continue };
        if source.is_empty() {
            continue;
        }
        let light = *quality == sp_model::NdiQuality::Light;
        match into.iter_mut().find(|(have, _)| have == source) {
            Some(found) => found.1 &= light,
            None => into.push((source.clone(), light)),
        }
    }
}

/// The sources found, less the ones this app sends itself (a source shown in a slide would show
/// itself): a found name `COMPUTER (Name)` is one of ours when `Name` is the name of one of our
/// outputs.
pub fn offered(found: &[String], own: &[String]) -> Vec<String> {
    found.iter().filter(|source| !own.iter().any(|name| source.ends_with(&format!("({name})")))).cloned().collect()
}

/// What the line under the Source menu says without NDI: the NDI Tools the operator installs,
/// or, where the app downloads NDI itself (Linux), the button under it.
fn not_installed(app_downloads: bool) -> &'static str {
    if app_downloads {
        "NDI isn't installed on this computer. Download NDI\u{2026}, below, gets it (free, from NDI), for receiving and for sending."
    } else {
        "NDI isn't installed on this computer. SoftPresenter uses the NDI Tools you install yourself, as it does for sending."
    }
}

/// The line under the Source menu, and whether it is amber.
pub fn line(installed: bool, found: usize, source: &str, stopped: bool) -> (String, bool) {
    if !installed {
        return (not_installed(crate::ndi_download::offered()).to_owned(), false);
    }
    if !source.is_empty() && stopped {
        return (
            format!(
                "\u{201c}{}\u{201d} isn't sending now. On screen the slide shows without it until it is back.",
                ndi_short_name(source)
            ),
            true,
        );
    }
    let live = "The picture shows, live, while the slide is on screen.";
    if !source.is_empty() {
        return (live.to_owned(), false);
    }
    if found == 0 {
        return (
            "Looking for NDI on this network. A source shows here as soon as it starts sending.".to_owned(),
            false,
        );
    }
    (format!("{found} found on this network. {live}"), false)
}

/// What the operator is told when a source stops.
pub fn stopped_notice(source: &str) -> String {
    format!(
        "NDI \u{201c}{}\u{201d} stopped sending. Its slides show without it until it is back.",
        ndi_short_name(source)
    )
}

impl Inputs {
    /// Looks at the finder and at what is wanted: starts and stops receivers and the finder, and
    /// says what the inspector shows. `wanted` is each source to receive, `choosing` whether a
    /// source is being chosen (so the finder is wanted though nothing is received).
    pub fn sync(
        &mut self,
        wanted: &[(String, bool)],
        choosing: bool,
        own: &[String],
        now: Instant,
    ) -> (NdiView, Report) {
        let mut report = Report::default();
        // The finder: wanted while choosing or receiving, and for a while after.
        if choosing || !wanted.is_empty() {
            self.finder_wanted_until = Some(now + FINDER_LINGER);
        }
        let finder_wanted = self.finder_wanted_until.is_some_and(|until| until > now);
        if finder_wanted && self.finder.is_none() && sp_ndi::installed() {
            self.finder = Some(sp_ndi::Finder::start());
            self.finder_since = Some(now);
        } else if !finder_wanted && self.finder.is_some() {
            self.finder = None;
            self.finder_since = None;
        }
        let found = self.finder.as_ref().map(sp_ndi::Finder::sources).unwrap_or_default();
        let looked = self.finder_since.is_some_and(|since| now.duration_since(since) >= GRACE);
        let unavailable = self.finder.as_ref().is_some_and(|f| matches!(f.state(), sp_ndi::FinderState::Failed(_)));

        // Receivers: one for each source wanted, none for the rest.
        self.receiving.retain(|have| {
            let keep = wanted.iter().any(|(source, light)| *source == have.source && *light == have.light);
            if !keep {
                sp_render::set_ndi_receiving(&have.source, false, false);
                report.dropped.push(have.source.clone());
            }
            keep
        });
        for (source, light) in wanted {
            if self.receiving.iter().any(|have| have.source == *source) {
                continue;
            }
            let name = source.clone();
            let incoming = sp_ndi::Incoming::start(source, *light, move |video| {
                // The newest picture, copied once into a picture every drawing thread shares;
                // NDI's own copy is freed as this returns.
                let order = sp_render::PixelOrder::Rgba;
                if let Some(picture) = sp_render::Picture::from_rows(
                    video.width,
                    video.height,
                    video.rows,
                    video.bytes_per_row,
                    order,
                    !video.alpha,
                ) {
                    sp_render::show_ndi(&name, picture);
                }
            });
            self.receiving.push(Receiving {
                source: source.clone(),
                light: *light,
                incoming,
                since: now,
                had_picture: false,
                told: false,
            });
            sp_render::set_ndi_receiving(source, true, false);
        }

        let mut stopped = Vec::new();
        for receiving in &mut self.receiving {
            let state = receiving.incoming.state();
            let sending = state == sp_ndi::IncomingState::Receiving;
            let in_grace = now.duration_since(receiving.since) < GRACE;
            // Not in the list of sources found (once the finder has had its time) counts the
            // same as one that has gone quiet.
            let missing = !in_grace
                && (matches!(state, sp_ndi::IncomingState::Waiting | sp_ndi::IncomingState::Failed(_))
                    || (looked && !found.contains(&receiving.source) && !sending));
            sp_render::set_ndi_receiving(&receiving.source, true, missing);
            if sending && !receiving.had_picture {
                receiving.had_picture = true;
                report.first.push(receiving.source.clone());
            }
            let quiet = sp_render::ndi_quiet(&receiving.source).is_some_and(|quiet| quiet >= sp_render::NDI_HOLD);
            let is_stopped = missing || (!in_grace && quiet);
            if is_stopped {
                stopped.push(receiving.source.clone());
                if receiving.had_picture && !receiving.told {
                    receiving.told = true;
                    report.stopped_now.push(receiving.source.clone());
                }
            } else if sending && !quiet {
                receiving.told = false;
            }
        }
        report.busy = !self.receiving.is_empty();
        let view = NdiView { sources: offered(&found, own), unavailable, stopped, look_again: false };
        (view, report)
    }

    /// Starts looking afresh, as the Look again button asks.
    pub fn look_again(&mut self) {
        if let Some(finder) = &self.finder {
            finder.look_again();
            self.finder_since = Some(Instant::now());
        }
    }

    /// True while anything is being received.
    pub fn receiving_any(&self) -> bool {
        !self.receiving.is_empty()
    }

    /// The sources being received now, for the log and the tests.
    pub fn receiving_sources(&self) -> Vec<String> {
        self.receiving.iter().map(|r| r.source.clone()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ndi_object(source: &str, quality: sp_model::NdiQuality) -> sp_model::Object {
        let mut object = sp_model::Object::ndi(source.to_owned());
        if let ObjectKind::Ndi { quality: q, .. } = &mut object.kind {
            *q = quality;
        }
        object
    }

    #[test]
    fn one_source_in_two_boxes_is_wanted_once_and_light_only_if_both_are() {
        let mut wanted = Vec::new();
        let objects = [
            ndi_object("PC (Camera 1)", sp_model::NdiQuality::Light),
            ndi_object("PC (Camera 1)", sp_model::NdiQuality::Full),
            ndi_object("", sp_model::NdiQuality::Full),
            ndi_object("PC (Camera 2)", sp_model::NdiQuality::Light),
        ];
        wanted_in(&objects, &mut wanted);
        assert_eq!(wanted, [("PC (Camera 1)".to_owned(), false), ("PC (Camera 2)".to_owned(), true)]);
        // A hidden box wants nothing.
        let mut hidden = ndi_object("PC (Camera 3)", sp_model::NdiQuality::Full);
        hidden.hidden = true;
        let mut again = Vec::new();
        wanted_in(&[hidden], &mut again);
        assert!(again.is_empty());
    }

    #[test]
    fn this_apps_own_outputs_are_never_offered() {
        let found = [
            "STAGE-PC (Camera 1)".to_owned(),
            "MAC (SoftPresenter)".to_owned(),
            "MacBook Pro (2) (Key)".to_owned(),
            "MAC (Not Key)".to_owned(),
        ];
        let own = ["SoftPresenter".to_owned(), "Key".to_owned()];
        assert_eq!(offered(&found, &own), ["STAGE-PC (Camera 1)", "MAC (Not Key)"]);
        assert_eq!(offered(&found, &[]).len(), 4);
    }

    #[test]
    fn the_line_under_the_menu_says_what_is_so_in_each_of_its_four_states() {
        let (words, amber) = line(true, 3, "", false);
        assert_eq!(words, "3 found on this network. The picture shows, live, while the slide is on screen.");
        assert!(!amber);
        let (words, amber) = line(true, 0, "", false);
        assert_eq!(words, "Looking for NDI on this network. A source shows here as soon as it starts sending.");
        assert!(!amber);
        let (words, amber) = line(false, 0, "", false);
        assert_eq!(words, not_installed(crate::ndi_download::offered()));
        assert!(!amber);
        // NDI Tools on a Mac and on Windows; on Linux, where NDI Tools doesn't exist, the app's
        // own download.
        assert!(not_installed(false).starts_with(
            "NDI isn't installed on this computer. SoftPresenter uses the NDI Tools you install yourself"
        ));
        assert!(not_installed(true).starts_with("NDI isn't installed on this computer. Download NDI\u{2026}"));
        let (words, amber) = line(true, 2, "STREAM-PC (Camera 1)", true);
        assert_eq!(
            words,
            "\u{201c}Camera 1\u{201d} isn't sending now. On screen the slide shows without it until it is back."
        );
        assert!(amber);
        // A source chosen and sending: just what the picture does.
        assert_eq!(
            line(true, 2, "STREAM-PC (Camera 1)", false).0,
            "The picture shows, live, while the slide is on screen."
        );
        // Without the runtime, a chosen source's trouble is still said first by the runtime's line.
        assert!(!line(false, 0, "STREAM-PC (Camera 1)", true).1);
    }

    #[test]
    fn the_menu_names_a_source_by_its_own_name_then_its_computer() {
        assert_eq!(menu_name("STREAM-PC (Camera 1)"), "Camera 1 \u{b7} STREAM-PC");
        assert_eq!(menu_name("odd"), "odd");
    }
}

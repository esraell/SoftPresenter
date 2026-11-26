//! The show: what is live, which song is selected, and the commands that change it.
//!
//! Headless and deterministic. A [`Command`] goes in; the new state and a [`Changes`] report
//! come out. Nothing here knows about windows or drawing.

use std::sync::Arc;

use sp_model::{MediaItem, Presentation, Slide};

#[cfg(test)]
mod tests;

/// Output is a stack of layers, each cleared on its own. Only `Slides` carries content so far.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layer {
    Audio,
    Media,
    Slides,
    Announcements,
    Props,
    Messages,
    Masks,
}

/// A slide, by the index of its presentation in the show and its flat index in that
/// presentation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SlideRef {
    pub presentation: usize,
    pub slide: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    /// Show another presentation's slides to the operator. Never changes what is live.
    Select(usize),
    GoTo(SlideRef),
    /// The slide at this place in a presentation's play order (see `Presentation::order`), for
    /// a group an arrangement plays more than once.
    GoToStep {
        presentation: usize,
        step: usize,
    },
    /// Show the media item with this index behind the words.
    ShowMedia(usize),
    Next,
    Previous,
    /// The first slide of the selected presentation.
    First,
    /// The last slide of the selected presentation.
    Last,
    Clear(Layer),
    ClearAll,
    /// Takes the live slide's words off the screens. The slide stays live, and its background
    /// with it. Show Again, a click on the slide, or another slide (or build) put on screen brings
    /// the words back; Next or Previous with nowhere to go leaves them hidden; a clear takes the
    /// slide off.
    HideWords,
    /// Shows hidden words again. With nothing live, puts back the slide the last clear took off,
    /// if no slide has gone up since and it is still there.
    ShowAgain,
    SetOutputEnabled(bool),
}

/// What a command changed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Changes {
    /// The slide layer shows something different.
    pub slides: bool,
    /// The media layer shows something different.
    pub media: bool,
    pub selection: bool,
    pub output_enabled: bool,
    /// The live slide went on to another of its builds (or back).
    pub builds: bool,
}

pub struct Show {
    presentations: Vec<Arc<Presentation>>,
    selected: usize,
    live: Option<SlideRef>,
    cursor: Option<SlideRef>,
    /// Where the cursor is in its presentation's play order. A slide whose group an
    /// arrangement plays twice is at two places; this says which.
    step: Option<usize>,
    /// How many of the live slide's click builds have been set going.
    build_step: usize,
    /// The live slide is up without its words; see [`Show::words_hidden`].
    words_hidden: bool,
    /// The id of the slide the last clear took off, until a slide goes up. The cursor keeps its
    /// place (some edits move it with its slide, others keep the index); the id says whether that
    /// place still holds the slide, so a song or passage put in its place never shows a slide
    /// that was never shown.
    taken_off: Option<sp_model::Uuid>,
    media: Vec<MediaItem>,
    live_media: Option<usize>,
    output_enabled: bool,
    /// Changes each time a presentation is added or replaced; see [`Show::version`].
    version: u64,
}

/// The next number a show's presentations are given when they change, shared by every show so
/// that two shows never give the same number to different songs.
fn next_version() -> u64 {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

impl Show {
    /// Starts with the first presentation selected, nothing live and the output on.
    pub fn new(presentations: Vec<Arc<Presentation>>) -> Show {
        Show {
            presentations,
            selected: 0,
            live: None,
            cursor: None,
            step: None,
            build_step: 0,
            words_hidden: false,
            taken_off: None,
            media: Vec::new(),
            live_media: None,
            output_enabled: true,
            version: next_version(),
        }
    }

    /// A number that changes whenever a presentation is added or replaced, and only then: a
    /// list worked out from the presentations (titles, words) needs working out again exactly
    /// when this has changed since.
    pub fn version(&self) -> u64 {
        self.version
    }

    pub fn presentations(&self) -> &[Arc<Presentation>] {
        &self.presentations
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    pub fn live(&self) -> Option<SlideRef> {
        self.live
    }

    /// The live slide is up without its words (Hide the Words). Never true with nothing live.
    pub fn words_hidden(&self) -> bool {
        self.words_hidden
    }

    /// The last slide that was live. Kept after a clear so `Next` can continue from it.
    pub fn cursor(&self) -> Option<SlideRef> {
        self.cursor
    }

    /// How many of the live slide's click builds have been set going.
    pub fn build_step(&self) -> usize {
        self.build_step
    }

    /// The number of click builds the live slide has.
    fn build_steps(&self) -> usize {
        self.live_slide().map_or(0, |(_, slide)| slide.click_steps().len())
    }

    /// True when Next would set another of the live slide's builds going, not change slides.
    pub fn builds_left(&self) -> bool {
        self.live.is_some() && self.build_step < self.build_steps()
    }

    /// Where the cursor is in its presentation's play order, and how long that order is.
    pub fn position(&self) -> Option<(usize, usize)> {
        let cursor = self.cursor?;
        Some((self.step?, self.order(cursor.presentation).len()))
    }

    /// The slide `Next` would put on screen after the cursor (the live slide, or the last one
    /// live), in the play order; `None` at the end of the song.
    pub fn upcoming(&self) -> Option<SlideRef> {
        let cursor = self.cursor?;
        let (at, step) = self.next_target()?;
        (at != cursor || step != self.step).then_some(at)
    }

    /// The slide `Next` would put on screen, as `apply` does it: the one after the cursor in the
    /// play order (a song that loops goes round), the selected presentation's first with nothing
    /// shown yet, or, after a clear at the end of a song, the slide last on screen again. `None`
    /// when Next would put no slide on: it sets the live slide's next build going instead, or
    /// the slide on screen is the last.
    pub fn next_on_screen(&self) -> Option<SlideRef> {
        if self.builds_left() {
            return None;
        }
        let (at, _) = self.next_target()?;
        let shown = at.slide < self.slide_count(at.presentation) && self.enabled(at);
        (shown && self.live != Some(at)).then_some(at)
    }

    /// The slide `Previous` would put on screen before the cursor, in the play order; `None`
    /// at the start of the song. A click build to take back doesn't count: see `build_step`.
    pub fn preceding(&self) -> Option<SlideRef> {
        let cursor = self.cursor?;
        let (at, step) = self.previous_target()?;
        (at != cursor || step != self.step).then_some(at)
    }

    /// A presentation's slides, by their places, in the order they are played.
    fn order(&self, presentation: usize) -> Vec<usize> {
        self.presentations.get(presentation).map_or_else(Vec::new, |song| song.order())
    }

    /// The place of a slide in its presentation's play order: the first place at or after the
    /// cursor's (in the same presentation), otherwise its first place.
    fn step_of(&self, at: SlideRef) -> Option<usize> {
        let order = self.order(at.presentation);
        let from = self.step.filter(|_| self.cursor.is_some_and(|cursor| cursor.presentation == at.presentation));
        let from = from.unwrap_or(0).min(order.len());
        order[from..]
            .iter()
            .position(|slide| *slide == at.slide)
            .map(|offset| from + offset)
            .or_else(|| order.iter().position(|slide| *slide == at.slide))
    }

    pub fn live_slide(&self) -> Option<(&Arc<Presentation>, &Slide)> {
        let at = self.live?;
        let presentation = self.presentations.get(at.presentation)?;
        let (_, slide) = presentation.slide(at.slide)?;
        Some((presentation, slide))
    }

    pub fn output_enabled(&self) -> bool {
        self.output_enabled
    }

    /// The media that can be shown, in the order it was added.
    pub fn media(&self) -> &[MediaItem] {
        &self.media
    }

    /// The index of the media item on screen, if any.
    pub fn live_media(&self) -> Option<usize> {
        self.live_media
    }

    /// Adds a presentation after the others and returns its index. Nothing on screen changes.
    pub fn add_presentation(&mut self, presentation: Arc<Presentation>) -> usize {
        self.presentations.push(presentation);
        self.version = next_version();
        self.presentations.len() - 1
    }

    /// Adds a media item to the end of the list and returns its index. It is not shown.
    pub fn add_media(&mut self, item: MediaItem) -> usize {
        self.media.push(item);
        self.media.len() - 1
    }

    /// Changes how a media item is shown. If it is on screen, the screens show the change.
    pub fn set_media_look(&mut self, index: usize, fit: sp_model::ImageFit, looping: bool) -> Changes {
        let Some(item) = self.media.get_mut(index) else { return Changes::default() };
        if item.fit == fit && item.looping == looping {
            return Changes::default();
        }
        item.fit = fit;
        item.looping = looping;
        Changes { media: self.live_media == Some(index), ..Changes::default() }
    }

    /// Gives a media item a new file (a new version of it, put in its place), keeping its id and
    /// how it is shown. Nothing on screen changes here: a video playing goes on as it is (the
    /// player is told of the new file, and takes it over at the end of its pass).
    pub fn set_media_file(&mut self, index: usize, name: String, path: std::path::PathBuf) {
        if let Some(item) = self.media.get_mut(index) {
            item.name = name;
            item.path = path;
        }
    }

    /// Takes a picture out of the media bin. If it was on screen it is taken off.
    pub fn remove_media(&mut self, index: usize) -> Changes {
        if index >= self.media.len() {
            return Changes::default();
        }
        self.media.remove(index);
        let was = self.live_media;
        self.live_media = match self.live_media {
            Some(live) if live == index => None,
            Some(live) if live > index => Some(live - 1),
            other => other,
        };
        Changes { media: was != self.live_media && self.live_media.is_none(), selection: true, ..Changes::default() }
    }

    /// The first slide of the first group, in the presentation in use, whose name is one of
    /// those given (in lower case). The presentation in use is the one with the slide last on
    /// screen, or the selected one if nothing has been shown.
    /// The slide given this hot key in the presentation the operator is working in, if one has
    /// it and it is not disabled.
    pub fn hot_key_slide(&self, key: char) -> Option<SlideRef> {
        let presentation = self.live.or(self.cursor).map_or(self.selected, |at| at.presentation);
        let song = self.presentations.get(presentation)?;
        let key = key.to_ascii_uppercase();
        let slide = song
            .groups
            .iter()
            .flat_map(|group| &group.slides)
            .position(|one| one.enabled && one.hot_key == Some(key))?;
        Some(SlideRef { presentation, slide })
    }

    pub fn group_start(&self, names: &[&str]) -> Option<SlideRef> {
        let presentation = self.live.or(self.cursor).map_or(self.selected, |at| at.presentation);
        let song = self.presentations.get(presentation)?;
        let mut slide = 0;
        for group in &song.groups {
            // The group's first slide that is not disabled.
            // A part named in the church's own language ("Припев") answers its English key.
            if names.contains(&sp_model::group_name_in_english(&group.name).as_str())
                && let Some(skip) = group.slides.iter().position(|one| one.enabled)
            {
                return Some(SlideRef { presentation, slide: slide + skip });
            }
            slide += group.slides.len();
        }
        None
    }

    /// True when any layer has something on screen.
    pub fn anything_live(&self) -> bool {
        self.live.is_some() || self.live_media.is_some()
    }

    /// Puts an edited presentation in place of the one at `index`. A live slide stays live if
    /// the new presentation still has a slide at that position; otherwise the slide layer is
    /// cleared. Reports a slide change when the presentation was the live one.
    pub fn replace_presentation(&mut self, index: usize, presentation: Arc<Presentation>) -> Changes {
        let was = self.cursor_occurrence(index);
        let Some(slot) = self.presentations.get_mut(index) else { return Changes::default() };
        let count = presentation.slide_count();
        *slot = presentation;
        self.version = next_version();
        if self.cursor.is_some_and(|c| c.presentation == index && c.slide >= count) {
            self.cursor = None;
        }
        if let Some(cursor) = self.cursor.filter(|c| c.presentation == index) {
            self.step = self.nth_place(cursor, was);
        }
        match self.live {
            Some(live) if live.presentation == index => {
                if live.slide >= count {
                    self.live = None;
                    self.words_hidden = false;
                }
                Changes { slides: true, ..Changes::default() }
            }
            _ => Changes::default(),
        }
    }

    /// Puts a whole new version of a presentation in place, keeping what is live and the cursor
    /// on the same slides (found by their ids) wherever they now are. A live slide the new
    /// version doesn't have is taken off the screen. Reports a slide change when the
    /// presentation was the live one.
    pub fn replace_presentation_following(&mut self, index: usize, presentation: Arc<Presentation>) -> Changes {
        let Some(old) = self.presentations.get(index).cloned() else { return Changes::default() };
        let id_at = |place: Option<SlideRef>| {
            place
                .filter(|here| here.presentation == index)
                .and_then(|here| old.slide(here.slide))
                .map(|(_, slide)| slide.id)
        };
        let (live_id, cursor_id) = (id_at(self.live), id_at(self.cursor));
        let place_of = |id: sp_model::Uuid| {
            presentation.groups.iter().flat_map(|group| &group.slides).position(|slide| slide.id == id)
        };
        let (live_at, cursor_at) = (live_id.map(place_of), cursor_id.map(place_of));
        let was = self.cursor_occurrence(index);
        let changes = self.replace_presentation(index, presentation);
        if let (Some(found), Some(live)) = (live_at, self.live.as_mut().filter(|live| live.presentation == index)) {
            match found {
                Some(slide) => live.slide = slide,
                None => {
                    self.live = None;
                    self.words_hidden = false;
                }
            }
        }
        if let (Some(found), Some(cursor)) = (cursor_at, self.cursor.as_mut().filter(|c| c.presentation == index)) {
            match found {
                Some(slide) => cursor.slide = slide,
                None => self.cursor = None,
            }
        }
        self.step = self.cursor.filter(|c| c.presentation == index).and_then(|cursor| self.nth_place(cursor, was));
        changes
    }

    /// Which time the cursor's slide is played, counting from 0, in a presentation's play
    /// order: the second chorus is 1. `None` when the cursor isn't in that presentation.
    fn cursor_occurrence(&self, presentation: usize) -> Option<usize> {
        let cursor = self.cursor.filter(|c| c.presentation == presentation)?;
        let order = self.order(presentation);
        let step = self.cursor_step(cursor, &order)?;
        Some(order[..step].iter().filter(|slide| **slide == cursor.slide).count())
    }

    /// The place in the play order where a slide is played for the given time (or its last
    /// time, if it isn't played that often), or its first place if no time is given.
    fn nth_place(&self, at: SlideRef, occurrence: Option<usize>) -> Option<usize> {
        let order = self.order(at.presentation);
        let places: Vec<usize> =
            order.iter().enumerate().filter(|(_, slide)| **slide == at.slide).map(|(step, _)| step).collect();
        places.get(occurrence.unwrap_or(0)).or(places.last()).copied()
    }

    /// Keeps what is live and the cursor on the same slides after a slide was removed from, or
    /// added to, a song: `at` is where, `added` says which. A removed slide that was live is
    /// taken off the screen.
    /// After a presentation's slides were put in another order (`new_of_old[old]` is each
    /// slide's new place), the slide on screen and the cursor follow their slides.
    pub fn slides_reordered(&mut self, presentation: usize, new_of_old: &[usize]) {
        for place in [&mut self.live, &mut self.cursor] {
            if let Some(here) = place.as_mut().filter(|here| here.presentation == presentation)
                && let Some(new) = new_of_old.get(here.slide)
            {
                here.slide = *new;
            }
        }
        if self.cursor.is_some_and(|c| c.presentation == presentation) {
            self.refind_step();
        }
    }

    pub fn slides_shifted(&mut self, presentation: usize, at: usize, added: bool) -> Changes {
        let shift = |place: &mut Option<SlideRef>| {
            if let Some(here) = place.as_mut().filter(|here| here.presentation == presentation) {
                if added && here.slide >= at {
                    here.slide += 1;
                } else if !added && here.slide > at {
                    here.slide -= 1;
                } else if !added && here.slide == at {
                    *place = None;
                }
            }
        };
        let was = self.live;
        shift(&mut self.live);
        self.words_hidden &= self.live.is_some();
        shift(&mut self.cursor);
        if self.cursor.is_some_and(|c| c.presentation == presentation) {
            self.refind_step();
        }
        Changes { slides: was.is_some() && self.live.is_none(), ..Changes::default() }
    }

    pub fn apply(&mut self, command: Command) -> Changes {
        match command {
            Command::Select(index) => {
                if index < self.presentations.len() && index != self.selected {
                    self.selected = index;
                    return Changes { selection: true, ..Changes::default() };
                }
                Changes::default()
            }
            Command::GoTo(target) => self.go_to(Some((target, None))),
            Command::GoToStep { presentation, step } => {
                let slide = self.order(presentation).get(step).copied();
                self.go_to(slide.map(|slide| (SlideRef { presentation, slide }, Some(step))))
            }
            // A slide with builds waiting for a click sets the next one going first (and shows
            // its words, if they were hidden).
            Command::Next if self.builds_left() => {
                self.build_step += 1;
                Changes { builds: true, slides: std::mem::take(&mut self.words_hidden), ..Changes::default() }
            }
            Command::Next => self.step_to(self.next_target()),
            Command::Previous if self.live.is_some() && self.build_step > 0 => {
                self.build_step -= 1;
                Changes { builds: true, slides: std::mem::take(&mut self.words_hidden), ..Changes::default() }
            }
            Command::Previous => self.step_to(self.previous_target()),
            Command::First => self.go_to(self.edge_of_selected(false)),
            Command::Last => self.go_to(self.edge_of_selected(true)),
            Command::ShowMedia(index) => {
                if index < self.media.len() && self.live_media != Some(index) {
                    self.live_media = Some(index);
                    return Changes { media: true, ..Changes::default() };
                }
                Changes::default()
            }
            Command::Clear(Layer::Slides) => Changes { slides: self.take_slide_off(), ..Changes::default() },
            Command::Clear(Layer::Media) => Changes { media: self.live_media.take().is_some(), ..Changes::default() },
            Command::ClearAll => {
                Changes { slides: self.take_slide_off(), media: self.live_media.take().is_some(), ..Changes::default() }
            }
            Command::Clear(_) => Changes::default(),
            Command::HideWords => {
                if self.live.is_none() || self.words_hidden {
                    return Changes::default();
                }
                self.words_hidden = true;
                Changes { slides: true, ..Changes::default() }
            }
            Command::ShowAgain if self.words_hidden => {
                self.words_hidden = false;
                Changes { slides: true, ..Changes::default() }
            }
            Command::ShowAgain => self.show_again(),
            Command::SetOutputEnabled(enabled) => {
                let changed = enabled != self.output_enabled;
                self.output_enabled = enabled;
                Changes { output_enabled: changed, ..Changes::default() }
            }
        }
    }

    fn slide_count(&self, presentation: usize) -> usize {
        self.presentations.get(presentation).map_or(0, |p| p.slide_count())
    }

    /// Makes `target` live if it names a real slide that is not disabled, at the place in the
    /// play order given (or the one found for it). A slide already live only moves its place:
    /// what is on screen doesn't change, unless its words were hidden, which come back. A slide
    /// going up forgets the one a clear took off.
    fn go_to(&mut self, target: Option<(SlideRef, Option<usize>)>) -> Changes {
        let Some((target, step)) = target else {
            return Changes::default();
        };
        if target.slide >= self.slide_count(target.presentation) || !self.enabled(target) {
            return Changes::default();
        }
        let step = step.or_else(|| self.step_of(target));
        self.step = step;
        let unhidden = std::mem::take(&mut self.words_hidden);
        if self.live == Some(target) {
            self.cursor = Some(target);
            return Changes { slides: unhidden, ..Changes::default() };
        }
        self.live = Some(target);
        self.cursor = Some(target);
        self.build_step = 0;
        self.taken_off = None;
        Changes { slides: true, ..Changes::default() }
    }

    /// Takes the live slide off the screen (its words hidden or not), and keeps its id for Show
    /// Again. True when there was one; a clear with nothing up keeps the last one taken off.
    fn take_slide_off(&mut self) -> bool {
        self.words_hidden = false;
        let Some(off) = self.live.take() else { return false };
        self.taken_off = self.slide_id(off);
        true
    }

    /// Next or Previous to `target`, as `go_to`, except that with no other slide to go to (the
    /// end of the item, or the same slide again at its next place in the play order) hidden
    /// words stay hidden and nothing changes on screen: a stray Next, or a timer's, never brings
    /// back the last verse.
    fn step_to(&mut self, target: Option<(SlideRef, Option<usize>)>) -> Changes {
        let stays_hidden = self.words_hidden && target.is_some_and(|(at, _)| self.live == Some(at));
        let changes = self.go_to(target);
        if stays_hidden {
            self.words_hidden = true;
            return Changes::default();
        }
        changes
    }

    /// Puts back the slide the last clear took off, at the cursor's place, if that place still
    /// holds it and it can be shown; otherwise nothing, and it is forgotten either way. Next then
    /// carries on from it.
    fn show_again(&mut self) -> Changes {
        let Some(id) = self.taken_off.take() else { return Changes::default() };
        let back = self.cursor.filter(|at| self.live.is_none() && self.slide_id(*at) == Some(id));
        self.go_to(back.map(|at| (at, None)))
    }

    /// The id of the slide at a place, if there is one.
    fn slide_id(&self, at: SlideRef) -> Option<sp_model::Uuid> {
        self.presentations.get(at.presentation)?.slide(at.slide).map(|(_, slide)| slide.id)
    }

    /// False for a slide that has been disabled: it stays in its song but is never shown.
    fn enabled(&self, at: SlideRef) -> bool {
        self.presentations
            .get(at.presentation)
            .and_then(|song| song.slide(at.slide))
            .is_some_and(|(_, slide)| slide.enabled)
    }

    /// Where the cursor is in its play order: its step if that still names its slide,
    /// otherwise the first place the slide is played.
    fn cursor_step(&self, cursor: SlideRef, order: &[usize]) -> Option<usize> {
        self.step
            .filter(|step| order.get(*step) == Some(&cursor.slide))
            .or_else(|| order.iter().position(|slide| *slide == cursor.slide))
    }

    /// The next slide in the play order after the cursor that is not disabled, with its place.
    /// At the end of the song it stays where it is, so after a clear the last slide can be
    /// shown again. A cursor on a slide the arrangement leaves out goes on as written.
    fn next_target(&self) -> Option<(SlideRef, Option<usize>)> {
        let Some(cursor) = self.cursor else { return self.edge_of_selected(false) };
        let order = self.order(cursor.presentation);
        // A song that loops goes back to its first slide after its last.
        let looped = self
            .presentations
            .get(cursor.presentation)
            .filter(|song| song.loops)
            .and_then(|_| {
                (0..order.len())
                    .map(|step| (SlideRef { slide: order[step], ..cursor }, Some(step)))
                    .find(|(at, _)| self.enabled(*at))
            })
            .filter(|(_, step)| *step != self.step);
        let stay = looped.or(Some((cursor, self.step)));
        match self.cursor_step(cursor, &order) {
            Some(here) => (here + 1..order.len())
                .map(|step| (SlideRef { slide: order[step], ..cursor }, Some(step)))
                .find(|(at, _)| self.enabled(*at))
                .or(stay),
            None => (cursor.slide + 1..self.slide_count(cursor.presentation))
                .map(|slide| (SlideRef { slide, ..cursor }, None))
                .find(|(at, _)| self.enabled(*at))
                .or(stay),
        }
    }

    /// The slide before the cursor in the play order that is not disabled; at the start of the
    /// song it stays.
    fn previous_target(&self) -> Option<(SlideRef, Option<usize>)> {
        let cursor = self.cursor?;
        let order = self.order(cursor.presentation);
        let stay = Some((cursor, self.step));
        match self.cursor_step(cursor, &order) {
            Some(here) => (0..here)
                .rev()
                .map(|step| (SlideRef { slide: order[step], ..cursor }, Some(step)))
                .find(|(at, _)| self.enabled(*at))
                .or(stay),
            None => (0..cursor.slide)
                .rev()
                .map(|slide| (SlideRef { slide, ..cursor }, None))
                .find(|(at, _)| self.enabled(*at))
                .or(stay),
        }
    }

    /// The selected song's first (or last) slide in its play order that is not disabled.
    fn edge_of_selected(&self, last: bool) -> Option<(SlideRef, Option<usize>)> {
        let order = self.order(self.selected);
        let at = |step: usize| (SlideRef { presentation: self.selected, slide: order[step] }, Some(step));
        let enabled = |(at, _): &(SlideRef, Option<usize>)| self.enabled(*at);
        if last { (0..order.len()).rev().map(at).find(enabled) } else { (0..order.len()).map(at).find(enabled) }
    }

    /// Finds the cursor's place in the play order again, after its presentation changed.
    fn refind_step(&mut self) {
        self.step = self.cursor.and_then(|cursor| self.step_of(cursor));
    }
}

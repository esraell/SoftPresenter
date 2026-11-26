//! Songs from the `.pro` documents ProPresenter 7 saves, so a church's library can move to
//! SoftPresenter in one go. Only ever read, never written.
//!
//! The documents are Protocol Buffers. The field numbers used here come from the unofficial,
//! MIT-licensed definitions published by greyshirtguy (github.com/greyshirtguy/ProPresenter7-Proto,
//! see docs/licences/), which their author worked out as a user of the program; nothing here
//! comes from the program itself. Only what a song needs is read: its name, its groups and
//! their slides' words, slide labels, notes and which slides are switched off, its
//! arrangements, and its copyright details; and, for a bundle that carries them, the picture or
//! video each slide shows (where the document says its file is). Everything else in the
//! document is passed over.
//!
//! A slide's text boxes are told apart by what they hold, since their names are whatever a
//! church typed: the box set in the largest type across the song is the main words, the next
//! the translation, and a line such as "Psalm 133:3" the reference (the one in the main words'
//! script, when there are two). Empty boxes are left out.

use std::collections::HashMap;

use crate::{
    Arrangement, Copyright, Group, Object, Presentation, Reason, Role, Slide, Uuid, group_color, rtf::rtf_largest_font,
    rtf_to_text,
};

/// The largest document read. A song is far smaller; anything bigger is refused unread.
pub const LARGEST_PRO_FILE: usize = 64 * 1024 * 1024;

/// One field of a Protocol Buffers message: its number and its value.
enum Value<'a> {
    Number(u64),
    Bytes(&'a [u8]),
    /// A fixed-width number; nothing read here uses one.
    Fixed,
}

/// The fields of one message, in order. A damaged message ends the reading with an error.
struct Fields<'a> {
    bytes: &'a [u8],
    at: usize,
}

const DAMAGED: Reason = Reason::DocumentDamaged;

impl<'a> Fields<'a> {
    fn of(bytes: &'a [u8]) -> Fields<'a> {
        Fields { bytes, at: 0 }
    }

    fn varint(&mut self) -> Result<u64, Reason> {
        let mut value = 0u64;
        for shift in (0..64).step_by(7) {
            let byte = *self.bytes.get(self.at).ok_or(DAMAGED)?;
            self.at += 1;
            value |= u64::from(byte & 0x7F) << shift;
            if byte & 0x80 == 0 {
                return Ok(value);
            }
        }
        Err(DAMAGED)
    }
}

impl<'a> Iterator for Fields<'a> {
    type Item = Result<(u64, Value<'a>), Reason>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.at >= self.bytes.len() {
            return None;
        }
        let read = (|| {
            let key = self.varint()?;
            let value = match key & 7 {
                0 => Value::Number(self.varint()?),
                1 | 5 => {
                    let width = if key & 7 == 1 { 8 } else { 4 };
                    self.at = self.at.checked_add(width).filter(|&end| end <= self.bytes.len()).ok_or(DAMAGED)?;
                    Value::Fixed
                }
                2 => {
                    let length = usize::try_from(self.varint()?).map_err(|_| DAMAGED)?;
                    let end = self.at.checked_add(length).filter(|&end| end <= self.bytes.len()).ok_or(DAMAGED)?;
                    let bytes = &self.bytes[self.at..end];
                    self.at = end;
                    Value::Bytes(bytes)
                }
                _ => return Err(DAMAGED),
            };
            Ok((key >> 3, value))
        })();
        if read.is_err() {
            // Stop after the first damage rather than reading on from a wrong place.
            self.at = self.bytes.len();
        }
        Some(read)
    }
}

/// The message or text in a field, or empty for a number.
fn bytes_of<'a>(value: &Value<'a>) -> &'a [u8] {
    match value {
        Value::Bytes(bytes) => bytes,
        _ => &[],
    }
}

fn text_of(value: &Value) -> String {
    String::from_utf8_lossy(bytes_of(value)).into_owned()
}

fn number_of(value: &Value) -> u64 {
    match value {
        Value::Number(n) => *n,
        _ => 0,
    }
}

/// The text of the first field with this number in a message (a UUID message's string, say).
fn first_text(message: &[u8], field: u64) -> Result<String, Reason> {
    for read in Fields::of(message) {
        let (number, value) = read?;
        if number == field {
            return Ok(text_of(&value));
        }
    }
    Ok(String::new())
}

/// A slide as read: its text boxes' words, label, notes, whether it is switched on, and the
/// picture or video it shows.
#[derive(Default)]
struct Cue {
    /// Each text box's words, and the largest type in it (in half points), in the slide's order.
    words: Vec<(String, Option<u32>)>,
    label: Option<String>,
    notes: Option<String>,
    /// `None` when the document doesn't say.
    enabled: Option<bool>,
    media: Option<ProMedia>,
}

/// A picture or video a slide of a `.pro` document shows, as the document names its file.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProMedia {
    /// The ways the document names the file, each possibly empty: its address (a `file://`
    /// address on the computer it was made on, or a bare file name in a bundle), its path from
    /// a known folder there, and a path relative to the document.
    pub names: Vec<String>,
    pub video: bool,
    /// A video set to play again from the start when it ends.
    pub looping: bool,
}

/// A song read from a `.pro` document, with the picture or video each of its slides shows (by
/// the slide's id), for a bundle that carries those files.
#[derive(Debug)]
pub struct ProDocument {
    pub song: Presentation,
    pub media: Vec<(Uuid, ProMedia)>,
    /// The name the document carries inside it (or its CCLI title), empty when it has none.
    /// Songs used to be named by it; see [`read_pro_file`] for why the file's name comes first.
    pub own_name: String,
}

/// Reads a song from a `.pro` document. `file_title` is the title its file's name gives (its
/// name without the ending; empty when there is none). The song is named by its file when that
/// name has a letter or digit in it. In the libraries seen, the files' names were the ones kept
/// right (hymn numbers and all), while the name kept inside a document was often its first line
/// as first typed (comma and all), a name garbled long ago, or another song's. The document's
/// own name is used when the file gives none. A document with pictures or videos but no words is refused, as its
/// slides would come across empty.
pub fn read_pro_file(bytes: &[u8], file_title: &str) -> Result<Presentation, Reason> {
    let document = read_pro_document(bytes, file_title)?;
    let has_words = |slide: &Slide| slide.objects.iter().any(|o| o.as_text().is_some_and(|t| !t.trim().is_empty()));
    if !document.song.groups.iter().flat_map(|g| &g.slides).any(has_words) {
        return Err(Reason::OnlyPictures);
    }
    Ok(document.song)
}

/// Reads a song from a `.pro` document with the pictures and videos its slides show, for a
/// bundle that carries their files, named as [`read_pro_file`] names it. A document with
/// neither words nor pictures is refused.
pub fn read_pro_document(bytes: &[u8], file_title: &str) -> Result<ProDocument, Reason> {
    if bytes.len() > LARGEST_PRO_FILE {
        return Err(Reason::DocumentTooLarge);
    }
    read(bytes, file_title)
}

fn read(bytes: &[u8], file_title: &str) -> Result<ProDocument, Reason> {
    let mut name = String::new();
    let mut selected = String::new();
    let mut arrangements: Vec<(String, String, Vec<String>)> = Vec::new();
    let mut groups: Vec<(String, String, Vec<String>)> = Vec::new();
    let mut cues: HashMap<String, Cue> = HashMap::new();
    let mut cue_order: Vec<String> = Vec::new();
    let mut copyright = Copyright::default();
    let mut ccli_title = String::new();
    for read in Fields::of(bytes) {
        let (number, value) = read?;
        match number {
            3 => name = text_of(&value),
            10 => selected = first_text(bytes_of(&value), 1)?,
            11 => {
                let (mut id, mut arrangement_name, mut ids) = (String::new(), String::new(), Vec::new());
                for read in Fields::of(bytes_of(&value)) {
                    match read? {
                        (1, value) => id = first_text(bytes_of(&value), 1)?,
                        (2, value) => arrangement_name = text_of(&value),
                        (3, value) => ids.push(first_text(bytes_of(&value), 1)?),
                        _ => {}
                    }
                }
                arrangements.push((id, arrangement_name, ids));
            }
            12 => {
                let (mut id, mut group_name, mut cue_ids) = (String::new(), String::new(), Vec::new());
                for read in Fields::of(bytes_of(&value)) {
                    match read? {
                        (1, group) => {
                            for read in Fields::of(bytes_of(&group)) {
                                match read? {
                                    (1, value) => id = first_text(bytes_of(&value), 1)?,
                                    (2, value) => group_name = text_of(&value),
                                    _ => {}
                                }
                            }
                        }
                        (2, value) => cue_ids.push(first_text(bytes_of(&value), 1)?),
                        _ => {}
                    }
                }
                groups.push((id, group_name, cue_ids));
            }
            13 => {
                let (id, cue) = read_cue(bytes_of(&value))?;
                cue_order.push(id.clone());
                cues.insert(id, cue);
            }
            14 => {
                for read in Fields::of(bytes_of(&value)) {
                    match read? {
                        (1, value) => copyright.author = text_of(&value),
                        (2, value) if copyright.artist.is_empty() => copyright.artist = text_of(&value),
                        (3, value) => ccli_title = text_of(&value),
                        (4, value) => copyright.publisher = text_of(&value),
                        (5, value) if number_of(&value) > 0 => copyright.year = number_of(&value).to_string(),
                        (6, value) if number_of(&value) > 0 => copyright.ccli_song = number_of(&value).to_string(),
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    // Whether slides are switched on is written only for the ones that are, in some documents,
    // and not at all in others; with no slide saying, every slide is on.
    let says = cues.values().any(|cue| cue.enabled.is_some());
    let mut song_groups = Vec::new();
    let mut group_ids: HashMap<String, Uuid> = HashMap::new();
    let mut placed: std::collections::HashSet<String> = std::collections::HashSet::new();
    let ranking = rank_positions(cues.values());
    let main_cyrillic = main_script(cues.values(), &ranking);
    let mut media = Vec::new();
    let mut make_slide = |cue: &Cue| {
        let mut slide = Slide::new(slide_objects(&cue.words, &ranking, main_cyrillic));
        slide.label = cue.label.clone();
        slide.notes = cue.notes.clone();
        slide.enabled = !says || cue.enabled.unwrap_or(false);
        if let Some(shown) = &cue.media {
            media.push((slide.id, shown.clone()));
        }
        slide
    };
    for (id, group_name, cue_ids) in &groups {
        let slides: Vec<Slide> = cue_ids.iter().filter_map(|cue| cues.get(cue)).map(&mut make_slide).collect();
        placed.extend(cue_ids.iter().cloned());
        if slides.is_empty() {
            continue;
        }
        let group = Group { id: Uuid::new_v4(), name: group_name.clone(), color: group_color(group_name), slides };
        group_ids.insert(id.clone(), group.id);
        song_groups.push(group);
    }
    // Slides in no group, if a document has any, come last.
    let loose: Vec<Slide> = cue_order
        .iter()
        .filter(|id| !placed.contains(*id))
        .filter_map(|id| cues.get(id))
        .map(&mut make_slide)
        .collect();
    if !loose.is_empty() {
        song_groups.push(Group { id: Uuid::new_v4(), name: String::new(), color: group_color(""), slides: loose });
    }
    let mut song_arrangements = Vec::new();
    let mut chosen = None;
    for (id, arrangement_name, ids) in &arrangements {
        let order: Vec<Uuid> = ids.iter().filter_map(|id| group_ids.get(id).copied()).collect();
        if order.is_empty() {
            continue;
        }
        let arrangement =
            Arrangement { id: Uuid::new_v4(), name: arrangement_name.clone(), groups: order, auto: false };
        if !selected.is_empty() && *id == selected {
            chosen = Some(arrangement.id);
        }
        song_arrangements.push(arrangement);
    }
    if song_groups.is_empty() {
        return Err(Reason::NoSlides);
    }
    let has_words = |slide: &Slide| slide.objects.iter().any(|o| o.as_text().is_some_and(|t| !t.trim().is_empty()));
    if media.is_empty() && !song_groups.iter().flat_map(|g| &g.slides).any(has_words) {
        return Err(Reason::NoWordsOrPictures);
    }
    let own_name = [name.trim(), ccli_title.trim()].into_iter().find(|t| !t.is_empty()).unwrap_or_default().to_owned();
    let file_title = file_title.trim();
    let title =
        if file_title.chars().any(char::is_alphanumeric) || own_name.is_empty() { file_title } else { &own_name };
    let title = if title.is_empty() { "Untitled" } else { title };
    let song = Presentation {
        id: Uuid::new_v4(),
        title: title.to_owned(),
        groups: song_groups,
        copyright,
        theme: None,
        arrangements: song_arrangements,
        arrangement: chosen,
        loops: false,
        transition: None,
        catalog: None,
    };
    Ok(ProDocument { song, media, own_name })
}

/// The positions of a song's text boxes, main words first. Each position is ranked by the size
/// of type it holds most often in the song: that is the size it was set in, where one slide can
/// mislead (long words shrunk to fit, a title slide in big type). Equal sizes keep their order.
/// Ranking once for the whole song keeps the main words in one language from slide to slide.
fn rank_positions<'a>(cues: impl Iterator<Item = &'a Cue>) -> Vec<usize> {
    let mut sizes: Vec<HashMap<u32, usize>> = Vec::new();
    for cue in cues {
        for (at, (text, size)) in cue.words.iter().enumerate() {
            if text.trim().is_empty() || is_reference(text) {
                continue;
            }
            if sizes.len() <= at {
                sizes.resize_with(at + 1, HashMap::new);
            }
            *sizes[at].entry(size.unwrap_or(0)).or_default() += 1;
        }
    }
    // The size each position holds most often; of two as often, the larger.
    let usual: Vec<Option<u32>> = sizes
        .iter()
        .map(|counts| counts.iter().max_by_key(|&(size, count)| (*count, *size)).map(|(size, _)| *size))
        .collect();
    let mut order: Vec<usize> = (0..usual.len()).filter(|&at| usual[at].is_some()).collect();
    order.sort_by_key(|&at| (std::cmp::Reverse(usual[at]), at));
    order
}

/// Whether a song's main words are in Cyrillic letters: what the main box holds on most of the
/// slides that have words in two scripts. `None` when no slide has both.
fn main_script<'a>(cues: impl Iterator<Item = &'a Cue>, ranking: &[usize]) -> Option<bool> {
    let (mut cyrillic, mut other) = (0, 0);
    for cue in cues {
        let words = ranked_words(&cue.words, ranking);
        let scripts: Vec<bool> = words.iter().map(|(_, text)| is_cyrillic(text)).collect();
        if scripts.contains(&true) && scripts.contains(&false) {
            if scripts[0] { cyrillic += 1 } else { other += 1 }
        }
    }
    (cyrillic + other > 0).then_some(cyrillic >= other)
}

/// A slide's word boxes (filled, not references), by their place, in the song's ranking.
fn ranked_words<'a>(boxes: &'a [(String, Option<u32>)], ranking: &[usize]) -> Vec<(usize, &'a str)> {
    let rank = |at: usize| ranking.iter().position(|&place| place == at).unwrap_or(usize::MAX);
    let mut words: Vec<(usize, &str)> = boxes
        .iter()
        .enumerate()
        .filter(|(_, (text, _))| !text.trim().is_empty() && !is_reference(text))
        .map(|(at, (text, _))| (at, text.as_str()))
        .collect();
    words.sort_by_key(|&(at, _)| (rank(at), at));
    words
}

/// A slide's text objects from its boxes' words, in the song's ranking of positions: the first
/// filled position as the main words, the next as the translation, and a reference line as the
/// reference (the one in the main words' script, when there are two). On a slide whose boxes
/// hold the two languages the other way round from the rest of the song (`main_cyrillic`), the
/// words in the song's main language are still the main words.
fn slide_objects(boxes: &[(String, Option<u32>)], ranking: &[usize], main_cyrillic: Option<bool>) -> Vec<Object> {
    let references: Vec<(usize, &str)> = boxes
        .iter()
        .enumerate()
        .filter(|(_, (text, _))| !text.trim().is_empty() && is_reference(text))
        .map(|(at, (text, _))| (at, text.as_str()))
        .collect();
    let mut words = ranked_words(boxes, ranking);
    if let Some(main) = main_cyrillic
        && words.first().is_some_and(|(_, text)| is_cyrillic(text) != main)
        && let Some(at) = words.iter().position(|(_, text)| is_cyrillic(text) == main)
    {
        let found = words.remove(at);
        words.insert(0, found);
    }
    // A slide with only a reference on it shows that as its words.
    let (words, references) = if words.is_empty() { (references, Vec::new()) } else { (words, references) };
    let mut objects = vec![Object::text(Role::primary(), words.first().map_or("", |(_, t)| t).to_owned())];
    if let Some((_, translation)) = words.get(1) {
        objects.push(Object::text(Role::translation(), (*translation).to_owned()));
    }
    let main_cyrillic = words.first().is_some_and(|(_, t)| is_cyrillic(t));
    let reference = references.iter().find(|(_, t)| is_cyrillic(t) == main_cyrillic).or(references.first());
    if let Some((_, reference)) = reference {
        objects.push(Object::text(Role::reference(), reference.trim().to_owned()));
    }
    objects
}

/// True for words in Cyrillic letters.
fn is_cyrillic(text: &str) -> bool {
    text.chars().any(|c| ('\u{0400}'..='\u{04FF}').contains(&c))
}

/// True for a line saying where a passage is from: a book, then chapter and verse ("Psalm
/// 133:3", "1 John 4:7-8", "Псалтирь 132:3").
fn is_reference(text: &str) -> bool {
    let text = text.trim();
    if text.contains('\n') || text.chars().count() > 40 {
        return false;
    }
    let Some((book, place)) = text.rsplit_once(' ') else { return false };
    let place_fits = place.starts_with(|c: char| c.is_ascii_digit())
        && place.contains(':')
        && place.chars().all(|c| c.is_ascii_digit() || ":,.-–".contains(c));
    let book = book.trim();
    let book = match book.split_once(' ') {
        Some((number, rest)) if number.len() == 1 && number.chars().all(|c| c.is_ascii_digit()) => rest,
        _ => book,
    };
    let letters = book.chars().filter(|c| c.is_alphabetic()).count();
    place_fits && letters >= 2 && book.chars().all(|c| c.is_alphabetic() || " .'’".contains(c))
}

/// One slide (a cue) and its id.
fn read_cue(message: &[u8]) -> Result<(String, Cue), Reason> {
    let mut id = String::new();
    let mut cue = Cue::default();
    for read in Fields::of(message) {
        match read? {
            (1, value) => id = first_text(bytes_of(&value), 1)?,
            (12, value) => cue.enabled = Some(number_of(&value) != 0),
            (10, action) => read_action(bytes_of(&action), &mut cue)?,
            _ => {}
        }
    }
    Ok((id, cue))
}

/// The parts of a cue's action that make a slide: the slide's words and notes, and its label,
/// or the picture or video it shows (the first, if it has several). Only a slide's own label
/// is kept: a picture's or a video's action is labelled with its file name.
fn read_action(message: &[u8], cue: &mut Cue) -> Result<(), Reason> {
    let mut label = String::new();
    let mut is_slide = false;
    for read in Fields::of(message) {
        match read? {
            (3, value) => label = first_text(bytes_of(&value), 2)?,
            // The slide, then the slide shown to the audience.
            (23, slide) => {
                is_slide = true;
                for read in Fields::of(bytes_of(&slide)) {
                    if let (2, presentation_slide) = read? {
                        read_presentation_slide(bytes_of(&presentation_slide), cue)?;
                    }
                }
            }
            (20, media) if cue.media.is_none() => cue.media = read_media(bytes_of(&media))?,
            _ => {}
        }
    }
    if is_slide && !label.trim().is_empty() {
        cue.label = Some(label.trim().to_owned());
    }
    Ok(())
}

/// A media action's picture or video: where its file is, and whether a video plays again when
/// it ends. `None` for a sound, a live camera or anything else.
fn read_media(message: &[u8]) -> Result<Option<ProMedia>, Reason> {
    let (mut names, mut kind) = (Vec::new(), None);
    for read in Fields::of(message) {
        match read? {
            // The file: its address, by every name the document gives it.
            (5, element) => {
                for read in Fields::of(bytes_of(&element)) {
                    if let (2, url) = read? {
                        names = url_names(bytes_of(&url))?;
                    }
                }
            }
            (6, _) => kind = Some((false, false)),
            (7, video) => {
                let mut looping = false;
                for read in Fields::of(bytes_of(&video)) {
                    if let (1, behaviour) = read? {
                        looping = number_of(&behaviour) == 1;
                    }
                }
                kind = Some((true, looping));
            }
            _ => {}
        }
    }
    Ok(kind.filter(|_| names.iter().any(|name| !name.trim().is_empty())).map(|(video, looping)| ProMedia {
        names,
        video,
        looping,
    }))
}

/// The names an address gives its file: the address itself, its path from a known folder,
/// and a path relative to the document, in that order (any may be empty).
fn url_names(message: &[u8]) -> Result<Vec<String>, Reason> {
    let (mut address, mut local, mut relative) = (String::new(), String::new(), String::new());
    for read in Fields::of(message) {
        match read? {
            (1, value) => address = text_of(&value),
            (2, value) => relative = text_of(&value),
            (4, value) => local = first_text(bytes_of(&value), 2)?,
            _ => {}
        }
    }
    Ok(vec![address, local, relative])
}

fn read_presentation_slide(message: &[u8], cue: &mut Cue) -> Result<(), Reason> {
    for read in Fields::of(message) {
        match read? {
            (1, base) => {
                for read in Fields::of(bytes_of(&base)) {
                    if let (1, element) = read?
                        && let Some(words) = element_words(bytes_of(&element))?
                    {
                        cue.words.push(words);
                    }
                }
            }
            (2, notes) => {
                let text = rtf_to_text(first_bytes(bytes_of(&notes), 1)?);
                if !text.trim().is_empty() {
                    cue.notes = Some(text);
                }
            }
            _ => {}
        }
    }
    Ok(())
}

/// The words of one element on a slide, and its largest type, when it is a text box that isn't
/// hidden.
fn element_words(message: &[u8]) -> Result<Option<(String, Option<u32>)>, Reason> {
    for read in Fields::of(message) {
        if let (1, graphics) = read? {
            let (mut hidden, mut rtf) = (false, None);
            for read in Fields::of(bytes_of(&graphics)) {
                match read? {
                    (16, value) => hidden = number_of(&value) != 0,
                    (13, text) => rtf = Some(first_bytes(bytes_of(&text), 5)?),
                    _ => {}
                }
            }
            return Ok(rtf.filter(|_| !hidden).map(|rtf| (rtf_to_text(rtf), rtf_largest_font(rtf))));
        }
    }
    Ok(None)
}

/// The bytes of the first field with this number in a message.
fn first_bytes(message: &[u8], field: u64) -> Result<&[u8], Reason> {
    for read in Fields::of(message) {
        let (number, value) = read?;
        if number == field {
            return Ok(bytes_of(&value));
        }
    }
    Ok(&[])
}

/// `.pro` documents written for tests, with text written for them: only the fields the reader
/// reads, laid out as the reader expects. Not part of the app.
#[cfg(any(test, feature = "test-documents"))]
#[doc(hidden)]
pub mod example {
    /// Writes Protocol Buffers messages, for documents made up in the tests.
    pub fn varint(mut n: u64, out: &mut Vec<u8>) {
        loop {
            let byte = (n & 0x7F) as u8;
            n >>= 7;
            if n == 0 {
                out.push(byte);
                return;
            }
            out.push(byte | 0x80);
        }
    }

    pub fn field(number: u64, bytes: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        varint(number << 3 | 2, &mut out);
        varint(bytes.len() as u64, &mut out);
        out.extend_from_slice(bytes);
        out
    }

    pub fn number(number: u64, value: u64) -> Vec<u8> {
        let mut out = Vec::new();
        varint(number << 3, &mut out);
        varint(value, &mut out);
        out
    }

    pub fn uuid(id: &str) -> Vec<u8> {
        field(1, id.as_bytes())
    }

    pub fn text_box(rtf: &str) -> Vec<u8> {
        let graphics = [field(2, b"Box"), field(13, &field(5, rtf.as_bytes()))].concat();
        field(1, &field(1, &graphics))
    }

    pub fn cue(id: &str, boxes: &[&str], label: &str, enabled: bool) -> Vec<u8> {
        let base: Vec<u8> = boxes.iter().flat_map(|b| text_box(b)).collect();
        let presentation_slide = [field(1, &base), field(2, &field(1, br"{\rtf1 Wait for the band}"))].concat();
        let action =
            [field(3, &field(2, label.as_bytes())), number(9, 11), field(23, &field(2, &presentation_slide))].concat();
        [field(1, &uuid(id)), field(10, &action), if enabled { number(12, 1) } else { Vec::new() }].concat()
    }

    pub fn group(id: &str, name: &str, cues: &[&str]) -> Vec<u8> {
        let mut out = field(1, &[field(1, &uuid(id)), field(2, name.as_bytes())].concat());
        for cue in cues {
            out.extend(field(2, &uuid(cue)));
        }
        out
    }

    /// A song made up for this test: two groups, one two-language slide, one switched off,
    /// an arrangement chosen, and its details.
    pub fn document() -> Vec<u8> {
        let arrangement = [
            field(1, &uuid("arr")),
            field(2, b"Short"),
            field(3, &uuid("g2")),
            field(3, &uuid("g1")),
            field(3, &uuid("g2")),
        ]
        .concat();
        let ccli = [field(1, b"Anna Writer"), field(4, b"Harbour Music"), number(5, 2021), number(6, 1234567)].concat();
        [
            field(1, b"application info, passed over"),
            field(3, b"Lantern Song"),
            field(10, &uuid("arr")),
            field(11, &arrangement),
            field(12, &group("g1", "Verse 1", &["c1", "c2"])),
            field(12, &group("g2", "Chorus", &["c3"])),
            field(
                13,
                &cue(
                    "c1",
                    &[
                        r"{\rtf1\ansi A light along the shore\par A road we walked}",
                        r"{\rtf1\ansi\uc1 \u1057?\u1074?\u1077?\u1090?}",
                    ],
                    "",
                    true,
                ),
            ),
            field(13, &cue("c2", &[r"{\rtf1 Home again}"], "Quiet", false)),
            field(13, &cue("c3", &[r"{\rtf1 Shine on}"], "", true)),
            field(14, &ccli),
        ]
        .concat()
    }

    /// A media action showing a picture, or a video (looping or not), at an address (and a
    /// path from a known folder, when not empty), labelled with the file's name as documents
    /// label them.
    pub fn media_action(address: &str, local: &str, video: Option<bool>) -> Vec<u8> {
        let mut url = field(1, address.as_bytes());
        if !local.is_empty() {
            url.extend(field(4, &[number(1, 12), field(2, local.as_bytes())].concat()));
        }
        let element = [field(1, &uuid("media")), field(2, &url)].concat();
        let kind = match video {
            None => field(6, &[]),
            Some(looping) => field(7, &number(1, u64::from(looping))),
        };
        let name = address.rsplit('/').next().unwrap_or(address);
        let media = [field(5, &element), kind].concat();
        [field(3, &field(2, name.as_bytes())), number(9, 2), field(20, &media)].concat()
    }

    /// A slide with words and, after its slide action, the extra actions given.
    pub fn cue_showing(id: &str, words: &str, actions: &[Vec<u8>]) -> Vec<u8> {
        let mut out = cue(id, &[words], "", true);
        for action in actions {
            out.extend(field(10, action));
        }
        out
    }

    /// "Harbour Lights", made up for tests: one verse whose slides each show one of the
    /// pictures or videos given (an address, a path from a known folder, and `None` for a
    /// picture or whether a video loops), then a slide showing nothing.
    pub fn song_showing(media: &[(&str, &str, Option<bool>)]) -> Vec<u8> {
        titled_song_showing("Harbour Lights", media)
    }

    /// The same, under another title.
    pub fn titled_song_showing(title: &str, media: &[(&str, &str, Option<bool>)]) -> Vec<u8> {
        let ids: Vec<String> = (0..=media.len()).map(|n| format!("s{n}")).collect();
        let names: Vec<&str> = ids.iter().map(String::as_str).collect();
        let mut out = [field(3, title.as_bytes()), field(12, &group("g", "Verse 1", &names))].concat();
        for (at, (address, local, video)) in media.iter().enumerate() {
            let words = format!(r"{{\rtf1 Lamp number {at}}}");
            out.extend(field(13, &cue_showing(&ids[at], &words, &[media_action(address, local, *video)])));
        }
        out.extend(field(13, &cue_showing(&ids[media.len()], r"{\rtf1 The harbour sleeps}", &[])));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::example::*;
    use super::*;

    #[test]
    fn a_song_comes_with_its_groups_words_translation_labels_and_details() {
        let song = read_pro_file(&document(), "").unwrap();
        assert_eq!(song.title, "Lantern Song", "a file with no name: the document's own");
        let names: Vec<&str> = song.groups.iter().map(|g| g.name.as_str()).collect();
        assert_eq!(names, ["Verse 1", "Chorus"]);
        let first = &song.groups[0].slides[0];
        assert_eq!(first.text_for(&Role::primary()), Some("A light along the shore\nA road we walked"));
        assert_eq!(first.text_for(&Role::translation()), Some("Свет"));
        assert_eq!(first.notes.as_deref(), Some("Wait for the band"));
        let second = &song.groups[0].slides[1];
        assert_eq!(second.label.as_deref(), Some("Quiet"));
        assert!(!second.enabled, "the slide switched off stays off");
        assert!(song.groups[1].slides[0].enabled);
        assert_eq!(song.copyright.author, "Anna Writer");
        assert_eq!(song.copyright.publisher, "Harbour Music");
        assert_eq!((song.copyright.year.as_str(), song.copyright.ccli_song.as_str()), ("2021", "1234567"));
    }

    #[test]
    fn the_arrangement_comes_too_and_stays_chosen() {
        let song = read_pro_file(&document(), "file").unwrap();
        assert_eq!(song.arrangements.len(), 1);
        let arrangement = &song.arrangements[0];
        assert_eq!(arrangement.name, "Short");
        let chorus = song.groups[1].id;
        let verse = song.groups[0].id;
        assert_eq!(arrangement.groups, vec![chorus, verse, chorus]);
        assert_eq!(song.arrangement, Some(arrangement.id));
    }

    #[test]
    fn with_no_slide_saying_whether_it_is_on_every_slide_is_on() {
        let doc =
            [field(12, &group("g", "Verse", &["a"])), field(13, &cue("a", &[r"{\rtf1 Words}"], "", false))].concat();
        let song = read_pro_file(&doc, "Fallback name").unwrap();
        assert!(song.groups[0].slides[0].enabled);
        assert_eq!(song.title, "Fallback name");
    }

    #[test]
    fn a_song_is_named_by_its_file_and_by_the_name_inside_when_the_file_gives_none() {
        let inside = document();
        let read = |file: &str| read_pro_document(&inside, file).unwrap();
        assert_eq!(read("12 - Lantern Song").song.title, "12 - Lantern Song");
        assert_eq!(read("12 - Lantern Song").own_name, "Lantern Song");
        assert_eq!(read("").song.title, "Lantern Song");
        assert_eq!(read(" - ").song.title, "Lantern Song", "no letter or digit in the file's name");
        assert_eq!(read("100").song.title, "100", "a number is a name");
        let nameless =
            [field(12, &group("g", "Verse", &["a"])), field(13, &cue("a", &[r"{\rtf1 Words}"], "", false))].concat();
        assert_eq!(read_pro_file(&nameless, "").unwrap().title, "Untitled");
    }

    #[test]
    fn the_box_in_larger_type_is_the_main_words_and_a_reference_line_is_the_reference() {
        let boxes = [
            r"{\rtf1 }",
            r"{\rtf1 \fs80 Let your light shine}",
            r"{\rtf1 \fs100 \u1057?\u1074?\u1077?\u1090?}",
            r"{\rtf1 \fs90 Psalm 133:3}",
            r"{\rtf1 \fs90 \u1055?\u1089? 132:3}",
        ];
        let doc = [field(12, &group("g", "", &["a"])), field(13, &cue("a", &boxes, "", true))].concat();
        let song = read_pro_file(&doc, "x").unwrap();
        let slide = &song.groups[0].slides[0];
        assert_eq!(slide.text_for(&Role::primary()), Some("Свет"), "the larger type is the main words");
        assert_eq!(slide.text_for(&Role::translation()), Some("Let your light shine"));
        assert_eq!(slide.text_for(&Role::reference()), Some("Пс 132:3"), "the reference in the main words' script");
        assert_eq!(slide.objects.len(), 3, "the empty box is left out");
    }

    #[test]
    fn the_main_words_stay_in_one_language_when_a_long_slide_was_shrunk_to_fit() {
        // The first box is set at 100 and the second at 80. On one slide the first box's long
        // words were shrunk to 60, and on another the second box holds a title in 300.
        let doc = [
            field(12, &group("g", "", &["a", "b", "c", "d"])),
            field(13, &cue("a", &[r"{\rtf1 \fs100 \'c4\'ee\'ec}", r"{\rtf1 \fs80 Home}"], "", true)),
            field(13, &cue("b", &[r"{\rtf1 \fs60 \'c4\'ee\'ec \'c4\'ee\'ec}", r"{\rtf1 \fs80 Home again}"], "", true)),
            field(13, &cue("c", &[r"{\rtf1 \fs100 \'c4\'ee\'ec}", r"{\rtf1 \fs300 Home Title}"], "", true)),
            field(13, &cue("d", &[r"{\rtf1 \fs100 \'c4\'ee\'ec}", r"{\rtf1 \fs80 Home last}"], "", true)),
        ]
        .concat();
        let song = read_pro_file(&doc, "x").unwrap();
        for slide in &song.groups[0].slides {
            assert!(slide.text_for(&Role::primary()).is_some_and(|t| t.starts_with('Ä')), "{:?}", slide.texts());
            assert!(slide.text_for(&Role::translation()).is_some_and(|t| t.starts_with("Home")));
        }
    }

    #[test]
    fn a_slide_with_its_languages_the_other_way_round_keeps_the_songs_main_language() {
        let cyrillic = r"{\rtf1 {\fonttbl\f0\fcharset204 PT;}\f0 \fs100 \'c4\'ee\'ec}";
        let swapped = r"{\rtf1 {\fonttbl\f0\fcharset204 PT;}\f0 \fs80 \'c4\'ee\'ec}";
        let doc = [
            field(12, &group("g", "", &["a", "b", "c"])),
            field(13, &cue("a", &[cyrillic, r"{\rtf1 \fs80 Home}"], "", true)),
            field(13, &cue("b", &[cyrillic, r"{\rtf1 \fs80 Home}"], "", true)),
            // Here the big box holds the English and the small one the Russian.
            field(13, &cue("c", &[r"{\rtf1 \fs100 Home}", swapped], "", true)),
        ]
        .concat();
        let song = read_pro_file(&doc, "x").unwrap();
        for slide in &song.groups[0].slides {
            assert_eq!(slide.text_for(&Role::primary()), Some("Дом"));
            assert_eq!(slide.text_for(&Role::translation()), Some("Home"));
        }
    }

    #[test]
    fn references_are_told_from_words() {
        for reference in ["Psalm 133:3", "1 John 4:7-8", "Псалтирь 132:3", "Ps. 23:1,2"] {
            assert!(is_reference(reference), "{reference}");
        }
        for words in ["Shine on", "Verse 2", "Psalm 133", "Count to 10:30 tonight we sing", "Shine\nPsalm 1:1"] {
            assert!(!is_reference(words), "{words}");
        }
    }

    #[test]
    fn a_pictures_file_name_is_not_kept_as_a_label_and_a_document_without_words_is_refused() {
        let picture = [field(3, &field(2, b"photo 12.jpg")), number(9, 2)].concat();
        let words = cue("a", &[r"{\rtf1 Words}"], "", true);
        let with_picture = [words, field(10, &picture)].concat();
        let doc = [field(12, &group("g", "", &["a"])), field(13, &with_picture)].concat();
        assert_eq!(read_pro_file(&doc, "x").unwrap().groups[0].slides[0].label, None);
        let empty = [field(12, &group("g", "", &["b"])), field(13, &cue("b", &[r"{\rtf1 }"], "", true))].concat();
        assert_eq!(read_pro_file(&empty, "x").unwrap_err(), Reason::NoWordsOrPictures);
    }

    #[test]
    fn each_slides_picture_or_video_is_read_with_every_name_its_file_has() {
        let doc = song_showing(&[
            ("file:///Volumes/Church/Backgrounds/Sky%20Blue.jpg", "Backgrounds/Sky Blue.jpg", None),
            ("Waves.mov", "Waves.mov", Some(true)),
            ("file:///Volumes/Church/Clips/Count.mp4", "", Some(false)),
        ]);
        let document = read_pro_document(&doc, "x").unwrap();
        let slides: Vec<&Slide> = document.song.groups[0].slides.iter().collect();
        assert_eq!(slides.len(), 4);
        assert_eq!(document.media.len(), 3, "the last slide shows nothing");
        let (first, picture) = &document.media[0];
        assert_eq!(*first, slides[0].id);
        assert_eq!(picture.names[0], "file:///Volumes/Church/Backgrounds/Sky%20Blue.jpg");
        assert_eq!(picture.names[1], "Backgrounds/Sky Blue.jpg");
        assert!(!picture.video);
        assert_eq!(
            document.media[1],
            (
                slides[1].id,
                ProMedia {
                    names: vec!["Waves.mov".into(), "Waves.mov".into(), String::new()],
                    video: true,
                    looping: true
                }
            )
        );
        assert!(document.media[2].1.video && !document.media[2].1.looping);
        // The file's name the media action is labelled with is not kept as the slide's label.
        assert!(slides.iter().all(|slide| slide.label.is_none()));
        // Read for its words alone, the same document is the same song.
        let words = read_pro_file(&doc, "x").unwrap();
        assert_eq!(words.slide_count(), 4);
    }

    #[test]
    fn a_document_of_pictures_alone_comes_with_them_but_is_refused_for_its_words() {
        let doc = [
            field(12, &group("g", "", &["a"])),
            field(13, &cue_showing("a", r"{\rtf1 }", &[media_action("Sky.png", "Sky.png", None)])),
        ]
        .concat();
        assert_eq!(read_pro_document(&doc, "x").unwrap().media.len(), 1);
        assert_eq!(read_pro_file(&doc, "x").unwrap_err(), Reason::OnlyPictures);
        // A sound is not a picture: a slide that plays one shows nothing.
        let element = [field(1, &uuid("m")), field(2, &field(1, b"Bell.mp3"))].concat();
        let sound = [number(9, 2), field(20, &[field(5, &element), field(8, &[])].concat())].concat();
        let doc =
            [field(12, &group("g", "", &["a"])), field(13, &cue_showing("a", r"{\rtf1 Words}", &[sound]))].concat();
        assert!(read_pro_document(&doc, "x").unwrap().media.is_empty());
    }

    #[test]
    fn damaged_documents_are_refused_not_half_read() {
        let mut doc = document();
        doc.truncate(doc.len() - 3);
        assert!(read_pro_file(&doc, "x").is_err());
        assert!(read_pro_file(&[0x0A, 0xFF, 0xFF, 0xFF, 0xFF, 0x0F], "x").is_err(), "a length past the end");
        assert!(read_pro_file(&[0x0B], "x").is_err(), "an old group field");
        assert!(read_pro_file(b"plain text, not a document", "x").is_err());
        assert_eq!(
            read_pro_file(&[0x0B], "x").unwrap_err().to_string(),
            "the file is damaged or isn't a song document"
        );
    }
}

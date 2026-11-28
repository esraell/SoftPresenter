//! Free songbooks shared by churches: songbooks many people put together over the years, which
//! a public page shares for anyone to use, offered for download as Bibles are (`bible_store`).
//! SoftPresenter keeps no copy of them: a church that chooses one fetches it from the page's own
//! site, over HTTPS only, and it is installed only when it is byte for byte the file this version
//! expects (its size and SHA-256 are built in below, measured on 25 November 2026). Its songs then
//! come in through the songbook import File › Import Songs uses (`load::import_songbook`, and
//! `App::take_songs`): one library for the songbook, each song keeping its number before its title.
//!
//! Each song brought in is marked with where it came from (`Presentation::catalog`, source
//! [`SOURCE`], key `<songbook>/<number>`), as the song catalog and TimelyChurch mark theirs, so the
//! app knows which songbooks are installed and which of their songs the church has changed (the
//! mark's fingerprint, `catalog::fingerprint`). A songbook installed is not downloaded again: the
//! file is pinned, so a second download would bring nothing new. Remove takes back out the songs
//! nobody changed and nothing else uses (to the Trash), and the songbook can be installed again.
//!
//! This module is the list, the download, the marks and the windows (the Free Songbooks window,
//! and the setup's part of the Songs step); the app's side, which runs the downloads on
//! threads of their own and brings the songs into the library, is `free_songbooks_cli`.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::io::Read;
use std::path::Path;
use std::sync::atomic::Ordering;
use std::time::Duration;

use egui::{Align, Align2, Color32, Frame, Layout, Stroke, vec2};
use sha2::{Digest, Sha256};
use sp_model::{CatalogOrigin, Presentation};

use crate::catalog::Progress;
use crate::i18n::{t, tf, tn};
use crate::studio::{CARE, TEXT_DIM, TEXT_FAINT};

/// The name a song's mark gives where it came from.
pub const SOURCE: &str = "free-songbooks";

/// Where the songbooks are published: each file's name is added to it. The page that shares
/// them links an older kind of file for most of them (zipped), which the songbook import doesn't
/// read; these are the files the same site lists for programs that download songbooks, in the
/// kind the import reads.
const DOWNLOADS: &str = "https://softprojector.org/songbooks/";

/// The largest songbook that will ever be fetched. The biggest offered is under 5 MB.
pub const LARGEST: u64 = 16 * 1024 * 1024;

/// A songbook offered for download.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Offered {
    /// Its short name, for `sp songbook install` and the songs' marks: small letters, digits, `-`.
    pub id: &'static str,
    /// The language its songs are in (`ru`).
    pub language: &'static str,
    /// Its name, as the windows show it and its library is called.
    pub name: &'static str,
    /// The file's name at the site, as its address writes it.
    pub file: &'static str,
    /// The file's size, in bytes.
    pub bytes: u64,
    /// The file's SHA-256, in hex.
    pub sha256: &'static str,
    /// How many songs it brings into an empty library.
    pub songs: usize,
}

const fn book(
    id: &'static str,
    language: &'static str,
    name: &'static str,
    file: &'static str,
    bytes: u64,
    sha256: &'static str,
    songs: usize,
) -> Offered {
    Offered { id, language, name, file, bytes, sha256, songs }
}

/// Every songbook offered, by language, each language's first the one to recommend first.
/// Left out: three Czech songbooks the page lists (Bratrské písně BJB, České chvály, Dětské
/// písně), whose links lead nowhere and which the site has under no other name.
pub const OFFERED: &[Offered] = &[
    book(
        "kancionaly-cb",
        "cs",
        "Kancionály CB",
        "Kancionaly_CB.sps",
        569_344,
        "665119c502eb5b46e831be175ea4ae3ab619eb0ebd960b67eec11c00d8519250",
        578,
    ),
    book(
        "laudate",
        "cs",
        "Laudate",
        "Laudate.sps",
        227_328,
        "3e71b614106ab0a0005daad0f615aacb2976b87667618c2bfad992289572ca78",
        280,
    ),
    book(
        "vanoce",
        "cs",
        "Vánoce",
        "Vanocni.sps",
        21_504,
        "95858c5280bf423ff602887003325bb7a9aa99143b65da732e9f151dc4f4b7f0",
        22,
    ),
    book(
        "glb",
        "de",
        "Gemeinschafts Lieder Buch",
        "Gemeinschafts-Lieder-Buch.sps",
        806_912,
        "46d8595c24088987830b29a96946206c0822980763aa89b8801c57b5919d6568",
        780,
    ),
    book(
        "christian-hymns",
        "en",
        "Christian Hymns",
        "Christian_Hymns.sps",
        436_224,
        "8e8c92f403a5950e2f71ff64b3cb388a89566f6f273ac31cf127c86591486c39",
        426,
    ),
    book(
        "worship-songs",
        "en",
        "Worship Songs",
        "englishworship.sps",
        47_104,
        "7329eb12c1731f4cb8a5d78085c2c9e04f96ce406d61d7d5fd93d247dc0e0cb3",
        37,
    ),
    book(
        "slovenske-chvaly",
        "sk",
        "Slovenské chvály",
        "SlovenskeChvaly.sps",
        220_160,
        "484ce812c976d3c46facb0e78a6225723abba25e10d05874e5ada46c630fc95e",
        358,
    ),
    book(
        "pv3300",
        "ru",
        "Песнь Возрождения 3300",
        "pv3300.sps",
        4_667_392,
        "8fcad18f2f19dce57bb4dcb34697e5986029a059cdec4d20eb235643c44a96ac",
        3301,
    ),
    book(
        "pv3055",
        "ru",
        "Песнь Возрождения 3055",
        "pv3055.sps",
        4_334_592,
        "a97d074fa39426e88414efd8aae71f66d6cbef9581a50f38f6a5ea9974e84266",
        3066,
    ),
    book(
        "pv2001",
        "ru",
        "Песнь Возрождения 2001",
        "pv2001.sps",
        2_820_096,
        "df9e070e76ccdf0e458f4c1b87973b2d72e376466b5c29a6c1ad33490fe7b243",
        2001,
    ),
    book(
        "pv-sovet",
        "ru",
        "Песнь Возрождения Совета Церквей 2006",
        "pvSovetCerkvei.sps",
        1_073_152,
        "a0083b8a454614885b34a8a3067b747d36bb6db877e9c8315b5da5622f574cc7",
        800,
    ),
    book(
        "zvuki-neba",
        "ru",
        "Звуки неба 2010",
        "Zvuki_neba%202010.sps",
        192_512,
        "09c2805dc50337a3a7675e2027ebec2ddd3de6698271e941f29a84f541faaaaa",
        153,
    ),
    book(
        "evangelski-pisni",
        "uk",
        "Євангельські Пісні",
        "EvangelskiPisni.sps",
        663_552,
        "d0b852a329aed09db1fb879d9cdbce915a15e9cfd878ba8be6b73bcd6ce13bab",
        522,
    ),
    book(
        "pisni-spasennyh",
        "uk",
        "Пісні Спасенних",
        "PisniSpasennyh.sps",
        945_152,
        "8093da26ad8026c5dfe0e7e46a5653327c9923a6a5d4b0b8c9a26743b093b8cb",
        770,
    ),
    book(
        "psalmospivy",
        "uk",
        "Псалмоспіви",
        "Psalmovivi.sps",
        1_283_072,
        "8f51bfc816e17b27b0a7152149e2f62db16b9b12f4c450bd3b4042d7b560c9ed",
        999,
    ),
];

/// The songbook offered under this short name, in any case.
pub fn by_id(id: &str) -> Option<&'static Offered> {
    OFFERED.iter().find(|book| book.id.eq_ignore_ascii_case(id.trim()))
}

/// The songbook a name typed means: its short name, its name in any case, or its file's name
/// (with or without the ending).
pub fn find(typed: &str) -> Option<&'static Offered> {
    let typed = typed.trim().to_lowercase();
    by_id(&typed).or_else(|| {
        OFFERED.iter().find(|book| {
            let file = book.file.replace("%20", " ").to_lowercase();
            book.name.to_lowercase() == typed || file == typed || file.trim_end_matches(".sps") == typed
        })
    })
}

/// A language's name written in itself (Русский, Čeština), as the song catalog names it.
pub fn language_name(code: &str) -> &'static str {
    crate::catalog::songbook(code).map_or("", |book| book.name())
}

/// A language's name in English, for `sp`.
pub fn language_english(code: &str) -> &'static str {
    crate::catalog::songbook(code).map_or("", |book| book.english)
}

/// The language a tag stands for (`ru` for `ru-RU`, `uk_UA.UTF-8`).
fn base_language(tag: &str) -> String {
    tag.trim().split(['-', '_', '.', '@']).next().unwrap_or("").to_ascii_lowercase()
}

/// The languages to recommend songbooks in: the one SoftPresenter speaks (`app`), then the
/// computer's (`computer`, as the system writes it), each once and only where a songbook is
/// offered. The computer's counts too because SoftPresenter speaks only a few languages, and a
/// Czech, Slovak or German church's app is in English.
pub fn recommended_languages(app: &str, computer: Option<&str>) -> Vec<&'static str> {
    let mut out: Vec<&'static str> = Vec::new();
    for tag in std::iter::once(app).chain(computer) {
        let code = base_language(tag);
        if let Some(book) = OFFERED.iter().find(|book| book.language == code)
            && !out.contains(&book.language)
        {
            out.push(book.language);
        }
    }
    out
}

/// The languages as the window lists them: the recommended first, then the others in the list's
/// order.
pub fn languages_in_order(recommended: &[&'static str]) -> Vec<&'static str> {
    let mut out: Vec<&'static str> = recommended.to_vec();
    for book in OFFERED {
        if !out.contains(&book.language) {
            out.push(book.language);
        }
    }
    out
}

/// The languages to recommend songbooks in for a church that shows `church` (the setup's
/// Languages step, the one on top first): those, each once and only where a songbook is offered,
/// and nothing else.
pub fn church_languages_offered(church: &[String]) -> Vec<&'static str> {
    let mut out: Vec<&'static str> = Vec::new();
    for tag in church {
        let code = base_language(tag);
        if let Some(book) = OFFERED.iter().find(|book| book.language == code)
            && !out.contains(&book.language)
        {
            out.push(book.language);
        }
    }
    out
}

/// The languages recommended, given what the church shows: the church's languages first, then
/// the app's and the computer's ([`recommended_languages`]), which alone count until the setup's
/// Languages step is answered.
pub fn recommended_with(church: &[String], app: &str, computer: Option<&str>) -> Vec<&'static str> {
    let mut out = church_languages_offered(church);
    for language in recommended_languages(app, computer) {
        if !out.contains(&language) {
            out.push(language);
        }
    }
    out
}

/// The languages recommended now, the church's first: the Free Songbooks window lists them
/// before the rest.
pub fn recommended_now() -> Vec<&'static str> {
    recommended_with(
        &crate::songbooks::church_languages(),
        crate::i18n::current().code(),
        crate::i18n::computer_tag().as_deref(),
    )
}

/// The songbooks of a language, the one to recommend first at the front.
pub fn of_language(code: &str) -> impl Iterator<Item = &'static Offered> + '_ {
    OFFERED.iter().filter(move |book| book.language == code)
}

/// The address songbooks are fetched from. A testing aid can name another with
/// `SOFTPRESENTER_SONGBOOK_URL`; what comes from it is checked all the same.
pub fn base() -> String {
    // Testing aid: SOFTPRESENTER_SONGBOOK_URL=<address> fetches the free songbooks from there (a
    // local test server, where plain http is allowed) instead of the site that shares them; each
    // file must still have the size and checksum this version expects.
    std::env::var("SOFTPRESENTER_SONGBOOK_URL")
        .ok()
        .filter(|url| !url.is_empty())
        .unwrap_or_else(|| DOWNLOADS.to_owned())
}

/// The address of a songbook's file.
pub fn address(base: &str, book: &Offered) -> String {
    format!("{}/{}", base.trim_end_matches('/'), book.file)
}

/// True for an address on this computer (a test server).
pub(crate) fn is_local(address: &str) -> bool {
    ["http://127.0.0.1:", "http://127.0.0.1/", "http://localhost:", "http://localhost/"]
        .iter()
        .any(|start| address.to_ascii_lowercase().starts_with(start))
}

/// Whether an address may be fetched: `https`, or plain `http` to this computer only (a test
/// server). Redirects are held to the same: the download agent follows `https` only.
pub fn allowed(address: &str) -> bool {
    address.to_ascii_lowercase().starts_with("https://") || is_local(address)
}

/// A SHA-256 in hex.
pub fn checksum(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|byte| format!("{byte:02x}")).collect()
}

/// What is said of a file that isn't the one this version expects.
fn changed(book: &Offered) -> String {
    tf(
        "freesongbooks.changed",
        "The file for {name} on the site that shares it has changed since this version of SoftPresenter was made, so it was not installed. A copy you trust can be brought in with Import Songs \u{203a} Song Files\u{2026}",
        &[("name", book.name)],
    )
}

/// Checks that the bytes are the songbook's file to the byte: its size and its SHA-256.
pub fn check(book: &Offered, bytes: &[u8]) -> Result<(), String> {
    if bytes.len() as u64 != book.bytes || checksum(bytes) != book.sha256 {
        return Err(changed(book));
    }
    Ok(())
}

/// Why one try at a download failed: the server refused (not tried again), or the network.
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

/// Fetches a songbook's file from `address`, trying once more if the network fails. Nothing
/// longer than the file expected (`book.bytes`) is ever read, and a body shorter than the server
/// said is "cut short".
pub fn fetch(address: &str, book: &Offered, progress: Option<&Progress>) -> Result<Vec<u8>, String> {
    if !allowed(address) {
        return Err(t(
            "freesongbooks.not_https",
            "The songbook's address isn't a secure (https) one, so nothing was downloaded.",
        )
        .to_owned());
    }
    fetch_once(address, book, progress).or_else(|first| match first {
        Failed::Server(words) => Err(words),
        Failed::Network(_) => fetch_once(address, book, progress).map_err(Failed::words),
    })
}

fn fetch_once(address: &str, book: &Offered, progress: Option<&Progress>) -> Result<Vec<u8>, Failed> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(300)))
        .timeout_connect(Some(Duration::from_secs(20)))
        // Every step, a redirect's too, must be https (but for a test server on this computer).
        .https_only(!is_local(address))
        .max_redirects(3)
        .build()
        .into();
    let refused = |e: ureq::Error| match e {
        ureq::Error::StatusCode(code) => Failed::Server(tf(
            "freesongbooks.answered",
            "The site that shares {name} answered {code}, not the songbook.",
            &[("name", book.name), ("code", &code.to_string())],
        )),
        other => Failed::Network(tf(
            "freesongbooks.unreachable",
            "Could not reach the site that shares the songbook: {error}",
            &[("error", &other.to_string())],
        )),
    };
    // Asked for as it is, so the server gives its length.
    let mut response = agent.get(address).header("Accept-Encoding", "identity").call().map_err(refused)?;
    let said = response
        .headers()
        .get("content-length")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.trim().parse::<u64>().ok());
    // A file of another size is another file: nothing of it is read.
    if said.is_some_and(|said| said != book.bytes) {
        return Err(Failed::Server(changed(book)));
    }
    if let Some(progress) = progress {
        progress.total.store(book.bytes, Ordering::Relaxed);
    }
    let mut reader = response.body_mut().with_config().limit(LARGEST).reader();
    let mut bytes = Vec::with_capacity(book.bytes.min(LARGEST) as usize);
    let mut chunk = [0_u8; 64 * 1024];
    loop {
        let read = reader.read(&mut chunk).map_err(|e| {
            Failed::Network(tf("freesongbooks.stopped", "The download stopped: {error}", &[("error", &e.to_string())]))
        })?;
        if read == 0 {
            break;
        }
        bytes.extend_from_slice(&chunk[..read]);
        if bytes.len() as u64 > book.bytes {
            return Err(Failed::Server(changed(book)));
        }
        if let Some(progress) = progress {
            progress.done.store(bytes.len() as u64, Ordering::Relaxed);
        }
    }
    if said.is_some_and(|said| said != bytes.len() as u64) {
        return Err(Failed::Network(t("freesongbooks.cut_short", "The download was cut short. Try again.").to_owned()));
    }
    Ok(bytes)
}

/// Downloads a songbook from the site at `base` and checks it is the file expected, before
/// anything is written.
pub fn download(base: &str, book: &Offered, progress: Option<&Progress>) -> Result<Vec<u8>, String> {
    let bytes = fetch(&address(base, book), book, progress)?;
    check(book, &bytes)?;
    Ok(bytes)
}

/// Reads a songbook's checked bytes into songs with the songbook import File › Import Songs uses,
/// from a file of their own in a folder of its own inside `folder` (the data folder), which is
/// gone again once read; each song is marked with where it came from (see [`mark`]).
pub fn read(book: &Offered, bytes: &[u8], folder: &Path, now: i64) -> Result<Vec<Presentation>, String> {
    let unreadable = || {
        tf(
            "freesongbooks.unreadable",
            "{name} was downloaded, but this computer couldn\u{2019}t read it.",
            &[("name", book.name)],
        )
    };
    let staging = folder.join(format!(".songbook-{}", sp_model::Uuid::new_v4()));
    let path = staging.join(format!("{}.sps", book.id));
    let written = std::fs::create_dir_all(&staging).and_then(|()| std::fs::write(&path, bytes));
    if let Err(e) = written {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(tf(
            "freesongbooks.not_saved",
            "{name} couldn\u{2019}t be saved for reading: {error}",
            &[("name", book.name), ("error", &crate::notices::write_problem(&e))],
        ));
    }
    let read = crate::load::import_songbook(&path);
    let _ = std::fs::remove_dir_all(&staging);
    let read = read.map_err(|problem| {
        crate::applog::problem("songbooks", &format!("{} could not be read: {problem}", book.id));
        unreadable()
    })?;
    // What reading it noticed (such as changes the file may be missing) goes in the log.
    for caution in &read.cautions {
        crate::applog::event("songbooks", &format!("{}: {caution}", book.id));
    }
    let songs = read.songs;
    if songs.is_empty() {
        return Err(unreadable());
    }
    Ok(mark(book, songs, now))
}

/// The number before a song's title ("123. …" gives 123), as the songbook import writes it.
fn number_of(title: &str) -> Option<u32> {
    let digits: String = title.trim_start().chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok().filter(|n| *n > 0)
}

/// Marks each song read from a songbook with where it came from: the source, a key of its own
/// (`pv3300/123`, its number in the songbook; `pv3300/123-2` for a second song with that
/// number, `pv3300/#7` for the seventh song where there is none), its language, and its
/// fingerprint as made, which tells later whether the church has changed it.
pub fn mark(book: &Offered, songs: Vec<(Presentation, Option<String>)>, now: i64) -> Vec<Presentation> {
    let mut seen: HashMap<String, usize> = HashMap::new();
    let added = crate::catalog::date_of(now);
    songs
        .into_iter()
        .enumerate()
        .map(|(at, (mut song, _))| {
            let base = match number_of(&song.title) {
                Some(number) => format!("{}/{number}", book.id),
                None => format!("{}/#{}", book.id, at + 1),
            };
            let times = seen.entry(base.clone()).or_insert(0);
            *times += 1;
            let key = if *times == 1 { base } else { format!("{base}-{times}") };
            let made = crate::catalog::fingerprint(&song);
            song.catalog = Some(Box::new(CatalogOrigin {
                source: SOURCE.to_owned(),
                key,
                language: book.language.to_owned(),
                second: None,
                text_hash: made.clone(),
                made_hash: made,
                added: added.clone(),
                newer: None,
                gone: false,
            }));
            song
        })
        .collect()
}

/// The songbook a song came from, by its mark.
pub fn songbook_of(song: &Presentation) -> Option<&'static Offered> {
    let origin = song.catalog.as_ref().filter(|origin| origin.source == SOURCE)?;
    by_id(origin.key.split('/').next()?)
}

/// Whether a song from a songbook is still as it came: nobody has changed its words, title,
/// details, theme or arrangements.
pub fn untouched(song: &Presentation) -> bool {
    song.catalog.as_ref().is_some_and(|origin| crate::catalog::fingerprint(song) == origin.made_hash)
}

/// One of a songbook's songs in the library, as Remove weighs it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InLibrary {
    /// Its place among the app's songs.
    pub at: usize,
    /// The key playlists and libraries name it by.
    pub key: String,
    /// Nobody has changed it.
    pub untouched: bool,
}

/// Which of a songbook's songs Remove takes out of the library, and which stay as the church's
/// own: a song goes only when nobody has changed it, no playlist or other library names it
/// (`used`), and it isn't open in the Edit view (`editing`).
pub fn removal(songs: &[InLibrary], used: &HashSet<String>, editing: Option<usize>) -> (Vec<usize>, Vec<usize>) {
    let (mut gone, mut kept) = (Vec::new(), Vec::new());
    for song in songs {
        if song.untouched && !used.contains(&song.key) && editing != Some(song.at) {
            gone.push(song.at);
        } else {
            kept.push(song.at);
        }
    }
    (gone, kept)
}

/// "3,301 songs", as the language writes it.
pub fn songs_words(songs: usize) -> String {
    tn("freesongbooks.songs", songs as u64, "{n} song", "{n} songs", &[])
}

/// A download's size, as the language writes it ("4.7 MB", "22 KB").
pub fn size_words(bytes: u64) -> String {
    if bytes < 1_000_000 {
        return tf("freesongbooks.size_kb", "{size} KB", &[("size", &crate::i18n::count(bytes.div_ceil(1000)))]);
    }
    tf("freesongbooks.size", "{size} MB", &[("size", &crate::i18n::number(bytes as f64 / 1_000_000.0, 1))])
}

/// What the windows know of the songbooks, kept up to date by the app.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Known {
    /// The songbooks installed, with the songs in each one's library.
    pub installed: BTreeMap<&'static str, usize>,
    /// Those downloading, with how far each has got.
    pub busy: BTreeMap<&'static str, Option<u8>>,
    /// Downloaded and read, their songs waiting for nothing to be on screen.
    pub held: BTreeSet<&'static str>,
}

impl Known {
    /// Installed, or on its way.
    pub fn here(&self, id: &str) -> bool {
        self.installed.contains_key(id) || self.busy.contains_key(id) || self.held.contains(id)
    }
}

/// What a window asks of the app.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Ask {
    Install(&'static str),
    /// Take a songbook's songs back out (the window asked first).
    Remove(&'static str),
}

/// The Free Songbooks window and the setup's part: what is known, what was asked, and whether
/// the window is open.
#[derive(Debug, Default)]
pub struct View {
    pub known: Known,
    pub asks: Vec<Ask>,
    /// The Free Songbooks window is open.
    pub open: bool,
    /// What went wrong last, for the window to show until the next Install.
    pub problem: Option<String>,
    /// The songbook whose Remove is asking first.
    removing: Option<&'static str>,
}

impl View {
    /// What a songbook's row says under its name: how far its download has got, that its songs
    /// are waiting for the screens, its songs once installed, or what it brings.
    pub fn detail(&self, book: &Offered) -> String {
        if let Some(per_cent) = self.known.busy.get(book.id) {
            return match per_cent {
                Some(per) => {
                    tf("freesongbooks.downloading_per", "Downloading \u{b7} {per}%", &[("per", &per.to_string())])
                }
                None => t("freesongbooks.downloading", "Downloading\u{2026}").to_owned(),
            };
        }
        if self.known.held.contains(book.id) {
            return t("freesongbooks.held", "Downloaded \u{b7} its songs come in once nothing is on screen").to_owned();
        }
        if let Some(songs) = self.known.installed.get(book.id) {
            return tf("freesongbooks.installed_row", "Installed \u{b7} {songs}", &[("songs", &songs_words(*songs))]);
        }
        format!("{} \u{b7} {}", songs_words(book.songs), size_words(book.bytes))
    }

    /// Whether the window must be drawn again soon: something was asked, or is under way.
    fn waiting(&self) -> bool {
        !self.asks.is_empty() || !self.known.busy.is_empty() || !self.known.held.is_empty()
    }

    /// Asks for a songbook, clearing what went wrong before.
    fn install(&mut self, id: &'static str) {
        self.problem = None;
        self.asks.push(Ask::Install(id));
    }
}

/// The line under every list of these songbooks: whose songs they are.
fn rights_words() -> &'static str {
    t(
        "freesongbooks.rights",
        "The songs belong to their writers; a church\u{2019}s licence (such as CCLI) covers showing them in its services.",
    )
}

/// The Free Songbooks window: the songbooks shared by churches, the recommended languages
/// first, each with Install, its download going on, or Remove (asking first) once installed.
/// Escape or Done closes it.
pub fn window(ctx: &egui::Context, view: &mut View) {
    let frame = Frame::new()
        .fill(Color32::from_rgb(38, 38, 40))
        .stroke(Stroke::new(1.0, Color32::from_white_alpha(22)))
        .corner_radius(14.0)
        .inner_margin(egui::Margin { left: 22, right: 22, top: 18, bottom: 16 })
        .shadow(egui::Shadow { offset: [0, 16], blur: 44, spread: 0, color: Color32::from_black_alpha(150) });
    let area =
        egui::Modal::default_area(egui::Id::unique("free songbooks")).anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0));
    let mut close = false;
    egui::Modal::new(egui::Id::unique("free songbooks"))
        .area(area)
        .backdrop_color(Color32::from_black_alpha(90))
        .frame(frame)
        .show(ctx, |ui| {
            let width = 560.0_f32.min(ctx.content_rect().width() - 80.0);
            ui.set_width(width);
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(t("freesongbooks.window.title", "Free Songbooks"))
                        .font(crate::studio::semibold(18.0))
                        .color(Color32::WHITE),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.button(t("freesongbooks.window.done", "Done")).clicked() {
                        close = true;
                    }
                });
            });
            ui.add_space(4.0);
            ui.add(
                egui::Label::new(
                    egui::RichText::new(t(
                        "freesongbooks.window.about",
                        "Free songbooks shared by churches. Install downloads one from the page that shares it, and its songs come into your library, in a library of its own, each with its number.",
                    ))
                    .size(12.5)
                    .color(TEXT_DIM),
                )
                .wrap(),
            );
            ui.add_space(8.0);
            if let Some(problem) = &view.problem {
                ui.add(egui::Label::new(egui::RichText::new(problem.as_str()).size(12.0).color(CARE)).wrap());
                ui.add_space(6.0);
            }
            let recommended = recommended_now();
            let height = (ctx.content_rect().height() - 240.0).clamp(220.0, 520.0);
            let mut asks = Vec::new();
            egui::ScrollArea::vertical().id_salt("free songbooks").max_height(height).auto_shrink([false, true]).show(
                ui,
                |ui| {
                    ui.set_width(width);
                    for language in languages_in_order(&recommended) {
                        ui.add_space(6.0);
                        crate::studio::caption(ui, &language_name(language).to_uppercase());
                        for book in of_language(language) {
                            let busy = view.known.busy.contains_key(book.id) || view.known.held.contains(book.id);
                            let button = if busy {
                                None
                            } else if view.known.installed.contains_key(book.id) {
                                Some(t("freesongbooks.remove", "Remove"))
                            } else {
                                Some(t("freesongbooks.install", "Install"))
                            };
                            let clicked =
                                crate::songbooks::button_card(ui, book.name, &view.detail(book), button);
                            if clicked && view.known.installed.contains_key(book.id) {
                                view.removing = Some(book.id);
                            } else if clicked {
                                asks.push(book.id);
                            }
                            ui.add_space(6.0);
                        }
                    }
                },
            );
            for id in asks {
                view.install(id);
            }
            ui.add_space(8.0);
            ui.add(egui::Label::new(egui::RichText::new(rights_words()).size(11.0).color(TEXT_FAINT)).wrap());
            if view.waiting() {
                ui.ctx().request_repaint_after(Duration::from_millis(250));
            }
        });
    // Remove asks first.
    if let Some(id) = view.removing
        && let Some(book) = by_id(id)
    {
        let title = tf("freesongbooks.remove_title", "Remove {name}?", &[("name", book.name)]);
        let note = t(
            "freesongbooks.remove_note",
            "Its songs leave your library and go to the Trash, except songs you have changed and songs a playlist or another library uses, which stay as yours. It can be installed again.",
        );
        let remove = t("freesongbooks.remove", "Remove");
        if let Some(answer) =
            crate::studio::alert(ctx, "remove free songbook", crate::icons::Icon::Close, &title, note, remove)
        {
            view.removing = None;
            if answer {
                view.asks.push(Ask::Remove(id));
            }
        }
        return;
    }
    if close || ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
        view.open = false;
    }
}

/// The setup's choice of these songbooks on its Songs step.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Picked {
    /// The songbooks ticked.
    pub ticked: Vec<&'static str>,
    /// Ticked or unticked by hand: the recommendation no longer changes the ticks.
    touched: bool,
    /// The step was left with Continue, so what is ticked is installed. Leaving it any other
    /// way (Skip, Back, Escape, Skip Setup) installs nothing.
    pub confirmed: bool,
}

impl Picked {
    /// Ticks the first songbook of each recommended language (the one to start with) not
    /// installed yet, until one is ticked by hand.
    pub fn recommend(&mut self, languages: &[&'static str], known: &Known) {
        if !self.touched {
            self.ticked = languages
                .iter()
                .filter_map(|language| of_language(language).next())
                .filter(|book| !known.here(book.id))
                .map(|book| book.id)
                .collect();
        }
    }

    pub fn toggle(&mut self, id: &'static str) {
        self.touched = true;
        match self.ticked.iter().position(|have| *have == id) {
            Some(at) => {
                self.ticked.remove(at);
            }
            None => self.ticked.push(id),
        }
    }

    /// Opened again from Settings: the recommended ones are marked but none is ticked unasked.
    pub fn leave_unticked(&mut self) {
        self.ticked.clear();
        self.touched = true;
    }

    /// What is to be installed once the step is passed: nothing unless it was left with
    /// Continue, and nothing installed or on its way already.
    pub fn to_install(&self, known: &Known) -> Vec<&'static str> {
        if !self.confirmed {
            return Vec::new();
        }
        self.ticked.iter().copied().filter(|id| !known.here(id)).collect()
    }
}

/// The setup's Songs step was passed: what was ticked and confirmed starts downloading.
pub fn setup_closed(picked: &Picked, view: &mut View) {
    for id in picked.to_install(&view.known) {
        view.install(id);
    }
}

/// The songbooks as the setup's Songs step lists them: every one offered, those of the church's
/// languages (`languages`, from [`church_languages_offered`]) first, a language at a time.
pub fn in_setup_order(languages: &[&'static str]) -> Vec<&'static Offered> {
    languages_in_order(languages).into_iter().flat_map(of_language).collect()
}

/// The free songbooks on the setup's Songs step: every songbook, the church's languages'
/// first, the first of each ticked to start with and marked Recommended, and whose songs they
/// are. The step's card scrolls them.
pub fn setup_rows(ui: &mut egui::Ui, picked: &mut Picked, view: &mut View, languages: &[&'static str]) {
    picked.recommend(languages, &view.known);
    let recommended: Vec<&'static str> =
        languages.iter().filter_map(|language| of_language(language).next()).map(|book| book.id).collect();
    for book in in_setup_order(languages) {
        let here = view.known.here(book.id);
        let ticked = here || picked.ticked.contains(&book.id);
        let mut detail = format!("{} \u{b7} {}", language_name(book.language), view.detail(book));
        if recommended.contains(&book.id) {
            detail = crate::setup::recommended_detail(&detail);
        }
        if crate::songbooks::tick_card(ui, book.name, &detail, ticked, here, !here).clicked() {
            picked.toggle(book.id);
        }
        ui.add_space(6.0);
    }
    ui.add_space(4.0);
    ui.add(egui::Label::new(egui::RichText::new(rights_words()).size(11.0).color(TEXT_FAINT)).wrap());
    if view.waiting() {
        ui.ctx().request_repaint_after(Duration::from_millis(250));
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;
    use crate::i18n::{Language, with_language};

    #[test]
    fn every_songbook_has_a_safe_short_name_an_https_file_and_a_checksum() {
        let mut seen = HashSet::new();
        for book in OFFERED {
            assert!(seen.insert(book.id), "{} is listed twice", book.id);
            assert!(book.id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'), "{}", book.id);
            assert!(!book.id.contains('/'), "a short name is the start of the songs' keys");
            assert_eq!(book.sha256.len(), 64, "{}", book.id);
            assert!(book.sha256.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()), "{}", book.id);
            assert!(book.bytes > 0 && book.bytes <= LARGEST, "{}", book.id);
            assert!(book.songs > 0, "{}", book.id);
            assert!(!language_name(book.language).is_empty(), "{} has a language with a name", book.id);
            // The address: the site's, over https, with nothing in the file's name that could
            // leave its folder or start a query.
            let address = address(DOWNLOADS, book);
            assert!(address.starts_with("https://") && allowed(&address), "{address}");
            assert!(book.file.chars().all(|c| c.is_ascii_alphanumeric() || "._-%".contains(c)), "{}", book.file);
            assert!(!book.file.contains("..") && book.file.ends_with(".sps"), "{}", book.file);
        }
        // A language's songbooks stand together, so the window lists each language once.
        let languages: Vec<&str> = OFFERED.iter().map(|book| book.language).collect();
        let mut grouped = languages.clone();
        grouped.dedup();
        let mut once = grouped.clone();
        once.sort();
        once.dedup();
        assert_eq!(grouped.len(), once.len(), "{languages:?}");
    }

    #[test]
    fn a_songbook_is_found_by_its_short_name_its_name_or_its_file() {
        assert_eq!(find("pv3300").map(|book| book.id), Some("pv3300"));
        assert_eq!(find("PV3300").map(|book| book.id), Some("pv3300"));
        assert_eq!(find("Песнь Возрождения 3055").map(|book| book.id), Some("pv3055"));
        assert_eq!(find("kancionály cb").map(|book| book.id), Some("kancionaly-cb"));
        assert_eq!(find("Zvuki_neba 2010.sps").map(|book| book.id), Some("zvuki-neba"));
        assert_eq!(find("englishworship").map(|book| book.id), Some("worship-songs"));
        assert!(find("pv").is_none() && find("").is_none());
    }

    #[test]
    fn the_persons_language_is_recommended_first_and_the_computers_too() {
        // A Russian church: Песнь Возрождения 3300 first; a Ukrainian one: Євангельські Пісні.
        assert_eq!(recommended_languages("ru", None), ["ru"]);
        assert_eq!(of_language("ru").next().map(|book| book.id), Some("pv3300"));
        assert_eq!(recommended_languages("uk", Some("uk-UA")), ["uk"]);
        assert_eq!(of_language("uk").next().map(|book| book.id), Some("evangelski-pisni"));
        // SoftPresenter speaks English to a Czech, Slovak or German computer: its songbooks
        // come after the English ones.
        assert_eq!(recommended_languages("en", Some("cs-CZ")), ["en", "cs"]);
        assert_eq!(recommended_languages("en", Some("sk_SK.UTF-8")), ["en", "sk"]);
        assert_eq!(recommended_languages("en", Some("de")), ["en", "de"]);
        // Spanish has none: nothing; a computer in a language with none adds nothing.
        assert!(recommended_languages("es", Some("es-ES")).is_empty());
        assert_eq!(recommended_languages("ru", Some("fr-FR")), ["ru"]);
        // The window lists the recommended languages first, then the rest, each once.
        let order = languages_in_order(&["uk", "en"]);
        assert_eq!(&order[..2], ["uk", "en"]);
        assert_eq!(order.len(), 6);
        // The setup ticks the first of each recommended language, but not one installed.
        let mut picked = Picked::default();
        picked.recommend(&["ru", "uk"], &Known::default());
        assert_eq!(picked.ticked, ["pv3300", "evangelski-pisni"]);
        let known = Known { installed: [("pv3300", 3301)].into(), ..Known::default() };
        picked.recommend(&["ru", "uk"], &known);
        assert_eq!(picked.ticked, ["evangelski-pisni"]);
        // The church's languages come first, and alone decide what the setup ticks: an English
        // app in a Russian and Ukrainian church recommends those two, not English.
        let church = ["ru".to_owned(), "uk-UA".to_owned(), "es".to_owned()];
        assert_eq!(church_languages_offered(&church), ["ru", "uk"]);
        assert_eq!(recommended_with(&church, "en", Some("cs-CZ")), ["ru", "uk", "en", "cs"]);
        // Until the setup has asked, today's rule stands.
        assert_eq!(recommended_with(&[], "en", Some("cs-CZ")), ["en", "cs"]);
        // The setup lists every songbook, the church's languages' first.
        let listed = in_setup_order(&["uk", "ru"]);
        assert_eq!(listed.len(), OFFERED.len());
        assert_eq!(listed[0].language, "uk");
        assert!(listed.iter().find(|book| book.language != "uk").is_some_and(|book| book.language == "ru"));
        // Once ticked by hand, the recommendation leaves the ticks alone.
        picked.toggle("pv2001");
        picked.recommend(&["en"], &Known::default());
        assert_eq!(picked.ticked, ["evangelski-pisni", "pv2001"]);
    }

    #[test]
    fn only_continue_installs_and_only_what_isnt_here() {
        let mut picked = Picked { ticked: vec!["pv3300", "laudate"], ..Picked::default() };
        let mut view = View::default();
        // Closed with Escape or Skip Setup: nothing.
        setup_closed(&picked, &mut view);
        assert!(view.asks.is_empty());
        // Continued: what is ticked and not here or on its way.
        picked.confirmed = true;
        view.known.busy.insert("laudate", None);
        setup_closed(&picked, &mut view);
        assert_eq!(view.asks, [Ask::Install("pv3300")]);
        // Skipped (never confirmed): nothing, whatever is ticked.
        let skipped = Picked { confirmed: false, ..picked.clone() };
        let mut view = View::default();
        setup_closed(&skipped, &mut view);
        assert!(view.asks.is_empty());
    }

    #[test]
    fn only_https_is_fetched_but_for_a_test_server_on_this_computer() {
        assert!(allowed("https://example.org/songbooks/pv3300.sps"));
        assert!(allowed("HTTPS://example.org/x.sps"));
        assert!(!allowed("http://example.org/songbooks/pv3300.sps"));
        assert!(!allowed("ftp://example.org/x.sps"));
        assert!(!allowed("file:///etc/hosts"));
        // A host named to look like this computer's isn't it.
        assert!(!allowed("http://127.0.0.1.example.org/x.sps"));
        assert!(!allowed("http://localhost.example.org/x.sps"));
        assert!(allowed("http://127.0.0.1:8080/x.sps") && allowed("http://localhost:9/x.sps"));
        // Plain http is refused before anything is asked of the network.
        let book = &OFFERED[0];
        let refused = fetch("http://example.org/songbooks/x.sps", book, None).unwrap_err();
        assert!(refused.contains("isn't a secure (https) one"), "{refused}");
    }

    /// A songbook made up for a test, with the size and checksum of `bytes`.
    fn made_up(bytes: &[u8]) -> Offered {
        let sum: &'static str = Box::leak(checksum(bytes).into_boxed_str());
        Offered { bytes: bytes.len() as u64, sha256: sum, ..OFFERED[0] }
    }

    /// Answers one request on this computer with `body`, saying its length is `length`, and
    /// returns the address.
    fn serve_once(body: Vec<u8>, length: usize) -> String {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = format!("http://{}/", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut request = [0_u8; 2048];
                let _ = stream.read(&mut request);
                let head = format!("HTTP/1.1 200 OK\r\nContent-Length: {length}\r\nConnection: close\r\n\r\n");
                let _ = stream.write_all(head.as_bytes());
                let _ = stream.write_all(&body);
            }
        });
        address
    }

    #[test]
    fn a_download_is_kept_only_when_it_is_the_file_expected() {
        let words = b"bytes written for this test, standing in for a songbook".to_vec();
        let book = made_up(&words);
        let progress = Progress::new();
        let base = serve_once(words.clone(), words.len());
        assert_eq!(download(&base, &book, Some(&progress)).unwrap(), words);
        assert_eq!(progress.per_cent(), Some(100));
        // The same size with other bytes: the checksum refuses it.
        let mut other = words.clone();
        other[0] ^= 1;
        let refused = download(&serve_once(other, words.len()), &book, None).unwrap_err();
        assert!(refused.contains("has changed since this version") && refused.contains("Song Files"), "{refused}");
        // A file of another size is refused by its length, before it is read.
        let longer = [words.clone(), b" and more".to_vec()].concat();
        let refused = download(&serve_once(longer.clone(), longer.len()), &book, None).unwrap_err();
        assert!(refused.contains("has changed since this version"), "{refused}");
        // A server that says nothing true of its length is stopped once it sends more than the
        // file: nothing past the songbook's size is kept.
        let huge = vec![b'x'; book.bytes as usize * 4];
        let refused = download(&serve_once(huge, words.len()), &book, None).unwrap_err();
        assert!(refused.contains("has changed") || refused.contains("stopped"), "{refused}");
        // Nobody answering: a plain sentence.
        let nobody = download("http://127.0.0.1:9", &book, None).unwrap_err();
        assert!(nobody.contains("Could not reach the site"), "{nobody}");
        // In Spanish too.
        let spanish = with_language(Language::Spanish, || download("http://127.0.0.1:9", &book, None).unwrap_err());
        assert!(!spanish.contains("Could not reach"), "{spanish}");
    }

    #[test]
    fn a_file_must_have_the_size_and_checksum_expected() {
        let book = made_up(b"the songbook");
        assert!(check(&book, b"the songbook").is_ok());
        assert!(check(&book, b"the songbooK").is_err());
        assert!(check(&book, b"the songbook!").is_err());
        assert_eq!(checksum(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }

    fn song(title: &str, words: &str) -> Presentation {
        sp_model::import_song_text(title, words)
    }

    #[test]
    fn each_song_is_marked_with_its_songbook_and_number_and_knows_when_it_is_changed() {
        let pv = by_id("pv3300").unwrap();
        let songs = vec![
            (song("1. Words for a test", "Verse 1\nA line made up"), None),
            (song("2. Another", "Verse 1\nAnother line"), None),
            (song("2. The same number", "Verse 1\nA third line"), None),
            (song("Untitled one", "Verse 1\nA fourth line"), None),
        ];
        let marked = mark(pv, songs, 1_764_000_000);
        let keys: Vec<&str> = marked.iter().map(|song| song.catalog.as_ref().unwrap().key.as_str()).collect();
        assert_eq!(keys, ["pv3300/1", "pv3300/2", "pv3300/2-2", "pv3300/#4"]);
        let origin = marked[0].catalog.as_ref().unwrap();
        assert_eq!((origin.source.as_str(), origin.language.as_str()), (SOURCE, "ru"));
        assert_eq!(songbook_of(&marked[0]).map(|book| book.id), Some("pv3300"));
        assert!(marked.iter().all(untouched));
        // The song catalog's songs aren't these.
        assert!(
            crate::catalog_songs::copies(&marked.iter().cloned().map(std::sync::Arc::new).collect::<Vec<_>>(), |_| {
                false
            })
            .is_empty()
        );
        // Any change makes it the church's.
        let mut changed = marked[1].clone();
        changed.title.push_str(" (ours)");
        assert!(!untouched(&changed));
        // Its ids don't count: the copy the library keeps has ids of its own.
        let mut filed = marked[1].clone();
        filed.id = sp_model::Uuid::new_v4();
        assert!(untouched(&filed));
        // A song with no mark isn't from a songbook.
        assert!(songbook_of(&song("x", "y")).is_none() && !untouched(&song("x", "y")));
    }

    #[test]
    fn remove_takes_only_songs_nobody_changed_and_nothing_else_uses() {
        let copy = |at: usize, key: &str, untouched: bool| InLibrary { at, key: key.to_owned(), untouched };
        let copies =
            [copy(3, "a", true), copy(4, "b", false), copy(5, "c", true), copy(6, "d", true), copy(7, "e", true)];
        // "c" is in a playlist or another library; 6 is open in the Edit view; 4 was changed.
        let used: HashSet<String> = ["c".to_owned()].into();
        let (gone, kept) = removal(&copies, &used, Some(6));
        assert_eq!(gone, [3, 7]);
        assert_eq!(kept, [4, 5, 6]);
    }

    #[test]
    fn a_row_says_how_its_songbook_stands() {
        with_language(Language::English, || {
            let mut view = View::default();
            let pv = by_id("pv3300").unwrap();
            assert_eq!(view.detail(pv), "3,301 songs \u{b7} 4.7 MB");
            assert_eq!(view.detail(by_id("vanoce").unwrap()), "22 songs \u{b7} 22 KB");
            view.known.busy.insert("pv3300", Some(42));
            assert_eq!(view.detail(pv), "Downloading \u{b7} 42%");
            view.known.busy.insert("pv3300", None);
            assert_eq!(view.detail(pv), "Downloading\u{2026}");
            view.known.busy.clear();
            view.known.held.insert("pv3300");
            assert!(view.detail(pv).contains("once nothing is on screen"));
            view.known.held.clear();
            view.known.installed.insert("pv3300", 3301);
            assert_eq!(view.detail(pv), "Installed \u{b7} 3,301 songs");
        });
    }

    #[test]
    fn a_songbook_read_from_its_file_is_a_marked_song_each() {
        // Reading needs the system's SQLite tool until the import has a reader of its own.
        if !Path::new("/usr/bin/sqlite3").exists() {
            return;
        }
        let folder = std::env::temp_dir().join(format!("softpresenter-test-free-songbooks-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        // A songbook written for the test, as the import reads one.
        let file = folder.join("made.sps");
        let sql = "CREATE TABLE SongBook (title TEXT, info TEXT); INSERT INTO SongBook VALUES ('Test book', '');\
                   CREATE TABLE Songs (id INTEGER, number INTEGER, title TEXT, words TEXT, music TEXT, song_text TEXT);\
                   INSERT INTO Songs VALUES (1, 1, 'First', '', '', 'Verse 1\nA line made up');\
                   INSERT INTO Songs VALUES (2, 2, 'Second', '', '', 'Verse 1\nAnother line made up');";
        let made = std::process::Command::new("/usr/bin/sqlite3").arg(&file).arg(sql).status().unwrap();
        assert!(made.success());
        let bytes = std::fs::read(&file).unwrap();
        let book = made_up(&bytes);
        let songs = read(&book, &bytes, &folder, 0).unwrap();
        // Each keeps its number before its title, as the import writes it.
        let titles: Vec<&str> = songs.iter().map(|song| song.title.as_str()).collect();
        assert!(titles.len() == 2 && titles[0].starts_with('1') && titles[0].contains("First"), "{titles:?}");
        assert!(titles[1].starts_with('2') && titles[1].contains("Second"), "{titles:?}");
        assert!(songs.iter().all(|song| songbook_of(song) == Some(&OFFERED[0]) && untouched(song)));
        // Nothing is left behind but the test's own file.
        let left: Vec<_> = std::fs::read_dir(&folder).unwrap().flatten().map(|entry| entry.file_name()).collect();
        assert_eq!(left, [std::ffi::OsString::from("made.sps")]);
        // Bytes that aren't a songbook: a plain sentence, and nothing left behind.
        let rubbish = read(&book, b"not a songbook", &folder, 0).unwrap_err();
        assert!(rubbish.contains("couldn\u{2019}t read it"), "{rubbish}");
        assert_eq!(std::fs::read_dir(&folder).unwrap().count(), 1);
        let _ = std::fs::remove_dir_all(&folder);
    }
}

//! Searching the catalog for the find box: words typed, found in a song's title or in the first
//! lines of its sections. Case, ё and е, stress marks and punctuation make no difference
//! (see `fold`), in Cyrillic and in Latin letters alike. The index of every language is searched
//! in memory; a search of 13,600 songs takes a few milliseconds.

use std::collections::HashSet;
use std::sync::Arc;

use super::fold::fold;
use super::store::{Catalog, Entry, LangIndex};

/// One catalog song a search found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hit {
    /// What names the song to everything else (`ru:свет-над-рекой~1a2b3c4d`).
    pub key: String,
    /// The catalog language it is in (`ru`).
    pub lang: String,
    pub title: String,
    /// The first line of its first section.
    pub line: String,
    /// The words were found in the title (else in a line).
    pub in_title: bool,
    /// The languages it also has versions in.
    pub versions: Vec<String>,
    pub chords: bool,
}

/// What a search found.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Found {
    /// The best songs, up to the limit asked for.
    pub hits: Vec<Hit>,
    /// How many songs matched in all.
    pub total: usize,
}

/// How well a song matched: lower is better.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Rank {
    /// The title is what was typed.
    Exact,
    /// The title starts with it.
    Starts,
    /// Every word is in the title.
    Title,
    /// Every word is in the title or a first line.
    Words,
}

impl Catalog {
    /// Looks in the catalog for `typed`: every word of it must be in the song's title or in the
    /// first lines of its sections. The songs whose title is what was typed come first, then
    /// those whose title starts with it, then those with every word in the title, then the rest;
    /// within each, songs of the languages in `prefer` (in that order) before others, then by
    /// title. A song in `skip` (those already in the library, by key) is left out. Languages
    /// that aren't downloaded add nothing.
    pub fn search(&self, typed: &str, limit: usize, prefer: &[String], skip: &HashSet<String>) -> Found {
        let indexes: Vec<Arc<LangIndex>> =
            self.installed().iter().filter_map(|lang| self.index(lang).ok().flatten()).collect();
        search_indexes(&indexes, typed, limit, prefer, skip)
    }
}

/// [`Catalog::search`] over indexes already read.
pub fn search_indexes(
    indexes: &[Arc<LangIndex>],
    typed: &str,
    limit: usize,
    prefer: &[String],
    skip: &HashSet<String>,
) -> Found {
    let query = fold(typed);
    let words: Vec<&str> = query.split(' ').filter(|w| !w.is_empty()).collect();
    if words.is_empty() {
        return Found::default();
    }
    let mut matched: Vec<(Rank, usize, &Entry, &str)> = Vec::new();
    for index in indexes {
        let language_rank = prefer.iter().position(|lang| *lang == index.lang).unwrap_or(prefer.len());
        for (entry, (title, rest)) in index.entries.iter().zip(&index.folded) {
            let rank = if *title == query {
                Rank::Exact
            } else if title.starts_with(&query) {
                Rank::Starts
            } else if words.iter().all(|word| title.contains(word)) {
                Rank::Title
            } else if words.iter().all(|word| rest.contains(word)) {
                Rank::Words
            } else {
                continue;
            };
            if skip.contains(&entry.key) {
                continue;
            }
            matched.push((rank, language_rank, entry, &index.lang));
        }
    }
    let total = matched.len();
    // The best few only: a partial sort, then the few put in order.
    let order = |a: &(Rank, usize, &Entry, &str), b: &(Rank, usize, &Entry, &str)| {
        (a.0, a.1, &a.2.title, &a.2.key).cmp(&(b.0, b.1, &b.2.title, &b.2.key))
    };
    if matched.len() > limit {
        if limit > 0 {
            matched.select_nth_unstable_by(limit - 1, order);
        }
        matched.truncate(limit);
    }
    matched.sort_by(order);
    let hits = matched
        .into_iter()
        .map(|(rank, _, entry, lang)| Hit {
            key: entry.key.clone(),
            lang: lang.to_owned(),
            title: entry.title.clone(),
            line: entry.line.clone(),
            in_title: rank != Rank::Words,
            versions: entry.langs.clone(),
            chords: entry.chords,
        })
        .collect();
    Found { hits, total }
}

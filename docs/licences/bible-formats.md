# Bible files in other formats: the XML library, and where the formats come from

A working note, not legal advice.

SoftPresenter installs Bibles kept in the formats other programs use (`crates/sp-bible/src/import.rs`):
Zefania XML, OSIS XML, Beblia XML, the XML that marks books, chapters and verses with `<b>`,
`<c>` and `<v>`, USFM, USX and SoftProjector's Bible files (`.spb`), each as a file or zipped.
They are read into SoftPresenter's own `.spbible` format when installed; the app never writes
them.

## The library

| Crate | Licence |
|---|---|
| quick-xml 0.41 (built with no optional features) | MIT |
| memchr (its one dependency, already used) | Unlicense OR MIT |

The zip files are read with the same `zip` and `flate2` crates as song packs
([song-packs.md](song-packs.md)), and text in older encodings with `sp_model::decode_text`. All
can be combined with the GNU GPL version 3, as NOTICE.md's "Other libraries" says.

quick-xml reads a document as a stream. It never expands an entity a document declares, never
fetches anything a document names, and passes over the document type declaration; SoftPresenter
reads only the five entities every XML document has and character references.

## The formats

Each was learnt from its own public description, and from Bible files the formats' and
translations' own publishers give out, but for SoftProjector's, whose layout was learnt from
SoftProjector's own source; no code comes from another program.

- **USFM** and **USX**: the Unified Standard Format Markers and its XML form, documented by the
  United Bible Societies (ubsicap.github.io/usfm and ubsicap.github.io/usx). Checked against the
  USFM files eBible.org publishes for the twenty Bibles SoftPresenter offers or carries: read
  this way, all but 68 of their 525,000 verses match the text those Bibles were made from, once
  square brackets and pilcrows are set aside (eBible's one-verse-per-line text writes the
  translators' added words in brackets, which a Bible read from USFM gives without them, as
  the King James has them). The rest: words the World English Bible has corrected since, the speakers' names in its Song of
  Songs (headings, now left out), the American Standard Version's Hebrew letters in Psalm 119
  (marked in its source as lines of the text, so kept), and seven verses of the Open New
  Ukrainian Translation whose last lines the earlier conversion lost.
- **OSIS**: the Open Scripture Information Standard 2.1.1 user manual (the Bible Technologies
  Group, crosswire.org/osis).
- **Zefania XML**: its schema, zef2005.xsd (the Zefania XML project on SourceForge).
- **Beblia XML**: the layout of the files Beblia publishes (github.com/Beblia/Holy-Bible-XML-Format).
- **`<bible>`, `<b n>`, `<c n>`, `<v n>`**: a plain layout several programs share, read by its
  shape.
- **SoftProjector's Bible files (`.spb`)**: learnt by reading SoftProjector's source code,
  published under the GPL version 3 (github.com/SoftProjector/softprojector), the part that
  exports and imports them: the first line giving the version, the lines about the Bible
  (title, abbreviation, information with its lines joined by `@%`, right to left), the books'
  lines (number, name, chapters), the line of dashes, and the verses' lines (an identifier,
  book, chapter, verse and words, with tabs between). Those are facts about the format, as for
  its songbooks ([song-documents.md](song-documents.md)); no code was copied, and SoftPresenter
  only reads the files. No SoftProjector Bible is shipped, or used in the tests.
- What a bundle says about its Bible: eBible.org's `copr.htm` (its title and language), a
  Door43 resource's `manifest.yaml`, and a Digital Bible Library bundle's `metadata.xml`.

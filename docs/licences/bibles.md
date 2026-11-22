# Bibles SoftPresenter may offer

Which Bible translations can be downloaded from inside the app, or shipped with it, and what
each one's terms ask. Researched on 2026-11-19. This is a working note, not legal advice, and it
comes from one pass of reading: **read each source again before the download is built**, and
before a version of the app is sold.

The rule for the list in `crates/sp-app/src/bible.rs`: a Bible is offered only if its terms let
anyone pass it on, including inside software that is sold, and only if those terms were read
at the source named here.

## Offered

"eBible" means the translation's own copyright page at `https://ebible.org/<ID>/copyright.htm`.
eBible states its public-domain findings as true in the United States to the best of its
knowledge.

| Language | Translation | eBible ID | Terms | What they ask |
|---|---|---|---|---|
| English | King James Version (1769 text) | `eng-kjv2006` | Public domain | In the United Kingdom the Crown holds printing rights; nothing elsewhere |
| English | American Standard Version (1901) | `eng-asv` | Public domain | Nothing |
| English | World English Bible | `engwebp` | Public domain | The name is a trademark: use it only on unchanged text |
| English | Berean Standard Bible | `engbsb` | Public domain since 30 April 2023 (berean.bible/licensing.htm) | Asks that changed text not carry the Berean name |
| English | Young's Literal Translation (1898) | `engylt` | Public domain | Nothing |
| English | Darby Translation (1884) | `engDBY` | Public domain | Nothing |
| English | Douay-Rheims, American edition (1899) | `engDRA` | Public domain | Nothing |
| English | Geneva Bible (1599) | `enggnv` | Public domain | Nothing |
| French | Louis Segond (1910) | `fraLSG` | Public domain | Nothing |
| French | Bible Darby (1885) | `frajnd` | Public domain | Nothing |
| German | Luther (1912) | `deu1912` | Public domain | Nothing |
| German | Elberfelder (1905) | `deuelo` | Public domain | Not `deuelbbk`, which is non-commercial |
| Greek | SBL Greek New Testament | `grcsbl` | CC BY 4.0 (sblgnt.com/license) | Credit SBL and Logos |
| Greek | Byzantine text, Robinson-Pierpont | `grcmt` | Public domain | Nothing |
| Greek | Septuagint, Brenton (1851) | `grcbrent` | Public domain | Nothing |
| Hebrew | Westminster Leningrad Codex | `hboWLC` | Text public domain | Its word-by-word data is CC BY-SA; we use the text only |
| Portuguese | Bíblia Livre (2018) | `porbr2018` | CC BY 4.0 | Credit; "BLIVRE" is enough on a slide |
| Russian | Синодальный перевод (1876) | `russyn` | Public domain | Nothing. Which printed edition the text follows was not confirmed |
| Russian | Russian Open Bible | Door43 `ru_rob` | CC BY-SA 4.0 | Credit; changed text stays under the same licence |
| Spanish | Reina-Valera (1909) | `spaRV1909` | Public domain | Nothing |
| Spanish | Versión Biblia Libre | `spavbl` | CC BY-SA 4.0 | Credit; changed text stays under the same licence |
| Ukrainian | Куліш and Пулюй (1905) | `ukr1871` | Public domain | Nothing |
| Ukrainian | Open New Ukrainian Translation, New Testament and Psalms (2022) | `ukronpu` | © 2022 Biblica, Inc., CC BY-SA 4.0 | Credit Biblica and the licence (the file carries both); anything made from the text is shared under the same licence; "Biblica" is a trademark, so it isn't used in the translation's name in the app. It follows the Synodal numbering (4 of its 410 chapters differ) |

Rules that follow from these terms:

- Show the text exactly as it is. Capitalisation and layout on a slide are fine; rewording is not.
- A Bible under a CC licence keeps its own licence and credit. Each installed Bible shows its
  terms (the "License" link beside each translation on the Bible page).
- These texts are data the app downloads or carries. They are not part of the app's own licence.

## Never offered

Their owners do not allow free redistribution, or forbid it in anything that is sold:
NIV, ESV, NLT, NKJV, NASB, CSB, Amplified, NET (may not be bundled with anything sold),
Reina-Valera 1960, Reina Valera Gómez, LBLA, the 1996 Ukrainian Bible, the modern Russian
translations (Новый Русский Перевод, the Russian Bible Society's translation). A church that
has bought one of these elsewhere cannot get it through SoftPresenter.

## Not decided

- **Огієнко (Ukrainian, 1962).** Widely copied as public domain, but no statement from a rights
  holder was found, again on 2026-11-22: the Ukrainian Bible Society's pages give no terms for
  it, YouVersion's listing names "British and Foreign Bible Society 1962" with no licence
  (bible.com/versions/186), and neither eBible nor Door43 carries it. Ohienko died in 1972, so
  under Ukraine's life-plus-seventy rule his work is protected until 2042. Left out until a
  rights holder's own terms allow it.
- **Almeida (Portuguese)** and the **Bible in Basic English**: public-domain status unclear outside
  the United States.
- **Incomplete or draft texts** that are free but not ready to project from: Open English Bible
  (CC0, not all books), Библия свободы (Russian Freedom Bible, eBible `rusfb`: public domain and
  now all sixty-six books, but its page still calls it "черновой вариант перевода", a draft,
  read 2026-11-22).

## Checked on 2026-11-22 and not offered

For Russian, Ukrainian and English first: every translation eBible lists in those languages,
and Door43's Russian and Ukrainian Bibles.

- **Russian.** Besides the Synodal, the Russian Open Bible (below, now offered) is the only
  finished, openly licensed whole Bible. Not offered: Библия свободы (a draft, above); Door43's
  Russian Unlocked Literal Bible (`ru_ulb`, CC BY-SA 4.0, the New Testament only; free, and
  could be added); Door43's other Russian texts, which are unfinished translators' aids
  (`ru_rlob` 31 books, `ru_rsob` 26, `ru_ust` 16, `ru_rost` 15 files though listed as 66) or
  not Russian at all (`ru_glt` holds the English unfoldingWord Literal Text); the New Russian
  Translation and the Central Asian Russian Scriptures ("All rights reserved", Biblica and
  IMB-ERTP, as Bible Gateway lists them).
- **Ukrainian.** Огієнко (above). Біблія свободи (eBible `ukrfb`, public domain): the Kulish
  and Puluj text with a few changes (31,052 of its 31,082 verses are the same as the offered
  UKR1905), so nothing new. The 1996 Bob Jones University Bible (`ukr1996`): copyright, never
  offered. Door43 has no Ukrainian Bible.
- **English**, free but not added: the World Messianic Bible (public domain; the World English
  Bible with Hebrew names), the Orthodox Jewish Bible (CC BY 4.0, Artists for Israel
  International; a Messianic Jewish wording), the Translation for Translators (CC BY-SA 4.0,
  Ellis W. Deibler; made for Bible translators, it writes out what the text implies), Tyndale's
  New Testament of 1534 (public domain; New Testament only, in its first spelling) and the
  Jewish translations of the Old Testament alone (JPS 1917, Leeser, the Masoretic English
  Tanakh; public domain). Any of these could be added.
- **English, turned down:** the English Majority Text Version (CC BY-NC-ND 4.0: non-commercial,
  no changes) and the Text-Critical English New Testament (CC BY 4.0, but its name may not be
  used once the translation or its footnotes are changed in any way, and the app leaves its
  footnotes out). The NET Bible, as before.

## Where the text comes from

- **eBible.org**, one zip per translation (`https://ebible.org/Scriptures/<ID>_vpl.zip`). The
  "VPL" form is one verse per line (`GEN 1:1 text`), which is all a slide needs. Each zip
  carries the translation's own copyright page. eBible's list marks many texts
  "redistributable" that are non-commercial only, so the list above goes by each
  translation's own page, never by that flag.
- **Door43** (`git.door43.org`) for the Russian Open Bible, in USFM.
- Not used: collections on GitHub that mix in copyrighted Bibles, and the online Bible APIs
  whose terms forbid keeping the text for use without a connection.

## What is built

- **Four Bibles come with the app**: King James, World English, Berean Standard and the
  Synodal. All four are public domain with no conditions. Their files are in `assets/bibles/`.
- **Our file format** (`.spbible`) is the publisher's words, one verse per line, with a few
  lines saying which Bible it is, its terms and its source, compressed. `cargo run -p sp-bible
  --example convert` makes one from eBible's one-verse-per-line text. The only changes to the
  words: the King James source marks paragraphs with a pilcrow and the translators' added
  words with square brackets, and those marks are removed (the words stay).
- **Verse numbering.** Bibles are lined up with the standard numbering tables from SIL's
  libpalaso (MIT licence; see `crates/sp-bible/data/README.md`): English, the Russian Synodal's
  Protestant edition, and the original-language numbering. Each Bible file names the one it
  follows, chosen by comparing its chapter lengths with every table. Checked against the
  shipped texts: of 31,102 King James verses, 2 have no partner in the Synodal text (Acts
  19:41 and 2 Corinthians 13:14, which it joins to the verse before), and 8 pairs of English
  verses share one Russian verse because the Synodal joins them.

## Downloads

- The Bibles under "Free" are files attached to the project's `bibles-3` release on GitHub (`bibles-2` holds the first fifteen, unchanged, for older versions of the app).
  Install fetches `<id>.spbible` from there, checks it against the SHA-256 written into the
  app for that Bible, checks that it reads as that Bible, and only then keeps it in the
  Bibles folder (`~/Library/Application Support/SoftPresenter/Bibles` on a Mac). A file with
  any other checksum is not installed.
- Dropping a `.spbible` file on the window installs it too, and so does a Bible file in another program's format (Zefania, OSIS, Beblia, USFM, USX and others; see [bible-formats.md](bible-formats.md)): a church installs only what it has the right to use.
- To publish a new or corrected Bible: convert it, attach it to a new release (the next `bibles-N`, with every file the app offers),
  and update its checksum and the release name in the app. Files in a release that the app
  already points at must never be replaced, or older versions of the app will refuse them.
- Offered now (16): ASV, Young's, Darby, Geneva, Louis Segond, French Darby, Luther 1912,
  Elberfelder 1905, SBL Greek New Testament, Byzantine, Westminster Leningrad Codex, Bíblia
  Livre, Reina-Valera 1909, Versión Biblia Libre, Kulish, and the Open New Ukrainian
  Translation (added 2026-11-19 in `bibles-3`).
- Not offered yet, though free: Douay-Rheims and Brenton's Septuagint (they have books beyond
  the sixty-six, which the app does not handle yet) and the Russian Open Bible (published in a
  different file format).
- **Numbering.** The French Darby, the Hebrew text and the SBL Greek follow the
  original-language numbering exactly. Geneva and Kulish mostly follow the English numbering
  (they differ from it in 40 and 55 chapters, by a verse or so) and Louis Segond mostly follows
  the original (54 chapters differ). Those three are marked as a loose fit, and the Bible page
  says that beside another Bible a verse here and there may be one off.
- **Brackets.** Several of these texts mark words the translators added with square brackets.
  They are shown as published; only the King James has them removed. eBible's one-verse-per-line
  text writes the USFM mark for added words as brackets; a Bible converted from USFM (with
  `cargo run -p sp-bible --example import`, as the app installs a file) keeps those words
  without brackets, as the King James has them. Brackets that are part of a translation's own
  text stay.

## Still to work out

- Exact tables for the three loosely fitting Bibles (Geneva, Kulish, Louis Segond).
- Books beyond the sixty-six.
- The Open New Ukrainian Translation's file (`ukronpu`, as published in `bibles-3`) lost the
  end of seven verses in its conversion from eBible's one-verse-per-line text: "Амінь і амінь!"
  after Psalms 40:14, 71:19 and 88:53, and the last lines of Revelation 1:7, 2:27, 4:8 and
  5:13. Converted from its USFM they are whole (found by comparing the two, 2026-11-22), but
  its added words then lose the brackets the published file shows. A corrected file needs the
  owner's choice of the two; until then the published one is offered unchanged.

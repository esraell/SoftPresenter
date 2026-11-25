# Bibles SoftPresenter may offer

Which Bible translations can be downloaded from inside the app, or shipped with it, and what
each one's terms ask. Researched on 2026-11-19; the Bibles added for `bibles-4` on 2026-11-22
and 2026-11-25. This is a working note, not legal advice, and it
comes from one pass of reading: **read each source again before the download is built**, and
before a version of the app is sold.

The rule for the list in `crates/sp-app/src/bible.rs`: a Bible is offered only if its terms let
anyone pass it on, including inside software that is sold, and only if those terms were read
at the source named here.

## Offered

A source in `code` alone is an eBible ID: the terms were read on the translation's own
copyright page at `https://ebible.org/<ID>/copyright.htm`. eBible states its public-domain
findings as true in the United States to the best of its knowledge. "CrossWire" names a
module of the CrossWire Bible Society's SWORD library: the terms were read in that module's
own description (its `.conf`: `DistributionLicense`, `Copyright`, `About`, `TextSource`), as
CrossWire publishes it (`https://www.crosswire.org/sword/modules/ModInfo.jsp?modName=<module>`).
The rows marked † were read on 2026-11-22, the rows marked ‡ on 2026-11-25 (both for
`bibles-4`), the rest on 2026-11-19.

A "Public domain" from a source was taken as it stands only for a text first published in 1929
or earlier with no translator known to have died in the last seventy years, or for a later
text its own rights holder dedicated or licensed (the New Heart English Bible, the Updated King
James Version, A Conservative Version, the Open English Bible), or whose rights holder says its term has run
out (개역한글). Anything else stays under "Not decided" below, as
Огієнко does.

| Language | Translation | Source | Terms | What they ask |
|---|---|---|---|---|
| Arabic | Van Dyck (1865) ‡ | `arb-vd` | Public domain | Nothing. eBible's copy, not CrossWire's `AraSVD`, whose digital edition is for non-commercial use |
| Armenian | Western Armenian New Testament (1853) ‡ | CrossWire `ArmWestern` | Public domain | Nothing |
| Basque | Leiçarraga's New Testament (1571) ‡ | CrossWire `BasHautin` | Public domain | Nothing |
| Chinese | Chinese Union Version (1919), traditional and simplified ‡ | CrossWire `ChiUn`, `ChiUns` (from bible.fhl.net) | Public domain (the modules' Copyright and DistributionLicense) | Nothing. Not eBible's 1988 new-punctuation edition, whose status is less clear. The spaces the traditional module puts between words for its Strong's numbers are taken out (Chinese has none); the wide space before 神 stays |
| Czech | Bible kralická (1613) ‡ | `ces1613` | Public domain | Nothing. Three stray "#" marks are taken out |
| Danish | Old Testament 1871, New Testament 1907 ‡ | CrossWire `DaOT1871NT1907` | Public domain ("Public Domain. Copy freely.", the module's About) | Nothing. Footnote asterisks are left out |
| Dutch | Statenvertaling ‡ | CrossWire `DutSVV` | Public domain | Nothing. The one apocryphal book it carries (Baruch) is left out |
| English | King James Version (1769 text) | `eng-kjv2006` | Public domain | In the United Kingdom the Crown holds printing rights; nothing elsewhere |
| English | American Standard Version (1901) | `eng-asv` | Public domain | Nothing |
| English | World English Bible | `engwebp` | Public domain | The name is a trademark: use it only on unchanged text |
| English | Berean Standard Bible | `engbsb` | Public domain since 30 April 2023 (berean.bible/licensing.htm) | Asks that changed text not carry the Berean name |
| English | Young's Literal Translation (1898) | `engylt` | Public domain | Nothing |
| English | Darby Translation (1884) | `engDBY` | Public domain | Nothing |
| English | Douay-Rheims, American edition (1899) | `engDRA` | Public domain | Nothing |
| English | Geneva Bible (1599) | `enggnv` | Public domain | Nothing |
| English | Literal Standard Version (2020) † | `englsv` | © 2020 Covenant Press, CC BY-SA 4.0 | Credit with the copyright and source; say that changes were made; changed text stays under the same licence. The file's credit says so, and that notes and headings are left out |
| English | Majority Standard Bible † | `engmsb` | Public domain ("dedicated to the public domain", Berean Bible Translation Committee) | Nothing |
| English | World English Bible British Edition † | `engwebpb` | Public domain | As the World English Bible: the name is a trademark, for unchanged text only |
| English | Free Bible Version † | `engfbv` | © 2018 Dr. Jonathan Gallagher, CC BY-SA 4.0 | As the Literal Standard Version |
| English | Unlocked Literal Bible † | `engULB` | © 2017 Door43 World Missions Community, CC BY-SA 4.0 | As the Literal Standard Version |
| English | Revised Version (1885) † | `eng-rv` | Public domain | Nothing. eBible's edition carries the Apocrypha (1895); they are left out |
| English | Noah Webster Bible (1833) † | `engwebster` | Public domain | Nothing |
| English | JPS Tanakh (1917) ‡ | `engjps` | Public domain | Nothing. The Old Testament only |
| English | Isaac Leeser Tanakh (1853) ‡ | `englee` | Public domain | Nothing. The Old Testament only; the Hebrew verse numbers it prints in brackets, such as "(7:26)", are left out |
| English | Noyes Translation (1869) ‡ | `engnoy` | Public domain | Nothing. The New Testament and Job to Malachi, which is all Noyes translated; its Hebrew verse numbers in brackets are left out |
| English | Open English Bible ‡ | `engoebus` | Public domain (CC0, openenglishbible.org) | Nothing. Unfinished: the New Testament and 17 books of the Old, and its name says so in the app |
| English | Tyndale New Testament (1534) ‡ | `engtnt` | Public domain | Nothing. In its first spelling |
| English | World Messianic Bible ‡ | `engwmb` | Public domain (dedicated by its translators) | The name is a trademark: for unchanged text only. The World English Bible with Hebrew names; it stands for the Hebrew Names Version too |
| English | King James Version, Pure Cambridge Edition ‡ | CrossWire `KJVPCE` | Public domain | As the King James Version |
| English | Updated King James Version (2000) ‡ | CrossWire `UKJV` | Public domain (the translator's own dedication, in the module's About) | Asks, without requiring it, for the credit "I love Jesus (UKJV)." (the file carries it) and no changes to the words |
| English | New Heart English Bible ‡ | CrossWire `NHEB` | Public domain ("dedicated to the Public Domain by the editors and translators", Wayne A. Mitchell, 2009) | Nothing |
| English | A Conservative Version ‡ | CrossWire `ACV` | Public domain (its translator, Walter L. Porter) | Nothing |
| English | Rotherham's Emphasised Bible (1902) ‡ | CrossWire `Rotherham` | Public domain | Nothing. Its "[****]" section marks in the Song of Songs are left out |
| English | Weymouth New Testament (1912) ‡ | CrossWire `Weymouth` | Public domain | Nothing. The asterisks marking emphasis are left out |
| English | Montgomery New Testament (1924) ‡ | CrossWire `Montgomery` | Public domain (Helen Barrett Montgomery died in 1934) | Nothing. Formatting codes left in the digital text are removed |
| English | Twentieth Century New Testament (1904) ‡ | CrossWire `Twenty` | Public domain | Nothing |
| English | Godbey New Testament (1902) ‡ | CrossWire `Godbey` | Public domain | Nothing |
| English | The Living Oracles (1826) ‡ | CrossWire `LO` | Public domain by age: Alexander Campbell's edition of 1826 (he died in 1866); the module itself states no licence | Nothing |
| English | Etheridge Peshitta New Testament (1846–1849) ‡ | CrossWire `Etheridge` | Public domain | Nothing |
| Esperanto | Londona Biblio (1926) ‡ | `epo` | Public domain | Nothing |
| Finnish | Biblia (1776) ‡ | CrossWire `FinBiblia` | Public domain | Nothing. The Apocrypha and footnote asterisks are left out |
| French | Louis Segond (1910) | `fraLSG` | Public domain | Nothing |
| French | Bible Darby (1885) | `frajnd` | Public domain | Nothing |
| French | Bible annotée (1900) ‡ | CrossWire `FreBA` | Public domain | Nothing |
| French | Bible David Martin (1744) ‡ | CrossWire `FreBDM1744` | Public domain | Nothing. Not CrossWire's `FreBDM1707`, whose digital edition is not free |
| French | Bible Perret-Gentil et Rilliet (1861, 1858) ‡ | CrossWire `FrePGR` | Public domain | Nothing. Typesetting codes left in the digital text are removed |
| German | Luther (1912) | `deu1912` | Public domain | Nothing |
| German | Elberfelder (1905) | `deuelo` | Public domain | Not `deuelbbk`, which is non-commercial |
| German | Albrecht, Neues Testament und Psalmen (1926) ‡ | CrossWire `GerAlbrecht` | Public domain (Ludwig Albrecht died in 1931) | Nothing. Footnote numbers are left out |
| German | Tafelbibel (1911) ‡ | CrossWire `GerTafel` | Public domain | Nothing. This edition has no Hagiographa (Ruth, Chronicles, Ezra, Nehemiah, Esther, Job, Proverbs, Ecclesiastes, Song of Songs), and its name says so in the app; quotation marks mis-encoded in the digital text are put right |
| German | Textbibel (1906) ‡ | `deutkw` | Public domain | Nothing |
| Greek | SBL Greek New Testament | `grcsbl` | CC BY 4.0 (sblgnt.com/license) | Credit SBL and Logos |
| Greek | Byzantine text, Robinson-Pierpont | `grcmt` | Public domain | Nothing |
| Greek | Septuagint, Brenton (1851) | `grcbrent` | Public domain | Nothing |
| Greek | Αγία Γραφή, Vamvas (1850) ‡ | CrossWire `GreVamvas` | Public domain | Nothing |
| Greek | Textus Receptus ‡ | `grctr` | Public domain ("firmly in the Public Domain due to its age"; Adam Boyd's notes are dedicated to it and left out) | Nothing. Not CrossWire's `TR`, which is CC BY-NC-SA |
| Greek | Tischendorf 8th Edition ‡ | `grc-tisch` | Public domain ("This text and its analysis are in the Public Domain. Copy freely.") | Nothing |
| Hebrew | Westminster Leningrad Codex | `hboWLC` | Text public domain | Its word-by-word data is CC BY-SA; we use the text only |
| Hebrew | The Old Testament with Delitzsch's New Testament ‡ | `heb` | Public domain (Franz Delitzsch, 1813–1890) | Nothing. Without vowel points |
| Hungarian | Károli Biblia (1908 revision) ‡ | CrossWire `HunKar` | Public domain | Nothing |
| Italian | Diodati (1649) ‡ | CrossWire `ItaDio` | Public domain | Nothing. A note from the Unbound Bible left at the end of Revelation is removed |
| Italian | Riveduta (1927) ‡ | `ita1927` | Public domain (Giovanni Luzzi died in 1948) | Nothing |
| Klingon | Klingon Language Version ‡ | CrossWire `KLV` | Public domain | As the World English Bible, which it puts into Klingon words (its own words: a demonstration, not a translation); the World English Bible's notes it carries in braces are left out |
| Korean | 개역한글 (1961) ‡ | CrossWire `KorRV` (from Wikisource) | Public domain: its rights holder, the Korean Bible Society, says its economic rights ended on 31 December 2011 ("2011년 12월 31일 저작재산권 보호기간 소멸", bskorea.or.kr, its page on the copyright of its Bibles, read 2026-11-25) | Its moral rights stay with the Korean Bible Society, so the words are shown unchanged; its later revisions (개역개정) are still protected |
| Malagasy | Baiboly Malagasy (1865) ‡ | CrossWire `Mg1865` | Public domain | Nothing |
| Norwegian | Studentmållagsbibelen (1921) ‡ | CrossWire `NorSMB` | Public domain | Nothing. Footnote marks are left out |
| Persian | Old Persian Version (1895) ‡ | `pesOPV` | Public domain | Nothing. A modern editor's note in Psalm 121:1 and two stray marks are left out |
| Polish | Biblia Gdańska (1881) ‡ | CrossWire `PolGdanska` | Public domain | Nothing |
| Portuguese | Bíblia Livre (2018) | `porbr2018` | CC BY 4.0 | Credit; "BLIVRE" is enough on a slide |
| Russian | Синодальный перевод (1876) | `russyn` | Public domain | Nothing. Which printed edition the text follows was not confirmed |
| Russian | Russian Open Bible † | Door43 `ru_rob` | CC BY-SA 4.0, Door43 World Missions Community (read in the release's LICENSE.md and manifest.yaml, git.door43.org/Door43-Catalog/ru_rob, release v8 of 2021-06-23) | Credit; say that changes were made; changed text stays under the same licence. A modern Russian translation made from the Synodal by the Russian Open Bible Committee, at Door43's checking level 3 (its manifest); numbered as the Synodal |
| Serbian | Daničić-Karadžić, Latin and Cyrillic ‡ | `srp1865`, `srp1868` | Public domain | Nothing |
| Spanish | Reina-Valera (1909) | `spaRV1909` | Public domain | Nothing |
| Spanish | Versión Biblia Libre | `spavbl` | CC BY-SA 4.0 | Credit; changed text stays under the same licence |
| Spanish | Reina-Valera New Testament (1858) ‡ | CrossWire `SpaVNT` | Public domain ("This text is not copyrighted, you are free to distribute it as you wish", Bill Kincaid) | Asks that copies published on the internet keep its notice. The epistles' closing notes in braces are left out |
| Swedish | Bibeln (1917) ‡ | CrossWire `Swe1917` | Public domain (from Projekt Runeberg) | Nothing. The Apocrypha and footnote numbers are left out |
| Swedish | Karl XII:s bibel (1873 edition) ‡ | CrossWire `SweKarlXII1873` | Public domain | Nothing |
| Tagalog | Ang Dating Biblia (1905) ‡ | CrossWire `TagAngBiblia` | Public domain | Nothing |
| Ukrainian | Куліш and Пулюй (1905) | `ukr1871` | Public domain | Nothing |
| Ukrainian | Open New Ukrainian Translation, New Testament and Psalms (2022) | `ukronpu` | © 2022 Biblica, Inc., CC BY-SA 4.0 | Credit Biblica and the licence (the file carries both); anything made from the text is shared under the same licence; "Biblica" is a trademark, so it isn't used in the translation's name in the app. It follows the Synodal numbering (4 of its 410 chapters differ) |
| Ukrainian | Новий Завіт, переклад П. Куліша (1871) ‡ | CrossWire `UkrKulish` | Public domain (Kulish died in 1897) | Nothing. The New Testament only; the whole Bible of 1905 (Kulish and Puluj) is offered above |

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
has bought one of these elsewhere cannot get it through SoftPresenter. More, checked on
2026-11-25, are under "Checked on 2026-11-25" below.

## Not decided

- **Огієнко (Ukrainian, 1962).** Widely copied as public domain, but no statement from a rights
  holder was found, again on 2026-11-22: the Ukrainian Bible Society's pages give no terms for
  it, YouVersion's listing names "British and Foreign Bible Society 1962" with no licence
  (bible.com/versions/186), and neither eBible nor Door43 carries it. Ohienko died in 1972, so
  under Ukraine's life-plus-seventy rule his work is protected until 2042. Left out until a
  rights holder's own terms allow it.
  CrossWire's `UkrOgienko` says "Public Domain" (read 2026-11-25), but CrossWire is not the
  rights holder, so nothing changes.
- **Almeida (Portuguese)** and the **Bible in Basic English**: public-domain status unclear outside
  the United States. CrossWire's `PorAlmeida1911` is offered under the GPL, but that is
  CrossWire's licence for its digital edition, not a rights holder's.
- **Draft texts** that are free but not ready to project from: Библия свободы (Russian Freedom
  Bible, eBible `rusfb`: public domain and now all sixty-six books, but its page still calls it
  "черновой вариант перевода", a draft, read 2026-11-22), and the Japanese Freedom Bible (eBible
  `jpnm`, "これは翻訳の草案です", a draft, read 2026-11-25). The Open English Bible, unfinished
  but not a draft, is now offered with its name saying what it holds.
- **Checked on 2026-11-25** (details in the next section): the Finnish Bible of 1933/38, the
  Norwegian Bible of 1930, Cornilescu (Romanian), Šarić (Croatian), the Grünewald Bible
  (German), the Haitian Creole Bible of 1985, and the Japanese Kougo-yaku (free, but CrossWire's
  module is missing chapters).

## Checked on 2026-11-22 and not offered

For Russian, Ukrainian and English first: every translation eBible lists in those languages,
and Door43's Russian and Ukrainian Bibles.

- **Russian.** Besides the Synodal, the Russian Open Bible (in the table above) is the only
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

## Checked on 2026-11-25

Every Bible on a list of about three hundred, in many languages, that the owner shared, asking
for all the free ones. The list grants no terms of its own, so each text was looked up at a
primary source (eBible or CrossWire, as above) and judged by the terms there; nothing was taken
from the list's own files. The fifty-three in the table marked ‡ were added. The rest, by why:

- **Already offered or built in:** the King James, World English, Berean Standard and
  Synodal (built in); ASV, Young's, Darby, Geneva, Louis Segond, French Darby, Luther 1912,
  Elberfelder 1905, SBL Greek New Testament, Byzantine text, Westminster Leningrad Codex,
  Bíblia Livre, Reina-Valera 1909, Kulish and Puluj (offered); the Revised Version, Webster
  (which also stands for the "Revised Webster", the same 1833 text) and the World English
  Bible British Edition (added on 2026-11-22). The Synodal's other editions on the list (with
  the deuterocanonical books, the Moscow Patriarchate's, "77 books") are the same 1876 text,
  whose sixty-six books are built in.
- **Copyright, not free to pass on** (or non-commercial only, or no changes allowed): the
  modern English translations (NIV, ESV, NKJV, NASB, NLT, CSB/HCSB, NRSV, RSV, The Message,
  Amplified, NET, ISV, LEB/Lexham, NCV, NIrV, TNIV, Good News, New Jerusalem, NAB, Complete
  Jewish Bible, GOD'S WORD, Modern KJV, KJ21, Third Millennium, Easy-to-Read, CEV, New Life,
  Jubilee 2000, English Majority Text, Literal Translation and Green's Literal, Apostles' Bible,
  Apostolic Polyglot, Bible in Worldwide English, Common Edition New Testament, American King
  James (CrossWire now marks it non-commercial), the Recovery Version); Afrikaans 1953;
  Amharic (1962, and Haile Selassie's); the Indian languages' WBTC and Bible Society of India
  editions (but for the Malayalam of 1910, added); Azerbaijani; Breton (Koad 21); Bulgarian
  (1914 Tsarigrad: CrossWire has permission only for itself; Veren's; the Bulgarian Bible);
  Chinese New Version; Coptic Sahidica (non-commercial); Czech ČSP (CC BY-NC-ND), B21, ČEP,
  KMS, Nová Bible kralická (CC BY-ND); Danish 1931 (its Old Testament is © 1931 Det Danske
  Bibelselskab); Dari; Persian Today's Version and Hezare Noh; Finnish 1992, Raamattu Kansalle,
  STLK 2017; Ge'ez (non-commercial); German NeÜ, Schlachter 1951, Hoffnung für alle,
  Leonberger; Greek Elzevir, Antoniades, Westcott-Hort, Tregelles, CrossWire's TR (all CC
  BY-NC-SA), the Septuagint (Rahlfs, non-commercial); BHS; Icelandic; Indonesian TB, BIS and
  TL; Italian Nuova Diodati and Nuova Riveduta; Kekchi (CC BY-NC-ND); Khmer; Korean versions
  other than 개역한글 (Hangul KJV non-commercial; the rest copyright); Lithuanian; Mongolian
  KJV; Brød (Norwegian); Polish Updated Gdańsk (non-commercial) and the Warsaw Bible; the
  Portuguese Almeida Atualizada and Corrigida (Sociedade Bíblica do Brasil), Almeida Recebida
  and Capuchinhos; Cornilescu plus; the modern Russian ones (Агапэ, Восточный перевод,
  Юбилейная Библия/Свет на Востоке, НРП, РБО 2011, Слово Жизни, ВБПЦ, Журомский's New
  Testament) and the Synodal of Licht im Osten; Slovak (all of them); Slovenian (SloChraska
  and SloKJV, CC BY-NC-ND); Somali; Spanish LBLA, DHH, Biblia Textual, NVI, RVG (CC BY-NC-ND),
  RV1960, RV1995, TLA; the Swahili Union Version; Swedish Folkbibeln; Tamajaq; Thai KJV (CC
  BY-NC-ND); Tigrinya; Turkish (HADI, the New Turkish Bible; the Turkish WEB is CC BY-ND);
  Ukrainian Гижа, УБТ, Турконяк, Хоменко; Uma (non-commercial); the Vietnamese versions after
  1934; Welsh (beibl.net); Wolof (CC BY-NC-ND).
- **Free, but carrying books the app can't hold yet** (deuterocanonical chapters inside Daniel
  and Esther, as Douay-Rheims above): the Douay-Rheims Challoner (CrossWire `DRC`), the
  Clementine Vulgate (eBible `latVUC`, CrossWire `VulgClementine`), Crampon 1923 (CrossWire
  `FreCrampon`, Catholic numbering), Wycliffe's Bible (CrossWire `Wycliffe`, CC BY-SA 4.0),
  the Septuagint (eBible `grclxx`; also Psalm 151 and the Greek order of Jeremiah).
- **Not decided** (status unclear, so not added):
  - Finnish Pyhä Raamattu 1933/38 (CrossWire `FinPR` says public domain; a church committee's
    translation of 1933 and 1938, its members' dates not found).
  - Norwegian 1930 (CrossWire `Norsk` says public domain; published 1930 by the Norwegian Bible
    Society, its translators' dates not found).
  - Cornilescu 1924 (Romanian): eBible's Cyrillic copy (`ron1924`) says the copyright has
    expired, CrossWire's (`RomCor`) says "Copyrighted"; Cornilescu died in 1975.
  - Šarić (Croatian, 1942; eBible `hrv`, CrossWire `CroSaric` both say public domain): Ivan
    Šarić died in 1960, so under life plus seventy it is protected in Croatia until 2030.
  - The Grünewald Bible (German, 1924; CrossWire `GerGruenewald`): Rupert Storr died in 1957.
  - Haitian Creole Bible 1985 (eBible `hat`): public domain in the United States only, through
    a missing copyright notice.
  - Luther 1545 (CrossWire `GerBoLut`): not the 1545 text but Michael Bolsinger's modernised
    one; the module, not its editor, calls it public domain.
  - Elberfelder "1871" (CrossWire `GerElb1871`): the module says the edition's date isn't known.
  - The Vietnamese Bible of 1923/1925 (eBible `vie1934`, CrossWire `Viet`, both "public domain"):
    eBible's dates are the United States' (95 years from publication). It was made by Grace and
    William Cadman and John Drange Olsen with the scholar Phan Khôi, who died in 1959 (find.bible,
    VIEVOV), so where the term is life plus seventy it may be protected until 2029. Built and
    checked (from eBible's copy, its capitals respelt from the older VNI spelling and six
    mis-encoded verses taken from CrossWire's copy), ready to add if the owner decides.
  - Pohnpeian New Testament and Psalms (eBible `pon`, `pon-pdn`; CrossWire `PohnOld`): A.
    Syring's revision of 1935, called public domain by those who typed it in (FSM Digital), not
    by its rights holder; Syring's dates not found.
  - Ostervald 1996 (French): eBible's `fra_fob` gives no edition or date.
  - NBG 1951 (Dutch, eBible `nldnbg`): offered under the GFDL by the person who typed it in,
    not by the Netherlands Bible Society; a GFDL text must also travel with the licence's whole
    text, which a Bible file can't carry. The Ndebele and Shona Bibles (CrossWire, GFDL, Free
    Presbyterian Church of Scotland) are unfinished, and GFDL too.
  - The Albanian Bible, Eastern Armenian (only Genesis, Exodus and the Gospels), Estonian,
    the Latvian New Testament, Maori, the Swahili New Testament, the Coptic New Testament
    (CrossWire `CopNT`) and the Bohairic one (eBible `copbhc`, a 2006 edition of Horner's text):
    no edition or rights holder named, or no terms at all.
  - The Aleppo Codex (CrossWire `Aleppo`): taken from Mechon Mamre, whose own terms are not
    the module's "Public Domain".
  - The Japanese Kougo-yaku (1955; CrossWire `JapKougo`): free (its copyright ended in Japan on
    1 January 2006, as the module says), but the module lacks Psalms 130 to 139, John 19 and
    Romans 10, so it waits for a whole text.
- **Free, but in a script the operator's Bible page can't draw yet** (checked 2026-11-25 on a
  Mac, in a test copy: the slides draw all but Coptic, through the system's fonts, but the page
  the operator reads from shows empty boxes): the Syriac Peshitta New Testament (British and
  Foreign Bible Society, 1905; CrossWire `Peshitta`, public domain; it stands for the list's
  "Aramaic Peshitta" too), Judson's Burmese Bible (1840; eBible `myajvb`, public domain), the
  Cherokee New Testament (1860; CrossWire `Che1860`, public domain), the Malayalam
  Sathyavedapusthakam (1910; CrossWire `Mal1910`, public domain; the page draws it with gaps and
  unjoined letters) and Horner's Sahidic Coptic New Testament (1911; CrossWire `CopSahHorner`,
  public domain; the slides can't draw it either). Their files are built and their terms read,
  so each can be added once the page has a font for its script.
- **Incomplete:** the Breton Gospels of 1827 (eBible `breBRG`, only four books), the Spanish
  "Traducción de dominio público" (Matthew to Romans), Karl XII's Bible of 1703 (CrossWire's
  module says it is not complete; its 1873 edition is added), Cebuano Pinadayag (a modern
  revision, rights not stated).
- **No primary source found:** Coverdale, the Bishops' Bible, the Updated Bible Version,
  Voice in the Wilderness, the Ararat Armenian Bible, Nias, Kabyle, Mizo, Paite, Romani (only a
  Gospel of Luke of 1837, and copyright portions), Xhosa, Zarma, Sagradas Escrituras 1569, Welsh
  1620, and the Esperanto text without diacritics (the same Londona Biblio, added once).
- **Free, not on the list, and could be added** for the owner to decide: Salkinson-Ginsburg's
  Hebrew New Testament (eBible `hebsg`), the 1965 Japanese New Testament (eBible `jpn1965`,
  copyright ended 2015), the Korean Bible of 1910 (eBible `kor`), Menge (German, 1939,
  CrossWire `GerMenge`, public domain since 2010 by the German Bible Society's word), the
  Diodati of 1885 (eBible `ita1885`), the Slovenian Stritar New Testament and Psalms (1882,
  CrossWire `SloStritar`), and modern translations in the list's languages that eBible or
  CrossWire show under open licences, whose own pages were not read this time: the Indian
  Revised Versions (Bridge Connectivity Solutions), Biblica's "Open" versions (as the
  Ukrainian НПУ above: Sorani, Hindi, Persian, Serbian, Swahili and others), Door43's Unlocked
  Literal Bibles (Cebuano, Tagalog, Swahili, Nepali) and the Offene Bibel (German, study
  edition).

Known flaws kept as published, too small to keep a Bible out: a few letters mis-encoded in the
Gdańska source (about ten, such as "Çwiczy" for "Ćwiczy"), some words run together in the
Kulish New Testament of 1871 (about fifteen) and in the Kralická (three), typing slips in the
Textbibel ("OffenbarungszeItes"), and Horner's angle brackets for restored Coptic letters.

## Where the text comes from

- **eBible.org**, one zip per translation (`https://ebible.org/Scriptures/<ID>_vpl.zip`). The
  "VPL" form is one verse per line (`GEN 1:1 text`), which is all a slide needs. Each zip
  carries the translation's own copyright page. eBible's list marks many texts
  "redistributable" that are non-commercial only, so the list above goes by each
  translation's own page, never by that flag.
- **Door43** (`git.door43.org`) for the Russian Open Bible, in USFM.
- **CrossWire** (`https://www.crosswire.org/ftpmirror/pub/sword/packages/rawzip/<module>.zip`)
  for the texts eBible doesn't carry: the SWORD module as CrossWire packages it, read by a
  script outside the repository that writes each verse into a plain OSIS file, which the
  app's own importer then converts (`cargo run -p sp-bible --example import`), as for any
  OSIS file. The script finds each verse by SWORD's own verse tables (CrossWire's
  `canon_*.h`); checked by converting CrossWire's ASV this way and comparing it with the
  offered ASV from eBible: the same 31,086 verses, every one alike (2026-11-25). A psalm's own
  title keeps its words at the start of its verse, as the importer keeps a USFM `\d`; section
  headings and notes are left out.
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

- The Bibles under "Free" are files attached to the project's `bibles-4` release on GitHub (`bibles-3` and `bibles-2` hold the earlier sets, unchanged, for older versions of the app).
  Install fetches `<id>.spbible` from there, checks it against the SHA-256 written into the
  app for that Bible, checks that it reads as that Bible, and only then keeps it in the
  Bibles folder (`~/Library/Application Support/SoftPresenter/Bibles` on a Mac). A file with
  any other checksum is not installed.
- Dropping a `.spbible` file on the window installs it too, and so does a Bible file in another program's format (Zefania, OSIS, Beblia, USFM, USX and others; see [bible-formats.md](bible-formats.md)): a church installs only what it has the right to use.
- To publish a new or corrected Bible: convert it, attach it to a new release (the next `bibles-N`, with every file the app offers),
  and update its checksum and the release name in the app. Files in a release that the app
  already points at must never be replaced, or older versions of the app will refuse them.
- Offered on 2026-11-22 (24): ASV, Young's, Darby, Geneva, Louis Segond, French Darby, Luther 1912,
  Elberfelder 1905, SBL Greek New Testament, Byzantine, Westminster Leningrad Codex, Bíblia
  Livre, Reina-Valera 1909, Versión Biblia Libre, Kulish, and the Open New Ukrainian
  Translation (added 2026-11-19 in `bibles-3`); the Literal Standard Version, Majority Standard
  Bible, World English Bible British Edition, Free Bible Version, Unlocked Literal Bible,
  Revised Version, Noah Webster Bible and Russian Open Bible (added 2026-11-22 for `bibles-4`,
  converted from their USFM with `cargo run -p sp-bible --example import`; the Revised
  Version's Apocrypha are left out, as the app keeps the sixty-six books).
- Added 2026-11-25 for `bibles-4` (53; 29 languages in all): every Bible in the table marked
  ‡, from a list the owner shared (see "Checked on 2026-11-25"). eBible's ones were converted
  from their USFM and CrossWire's from their OSIS, both with `cargo run -p sp-bible --example
  import`. The only changes to the words are the clean-ups each row names: marks an earlier
  conversion left that are not the publisher's words (footnote anchors, notes in braces,
  alternative verse numbers, typesetting codes), letters mis-encoded by one, and verses that
  hold only a placeholder ("-" or "[ ]") for a verse joined to the one before. Checked for each:
  its books, chapters and verses against the numbering tables, no markup left, and John 3:16
  (or Psalm 23) read against eBible's own page for ten of them, all identical.
- `bibles-4` holds all 77 files: the sixteen of `bibles-3`, byte for byte the same (the same
  checksums), the eight of 2026-11-22 and the fifty-three of 2026-11-25. The app asks for
  `<id>.pcbible`, the ending the files were first published with.
- Not offered yet, though free: Douay-Rheims and Brenton's Septuagint (they have books beyond
  the sixty-six within books the app has, such as Daniel and Esther, which the app does not
  handle yet).
- **Numbering.** The French Darby, the Hebrew text and the SBL Greek follow the
  original-language numbering exactly. Geneva and Kulish mostly follow the English numbering
  (they differ from it in 40 and 55 chapters, by a verse or so) and Louis Segond mostly follows
  the original (54 chapters differ). Those three are marked as a loose fit, and the Bible page
  says that beside another Bible a verse here and there may be one off. Of the fifty-three added
  on 2026-11-25, the Textbibel and the Malagasy follow the original-language numbering exactly;
  the Bible annotée, Martin, Kralická and Károli mostly follow it (77 to 83 chapters differ),
  and Albrecht, the Tafelbibel, Montgomery, the Western Armenian and Leiçarraga mostly follow
  the English or the original (up to 30 chapters differ): all marked as a loose fit. The rest
  follow the English numbering exactly.
- **Brackets.** Several of these texts mark words the translators added with square brackets.
  They are shown as published; only the King James has them removed. eBible's one-verse-per-line
  text writes the USFM mark for added words as brackets; a Bible converted from USFM (with
  `cargo run -p sp-bible --example import`, as the app installs a file) keeps those words
  without brackets, as the King James has them. Brackets that are part of a translation's own
  text stay.

## Still to work out

- Searching by words: both Serbian Bibles say `sr`, which the search takes as Cyrillic, so
  words typed in Latin letters are not looked for in the Latin-letter Daničić-Karadžić (DK)
  while another Latin-letter Bible is in use (it is, when it is the only one).

- Exact tables for the three loosely fitting Bibles (Geneva, Kulish, Louis Segond).
- Books beyond the sixty-six.
- The Open New Ukrainian Translation's file (`ukronpu`, as published in `bibles-3`) lost the
  end of seven verses in its conversion from eBible's one-verse-per-line text: "Амінь і амінь!"
  after Psalms 40:14, 71:19 and 88:53, and the last lines of Revelation 1:7, 2:27, 4:8 and
  5:13. Converted from its USFM they are whole (found by comparing the two, 2026-11-22), but
  its added words then lose the brackets the published file shows. A corrected file needs the
  owner's choice of the two; until then the published one is offered unchanged.

# Reading song documents saved by ProPresenter 7

SoftPresenter can import songs from the `.pro` documents ProPresenter 7 saves, so a church can
bring its library across (File › Import from ProPresenter…, or dropping the files or their
folders on the window), and from the `.proBundle` bundles it exports a song in with its pictures
and videos. It only ever reads them; it never writes either format.

## Where the format comes from

The documents are Protocol Buffers. SoftPresenter's reader
(`crates/sp-model/src/pro_file.rs`) uses the field numbers from the unofficial `.proto`
definitions published by greyshirtguy at
[github.com/greyshirtguy/ProPresenter7-Proto](https://github.com/greyshirtguy/ProPresenter7-Proto),
which their author worked out as a user of the program. Nothing in SoftPresenter comes from
decompiling ProPresenter or from its own files. The definitions themselves are not copied into
this repository: the reader is SoftPresenter's own code and reads only the few fields a song
needs.

For a bundle's song, the reader also reads which picture or video each slide shows and where
its file is (the media action, its media element and that element's address), from the same
definitions.

## Bundles

A `.proBundle` is an ordinary zip file holding one `.pro` document and the pictures and videos it
uses. That much, and the two ways the files inside are named (the whole path the file had on the
computer it came from, for a bundle the program exports; a bare file name, for one other tools
write), and that exported archives give the place of their ZIP64 end record wrongly, come from
public descriptions by people who read the files as users: the prose format notes of the
MIT-licensed propresenter7-php-lib project, and an open-source presentation program's notes
that it unpacks bundles as zip files. No code was copied from either, and no bundle from the
program was taken apart to learn it: the tests make their own bundles in both layouts.
SoftPresenter's reader (`crates/sp-app/src/bundle.rs`) reads the zip file with the same zip
library song packs use, takes out only the song and the pictures and videos it shows, and puts
the one wrong number right as it reads.

ProPresenter is a trademark of Renewed Vision, LLC. SoftPresenter is not affiliated with or
endorsed by Renewed Vision, and names ProPresenter only to say which documents it can read.

## The licence of the definitions

The definitions are under the MIT licence, which asks that its notice be kept with copies of
the work. It is reproduced in [NOTICE.md](../../NOTICE.md).

## Why this is an exception

[AGENTS.md](../../AGENTS.md) otherwise forbids file formats taken from other products. The
owner agreed this one exception on 2026-11-19, for reading a church's own songs only, through
the definitions above. It does not extend to writing the format, to other files of that
program, or to any other product. Reading the bundles a song is exported in (the same song
document in a zip file, with its pictures and videos) was asked for by the owner on 2026-11-21.

## SoftProjector song databases and songbooks

SoftPresenter also imports songs from SoftProjector (its `spData.sqlite` database and the
`.sps` songbooks it exports), so a church moving from it keeps its songs. The layout was learnt
by reading SoftProjector's source code, which is published under the GPL version 3
(github.com/SoftProjector/softprojector): the names of its tables and columns, and how it splits
a song's text into verses and choruses. Those are facts about the format; no code was copied.
The files are read with macOS's own `sqlite3`, read-only. The songs in a church's own
SoftProjector files are the church's to bring across; the songbooks SoftProjector offers for
download carry their publishers' copyrights, and SoftPresenter does not ship any of them.

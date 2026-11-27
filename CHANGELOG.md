# What's new in SoftPresenter

What changed in SoftPresenter, newest first, for the volunteers who run services and the people testing it.
Versions are numbered 0.x until version 1, and until then things may still change from one build to the next.

## Unreleased

Your changes are safer, keys you can set yourself (with a layout like SoftProjector's), a mark for the end of a song, opening a video no longer holds up the window, and Remote Control uses less of the computer.

### New

- **Key layout.** Settings › Keys has a switch, Standard or Like SoftProjector, that gives Esc and the F keys the jobs they have in SoftProjector. In Like SoftProjector, Esc hides the words and leaves the background up, F5 shows what is picked, F2 adds to the playlist and F1 clears everything; on a Mac, where the F keys are the system's, only Esc and "." change.
- **Set what each key does.** Settings › Keys lists what each key that runs the show does, with a chooser beside each to change it. Esc, F1 to F12 (not on a Mac), Space, the arrows, Page Down and Page Up, Home, End and "." each do one thing or nothing; a changed row says what it did at first, and Reset to Defaults puts them all back.
- **Key list follows your keys.** The `?` list and the clear buttons' hints name the keys as Settings › Keys has them now. A clear that no key does is marked "Button only" there.
- **Add a Key.** Any other key, or a clicker's button, can be added in Settings › Keys and given something to do. Click Add a Key… and press it: it joins the top of the list set to Nothing, and its × takes it out again. Enter, Tab, ↑, ↓, /, ? and Delete are kept for what they always do, and it says so.

### Better

- **Calmer Remote Control.** With a phone's page open and nothing happening, the app now wakes about a fifth as often and uses much less of the computer.

### Fixed

- **Delete a new web page box.** In the Edit view, Delete, Backspace or Undo now takes away a Web Page box straight after you add it, as with any other box; its address can still be typed at once. Before, none of them did anything until you clicked another box and back.
- **Escape out of a field.** In the Edit view, Escape out of a field in the inspector, such as a web page's address or the slide's notes, now only leaves the field, and the box stays picked. Before, it let go of the box too, so a Delete after it took the slide instead.
- **Delete in the Theme Editor.** In the Theme Editor, Delete right after adding a box now takes that box away. Before, with a theme picked in the list, it asked about deleting the theme until you clicked the box.
- **Damaged settings kept.** A settings file damaged by a power cut was saved over, losing its screens, transitions or lists; now it is kept and you are told. SoftPresenter starts with new settings, keeps the old file beside it, and says in the notices where it is. This covers the screens (with their NDI names), the settings (with the transition), the playlists, libraries, media bin, audio playlists, macros, timers, messages, props, stage layouts and the Bible page and its history. A file another program holds open at the start is never saved over until SoftPresenter starts again. Saving also waits until each file, songs and themes included, is whole on the disk before it takes the old one's place, so a power cut leaves the old file or the new one, never an empty one; and it never holds up the window, even while you drag a slider.
- **Changes just before quitting.** A setting or playlist changed from the phone, `sp` or an assistant a moment before quitting, while SoftPresenter's window wasn't being drawn (covered or minimised), could be lost; it is saved now.
- **A failed save is tried again.** When a song or theme couldn't be saved because another program held its file for a moment (a virus scanner, a sync tool), the change was lost at quit; now SoftPresenter keeps trying, says when it is saved, and at quit keeps any change it still couldn't save in the library's Not saved folder.
- **Saved when Windows shuts down.** When Windows signed out, restarted or shut down with SoftPresenter open, the last second's changes and any files still being saved were lost; now SoftPresenter saves and closes first, as it does when you quit it.
- **Window froze opening videos.** Showing a video could freeze SoftPresenter's window for seconds on a busy or slow PC; the window now carries on at once. The screens keep what they showed, the earlier video still playing, until the new video's first picture is ready, for ten seconds at most: a video that hasn't started by then is named in the notices, the screens go on without it, and it still shows if it starts later (the notice goes once it starts, or once something else is shown or the layer is cleared). `sp media show` answers at once.
- **Stuck on the last picture before a new announcements video.** A new video in a followed folder that never started could leave the video before it stopped on its last picture for good; that video now plays round again until the new one starts, and the notices name the new one until it does, or until the layer goes on to something else.
- **Media playlist stuck on a video that won't play.** A media playlist playing on the media layer could stop for good on a video of it that never started or couldn't play; it now goes on to its next item, after ten seconds for one that hasn't started, and the notices name the video until it plays on a later turn.

## 0.9.0-dev.27 — 27 November 2026 (development build)

A What's new window, a search that stays after a drag, AirPlay by name, a tidier Find songs, and quieter VoxLive notices.

### New

- **What's new window.** What's new in Settings › About now opens a window with each version's changes, and needs no internet. On a Mac it is in the Help menu too. Each change is a short name and one plain sentence, in New, Better and Fixed, with the versions before it below to scroll through; before, the key opened the list of changes on GitHub.

### Better

- **Search stays after a drag.** After you drag a song or passage from the find box, its words and results stay, ready for the next one. This is so for a drop into the playlist, onto a placeholder, or onto a playlist or library in the left column. The box gives the keys back at once, so Space, the arrows and a clicker run the slides, and the list dims and says so; click the box or press / to type in it again. Escape, or a click anywhere else, closes the search.
- **Dropped songs wait.** A song or passage dropped from the find box no longer opens or moves the slides; its row flashes orange instead. What you had open stays open, and the slides stay where they were, even when it goes in above them. The row is brought into view if it landed out of sight; click it when you want it.
- **Last item reaches the top.** The last item of a playlist can now be scrolled to the top of the slides, like any other. Clicking it in the playlist takes it there, and a faint line under it marks the end of the playlist.
- **Ticking rows in Find songs.** In Settings › Song files › Find songs on this computer, ticking or unticking a row no longer moves anything on the page. The key always reads Import New Songs, greyed with nothing new ticked, and how many new songs come in is said beside it; before, the key's words and width changed with every tick ("Import 3,518 New Songs"). A database copied while SoftProjector had it open now says so once, under the first of its libraries, instead of under each of them.
- **Reasons under the name.** In Find songs, why a songbook couldn't be read now sits under its name, in two lines at most. The whole reason shows when the pointer is over the row. Before, it was squeezed into a narrow column beside the name, where it could take three lines.
- **Find songs, one line.** Find songs on this computer now starts as one line, with Choose a Folder… and Find beside a sentence saying where it looks. A folder you choose joins that sentence by its name, and with three or more it names the first two and counts the others; nothing on the page moves when you choose one, and only a very long name is cut, in its middle. In a narrow Settings window the keys go under the sentence. Before, it started with a row of where it looks in pills, its keys on a line of their own and a note under them.
- **Called AirPlay.** What was Phone Screens is now called AirPlay in SoftPresenter's window and in `sp`, in every language, though boxes already on slides keep their name.
- **Boxes that fill the slide.** A Web Page, NDI Source or AirPlay box added in the Edit view covers the whole slide, and can be moved and sized as usual.
- **Boxes behind the words.** A new Web Page, NDI Source or AirPlay box goes just under the words, so clicking the words still picks them.
- **Empty boxes are shaded.** In the Edit view, a Web Page, NDI Source or AirPlay box with nothing to show dims what is behind it.
- **Pictures go under the words.** A picture or video added in the Edit view goes just above the slide's background (pictures, videos or a full-slide fill), under the words.
- **A thin white outline.** A picked box in the Edit view is outlined in thin white.

### Fixed

- **Reasons in your language.** When a songbook or song file can't be read, Song files and the import's notice now say why in your language. Details and page numbers are in your language too. Before, they said it in English in the middle of a Spanish, Russian or Ukrainian line ("Не удалось прочитать: the file is cut short or damaged…").
- **Drag files in on Linux.** On Linux, files dragged from the file manager onto SoftPresenter now come in on Wayland desktops too (Ubuntu's default), not only on X11. Before, nothing happened when photos, videos or songs were dragged onto the window there; they had to be added with the + button. They also land where the pointer is, as on a Mac: on the playlist, the library or the media strip you drop them on.
- **Fewer VoxLive notices.** When VoxLive can't be reached, one plain notice now comes after two minutes, or at once while VoxLive captions are on screen. It goes by itself when VoxLive answers again, and a look that can't be saved says why plainly too. Before, every drop in the internet brought a stream of notices in technical words.

## 0.9.0-dev.26 — 26 November 2026 (development build)

A second copy of SoftPresenter no longer opens on the same library, where either could save over the other's changes.

### Fixed

- **One SoftPresenter at a time.** Starting SoftPresenter again while it runs now brings the window already open to the front instead of opening a second copy. Before, starting it again (another click on its icon, say, while an update installs) opened a second copy on the same library, with its own audience window and NDI source, and either could save over the other's changes.

## 0.9.0-dev.25 — 26 November 2026 (development build)

SoftPresenter can now find the songs already on this computer and import SoftProjector Bibles, and many things that went wrong are fixed.

### New

- **Find songs on this computer.** Settings › Song files can find the songs already on this computer. It finds SoftProjector's songbooks and its database, libraries of song documents, and folders of song files, in Documents, Downloads, Desktop, SoftProjector's folder and any folder you add. It lists each with how many songs it has, how many are new and how many are already in your library, and nothing comes in until you tick what you want and press Import. Each comes in as a library of its own name with only its new songs; a song already in your library is never brought in twice. Files kept only in iCloud Drive or OneDrive are never downloaded to look at them: a songbook or a folder of songs there is listed as in the cloud.
- **Import SoftProjector Bibles.** SoftProjector Bible files (.spb) can be imported. Drop one on the window, or choose it with Install from File… in the Bibles window, and it becomes one of your Bibles with its name, abbreviation and description, its language filled in from its words for you to check. Before, SoftPresenter said such a file "isn't a Bible file SoftPresenter can read". One of the older kind, which SoftProjector itself no longer opens, is refused saying so.

### Better

- **Less drawing when nothing changes.** With nothing changing, the operator's window is now drawn half as often, once a second, for the clock. It was drawn twice a second, the second time only to look at the displays again, which is now done without drawing.
- **Editing during a video.** Editing a slide during a video now takes less of the computer. While a video plays behind the words, the Edit view is no longer drawn again for every picture of the video (it has no Program preview to show them in).
- **Opacity as a percentage.** The Opacity slider in the editor now says how solid the object is, beside it (75%), and you can type a percentage there. The shadows' Opacity is written the same way, with a space before the % in Spanish, Russian and Ukrainian.

### Fixed

- **The operator's window size.** After its size changes, the operator's window (and Screens and Settings) no longer needs resizing again to be drawn at its own size. Before, it could stop being drawn at its own size after its size changed while the screens were busy drawing, until it was resized again; the log then filled with graphics errors. It now catches up with its size by itself.
- **Questions with three answers.** On Windows, the questions with three answers now show their own words, not Yes, No and Cancel. Before, they showed Yes, No and Cancel instead of their own words, such as Import a Library Folder's "A Folder…" and "Files…" and a song pack's "Replace", "Keep Both" and "Skip".
- **Songbooks cut short.** A SoftProjector songbook kept as text that was cut short while it was copied or downloaded now comes in with a notice saying so. Such a file ends part way through a song, and it used to come in quietly, its last song's words cut off. It now comes in without that song, and a notice says the file was cut short and the songs after it are missing.
- **While choosing files.** SoftPresenter no longer stands still while a window for choosing files is open. Before, while one was open (Add in the media bin or the Audio tab, Import, Export, Install from File…), SoftPresenter stood still until it was closed: a video dragged from that window onto the media bin came in only once the window was closed, and the phone remote and other commands waited. SoftPresenter now goes on while it is open, and what you drag from it comes in at once. On Windows, clicking Add again while it is open brings it back in front, and when it closes the keys you press go to the operator's window again.
- **Oversized songbooks.** A damaged or deliberately oversized SoftProjector songbook can no longer use up the computer's memory as it is imported. What one file can bring in is now bounded, and one that goes past it is refused, saying so.
- **In front of the words.** In the theme editor's Objects list, a shape, picture or video dragged above the text boxes is now drawn in front of the words. Before, one dragged there went straight back under them. The Objects list is at the bottom left. Dragged there now, or moved with Bring Forward or Bring to Front, it is drawn in front of the words, on every slide that uses the template and on the screens at once. It sits in front of all the text boxes or behind them all, and the line shown while dragging says which. A new one still starts behind the words, and Undo puts it back.
- **A steady NDI stream.** A screen sent over NDI now gives whoever watches it a steady run of pictures, as a camera does. The slide on screen is sent again at the screen's own frame rate (it was sent once a second), and a display that is slow to take its pictures (behind an HDMI splitter, say) no longer holds the stream up with it. Before, a fade could stutter on the stream each time such a display fell behind. That display's window is also given the picture it missed the moment it can take it, where it could keep the end of a fade, or the slide before, until the next click. The log no longer counts the rest on a verse between two fades as an NDI picture that "stood still".
- **Quitting on a Mac.** On a Mac, SoftPresenter now saves and closes its screens first when you quit from the Dock, log out, restart or shut down. Before, quitting from the Dock, logging out, restarting or shutting down ended SoftPresenter on the spot: words changed in the last second were not saved, and a screen being sent over NDI could make it crash as it closed. Now it does as its own Quit does.
- **Song changes while quitting.** A change to a song is no longer lost if another program changes its file while SoftPresenter is quitting. A song you changed whose file another program changed while SoftPresenter was quitting is now kept in the library's "Not saved" folder, as one held back earlier already was; before, that one change could be lost.
- **Phones connecting again.** In Phone Screens, a phone that connected again straight after its connection closed is now let in. Before, it could be turned away without a word.
- **Quitting from a script.** When a script or an AI assistant quits SoftPresenter, SoftPresenter now writes its answer before it closes. Before, `sp menu Quit` (from a script, or an AI assistant) could now and then say it couldn't read SoftPresenter's answer, though SoftPresenter quit: on a busy computer it closed before its answer was written. It now waits for its answers to be written before it closes.

## 0.9.0-dev.24 — 26 November 2026 (development build)

On Windows, videos no longer need FFmpeg in the same folder as SoftPresenter.

### Fixed

- **Videos on Windows.** On Windows, videos now play wherever the program is, as SoftPresenter keeps its own FFmpeg in your user folder. Before, videos played only while FFmpeg (which plays them) was in the same folder as SoftPresenter.exe: run on its own from the Desktop, SoftPresenter said FFmpeg wasn't on this computer. SoftPresenter now keeps its own FFmpeg in your user folder and uses that one, wherever the program is: it copies it there from beside the program the first time, or, where it isn't there either, gets it by itself (23 MB, checked before it is used), saying "Getting FFmpeg for videos…" and then "FFmpeg is ready: videos play again". A video added meanwhile goes in the media bin when it is ready, and one on screen starts playing.

## 0.9.0-dev.23 — 26 November 2026 (development build)

Songs can have the chorus after every verse, as SoftProjector sings them, and dropping a video no longer holds the window still.

### New

- **Chorus after each verse.** A song whose chorus is written once is sung with the chorus after every verse, as SoftProjector sings it. A song made from pasted words, imported from a text file or a SoftProjector songbook, or whose words are changed in Edit Words, comes with the arrangement "Chorus after each verse", already chosen, so clicking its first slide and pressing Next goes Verse 1, Chorus, Verse 2, Chorus… A later chorus ("[Chorus 1.2]") is sung from where it is written on; inserts, intros and endings are shown where they are. The arrangement keeps up as the song's parts change until you change it yourself, and so does the "As sung" of songs brought from SoftProjector before. For other songs you already have, choose Chorus After Each Verse in the song's Arrangement menu.

### Fixed

- **Dropping a video.** Dropping a video on the media bin no longer holds the window still while its picture is read. On Windows it could freeze for seconds. The video's tile comes at once and its picture a moment later. One that can't be read leaves the bin with a notice saying why.
- **SoftProjector's part names.** Words pasted or imported the way SoftProjector writes them now come in with every part named and coloured, as a SoftProjector songbook does. These include Куплет 1, Припев, Приспів 1.2, Вставка, Слайд, Окончание and "&Куплет" for more of a verse. Before, some of them ("Вставка", "Слайд", "Припев 1.2", "&Куплет") were taken for words of the part before.
- **A second chorus.** In a SoftProjector song with a second chorus after a verse, the second now takes the first one's place, as SoftProjector sings it. Before, it played the first chorus and then the second one after that verse; the second now takes the first one's place there and from then on.

## 0.9.0-dev.22 — 25 November 2026 (development build, Windows only)

SoftPresenter looks for FFmpeg in more places on Windows, and Settings › Integrations is laid out like the rest of Settings.

### Better

- **Integrations pages.** The pages of Settings › Integrations are laid out in the same cards and rows as the rest of Settings. They are VoxLive, Canva Sync and TimelyChurch, each with what the service shows first (VoxLive's screens, Canva Sync's sync, TimelyChurch's upcoming services), then the presentation or the connection, then the account. What the hints said only under the pointer is written on the page, and Canva Sync's pages are a strip of numbered pictures with the divider ringed. Nothing they do has changed.

### Fixed

- **FFmpeg on Windows.** On Windows, SoftPresenter now also looks for FFmpeg in your own SoftPresenter folder and on the search path. Before, a video said FFmpeg wasn't on the computer when SoftPresenter.exe was run on its own (from the Desktop, say), away from the FFmpeg that comes beside it. It looks in %LOCALAPPDATA%\SoftPresenter\ffmpeg, and its log says where it looked.
- **The notices button.** The number on the notices button sits in the middle of its circle. On Windows it sat high.

## 0.9.0-dev.21 — 25 November 2026 (development build)

A songbook's songs stay in number order in the song list, and long names end in "…" instead of being cut off.

### Fixed

- **Songbook song order.** A songbook's songs now stay in number order in the song list, even after SoftPresenter is restarted. Before, a songbook's library listed its songs 1, 10, 100, 1000… in the song list once SoftPresenter had been restarted, though it listed them 1, 2, 3… right after it was installed. As listed now always follows the library's own order, in every library, so a songbook's songs stay in number order.
- **Long names.** A long name at the top of the song list or under the preview now ends in "…" instead of being cut off. Before, a long library or playlist name at the top of the song list, and a long song name in the line under the preview, were cut off in the middle of a letter. They now end in "…", and resting the pointer on them shows the whole name.

## 0.9.0-dev.20 — 25 November 2026 (development build, Mac only)

Settings › Screens and Phone remote have a new layout, and a screen switched off now keeps where it goes.

### Better

- **Switching a screen off.** A screen switched off now keeps where it goes, so switching it on brings it back exactly as it was. Its On key in Settings › Screens, Off under Output in the Screens window, the X on a screen that fills a display and closing a screen's window all stop it going out and keep its display, its NDI name or its DeckLink format and keyer. Before, Off forgot them and they had to be chosen again. A screen on NDI or a DeckLink output that is switched off goes on sending black, so the stream or switcher keeps it as a source.
- **Phone remote settings.** Settings › Phone remote is laid out in the same cards and rows as the rest of Settings. It has Remote Control (Turn on Remote Control; while it is on, the address to open with Copy; and Port and certificate, folded away under it), Password (Set or Not set yet, with Change Password…, which opens the two fields with Cancel), Connected now and Devices remembered. What the hints said only under the pointer is now written on the page. Nothing it does has changed. Its row in the Settings list no longer cuts its name short in Russian and Ukrainian while people are connected.
- **A card for each screen.** Settings › Screens now has a card for each screen, with an On key, what it is doing and where it goes. It now looks as the design draws it: a line saying how many screens are going out and how many need attention, Identify and the test patterns, All audience screens, All stage screens and Details (a technical line on every card: where it goes in full, see-through or key, theme, layers, edges; remembered), then a card for each screen, audience screens first, with its colour, its name, an On key, what it is doing and where it goes. A screen whose display isn't plugged in has Use a window for now. A click on a card opens that screen in the Screens window, which is where its output, look, background and key are still set, and Add a screen adds one that goes nowhere until it is told where. The card's On key does what Off under Output does: it sends the screen nowhere, and On asks where it goes.

### Fixed

- **The clock.** The clock in the bar along the top sits in the middle of its box. With a one-figure hour (8:51:12) it was pushed to the right, with a gap before it.

## 0.9.0-dev.19 — 25 November 2026 (development build, Mac only)

- The phone remote's preview no longer flickers "What the audience sees" while media plays or a slide changes. Bible, Edit and Settings on the phone, which aren't there yet, are dimmed but answer a tap and say why they wait; before, they looked broken.
- On an Intel Mac, a video behind the words now plays at its own frame rate: a 2015 iMac showed about 10 pictures a second before, and now shows a 30 fps video whole and a 60 fps one at about 53. Slides there also come up sooner: an ordinary slide is drawn in about 12 ms, where it took 53.
- On a slower computer (an Intel Mac, say), keys pressed quickly one after another now act in turn. Return in the find box and a quick Enter after it left the song open with its first slide picked, not on screen, as if the second press had been lost; now the slide goes on screen, as on a faster computer. In the same way ↓ and a quick Enter in the playlist started the song that was open before instead of the next one, and a letter typed straight after `/` could put a slide of the song on screen (C, the chorus) instead of going into the find box.
- On a big screen, SoftPresenter's window now grows in proportion: full screen on a 27-inch iMac, the library, the preview, the layer keys and the tabs are drawn 25% larger, instead of being thin columns round a huge middle. Automatic goes no larger than 125%. Words stay sharp, and so do the slides' small pictures and the preview. At the usual window size nothing changes. Settings › Slides › In your window › Interface size chooses Automatic (the usual), or always 100%, 125%, 150% or 175%.

## 0.9.0-dev.18 — 25 November 2026 (development build)

- SoftPresenter now runs on Intel Macs as well as on Apple silicon, and on macOS 11 (Big Sur) and later, where it needed macOS 13 before. There is one Mac app for both kinds of Mac.
- A dimmed line under the preview says what Next will put on screen: "Next: Verse 2", "Next: John 3:18", "Next: Amazing Grace, slide 1" when Next would start the song opened or go on into the next one, or "Next: nothing — end of the playlist". It follows the same rules as the key, so it never says other than what Next does; `sp status` says the same. The preview block is a line taller for it.
- With a Bible reading (or a song from a library) still on screen at its last slide, opening a song in the playlist and pressing Next now starts that song at its first slide. Before, Next stayed on the last verse until the reading was cleared. Next always moves what the audience sees; opening a song only lines it up.
- On a Windows PC with no working graphics driver (Device Manager shows "Microsoft Basic Display Adapter"), and in some virtual machines, SoftPresenter closed by itself a moment after starting. It now starts there and shows its slides, working the pictures out on the processor: more slowly than with a graphics card (a full-screen slide can take a few seconds to appear), and a 4K slide packed with small text may be refused as too much to draw. Installing the graphics card's own driver is still the way to make it fast.
- Linux has a package: `softpresenter_<version>_amd64.deb`, for Ubuntu 22.04 and later, Linux Mint 21 and later and Debian 12 and later. `sudo apt install ./softpresenter_….deb` puts SoftPresenter in the applications menu with its icon and `sp` in the terminal, and brings its own FFmpeg, so videos play the same whatever the computer has; `sudo apt remove softpresenter` takes it away again and leaves your songs and settings. Settings › Command line then says sp came with the package.

## 0.9.0-dev.17 — 25 November 2026 (development build)

- A pre-release says so: the setup's first step, and Settings › About under Updates, say "This is a pre-release" with its version, that things may still change while the core of how SoftPresenter works and looks stays, that bugs will be fixed in the versions that come next, and that Check for updates tells you when one is out.
- After a Bible verse shown from the Bible page has been cleared, opening a song in the playlist and pressing Next now starts that song at its first slide. Before, the next verse of the reading went back on screen instead.

## 0.9.0-dev.16 — 25 November 2026 (development build)

- SoftPresenter now sends small anonymous reports of how it runs and what goes wrong (the kind of computer, speeds, crashes), so problems get found and fixed. Never your songs, Bible text, file names or anything about you. They are on unless you turn them off: the setup has a Reports step with the key, and if you already use SoftPresenter a notice says so once, with Turn off beside it; nothing is sent before you have seen one of the two. On a computer whose region is in the European Union, Iceland, Liechtenstein, Norway or the United Kingdom they are off unless you turn them on, and the setup invites you to. Settings › About › Anonymous reports has the On/Off key, See what's sent (the next report, exactly as it would go) and Reset my anonymous id. PRIVACY.md says what is sent, where it goes and that it is kept for 90 days.

## 0.9.0-dev.15 — 25 November 2026 (development build)

- A big import no longer freezes the app while its songs are saved. Installing a free songbook or importing hundreds of songs on a slow computer (on Windows, where each new file is checked for viruses) left the window and `sp` unanswered for minutes until every song was written; now the songs are written in the background while you carry on, and quitting waits, saying so, until the last one is saved.
- A fade to the next slide or verse asks less of the graphics card: each screen now draws only the new picture, once, where it used to draw the old picture again as the fade began and the new one again as it ended. On large screens, such as 4K with a photo behind the words, the screens are held up less at each change.
- The first time you open the Bible view after starting the app, the window no longer stands still for a moment while the Bibles are read: the translations you use are read in the background as the app starts.
- Looking up a passage in the Bible view, or opening a long song, no longer makes the window stand still for a moment on a slower Mac or PC while the slides' small pictures are drawn: they fill in over the next moments instead, and the window answers at once.
- On Windows and Linux, a placeholder's name in the playlist no longer jumps a little when you start renaming it; the words stay where they were, as on a Mac.
- You can check for a newer version yourself: SoftPresenter › Check for Updates… on a Mac, or Settings › About › Check for updates on any computer. It says "You have the newest version (0.9.0)", or which version is available with a key that opens its download page, and when it last checked. It asks only when you click, never in the background. A development build says the same of development versions, with its full number (0.9.0-dev.15), which About now shows too.
- The Bibles window lists Bibles in the languages you use first, under Your languages, then the rest under Other languages: SoftPresenter's language, your computer's preferred languages and the languages of Bibles you installed. With the app in Russian, the Russian Bible to download is at the top instead of near the end of the list; `sp bibles` lists them the same way.
- The phone Remote Control page speaks Russian and Ukrainian too, following the app's language.
- Song parts named in Russian, Ukrainian or Spanish (Куплет 2, Припев, Приспів, Estribillo…) get the same colours and jump keys as English ones: C goes to «Припев», S to «Куплет 2».
- The app can be used in Russian: choose Русский in Settings › Language or in the setup, and every button, menu, notice and window changes at once, in plain church words (Служение, список, куплет, припев, экраны для зала).
- The app can be used in Ukrainian: choose Українська in Settings › Language or in the setup, or leave it on This computer's on a computer set to Ukrainian, and every window, menu and notice changes at once. Songs and Bibles keep their own languages.
- Settings › AI assistants: a tile's keys are as wide as their words and sit side by side where the tile has room, Get Claude Code ↗ is a key that opens its page, and a tile whose apps aren't on the computer says what it is for ("For the desktop app or Codex").
- Settings › Groups says how many slides of your library use each group ("Not used" for none). In a narrow Settings window, the test patterns on Settings › Screens go on a line of their own under Identify instead of being cut off.
- Settings › Keys shows every row's keys at any size of the window, in every language. Before, a long line pushed the keys off the page and was cut in the middle of a word; now what a key does wraps in its own column.
- The phone Remote Control page has the audio player: the track, Play/Pause, the position, Next and Previous, the volume, Play through, Shuffle, Loop and the crossfade. `sp audio mute on|off` mutes and brings back the level, as the Audio tab's Mute does.
- Settings is regrouped in four groups, The room, On screen, Connect and Advanced, and each row says how its page stands ("3 of 4 on", "1 connected", Off), so a problem shows before anything is opened. General is gone: the copyright line is under Slides, the language under Language and Reload songs under Song files; Command line has a page of its own under Advanced, and Remote Control is now Phone remote.
- Settings › Command line, Song files, About and Groups are cards in the same look as the rest of Settings, with a row and a key for each thing they do. Settings › Screens says how many screens are going out, and Identify and the test patterns sit in one strip above the screens' card.
- Settings › AI assistants is simpler: choose how much an assistant may do (Off, Prepare, or Prepare and run the screens), then Add it or Copy setup on its tile. Copy setup copies a message to paste into Claude, ChatGPT, Gemini or Grok that says how to connect and how to use SoftPresenter.
- Settings › Integrations shows every service as the same card, with its logo, what it brings, how it stands and one key; each opens its page the same way, with the way back at the top. TimelyChurch shows as Coming soon until it works.
- Settings › Language chooses the app's language with one click: This computer's, or English, Español, Русский or Українська, each with its flag.
- The language flags have clean rounded corners and crisp stripes.
- Settings › Slides shows what the room sees with every song: the copyright line drawn on a small slide, where it falls as four keys (First slide, Last slide, Every slide, Nowhere) with your CCLI licence beside them, the transition for all slides as keys with a picture of each (More… for every other kind), Carry on to the next item, and how the slides are laid out in your window.
- The Edit view no longer shows the preview and the clear buttons on the right: the settings there (Shape, Text, Build, and a theme's or a stage layout's own) take the whole column, with far more room. While editing, the keys still clear the screens (Escape, and off a Mac F1 to F6); the buttons are in the Service and Bible views as before.
- The clearing keys work in the Edit view too: off a Mac F1 to F6, as in the Service view, and Escape once nothing is picked, being typed or open there (a first Escape still lets go of the picked box, puts back words being typed or closes a menu; the next press clears the screens, and holding Escape down never does). Space, the arrows and the letters still never change the screens from the Edit view.
- Free songbooks shared by churches can be installed from Import Songs › Free Songbooks… or the setup's Free Songs step: Песнь Возрождения 3300 and others in Russian, Ukrainian, Czech, Slovak, German and English. Each is downloaded from the page that shares it only when you choose it, and its songs come into a library of their own, each with its number. Remove takes out again the songs you haven't changed.
- Songs brought across from another program's library folder are named as their files are, hymn numbers and all (“12 - Amazing Grace”). They used to take the name kept inside each file instead: often the first line with a comma, a garbled name, or another song's. `sp library tidy --titles-from FOLDER` renames songs brought across before, FOLDER being the folder they came from.
- Imported songs come in tidier. Garbled titles such as “╨Ñ╨▓╨░╨╗╨░” come out as the words they should be. A title loses a “.docx” ending and a comma left at its end. “Untitled” becomes the song's first line, and a title that is only a number gets the first line after it (“619 - …”). Stanzas separated by a blank line or a “* * *” line inside one slide become separate slides, and a slide of more than eight lines is shared out. Odd spaces and letters typed in two parts are put right too. `sp library tidy` does the same to songs already in the library: first it lists every change, then `--apply` makes them and keeps a copy of each song as it was.
- Apple Lossless (ALAC) music, as Music saves it, now plays, and so do other formats through the app's FFmpeg; a track that can't play says so.
- Audio plays more smoothly: each track is read a few seconds ahead of where it plays, away from the sound card, so a busy computer doesn't make it stutter; a track that follows another starts on the very next sound with no crossfade set, and a crossfade, a pause, a jump in the track and the volume all move smoothly, with no clicks. A track that came at a different rate from the sound card's is brought to it with better quality, and a surround track is folded into stereo with its centre kept.
- Walk-in music can loop: the Audio tab has Loop beside Play through and Shuffle. On, the playlist starts again from the first track after the last (with Shuffle, in a fresh order, never the same track twice in a row), crossfading as between any two tracks; with Play through off, Loop plays the same track again. A playlist set to Play through before keeps going round as it did; one with Play through on and Loop off now stops after its last track. A track that can't be played is passed over, and if none can, the audio stops and says so once. `sp audio loop on|off` and `sp audio seek 1:23` (or `83`, `+15`, `-15`) work from the command line and the phone's remote, and `sp audio` says what plays, where it is and how tracks follow on.
- The six clear buttons on the right are in the order the screens stack the layers, from the back: Media, Slide, Announce, then Props, Messages and Audio. Their keys are the same.
- The songbooks SoftProjector offers for download (Песнь Возрождения, Євангельські Пісні, Christian Hymns and the others) import now, on every system. They are an older kind of songbook file, which was refused before.
- A zip that holds SoftProjector songbooks imports as it is, by dropping it on the window or with File › Import Songs › Song Files…; no need to unpack it first.
- SoftProjector's song database (`spData.sqlite`) and the songbooks it exports (`.sps`) now import on Windows and Linux too. Before, they imported only on a Mac.
- Connecting an AI assistant is one click: Settings › AI assistants has a tile for Claude, ChatGPT, Gemini and Grok (and Cursor and VS Code under Other apps) with Add, which adds SoftPresenter to that company's apps on the computer, and Remove. Only SoftPresenter's entry in their settings changes, a copy of the settings as they were is kept, and a settings file that isn't valid is left alone. Copy setup copies a message to paste into the assistant: how to connect it, and how to use SoftPresenter.
- Sixty-one more free Bibles can be downloaded from the Bibles window, in 29 languages in all: among them the Chinese Union Version, the Korean 개역한글, the Statenvertaling, Károli, Biblia Gdańska, the Kralická, Diodati and Riveduta, the Swedish Bible of 1917, Daničić-Karadžić in Latin and Cyrillic letters, the Arabic Van Dyck, the Persian Old Version, the Russian Open Bible, and many more in English. Every one is public domain or openly licensed.
- The phone and tablet Remote Control page speaks the app's language: Spanish when the app is in Spanish, from the sign-in on, with the same words as the app.
- Unofficial: an iPhone, iPad or Mac can show its screen in a box on a slide, through its own Screen Mirroring. Switch it on in Settings › Phone Screens; it is off until then, a device shows only where you put a Phone or iPad Screen box, and you're asked before it does. It isn't made or supported by Apple, and an Apple update may stop it working. On a Mac only for now, and without sound yet.
- Claude and other AI assistants can work with SoftPresenter: find songs and Bible passages, make a song from lyrics in one or two languages, and prepare a service's playlist. They can change what is on the screens only if you choose Prepare and run the screens in Settings › AI assistants, and they ask you each time first. Nothing works until you choose Prepare.

## 0.9.0-dev.14 — 24 November 2026 (development build)

- The screens no longer wait for the computer's disk to write the log, which once held them still for half a second.
- A video behind the words no longer freezes on NDI and DeckLink for each slide change: those outputs now show the dissolve the TVs show, and the video keeps moving through it.
- Each NDI screen has a frame rate in the Screens window: 29.97 unless you choose 30, 59.94, 60, 25 or 50. Sending exactly 30 to 29.97 equipment, as before, made a small pause every half-minute or so.
- The stage screen's clock no longer makes the other NDI screens redraw once a second.
- Keying a DeckLink screen is one choice in the Screens window now: Keyer Off, Fill and key, or Over the card's input. Keying makes the screen see-through with premultiplied soft edges, the usual for a switcher's keyer, so only the words are keyed; a keyed screen left solid before is put right when the app opens. When the screen's format isn't the one Desktop Video Setup has, the window says so in amber, with a button to use it.

## 0.9.0-dev.13 — 24 November 2026 (development build)

The first numbered build, with the same number on Mac and Windows. This is everything SoftPresenter could do on 24 November 2026.

### Songs and lyrics

- Songs are slides in groups (Verse, Chorus, Bridge), each with its colour and a letter key (A for Verse 1, C for the chorus).
- Two languages on every slide: the main words, with the translation under them.
- New Song (Command-N) takes pasted words: `[Verse 1]` starts a group, a blank line a new slide, a `--` line the translation.
- Arrangements set the order the groups are sung in; a chorus sung twice shows as ×2, and the slides stand in that order.
- Songs come in from text, rich text, ChordPro, lyric and (on a Mac) Word files, and whole libraries from other presentation apps.
- A .pptx presentation comes in as slides you can edit, or as pictures; PDFs and folders of pictures become picture slides.
- After an import, slides that may need a look (a chart that came in as a picture) wear a Check tag until marked as checked.
- Song packs carry songs, or a whole playlist with its pictures and videos, to another computer in one file.

### Bible

- Type a reference in English, Russian or Ukrainian; nothing goes on screen until you click a verse or press Enter.
- Up to four translations on one slide, lined up verse by verse even where they number verses differently.
- Four Bibles come with the app, and sixteen more install from the Bibles window.
- A Bible file the church already has installs when dropped on the window (Zefania, OSIS, USFM, USX and other formats).
- The page shows the whole chapter, a keypad of all 66 books and History of passages shown; Аа sets its reading language.
- + Add puts the passage in the playlist, and the find box (/ or Command-F) finds passages and verses as well as songs.

### Media and video

- Pictures and videos go in the media bin, stay between runs, and show behind the words or on their own.
- Videos play with their sound and can loop; MKV, WebM, AVI and other formats play with nothing to install on a Mac or Windows.
- Under the preview, a playing video has Play or Pause, Start Over, Forward 10 seconds, a bar to drag through, Loop and Sound.
- The Announcements layer loops pictures, videos or a song's slides before the service, and comes off as the first slide goes up.
- A media playlist can follow a folder: a fresh video put there takes over at the end of the loop, with no black between.
- The Audio tab plays walk-in music from playlists, with shuffle, a crossfade and its own volume.
- Seven sample backgrounds come with the app: Add the Sample Backgrounds, in the menu beside MEDIA.

### Screens, NDI and DeckLink

- Any number of audience screens, each with its own look: on a display, in a window, over NDI or out of a DeckLink card.
- Your own display is never filled; with only one, the audience screen opens in a window until a projector is plugged in.
- A screen takes over its display at once; when its projector goes off it hides, and it comes back when the display returns.
- Moving the pointer over a screen that fills a display shows an X that turns it off, so a wrong display never leaves you stuck.
- Identify Screens shows each screen's name on it; test patterns (colour bars, grid, white) are in Settings › Screens.
- NDI sends only while someone watches, with the sound of what plays; a screen can be see-through, or another's key, for a stream.
- A camera or another computer sending NDI on the network can be shown in a box on a slide.
- DeckLink cards play out over SDI or HDMI, with fill and key where the card can key (Desktop Video 16 or later).

### Stage screen

- Stage screens show the words now, the next slide, the clock, a running timer and a stage message typed in the Stage tab.
- Ready-made layouts: Words and Next, Words Only, Words and Notes, Clock and Timer; one click in the Stage tab switches.
- Edit › Stage designs your own layouts with the slide editor: words, shapes, pictures, videos, web pages and the slide's notes.
- A layout can show a live picture of an audience screen, so the band sees exactly what the room sees.
- With several stage screens, each can show a layout of its own.

### Themes and the editor

- The slide editor places text, shapes, pictures, videos and web pages (on a Mac only), with guides, rulers and undo.
- Words can be styled inside one box: a bold reference, an italic line, one word in a colour.
- SoftPresenter Sans, with Latin, Cyrillic and Greek letters, is built in, so slides look the same on every computer.
- A theme has templates for two-language songs, one-language songs and Bible slides; each audience screen can have its own theme.
- Theme versions: a named look for a moment in the service ("Offering") that one switch puts on every screen whose theme has it.
- Nineteen transitions, for all slides, one song or one slide; builds, scrolling text, and text that shows the clock or a timer.
- Props: a logo or a banner that stays over the slides until taken off.

### Playlists and services

- A playlist is the running order: songs, Bible passages, pictures, videos, media playlists, headers and placeholders.
- The whole playlist shows as one scroll of slides; a click on an item opens it without changing the screens.
- Space and the right arrow go forward, letters jump to a group, Escape (or F1 off a Mac) clears everything; ? lists every key.
- The Show view is now the Service view (Command-1); Bible is Command-2 and Edit Command-3. A Mac's menu bar is six short menus.
- Timers, messages, props, macros, audio and the stage each have a tab of cards under Clear All (Option-1 to Option-7).
- Macros do several things in turn; one can run as the app opens, playing the announcement loop with the audience screens off.
- Undo brings back what was deleted; the app asks first only when Undo can't bring something back.
- The notices button in the top bar keeps what needs your attention; nothing pops up over the show.
- Nothing needs saving: every change is written as you make it, on a Mac into Documents › SoftPresenter, easy to back up.

### Remote control and the command line

- Remote Control runs the show from a computer, tablet or phone on the church's network (Settings › Remote Control).
- It is off until switched on, uses HTTPS and a password of ten characters or more, and answers only the church's own network.
- The page shows what is on screen with a live picture, the playlist, the slides and the tabs; Settings can forget a device.
- `sp` runs the app from a terminal or a script (`sp status`, `sp next`), and installs itself as the app first starts.

### Languages

- The app can be used in Spanish: choose it in Settings › General › Language or in the setup, and it changes at once.
- A few messages that come straight from the video, NDI and DeckLink drivers are still in English.
- Russian and Ukrainian can be chosen too, but the window's words aren't translated into them yet.
- Names the app gives (the first playlist, a new box) follow the language; names the church gave never change.
- The Bible takes references in English, Russian and Ukrainian; imported songs read Cyrillic, in older Windows encodings too.

### Windows

- Windows 10 and 11 (64-bit) builds are in testing; nothing is released yet.
- The package is a zip with the program and its own FFmpeg; it isn't signed yet, so Windows asks the first time ("Run anyway").
- Videos play through that FFmpeg, decoded on the graphics chip where it can be, without a console window opening.
- Screens are listed by the monitors' own names and cover the taskbar on their monitor.
- PDFs come in, and so do presentations as pictures when an app that opens them is installed.
- NDI is found through NDI Tools; the firewall asks once, when NDI or Remote Control is first used.
- The `sp` command installs itself; VoxLive's key and Canva Sync's sign-in are kept in Credential Manager.
- Not yet on Windows: DeckLink outputs, web pages on slides, and Word or OpenDocument files.

### Linux

- Linux builds are in testing; nothing is released yet.
- Displays are listed by the monitors' own names, each once.
- Download NDI… in the Screens window installs NDI for SoftPresenter once you accept NDI's licence, with no admin password.
- VoxLive's key and Canva Sync's sign-in are kept in the system keyring (GNOME Keyring or KWallet).
- Videos need FFmpeg installed on the computer (for example `sudo apt install ffmpeg`).
- Not yet on Linux: PDFs, presentations as pictures, Word or OpenDocument files, and web pages on slides.

### Integrations

- VoxLive: connect with an API key in Settings › Integrations; the church's caption screens become slides and a VoxLive tab.
- A VoxLive screen's look can be changed from Settings, with a preview that starts no translation.
- Canva Sync makes the announcements video from the church's Canva presentation, from the pages above a divider page.
- Sync Now, a macro step or a start-up macro fetches it, and the new video takes over at the end of the loop playing.
- Free songbooks in many languages (development builds only, for now) download in the setup or Settings › Integrations.
- The find box searches them; a song joins the library once put in a playlist, and a church's edits are never overwritten.
- Keys and sign-ins are kept in the computer's secure store (Keychain, Credential Manager or keyring), never in a plain file.

### Fixes worth knowing

- Early builds used more and more memory while they were open; they no longer do.
- Changing or removing a font while SoftPresenter was open could crash it in the middle of a song; it no longer does.
- Clicking through slides faster than the fades could leave a slide stuck or slow the screens; both are fixed.
- When a projector was switched off, its screen could land on the operator's display; now it hides until the projector is back.
- Playlists lost their songs if the library folder moved; songs are now found wherever the folder goes.
- Some looping videos froze on their last picture instead of going round; they loop now.
- Escape in the Bibles window, the playlist chooser or Edit Words cleared the screens; now it just closes them.
- Linux crashed every time it quit; it now quits cleanly.

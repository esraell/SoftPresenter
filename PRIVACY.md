# Privacy

SoftPresenter is free and open source, and it has no accounts. Your songs, Bibles, pictures,
videos and settings stay on your computer. This page says what leaves it.

## Anonymous reports

SoftPresenter sends the SoftPresenter project small anonymous reports of how it runs and what
goes wrong, so problems on computers we never see get found and fixed.

**Whether they start on or off depends on where your computer says it is.**

- **In the European Union, Iceland, Liechtenstein, Norway and the United Kingdom they are off
  unless you turn them on.** The setup's Reports step shows the switch Off and invites you to
  turn it on; an install updated from a version without them stays off and is told nothing,
  since nothing is on. Nothing is counted, kept or sent unless you switch them on.
- **Everywhere else they are on unless you turn them off.** The setup shows them on its
  Reports step, with the key that turns them off. An install updated from a version without
  them gets a notice once, "Anonymous reports are on…", with **Turn off** beside it. Nothing
  is counted, kept or sent until you have passed that step or been shown that notice, and
  nothing goes in the first minute after it: turn them off there and nothing ever left your
  computer.

"Where your computer says it is" is its *region* setting, not its language: a Mac's region
(System Settings › General › Language & Region), Windows' Country or region, and on Linux the
territory of its locale (`de_DE`). The region is read on your computer only to choose this
starting point; it is never sent and not kept. **If the computer doesn't say where it is,
reports start on**, with the step or the notice as above. Once you have chosen On or Off, your
choice is kept and always wins, wherever the computer is taken.

### Turning them off, and seeing what is sent

- **Settings › About › Anonymous reports**: the On/Off key. Off stops at once, and deletes any
  report that was waiting to be sent.
- **See what's sent**, in the same place and on the setup's step, shows the next report exactly
  as it would go, all of it. `sp reports show` prints the same in a terminal; `sp reports off`
  and `sp reports on` switch them.
- **Reset my anonymous id** gives the install a new random number.

### What is sent

- The app's version, and the system (macOS, Windows or Linux, and its version).
- The kind of computer: the processor's name and number of cores, the memory in gigabytes, the
  graphics card and its driver, and each display's size, scale and refresh rate.
- How many screens are set up and on, and whether they go to a display, a window, NDI or a
  DeckLink card; whether NDI, DeckLink and FFmpeg are there; the app's language.
- For each minute it ran: how much processor and memory the app and the video player used,
  how long the window and the screens took to draw, how many keys and clicks (counts only),
  and what *kinds* of thing were on screen, as codes the program made up: `song`, `bible`,
  `video_1080p30`, `prop`.
- Things that happened, as codes with a number: a slow frame and how long it took, a freeze, a
  display plugged in, that a notice was shown (never its words).
- How many times each view and menu command was used: `view_bible: 7`.
- Lines from the log that report a problem with the graphics, the screens or video playing,
  and only those that can't hold a name: a line with a quotation, a file name, a folder or
  anything not in plain English letters is left out whole.
- A crash: where in the program it happened, and the program's own steps that led there.
- A random number made up for this install, so reports from the same install can be read
  together. It is not made from anything about you or your computer, it says nothing about who
  you are, and you can reset it.

### What is never sent

The words or titles of your songs. Bible text. The names of your files, folders, displays,
screens, NDI sources or devices. The names of people or of your church. Your computer's name or
model, or your user name. IP or MAC addresses. Remote Control devices, passwords and keys. The
log itself. Anything you typed.

The report has no place for these: it is built from a fixed list of fields
([docs/anonymous-reports.md](docs/anonymous-reports.md) lists every one), the app checks each
report against that list before it is kept, and the server refuses any report with a field it
doesn't know or with text that looks like a folder, an address, a link or a key.

### Where it goes, and how long it is kept

Reports go over HTTPS to a server the SoftPresenter project runs on Cloudflare
(`reports.softpresenter.com`). The server keeps the report and the time it
arrived for **90 days**, then deletes it. It does not keep your IP address or your country, and
its request logs are switched off. Only the project's owner can read the reports. They are not
sold, shared or used for advertising.

To have an install's reports deleted sooner, open an
[issue](https://github.com/esraell/SoftPresenter/issues/new/choose) with its anonymous id
(Settings › About › See what's sent shows it as `install_id`; so does `sp reports`).

### When

One report every fifteen minutes while the app runs, one as it quits, and one after a crash.
With no network they wait on disk (a quarter of a megabyte at the most) and go later. Sending
never holds up the screens or the window.

## What else uses the network

Other features use the network when you switch them on or ask for them, such as downloading a
Bible or a free songbook, NDI, Remote Control and phone screens on your own network, VoxLive
captions and Canva Sync. Each talks to the service it names, and none of them sends the reports
above. One thing happens without asking: on Windows, when the app's own copy of FFmpeg (which
plays videos) isn't in your user folder and can't be copied there from beside the program, the
app downloads the same FFmpeg from SoftPresenter's own release on GitHub (23 MB, checked before
it is used); it sends nothing but the request for that file. [docs/features.md](docs/features.md)
says what each one does.

## Changes to this page

This page changes in the open, with the code: its history is in the repository.

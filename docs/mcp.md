# AI assistants (MCP)

SoftPresenter can work with an AI assistant, such as Claude in Claude Code or Claude Desktop,
through the Model Context Protocol (MCP): the assistant can look at what is on the screens, find
songs and Bible passages, make songs and playlists for a service, and, only when the person at
the computer has allowed it and says yes each time, run the screens.

- [Connecting](#connecting): Claude Code, Claude Desktop, other assistants
- [What the assistant can do](#what-the-assistant-can-do): the tools, what each needs
- [What is refused, and why](#what-is-refused-and-why)
- [Skills, questions and long jobs](#skills-questions-and-long-jobs)
- [How it works](#how-it-works) and [its security](#security)

Nothing here works until it is switched on in **Settings › Integrations › AI assistants (MCP)**:

- **Let AI assistants use SoftPresenter** (off unless switched on): the assistant may read the
  library, the Bibles and the playlists, and make songs and playlists. Nothing on the screens
  changes.
- **Let them change what's on the screens** (off unless switched on): the assistant may put a
  slide, a passage or a message on the screens, go to the next or previous slide, and clear the
  screens; each time it asks the person first. With this off, it can't, even when the person
  says yes to its question.

## Connecting

The server is the app's own program: `sp mcp` (or `softpresenter mcp`). The assistant starts it
and talks to it over its standard input and output; it passes each request to the running
SoftPresenter through the same socket `sp` uses. It never starts SoftPresenter: with the app
closed, every tool answers "SoftPresenter isn't open."

The card in Settings shows both of these, with this computer's path to `sp` filled in, and a
Copy button beside each.

### Claude Code

    claude mcp add softpresenter -- ~/.local/bin/sp mcp

(`--scope user` makes it available in every folder, not only the one it was added in.) In Claude
Code, `/mcp` then lists SoftPresenter with its tools, and its two skills appear as
`softpresenter:preparing-a-service` and `softpresenter:running-the-screens`.

### Claude Desktop

Add SoftPresenter to Claude Desktop's configuration file (on a Mac,
`~/Library/Application Support/Claude/claude_desktop_config.json`; on Windows,
`%APPDATA%\Claude\claude_desktop_config.json`), then quit and open Claude Desktop again:

```json
{
  "mcpServers": {
    "softpresenter": {
      "command": "/Users/you/.local/bin/sp",
      "args": ["mcp"]
    }
  }
}
```

The command must be the full path (Claude Desktop doesn't read your shell's PATH). On Windows it
is the program itself, `SoftPresenter.exe`, wherever it is installed, with `"args": ["mcp"]`.

### Other assistants

Any MCP client that starts a server over stdio works: the command is `sp mcp`. The server speaks
the protocol's revision 2026-07-28 (each request carrying its own version and capabilities) and,
for clients that still open with a handshake, the earlier revisions 2025-11-25, 2025-06-18,
2025-03-26 and 2024-11-05.

### Trying it on a test copy

`sp mcp` reaches whichever SoftPresenter its environment points at, as `sp` does. To try it
without touching the copy someone is using, start a quiet test copy and give the server the same
variables ([cli.md](cli.md#trying-it-safely)):

    claude mcp add softpresenter-test --env SOFTPRESENTER_DATA=/path/to/test-library \
      --env SOFTPRESENTER_CONTROL_SOCKET=/tmp/sp-test/ctl.sock -- ~/.local/bin/sp mcp

## What the assistant can do

Each tool runs `sp` commands in the running app, through the same code as the window's buttons.
"Asks" means the assistant shows the person a question first ("Put “Amazing Grace” on the
screens now?") and goes ahead only on Yes.

| Tool | Does | Needs |
|---|---|---|
| `get_status` | What is on the screens (song, slide, words, media, message), the next slide, each screen and what it is doing, and the playlist in front | the first switch |
| `search_songs` | Songs whose title or words hold the words given, with each song's id; with no words, the library's songs (the first 50, or as many as asked, up to 200) | the first switch |
| `read_song` | A song's words as song text (groups, slides, translation under `--`), with its details | the first switch |
| `look_up_bible` | A passage's verses in the translations the Bible page uses, leaving the page as it is | the first switch |
| `search_bible` | Verses holding the words given, leaving the Bible page as it is | the first switch |
| `list_bibles` | The Bibles installed, and those that can be downloaded | the first switch |
| `list_playlists` | The playlists, and which is in front | the first switch |
| `read_playlist` | A playlist's items in order (each with its kind, title and id), the one in front if none is named | the first switch |
| `list_media` | The pictures and videos in the media bin | the first switch |
| `list_messages` | The messages kept for showing over the slides | the first switch |
| `create_song` | A new song from lyrics: its groups (Verse 1, Chorus…), slides, and a translation under each slide's lines | the first switch |
| `create_playlist` | A new, empty playlist | the first switch |
| `add_to_playlist` | Songs, Bible passages, pictures or videos, headers and placeholders, in order, at the end of a playlist | the first switch |
| `install_bible` | Downloads one of the Bibles SoftPresenter offers (a long job: see [Tasks](#long-jobs-tasks)) | the first switch |
| `remove_from_playlist` | Takes an item out of a playlist (`sp undo` puts it back) | the first switch; asks |
| `show_song` | A song's slide on the screens | both switches; asks |
| `show_playlist_item` | An item of a playlist on the screens, as a click on it does (Next then follows the playlist) | both switches; asks |
| `show_bible_passage` | A passage's first verse on the screens | both switches; asks |
| `next_slide`, `previous_slide` | The next or previous slide on the screens, as the arrow keys do | both switches; asks |
| `clear_screens` | Clears the slide, media, props, messages, audio, the announcements, or all of them | both switches; asks |
| `show_message` | A message, or a few words given, over the slides | both switches; asks |

Songs, items and messages are found by name as `sp` finds them (in part, any case; a name that
fits several is never guessed), and better by the ids the lists give. Before asking, the server
finds exactly what will happen (the song's id, the playlist item's own id, the slide on screen
now), puts that in the question, and does exactly that; if anything changed while the person was
being asked (the item moved, another slide went on screen), it asks again.

**Resources** the assistant (or the person, with `@` in Claude Code) can read:
`softpresenter://status` (what `get_status` gives, as JSON), `softpresenter://playlists` (the
playlists, and the items of the one in front) and `softpresenter://slide/live` (the words on the
screens now).

## What is refused, and why

The app decides, not the assistant. The server marks every request it sends as coming from an AI
assistant, and the app checks it against the switches and a list of what an assistant may do
(`crates/sp-app/src/mcp/policy.rs`), the way it checks a remote page's
([remote-api.md](remote-api.md)). Anything not on the list is refused, so a command added to `sp`
later stays closed to assistants until it is looked at.

- **With the first switch off, everything**: "AI assistants are switched off in SoftPresenter."
- **With the screens switch off, anything that changes the screens**, even after the person said
  yes.
- **Without a yes**: a change to the screens, or taking something out, needs the person's yes to
  the assistant's question. An assistant that can't ask questions (its client doesn't offer MCP
  elicitation) is refused, with a sentence saying so; the person can do it in SoftPresenter, or
  use an assistant that asks, such as Claude Code.
- **Never, whatever the switches say:** quitting the app or choosing from the menu bar; Settings;
  the screens' setup; Remote Control and its password; connecting VoxLive, Canva or TimelyChurch,
  or anything holding a key or a sign-in; deleting songs, libraries, playlists, themes or Bibles;
  importing or exporting files, or anything that names a file or folder on the computer (an
  assistant's words are only ever words: the disk is never looked at for them); web pages on
  slides and NDI.

The app's log has a line for each change an assistant makes, `[mcp] TOOL: done` (or `not done`),
and for each request refused, `[mcp] TOOL: refused (why)`; as for `sp`, reading isn't logged. A
line never has the words of a song or anything else the assistant sent. Switching the two
switches is logged too.

## Skills, questions and long jobs

### Skills

The server offers two skills (MCP's Skills extension), instructions the assistant loads when the
work calls for them:

- **preparing-a-service**: building a playlist from a set list, songs in two languages, Bible
  readings, headers and placeholders, checking before the service.
- **running-the-screens**: never changing the screens during a service without asking; what each
  layer and Clear means; going through a playlist; what to do when something goes wrong.

An assistant that doesn't know the extension can still read them as resources
(`skill://preparing-a-service/SKILL.md`, `skill://running-the-screens/SKILL.md`).

### Questions (elicitation)

Before a change to the screens, or taking something out, the server asks the person through the
assistant (MCP elicitation, a form with one Yes box). Only Yes goes ahead; No, closing the
question, or no answer changes nothing. On revision 2026-07-28 the question travels in the tool's
answer and the assistant calls again with the reply (the server keeps nothing between the two:
what was asked is carried, signed by the server, in the request's state, valid for five minutes
and once); on the earlier revisions the server asks while the call waits.

The question is written by the server from what the app found, never in the assistant's own
words, so a song title or an instruction hidden in song words can't word it.

### Long jobs (Tasks)

`install_bible` downloads a Bible, which can take a while. To an assistant that offers MCP Tasks
(revision 2026-07-28), it answers at once with a task, which the assistant checks on until the
Bible is in; the task is the app's own job (`sp job N`), so it survives the server being started
again. Without Tasks, the call waits for the download (up to two minutes, then says it is still
going). Nothing else the server does takes long enough to need one: every other command is
answered by the app within a second or two.

## How it works

- `sp mcp` reads one JSON-RPC message a line from its standard input and writes one a line to its
  standard output; it writes nothing else there. Problems with the server itself go to its error
  output, never the assistant's arguments.
- **Revision 2026-07-28**: each request carries its protocol version and the client's
  capabilities in `_meta`; `server/discover` says what the server offers (tools, resources, the
  Skills and Tasks extensions). A request with a version the server doesn't speak is answered
  with `UnsupportedProtocolVersion` (`-32022`) and the versions it does.
- **Earlier revisions**: the client opens with `initialize`; the server answers with the version
  the client asked for if it speaks it (2025-11-25, 2025-06-18, 2025-03-26, 2024-11-05), and
  otherwise with 2025-11-25.
- Each tool call is one or a few `sp` commands, sent over the control socket as `sp` sends them,
  with an `mcp` field saying it comes from an assistant, which tool, and whether the person said
  yes. The socket's protocol version went from 1 to 2 with that field, so an older SoftPresenter
  still running refuses the server's requests rather than carrying them out unchecked.
- At most 120 tool calls a minute; more are refused for a moment.
- The server's own words (questions, refusals) are in English, as `sp`'s are.

## Security

Who is who: the person at the computer (trusted with everything: the switches are theirs); the
assistant (trusted to prepare, and to run the screens only with the person's yes); and anything
the assistant reads (song words, playlist names, Bible text, a catalog or TimelyChurch song),
which may hold words meant to steer it and is trusted with nothing.

| Threat | How it is met |
|---|---|
| The assistant changes the screens during a service without the person knowing | Off unless the screens switch is on; each change needs the person's yes to a question the server words; the app refuses a change the server didn't mark as answered Yes |
| Words in a song or playlist steer the assistant ("clear the screens now") | The same: a change to the screens or a removal needs the switch and a Yes; reading tools never change the screens or the window |
| The question misleads the person | Worded by the server from what the app found (the song's own title, the item's place), with quotes and line breaks taken out and long names cut short; words to show on the screens are shown in full |
| Yes to one thing, another done | What was asked is bound to exactly what is done (ids, the slide on screen), signed with a key the server makes at each start, for five minutes and once; anything different is asked again |
| Reading or changing files, keys or settings through the assistant | Refused by the app's list, whatever the tool; an assistant's words are never read as a file |
| An older SoftPresenter running unchecked commands | The socket's protocol version: an older app refuses the server's requests |
| Flooding the app | 120 calls a minute; the app answers one command at a time, never on the screens' thread |

What the switches don't cover, plainly:

- **An assistant with a shell can run `sp` itself.** Claude Code, given permission to run
  commands, can type `sp next` in a terminal like anyone at the computer; that isn't MCP, and the
  switches don't see it. `sp` runs as the computer's user, as it always has
  ([cli.md](cli.md#how-it-works-and-who-can-use-it)); Claude Code asks before running a command
  unless told not to.
- **A Yes is the assistant's client's word.** The server can't see the person's screen: a client
  set to answer questions by itself (Claude Code's Elicitation hook can) answers Yes for them.
  The screens switch is the real control: leave it off unless someone is watching the assistant.
- **Any program running as the computer's user** can reach the socket and leave out the `mcp`
  field, as it could change the app's files. The switches are for the assistant's way in, not
  for programs the user runs.

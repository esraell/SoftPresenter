# AI assistants (MCP)

SoftPresenter can work with an AI assistant, such as Claude in Claude Code or Claude Desktop,
through the Model Context Protocol (MCP): the assistant can look at what is on the screens, find
songs and Bible passages, make songs and playlists for a service, and, only when the person at
the computer has allowed it and says yes each time, run the screens.

- [Connecting](#connecting): in one click (Claude Desktop, Claude Code, ChatGPT / Codex, Cursor,
  VS Code / GitHub Copilot, Antigravity, Grok Build), by hand, other assistants
- [What the assistant can do](#what-the-assistant-can-do): the tools, what each needs
- [What is refused, and why](#what-is-refused-and-why)
- [Skills, questions and long jobs](#skills-questions-and-long-jobs)
- [How it works](#how-it-works) and [its security](#security)

Nothing here works until it is allowed in **Settings › AI assistants**, under How much it may
do, one choice of three (kept as two switches in `ai-assistants.json`):

- **Off** (unless another is chosen): nothing can connect.
- **Prepare**: the assistant may read the library, the Bibles and the playlists, and make songs
  and playlists. Nothing on the screens changes.
- **Prepare and run the screens**: as well, it may put a slide, a passage or a message on the
  screens, go to the next or previous slide, and clear the screens; each time it asks the person
  first. Without it, it can't, even when the person says yes to its question.

Under the choice, two short lists say what Prepare allows and what is never allowed.

## Connecting

The server is the app's own program: `sp mcp` (or `softpresenter mcp`). The assistant starts it
and talks to it over its standard input and output; it passes each request to the running
SoftPresenter through the same socket `sp` uses. It never starts SoftPresenter: with the app
closed, every tool answers "SoftPresenter isn't open."

Nothing is added to any assistant by itself, at install or when SoftPresenter starts: these are
other programs' settings, and they change only when you click or type it.

### In one click

**Settings › AI assistants**, under Connect one, has a tile for each company, **Claude**,
**ChatGPT**, **Gemini** and **Grok**, each with its logo (its initials where there is none), and
the other apps (Cursor, VS Code) under **Other apps**, a row each. A tile's line says which of
its apps are on this computer and where SoftPresenter stands in them ("Claude Desktop and Claude
Code found", "Added to Claude Desktop and Claude Code" with a green light, "Added to Claude
Desktop; not yet to Claude Code.", or "For Grok Build" where none is found). **Add** adds
SoftPresenter to every app of the tile found here and becomes **Remove** once it is in, which
takes it out of each app it is in (an entry by that name that starts another program is left to
that app); when such an entry is there (another copy of SoftPresenter, say), **Replace** points it
at this one. Then quit and reopen the app. A line under the tiles says what the last click did,
and that the versions in a browser can't connect. Adding SoftPresenter lets nothing in by itself:
the choice above does.

**Copy setup**, on every tile, copies a message to paste into that assistant, written to it: how
to connect (its own way, with this computer's path to `sp` filled in and quoted as a shell takes
it, and the plain stdio command for any other version), then how to use SoftPresenter: what its
tools do, that changing the screens needs the person's yes each time, what it can never do, and
to start by asking what is on screen. **What gets copied** shows it. The message stays English,
as everything the server tells an assistant does. Other apps' **Copy setup** copies the settings
to add by hand, as those apps keep them.

| Tile: app | Found when | What Add does |
|---|---|---|
| Claude: Claude Desktop (Mac, Windows) | its folder is there (`~/Library/Application Support/Claude`, `%APPDATA%\Claude`) | `mcpServers.softpresenter` in its `claude_desktop_config.json` |
| Claude: Claude Code | `claude` is found | `claude mcp add --scope user softpresenter -- PROGRAM mcp` |
| ChatGPT: Codex (the ChatGPT app, Codex's command line and editor extensions share its setting) | `codex` is found | `codex mcp add softpresenter -- PROGRAM mcp`, which writes `~/.codex/config.toml` |
| More: Cursor | `~/.cursor` is there | `mcpServers.softpresenter` in `~/.cursor/mcp.json` |
| More: VS Code / GitHub Copilot | `~/.copilot` (or `COPILOT_HOME`) is there | `mcpServers.softpresenter` in its `mcp-config.json`, with `"type": "stdio"` and `"tools": ["*"]` |
| Gemini: Antigravity (which took the Gemini command line's place) | `~/.gemini/antigravity`, `~/.gemini/antigravity-cli` or `~/.gemini/config` is there | `mcpServers.softpresenter` in `~/.gemini/config/mcp_config.json` |
| Grok: Grok Build | `grok` is found | `grok mcp add -s user softpresenter -- PROGRAM mcp`, which writes `~/.grok/config.toml` |

PROGRAM is this computer's path to `sp` (the installed `sp` when it opens this copy; on Windows
the program itself, `SoftPresenter.exe`, since an assistant starts a program, not a batch file).
`claude`, `codex` and `grok` are looked for where their installers put them (under the home
folder, Homebrew's folder, `/usr/local/bin`, npm's folder on Windows) and then on the PATH.

When SoftPresenter changes a settings file itself:

- Only `mcpServers.softpresenter` changes. Every other byte stays as it was: other servers, other
  settings, the order, the spacing, the line endings. The new text is read back before it is
  written, and must hold exactly what the file held with only that changed.
- A file that isn't valid JSON (or isn't laid out as an object with `mcpServers` an object) is
  left untouched, and the card says why. A file with only spaces in it counts as empty.
- The file as it was is kept beside it first, as `….before-softpresenter` (replaced at each
  change), with the file's own permissions.
- The new file is written whole beside it and renamed over it, so it is never half written. A
  file or folder that is a link is left alone, never written through.
- A missing file is made, readable only by you.

When the app has its own command (Claude Code, Codex, Grok Build), SoftPresenter runs it, by its
full path with each word apart (no shell), from your home folder, and stops it after 30 seconds.
It reads where SoftPresenter stands first (`claude mcp get softpresenter`, `codex mcp get
softpresenter --json`, `grok mcp list --json`), so an entry starting another program is only
replaced when you choose Replace, and it copies Codex's and Grok Build's `config.toml` beside it
first. Claude Code's own settings are never touched by SoftPresenter (Claude Code keeps its own
copies). How each app writes its file is up to the app: Grok Build's own command drops comments
from `config.toml`.

`sp mcp connect WHICH`, `sp mcp disconnect WHICH` and `sp mcp status` (`--json` for a script)
do the same from a terminal ([cli.md](cli.md#ai-assistants-mcp)).

Apps that only reach servers on the internet can't start one on this computer: ChatGPT in a
browser, and the Grok and Gemini apps. SoftPresenter isn't put on the internet for them.

### By hand

**Copy setup** on each tile copies what to add by hand, with this computer's path to `sp`
filled in: the line for Claude Code, the `mcpServers` settings for Claude Desktop, Cursor, VS
Code, Antigravity and most others, and the `config.toml` table for Codex and Grok Build.

#### Claude Code

    claude mcp add --scope user softpresenter -- ~/.local/bin/sp mcp

(`--scope user` makes it available in every folder, not only the one it was added in.) In Claude
Code, `/mcp` then lists SoftPresenter with its tools, and its two skills appear as
`softpresenter:preparing-a-service` and `softpresenter:running-the-screens`.

#### Claude Desktop, Cursor, Antigravity and others

Add SoftPresenter to the app's MCP settings file (Claude Desktop's is, on a Mac,
`~/Library/Application Support/Claude/claude_desktop_config.json`; on Windows,
`%APPDATA%\Claude\claude_desktop_config.json`), then quit and open the app again:

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

The command must be the full path (these apps don't read your shell's PATH). On Windows it is
the program itself, `SoftPresenter.exe`, wherever it is installed, with `"args": ["mcp"]`.

#### ChatGPT, Codex and Grok Build

In `~/.codex/config.toml` (ChatGPT's app and Codex) or `~/.grok/config.toml` (Grok Build), or in
the ChatGPT app's Settings › MCP servers:

```toml
[mcp_servers.softpresenter]
command = "/Users/you/.local/bin/sp"
args = ["mcp"]
```

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

A test copy (quiet, its own data folder, or a capture run) never reads or changes a real
assistant's settings: the card's list and `sp mcp connect` look only under
`SOFTPRESENTER_ASSISTANTS_HOME`, a stand-in home folder, and find nothing without it.

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
| `create_song` | A new song from lyrics: its groups (Verse 1, Chorus…), slides, and a translation under each slide's lines (at most four lines a slide in each language, each at most 100 characters, so a whole slide always fits in a question) | the first switch |
| `create_playlist` | A new, empty playlist | the first switch |
| `add_to_playlist` | Songs, Bible passages, pictures or videos, headers and placeholders, in order, at the end of a playlist | the first switch |
| `install_bible` | Downloads one of the Bibles SoftPresenter offers (a long job: see [Tasks](#long-jobs-tasks)) | the first switch |
| `remove_from_playlist` | Takes an item out of a playlist (`sp undo` puts it back) | the first switch; asks |
| `show_song` | A song's slide on the screens | both switches; asks |
| `show_playlist_item` | An item of a playlist on the screens, as a click on it does (Next then follows the playlist) | both switches; asks |
| `show_bible_passage` | A passage's first verse on the screens | both switches; asks |
| `next_slide`, `previous_slide` | The next or previous slide on the screens, as the arrow keys do | both switches; asks |
| `clear_screens` | Clears the slide, media, props, messages, audio, the announcements, or all of them | both switches; asks |
| `show_message` | A message kept in SoftPresenter, or a few words given, over the slides (words given are kept as SoftPresenter's quick message, in place of the words it had, as the Messages tab's Words once… keeps them) | both switches; asks |

Each tool answers in words and, where it has them, with the same facts as JSON
(`structuredContent`), which some assistants are shown in place of the words. The facts say what
each is on its own: `search_songs` gives each song's `title` and `id` and where the words were
found (`where_found`: "in its title and on slide 2", with `title_holds_the_words` and
`slides_holding_the_words`), `read_playlist` each item's `number`, `uid`, `kind`, `title` and a
song's `song_id`, and so on.

Songs, items and messages are found by name as `sp` finds them (in part, any case; a name that
fits several is never guessed), and better by the ids the lists give. Before asking, the server
finds exactly what will happen (the song's id, the playlist item's own id, the slide on screen
now), puts that in the question with the words of the slide that will go up, and does exactly
that; if anything changed while the person was being asked (the item moved, another slide went
on screen, the words changed), it asks again.

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
- **A Bible SoftPresenter doesn't offer**: `install_bible` takes the short name of one that
  `list_bibles` lists to download (ASV, or НПУ in any case); any other name, or a file's path, is
  refused as not offered, pointing to `list_bibles`.

The app's log has a line for each change an assistant makes, `[mcp] TOOL: done` (or `not done`;
a Bible download says `started (job N)` when it starts, and `done (job N)` or `failed (job N)`
when it ends, whether or not the assistant waits for it), and for each request refused,
`[mcp] TOOL: refused (why)`; as for `sp`, reading isn't logged. An
`[mcp]` line never has the words of a song or anything else the assistant sent (the app's other
lines are as for any change: a song that couldn't be saved is named, whoever made it). Switching
the two switches is logged too.

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
words, and it shows the words that will go on the screens (the slide's, or the message's), not
only a name: an assistant can make a song and give it any title, so a title alone proves
nothing. A slide an assistant made is always shown whole (its slides are kept small enough); a
long slide from elsewhere in the library is shown to about 800 characters. Quotation marks of
every kind, and the characters that change how text is shown (right-to-left overrides,
zero-width spaces), are taken out of what it quotes.

### Long jobs (Tasks)

`install_bible` downloads a Bible, which can take a while. To an assistant that offers MCP Tasks
(revision 2026-07-28), it answers at once with a task, which the assistant checks on until the
Bible is in; the task is the app's own job (`sp job N`), so it survives the server being started
again. Job numbers start again from 1 when SoftPresenter does, so a task's id also carries a word
the app makes each time it starts: after the app has been started again, an earlier task is an
unknown task (JSON-RPC's invalid params), never the new run's job of the same number. Without
Tasks, the call waits for the download (up to two minutes, then says it is still going, or that
SoftPresenter was started again meanwhile). Nothing else the server does takes long enough to need one: every other command is
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
| The question misleads the person | Worded by the server from what the app found (the song's title, the item's place, and the words of the slide that will go up), with quotation marks of every kind, format characters and line breaks taken out and long words cut short; words for a message are shown in full |
| Yes to one thing, another done | What was asked is bound to exactly what is done (ids, the slide on screen), signed with a key the server makes at each start, for five minutes and once; anything different is asked again |
| Reading or changing files, keys or settings through the assistant | Refused by the app's list, whatever the tool; an assistant's words are never read as a file |
| An older SoftPresenter running unchecked commands | The socket's protocol version: an older app refuses the server's requests |
| Flooding the app | 120 calls a minute; the app answers one command at a time, never on the screens' thread |
| Adding SoftPresenter breaks an assistant's settings | Only on a click or a typed command; only `mcpServers.softpresenter` changes and the result is checked before it is written; an invalid file is never written; a copy of the file as it was is kept; written whole and renamed, never through a link; an entry starting another program is replaced only on Replace |
| An assistant (or a remote page) adds itself | `mcp` is answered by `sp` itself, never by the app; the remote page's list and the assistants' list both refuse it |

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

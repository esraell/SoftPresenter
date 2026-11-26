# Automation: triggers, actions and the devices a church already has

Date: 2026-11-26. Status: approach chosen by the owner; slice 1 designed here in detail and checked by a
reviewer reading it as its builder; later slices outlined, each to get its own short design
before it is built.

## Why

The owner: "we want this application to be compatible with any churches existing setup, if they
use midi ques then we have that too so they dont need to work around a new system, or if the do
DMX then we do it too, we do it all, they have streamdeck and their volunteers rely on that, well
they wont need to get rid of it cuz we support it". A church moving to SoftPresenter keeps its
MIDI cues, its lighting, its Stream Deck and its volunteers' habits.

The first real case is the owner's own church, whose lights run on Home Assistant: "tag a macro to
a video and when video plays lights dim and when video is done lights go up automaticly".

## Goals, non-goals, anti-goals

Goals
- One way of making things happen: a **trigger** starts **actions** (one, or a macro of several).
  Every new protocol is a new trigger, a new action, or both. No second system beside it.
- Every action goes through the app's one command handler and its permission rules
  (`control.rs`, `remote/policy.rs`, `mcp/policy.rs`), so logging and refusals stay one system.
- Simple for a volunteer: connecting a service or device is done once, in Settings; choosing
  what happens is done in the Add action menus they already use; what starts it is set where
  they would look for it.
- The show never waits on another device: a slow or missing Home Assistant, MIDI port or
  lighting node never holds up a slide, a video or the screens.

Non-goals (for now)
- Speaking another presentation program's network API so existing Companion or Stream Deck
  setups work unchanged (considered as approach B, not chosen).
- Running a church's lighting as a full lighting console.

Anti-goals
- Anything that can make the audience screens hitch.
- A step added from the phone page or by an AI assistant that reaches outside the app without
  the operator having put it there.
- Losing a church's macros or slide actions because a file holds a step this build doesn't know.

## Approaches considered

- **A, chosen (owner: "i say A"):** native support for each protocol, all on one engine of
  triggers and actions. Our own Stream Deck plugin (Elgato's SDK) and a Bitfocus Companion module
  talk to the app over its existing Remote Control API. Works without extra software; one place
  to set things up; one set of permissions. A church that set up Stream Deck buttons for other
  software re-makes them once.
- B: A, plus speaking the network API of the software churches are moving from, so their old
  Companion and Stream Deck setups work unchanged. Zero rework for volunteers, but much larger,
  and it follows another product's changes. Not chosen.
- C: one rich API and a Companion module, leaving MIDI, OSC and DMX to Companion. Fastest, but
  makes churches install and learn Companion, the opposite of the goal. Dropped.

## Where things live (agreed with the owner)

1. **Settings › Integrations: connecting, once.** One card per service or device, holding only
   the connection, never what it does. The list is split in two: Services (VoxLive, Canva Sync,
   Worship Leader App, TimelyChurch, Home Assistant) and Devices (MIDI, Stream Deck and Companion,
   OSC, Lighting (DMX over Art-Net and sACN)).
2. **What happens: the Add action menus.** The menus slides and macros already have. Each
   connected integration adds one entry; one not connected doesn't appear. Home Assistant lists
   the church's own scenes, scripts and lights by name, read from Home Assistant.
3. **What starts it: triggers.**
   - Inside the show, set on the item itself: a slide (as today), a video or picture starting
     and ending (new, on the media bin item), a song going live (later).
   - Outside the show: keys, MIDI, OSC and DMX input, in one table, "When this happens → do
     this": Settings › Keys becomes **Keys and triggers**.
   - Stream Deck buttons are set up in the Stream Deck app or in Companion, as churches already
     do; each button picks a SoftPresenter action or macro from a list the app gives.
4. **Macros** stay what they are: a named bundle of actions, for when the same things should
   happen from several places. A single action needs no macro.

The design session draws every screen before its UI is built (Settings › Integrations › Home
Assistant, the media item's "When it starts" / "When it ends", the Add action entries, Keys and
triggers).

## Order of slices

1. **Lights on a video (this document):** tolerant reading of actions; Home Assistant
   connection and action; a media item's actions when it starts and ends; a Run macro action;
   `sp` commands. The macro editor's polish (owner: "macros editing ui needs to be polished,
   and should be robust enough to be able to trigger and do whatever they need to do") comes
   straight after, with its own short design drawn from the web prototype's Macros tab.
2. **Macro engine:** actions that wait, run any `sp` command, run in turn without holding the
   window; Keys and triggers table; a song going live as a trigger.
3. **MIDI** in and out, including playback rigs (Ableton Live, MultiTracks Playback, Loop
   Community Prime) through the computer's virtual MIDI ports (IAC on a Mac, loopMIDI on
   Windows).
4. **Stream Deck plugin and Companion module.**
5. **OSC** in and out (lighting desks, mixers, QLab).
6. **DMX over Art-Net and sACN** (owner: "also dmx over artnet").
7. **Scripts**, written by a person or an AI assistant, run as an action; an assistant's script
   never runs until the operator has read and approved it in the app, a changed script needs
   approval again, and the phone page can never add one.
8. Later: timecode (MTC/LTC), OBS and vMix, Blackmagic ATEM, webhooks.

## Slice 1 in detail: lights on a video

### What the operator does

1. Settings › Integrations › Home Assistant: types the address (`http://homeassistant.local:8123`
   or an IP), pastes a long-lived access token made in Home Assistant, and clicks Test. The card
   says "Connected to Home Assistant (version …)" or what went wrong.
2. In the media bin, on a video: **When it starts** › Add action › Home Assistant › Scenes ›
   "Sanctuary dim". **When it ends** › Add action › Home Assistant › Scenes › "Sanctuary full".
   A macro can be chosen instead (Add action › Run macro › "Lights down").
3. During the service, the video goes on screen and the lights dim; when it leaves the screen
   the lights come up. Nothing else to do.

### Decided (the owner's words where they gave them)

- The tag lives on the **media bin item**: wherever that video plays, the lights follow. (My
  default; the owner agreed with the proposal as a whole.)
- **"Ends" is any way the item leaves the screen:** it plays to its end, it is cleared, or
  something replaces it. So the lights never stay down by mistake. A looping video ends only
  when it leaves. (My default, agreed with the proposal.)
- Home Assistant actions: turn on a **scene**, run a **script**, or turn a **light** on (with a
  brightness and a fade time) or off. The lists are read from the church's Home Assistant.
- The show never waits: the call is made off the window's and the screens' threads, with a
  timeout; if it fails, the video plays on and a notice says the lights step didn't work.

### My calls (marked so they can be changed)

- **Both layers count.** A video starts and ends its actions on the Media layer and on the
  Announcements layer alike ("wherever that video plays").
- **Ending follows what the room sees:** an item has ended when it leaves the screens that show
  the slides. An Announcements loop a slide takes off the main screens has ended, even where a
  lobby screen keeps playing it; coming back to the main screens starts it again. (Otherwise a
  pre-service loop's lights-down would last through worship in any church with a foyer screen.)
- **Live means the operator put it live**, whether or not the audience screens are switched on
  at that moment. A preview or a thumbnail never fires anything.
- **No flicker between two tagged videos:** when an item with actions is replaced directly by
  another that has When it ends actions of its own, the first one's When it ends actions are
  skipped (the second one's will put things back). Replaced by anything else, or cleared, they
  run, so the lights never stay down because of the rule.
- **Quitting** while a tagged item is live runs its When it ends actions, waiting at most two
  seconds before the app closes, so the room isn't left dark.
- **Pictures too:** a picture in the media bin gets the same two lists; it costs nothing more.
- **Address:** `http://` is accepted for addresses on the church's network (private IP ranges and
  `.local` names) with a line on the card saying the connection isn't encrypted; anything else
  needs `https://`. The token is kept in the computer's secure store, as the other services'
  keys are, and never written to the log or to a settings file.
- A Home Assistant action with Home Assistant disconnected is kept, shown dimmed with "Home
  Assistant isn't connected", and does nothing when run.

### Keeping files safe (must land first)

- Every place actions are stored (`macros.json`, slide actions in song files, the media bin's
  file) reads an action kind it doesn't know as **kept, not dropped**: it stays in the file
  exactly as it was, is shown as "A step from a newer version of SoftPresenter", does nothing
  when run, and is written back unchanged.
- Today `macros::load` turns a file it can't read into no macros at all, and the next save
  writes that over the file. A file that can't be read must be left untouched, with a notice,
  and not saved over.
- **What is kept:** an action whose kind (`do`) this build doesn't know is kept as its raw JSON;
  so is a known kind holding a value this build doesn't know (a new kind of clear, say). Known
  actions stay plain values, so comparing and finding actions works as it does today. Unknown
  extra fields on a known action are not kept (serde's usual behaviour); later slices add new
  kinds rather than new fields to old ones.
- **Song files' format version:** a new action kind does not raise the version a song file is
  written at (`sp-model/src/format.rs`), from this slice on. Raising it would make every older
  build refuse the whole song, so tolerant reading would never reach song files. (Builds from
  before this slice will call such a song damaged rather than newer; they don't overwrite it.)
- **The media bin item's two new lists** (When it starts, When it ends) are added to the model
  in the first commit, even before anything fills them, so no build from then on drops them.
- **A file that can't be read:** `macros.json` and the media bin's file (`media::load_list` has
  the same "unreadable means empty" fallback) are then left exactly as they are, with a notice
  naming the file, and that part of the app is read-only until the next start: nothing new is
  made, changed or saved there (the start-up conversions in `macros.rs` included).
- These land in the first commit, before anything writes a new action kind, so builds from
  here on are safe to go back from. That commit also goes to the Windows services on 27 and 29
  November if a build is made, so it must change nothing else.

### Permissions

- The phone page and `sp` may **run** macros and put media live, as today, which may now run
  Home Assistant actions the operator set up.
- The phone page may **not add, replace or remove** an action that reaches outside the app (a
  Home Assistant action now; any-command, MIDI out, DMX and scripts in later slices), anywhere
  actions are kept: `macro add`, `macro step`, `macro remove`, `media set ITEM starts|ends`.
  Removing one matters as much as adding one: taking out a lights-up can leave the room dark.
  Nor may it add a Run macro naming a macro that holds one. This is checked in the command
  handler, which can see what a macro or item holds (as `App::only_at_the_computer` does), not
  by the words alone; `remote/policy.rs` keeps refusing what it can tell from the words.
- **Song packs:** "Nothing in a pack is run" (`pack.rs`). A Home Assistant action that comes in
  a pack is kept but held: shown dimmed as "Came in a song pack: choose it again to use it",
  and it does nothing until the operator chooses it again on this computer. Run macro actions in
  a pack name a macro by its id, which doesn't exist on another computer: kept, shown as a
  macro that isn't here. A media item's When it starts / When it ends lists don't travel in
  packs.
- AI assistants get no macro or Home Assistant tools in this slice (`mcp/policy.rs` lists none).
- Settings › Integrations › Home Assistant is the operator's: the phone page can't connect,
  disconnect or read the token, as with the other services.

### `sp` commands

- `sp homeassistant connect ADDRESS` (the token read from standard input, never an argument),
  `sp homeassistant status`, `sp homeassistant disconnect`, `sp homeassistant list
  [scenes|scripts|lights]`.
- Actions in the words `macro add` already takes: `homeassistant scene NAME`, `homeassistant
  script NAME`, `homeassistant light NAME on [BRIGHTNESS%] [fade SECONDS]|off`, `run macro NAME`.
- `sp media set ITEM starts "ACTION [then ACTION…]"`, `sp media set ITEM ends "…"`, `sp media set
  ITEM starts none`; `sp media` shows what each item does.
- docs/cli.md updated in the same commits.

### Built how (left to the builder within these lines)

- A new action kind for Home Assistant (domain, service, entity, optional brightness and fade),
  stored by Home Assistant's entity id, shown by its friendly name read from Home Assistant, and
  a Run macro kind (by the macro's id).
- Media bin items gain two lists of actions, saved with the item, empty by default; older files
  open with none.
- The trigger is found where the live media changes (`sync_video` and its picture and
  Announcements counterparts in `main.rs`), on the app's thread; the actions are handed to the
  same runner macros and slide actions use.
- Home Assistant calls go over its REST API with `ureq` (already a dependency): `GET /api/` to
  test, `GET /api/states` for the lists (read when a menu opens, cached, refreshed in the
  background), `POST /api/services/{domain}/{service}` to act. One worker thread, a timeout of
  about three seconds, no retries; each call logged without the token.
- No new crates.

### Tests

- A pretend Home Assistant served on 127.0.0.1 in the tests (as `timelychurch/mock.rs` does):
  connect, wrong token, lists, each action, a slow reply (the show doesn't wait), no reply.
- **Never the church's real Home Assistant:** it runs the building's lights.
- Tolerant reading: a file with an unknown action kind loads, shows it, runs the rest, and is
  written back byte-for-byte the same in that part; a `macros.json` that can't be read is left
  as it was.
- Triggers: a tagged video put live runs When it starts once; cleared, replaced and played out
  each run When it ends once; two tagged videos back to back don't flash; quitting runs When it
  ends.
- Permissions: the phone page's `macro add … homeassistant …` is refused; running a macro that
  holds one isn't.
- Headless egui tests for the new rows and menus once the design session has drawn them.

## Decided while reviewing (the builder's own calls, written down)

- Home Assistant scripts are run with `script.turn_on`, so a call never waits for the script
  to finish; the version shown on the card comes from `GET /api/config` (`/api/` gives none).
- A Run macro action never runs a macro already running in the same chain (no loops).
- Props and themes don't hold actions in this slice.
- An AI assistant can make a tagged item go live through the commands it already has (`go`,
  `next`, `playlist go`), as the phone page can; that runs the item's actions, which the
  operator set up.

## Left to the builder

The exact names of the new types and functions, how the Home Assistant list is cached, the
wording of notices (plain, British spelling, es/ru/uk for every new line), and how slice 1 is
split into commits, as long as the file-safety commit comes first and master stays shippable
after each commit (the Windows services on 27 and 29 November run from builds of it).

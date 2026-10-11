# Voice Actions reference

Enter command mode with your **Voice Actions shortcut**, then say one of the
phrases below. Matching is local and instant — no model is involved, and no
phrase ever becomes a shell command (see `SECURITY.md`).

Phrases are matched case-insensitively and trailing punctuation is ignored.
A polite lead-in is stripped, so "Hey AirPrompt, open Safari." works.

## Opening applications

| Say | What happens |
|---|---|
| "open Safari" | Opens Safari |
| "open chrome" | Fuzzy-matches **Google Chrome** |
| "open vsc" | Matches **Visual Studio Code** by acronym |
| "launch Spotify" | Same as *open* |
| "go to Terminal" | Same as *open* |

Matching prefers an exact name, then a prefix, then the shortest containing
name, then an acronym, then edit distance. If Docker and Docker Desktop are both
installed, "open docker" picks the shorter name — matching exact beats matching
longest.

When a match is not confident enough (below 72%), AirPrompt will not act silently.
It shows the best guess and asks.

## Opening websites

Known names need no network lookup:

`youtube` · `github` · `gmail` · `google` · `maps` · `chatgpt` · `claude` ·
`drive` · `calendar` · `notion` · `figma` · `linkedin` · `reddit` ·
`stack overflow` · `x` · `twitter` · `netflix` · `spotify`

| Say | What happens |
|---|---|
| "open youtube" | Opens https://youtube.com |
| "open github.com" | Opens https://github.com |
| "open example dot com" | Spoken "dot" is understood |
| "open localhost 3000" | Opens http://localhost:3000 |

Only `http` and `https` are allowed. Every other scheme — including `file:`,
`javascript:`, `data:` and `ms-msdt:` — is refused. URLs containing embedded
credentials (`https://user:pass@host`) are refused too.

## Opening folders and files

| Say | Opens |
|---|---|
| "open downloads" | `~/Downloads` |
| "open download folder" | `~/Downloads` |
| "open documents" | `~/Documents` |
| "open desktop" | `~/Desktop` |
| "open pictures" / "open photos" | `~/Pictures` |
| "open movies" | `~/Movies` |
| "open music" | `~/Music` |
| "open projects" / "open project folder" | `~/Projects` |
| "open home" | your home directory |

An explicit path works too: "open ~/Projects/my-app". A path ending in an
executable extension (`.exe`, `.bat`, `.ps1`, `.sh`, `.app`, `.command`, `.lnk`
and similar) is **refused** — launching an arbitrary binary from a spoken phrase
is not something this app does.

## Shortcuts (macOS)

| Say | What happens |
|---|---|
| "run my Focus shortcut" | Runs the Shortcut named *Focus* |
| "run morning routine" | Runs the Shortcut named *Morning Routine* |

Running a Shortcut is classified **medium risk**, so AirPrompt confirms before
running it. Shortcut names are matched against the Shortcuts app's library;
if the name is not found it is offered at low confidence rather than guessed.

## Searching and copying

| Say | What happens |
|---|---|
| "search for local speech recognition" | Opens a web search |
| "google rust tauri" | Same |
| "copy the draft" | Puts the text on the clipboard |

## Controlling AirPrompt

| Say | What happens |
|---|---|
| "start dictation" / "dictate" | Begins a dictation take |
| "stop" / "stop dictation" / "cancel" | Ends it |
| "open settings" / "preferences" | Opens the dashboard Settings |
| "change model to small" | Switches the speech model |

## Custom commands

Create these in **Voice Actions → Add command**. A custom command maps one or
more trigger phrases onto a fixed sequence of actions you chose in advance.

Example — trigger `start coding` (and `coding mode`):

1. Open application: Visual Studio Code
2. Open folder: `~/Projects/my-app`

Say "start coding" and both happen. Trigger matching is **exact**: "start coding
tomorrow morning" does *not* fire, because command mode is entered deliberately
and a prefix match would let an unrelated sentence launch a workspace.

Each command has an **Always ask before running** switch for anything that opens
more than one thing.

## What AirPrompt will never do

There is no intent — and therefore no phrasing — that can:

* run a shell command, script or interpreter,
* delete or move files,
* change a system setting,
* launch an executable,

and no AI model is ever asked to produce a command. A phrase containing shell
syntax (`&&`, `|`, `$(`, backticks, `;`, `sudo `, `Invoke-Expression`, …) is
rejected outright by the parser rather than filtered downstream.

## Adding a new action

1. Add a variant to `CommandIntent` in `core/src/storage.rs`.
2. Add a matching `ActionKind` and its risk level in `core/src/security.rs`.
3. Add the parse rule in `core/src/commands.rs`.
4. Implement it in both platform adapters.

Step 3 of `SECURITY.md`'s threat model is why this is a four-file change rather
than a string in a table.

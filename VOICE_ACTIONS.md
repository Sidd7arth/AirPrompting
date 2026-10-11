# Voice actions (command mode)

Command mode turns speech into desktop actions. Where dictation inserts text,
command mode **does** things — with a safety layer between your voice and your
machine.

## Invoking

Hold the command shortcut (default `Option+Command+Space` on macOS,
`Ctrl+Alt+Space` on Windows), say one command, release. The overlay shows the
parsed intent and what will happen; medium-risk actions ask for confirmation
first.

## Built-in grammar

| Say | Does |
|---|---|
| "open **safari**" / "open **spotify**" | launches an installed application |
| "open **downloads folder**" | opens a folder in Finder / Explorer |
| "open **github.com**" | opens a URL in the default browser |
| "run **morning**" | runs one of your macOS Shortcuts (macOS) |
| "copy this" / "paste that" | clipboard actions on the last transcript |
| "new note", "screenshot" … | your custom commands (below) |

Application names are matched against the installed-app list the shell
discovers on each platform; the overlay confirms what matched.

## Custom commands

Dashboard → **Commands** → add a phrase plus its action. A custom command can:

- insert text (a snippet, with cursor placement),
- open an app, URL or folder,
- run a shortcut (macOS).

Custom phrases take precedence over built-ins when both match.

## The safety model

Every parsed command passes the same pipeline — there is exactly one executor,
and it is the only path from a transcript to an action
([commands.rs](src-tauri/src/commands.rs)):

1. **Parse** into a structured intent with a confidence score. Below the
   confidence floor the command is refused — AirPrompt never "guesses hard".
2. **Judge**: anything that smells like a shell invocation, a path escape, a
   non-HTTP URL scheme, or an unrecognised target is refused, not sanitized.
3. **Classify risk**: opening an app/URL/folder runs immediately; anything
   flagged medium-risk, multi-step, or referencing a named external target
   requires explicit confirmation in the overlay.
4. **Execute** with validated, whitelisted operations only. There is no shell
   anywhere in the product — commands cannot spawn `sh`/`cmd` no matter what
   is spoken.

If the transcript is actually dictation that happens to look like a command
("open quotes and brackets"), dictation wins: command mode only runs from the
command shortcut.

## Failure behaviour

- Unknown target → overlay error naming what was not found.
- Permission missing (e.g. Accessibility off on macOS) → the command fails
  with a button that opens the right System Settings pane.
- Any failure → the transcript text is preserved (clipboard / recovery file);
  a voice command never destroys what you said.

Full grammar details: [docs/COMMANDS.md](docs/COMMANDS.md).
Security design: [docs/SECURITY.md](docs/SECURITY.md).

# Troubleshooting

Start here: **Settings → Diagnostics** shows the state of the engine, model,
microphone, shortcut, accessibility access and AI provider, plus median timings.
"Copy diagnostics" produces a report with no transcript content, safe to share.

---

## Nothing happens when I press the shortcut

1. **Is the shortcut registered?** Settings → Shortcuts shows each binding, and
   reports a conflict rather than failing silently. Another app may already own
   the combination — pick a different one.
2. **Input Monitoring (macOS).** A global shortcut cannot work without it.
   Settings → Permissions → Input Monitoring → Grant.
3. **Is a model installed?** With no model, AirPrompt cannot start a take. The
   dashboard shows "Needs a model" and links to the Models screen.
4. **Is the overlay hidden?** Turn off *Show the overlay* and you would see
   nothing, but the transcript would still be inserted. Check Settings →
   Appearance.

## The overlay appears but no text is inserted

This is almost always **Accessibility access** on macOS.

* Settings → Permissions → Accessibility → Grant.
* Pressing *System Settings* is not enough on its own: macOS requires you to
  enable the app in the list, and it usually requires a restart of the app
  afterwards, because the trust decision is made when the process starts.

Until access is granted, AirPrompt copies the transcript to the clipboard instead
and tells you so. Dictation itself is unaffected.

If a password field had focus, insertion is skipped deliberately — the text goes
to the clipboard with a message. This is not configurable.

## Text is inserted into the wrong application

* Turn on **Settings → Dictation → Restore focus before inserting**. AirPrompt
  captures the focused application when recording starts and returns to it
  before inserting.
* If the target app was closed while you were speaking, AirPrompt reports "The
  original app is no longer available" and leaves the transcript on the
  clipboard rather than typing into whatever is in front.
* Some apps (notably Electron-based editors and terminals) ignore synthetic
  paste. Switch that app's profile to **Type** insert mode in Settings →
  Dictation, or set a per-application profile.

## "Unable to access the microphone"

macOS: **System Settings → Privacy & Security → Microphone** → enable AirPrompt.
Windows: **Settings → Privacy & security → Microphone** → allow desktop apps.

The permission is requested when you first dictate, and again from Settings →
Permissions → Microphone → Grant. Microphone disconnect mid-take is handled as a
clean error, not a crash: the take is abandoned and the overlay reports it.

## The microphone works but the level meter stays flat

The meter is driven by real audio levels, so a flat bar means no audio is
arriving. Settings → Speech → Input device: pick a specific device rather than
"System default" — some interfaces report a default that is not the one you are
speaking into. Then Settings → Speech → Audio test to confirm.

## Speech recognition returns nothing or nonsense

* **Too short.** Takes under 250 ms are rejected rather than sent to a model
  that would hallucinate on them.
* **Silence detection too aggressive.** Settings → Speech → Automatic silence
  detection: move from *Short* to *Balanced* or *Long*, or set it to *Off*. The
  detector adapts to the room's noise floor, but a fan close to the microphone
  can still defeat it.
* **Wrong model size.** Tiny mis-hears names and technical vocabulary. Small is
  noticeably better; see MODEL_MANAGEMENT.md.
* **Wrong language.** Settings → Speech → Language.

## A model download fails or stalls

* Verify your connection: the download is an HTTPS GET to `huggingface.co`.
* **Local Only mode blocks downloads by design.** Settings → Privacy → turn it
  off to download, then turn it back on — models stay installed.
* Pause and resume rather than restarting: resume continues with an HTTP Range
  request from the current offset.
* If a download completes but fails verification, delete and retry. A failure
  here usually means a proxy returned an HTML page instead of the model.

## "The whisper.cpp speech engine is not installed"

See MODEL_MANAGEMENT.md for how to build `whisper-cli` and where to put it. You
can also point AirPrompt at an existing binary with the `AIRPROMPT_WHISPER_BIN`
environment variable.

## AI enhancement never runs

Settings → AI Enhancement reports why:

* *Choose an AI provider first* — the provider is None.
* *Local Only mode blocks online providers* — turn off Local Only, or use Ollama,
  LM Studio or a llama.cpp server, which are local and stay usable.
* *Set an API endpoint* — the endpoint is empty.

Then press **Test connection**. It reports the actual failure: key rejected (401),
endpoint or model not found (404), rate limited (429), provider error (5xx), or
timed out.

Enhancement failing **never** costs you a dictation. The original transcript is
used and the overlay tells you what happened.

## Where does the temporary WAV go?

`whisper-cli` cannot read raw PCM from stdin, so each decode writes a short
16 kHz mono WAV into the app's cache directory and deletes it as soon as the
process exits, including on failure.

| Platform | Cache directory |
|---|---|
| macOS | `~/Library/Caches/com.airprompt.app/` |
| Windows | `%LOCALAPPDATA%\com.airprompt.app\cache\` |

If you see files accumulating there, the recogniser process was killed before it
could clean up — they are safe to delete. If you would rather no such file
existed, MODEL_MANAGEMENT.md explains the in-process alternative.

## "Shortcut is already in use"

Another application has registered that combination globally. Choose a different
one in Settings → Shortcuts. AirPrompt reports the conflict instead of silently
failing to register.

## Models take up too much space

Models → Storage used shows the total. Delete any model you are not using; a
model that is currently active must be swapped out first. Models live outside
the app bundle, so deleting a model does not affect your settings, and updating
the app never re-downloads them.

## Nothing works at all and I see no window

AirPrompt is a menu bar / tray application. If the dashboard is closed, look for the
icon in the menu bar (macOS) or the notification area (Windows). Left-click opens
the control menu; *Open Dashboard* shows the main window.

## The app will not start after an update

Launch it from a terminal to see the log, or read the log file directly —
Settings → Diagnostics → Reveal shows its location. If a settings file were
corrupted it falls back to defaults rather than failing to start, so a startup
failure is more likely a permissions or bundling issue. Include the copied
diagnostics when reporting it.

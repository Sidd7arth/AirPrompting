# Privacy

## Default data flow

```
microphone
   → RAM
   → local speech model
   → local text
   → the application you were using
   (audio is discarded; nothing is uploaded)
```

Dictation works with the network unplugged. Nothing in the basic path makes a
network request, and there is no account, login or subscription anywhere in the
product.

## What is stored, and where

| Data | Stored? | Where | Left behind by |
|---|---|---|---|
| Audio recordings | **No** by default | Never written | Nothing to delete |
| Transcripts | Yes, if *Save history* is on | Local SQLite, `airprompt.sqlite3` | Settings → History |
| Custom dictionary | Yes | Same database | Settings → Dictionary |
| Voice commands | Yes | Same database | Settings → Voice Actions |
| App profiles | Yes | Same database | Same database |
| Settings | Yes | Same database (`kv` table) | Settings → reset |
| API keys | Yes, if you enter one | OS credential store (Keychain / Credential Manager) | Settings → AI → Clear |
| Latency samples | Yes | Same database, timings only | Advanced → Reset diagnostics |
| Speech models | Yes | App data directory | Models → Delete |

The one exception to "audio is never written": see *The temporary WAV* below.

## What can leave the machine

Exactly three things, and only when you turn them on:

1. **Model downloads.** HTTPS GET to `huggingface.co` for a GGML model, started
   only by pressing Download on the Models screen.
2. **AI enhancement.** The transcript — and only the transcript — is POSTed to
   the endpoint you configured, if you enable enhancement **and** select an
   OpenAI-compatible (online) provider. Audio is never uploaded.
3. **Update checks.** A version query to the release endpoint.

Nothing else. Specifically, AirPrompt never sends audio, never sends file
contents, never sends clipboard contents, never sends keystrokes, and never
sends your command phrases.

## Local Only mode

Settings → Privacy → **Local Only** makes the guarantee enforceable rather than
a promise: the Rust core refuses outbound requests while it is on. Model
downloads and online enhancement both fail with a clear message instead of
quietly using the network. Dictation is completely unaffected, so the app stays
fully usable in airplane mode.

When Local Only is on and an online provider is selected, enhancement is
switched off automatically and the UI explains why.

## The temporary WAV

`whisper-cli` cannot read raw PCM from stdin — it accepts a WAV file and nothing
else. So each decode:

1. writes a 16 kHz mono WAV into the app's own cache directory,
2. runs the recogniser against it,
3. deletes it immediately, including when the recogniser fails.

The file never leaves that directory, is never indexed into history, and is not
retained between takes. If you would rather not have that file exist at all,
build whisper.cpp in-process instead (`whisper-rs`) — `docs/MODEL_MANAGEMENT.md`
explains the trade-off.

Turning on *Save recordings* keeps takes in the app data directory. We
recommend leaving it off.

## Permissions, and why

| Permission | What it is for | What happens without it |
|---|---|---|
| Microphone (macOS, Windows) | Hearing you while you dictate | Dictation is unavailable; everything else still works |
| Accessibility (macOS) | Inserting text into the app you are using | The transcript goes to the clipboard and you paste it |
| Input Monitoring (macOS) | Your global shortcut working while another app is focused | The shortcut only works while AirPrompt is frontmost |
| UI Automation (Windows, optional) | Better insertion reliability in some apps | Falls back to the clipboard and simulated keys |

No permission is requested without an explanation on screen of what it is for
and what stops working without it.

## Telemetry

Disabled, and there is no code path that collects transcript content, audio,
file contents, API keys or command phrases under any setting. The diagnostics
screen shows machine facts and *aggregate* timings, which is what the "Copy
diagnostics" button assembles — safe to share.

## Your controls

* **Save history** — off means nothing is written.
* **Automatically delete after** — Never, 1, 7 or 30 days.
* **Delete all history** — with a confirmation.
* **Local Only mode** — blocks every network path.
* **Clear API key** — removes it from the OS credential store.
* **Delete a model** — frees the space immediately.

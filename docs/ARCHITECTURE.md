# Architecture

## The one rule

**The shared core never calls an operating system API directly.**

Everything OS-specific lives behind the traits in
`src-tauri/core/src/platform.rs`, and the implementations live in
`src-tauri/platform/`. There is no `#[cfg(target_os = …)]` anywhere in the
core; `cfg` selects real behaviour in exactly one place — the platform crate's
`create()` function.

## Layers

```
┌─────────────────────────────────────────────────────────────────┐
│ Frontend — React + TypeScript + Vite + Tailwind                 │
│   Dashboard · Overlay · History · Models · Commands · Settings  │
│   Talks to Rust through one typed bridge (src/services/ipc.ts)  │
└───────────────────────────────┬─────────────────────────────────┘
                                │ invoke() / one event channel
┌───────────────────────────────▼─────────────────────────────────┐
│ airprompt (src-tauri/src) — Tauri shell                         │
│   windows · tray · global shortcuts · IPC commands · pipeline   │
│   Owns no domain logic; it sequences core calls                 │
└───────────────────────────────┬─────────────────────────────────┘
                                │
┌───────────────────────────────▼─────────────────────────────────┐
│ airprompt-core — platform-independent, GUI-free, testable       │
│   settings · storage · audio · vad · transcription · whisper    │
│   formatting · enhancement · models · commands · security       │
└───────────────────────────────┬─────────────────────────────────┘
                                │ traits only
┌───────────────────────────────▼─────────────────────────────────┐
│ airprompt-platform — macOS and Windows adapters                 │
│   accessibility · CGEvent / SendInput · clipboard · launcher    │
│   permissions · credential store · shortcuts                    │
└─────────────────────────────────────────────────────────────────┘
```

`airprompt-core` has no Tauri dependency. That is a deliberate constraint: it is
what makes `cargo test -p airprompt-core` run without a GUI, a WebView or a display.

## Why three crates

The brief asked for the shared logic to be isolated from platform code. Crates
enforce that at compile time rather than by convention — the core literally
cannot reference a macOS symbol, because it does not depend on one.

## Dictation data flow

```
global shortcut
  → capture the focused application            (platform)
  → check microphone permission                (platform)
  → load the model if not resident             (core::transcription)
  → capture 16 kHz mono audio                  (platform audio → core::audio)
  → level meter + VAD                          (core::audio, core::vad)
  → stream hypotheses every ~1 s               (core::transcription)
  → stabilise into committed + interim         (core::transcription::StreamingTranscript)
  → stop, final decode
  → deterministic formatting                   (core::formatting)
  → optional AI enhancement                    (core::enhancement)
  → restore focus, insert text                 (platform)
  → append to history                          (core::storage)
  → hide the overlay
```

The overlay is updated from a single event channel (`airprompt://event`) carrying
overlay state, audio level, transcript, model progress, history inserts,
permission changes and toasts. The dashboard and the overlay window run the same
store, so they cannot disagree.

## Module responsibilities

| Module | Responsibility |
|---|---|
| `core/settings.rs` | Settings model, privacy-first defaults, validation, shortcut bindings |
| `core/storage.rs` | SQLite: settings, history, dictionary, commands, app profiles, latency samples |
| `core/audio.rs` | Mono downmix, level metering, windowed-sinc resampler, PCM conversion, ring buffer |
| `core/vad.rs` | Adaptive voice activity detection with attack/release hysteresis |
| `core/transcription.rs` | `SpeechEngine` trait, streaming transcript stabiliser, thread defaults |
| `core/whisper.rs` | whisper.cpp backend: process management, WAV encoding, output parsing |
| `core/formatting.rs` | Spoken punctuation, dictionary, filler removal, continuous dictation, GAAV |
| `core/enhancement.rs` | `AiEnhancementProvider` trait, prompt construction, OpenAI-compatible client |
| `core/models.rs` | Model catalogue, hardware recommendation, resumable verified downloads |
| `core/commands.rs` | Deterministic voice-command parser + fuzzy target matching |
| `core/security.rs` | Command allowlist, risk policy, URL/path validation, shell-marker rejection |
| `core/platform.rs` | The traits the core depends on — the whole OS surface |
| `platform/macos.rs` | Accessibility, CGEvent, NSWorkspace, Shortcuts, permissions, Keychain |
| `platform/windows.rs` | SendInput, UI Automation, ShellExecute, hotkeys, Credential Manager |

## Design decisions worth knowing

**Enhancement is never required.** It runs after the transcript exists and every
failure path returns the original text. A dead AI provider cannot cost you a
dictation.

**Voice commands are parsed locally first.** The deterministic parser handles
"open chrome", "open downloads", "run my focus shortcut" with no model at all.
Output is a value from a closed enum; the executor calls a named platform
function. There is no `sh -c` and no `powershell -Command` anywhere in the
repository — see `docs/SECURITY.md`.

**One HTTP client, five providers.** llama.cpp, Ollama, LM Studio, Groq and
OpenAI all speak the OpenAI chat-completions shape, so there is one client with
an endpoint, not five integrations.

**Streaming is re-decoding, not true streaming.** whisper.cpp has no incremental
API, so a growing audio window is re-decoded on a schedule. `StreamingTranscript`
exists to make that pleasant: a word is only committed once two consecutive
decodes agree on it, which is what stops the overlay flickering.

**Models are data, not code.** The catalogue is static; nothing downloads
without an explicit user action, and a download is verified before it can be
loaded.

## Implementation status

Checked on 2026-10-06 on Windows x64, cross-compiled to `i686-pc-windows-gnu`.
Read this before assuming a feature is finished. "Compiles" means a real
`cargo check` exited 0, not that it looked right.

| Area | Status | Notes |
|---|---|---|
| `airprompt-core` logic | **Compiles clean** | `cargo check -p airprompt-core --all-targets` exits **0** with zero errors and zero warnings. 7,155 lines across 15 modules, including all test code. |
| `airprompt-core` tests | **Written, never executed** | The test *binary* could not be linked on the build host (see below), so **none of the assertions have run**. Treat them as unproven. |
| Frontend | **Verified** | `npm run build` exits **0**: `tsc --noEmit` clean, 2,030 modules, both windows bundled. |
| `airprompt-platform` | **Not written** | Only the trait dispatch in `lib.rs` exists. `macos.rs` and `windows.rs` are missing, so `cargo check -p airprompt-platform` fails (exit 101). **This is the largest remaining piece of work.** |
| Tauri shell (`src-tauri/src`) | **Stub — app cannot launch** | 16 lines total: `main.rs` calls an empty `airprompt_lib::run()`. The project manifest, `build.rs`, `capabilities/` and the icons now exist and validate, so the project is well-formed — but the shell itself is not written, so `npm run tauri dev` still cannot start the app. **0 of the 62 IPC commands declared in `src/services/ipc.ts` are implemented in Rust**, so every UI action currently fails. |
| End-to-end dictation | **Not runnable** | Blocked on the platform adapters and the shell wiring, and on a whisper.cpp binary being present. |
| Voice actions | Parser and safety policy implemented in `core/src/commands.rs`; execution path needs the platform adapters. | |
| Docs | Complete | This set. See `docs/STATUS.md` for the full section-by-section audit against the specification. |

### Build-host limitations, stated plainly

The development host for this revision was Windows with no Visual Studio C++
tools. The installed MinGW-w64 is gcc 6.3.0, which is too old to link modern
Rust (it lacks the `bcrypt`/`dbghelp` import libraries), so:

* `cargo check -p airprompt-core --all-targets` **passes**, because `check` does not
  link. This is real type-checking of every module and every test.
* `cargo test -p airprompt-core` **compiles both test targets but fails at the link
  step** on `-lbcrypt`. The assertions have therefore never been executed.
* `cargo check -p airprompt` (the Tauri shell) was not attempted: it needs a
  Windows C toolchain for its dependencies.

Run these on a machine with the proper toolchain — on macOS, Xcode Command Line
Tools is sufficient:

```bash
cd src-tauri
cargo test -p airprompt-core          # the suite has never run; expect findings
cargo check -p airprompt-platform     # currently fails: adapters are unwritten
cargo check --workspace            # fails until the shell is wired
```

A failing test here is a real bug or a wrong expectation, not a reason to soften
an assertion: the assertions encode the behaviour the product needs.

macOS:

```bash
cd src-tauri && cargo test -p airprompt-core && cargo check --workspace
npm run tauri dev
```

## Repository layout

This repository **is** the product: the frontend, the Rust crates, the
documentation and the build scripts all live at the root, and there is no nested
application directory to confuse a reader or a packager.

Reference material was moved out of the shipping tree: an earlier Swift
prototype of the same product, and the third-party sources studied while
deriving the GAAV and continuous-dictation rules that `core/formatting.rs`
implements. Neither is needed to build AirPrompt, and keeping them in-tree made
the repository look like a copy of another application. What was reused is
implemented here in AirPrompt's own modules, and the third-party projects the
product genuinely depends on — whisper.cpp, FluidAudio, SQLite, Tauri — keep
their own names and their own attribution. The full accounting is in
[BRANDING_AUDIT.md](../BRANDING_AUDIT.md).

## Product identity and migration

Every name AirPrompt uses for itself is defined once, in two mirrored files:

| Where | File | What it holds |
|---|---|---|
| Rust | `src-tauri/core/src/identity.rs` | display name, bundle identifier, data and cache directory names, database file, credential-store service, log namespace, event channel, environment variables |
| TypeScript | `src/identity.ts` | the same name and identifier, the event channel, and the browser-storage prefix |

Nothing else in the tree spells the product name out. The HTML entries cannot
import a module, so they carry `%APP_NAME%`-style placeholders that
`vite.config.ts` fills from `src/identity.ts` at build time — which is why the
built `dist/index.html` reads "AirPrompt" although no literal appears in the
HTML source. Component copy renders `{APP_NAME}` rather than a hard-coded
string, so a future rename is one edit in each of the two files above.

`core/src/migration.rs` owns the upgrade path. On first launch it looks for data
left under an earlier identity in the per-user application data directory,
validates that the directory really holds this product's files rather than an
unrelated application with a similar name, copies them into the AirPrompt
directory — renaming only the database to `airprompt.sqlite3`, so no two
competing databases can appear — and writes a marker so the work never repeats.
It never deletes or moves the old data, never overwrites a file the new install
already owns, and logs paths and counts rather than content. The legacy names it
recognises are migration identifiers; each one is listed with its justification
in [BRANDING_AUDIT.md](../BRANDING_AUDIT.md).

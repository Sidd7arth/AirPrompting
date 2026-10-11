# Development

## Layout

```
app/
├── src/                        React frontend (one store, one IPC bridge)
│   ├── components/             ui.tsx (primitives), Waveform, ShortcutRecorder, Toasts
│   ├── pages/                  Dashboard, Models, History, Commands, Settings,
│   │                           Diagnostics, Onboarding
│   ├── overlays/               The floating overlay window
│   ├── services/               ipc.ts (typed backend bridge), format.ts (display helpers)
│   ├── stores/appStore.ts      Single source of UI truth
│   ├── styles/globals.css      Design tokens for both themes
│   └── types/index.ts          Domain types, mirrored from Rust
└── src-tauri/
    ├── src/                    Tauri shell (windows, tray, shortcuts, IPC)
    ├── core/                   airprompt-core — no GUI, no Tauri, fully testable
    └── platform/               airprompt-platform — macOS and Windows adapters
```

## Commands

```bash
npm run dev            # Vite only, in a browser (no backend; the UI says so)
npm run typecheck      # tsc --noEmit
npm run build          # typecheck + production frontend bundle
npm run tauri dev      # full desktop app
npm run tauri build    # installers

cd src-tauri
cargo test -p airprompt-core   # core logic, no GUI required
cargo clippy --workspace    # lints
cargo fmt --all             # formatting
```

## Conventions

**Types cross the boundary once.** A Rust type's `serde` representation is the
wire format, and `src/types/index.ts` mirrors it. Structs are
`#[serde(rename_all = "camelCase")]`, enums are `snake_case`. Do not "prettify" a
name on one side only — the mismatch will fail at runtime, not at compile time.

**Every settings struct is `#[serde(default)]`.** A settings file written by an
older build must always load. `Settings::normalized()` then clamps anything out
of range, so the frontend cannot wedge the backend.

**The core never touches the OS.** If you need a platform call, add it to
`PlatformIntegration` in `core/src/platform.rs` and implement it twice. If you
find yourself typing `#[cfg(target_os` in `core/`, stop.

**Errors are user-facing sentences.** `core::Error::message()` returns text safe
to show. Variants carry detail for logs; `message()` is what the user reads. Never
surface a raw error string in the UI.

**No shell, ever.** See `SECURITY.md` before adding anything that opens a
program, file or URL.

**Colours come from tokens.** Use `bg-bg`, `text-fg`, `border-border` and the
other names in `tailwind.config.js`; they resolve through the CSS variables in
`globals.css`, which is what makes both themes correct. A hard-coded hex will
look wrong in one of them.

## Adding an AI provider

Most providers need no code — llama.cpp, Ollama, LM Studio, Groq and OpenAI all
speak the same chat-completions shape, so they are entries in
`AiProviderKind` with an endpoint. If a genuinely different protocol is needed:

1. Implement `AiEnhancementProvider` (`core/src/enhancement.rs`).
2. Add the variant to `AiProviderKind` with `is_local()`, `default_endpoint()`
   and `secret_key()`.
3. Surface it in `PROVIDERS` in `pages/Settings.tsx`.

`enhance_or_original` already guarantees that any failure returns the original
transcript, so a new provider cannot lose a dictation.

## Adding a speech engine

Implement `SpeechEngine` (`core/src/transcription.rs`) — `prepare`, `transcribe`,
`is_ready`, `unload`, plus `is_available`/`unavailable_reason` so the settings UI
can explain why a backend is not usable. `WhisperCliEngine` is a complete
reference, including process management and timeout handling.

The trait deliberately takes a finished audio buffer rather than a token stream,
because every backend in scope works that way. Streaming is achieved by
re-decoding a growing window, and `StreamingTranscript` makes that stable.

## Adding a voice action

Four steps, and the order matters because each one narrows what is possible:

1. `CommandIntent` in `core/src/storage.rs` — the wire value.
2. `ActionKind` + `risk()` in `core/src/security.rs` — the permission.
3. A parse rule in `core/src/commands.rs` — how a phrase maps to it.
4. The execution in **both** platform adapters.

A unit test asserts no allowlisted intent is ever high risk, so adding one that
needs confirmation will fail the suite until you classify it deliberately.

## Tests

The core suite is the main safety net and needs no display:

```bash
cargo test -p airprompt-core
```

It covers resampling and level metering, VAD hysteresis and noise-floor
adaptation, spoken punctuation and continuous dictation, the dictionary
(including multi-byte text), filler removal, settings normalisation and
migration, the SQLite store (search escaping, retention, profiles), model
integrity checks, prompt construction and response cleanup, the streaming
transcript stabiliser, WAV encoding, and the command parser plus its safety
policy.

Frontend: `npm run typecheck`. There is no component test suite yet; the build
is the current check.

## Debugging

* Settings → Diagnostics → Log level. `debug` and `trace` add detail and never
  include transcript content.
* Settings → Diagnostics → Reveal opens the log file.
* `npm run dev` in a browser shows the UI without a backend; the app detects
  this and displays a banner rather than failing. Anything requiring the backend
  will report "AirPrompt desktop backend is not running".
* Tauri devtools: right-click the window → Inspect, in a dev build.

## Before committing

```bash
npm run typecheck
cd src-tauri && cargo fmt --all && cargo clippy --workspace && cargo test -p airprompt-core
```

Then build the app once with `npm run tauri build`. A change that compiles in
isolation but breaks the bundle is a change that is not finished.

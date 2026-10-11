<<<<<<< HEAD
# AirPrompt

Fast, private, local-first **voice dictation** and **voice actions** for macOS and Windows.

> Press. Speak. See. Insert. Done.

AirPrompt lives in your menu bar or system tray. Hold a shortcut, talk, and the
transcript appears in a floating overlay and is inserted into whatever
application has focus. A second shortcut turns speech into safe desktop
actions — open an app, a website, a folder, or one of your macOS Shortcuts.

Everything runs on your machine. No account, no subscription, no telemetry, and
it works with the network off.

---

## What is in this repository

```
.                         The product root (Tauri 2 + Rust + React)
├── src/                  React + TypeScript + Vite frontend
│   ├── components/       Shared UI primitives
│   ├── pages/            Dashboard, Models, History, Commands, Settings, …
│   ├── overlays/         The floating dictation overlay
│   ├── services/         Typed IPC bridge to the Rust core
│   ├── stores/           Single application store
│   ├── identity.ts       The product's own names, defined once (spec §19)
│   └── types/            Domain types mirrored from Rust
├── src-tauri/
│   ├── src/              Tauri shell: windows, tray, IPC commands, hotkeys
│   ├── core/             airprompt-core — platform-independent logic (testable)
│   └── platform/         airprompt-platform — macOS and Windows adapters
├── docs/                 Architecture, security, privacy, build, troubleshooting
├── scripts/              Build-time helper scripts
└── .env.example          Development configuration (no secrets)
```

The repository root **is** the product root — there is no nested application
directory. Reference material (an earlier Swift prototype and the third-party
sources studied while deriving the formatter rules) lives outside the shipping
tree; that separation is recorded in [BRANDING_AUDIT.md](BRANDING_AUDIT.md).

---

## Requirements

| | macOS | Windows |
|---|---|---|
| OS | macOS 11+ (13+ recommended) | Windows 10 1809+ / Windows 11 |
| Architecture | Apple Silicon or Intel | x64 |
| Node.js | 20+ | 20+ |
| Rust | 1.77+ | 1.77+ |
| Build tools | Xcode Command Line Tools | Visual Studio Build Tools (C++) |
| Runtime | — | WebView2 (present on Windows 11) |

Full instructions: [docs/BUILD_MACOS.md](docs/BUILD_MACOS.md) ·
[docs/BUILD_WINDOWS.md](docs/BUILD_WINDOWS.md)

---

## Quick start

Run these from the repository root:

```bash
npm install
npm run tauri dev      # development, with hot reload
npm run tauri build    # production .app/.dmg or .msi/.exe
```

On first run, onboarding asks for the microphone, a shortcut and a speech model.
**No speech model is bundled**; you choose one and AirPrompt downloads it. See
[docs/MODEL_MANAGEMENT.md](docs/MODEL_MANAGEMENT.md).

---

## Running the core tests

The shared core has no GUI and no Tauri dependency, so its logic is testable
anywhere Rust runs:

```bash
cd src-tauri
cargo test -p airprompt-core
```

The suite covers audio resampling and level metering, voice activity detection,
spoken-punctuation and continuous-dictation formatting, the dictionary, the
command parser and its safety policy, settings normalisation, the SQLite store,
the model catalogue and integrity checks, provider prompts and response
cleanup, the streaming transcript stabiliser, and WAV encoding.

Frontend type checking and build:

```bash
npm run typecheck
npm run build
```

---

## Documentation

| Document | Contents |
|---|---|
| [RELEASE_READINESS.md](RELEASE_READINESS.md) | **Release gates with evidence — read this first** |
| [STATUS.md](docs/STATUS.md) | Earlier audit of the spec against reality (partly superseded) |
| [ARCHITECTURE.md](docs/ARCHITECTURE.md) | Layers, data flow, module responsibilities |
| [SECURITY.md](docs/SECURITY.md) | Threat model, command allowlist, why there is no shell |
| [PRIVACY.md](docs/PRIVACY.md) | Exactly what leaves the machine, and when |
| [DEVELOPMENT.md](docs/DEVELOPMENT.md) | Layout, conventions, how to add a provider |
| [BUILD_MACOS.md](docs/BUILD_MACOS.md) | macOS build, signing, notarisation |
| [BUILD_WINDOWS.md](docs/BUILD_WINDOWS.md) | Windows build, installers, signing |
| [TROUBLESHOOTING.md](docs/TROUBLESHOOTING.md) | Permissions, audio, insertion, models |
| [MODEL_MANAGEMENT.md](docs/MODEL_MANAGEMENT.md) | Models, sizes, storage, integrity |
| [COMMANDS.md](docs/COMMANDS.md) | Every supported voice action |
| [TOOLCHAIN.md](docs/TOOLCHAIN.md) | Dev-host toolchain limitations and the CI compile path |

---

## What is not included

AirPrompt deliberately ships **no** speech model, **no** AI model and **no** API
keys. These are external assets:

| Asset | Why | Where it comes from |
|---|---|---|
| whisper.cpp `whisper-cli` binary | Local speech recognition | Built from [ggml-org/whisper.cpp](https://github.com/ggml-org/whisper.cpp) or from a release bundle; placed in the app resources or pointed at with `AIRPROMPT_WHISPER_BIN` |
| GGML speech model (75 MB – 1.5 GB) | The recogniser's weights | Downloaded in-app from Hugging Face (`ggerganov/whisper.cpp`) |
| Optional AI provider | Transcript cleanup | Ollama, LM Studio, llama.cpp server, or an OpenAI-compatible API |
| Apple Speech / Parakeet | Alternative engines on macOS | System frameworks / FluidAudio CoreML bundle — see MODEL_MANAGEMENT.md |

[docs/TROUBLESHOOTING.md](docs/TROUBLESHOOTING.md) explains each one.

---

## Status

**Release candidate.** The complete application is implemented — Tauri shell,
62 IPC commands, platform adapters, capture, dictation state machine, overlay,
tray, shortcuts — and every check this host can run passes (see
[RELEASE_READINESS.md](RELEASE_READINESS.md) for the gate-by-gate evidence).
The shell crate has never been compiled on this machine (no x86_64 linker,
diagnosed in [docs/TOOLCHAIN.md](docs/TOOLCHAIN.md)); the CI matrix compiles
and tests it on native macOS and Windows runners, and the packaged installers
come from the build workflow. Runtime validation on real hardware is the last
mile to **RELEASE READY**.

Note: [docs/STATUS.md](docs/STATUS.md) and [docs/TEST_REPORT.md](docs/TEST_REPORT.md)
are earlier, pre-shell audits kept for history; RELEASE_READINESS.md supersedes
their verdicts.
=======
# AirPrompting
Open-source AI voice-to-text assistant that turns natural speech into polished prompts, commands, and text.
>>>>>>> 0c1ac8b666ce8aaf666413721253c4f8b81747b2

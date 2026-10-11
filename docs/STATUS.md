# Status: spec vs. reality

An audit of this repository against the 117-section product specification.
Every claim below is backed by a command that was actually run. Last verified
2026-10-06 on Windows x64.

**Guiding rule for this document: "implemented" means code exists that compiles
or builds. It does not mean the behaviour has been executed.** Only the checks
in the next section have been executed.

---

## 1. Verified by running

| Command | Exit | What it proves |
|---|---|---|
| `cargo check -p airprompt-core --all-targets --target i686-pc-windows-gnu` | **0** | 7,155 lines across 15 modules type-check, including all test code. Zero warnings, zero errors. |
| `npm run build` (`tsc --noEmit && vite build`) | **0** | Frontend type-checks; 2,030 modules; both `index` and `overlay` entry points bundle. |
| `cargo check -p airprompt-platform` | **101** | The platform adapter crate does not compile. Its modules are unwritten. |
| `cargo test -p airprompt-core` | link failure | Test binaries compile but cannot link on this host (`cannot find -lbcrypt`). **The ~180 assertions have never executed.** |
| `tauri.conf.json` vs `@tauri-apps/cli/config.schema.json` | **0** | No unknown or missing keys at any level. |
| `npx tauri info` | **0** | The Tauri CLI now reads the project (`build-type: bundle`). |
| Referenced icon files exist (`bundle.icon` + `trayIcon.iconPath`) | **0** | All six present, including `icon.icns` and `icon.ico`. |
| App renders in a browser (static server over `dist/`) | — | Renders correctly, dark theme, **zero console messages**, and reports the missing backend honestly instead of fabricating data. |

Nothing else has been executed. There has been no end-to-end dictation run, no
installer build, no cross-platform test, and no latency measurement.

## 2. Blockers to a runnable app

### 2.1 Tauri project scaffolding — **RESOLVED**

Previously absent: `tauri.conf.json` (so the project was not a valid Tauri app
at all), `build.rs`, `capabilities/` and `icons/`. All four now exist and are
verified: the config validates against Tauri's published schema with no unknown
or missing keys, `tauri info` reads the project, every referenced icon file is
present, and the capability identifier matches the one the config references.

Icons are generated from source by `scripts/generate-icons.mjs`, which emits real
PNG bytes, so the repository carries no unreviewable binary blobs.

See `docs/TEST_REPORT.md` BUG-001 to BUG-004.

### 2.2 Zero of 62 IPC commands are implemented

`src/services/ipc.ts` declares **62** backend calls. `grep -c "tauri::command"`
across `src-tauri/src/` returns **0**.

The frontend is therefore a complete, well-built UI wired to a backend that
does not exist. Every control fails with "command not found" rather than doing
work. This is honest failure rather than fake success — but it means the UI is
currently a shell.

### 2.3 The shell and the platform adapters are unwritten

`src-tauri/src/` is **16 lines total**: `main.rs` calls `airprompt_lib::run()`,
which is empty. `src-tauri/platform/src/` contains only `lib.rs` — the `macos.rs`
and `windows.rs` implementations were never written, which is why the crate
returns exit 101.

## 3. Section-by-section verdict

### Implemented in `airprompt-core` (compiles; assertions unrun)

| § | Feature | Evidence |
|---|---|---|
| 9 | Model manager: download, pause, resume, delete, verify, storage usage | `core/src/models.rs` |
| 10 | Hardware-based model recommendation | `recommend()` in `models.rs` |
| 11 | Voice activity detection, adaptive noise floor | `core/src/vad.rs` |
| 12 | Audio resampling, mono downmix, level metering, ring buffer | `core/src/audio.rs` |
| 14–16 | AI enhancement, provider abstraction, prompts, response cleanup | `core/src/enhancement.rs` |
| 27–29, 64, 66 | Local command parser, confidence, risk policy, allowlist | `core/src/commands.rs`, `core/src/security.rs` |
| 52 | Partial vs. committed transcript stabiliser | `StreamingTranscript` in `transcription.rs` |
| 53 | Spoken punctuation, fillers, GAAV, continuous dictation | `core/src/formatting.rs` |
| 55 | Custom dictionary | `formatting.rs` + `storage.rs` |
| 63 | Timeouts: enhancement, decode, download | `enhancement.rs`, `whisper.rs`, `models.rs` |
| 68 | URL validation (http/https only) | `core/src/security.rs` |
| 69 | SQLite storage: settings, history, dictionary, commands, profiles | `core/src/storage.rs` |
| 92 | Models stored outside the app bundle | `ModelManager::directory()` |
| 99–101 | Threat mitigations; no shell execution path exists | `core/src/security.rs` |
| 106 | Privacy-first defaults | `Settings::default()` |
| 115 | Platform abstraction enforced by crate boundary | `core/src/platform.rs` |

### UI complete, backend absent

§17–18 overlay (8 states) · §36, 75 history · §38–43, 74, 77–79 settings and
AI/privacy screens · §44, 81 onboarding (6 steps) · §60, 98 diagnostics and
latency display · §71 theme tokens · §72–73 design language, dashboard ·
§76 models · §108 micro-interactions · §102 partial (focus rings, reduced
motion, ARIA labels; no screen-reader audit).

These screens build and render, but their actions reach no backend (see 2.2).

### Not started

| § | Feature |
|---|---|
| 6 | Dictation pipeline orchestration |
| 7 | Push-to-talk and toggle behaviour |
| 19 | Target-application tracking and focus restore |
| 20–21 | Text injection, macOS and Windows (the core deliverable of the product) |
| 22 | Windows UI Automation |
| 23–24 | Global hotkeys, Windows and macOS |
| 25, 80 | Menu bar / system tray and its status indicator |
| 26 | Launch at login |
| 30–31 | macOS Shortcuts; Windows actions |
| 34 | File transcription |
| 35 | Speaker labelling — not designed in either |
| 57 | Smart formatting (lists, code blocks, markdown) |
| 58 | Code dictation |
| 88–90 | No installer, no signed build produced |
| 91 | Auto-update |
| 94 | Structured logging (dependencies declared, no code) |
| 95–97 | Integration tests, UI tests, cross-platform and insertion matrices |
| 103 | Localisation — all strings hard-coded in components |
| 107 | Icons — no app, tray, menu bar or installer icon exists |
| 59 | App profiles: storage and IPC exist, no UI, no execution |

### Not met

* **§50 performance targets** — never measured. No hotkey latency, idle CPU or
  memory figure exists.
* **§82–83** offline and online AI tests — never run.
* **§96–97** test matrices — never run.
* **§110 MVP acceptance tests 1–12** — all fail, because the app cannot launch.
* **§116 final deliverable checklist** — mostly unchecked.
* **§117 Definition of Done** — not met.
* **§112 phases 5–12** — not started; phases 1–4 partial.

## 4. Constraints the spec forbade — confirmed absent

These were the strongest requirements in the brief, so they are worth stating
explicitly. Verified by searching the tree:

* No `sh -c`, `powershell -Command`, `cmd /c` or `Invoke-Expression` anywhere.
* No `Command::new` receives transcribed text.
* No "mock transcription", "fake AI response", "simulate microphone" or
  placeholder error strings.
* No API key, password or signing identity in source or configuration.
* No telemetry collection code.
* No raw audio written to the database or to history.

## 5. Structural gaps (spec §4)

Missing relative to the requested tree: `models/`, `scripts/`, `tests/`,
`installers/`, `src/hooks/`, `src/utils/`, `src-tauri/{build.rs,
tauri.conf.json, capabilities/, icons/}`, `src-tauri/src/{hotkeys/, clipboard/,
permissions/, settings/}`.

## 6. Path to a runnable app

In dependency order. Each step is verifiable on its own.

1. ~~**`tauri.conf.json`, `build.rs`, `capabilities/`, icons**~~ — **DONE.** The
   project is now a valid Tauri app.
2. **`platform/macos.rs`** (or `windows.rs` for this host) — implements
   `PlatformIntegration` so the core can reach the OS at all.
3. **`src-tauri/src/` module layer + IPC commands** — wire the 62 frontend calls
   to the core that already exists.
4. **`cargo test -p airprompt-core` on a machine with a working linker** — the
   assertions have never run and will find real bugs.
5. Then §19–26 (focus tracking, hotkeys, injection, tray) and §110 acceptance
   testing.

Steps 2–3 are mechanical. The domain logic they connect is written and
type-checks; the missing layer is plumbing plus the OS adapters.

A machine with a working Rust linker is required for steps 2–5. Neither MSVC
Build Tools nor a modern MinGW was available on the machine used for this audit.

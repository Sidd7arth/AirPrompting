# Changelog

All notable changes to AirPrompt are documented here.
Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/);
versioning follows [SemVer](https://semver.org/).

## [Unreleased]

The 0.1.0 line: the application went from "core library + UI, no shell" to a
complete Tauri application awaiting its first compiled run in CI.

### Added

- **Application shell** (`src-tauri/src/`): Tauri 2 runtime wiring — window and
  overlay management, tray menu, global-shortcut manager with diff-based
  re-registration, cpal audio capture (F32/I16/U16 with resampling), the
  dictation state machine (idle → listening → transcribing → enhancing →
  inserting, with cancel, clipboard fallback and recovery files), model
  management commands, diagnostics, and a settings watcher.
- **62 IPC commands** registered 1:1 with the frontend bridge
  (`src/services/ipc.ts`), enforced by a new guard
  (`scripts/check-ipc-contract.mjs`, wired into `npm run verify` and CI).
- **Single event channel** (`airprompt://event`) with a typed union —
  overlay, level, transcript, model, history, permissions, toast, command,
  diagnostics — matching `src/types/index.ts`.
- **Platform adapters**: macOS (AVFoundation permissions, CGEvent insertion,
  NSWorkspace app detection, Shortcuts) and Windows (Win32 APIs for memory,
  disk, app detection, SendInput insertion), behind one
  `PlatformIntegration` trait in `airprompt-core` with `camelCase` wire types.
- **Voice actions pipeline**: intent parser → confidence floor → security
  judge (shell detection, URL/path validation, confirmation for medium-risk)
  → single checked executor.
- **CI matrix**: `test.yml` (typecheck, build, 4 guards, `cargo test` core on
  all runners, `cargo test --workspace` on native macOS/Windows, fmt, clippy
  `-D warnings`, RustSec + npm audits), `ci.yml`, `build.yml` (real
  `tauri build` installers), `release.yml` (draft releases).
- **Release documentation**: `RELEASE_READINESS.md` (gate-by-gate, evidence
  based), `INSTALLATION.md`, `MODELS.md`, `VOICE_ACTIONS.md`, and
  `docs/TOOLCHAIN.md` (the full local-toolchain blocker diagnosis).
- Shell unit tests (~60) across capture, dictation, events, commands, paths,
  shortcuts, overlay, tray — they execute for the first time in CI.

### Fixed

- `reset_settings` was called by the frontend but not implemented on the
  backend (added and registered).
- `start_command_mode` was registered on the backend with no frontend wrapper
  (added to the api object).
- `DictationController::handle_activation` was referenced by the shortcut
  dispatcher without existing (implemented: toggle semantics).
- `PermissionStatus` / `InputDevice` wire format is now `camelCase`,
  matching the frontend types.
- `[lib] path` in `src-tauri/Cargo.toml` pointed at the wrong file.

### Known limitations

- The shell crate has never been compiled locally (no x86_64 linker on the dev
  host — see `docs/TOOLCHAIN.md`); CI is the compile-and-test path.
- No auto-updater endpoint; `check_update` reports "not configured" honestly.
- Installers are unsigned (no Apple Developer ID, no Authenticode cert).

## [0.1.0] — initial public line

- Core crate: settings, storage (SQLite), transcription, VAD, streaming
  partials, formatting, enhancement providers, command parser, security
  judge, model manager, whisper CLI integration.
- React UI: dashboard, overlay, history, dictionary, commands, models,
  diagnostics screens; honest browser-mode (no backend) fallback.
- Landing page with OS detection and download routing.

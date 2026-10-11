# Release readiness — AirPrompt 0.1.0

Last updated: 2026-10-09.

**Verdict: RELEASE CANDIDATE.**

The application shell, IPC layer, platform adapters and every supporting
release artifact now exist and pass every check that this host can execute.
What separates this from **RELEASE READY** is exactly two things, both
environmental:

1. The shell crate (`src-tauri/src/*.rs`, ~6,500 lines) has **never been
   compiled or executed**. This Windows host has no x86_64 linker for the Rust
   GNU toolchain (see [docs/TOOLCHAIN.md](docs/TOOLCHAIN.md)), so the compile
   and the ~60 integration tests in the shell must land via the CI matrix,
   which runs `cargo test --workspace` on native macOS and Windows runners.
2. No packaged installer has been produced and no runtime behaviour (dictation,
   overlay, insertion) has been observed by a human on any OS.

Nothing below claims a PASS without execution evidence.

---

## 1. What is proven (executed on this host, 2026-10-09)

| Check | Command | Result |
|---|---|---|
| Core + platform type check (i686-pc-windows-gnu) | `cargo check -p airprompt-core -p airprompt-platform --all-targets --target i686-pc-windows-gnu` | **PASS**, exit 0, zero warnings |
| Core + platform clippy | same with `clippy … -- -D warnings` | **PASS**, exit 0 |
| Workspace formatting | `cargo fmt --all --check` | **PASS**, no diffs |
| Shell syntax (13 files) | `rustfmt --edition 2021 --check src/*.rs` | **PASS**, all 13 parse clean |
| Frontend typecheck + build | `npm run build` | **PASS**, 2,030 modules |
| Full verify pipeline | `npm run verify` | **PASS** (typecheck, branding, secrets, versions, IPC) |
| Branding guard | `node scripts/check-branding.mjs` | **PASS**, 0 retired names |
| Secret guard | `node scripts/check-secrets.mjs` | **PASS**, no credentials |
| Version gate | `node scripts/check-versions.mjs` | **PASS**, 0.1.0 × 5 sources |
| **IPC contract guard (new)** | `node scripts/check-ipc-contract.mjs` | **PASS**, 62 = 62 = 62 |
| npm advisory audit | `npm audit --omit=dev` | 0 vulnerabilities |
| Workflow YAML validity | js-yaml over all four workflows | **PASS** |

Two real contract bugs were found and fixed by the new IPC guard this session:
`reset_settings` was called by the frontend but missing from the backend
(added, [commands.rs](src-tauri/src/commands.rs)), and `start_command_mode`
was registered on the backend with no frontend wrapper (added,
[src/services/ipc.ts](src/services/ipc.ts)). The shell dispatcher also
referenced a `handle_activation` method that did not exist — added to
`DictationController` ([dictation.rs](src-tauri/src/dictation.rs)).

## 2. CI matrix (the compile + test path)

| Workflow | Runners | Does |
|---|---|---|
| [test.yml](.github/workflows/test.yml) | ubuntu + **macos** + **windows** | `npm ci`, typecheck, build, all four guards, `cargo test -p airprompt-core` everywhere, `cargo test --workspace` + `cargo check --workspace` on native macOS/Windows (this is where the shell compiles), rustfmt, clippy `-D warnings` on the whole workspace, RustSec audit, npm audit |
| [ci.yml](.github/workflows/ci.yml) | orchestration | tests on every push/PR; packaging on main |
| [build.yml](.github/workflows/build.yml) | macos + windows | real `tauri build` → `.dmg`/`.app`, `.msi`/`.exe` (NSIS) |
| [release.yml](.github/workflows/release.yml) | orchestration | verify → build → draft GitHub release with artifacts |

The first green run of `cargo test --workspace` on a native runner converts
shell compile status from UNTESTED to PASS; a red run surfaces the first
compiler errors, which is precisely what CI is for.

## 3. Gate status

Legend: PASS = executed with evidence · NOT TESTED = code exists, never
executed · BLOCKED = environment prevents execution · NOT IMPLEMENTED.

### Application

| Gate | Status | Evidence / reason |
|---|---|---|
| Application builds (frontend, incl. landing page) | **PASS** | `npm run build` exit 0 |
| Landing page | **PASS (render + fallback)** | Served build opened in a browser: renders with zero page errors, OS detection works, no-release fallback exercised against the real API (404 → intentional panel); resolved-download + confirmation paths verified against a mocked release payload. Direct-download path unexercised (no real artifact exists yet). |
| Rust core + platform builds | **PASS** | i686 `cargo check` exit 0 |
| Shell (`airprompt`) builds | **NOT TESTED** | No x86_64 linker on host; CI is the path |
| macOS build | **NOT TESTED** | CI build.yml; no macOS host |
| Windows build/installer | **NOT TESTED** | CI build.yml; host lacks MSVC |
| Tauri app launches | **NOT TESTED** | never executed anywhere yet |
| IPC works | **PARTIAL** | contract guard PASS (names/shapes match); runtime untested |
| Global hotkey | **NOT TESTED** | registered via plugin in shell; never run |
| Microphone capture | **NOT TESTED** | cpal capture written; unit tests exist, never run |
| Local speech | **NOT TESTED** | WhisperCliEngine wraps real binary; never run |
| Live transcription | **NOT TESTED** | StreamingTranscript + VAD in core tests (unexecuted) |
| Overlay | **NOT TESTED** | controller + window written; never run |
| Text injection | **NOT TESTED** | platform adapters implement insert; never run |
| Clipboard handling | **NOT TESTED** | save/restore contract in adapters; never run |
| Offline mode | **PARTIAL** | engine is a local binary by design; end-to-end untested |
| Local AI / Cloud AI | **PARTIAL** | core provider code compiles; no live call made |
| Voice Actions | **NOT TESTED** | parser + executor + security judge written; unit tests unexecuted |
| Safety layer | **PARTIAL** | executor tests written (unexecuted); judge logic reviewed |
| History / Settings persist | **PARTIAL** | SQLite via rusqlite, core tests unexecuted |
| Permissions | **NOT TESTED** | status/request/open-settings implemented both platforms |
| Recovery | **PARTIAL** | recovery-file path implemented + tested in unexecuted suite |
| Performance | **NOT TESTED** | no measurement exists; do not quote numbers |
| Security audit | **PASS (static)** | secret guard, no-shell design, URL/path validation; no dynamic audit |
| macOS packaging | **NOT TESTED** | CI only |
| Windows packaging | **NOT TESTED** | CI only |
| Release CI passes | **NOT TESTED** | workflows YAML-valid; never run on GitHub |
| No P0 bugs | **UNKNOWN** | first compile may reveal errors; that is expected |
| No P1 bugs | **UNKNOWN** | runtime bugs unobservable from here |
| Regression suite | **PARTIAL** | ~60 shell + ~180 core assertions written; zero executed on host |
| Release documentation | **PASS** | this file + CHANGELOG, INSTALLATION, MODELS, VOICE_ACTIONS, build docs |

### macOS

| Gate | Status |
|---|---|
| Native build | NOT TESTED (CI path) |
| Runtime, dictation, text injection, voice actions | NOT TESTED (manual, needs Mac + mic) |
| Installer (.dmg) | NOT TESTED (CI path) |
| Signing / notarisation | BLOCKED (no Apple Developer credentials) |

### Windows

| Gate | Status |
|---|---|
| Native build | NOT TESTED on this host (no MSVC); CI path is windows-latest |
| Runtime, dictation, text injection, voice actions | NOT TESTED (needs a real Windows session with mic) |
| Installer (.msi/NSIS) | NOT TESTED (CI path) |
| Signing (Authenticode) | BLOCKED (no certificate) |

## 4. Known limitations shipped honestly

- `check_update` / `install_update` return "no updater configured" — there is
  no signed update endpoint yet; the UI says so rather than pretending.
- macOS build docs require Xcode CLT; Windows docs explain the MSVC requirement
  that this dev host itself fails (documented in
  [docs/TOOLCHAIN.md](docs/TOOLCHAIN.md)).
- Frontend has no unit-test runner yet; the typecheck + build + guards are the
  gate (tracked as a release task).

## 5. Path from here to RELEASE READY

1. Push to GitHub → let `ci.yml` run → fix any shell compile errors CI finds.
2. Green tests → run `build.yml` → download and install the artifacts on real
   machines.
3. Manual pass on macOS and Windows: hotkey → dictate → overlay → insertion →
   history; command mode end-to-end; model download/verify; permission flows.
4. Measure latency (hotkey→overlay, speech→final, final→insert) and record in
   this file.
5. Only then flip this document to **RELEASE READY** with evidence links.

## 6. Remaining blockers

| Blocker | Type |
|---|---|
| No x86_64 linker on dev host (shell uncompiled locally) | Environment |
| No MSVC on dev host (no local installer) | Environment |
| No macOS host, no Apple signing credentials | Environment / credentials |
| No Authenticode certificate for Windows signing | Credentials |
| No human runtime validation on any OS | Requires step 3 above |

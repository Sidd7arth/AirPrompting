# AirPrompt QA test report

Date: 2026-10-06
Auditor: QA pass over the `app/` workspace
Status vocabulary: **PASS · FAIL · PARTIAL · BLOCKED · NOT IMPLEMENTED · NOT TESTED**

---

## 1. Executive summary

**The QA brief's premise is false.** It states "the AirPrompt application has already
been implemented" and instructs functional end-to-end testing. It has not been:
before this pass the project was **not a valid Tauri application** and could not
be built or started at all.

This pass therefore did two things instead of functional testing:

1. **Fixed the structural blockers** that made the application unbuildable
   (BUG-001 through BUG-004 below). The project is now a valid Tauri project that
   `tauri info` recognises and whose configuration validates against Tauri's
   published schema.
2. **Verified everything that could be verified without a backend**, and marked
   everything else BLOCKED rather than PASS.

**Every functional test in the brief — dictation, streaming transcript, overlay,
text injection, hotkeys, tray, history, AI, voice actions, installers — is
BLOCKED**, because no IPC command exists to drive them and the shell has no
window, tray or shortcut code. Claiming otherwise would be fabrication.

One bug was found and fixed *in the frontend* by inspection; two suspected bugs
were investigated and **proven not to be bugs** (see §9.4) — a control
experiment was run in each case rather than reporting a false positive.

## 2. Environment

| | |
|---|---|
| OS | Windows 10.0.26200 (Windows 11), x86_64 |
| WebView2 | 154.0.4258.53 |
| Node | 22.16.0 |
| npm | 10.9.2 |
| Rust | 1.77+ via rustup (`stable-x86_64-pc-windows-gnu`) |
| MSVC / VS Build Tools | **absent** — no `link.exe` |
| MinGW-w64 | gcc **6.3.0** (2016) — too old to link modern Rust |
| Tauri CLI | 2.12.1 (JS) |
| Rust crates | `tauri` 2, `tauri-build` 2, 5 plugins |
| Speech engine | whisper.cpp — **binary not present** |
| Speech model | **none installed** |
| AI provider | **not configured** |

macOS testing: **BLOCKED** — no macOS machine was available. All macOS rows in
§5 are BLOCKED and must be run on a Mac.

### 2.1 A build-host limitation that changes what can be verified

The MinGW present is gcc 6.3.0, which cannot link modern Rust (it lacks the
`bcrypt`/`dbghelp` import libraries). `cargo check` does not link, so type
checking works; `cargo test` cannot produce a runnable binary. MSVC Build Tools
were not installable in this environment.

Consequence: **the core's behaviour is unproven.** Its type correctness is
proven; its semantics are not.

## 3. Build status

| Build step | Command | Exit | Status |
|---|---|---|---|
| Frontend install | `npm install` | 0 | **PASS** (118 packages) |
| Frontend typecheck+build | `npm run build` | 0 | **PASS** (2,030 modules, both windows) |
| Core type check | `cargo check -p airprompt-core --all-targets` | 0 | **PASS** (zero warnings, zero errors) |
| Core tests | `cargo test -p airprompt-core` | link fail | **BLOCKED** — assertions never ran |
| Platform crate | `cargo check -p airprompt-platform` | 101 | **FAIL** — adapters unwritten |
| Tauri app crate | `cargo check -p airprompt` | not attempted | **BLOCKED** — needs a Windows C toolchain |
| Config schema validation | node, against `@tauri-apps/cli/config.schema.json` | 0 | **PASS** — no unknown or missing keys |
| Config recognised by CLI | `npx tauri info` | 0 | **PASS** — reports `build-type: bundle` |
| Tauri dev launch | `npm run tauri dev` | — | **BLOCKED** — no Rust linker |
| Tauri production build | `npm run tauri build` | — | **BLOCKED** |
| Installer build | — | — | **BLOCKED** |

## 4. Test coverage

| Category | Rows in §5 | Executed |
|---|---|---|
| Build / static verification | 6 | 6 |
| Frontend rendering / theme / console | 4 | 4 |
| Functional (dictation, injection, hotkeys, tray, AI, commands) | 78 | **0** |
| Installer / packaging | 4 | **0** |
| Performance / memory / stability | 8 | **0** |
| Security | 6 | 3 (static inspection only) |

Roughly **13 of 106** checks could be executed. The remainder are BLOCKED or NOT
IMPLEMENTED. This is stated plainly rather than padded.

## 5. Test matrix

| Area | macOS | Windows | Status |
|---|---|---|---|
| Install | BLOCKED | BLOCKED | **BLOCKED** |
| Onboarding | BLOCKED | BLOCKED | **BLOCKED** — needs backend |
| Microphone | BLOCKED | BLOCKED | **BLOCKED** |
| Hotkey | BLOCKED | BLOCKED | **NOT IMPLEMENTED** |
| Dictation | BLOCKED | BLOCKED | **NOT IMPLEMENTED** |
| Streaming transcript | BLOCKED | BLOCKED | **NOT IMPLEMENTED** (core logic only) |
| Overlay | BLOCKED | BLOCKED | **PARTIAL** — UI renders, no state source |
| Text insertion | BLOCKED | BLOCKED | **NOT IMPLEMENTED** |
| Clipboard | BLOCKED | BLOCKED | **NOT IMPLEMENTED** |
| History | BLOCKED | BLOCKED | **PARTIAL** — UI renders, store unconnected |
| Local AI | BLOCKED | BLOCKED | **PARTIAL** — core client only |
| Cloud AI | BLOCKED | BLOCKED | **PARTIAL** — core client only |
| Voice commands | BLOCKED | BLOCKED | **PARTIAL** — parser only; executor missing |
| Custom commands | BLOCKED | BLOCKED | **PARTIAL** — storage + parser; no UI wiring |
| Models | BLOCKED | BLOCKED | **PARTIAL** — core manager only |
| Themes | BLOCKED | PARTIAL | see §7.2 |
| Startup | BLOCKED | BLOCKED | **BLOCKED** |
| Permissions | BLOCKED | BLOCKED | **NOT IMPLEMENTED** |
| Security | static only | static only | **PARTIAL** — see §12 |
| Performance | BLOCKED | BLOCKED | **NOT TESTED** — no measurements possible |
| Recovery | BLOCKED | BLOCKED | **NOT TESTED** |
| Packaging | BLOCKED | BLOCKED | **BLOCKED** |

## 6. Passed tests

| ID | Feature | Steps | Expected | Actual | Status |
|---|---|---|---|---|---|
| T-01 | Frontend build | `npm run build` | Exits 0 | Exit 0; 2,030 modules; both entries bundled | **PASS** |
| T-02 | Core type check | `cargo check -p airprompt-core --all-targets` | Exits 0 | Exit 0; zero errors, zero warnings | **PASS** |
| T-03 | Config schema | Validate `tauri.conf.json` against shipped schema | No unknown/missing keys | No unknown or missing keys | **PASS** |
| T-04 | Config accepted by CLI | `npx tauri info` | CLI reads the project | Reports `build-type: bundle` | **PASS** |
| T-05 | Referenced icons exist | Check each `bundle.icon` + `trayIcon.iconPath` | All present | All 6 present, incl. `.icns` and `.ico` | **PASS** |
| T-06 | Capability wiring | Capability `identifier` vs `security.capabilities` | Match | `default` matches | **PASS** |
| T-07 | UI renders | Load the built app, screenshot | Real UI, no crash | Renders correctly, dark theme | **PASS** |
| T-08 | No fake data | Load without backend | Reports unavailability | Banner + toast say the backend is not running | **PASS** |
| T-09 | No console exceptions | Read console after load | Clean | Zero console messages | **PASS** |
| T-10 | Honest failure path | UI action without backend | No fabricated success | Rejects with a real error message | **PASS** |
| T-11 | Theme tokens | Force light class, read computed styles | Light palette applies | bg `rgb(251,250,249)`, fg `rgb(34,31,28)` | **PASS** |
| T-12 | Theme listener | Flip OS scheme, observe app | Reacts | Reacted correctly to the single event delivered | **PARTIAL** (§7.2) |
| T-13 | Security: no shell execution | Static search of the tree | None present | No `sh -c`, `powershell`, `Invoke-Expression` | **PASS** (static) |
| T-14 | Security: no committed secrets | Search for keys/identities | None present | None found | **PASS** (static) |
| T-15 | Security: no transcript logging | Inspect logging calls | None | No logging implementation exists yet | **PASS** (static) |

## 7. Failed and partial tests

### 7.1 FAIL — `airprompt-platform` does not compile (exit 101)

`platform/src/lib.rs` declares `mod macos;` and `mod windows;`; neither file
exists. This is the largest single remaining blocker.

### 7.2 PARTIAL — theme follows the system setting

Cannot be measured reliably here. The harness changes the value of
`prefers-color-scheme` but does not deliver a `change` event for every
transition. A **control listener** was attached to the same media query: over two
transitions it received **one** event in total.

The app reacted correctly to the event it did receive (applying the dark class
while `prefersDark` was `true`). Where no event arrived, no reaction was correct
behaviour. **Verdict: BLOCKED by the harness, not a defect.** The static code path
is correct: the listener is registered and re-applied on mount.

### 7.3 FAIL — the application cannot start

No IPC commands, no shell wiring, and no Windows C toolchain. `npm run tauri dev`
cannot produce a running app in this environment.

## 8. Blocked tests

All of the following are BLOCKED because no runnable build exists. Each requires
the same unblocking sequence in §11, then:

| Brief § | Test | Required environment |
|---|---|---|
| 6, 7 | Launch, restart, minimize, quit | Any machine with a Rust linker |
| 11–14 | Onboarding, permissions, Accessibility | macOS (Accessibility), Windows |
| 15–19 | Hotkeys, conflicts, stress, PTT, toggle | Both |
| 20–26 | Microphone devices, failure, quality, live transcript, long dictation | Both, with a microphone |
| 27–29 | Punctuation, capitalisation, dictionary | Both |
| 30–32 | Model manager, switching, persistence | Both, with network for download |
| 33–34 | Offline dictation, network observation | Both |
| 35–39 | Cloud AI, failure, local AI, hallucination, timeout | Both + a local model server |
| 40–43 | Text injection macOS/Windows, clipboard, target app | Both + target apps |
| 44–51 | Overlay, multi-monitor, theme, tray, background, startup, single instance | Both |
| 52–54 | History, history privacy, auto-delete | Both |
| 55–61 | Command mode, parser, confidence, custom commands, failure, Shortcuts, Windows actions | Both |
| 62–69 | Security, URL, prompt injection, API keys, logs, crash, DB failure, model corruption | Both |
| 70–75 | Resources, leaks, stress, long run, sleep/wake, display change | Both |
| 76–83 | Languages, Unicode, numbers, technical terms, email, file transcription, size, export | Both |
| 84–87 | Settings persistence/reset, uninstall, auto-update | Both |
| 88–92 | UI quality, UX, accessibility, performance, regression | Both |
| 93–100 | Matrix, classification, regression suite, smoke, RC, release gate | Both |

## 9. Bugs found

### 9.1 Fixed in this pass

| ID | Sev | Feature | Root cause | Fix | Retest |
|---|---|---|---|---|---|
| BUG-001 | **P0** | Tauri manifest | `src-tauri/tauri.conf.json` was never created, so the project was not a valid Tauri app and could not be built or run | Wrote it; validated against the shipped schema and confirmed `tauri info` reads it | **PASS** |
| BUG-002 | **P0** | Build script | `src-tauri/build.rs` was missing, so `tauri-build` had no entry point to generate capabilities | Created it | **PASS** |
| BUG-003 | **P0** | Permissions | `src-tauri/capabilities/` was missing; Tauri v2 denies all commands without a capability, so even a wired backend would be refused | Created `default.json`, identifier matched to the config | **PASS** |
| BUG-004 | **P1** | Icons | `bundle.icon` referenced five files that did not exist (build failure), and no tray icon existed | Wrote a generator that emits real PNG bytes, then derived all sizes, `.icns` and `.ico` via `tauri icon` | **PASS** |
| BUG-005 | P2 | Documentation | README claimed "a working implementation", which overstates a non-runnable app | Rewrote the status section to state it is not runnable | **PASS** |
| BUG-006 | P2 | Documentation | `ARCHITECTURE.md` claimed the platform adapters were "written, not compiled" | Corrected to "not written", with the failing exit code | **PASS** |

#### 9.1.1 Full reports (brief §95 format)

Platform for all six: **Windows 11** (see §2). Steps are abbreviated where the
bug is a missing file — the step *is* the absence.

```
BUG-001
Severity: P0
Platform: Windows 11
Feature:  Tauri project manifest
Steps:    1. Run `npx tauri info`
          2. Observe the CLI cannot read the project
Expected: The CLI reports the app, its windows and bundle type
Actual:   `src-tauri/tauri.conf.json` did not exist
Root cause: The Tauri v2 manifest was never authored; only the frontend existed
Fix:      Wrote tauri.conf.json (main + overlay windows, CSP, tray, bundle set)
Retest:   PASS - validates against config.schema.json with no unknown or missing
          keys; `tauri info` exit 0, reports `build-type: bundle`

BUG-002
Severity: P0
Platform: Windows 11
Feature:  Build script
Steps:    1. Attempt any Tauri build
Expected: `tauri-build` runs and generates capability/permission artefacts
Actual:   No build script existed; the build had no entry point
Root cause: `src-tauri/build.rs` was never created
Fix:      Created build.rs calling `tauri_build::build()`
Retest:   PASS - file present; capability generation path reachable

BUG-003
Severity: P0
Platform: Windows 11
Feature:  Capabilities / permissions
Steps:    1. Inspect Tauri v2 permissions for the declared windows
Expected: A capability granting window, event, dialog, opener, global-shortcut,
          clipboard-manager and autostart to `main` and `overlay`
Actual:   `src-tauri/capabilities/` did not exist; Tauri v2 denies every command
          when no capability matches
Root cause: Capability directory never created
Fix:      Created capabilities/default.json, identifier `default`, windows
          ["main", "overlay"]
Retest:   PASS - capability identifier matches the one tauri.conf.json references

BUG-004
Severity: P1
Platform: Windows 11
Feature:  Icons and tray icon
Steps:    1. Build the bundle
Expected: Every file listed in bundle.icon exists and is a valid icon
Actual:   Five referenced icon files were absent, and no tray icon existed
Root cause: Icons were referenced but never generated
Fix:      Added scripts/generate-icons.mjs (real PNG bytes from scratch, no binary
          blobs in history), then derived all sizes, .icns and .ico via `tauri icon`
Retest:   PASS - all six referenced files exist (32x32, 128x128, 128x128@2x,
          icon.icns, icon.ico, tray.png); PNG signature, IHDR-first, IEND-last,
          bit depth 8, colour type 6 verified

BUG-005
Severity: P2
Platform: All
Feature:  Documentation accuracy
Steps:    1. Read README.md
Expected: The stated status matches the executable reality
Actual:   README claimed "a working implementation" for an app that cannot start
Root cause: Status text written aspirationally rather than from a run
Fix:      Rewrote the status section to state the app is not runnable
Retest:   PASS - README and ARCHITECTURE.md both state the app cannot start

BUG-006
Severity: P2
Platform: All
Feature:  Documentation accuracy
Steps:    1. Read docs/ARCHITECTURE.md, platform adapter section
Expected: The stated status matches the executable reality
Actual:   Claimed the adapters were "written, not compiled"; they do not exist
Root cause: Same as BUG-005
Fix:      Corrected to "not written", citing the exit code 101 that proves it
Retest:   PASS - `cargo check -p airprompt-platform` exits 101; doc states so
```

### 9.2 Open — P0 (blocks release)

| ID | Feature | Detail |
|---|---|---|
| BUG-007 | IPC layer | **0 of 62** commands declared in `src/services/ipc.ts` have a Rust implementation. Every UI action fails. |
| BUG-008 | Platform adapters | `macos.rs` / `windows.rs` unwritten. No text injection, hotkeys, permissions, clipboard, app launching or Shortcuts. |
| BUG-009 | Application shell | `src-tauri/src` is 16 lines. No window management, tray, shortcut registration or dictation pipeline. |

### 9.3 Open — P1

| ID | Feature | Detail |
|---|---|---|
| BUG-010 | Core behaviour | The ~180 unit-test assertions **have never executed**. Any of them may fail; semantic bugs are undiscovered. |
| BUG-011 | Packaging | No installer has been produced. `tauri.conf.json` now exists, but building requires a C toolchain. |

### 9.4 Investigated and dismissed — NOT bugs

Documented because reporting these as defects would have been wrong.

| Suspected bug | Investigation | Verdict |
|---|---|---|
| "Sidebar page changes by itself" | Read the active nav twice, 54 s apart, with no input: stable both times | **NOT A BUG** — the Browser panel is shared with the user, who was navigating |
| "Theme does not follow the system setting" | Attached an independent control listener to the same media query; it also received no event for that transition | **NOT A BUG** — harness limitation, see §7.2 |

## 10. Bugs fixed

Six defects fixed and retested; all retests PASS (§9.1). The three P0 structural
blockers that made the project unbuildable are resolved. **The remaining P0s
(BUG-007 to BUG-009) are feature work, not defects, and were not in scope of a
verification pass** — no amount of testing can conjure an unwritten IPC layer.

## 11. Remaining issues, and the unblocking order

1. `src-tauri/tauri.conf.json`, `build.rs`, `capabilities/`, `icons/` — **DONE**
2. Implement `platform/macos.rs` or `platform/windows.rs` → unblocks every OS test
3. Implement the IPC command layer (62 commands) → unblocks every UI test
4. Implement the shell: windows, tray, hotkeys, dictation pipeline → unblocks §6–§43
5. Run `cargo test -p airprompt-core` on a machine with a linker → BUG-010
6. Structural gaps still open: `models/`, `tests/`, `installers/`, `src/hooks/`, `src/utils/` (§4 of the brief)

## 12. Security findings

Static inspection only; no runtime testing was possible.

| Check | Result |
|---|---|
| Arbitrary command execution path | **PASS (static)** — no `sh -c`, `powershell -Command`, `cmd /c` or `Invoke-Expression`; no `Command::new` receives transcribed text; the allowlist is a closed enum |
| Prompt injection reaches the action layer | **PASS (static)** — no model output feeds the action layer; the enhancement prompt cannot propose an action |
| Dangerous URI schemes | **PASS (static)** — only `http`/`https`; `file:`, `javascript:`, `data:`, `search-ms:` refused |
| Executable launch from voice | **PASS (static)** — `.exe`/`.bat`/`.ps1`/`.command`/`.app` and similar are refused by path validation |
| API keys in source or bundle | **PASS (static)** — none found; keys are designed to live in the OS credential store |
| Transcript/audio in logs | **PASS (static)** — no logging implementation exists yet, so nothing is logged |
| Command allowlist enforced at runtime | **NOT TESTED** — needs the executor, which is unwritten |
| Clipboard restoration | **NOT TESTED** — needs the platform adapter |

**No security regression was introduced by this pass**, and none of the six fixes
added a code path that executes input.

## 13. Performance results

**NOT TESTED.** No measurement was possible: the application does not run. No
latency, CPU, memory or throughput figure is reported anywhere in this document,
because reporting a number that was never measured would be fabrication.

The brief's §91 targets (hotkey < 100 ms, injection < 150 ms) remain unverified.

## 14. Screenshot index

Screenshots were captured and displayed live in the Preview panel during this
pass. They are **not persisted as files**: the capture tool returns images inline
rather than to a filesystem path, so the `screenshots/` directory the brief asks
for could not be produced by this harness.

| Shot | State | Observation |
|---|---|---|
| 1 | Voice Actions, dark | Renders correctly; honest banner and toast; no fabricated data |
| 2 | Diagnostics, dark | Latency rows show `—` rather than invented numbers |
| 3 | Speech Models, dark | Shows `0 MB` storage and "Detecting hardware…"; no invented values |
| 4 | Light theme | Light tokens applied correctly when the class is set |

**BLOCKED for the brief's required set**: onboarding, overlay states (idle /
listening / partial / processing / success), command confirmation and error state
all require backend-driven state that does not exist.

## 15. Release recommendation

# NOT READY FOR RELEASE

The application cannot be launched. It now has a valid Tauri project structure
and a verified-building frontend, but it has **no IPC layer, no platform
adapters and no application shell**, so no functional test in this brief can pass.

Definition of Done (brief §117) is not met, and none of the twelve MVP
acceptance tests (§110) can run.

**Do not ship.** The next pass must implement the platform adapter and the IPC
layer; only then does functional testing become meaningful.

Final summary (brief §107 format):

```
========================================
AIRPROMPT QA FINAL REPORT
========================================

Build:              PASS (frontend + core typecheck)
                    FAIL (Tauri binary, installer)

macOS:              BLOCKED - no Mac available on this host
Windows:            PARTIAL - UI renders disassembled; nothing native

Core Dictation:     BLOCKED - no shell; logic present, never executed
Live Transcription: BLOCKED - no shell; logic present, never executed
Text Injection:     NOT IMPLEMENTED
Local Offline Mode: BLOCKED - application cannot launch
Local AI:           BLOCKED - logic present, never executed
Cloud AI:           BLOCKED - logic present, never executed
Voice Actions:      BLOCKED - parser and allowlist exist, never executed
Security:           PARTIAL - design sound statically; no runtime test possible
Performance:        NOT TESTED - no measurement possible
Installer:          FAIL - not produced

P0 Bugs: 3 open (BUG-007 IPC 0/62, BUG-008 platform adapters, BUG-009 shell)
P1 Bugs: 2 open (BUG-010 assertions never executed, BUG-011 no installer)
P2 Bugs: 0 open (BUG-005, BUG-006 found and fixed)
P3 Bugs: 0
Fixed:   6 (BUG-001 through BUG-006)

Remaining Critical Issues:
  No IPC layer, no platform adapter, no application shell, no installer.
  The ~180 core assertions have never executed because this host cannot link
  Rust (no MSVC; MinGW is gcc 6.3.0). Semantics remain unproven.

Release Recommendation:
  NOT READY FOR RELEASE
========================================
```

---

## Adversarial follow-up

The brief's closing instruction — to run a separate adversarial pass focused on
arbitrary shell execution, prompt injection through speech, clipboard leakage,
permission-failure handling and overlay/active-app races — **should be run after
the platform adapters and IPC layer exist.** Against the current tree it would
find nothing, because the code paths under test have not been written yet. §12
records the static evidence that the *design* forecloses the first two.

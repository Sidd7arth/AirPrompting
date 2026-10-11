# Security

## The central property

**Model output can never become a shell command.**

There is no `Command::new` in this project that receives transcribed text. There
is no `sh -c`, no `powershell -Command`, no `cmd /c`, no `Invoke-Expression`.
Voice input is parsed into a value from a closed enum
(`core::storage::CommandIntent`), and each variant is executed by calling one
named platform function with a validated target. Adding a capability means
adding an enum variant and a function — not adding a string that gets executed.

This is a design property, not a filter that can be bypassed by clever phrasing.

## Threat model

| Threat | Mitigation |
|---|---|
| **A spoken phrase tries to run a shell command** | `security::looks_like_shell` rejects any phrase containing `&&`, `|`, `$(`, backticks, `;`, `>`/`<`, `rm -`, `sudo `, `Invoke-Expression`, `-EncodedCommand` and similar. The parser returns `None`, so there is no intent to execute. |
| **Prompt injection through a transcript** | Transcripts are never given to a model as instructions. The enhancement prompt is a fixed rewrite template whose output is treated as text to insert, never as a value to interpret. The model cannot propose an action. |
| **A malicious URL** | `security::validate_url` allows only `http` and `https`. Bare hosts are upgraded to `https`, never passed through with an attacker-chosen scheme. `javascript:`, `data:`, `file:`, `vbscript:`, `ms-msdt:`, `search-ms:`, `shell:`, `command:` and friends are refused. URLs containing credentials (`https://user:pass@host`) are refused. |
| **Launching an arbitrary binary** | `security::validate_path` refuses `.exe`, `.bat`, `.cmd`, `.ps1`, `.vbs`, `.js`, `.jar`, `.sh`, `.command`, `.app`, `.dll`, `.hta`, `.lnk`, `.reg`, `.msi`, `.scr`, `.com` and `.pif`. Folders and documents are allowed; executables are not. |
| **A tampered model file** | A download is verified before it can be loaded: exact published byte length, GGML magic number, and SHA-256 when the catalogue pins one (`core::models::verify_bytes`). A file that fails verification is never marked ready. |
| **API key leakage** | Keys go to the OS credential store — Keychain on macOS, Credential Manager (DPAPI-backed) on Windows — through the `keyring` crate. Keys are never written to the settings database, never logged, never sent to the frontend, and the UI only reports whether one is stored. |
| **Clipboard leakage** | Paste-based insertion saves the previous clipboard contents, sets the transcript, pastes, then restores the previous contents. The clipboard is left exactly as the user had it. |
| **Untrusted local AI server** | A local server is only ever sent the transcript text, at an endpoint the user typed. It cannot influence which actions run: the command engine does not consult any model. |
| **A stuck or hostile provider** | Every request has a timeout (`timeout_secs`, clamped to 5–120 s). A failure returns the original transcript. |
| **Secure input fields** | `PlatformIntegration::is_secure_input` is consulted before insertion. A focused password field means the text goes to the clipboard instead, with a clear message. |
| **Telemetry** | Off by default, and the app collects no transcript, audio, file content or key material under any setting. |

## The command allowlist

These are all the actions that exist. `ActionKind` in `core/security.rs` is the
single source of truth, and the risk level determines whether the user is asked
first.

| Intent | Risk | Confirmation |
|---|---|---|
| `OPEN_APP` | Low | No |
| `OPEN_URL` | Low | No |
| `OPEN_FILE` | Low | No |
| `OPEN_FOLDER` | Low | No |
| `RUN_SHORTCUT` | Medium | **Yes** |
| `COPY_TEXT` | Low | No |
| `SEARCH_WEB` | Low | No |
| `DICTATE` / `STOP_DICTATION` | Low | No |
| `CHANGE_MODEL` | Low | No |
| `OPEN_SETTINGS` | Low | No |

Nothing in the allowlist is high risk. The `RiskLevel::High` variant exists so
the policy is explicit rather than implied, and a unit test asserts that no
allowlisted intent is ever reachable at high risk.

There is deliberately **no** intent for running a script, deleting a file,
moving a file or changing a system setting. Those would require a secure
confirmation layer that does not exist yet, so they are not offered.

## Filesystem and network

* **Audio** is written to disk in exactly one situation: `whisper-cli` cannot
  read raw PCM from stdin, so a 16 kHz mono WAV is written into the app's own
  cache directory and deleted as soon as the process exits — including on
  failure. It is never added to history and never leaves that directory. See
  `docs/TROUBLESHOOTING.md`. Setting *Save recordings* keeps takes, and we
  recommend against it.
* **Signing credentials** are never in source control. See the build documents.
* **Publishing**: neither `docs/BUILD_MACOS.md` nor `docs/BUILD_WINDOWS.md`
  contains a real identity or password; they explain where a user's own
  credentials belong.

## Reporting a problem

Diagnostics contain machine facts and aggregate timings only — no transcript
content and no audio — so they are safe to attach to a report. Settings →
Diagnostics → Copy diagnostics assembles exactly that.

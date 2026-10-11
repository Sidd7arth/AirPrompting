# Installing AirPrompt

AirPrompt is a desktop app for macOS 12+ and Windows 10/11 (64-bit).
Installers are produced by the [build workflow](.github/workflows/build.yml)
and attached to each release.

> **Status:** the first installers come from CI (`build.yml`). Until a release
> is cut, you can build from source — see
> [BUILD_MACOS.md](docs/BUILD_MACOS.md) and [BUILD_WINDOWS.md](docs/BUILD_WINDOWS.md).

## macOS

1. Download `AirPrompt_0.1.0_aarch64.dmg` (Apple Silicon) or
   `…_x64.dmg` (Intel) from the release page.
2. Open the `.dmg` and drag **AirPrompt** into **Applications**.
3. First launch: right-click the app → **Open** (unsigned builds trip
   Gatekeeper once; see *Signing* below).
4. Grant permissions when prompted:
   - **Microphone** — System Settings → Privacy & Security → Microphone.
   - **Accessibility** (text insertion) — System Settings → Privacy &
     Security → Accessibility.
   - **Input Monitoring** (command mode) — same pane.
5. AirPrompt appears in the menu bar. Open its dashboard and install a speech
   model (see [MODELS.md](MODELS.md)).

### Signing

Release builds are **unsigned** until an Apple Developer ID is configured, so
macOS shows "unidentified developer". To sign and notarise your own build,
follow [BUILD_MACOS.md](docs/BUILD_MACOS.md).

## Windows

1. Download `AirPrompt_0.1.0_x64-setup.exe` (NSIS) or
   `AirPrompt_0.1.0_x64_en-US.msi` from the release page.
2. Run the installer. SmartScreen warns on unsigned binaries — choose
   **More info → Run anyway**.
3. AirPrompt starts in the system tray. Open **Settings → Permissions** and
   allow microphone access (Windows shows one prompt on first capture).
4. Install a speech model from the dashboard (see [MODELS.md](MODELS.md)).

### Signing

Release builds carry **no Authenticode signature** yet. Every binary is built
by the public CI workflow from this repository's sources.

## Local speech engine (both platforms)

AirPrompt does **not** bundle `whisper-cli`. For local dictation you need the
[whisper.cpp](https://github.com/ggml-org/whisper.cpp) `whisper-cli` binary:

- **macOS:** `brew install whisper-cpp` (or build with `cmake` per the
  whisper.cpp README), then in AirPrompt → Settings → Speech, point at the
  binary — or place it in the app's resources folder.
- **Windows:** download a whisper.cpp release zip, extract `whisper-cli.exe`,
  and point Settings → Speech at it.
- Alternatively set the environment variable `AIRPROMPT_WHISPER_BIN` to the
  binary's full path.

Without the binary, dictation reports "The speech engine is not installed"
instead of failing silently. Cloud-AI-only enhancement still works, but the
core press-to-dictate flow requires the engine.

## After install — 60-second setup

1. Menu bar / tray icon → **Open AirPrompt**.
2. **Models** tab → download a model (small `base.en` is a good start).
3. **Settings** → check the dictation shortcut (default `Option+Space` /
   `Alt+Space`).
4. Click into any text field, hold the shortcut, speak, release. The
   transcript lands in the focused app.
5. Try **command mode** (default `Option+Command+Space` / `Ctrl+Alt+Space`):
   "open safari", "open downloads folder", "run my morning shortcut".

## Uninstall

- **macOS:** quit AirPrompt from the menu bar → Quit, then move the app from
  Applications to Trash. Data lives in `~/Library/Application Support/com.airprompt.app`.
- **Windows:** Settings → Apps → AirPrompt → Uninstall. Data lives in
  `%APPDATA%\com.airprompt.app`.

Both leave nothing behind except the data folder above, which you can delete.

## Troubleshooting

See [docs/TROUBLESHOOTING.md](docs/TROUBLESHOOTING.md) for permissions that
refuse to stick, no-audio-device errors, insertion into secure fields, and
model download failures. Diagnostics → **Copy report** captures everything
support needs.

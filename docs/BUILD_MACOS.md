# Building for macOS

## Requirements

| Tool | Version | Install |
|---|---|---|
| macOS | 11+ (13+ recommended) | — |
| Xcode Command Line Tools | latest | `xcode-select --install` |
| Node.js | 20+ | `brew install node` or nodejs.org |
| Rust | 1.77+ | `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh` |
| Tauri CLI | v2 | installed as a dev dependency, no global install needed |

Apple Silicon and Intel are both supported. On Apple Silicon the build is
`aarch64-apple-darwin`; there is no universal-binary step yet (see *Universal
binary* below).

## Development

Run from the repository root:

```bash
npm install
npm run tauri dev
```

The first run compiles the Rust workspace, which takes a few minutes. Vite serves
the frontend on port 1420 and Tauri opens the window; frontend changes hot-reload
and Rust changes restart the app.

Run the core tests before and after changing anything in `src-tauri/core`:

```bash
cd src-tauri
cargo test -p airprompt-core
cargo check --workspace
```

## Production build

```bash
npm run tauri build
```

Outputs:

```
src-tauri/target/release/bundle/macos/AirPrompt.app
src-tauri/target/release/bundle/dmg/AirPrompt_<version>_<arch>.dmg
```

To select the architecture explicitly:

```bash
npm run tauri build -- --target aarch64-apple-darwin
npm run tauri build -- --target x86_64-apple-darwin
```

### Universal binary

Tauri can produce a universal `.app` with both slices:

```bash
rustup target add x86_64-apple-darwin aarch64-apple-darwin
npm run tauri build -- --target universal-apple-darwin
```

This doubles build time and bundle size and requires the whisper.cpp binary to
be universal too (`lipo -create` the two per-arch binaries). Until that is set
up, ship per-architecture DMGs instead.

## The whisper.cpp binary

Not bundled. Build it and place it in the app resources:

```bash
git clone https://github.com/ggml-org/whisper.cpp
cd whisper.cpp
cmake -B build -DCMAKE_BUILD_TYPE=Release -DWHISPER_METAL=ON
cmake --build build --config Release -j
```

`-DWHISPER_METAL=ON` enables GPU acceleration on Apple Silicon and roughly halves
decode time on the larger models. Copy `build/bin/whisper-cli` into the app's
resources directory (see `src-tauri/tauri.conf.json` → `bundle.resources`), or
set `AIRPROMPT_WHISPER_BIN` to its path during development.

Speech models are **not** bundled — they download on demand into
`~/Library/Application Support/com.airprompt.app/models/`.

## Permissions and entitlements

Three macOS permissions are involved. Two need usage descriptions in
`Info.plist`; without them the app is killed the moment it asks, with no dialog.

| Key | Purpose |
|---|---|
| `NSMicrophoneUsageDescription` | Why AirPrompt needs the microphone |
| `NSSpeechRecognitionUsageDescription` | Only if you enable the Apple Speech engine |

Accessibility and Input Monitoring have no usage-description keys — the user
grants them in System Settings. AirPrompt explains what each is for on screen
before sending the user there, and works without Accessibility (falling back to
the clipboard) rather than refusing to run.

Entitlements for a signed build should include:

```
com.apple.security.device.audio-input    = true
com.apple.security.automation.apple-events = true   (only if you drive other apps via Apple Events)
```

Do **not** enable the App Sandbox for a Developer ID build unless you also move
the whisper.cpp binary and the models directory inside the sandbox container —
the Accessibility API is not available to sandboxed apps, which would remove
automatic text insertion.

## Signing and notarisation

Prepare these yourself; **never commit them**. Keep the identity out of the
repository and out of `tauri.conf.json`.

| Credential | Where it belongs |
|---|---|
| Developer ID Application identity | macOS Keychain, or `APPLE_SIGNING_IDENTITY` in your shell |
| Apple ID + app-specific password | `APPLE_ID`, `APPLE_PASSWORD` environment variables |
| Team ID | `APPLE_TEAM_ID` |
| Notarisation key (App Store Connect API) | `APPLE_API_KEY`, `APPLE_API_ISSUER`, `APPLE_API_KEY_PATH` |

Tauri reads these from the environment at build time:

```bash
export APPLE_SIGNING_IDENTITY="Developer ID Application: Your Name (TEAMID)"
export APPLE_ID="you@example.com"
export APPLE_PASSWORD="app-specific-password"
export APPLE_TEAM_ID="TEAMID"
npm run tauri build
```

Verify the result:

```bash
codesign --verify --deep --strict --verbose=2 "src-tauri/target/release/bundle/macos/AirPrompt.app"
spctl --assess --type execute --verbose=4 "src-tauri/target/release/bundle/macos/AirPrompt.app"
xcrun stapler validate "src-tauri/target/release/bundle/dmg/AirPrompt_0.1.0_aarch64.dmg"
```

An unsigned build is fine for personal use; Gatekeeper will require a
right-click → Open the first time, and the Accessibility permission will reset
on every rebuild because the code signature changes.

## Distribution

1. Build with signing and notarisation enabled.
2. Staple the notarisation ticket to both the `.app` and the `.dmg`.
3. Distribute the DMG.
4. For automatic updates, host the update manifest and configure the updater
   endpoint in `tauri.conf.json`; the signing public key lives there too and is
   safe to commit — the private key is not.

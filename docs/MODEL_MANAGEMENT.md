# Model management

AirPrompt ships **no** speech model. Recognition weights are large files with their
own licences, so you choose one and the app downloads it. Nothing is downloaded
until you press Download.

## The catalogue

| Model | Size | Languages | Speed | Accuracy | Memory |
|---|---|---|---|---|---|
| Tiny (`whisper-tiny-en`) | 78 MB | English | ★★★★★ | ★★☆☆☆ | ~390 MB |
| **Base** (`whisper-base-en`) | 148 MB | English | ★★★★☆ | ★★★☆☆ | ~440 MB |
| Small (`whisper-small-en`) | 488 MB | English | ★★★☆☆ | ★★★★☆ | ~1.0 GB |
| Medium (`whisper-medium`) | 1.53 GB | 99 languages | ★☆☆☆☆ | ★★★★★ | ~2.6 GB |

Base is the default recommendation. The English-only models are both faster and
more accurate than the multilingual models of the same size; Medium is worth it
only if you need another language or the extra accuracy, and it needs a capable
machine.

Files come from `huggingface.co/ggerganov/whisper.cpp`. The catalogue is static
data in `core/src/models.rs` — the app never enumerates a remote directory.

## Automatic recommendation

During onboarding AirPrompt reads your CPU count, total memory and free disk space
and recommends accordingly:

| Memory | Recommendation |
|---|---|
| under 6 GB | Tiny |
| 6–12 GB | Base |
| 12 GB and above | Small |

It never silently selects Medium, even on a very large machine — Medium is
offered as the "highest accuracy" option but the default stays at Small, because
a 2.6 GB resident model changes how the whole machine feels.

## The lifecycle

**Download.** Streaming HTTPS GET with progress reported to the UI roughly every
256 KB. The partial file is written as `ggml-….bin.part`; the final name is only
used once verification passes.

**Pause.** Sets a flag the download loop checks between chunks. The partial file
stays on disk.

**Resume.** Re-requests with an HTTP `Range` header from the current offset. If
the server ignores the range and returns 200 rather than 206, the download
restarts cleanly rather than corrupting the file.

**Verify.** Three independent checks, because they catch different failures:

1. **Exact byte length** — catches truncation.
2. **GGML magic number** — catches a saved HTML error page or a proxy
   interstitial.
3. **SHA-256** — only when the catalogue entry pins a digest.

Verification runs automatically after every download and can be re-run from the
Models screen. A model that fails is never marked ready and never loaded.

> Some catalogue entries do not yet have a pinned SHA-256, so those rely on the
> length and magic checks. Add the upstream digest to `CATALOG` in
> `core/src/models.rs` to enable full digest verification for a model.

**Delete.** Removes the model and any partial file. A model currently selected as
active cannot be deleted until you select another.

## Where files live

| Platform | Path |
|---|---|
| macOS | `~/Library/Application Support/com.airprompt.app/models/` |
| Windows | `%APPDATA%\com.airprompt.app\models\` |

They are deliberately outside the application bundle: updating or reinstalling
AirPrompt does **not** re-download speech models. "Storage used" on the Models
screen reports the total.

## The recogniser binary

whisper.cpp's command-line tool is also an external asset. AirPrompt looks for it in
this order:

1. `AIRPROMPT_WHISPER_BIN` — an explicit path, useful in development.
2. The app's bundled resources directory.
3. `whisper-cli`, `whisper-cli.exe`, `main` or `main.exe` on `PATH`.

Build it from [ggml-org/whisper.cpp](https://github.com/ggml-org/whisper.cpp):

```bash
git clone https://github.com/ggml-org/whisper.cpp
cd whisper.cpp
cmake -B build -DCMAKE_BUILD_TYPE=Release
cmake --build build --config Release -j
# macOS also:  -DWHISPER_METAL=ON
```

The result is `build/bin/whisper-cli` (older versions: `build/bin/main`). Copy it
into the app's resources, or set `AIRPROMPT_WHISPER_BIN` to its full path.

If the binary is missing, Settings → Speech reports it as unavailable with the
remedy, rather than failing at the moment you press the shortcut.

## Why a subprocess rather than linked-in inference

`whisper-rs` would link whisper.cpp into the binary: no temporary WAV, one fewer
moving part, marginally faster startup. It also requires a working C++ toolchain
and CMake on **every** machine that builds the app, and it makes the app's binary
platform-locked at build time.

The subprocess keeps builds reproducible with a plain Rust toolchain and Xcode
Command Line Tools or VS Build Tools. The cost is the temporary WAV described in
`TROUBLESHOOTING.md`.

To switch: implement `SpeechEngine` (in `core/src/transcription.rs`) with a
`whisper-rs` backend and select it instead of `WhisperCliEngine`. Nothing else in
the app needs to change, because the trait is the only thing the pipeline knows
about.

## Other engines

| Engine | Platform | Status |
|---|---|---|
| whisper.cpp | macOS, Windows | Implemented and default |
| Parakeet v3 | macOS, Apple Silicon | Planned — CoreML bundle from FluidAudio (~460 MB, downloaded on first use) |
| Apple Speech | macOS | Planned — uses the system recogniser; requires `NSSpeechRecognitionUsageDescription` |

Both macOS-only engines are declared in the settings model so the abstraction is
exercised, but only whisper.cpp has an implementation today. The app reports the
others as unavailable rather than offering a control that does nothing.

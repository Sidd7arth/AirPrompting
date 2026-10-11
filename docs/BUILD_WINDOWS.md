# Building for Windows

## Requirements

| Tool | Version | Notes |
|---|---|---|
| Windows | 10 (1809+) or 11 | Windows 11 is the primary target |
| Node.js | 20+ | 64-bit |
| Rust | 1.77+ | **`x86_64-pc-windows-msvc`** toolchain |
| Visual Studio Build Tools | 2019 or 2022 | Must include **Desktop development with C++** |
| WebView2 Runtime | — | Present on Windows 11; the installer adds it on 10 |
| Tauri CLI | v2 | A dev dependency; no global install needed |

### Install the prerequisites

```powershell
winget install OpenJS.NodeJS.LTS
winget install Rustlang.Rustup
winget install Microsoft.VisualStudio.2022.BuildTools
```

Then add the C++ workload — `VsDevCmd` will not exist without it:

```powershell
winget install Microsoft.VisualStudio.2022.BuildTools --override "--quiet --wait --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"
```

Confirm the default Rust toolchain is the MSVC one:

```powershell
rustup default stable-x86_64-pc-windows-msvc
rustup target list --installed
```

> **Use MSVC, not GNU.** The GNU toolchain needs a MinGW-w64 gcc 13 or newer;
> older MinGW (for example the 6.3.0 build bundled with some IDEs) fails at the
> link step with `cannot find -lbcrypt` and `cannot find -ldbghelp`. MSVC is the
> supported path.

## Development

Open a **Developer Command Prompt** (or run `vcvars64.bat` first so `cl.exe` is
on `PATH`):

Run from the repository root:

```powershell
npm install
npm run tauri dev
```

Run the core tests before and after changing anything in `src-tauri/core`:

```powershell
cd src-tauri
cargo test -p airprompt-core
cargo check --workspace
```

## Production build

```powershell
npm run tauri build
```

Outputs:

```
src-tauri\target\release\bundle\msi\AirPrompt_<version>_x64_en-US.msi
src-tauri\target\release\bundle\nsis\AirPrompt_<version>_x64-setup.exe
```

Tauri produces both an MSI (WiX) and an NSIS installer by default. Either
supports install, uninstall and upgrade; see *Installer behaviour* below.

### ARM64

Windows on ARM is designed for but not yet built. Add the target and build per
architecture:

```powershell
rustup target add aarch64-pc-windows-msvc
npm run tauri build -- --target aarch64-pc-windows-msvc
```

The whisper.cpp binary must be rebuilt for ARM64 as well, and the USB/audio
device stack behaves slightly differently — test before shipping it.

## The whisper.cpp binary

Not bundled. Build it with MSVC and place it in the app resources:

```bat
git clone https://github.com/ggml-org/whisper.cpp
cd whisper.cpp
cmake -B build -DCMAKE_BUILD_TYPE=Release -DWHISPER_CUDA=OFF
cmake --build build --config Release -j
```

For NVIDIA acceleration use `-DWHISPER_CUDA=ON` (requires the CUDA toolkit), or
`-DWHISPER_VULKAN=ON` for a vendor-neutral GPU path.

The binary lands in `build\bin\Release\whisper-cli.exe`. Copy it into the app's
resources (see `src-tauri\tauri.conf.json` → `bundle.resources`), or set
`AIRPROMPT_WHISPER_BIN` to its full path during development.

Speech models are **not** bundled — they download on demand into
`%APPDATA%\com.airprompt.app\models\`.

## Permissions

Windows has no Accessibility grant to request. Two things matter:

* **Microphone.** Settings → Privacy & security → Microphone must allow desktop
  apps. A packaged MSI installs to `Program Files`; the prompt appears on first
  dictation.
* **UI Automation** needs no permission — it is available to any desktop process.
  AirPrompt treats it as an optimisation and always keeps the clipboard path as a
  fallback, because accessibility information is incomplete in many apps.

## Launch at login

Implemented through `tauri-plugin-autostart`, which writes a per-user registry
entry under `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`. No
administrator rights are required, and turning the setting off removes it.

## Installer behaviour

Both installer formats are generated:

| Action | MSI (WiX) | NSIS |
|---|---|---|
| Install | Yes | Yes |
| Uninstall | Add/Remove Programs | Uninstaller in the install directory |
| Upgrade in place | Yes | Yes |
| Per-user install | Configure via WiX | Default |
| Launch at startup | App setting, not installer | App setting, not installer |

Nothing requires the command line — a normal user double-clicks the installer.

## Signing

Prepare these yourself; **never commit them**.

| Credential | Where it belongs |
|---|---|
| Code signing certificate (`.pfx`) | Outside the repository; reference by path or install into the certificate store |
| Certificate password | `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` or your own CI secret |
| Timestamp server | Configured in `tauri.conf.json`, not a secret |
| Updater signing key | `TAURI_SIGNING_PRIVATE_KEY` — the **private** key stays out of source control |

```powershell
$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = "your-password"
npm run tauri build
```

Verify:

```powershell
Get-AuthenticodeSignature "src-tauri\target\release\bundle\msi\AirPrompt_0.1.0_x64_en-US.msi"
```

An unsigned build works for personal use, but SmartScreen will warn on first run
until the binary has reputation.

## Distribution

1. Build with signing enabled.
2. Attach the `.msi` and `-setup.exe` to a release.
3. For automatic updates, host the update manifest and set the updater endpoint
   and public key in `tauri.conf.json`. The public key is safe to commit; the
   private key is not.
4. Keep model files out of the installer — an app update must never force a
   speech model re-download, which is why models live in `%APPDATA%`.

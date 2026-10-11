# Speech models

AirPrompt ships no model. You download one inside the app; it stays on your
disk and never leaves it.

## What the model does

The model is the acoustic+language model that whisper.cpp runs against your
microphone audio. Bigger = more accurate, slower, more RAM.

## Which one to pick

The app recommends one based on your hardware (Models tab → **Recommend for
this machine**), using CPU cores, RAM and free disk:

| Model | Size | RAM | Best for |
|---|---|---|---|
| `tiny.en` | ~75 MB | ~390 MB | newest machines, quick notes |
| `base.en` | ~142 MB | ~500 MB | **default recommendation** |
| `small.en` | ~466 MB | ~1.0 GB | accuracy on modest hardware |
| `medium.en` | ~1.5 GB | ~2.6 GB | demanding accuracy needs |
| `large-v3` (multilingual) | ~2.9 GB | ~4.7 GB | non-English, maximum accuracy |

Multilingual variants (without `.en`) trade some English accuracy for other
languages. Choose one if you dictate outside English.

## Downloading

1. Open the dashboard → **Models**.
2. Pick a model → **Download**. Progress streams live; you can **Pause** and
   **Resume** (partial files are kept and continued, not restarted).
3. After download the app **verifies integrity** against the published
   checksum before marking it ready.
4. **Set active** switches the engine; the next take uses it immediately.

## Managing storage

**Models → Disk usage** shows the total footprint. **Delete** removes a model
file; the active model cannot be deleted while in use. Models live in:

- macOS: `~/Library/Caches/com.airprompt.app/models`
- Windows: `%LOCALAPPDATA%\com.airprompt.app\models`

## Offline verification

After any download you can run **Verify** again at any time — it re-checks the
file against the checksum. AirPrompt never phones home; the only network use
of the Models tab is the download itself from Hugging Face
(`ggerganov/whisper.cpp`).

## Local AI enhancement (optional, separate from speech)

Speech models and AI enhancement are independent:

- **Speech model** (above) turns audio into text.
- **AI enhancement** cleans the text (punctuation, fillers, formatting). It
  can run locally through Ollama / LM Studio / llama.cpp server, or via an
  OpenAI-compatible cloud API.

Configure enhancement in **Settings → AI**. A local endpoint means the entire
dictation pipeline — audio, transcription, cleanup — never touches the
network. See [docs/PRIVACY.md](docs/PRIVACY.md) for the exact traffic table.

If no engine or model is present, dictation says so clearly; it never fakes a
transcript.

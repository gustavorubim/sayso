# Shortcut to Text

Hold a keyboard shortcut, speak, let go — your words are typed wherever the cursor is. Email, chat, documents, code
editors: any app.

Speech recognition runs entirely on your computer with open models you choose and download from
[Hugging Face](https://huggingface.co). No account, no subscription, and no audio ever leaves the machine.

Runs on Windows, macOS and Linux.

## Features

- **One global shortcut.** Hold to talk, or press once to start and again to stop. You pick the keys.
- **Text lands at the cursor.** Pasted in one go (your clipboard is put back afterwards), typed out key by key for
  apps that block pasting, or just copied.
- **Choose your model.** Seven speech models in a dropdown; picking one downloads it from Hugging Face with progress,
  cancel and resume. Switch whenever you like.
- **Dictionary.** Fix names and jargon the model gets wrong, or define spoken shortcuts ("my address") that expand
  into longer text.
- **Spoken line breaks.** Say "new line" or "new paragraph".
- **Filler removal.** Optionally drop "um" and "uh".
- **History.** The last 300 dictations, searchable, stored locally. Can be turned off.
- **Listening indicator.** A small pill at the bottom of the screen shows your voice level and when text is being
  written. It never takes keyboard focus.
- **Long recordings.** Anything over 25 seconds is split at pauses with a voice-activity model, so there is no length
  limit in practice.
- **Lives in the tray**, optionally starting at login.

## Speech models

| Model | Languages | Download | Good for |
| --- | --- | --- | --- |
| [Parakeet TDT 0.6B v3](https://huggingface.co/nvidia/parakeet-tdt-0.6b-v3) | 25 European languages | 671 MB | Best all-round choice on a CPU |
| [Whisper Large v3 Turbo](https://huggingface.co/openai/whisper-large-v3-turbo) | About 100 | 1.04 GB | Widest language coverage |
| [Distil-Whisper Large v3.5](https://huggingface.co/distil-whisper/distil-large-v3.5) | English | 984 MB | Whisper-level English, faster |
| [Whisper Small](https://huggingface.co/openai/whisper-small) | About 100 | 375 MB | Older or low-memory machines |
| [Canary 180M Flash](https://huggingface.co/nvidia/canary-180m-flash) | English, Spanish, German, French | 207 MB | Small download, punctuated output |
| [SenseVoice Small](https://huggingface.co/FunAudioLLM/SenseVoiceSmall) | Chinese, Cantonese, English, Japanese, Korean | 240 MB | East Asian languages, very fast |
| [Moonshine Base](https://huggingface.co/UsefulSensors/moonshine-base) | English | 287 MB | Near-instant short phrases |

Models run through [sherpa-onnx](https://github.com/k2-fsa/sherpa-onnx) using 8-bit quantized ONNX exports. Each
model has its own license; check its model card before commercial use.

## Install

Download the installer for your system from the
[Releases page](https://github.com/gustavorubim/shortcut-to-text/releases).

| System | File |
| --- | --- |
| Windows | `…-windows-x64-setup.exe` |
| macOS, Apple silicon | `…-mac-arm64.dmg` |
| macOS, Intel | `…-mac-x64.dmg` |
| Linux | `…-linux-x86_64.AppImage` or `…-linux-amd64.deb` |

Then open the app, pick a model under **Speech models**, and hold the shortcut (`Ctrl+Shift+Space` on Windows and
Linux, `⌥ Space` on macOS).

### Platform notes

- **Installers are not code-signed yet.** Windows SmartScreen will ask you to confirm ("More info" → "Run anyway").
  On macOS, right-click the app and choose Open the first time.
- **macOS** asks for two permissions: Microphone (to hear you) and Accessibility (to paste into other apps).
- **Linux** needs an X11 session. Global shortcuts and simulated paste are not available on Wayland yet.
- **Updates.** Windows and Linux AppImage builds update themselves from GitHub Releases. On macOS, download the new
  version by hand until the builds are signed.

## Build from source

You need [Node.js](https://nodejs.org) 22 or newer and [Rust](https://rustup.rs) (stable).

```bash
git clone https://github.com/gustavorubim/shortcut-to-text.git
cd shortcut-to-text
npm install
npm run dev
```

`npm run dev` compiles the Rust core, then starts the app with hot reload for the interface.

Linux also needs: `sudo apt install pkg-config libasound2-dev libx11-dev libxi-dev libxtst-dev libxdo-dev`.

To build an installer for the system you are on: `npm run dist` (output in `dist/`).

More in [CONTRIBUTING.md](CONTRIBUTING.md) and [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

## How it is built

- **`core/`** — a Rust program that owns everything time-critical: the global shortcut, microphone capture, speech
  recognition, model downloads, and pasting.
- **`src/`** — an Electron app (TypeScript and React) for the settings window, tray icon, and listening indicator.

The two talk over a small JSON protocol, so the core can be reused by other front ends later.

## Roadmap

- GPU acceleration (CUDA, DirectML, CoreML)
- Text appearing while you speak (streaming models)
- Optional clean-up with a local language model: punctuation, tone, formatting
- Per-app settings, such as a different model or style in your code editor
- Wayland support
- Signed installers

## License

[MIT](LICENSE). Speech models are licensed separately by their authors.

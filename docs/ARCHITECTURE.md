# Architecture

```
┌──────────────────────── Electron ────────────────────────┐
│  main process (src/main)                                  │
│    windows · tray · login item · auto-update              │
│        │ IPC (contextBridge, src/preload)                 │
│  renderer (src/renderer)                                  │
│    settings window · listening indicator                  │
└────────────┬──────────────────────────────────────────────┘
             │ JSON lines over stdin / stdout
┌────────────┴──────────── Rust core (core/) ───────────────┐
│  main thread:  global hotkey + OS event pump (hotkey.rs)  │
│  rpc thread:   requests from Electron (rpc.rs)            │
│  workers:      recording (audio.rs) · transcription       │
│                (engine.rs) · downloads (download.rs)      │
│  state:        app.rs — idle → recording → transcribing   │
└───────────────────────────────────────────────────────────┘
```

## Why two processes

Electron's `globalShortcut` reports key presses but not releases, so hold-to-talk needs native code. Putting the
hotkey, audio and inference in one Rust process keeps the hot path — key down to text pasted — free of the JavaScript
event loop, and a crash in native code cannot take the interface down with it. Electron restarts the core if it exits.

## The dictation path

1. `hotkey.rs` receives press/release on the main thread and calls `App::on_hotkey`.
2. `audio.rs` opens the microphone on its own thread and emits a `level` event every 50 ms.
3. On release, the recording is mixed to mono, resampled to 16 kHz, and handed to a worker thread.
4. `engine.rs` runs the model. Audio longer than 25 s is first split at pauses with Silero VAD.
5. `textproc.rs` applies filler removal, spoken line breaks, and dictionary replacements.
6. `paste.rs` puts the text on the clipboard, sends the paste keystroke, and restores the clipboard.
Two variations on this path:

- **Writing as you speak.** The recorder also streams 16 kHz audio to a worker (`App::live_worker`) that runs the
  voice-activity model on it and performs steps 4–6 for each utterance as soon as the speaker pauses.
- **Computer audio.** The third shortcut records the default output device (a WASAPI loopback on Windows)
  through the same live path. Audio is streamed to the worker and dropped, so memory stays flat on long calls.
- **Rewrite shortcut.** Between steps 5 and 6, `rewrite.rs` sends the text to OpenRouter and the reply is delivered
  instead. If the request fails, the plain transcript is delivered and the error is shown.

7. `status` events drive the indicator window, which Electron shows without activating so focus never moves.

## Protocol

One JSON object per line. The full list of methods and events, with types, is in
[`src/shared/types.ts`](../src/shared/types.ts); the Rust side is [`core/src/rpc.rs`](../core/src/rpc.rs).

```
→ {"id": 7, "method": "download_model", "params": {"id": "whisper-small"}}
← {"id": 7, "result": null}
← {"event": "download-progress", "payload": {"id": "whisper-small", "downloaded": 1048576, "total": 377293849, "done": false, "error": null}}
```

You can drive the core by hand, which is useful when debugging:

```bash
echo '{"id":1,"method":"list_models"}' | core/target/debug/sayso-core --data-dir /tmp/stt
```

`transcribe_file` (params: `{"path": "speech.wav"}`) runs the selected model on a WAV file without a microphone.
Add `"live": true` to push the file through the write-as-you-speak path and get the pieces back in order.

## Data on disk

Everything lives in Electron's per-user data folder (`%APPDATA%/sayso`,
`~/Library/Application Support/sayso`, `~/.config/sayso`):

| File | Contents |
| --- | --- |
| `settings.json` | All settings, including dictionary replacements |
| `history.json` | The last 300 dictations |
| `models/<id>/` | Downloaded model files |
| `models/_vad/` | The shared voice-activity model |
| `openrouter.key` | The OpenRouter API key, encrypted by the operating system's credential store |

## Security model

- The renderer is sandboxed with context isolation. It can call only the core methods listed in
  `RENDERER_METHODS` in `src/main/index.ts`, and only from the app's own pages. Starting a recording, reading a
  file, and handing over the API key are reachable from the main process alone.
- Windows cannot navigate away from the app's pages or open new windows, and all browser permission requests are
  denied.
- Model files are pinned to a repository revision and verified against SHA-256 checksums in `core/src/models.rs`.
- A rewrite reply is treated as untrusted: see `sanitize` in `core/src/rewrite.rs`.
- External links open in the system browser and only for `huggingface.co`, `github.com` and `openrouter.ai`.
- Network access is limited to model downloads from Hugging Face, update checks against GitHub Releases, and —
  only when the rewrite shortcut is used — a request to OpenRouter carrying the transcribed text.
- The OpenRouter key is encrypted with Electron's `safeStorage`. The interface can set or remove it but cannot
  read it back; the core receives it at start-up and keeps it in memory only.
- Audio is held in memory only for the length of a dictation and is never written to disk.

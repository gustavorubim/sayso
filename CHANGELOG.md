# Changelog

Notable changes to Sayso. The format follows [Keep a Changelog](https://keepachangelog.com), and versions
follow [Semantic Versioning](https://semver.org).

## 0.4.4

### Fixed

- Settings and history are now saved in one step. If the app was killed or the computer lost power while saving,
  the file could be left half-written and the app would start again with default settings
- A very short tap on the shortcut could crash the speech engine with some models (Whisper among them)
- The app now keeps restarting the speech engine if it stops. It used to give up after the third time in a
  session, however long the session had lasted
- Asking the engine to transcribe something that is not a file (a device name such as `CON` on Windows) made it
  stop answering; it is now refused
- Audio containing values a sound card should never produce (not-a-number, infinities, far out of range) is
  cleaned before it reaches the speech model

### Changed

- Releases are tested harder. New stress tests cover hundreds of dictations in a row, model switching,
  ten-minute recordings, hostile audio, a flood of requests, the engine being killed mid-save, and interrupted
  or corrupted downloads
- The installers that are published are now the very ones that were tested, and a release can be rehearsed
  without publishing anything
- The build pipeline no longer uses the deprecated Node 20 runtime, and duplicated work was removed from it

## 0.4.3

### Added

- A "Check for updates" button in the sidebar, under the version number

### Fixed

- Ending a dictation with "new line" or "new paragraph" now leaves the line break; it used to be dropped and
  replaced by a space

### Changed

- A much larger test suite guards every release: audio regression tests that run real recordings through real
  speech models, tests for the engine's protocol and state, tests for every screen of the interface, and a
  packaged-app check that transcribes a recording
- `sayso-core fetch` downloads models from the command line, with the same checksum verification as the app

### Removed

- The Intel Mac build. macOS releases are for Apple silicon only from now on; 0.4.2 is the last version with
  an Intel installer

## 0.4.2

### Fixed

- Whisper models are accurate again. Version 0.4.0 shortened the silence Whisper appends to audio; measured
  on a real test set, that raised Whisper Large v3 Turbo's word error rate from 4.9% to 8.6%. The engine's own
  setting is restored. Whisper is somewhat slower than in 0.4.0 as a result

### Changed

- The README explains how to open Sayso on a Mac that reports it as damaged; the real fix, notarization by
  Apple, needs a Developer ID certificate

### Added

- Three speech models: **Parakeet Ultra** (now the recommended model), **Cohere Transcribe** (the most accurate
  in our tests) and **Qwen3-ASR 0.6B** (30 languages and 22 Chinese dialects)
- `sayso-core bench --eval <folder>` scores every installed model for word error rate and speed on a folder of
  recordings with known transcripts

### Changed

- The benchmark mode now runs the way the app does, with the system asked not to throttle it; pass
  `--background` to measure an ordinary background process

## 0.4.1

### Fixed

- macOS no longer reports the downloaded app as "damaged": builds made without a Developer ID certificate are
  now signed ad hoc, and CI checks the signature of the packaged app

### Changed

- Releases are built only after the full check suite passes on the tagged commit
- CI now builds the real app on each system, starts it, and has it check itself
- Tags with a suffix (`v0.5.0-beta.1`) publish as pre-releases that are not offered to stable users
- The interface is linted with ESLint, formatted with Prettier, and has unit tests
- A weekly dependency audit opens an issue when a shipped dependency has a known vulnerability
- The smoke test opens every screen, and a failure on one screen no longer blanks the whole window

## 0.4.0

### Changed

- Faster recognition, most of all when the computer is busy: the speech engine asks Windows not to throttle it
  and runs at slightly raised priority. With a game running, a 7-second clip went from about 1.15 s to 0.45 s
- Uses up to six threads (never more than the number of physical cores) instead of four
- Whisper models append 3 s of silence instead of the engine's longer default, roughly halving their time on
  short dictations

### Added

- Computer audio: an optional third shortcut that writes down what the computer is playing (a call, a video),
  at the cursor, each time the speaker pauses. Windows; Linux where a "Monitor" input exists
- `sayso-core bench`, a benchmark mode for measuring recognition speed and tuning options

### Security

- Model downloads are pinned to an exact revision and every file is checked against a SHA-256 checksum
- The interface can only call an explicit list of core methods, and only from the app's own pages
- Windows can no longer be navigated away from the app or open new windows; browser permissions are denied
- Rewrite replies are stripped of control characters, cannot add line breaks that were not dictated, and are
  rejected if far longer than what was said
- Dictations are kept out of clipboard history and cloud clipboard sync, and are cleared from the clipboard when
  the previous contents cannot be restored
- A crash inside a request no longer leaves the speech engine running but unresponsive
- Downloads and rewrite requests refuse non-HTTPS redirects
- Packaged apps can no longer be run as a plain Node.js runtime or load code from outside the app archive
- macOS entitlements reduced to the two the app needs
- Release builds verify the speech engine's checksum, compile before any signing secret is available, pin
  third-party actions to commit hashes, and no longer use a build cache

## 0.3.0

### Changed

- **The app is now called Sayso** (it was "Shortcut to Text"), with a new icon and colours. The repository moved to
  `gustavorubim/sayso`. Models, settings and history are carried over on first start. The old app is not removed
  automatically; uninstall it yourself.

### Added

- Write as you speak: an option in Settings to type each sentence at every pause while you keep talking
- Rewrite shortcut: a second, optional shortcut that dictates and then cleans up the text with a language model
  through OpenRouter, using your own API key, with a choice of model and editable instructions
- A "Try it" box on the Rewrite screen to test the key, model and instructions on typed text
- History marks dictations that were rewritten

## 0.2.1

### Fixed

- The tray icon and window icon were missing in the installed app

## 0.2.0

### Added

- Update notice in the sidebar: "Restart to update" once a new version has downloaded (Windows, Linux AppImage),
  or a download link where the app cannot update itself (macOS, Linux .deb)
- "Check for updates" in Settings
- The app checks for updates every six hours while it runs

### Changed

- Development runs use their own data folder, so `npm run dev` works while the installed app is open

## 0.1.1

### Fixed

- macOS installers now build when no code-signing secrets are configured

### Added

- Screenshots in the README

## 0.1.0

First version, released as "Shortcut to Text".

### Added

- Global dictation shortcut with hold-to-talk and press-to-toggle modes
- Text delivery by paste (with clipboard restore), simulated typing, or clipboard only
- Seven speech models downloadable from Hugging Face: Parakeet TDT 0.6B v3, Whisper Large v3 Turbo,
  Distil-Whisper Large v3.5, Whisper Small, Canary 180M Flash, SenseVoice Small, Moonshine Base
- Model downloads with progress, cancel and resume
- Dictionary replacements, spoken line breaks, and optional filler removal
- Local, searchable dictation history
- Listening indicator, tray icon, and open-at-login
- Installers for Windows, macOS and Linux built by GitHub Actions
